//! HTTP + WebSocket front-end for the engine.

use crate::engine::{EngineIn, Handle, Out};
use crate::model::*;
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, DefaultBodyLimit, Multipart, State},
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rust_embed::RustEmbed;
use std::collections::HashSet;
use tokio::sync::{broadcast::error::RecvError, mpsc, oneshot};
use tower_http::{cors::CorsLayer, services::ServeDir};

#[derive(RustEmbed)]
#[folder = "ui/dist"]
struct Assets;

pub fn router(h: Handle) -> Router {
    let media = h.store.media_dir();
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/ports", get(ports))
        .route("/api/net", get(net))
        .route("/api/media", post(upload).layer(DefaultBodyLimit::max(512 * 1024 * 1024)))
        .route("/api/show", get(get_show).put(put_show).layer(DefaultBodyLimit::max(64 * 1024 * 1024)))
        .nest_service("/media", ServeDir::new(media))
        .fallback(static_handler)
        .layer(CorsLayer::permissive())
        .with_state(h)
}

// ───────────── REST ─────────────

async fn ports() -> Json<serde_json::Value> {
    let v: Vec<_> = tokio::task::spawn_blocking(crate::io::list_serial_ports)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(name, description)| serde_json::json!({ "name": name, "description": description }))
        .collect();
    Json(serde_json::Value::Array(v))
}

async fn net() -> Json<serde_json::Value> {
    let v: Vec<_> = local_ip_address::list_afinet_netifas()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, ip)| ip.is_ipv4())
        .map(|(name, ip)| serde_json::json!({ "name": name, "ip": ip.to_string() }))
        .collect();
    Json(serde_json::Value::Array(v))
}

async fn get_show(State(h): State<Handle>) -> Response {
    match h.get_show().await {
        Some(s) => (
            [(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}.json\"", s.name.replace('"', "")))],
            Json(s),
        )
            .into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "engine stopped").into_response(),
    }
}

async fn put_show(State(h): State<Handle>, Json(show): Json<Show>) -> Response {
    match h.import_show(show).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

fn safe_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("file");
    let s: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let s = s.trim_start_matches('.').to_string();
    if s.is_empty() { "file".into() } else { s.chars().take(100).collect() }
}

async fn upload(State(h): State<Handle>, mut mp: Multipart) -> Response {
    loop {
        let field = match mp.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => return (StatusCode::BAD_REQUEST, "no file field").into_response(),
            Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        };
        let Some(fname) = field.file_name().map(|s| s.to_string()) else { continue };
        let data = match field.bytes().await {
            Ok(b) => b,
            Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        };
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let name = format!("{ms}-{}", safe_filename(&fname));
        let path = h.store.media_dir().join(&name);
        if let Err(e) = tokio::fs::write(&path, &data).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
        return Json(serde_json::json!({ "url": format!("/media/{name}") })).into_response();
    }
}

// ───────────── static UI ─────────────

async fn static_handler(uri: Uri) -> Response {
    let raw = uri.path().trim_start_matches('/');
    let path = if raw.is_empty() { "index.html" } else { raw };
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Some(f) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let cache = if path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
        return ([(header::CONTENT_TYPE, mime.to_string()), (header::CACHE_CONTROL, cache.to_string())], f.data.into_owned())
            .into_response();
    }
    if !path.contains('.') {
        if let Some(f) = Assets::get("index.html") {
            return ([(header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()), (header::CACHE_CONTROL, "no-cache".to_string())], f.data.into_owned())
                .into_response();
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

// ───────────── WebSocket ─────────────

async fn ws_handler(ws: WebSocketUpgrade, State(h): State<Handle>) -> Response {
    ws.max_message_size(64 * 1024 * 1024).on_upgrade(move |s| client(s, h))
}

enum Sub {
    None,
    All,
    Some(HashSet<u16>),
}

impl Sub {
    fn wants(&self, u: u16) -> bool {
        match self {
            Sub::None => false,
            Sub::All => true,
            Sub::Some(s) => s.contains(&u),
        }
    }
}

fn ack_json(id: &str, r: &Result<(), String>) -> String {
    serde_json::to_string(&ServerMsg::Ack { id: id.into(), ok: r.is_ok(), err: r.as_ref().err().cloned() }).unwrap_or_default()
}

async fn client(mut socket: WebSocket, h: Handle) {
    // Subscribe before taking the snapshot so no update is missed in between.
    let mut rx = h.out.subscribe();
    for m in h.snapshot().await {
        if socket.send(Message::Text(m.into())).await.is_err() {
            return;
        }
    }
    let (ack_tx, mut ack_rx) = mpsc::unbounded_channel::<String>();
    let mut sub = Sub::None;

    loop {
        tokio::select! {
            msg = socket.recv() => match msg {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(Message::Text(t))) => {
                    let cm: ClientMsg = match serde_json::from_str(t.as_str()) {
                        Ok(c) => c,
                        Err(e) => {
                            let r: Result<(), String> = Err(format!("Bad message: {e}"));
                            let _ = socket.send(Message::Text(ack_json("", &r).into())).await;
                            continue;
                        }
                    };
                    if let Command::SubscribeDmx { universes } = &cm.cmd {
                        sub = match universes {
                            None => Sub::All,
                            Some(v) if v.is_empty() => Sub::None,
                            Some(v) => Sub::Some(v.iter().copied().collect()),
                        };
                        if !cm.id.is_empty() {
                            let _ = socket.send(Message::Text(ack_json(&cm.id, &Ok(())).into())).await;
                        }
                        continue;
                    }
                    if cm.id.is_empty() {
                        let _ = h.tx.send(EngineIn::Cmd { cmd: cm.cmd, reply: None });
                    } else {
                        let (r, rrx) = oneshot::channel();
                        if h.tx.send(EngineIn::Cmd { cmd: cm.cmd, reply: Some(r) }).is_err() {
                            break;
                        }
                        let id = cm.id;
                        let tx = ack_tx.clone();
                        tokio::spawn(async move {
                            let res = rrx.await.unwrap_or_else(|_| Err("Engine stopped".into()));
                            let _ = tx.send(ack_json(&id, &res));
                        });
                    }
                }
                Some(Ok(_)) => {}
            },
            out = rx.recv() => match out {
                Ok(Out::Json(s)) => {
                    if socket.send(Message::Text(s.to_string().into())).await.is_err() { break; }
                }
                Ok(Out::Dmx(u, data)) => {
                    if sub.wants(u) {
                        let mut buf = Vec::with_capacity(515);
                        buf.push(BIN_DMX);
                        buf.extend_from_slice(&u.to_be_bytes());
                        buf.extend_from_slice(&data[..]);
                        if socket.send(Message::Binary(buf.into())).await.is_err() { break; }
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    for m in h.snapshot().await {
                        if socket.send(Message::Text(m.into())).await.is_err() { return; }
                    }
                }
                Err(RecvError::Closed) => break,
            },
            Some(a) = ack_rx.recv() => {
                if socket.send(Message::Text(a.into())).await.is_err() { break; }
            }
        }
    }
}
