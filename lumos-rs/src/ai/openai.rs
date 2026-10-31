//! OpenAI Chat Completions adapter. One wire format, many services: OpenAI, Google Gemini (its
//! OpenAI-compatible endpoint), OpenRouter, Groq, Mistral, DeepSeek, Together, xAI, Perplexity,
//! Azure-style bases and local servers (Ollama, LM Studio, llama.cpp, vLLM).
//!
//! Parsing is deliberately tolerant: servers differ in usage reporting, tool-call ids/indices,
//! whether arguments arrive in one chunk or many, and whether they stream at all.

use super::client::*;
use super::sse::SseEvent;
use super::types::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

static CALL_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn build_body(t: &Target, r: &ChatReq<'_>) -> Value {
    let mut b = json!({
        "model": t.model.id,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": to_messages(t, r.system, r.msgs),
    });
    if t.model.tools && !r.tools.is_empty() {
        b["tools"] = Value::Array(
            r.tools.iter().map(|d| json!({ "type": "function", "function": { "name": d.name, "description": d.description, "parameters": d.schema } })).collect(),
        );
    }
    if t.model.reasoning {
        b["max_completion_tokens"] = json!(r.max_tokens.max(8192));
        b["reasoning_effort"] = json!(r.effort);
    } else {
        b["max_tokens"] = json!(r.max_tokens.min(8192));
    }
    b
}

fn data_url(media: &str, data: &str) -> String {
    format!("data:{media};base64,{data}")
}

/// Neutral history → OpenAI `messages` (system first). Raw blocks are never sent.
pub fn to_messages(t: &Target, system: &str, msgs: &[Msg]) -> Vec<Value> {
    let mut out = vec![json!({ "role": "system", "content": system })];
    // tool names by call id (needed to render tool results as text for models without tool support)
    let mut names: HashMap<&str, &str> = HashMap::new();
    for m in msgs {
        for b in &m.blocks {
            if let Block::ToolUse { id, name, .. } = b {
                names.insert(id, name);
            }
        }
    }
    for m in msgs {
        match m.role {
            Role::Assistant => {
                let mut text = String::new();
                let mut calls = Vec::new();
                for b in &m.blocks {
                    match b {
                        Block::Text(s) => {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(s);
                        }
                        Block::ToolUse { id, name, input } => {
                            if t.model.tools {
                                calls.push(json!({ "id": id, "type": "function", "function": { "name": name, "arguments": input.to_string() } }));
                            } else {
                                text.push_str(&format!("\n(herramienta {name} usada)"));
                            }
                        }
                        _ => {}
                    }
                }
                if text.is_empty() && calls.is_empty() {
                    continue;
                }
                let mut v = json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { Value::String(text) } });
                if !calls.is_empty() {
                    v["tool_calls"] = Value::Array(calls);
                }
                out.push(v);
            }
            Role::User => {
                let mut parts: Vec<Value> = Vec::new();
                let mut all_text = true;
                for b in &m.blocks {
                    match b {
                        Block::ToolResult { id, content, is_error } => {
                            let body = if *is_error { format!("[error] {content}") } else { content.clone() };
                            if t.model.tools {
                                out.push(json!({ "role": "tool", "tool_call_id": id, "content": body }));
                            } else {
                                let n = names.get(id.as_str()).copied().unwrap_or("herramienta");
                                parts.push(json!({ "type": "text", "text": format!("Resultado de {n}: {body}") }));
                            }
                        }
                        Block::Text(s) => parts.push(json!({ "type": "text", "text": s })),
                        Block::Image { media_type, data } => {
                            if t.model.vision {
                                all_text = false;
                                parts.push(json!({ "type": "image_url", "image_url": { "url": data_url(media_type, data) } }));
                            } else {
                                parts.push(json!({ "type": "text", "text": "[imagen omitida: el modelo no admite visión]" }));
                            }
                        }
                        _ => {}
                    }
                }
                if parts.is_empty() {
                    continue;
                }
                if all_text {
                    let s = parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n\n");
                    out.push(json!({ "role": "user", "content": s }));
                } else {
                    out.push(json!({ "role": "user", "content": parts }));
                }
            }
        }
    }
    out
}

pub async fn stream(t: &Target, r: &ChatReq<'_>, on: &mut (dyn FnMut(Ev) + Send), cancel: &mut Cancel) -> Result<Turn, ApiError> {
    let body = build_body(t, r);
    let mut rb = http().post(format!("{}/chat/completions", t.base_url)).header("content-type", "application/json").json(&body);
    if let Some(k) = &t.api_key {
        rb = rb.bearer_auth(k);
    }
    let resp = send(rb, cancel).await?;
    let json_body = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).map_or(false, |c| c.contains("application/json"));
    let mut acc = Accum::new();
    if json_body {
        // server ignored stream:true
        let v: Value = resp.json().await.map_err(|e| ApiError::new(ErrKind::Bad, format!("respuesta no válida: {e}")))?;
        acc.handle_full(&v, on)?;
    } else {
        read_sse(resp, cancel, |ev| acc.handle(&ev, on)).await.map_err(|mut e| {
            e.partial = acc.got_content();
            e
        })?;
    }
    acc.finish()
}

struct ToolAcc {
    index: Option<i64>,
    id: String,
    name: String,
    args: String,
}

#[derive(Default)]
pub struct Accum {
    text: String,
    tools: Vec<ToolAcc>,
    finish: Option<String>,
    usage: Usage,
    think: ThinkFilter,
    signalled_reasoning: bool,
    done: bool,
    saw_chunk: bool,
}

impl Accum {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn got_content(&self) -> bool {
        !self.text.is_empty() || !self.tools.is_empty()
    }

    pub fn handle(&mut self, ev: &SseEvent, on: &mut (dyn FnMut(Ev) + Send)) -> Result<(), ApiError> {
        let d = ev.data.trim();
        if d == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let Ok(v) = serde_json::from_str::<Value>(d) else { return Ok(()) };
        self.saw_chunk = true;
        if let Some(e) = v.get("error") {
            let msg = error_message(&v.to_string());
            let _ = e;
            return Err(classify(500, &json!({ "error": { "message": msg } }).to_string()));
        }
        self.chunk(&v, on);
        Ok(())
    }

    /// Non-streaming completion body.
    pub fn handle_full(&mut self, v: &Value, on: &mut (dyn FnMut(Ev) + Send)) -> Result<(), ApiError> {
        if v.get("error").is_some() {
            return Err(classify(500, &v.to_string()));
        }
        self.saw_chunk = true;
        // reuse the chunk path: message → delta
        let mut c = v.clone();
        if let Some(choices) = c.get_mut("choices").and_then(|x| x.as_array_mut()) {
            for ch in choices {
                if let Some(m) = ch.get("message").cloned() {
                    ch["delta"] = m;
                }
            }
        }
        self.chunk(&c, on);
        self.done = true;
        Ok(())
    }

    fn chunk(&mut self, v: &Value, on: &mut (dyn FnMut(Ev) + Send)) {
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            let g = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            if g("prompt_tokens") > 0 {
                self.usage.input = g("prompt_tokens");
            }
            if g("completion_tokens") > 0 {
                self.usage.output = g("completion_tokens");
            }
        }
        let Some(ch) = v.get("choices").and_then(|c| c.as_array()).and_then(|a| a.first()) else { return };
        if let Some(f) = ch.get("finish_reason").and_then(|x| x.as_str()) {
            self.finish = Some(f.to_string());
        }
        let Some(delta) = ch.get("delta") else { return };
        // reasoning streams (DeepSeek, Ollama, OpenRouter): shown only as status
        let reasoning = delta.get("reasoning_content").or_else(|| delta.get("reasoning")).and_then(|x| x.as_str()).unwrap_or("");
        if !reasoning.is_empty() && !self.signalled_reasoning {
            self.signalled_reasoning = true;
            on(Ev::Status("Razonando…".into()));
        }
        let content = match delta.get("content") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(parts)) => parts.iter().filter_map(|p| p.get("text").and_then(|t| t.as_str())).collect::<String>(),
            _ => String::new(),
        };
        if !content.is_empty() {
            let shown = self.think.push(&content);
            if !shown.is_empty() {
                self.text.push_str(&shown);
                on(Ev::Text(shown));
            }
        }
        if let Some(calls) = delta.get("tool_calls").and_then(|x| x.as_array()) {
            for tc in calls {
                self.tool_delta(tc);
            }
        }
    }

    fn tool_delta(&mut self, tc: &Value) {
        let index = tc.get("index").and_then(|x| x.as_i64());
        let id = tc.get("id").and_then(|x| x.as_str()).unwrap_or("");
        let f = tc.get("function").cloned().unwrap_or(Value::Null);
        let name = f.get("name").and_then(|x| x.as_str()).unwrap_or("");
        let pos = match index {
            Some(i) => self.tools.iter().position(|t| t.index == Some(i)),
            None => {
                // no index: a new id starts a new call, otherwise continue the last one
                if !id.is_empty() && self.tools.last().map_or(true, |l| l.id != id) {
                    None
                } else if self.tools.is_empty() && !name.is_empty() {
                    None
                } else {
                    self.tools.len().checked_sub(1)
                }
            }
        };
        let pos = match pos {
            Some(p) => p,
            None => {
                self.tools.push(ToolAcc { index, id: String::new(), name: String::new(), args: String::new() });
                self.tools.len() - 1
            }
        };
        let t = &mut self.tools[pos];
        if t.id.is_empty() && !id.is_empty() {
            t.id = id.to_string();
        }
        if t.name.is_empty() && !name.is_empty() {
            t.name = name.to_string();
        }
        match f.get("arguments") {
            Some(Value::String(s)) => t.args.push_str(s),
            Some(o @ Value::Object(_)) => t.args = o.to_string(),
            _ => {}
        }
    }

    pub fn finish(mut self) -> Result<Turn, ApiError> {
        if !self.saw_chunk && !self.done {
            return Err(ApiError::new(ErrKind::Network, "el proveedor no devolvió ninguna respuesta"));
        }
        let rest = self.think.finish();
        self.text.push_str(&rest);
        let mut blocks = Vec::new();
        if !self.text.trim().is_empty() {
            blocks.push(Block::Text(self.text.clone()));
        }
        let mut seen: Vec<String> = Vec::new();
        for t in self.tools {
            if super::tools::clean_tool_name(&t.name).is_empty() {
                continue;
            }
            let mut id = t.id;
            if id.is_empty() || seen.contains(&id) {
                id = format!("call_{}", CALL_SEQ.fetch_add(1, Ordering::Relaxed));
            }
            seen.push(id.clone());
            let input = if t.args.trim().is_empty() { Ok(json!({})) } else { serde_json::from_str::<Value>(&t.args) };
            let input = match input {
                Ok(v) if v.is_object() => v,
                _ => json!({ super::agent::INVALID_JSON_KEY: t.args }),
            };
            blocks.push(Block::ToolUse { id, name: super::tools::clean_tool_name(&t.name), input });
        }
        let has_tools = blocks.iter().any(|b| matches!(b, Block::ToolUse { .. }));
        let stop = if has_tools {
            Stop::ToolUse
        } else {
            match self.finish.as_deref() {
                Some("length") => Stop::MaxTokens,
                Some("content_filter") => Stop::Refusal("content_filter".into()),
                Some("stop") | Some("end_turn") | None => Stop::EndTurn,
                Some(o) => Stop::Other(o.to_string()),
            }
        };
        Ok(Turn { blocks, stop, usage: self.usage })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AiModelInfo, AiProviderKind};

    fn target(tools: bool, vision: bool, reasoning: bool) -> Target {
        Target {
            kind: AiProviderKind::OpenaiCompat,
            base_url: "http://x/v1".into(),
            api_key: None,
            model: AiModelInfo { id: "m".into(), label: "m".into(), note: String::new(), vision, tools, reasoning },
        }
    }

    fn data(v: Value) -> SseEvent {
        SseEvent { event: None, data: v.to_string() }
    }

    fn run(evs: Vec<SseEvent>) -> (Result<Turn, ApiError>, String, Vec<String>) {
        let mut acc = Accum::new();
        let mut text = String::new();
        let mut status = Vec::new();
        let mut on = |e: Ev| match e {
            Ev::Text(t) => text.push_str(&t),
            Ev::Status(s) => status.push(s),
        };
        for e in evs {
            if let Err(e) = acc.handle(&e, &mut on) {
                return (Err(e), text, status);
            }
        }
        (acc.finish(), text, status)
    }

    #[test]
    fn body_switches_token_param_and_reasoning() {
        let tools = vec![ToolDef { name: "t".into(), description: "d".into(), schema: json!({"type":"object","properties":{"a":{"type":"string"}}}) }];
        let msgs = vec![Msg::user_text("hola")];
        let b = build_body(&target(true, false, false), &ChatReq { system: "S", tools: &tools, msgs: &msgs, effort: "medium", max_tokens: 32000 });
        assert_eq!(b["max_tokens"], 8192);
        assert!(b.get("max_completion_tokens").is_none() && b.get("reasoning_effort").is_none());
        assert_eq!(b["stream_options"]["include_usage"], true);
        assert_eq!(b["tools"][0]["function"]["parameters"]["type"], "object");
        assert_eq!(b["messages"][0], json!({"role":"system","content":"S"}));
        let b = build_body(&target(true, false, true), &ChatReq { system: "S", tools: &tools, msgs: &msgs, effort: "high", max_tokens: 32000 });
        assert_eq!(b["max_completion_tokens"], 32000);
        assert_eq!(b["reasoning_effort"], "high");
        assert!(b.get("max_tokens").is_none());
        let b = build_body(&target(false, false, false), &ChatReq { system: "S", tools: &tools, msgs: &msgs, effort: "high", max_tokens: 100 });
        assert!(b.get("tools").is_none(), "models without tools never get tool definitions");
    }

    #[test]
    fn standard_stream_with_tool_call_deltas() {
        let evs = vec![
            data(json!({"choices":[{"delta":{"role":"assistant","content":"Miro "}}]})),
            data(json!({"choices":[{"delta":{"content":"el patch."}}]})),
            data(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"get_show","arguments":""}}]}}]})),
            data(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"sec"}}]}}]})),
            data(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"tion\":\"fixtures\"}"}}]}}]})),
            data(json!({"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_b","type":"function","function":{"name":"get_runtime","arguments":"{}"}}]}}]})),
            data(json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]})),
            data(json!({"choices":[],"usage":{"prompt_tokens":50,"completion_tokens":7}})),
            SseEvent { event: None, data: "[DONE]".into() },
        ];
        let (r, text, _) = run(evs);
        let t = r.unwrap();
        assert_eq!(text, "Miro el patch.");
        assert_eq!(t.stop, Stop::ToolUse);
        assert_eq!(t.usage, Usage { input: 50, output: 7 });
        assert_eq!(t.blocks.len(), 3);
        assert!(matches!(&t.blocks[1], Block::ToolUse { id, name, input } if id == "call_a" && name == "get_show" && input["section"] == "fixtures"));
        assert!(matches!(&t.blocks[2], Block::ToolUse { id, input, .. } if id == "call_b" && input == &json!({})));
    }

    #[test]
    fn ollama_style_quirks() {
        // whole call in one chunk, no index, no id, arguments as an object, finish_reason "stop", no usage
        let evs = vec![
            data(json!({"choices":[{"delta":{"content":"<think>pienso</think>Hecho. "}}]})),
            data(json!({"choices":[{"delta":{"tool_calls":[{"function":{"name":"console_command","arguments":{"text":"gm 50"}}}]},"finish_reason":"stop"}]})),
        ];
        let (r, text, _) = run(evs);
        let t = r.unwrap();
        assert_eq!(text, "Hecho. ");
        assert_eq!(t.stop, Stop::ToolUse, "tool calls win over finish_reason=stop");
        assert_eq!(t.usage, Usage::default());
        match &t.blocks[1] {
            Block::ToolUse { id, name, input } => {
                assert!(id.starts_with("call_"), "synthesised id: {id}");
                assert_eq!(name, "console_command");
                assert_eq!(input["text"], "gm 50");
            }
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn two_calls_without_index_and_duplicate_ids() {
        let evs = vec![
            data(json!({"choices":[{"delta":{"tool_calls":[{"id":"x","function":{"name":"a","arguments":"{}"}}]}}]})),
            data(json!({"choices":[{"delta":{"tool_calls":[{"id":"x","function":{"name":"b","arguments":"{}"}}]}}]})),
            data(json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]})),
        ];
        // identical ids and no index → treated as continuation of the first call (name kept)
        let t = run(evs).0.unwrap();
        assert_eq!(t.blocks.iter().filter(|b| matches!(b, Block::ToolUse { .. })).count(), 1);
        let evs = vec![
            data(json!({"choices":[{"delta":{"tool_calls":[{"id":"x","function":{"name":"a","arguments":"{}"}},{"id":"x","function":{"name":"b","arguments":"{}"}}]}}]})),
            data(json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]})),
        ];
        let t = run(evs).0.unwrap();
        let ids: Vec<_> = t.blocks.iter().filter_map(|b| if let Block::ToolUse { id, .. } = b { Some(id.clone()) } else { None }).collect();
        assert_eq!(ids.len(), 1 + 0, "same id in one chunk merges; ids are unique after finish");
        let _ = ids;
    }

    #[test]
    fn finish_reasons_and_reasoning_stream() {
        let evs = vec![data(json!({"choices":[{"delta":{"reasoning_content":"hmm"}}]})), data(json!({"choices":[{"delta":{"content":"largo"},"finish_reason":"length"}]}))];
        let (r, _, status) = run(evs);
        assert_eq!(r.unwrap().stop, Stop::MaxTokens);
        assert_eq!(status, vec!["Razonando…".to_string()]);
        let r = run(vec![data(json!({"choices":[{"delta":{"content":"no"},"finish_reason":"content_filter"}]}))]).0.unwrap();
        assert!(matches!(r.stop, Stop::Refusal(_)));
    }

    #[test]
    fn non_streaming_body_is_accepted() {
        let mut acc = Accum::new();
        let mut text = String::new();
        let body = json!({"choices":[{"message":{"role":"assistant","content":"Hola","tool_calls":[{"id":"c1","type":"function","function":{"name":"get_runtime","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":3,"completion_tokens":2}});
        acc.handle_full(&body, &mut |e| if let Ev::Text(t) = e { text.push_str(&t) }).unwrap();
        let t = acc.finish().unwrap();
        assert_eq!(text, "Hola");
        assert_eq!(t.stop, Stop::ToolUse);
        assert_eq!(t.usage.input, 3);
    }

    #[test]
    fn stream_error_and_empty() {
        let e = run(vec![data(json!({"error":{"message":"boom","type":"server_error"}}))]).0.unwrap_err();
        assert_eq!(e.kind, ErrKind::Server);
        let e = run(vec![]).0.unwrap_err();
        assert_eq!(e.kind, ErrKind::Network);
    }

    #[test]
    fn history_conversion_tool_roundtrip_and_images() {
        let t = target(true, true, false);
        let msgs = vec![
            Msg { role: Role::User, blocks: vec![Block::Text("ctx".into()), Block::Text("hola".into()), Block::Image { media_type: "image/png".into(), data: "AAA".into() }] },
            Msg { role: Role::Assistant, blocks: vec![Block::Raw { origin: "Anthropic:x".into(), value: json!({"type":"thinking"}) }, Block::Text("ok".into()), Block::ToolUse { id: "c1".into(), name: "get_show".into(), input: json!({"section":"all"}) }, Block::ToolUse { id: "c2".into(), name: "get_runtime".into(), input: json!({}) }] },
            Msg { role: Role::User, blocks: vec![Block::ToolResult { id: "c1".into(), content: "r1".into(), is_error: false }, Block::ToolResult { id: "c2".into(), content: "r2".into(), is_error: true }] },
        ];
        let m = to_messages(&t, "S", &msgs);
        assert_eq!(m.len(), 1 + 1 + 1 + 2);
        assert_eq!(m[1]["content"][2]["image_url"]["url"], "data:image/png;base64,AAA");
        assert_eq!(m[2]["content"], "ok");
        assert_eq!(m[2]["tool_calls"].as_array().unwrap().len(), 2);
        assert_eq!(m[2]["tool_calls"][0]["function"]["arguments"], "{\"section\":\"all\"}");
        assert_eq!(m[3], json!({"role":"tool","tool_call_id":"c1","content":"r1"}));
        assert_eq!(m[4]["content"], "[error] r2");
        // no vision: image replaced by a note; no tools: tool traffic flattened to text
        let nt = target(false, false, false);
        let m = to_messages(&nt, "S", &msgs);
        assert!(m[1]["content"].as_str().unwrap().contains("imagen omitida"));
        assert!(m[2].get("tool_calls").is_none());
        assert!(m[3]["content"].as_str().unwrap().contains("Resultado de get_show: r1"));
    }
}
