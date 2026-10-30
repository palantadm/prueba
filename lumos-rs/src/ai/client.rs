//! HTTP plumbing shared by the provider adapters: client, SSE reading, error mapping, retries.

use super::sse::{SseEvent, SseParser};
use super::types::*;
use crate::model::{AiModelInfo, AiProviderKind};
use futures_util::StreamExt;
use std::sync::OnceLock;
use std::time::Duration;

pub const IDLE: Duration = Duration::from_secs(120);

pub struct ChatReq<'a> {
    pub system: &'a str,
    pub tools: &'a [ToolDef],
    pub msgs: &'a [Msg],
    pub effort: &'a str,
    pub max_tokens: u32,
}

pub fn http() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .user_agent(concat!("lumos/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("http client")
    })
}

/// Stream one assistant turn with retries on transient failures.
pub async fn chat(t: &Target, req: &ChatReq<'_>, on: &mut (dyn FnMut(Ev) + Send), cancel: &mut Cancel) -> Result<Turn, ApiError> {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let r = match t.kind {
            AiProviderKind::Anthropic => super::anthropic::stream(t, req, on, cancel).await,
            AiProviderKind::OpenaiCompat => super::openai::stream(t, req, on, cancel).await,
        };
        match r {
            Ok(turn) => return Ok(turn),
            Err(e) if e.retryable() && attempt < 4 => {
                let wait = e.retry_after.unwrap_or_else(|| Duration::from_millis(1000 * (1 << (attempt - 1)))).min(Duration::from_secs(30));
                on(Ev::Status(format!("El proveedor no responde bien ({}). Reintentando en {} s…", short(&e.msg), wait.as_secs().max(1))));
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = wait_cancel(cancel) => return Err(ApiError::new(ErrKind::Cancelled, "cancelado")),
                }
            }
            Err(e) => return Err(e),
        }
    }
}

fn short(s: &str) -> String {
    s.chars().take(80).collect()
}

/// Read an SSE response body, calling `f` for every event.
pub async fn read_sse(resp: reqwest::Response, cancel: &mut Cancel, mut f: impl FnMut(SseEvent) -> Result<(), ApiError>) -> Result<(), ApiError> {
    let mut parser = SseParser::default();
    let mut stream = resp.bytes_stream();
    loop {
        tokio::select! {
            _ = wait_cancel(cancel) => return Err(ApiError::new(ErrKind::Cancelled, "cancelado")),
            r = tokio::time::timeout(IDLE, stream.next()) => match r {
                Err(_) => return Err(ApiError::new(ErrKind::Timeout, "sin datos del proveedor durante 120 s")),
                Ok(None) => {
                    for ev in parser.finish() {
                        f(ev)?;
                    }
                    return Ok(());
                }
                Ok(Some(Err(e))) => return Err(ApiError::new(ErrKind::Network, e.to_string())),
                Ok(Some(Ok(b))) => {
                    for ev in parser.feed(&b) {
                        f(ev)?;
                    }
                }
            }
        }
    }
}

/// Send a request, racing it against cancellation and mapping failures.
pub async fn send(rb: reqwest::RequestBuilder, cancel: &mut Cancel) -> Result<reqwest::Response, ApiError> {
    let resp = tokio::select! {
        _ = wait_cancel(cancel) => return Err(ApiError::new(ErrKind::Cancelled, "cancelado")),
        r = tokio::time::timeout(IDLE, rb.send()) => match r {
            Err(_) => return Err(ApiError::new(ErrKind::Timeout, "el proveedor no ha respondido en 120 s")),
            Ok(Err(e)) => return Err(map_send_error(e)),
            Ok(Ok(r)) => r,
        }
    };
    if resp.status().is_success() {
        Ok(resp)
    } else {
        Err(http_error(resp).await)
    }
}

fn map_send_error(e: reqwest::Error) -> ApiError {
    if e.is_timeout() {
        ApiError::new(ErrKind::Timeout, "tiempo de espera agotado")
    } else if e.is_connect() {
        ApiError::new(ErrKind::Network, format!("no se puede conectar con el proveedor: {e}"))
    } else {
        ApiError::new(ErrKind::Network, e.to_string())
    }
}

/// Extract a human message from the many error body shapes (Anthropic, OpenAI, Ollama, Gemini).
pub fn error_message(body: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return body.trim().chars().take(300).collect(),
    };
    let v = if let Some(a) = v.as_array() { a.first().cloned().unwrap_or_default() } else { v };
    let e = v.get("error").unwrap_or(&v);
    if let Some(s) = e.as_str() {
        return s.chars().take(300).collect();
    }
    e.get("message").and_then(|m| m.as_str()).or_else(|| v.get("message").and_then(|m| m.as_str())).unwrap_or(body).chars().take(300).collect()
}

pub fn classify(status: u16, body: &str) -> ApiError {
    let msg = error_message(body);
    let low = msg.to_lowercase();
    let kind = match status {
        401 | 403 => ErrKind::Auth,
        404 => ErrKind::NotFound,
        408 | 504 => ErrKind::Server,
        429 => ErrKind::RateLimit,
        s if s >= 500 => ErrKind::Server,
        _ if low.contains("does not support tools") || low.contains("tools are not supported") || low.contains("tool use is not supported") || low.contains("does not support function") => ErrKind::NoTools,
        _ => ErrKind::Bad,
    };
    // Anthropic reports overload as 529 / "overloaded_error"; some gateways use 429 for quota exhaustion.
    let kind = if status == 429 && (low.contains("quota") || low.contains("insufficient") || low.contains("billing")) { ErrKind::Bad } else { kind };
    ApiError::new(kind, format!("{status}: {msg}"))
}

async fn http_error(resp: reqwest::Response) -> ApiError {
    let status = resp.status().as_u16();
    let retry = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|s| s.trim().parse::<f64>().ok()).map(|s| Duration::from_secs_f64(s.clamp(0.0, 60.0)));
    let body = resp.text().await.unwrap_or_default();
    let mut e = classify(status, &body);
    e.retry_after = retry;
    e
}

fn auth_hint(kind: AiProviderKind, e: &mut ApiError) {
    if e.kind == ErrKind::Auth {
        e.msg = format!("API key rechazada por el proveedor ({}). Revisa la clave en Ajustes → Asistente IA.", e.msg);
    }
    let _ = kind;
}

/// Spanish, user-facing text for an error.
pub fn user_message(e: &ApiError) -> String {
    match e.kind {
        ErrKind::Auth => "API key rechazada por el proveedor. Revisa la clave en Ajustes → Asistente IA.".into(),
        ErrKind::RateLimit => format!("Límite de uso del proveedor alcanzado ({}). Espera un momento.", short(&e.msg)),
        ErrKind::Server => format!("El proveedor tiene problemas ahora mismo ({}).", short(&e.msg)),
        ErrKind::Network => format!("No hay conexión con el proveedor: {}", short(&e.msg)),
        ErrKind::Timeout => format!("El proveedor tarda demasiado: {}", short(&e.msg)),
        ErrKind::NotFound => format!("Modelo o URL no encontrados ({}). Actualiza la lista de modelos en Ajustes → Asistente IA.", short(&e.msg)),
        ErrKind::NoTools => "Este modelo no admite herramientas: se usará solo como chat y ayuda.".into(),
        ErrKind::Bad => format!("Petición rechazada por el proveedor: {}", short(&e.msg)),
        ErrKind::Cancelled => "Cancelado.".into(),
    }
}

// ───────────── model listing / test ─────────────

pub async fn list_models(t: &Target) -> Result<Vec<AiModelInfo>, ApiError> {
    let (_tx, mut cancel) = tokio::sync::watch::channel(false);
    let url = match t.kind {
        AiProviderKind::Anthropic => format!("{}/v1/models?limit=1000", t.base_url),
        AiProviderKind::OpenaiCompat => format!("{}/models", t.base_url),
    };
    let mut rb = http().get(url);
    rb = match t.kind {
        AiProviderKind::Anthropic => {
            let mut rb = rb.header("anthropic-version", "2023-06-01");
            if let Some(k) = &t.api_key {
                rb = rb.header("x-api-key", k);
            }
            rb
        }
        AiProviderKind::OpenaiCompat => match &t.api_key {
            Some(k) => rb.bearer_auth(k),
            None => rb,
        },
    };
    let resp = send(rb, &mut cancel).await.map_err(|mut e| {
        auth_hint(t.kind, &mut e);
        e
    })?;
    let v: serde_json::Value = resp.json().await.map_err(|e| ApiError::new(ErrKind::Bad, format!("respuesta no válida: {e}")))?;
    let arr = v.get("data").and_then(|d| d.as_array()).or_else(|| v.get("models").and_then(|d| d.as_array())).cloned().unwrap_or_default();
    let skip = ["embed", "whisper", "tts", "dall-e", "moderation", "rerank", "imagen", "veo", "audio", "transcribe", "image", "realtime", "guard"];
    let mut out = Vec::new();
    for m in arr {
        let id = m.get("id").or_else(|| m.get("name")).and_then(|x| x.as_str()).unwrap_or("").trim_start_matches("models/").to_string();
        if id.is_empty() {
            continue;
        }
        let low = id.to_lowercase();
        if t.kind == AiProviderKind::OpenaiCompat && skip.iter().any(|s| low.contains(s)) {
            continue;
        }
        let label = m.get("display_name").and_then(|x| x.as_str()).unwrap_or(&id).to_string();
        out.push(AiModelInfo { id, label, note: String::new(), vision: t.kind == AiProviderKind::Anthropic, tools: true, reasoning: false });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Merge a refreshed model list into the stored one, keeping user-set flags for known ids.
pub fn merge_models(existing: &[AiModelInfo], fresh: Vec<AiModelInfo>) -> Vec<AiModelInfo> {
    let mut out: Vec<AiModelInfo> = Vec::new();
    for f in fresh {
        match existing.iter().find(|e| e.id == f.id) {
            Some(e) => out.push(e.clone()),
            None => out.push(f),
        }
    }
    // keep manually added models that the service does not list (e.g. local aliases)
    for e in existing {
        if !out.iter().any(|o| o.id == e.id) {
            out.push(e.clone());
        }
    }
    out
}

/// Tiny non-streaming request: validates URL, key and model.
pub async fn test(t: &Target) -> Result<(), ApiError> {
    let (_tx, mut cancel) = tokio::sync::watch::channel(false);
    let (url, body) = match t.kind {
        AiProviderKind::Anthropic => (format!("{}/v1/messages", t.base_url), serde_json::json!({ "model": t.model.id, "max_tokens": 16, "messages": [{ "role": "user", "content": "Responde solo: ok" }] })),
        AiProviderKind::OpenaiCompat => {
            let mut b = serde_json::json!({ "model": t.model.id, "messages": [{ "role": "user", "content": "Responde solo: ok" }] });
            if t.model.reasoning {
                b["max_completion_tokens"] = 256.into();
            } else {
                b["max_tokens"] = 16.into();
            }
            (format!("{}/chat/completions", t.base_url), b)
        }
    };
    let mut rb = http().post(url).json(&body);
    rb = match t.kind {
        AiProviderKind::Anthropic => {
            let mut rb = rb.header("anthropic-version", "2023-06-01");
            if let Some(k) = &t.api_key {
                rb = rb.header("x-api-key", k);
            }
            rb
        }
        AiProviderKind::OpenaiCompat => match &t.api_key {
            Some(k) => rb.bearer_auth(k),
            None => rb,
        },
    };
    send(rb, &mut cancel).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_shapes() {
        assert_eq!(error_message(r#"{"type":"error","error":{"type":"x","message":"bad key"}}"#), "bad key");
        assert_eq!(error_message(r#"{"error":{"message":"nope","code":1}}"#), "nope");
        assert_eq!(error_message(r#"{"error":"model not found"}"#), "model not found");
        assert_eq!(error_message(r#"[{"error":{"message":"gemini says no"}}]"#), "gemini says no");
        assert_eq!(error_message("plain text"), "plain text");
    }

    #[test]
    fn classification() {
        assert_eq!(classify(401, "{}").kind, ErrKind::Auth);
        assert_eq!(classify(429, "{}").kind, ErrKind::RateLimit);
        assert_eq!(classify(429, r#"{"error":{"message":"You exceeded your current quota"}}"#).kind, ErrKind::Bad);
        assert_eq!(classify(529, "{}").kind, ErrKind::Server);
        assert_eq!(classify(404, "{}").kind, ErrKind::NotFound);
        assert_eq!(classify(400, r#"{"error":"registry.ollama.ai/library/gemma does not support tools"}"#).kind, ErrKind::NoTools);
        assert_eq!(classify(400, "{}").kind, ErrKind::Bad);
        assert!(classify(500, "{}").retryable());
        assert!(!classify(400, "{}").retryable());
    }

    #[test]
    fn merge_keeps_user_flags() {
        let ex = vec![AiModelInfo { id: "a".into(), label: "A".into(), note: "n".into(), vision: true, tools: false, reasoning: true }];
        let fresh = vec![
            AiModelInfo { id: "a".into(), label: "A2".into(), note: String::new(), vision: false, tools: true, reasoning: false },
            AiModelInfo { id: "b".into(), label: "B".into(), note: String::new(), vision: false, tools: true, reasoning: false },
        ];
        let m = merge_models(&ex, fresh);
        assert_eq!(m.len(), 2);
        assert!(m[0].vision && !m[0].tools && m[0].reasoning, "user flags preserved");
    }
}
