//! End-to-end tests: the real engine + hub talking to an in-process mock server that speaks both wire formats.
//! No real API is ever contacted.

use crate::engine::{self, Handle, Out};
use crate::model::*;
use crate::persist::Store;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

const AKEY: &str = "sk-ant-SECRET-123456";
const OKEY: &str = "sk-oai-SECRET-654321";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Wire {
    Anthropic,
    OpenAi,
}

enum Part {
    Text(&'static str),
    Tool(&'static str, Value),
}

#[derive(Default)]
struct Rec {
    /// (wire, auth header value, body)
    reqs: Vec<(Wire, String, Value)>,
}

type Shared = Arc<Mutex<Rec>>;

fn last_user_text(w: Wire, body: &Value) -> String {
    let msgs = body["messages"].as_array().cloned().unwrap_or_default();
    for m in msgs.iter().rev() {
        if m["role"] != "user" {
            continue;
        }
        return match &m["content"] {
            Value::String(s) => s.clone(),
            Value::Array(a) => a.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"),
            _ => String::new(),
        };
    }
    let _ = w;
    String::new()
}

fn has_tool_result(w: Wire, body: &Value) -> bool {
    let msgs = body["messages"].as_array().cloned().unwrap_or_default();
    let Some(last) = msgs.last() else { return false };
    match w {
        Wire::OpenAi => last["role"] == "tool",
        Wire::Anthropic => last["content"].as_array().map_or(false, |a| a.iter().any(|b| b["type"] == "tool_result")),
    }
}

fn script(w: Wire, body: &Value) -> Vec<Part> {
    if has_tool_result(w, body) {
        return vec![Part::Text("Hecho.")];
    }
    let t = last_user_text(w, body);
    if t.contains("[demo:scene]") {
        vec![Part::Text("Voy a crear la escena. "), Part::Tool("console_command", json!({"text": "fixture 1 thru 2 at 50; red at 100; record scene 1 Rojo; clear"}))]
    } else if t.contains("[demo:confirm]") {
        vec![Part::Tool("console_command", json!({"text": "blackout on"}))]
    } else if t.contains("[demo:ask]") {
        vec![Part::Tool("ask_user", json!({"question": "¿Qué color?", "options": ["Rojo", "Azul"]}))]
    } else if t.contains("[demo:badjson]") {
        vec![Part::Tool("__badjson", Value::Null)]
    } else {
        vec![Part::Text("Hola, soy el asistente.")]
    }
}

fn sse(events: Vec<(&str, Value)>, wire: Wire) -> Response {
    let mut s = String::new();
    for (ev, d) in events {
        if wire == Wire::Anthropic {
            s.push_str(&format!("event: {ev}\n"));
        }
        s.push_str(&format!("data: {d}\n\n"));
    }
    if wire == Wire::OpenAi {
        s.push_str("data: [DONE]\n\n");
    }
    ([("content-type", "text/event-stream")], s).into_response()
}

fn anthropic_events(parts: Vec<Part>) -> Vec<(&'static str, Value)> {
    let mut ev = vec![("message_start", json!({"type":"message_start","message":{"id":"m1","role":"assistant","usage":{"input_tokens":11,"output_tokens":1}}}))];
    let mut idx = 0;
    // thinking block first, like adaptive thinking would produce
    ev.push(("content_block_start", json!({"type":"content_block_start","index":idx,"content_block":{"type":"thinking","thinking":""}})));
    ev.push(("content_block_delta", json!({"type":"content_block_delta","index":idx,"delta":{"type":"thinking_delta","thinking":"pienso"}})));
    ev.push(("content_block_delta", json!({"type":"content_block_delta","index":idx,"delta":{"type":"signature_delta","signature":"sig"}})));
    ev.push(("content_block_stop", json!({"type":"content_block_stop","index":idx})));
    idx += 1;
    let mut tool = false;
    for p in parts {
        match p {
            Part::Text(t) => {
                ev.push(("content_block_start", json!({"type":"content_block_start","index":idx,"content_block":{"type":"text","text":""}})));
                ev.push(("content_block_delta", json!({"type":"content_block_delta","index":idx,"delta":{"type":"text_delta","text":t}})));
            }
            Part::Tool(n, a) => {
                tool = true;
                let args = if n == "__badjson" { "{\"text\": ".to_string() } else { a.to_string() };
                let n = if n == "__badjson" { "console_command" } else { n };
                ev.push(("content_block_start", json!({"type":"content_block_start","index":idx,"content_block":{"type":"tool_use","id":format!("toolu_{idx}"),"name":n,"input":{}}})));
                let (a1, a2) = args.split_at(args.len() / 2);
                for chunk in [a1, a2] {
                    ev.push(("content_block_delta", json!({"type":"content_block_delta","index":idx,"delta":{"type":"input_json_delta","partial_json":chunk}})));
                }
            }
        }
        ev.push(("content_block_stop", json!({"type":"content_block_stop","index":idx})));
        idx += 1;
    }
    ev.push(("message_delta", json!({"type":"message_delta","delta":{"stop_reason": if tool {"tool_use"} else {"end_turn"}},"usage":{"output_tokens":7}})));
    ev.push(("message_stop", json!({"type":"message_stop"})));
    ev
}

fn openai_events(parts: Vec<Part>) -> Vec<(&'static str, Value)> {
    let mut ev = Vec::new();
    let mut tool = false;
    let mut n_call = 0;
    for p in parts {
        match p {
            Part::Text(t) => ev.push(("", json!({"choices":[{"index":0,"delta":{"role":"assistant","content":t}}]}))),
            Part::Tool(n, a) => {
                tool = true;
                let args = if n == "__badjson" { "{\"text\": ".to_string() } else { a.to_string() };
                let n = if n == "__badjson" { "console_command" } else { n };
                let (a1, a2) = args.split_at(args.len() / 2);
                ev.push(("", json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":n_call,"id":format!("call_{n_call}"),"type":"function","function":{"name":n,"arguments":a1}}]}}]})));
                ev.push(("", json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":n_call,"function":{"arguments":a2}}]}}]})));
                n_call += 1;
            }
        }
    }
    ev.push(("", json!({"choices":[{"index":0,"delta":{},"finish_reason": if tool {"tool_calls"} else {"stop"}}],"usage":{"prompt_tokens":9,"completion_tokens":4}})));
    ev
}

async fn anthropic(State(rec): State<Shared>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let key = headers.get("x-api-key").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    rec.lock().reqs.push((Wire::Anthropic, key.clone(), body.clone()));
    if key != AKEY {
        return (StatusCode::UNAUTHORIZED, Json(json!({"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}))).into_response();
    }
    if headers.get("anthropic-version").is_none() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if last_user_text(Wire::Anthropic, &body).contains("[demo:error]") {
        return (StatusCode::BAD_REQUEST, Json(json!({"type":"error","error":{"type":"invalid_request_error","message":"modelo rechazado"}}))).into_response();
    }
    sse(anthropic_events(script(Wire::Anthropic, &body)), Wire::Anthropic)
}

async fn openai(State(rec): State<Shared>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    rec.lock().reqs.push((Wire::OpenAi, auth.clone(), body.clone()));
    if auth != format!("Bearer {OKEY}") {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error":{"message":"Incorrect API key provided"}}))).into_response();
    }
    sse(openai_events(script(Wire::OpenAi, &body)), Wire::OpenAi)
}

async fn models() -> Json<Value> {
    Json(json!({"data":[{"id":"gpt-mock-1"},{"id":"text-embedding-3-small"},{"id":"gpt-mock-2"}]}))
}

async fn mock() -> (String, Shared) {
    let rec: Shared = Default::default();
    let app = Router::new().route("/v1/messages", post(anthropic)).route("/chat/completions", post(openai)).route("/models", get(models)).with_state(rec.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move {
        let _ = axum::serve(l, app).await;
    });
    (url, rec)
}

struct Rig {
    h: Handle,
    rx: broadcast::Receiver<Out>,
    /// every JSON line ever broadcast (for leak checks)
    seen: Vec<String>,
    dir: std::path::PathBuf,
}

fn tmpdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("lumos-ai-e2e-{tag}-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

async fn rig(tag: &str) -> Rig {
    let dir = tmpdir(tag);
    let h = engine::spawn(Store::new(dir.clone()).unwrap());
    let rx = h.out.subscribe();
    h.command(Command::FixtureAdd { profile: "generic/rgb-par".into(), mode: "3ch".into(), count: 4, universe: 1, address: None, name: None }).await.ok();
    Rig { h, rx, seen: vec![], dir }
}

fn provider_in(id: &str, kind: AiProviderKind, base: &str, key: &str) -> AiConfigIn {
    AiConfigIn {
        provider: Some(AiProviderIn { id: id.into(), name: String::new(), kind, base_url: base.into(), api_key: Some(key.into()), models: None }),
        remove_provider: None,
        active_provider: Some(id.into()),
        model: None,
        enabled: None,
        effort: None,
        mode: None,
    }
}

impl Rig {
    async fn cfg(&self, c: AiConfigIn) {
        self.h.command(Command::AiConfigure { cfg: c }).await.unwrap();
    }

    async fn send(&self, session: &str, text: &str) {
        self.h
            .command(Command::AiSend { session: session.into(), text: text.into(), images: vec![], context: AiContext { workspace: "live".into(), selection: vec![1, 2], lang: "es".into() } })
            .await
            .unwrap();
    }

    /// Collect `ai_event`s of `session` until `stop` says to finish (or a 15 s timeout).
    async fn events(&mut self, session: &str, mut stop: impl FnMut(&Value) -> bool) -> Vec<Value> {
        let mut out = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            let m = tokio::time::timeout_at(deadline, self.rx.recv()).await.expect("timeout waiting for AI events").unwrap();
            if let Out::Json(j) = m {
                let s = j.to_string();
                self.seen.push(s.clone());
                let v: Value = serde_json::from_str(&s).unwrap();
                if v["t"] == "ai_event" && v["session"] == session {
                    let ev = v["ev"].clone();
                    let done = stop(&ev);
                    out.push(ev);
                    if done {
                        return out;
                    }
                }
            }
        }
    }

    async fn turn(&mut self, session: &str, text: &str) -> Vec<Value> {
        self.send(session, text).await;
        self.events(session, |e| matches!(e["type"].as_str(), Some("done") | Some("error"))).await
    }

    async fn show(&self) -> Show {
        self.h.get_show().await.unwrap()
    }

    fn no_leaks(&self) {
        for s in &self.seen {
            for k in [AKEY, OKEY] {
                assert!(!s.contains(k), "API key leaked in a broadcast: {}", &s[..s.len().min(200)]);
            }
        }
        let cfg_file = std::fs::read_to_string(self.dir.join("ai.json")).unwrap();
        assert!(cfg_file.contains(AKEY) || cfg_file.contains(OKEY), "keys live in ai.json");
        let autosave = std::fs::read_dir(&self.dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name() != "ai.json").filter_map(|e| std::fs::read_to_string(e.path()).ok()).collect::<String>();
        assert!(!autosave.contains(AKEY) && !autosave.contains(OKEY), "keys must never reach show files");
    }
}

fn types(evs: &[Value]) -> Vec<String> {
    evs.iter().map(|e| e["type"].as_str().unwrap_or("").to_string()).collect()
}

#[tokio::test]
async fn anthropic_scene_turn_with_undo_and_no_key_leaks() {
    let (url, rec) = mock().await;
    let mut r = rig("anth").await;
    r.cfg(provider_in("anthropic", AiProviderKind::Anthropic, &url, AKEY)).await;
    // set auto mode so nothing needs confirmation
    r.cfg(AiConfigIn { provider: None, remove_provider: None, active_provider: None, model: None, enabled: None, effort: None, mode: Some(AiMode::Auto) }).await;
    let evs = r.turn("s1", "Crea una escena roja [demo:scene]").await;
    let t = types(&evs);
    assert_eq!(t.first().map(|s| s.as_str()), Some("started"), "{t:?}");
    assert!(t.contains(&"text".to_string()) && t.contains(&"tool".to_string()), "{t:?}");
    let done = evs.last().unwrap();
    assert_eq!(done["type"], "done", "{evs:?}");
    assert!(done["changes"].as_u64().unwrap() >= 1, "scene recording must be counted as undo steps: {done}");
    assert_eq!(done["input_tokens"].as_u64().unwrap(), 22, "11 per request × 2 requests");
    let tool_done = evs.iter().find(|e| e["type"] == "tool" && e["state"] == "done").expect("tool finished");
    assert_eq!(tool_done["risk"], "live", "programmer values outside blind change the output");
    let show = r.show().await;
    assert!(show.scenes.iter().any(|s| s.name == "Rojo"), "scene was recorded: {:?} {evs:?}", show.scenes.iter().map(|s| s.name.clone()).collect::<Vec<_>>());

    // request shape
    {
        let g = rec.lock();
        let (_, key, body) = &g.reqs[0];
        assert_eq!(key, AKEY);
        assert_eq!(body["stream"], true);
        assert!(body["tools"].as_array().unwrap().len() >= 11);
        let sys = body["system"].to_string();
        assert!(!sys.contains("seleccion_en_la_interfaz"), "dynamic context must not be in the cached system prompt");
        assert!(!sys.contains(AKEY));
        let first_user = body["messages"][0]["content"].to_string();
        assert!(first_user.contains("<contexto_lumos>") && first_user.contains("aparatos 1,2"));
        // second request carries the assistant turn verbatim (thinking + signature) and the tool_result first
        let (_, _, b2) = &g.reqs[1];
        let asst = &b2["messages"][1]["content"];
        assert_eq!(asst[0]["type"], "thinking");
        assert_eq!(asst[0]["signature"], "sig");
        assert_eq!(b2["messages"][2]["content"][0]["type"], "tool_result");
    }

    // undo what the assistant did
    r.h.command(Command::AiUndo { session: "s1".into() }).await.unwrap();
    r.events("s1", |e| e["type"] == "done").await;
    assert!(!r.show().await.scenes.iter().any(|s| s.name == "Rojo"), "AiUndo removes the assistant's scene");
    r.no_leaks();
}

#[tokio::test]
async fn smart_mode_asks_before_live_actions_and_denial_is_reported_to_the_model() {
    let (url, rec) = mock().await;
    let mut r = rig("confirm").await;
    r.cfg(provider_in("anthropic", AiProviderKind::Anthropic, &url, AKEY)).await;
    r.send("s", "apaga todo [demo:confirm]").await;
    let evs = r.events("s", |e| e["type"] == "confirm").await;
    let c = evs.last().unwrap();
    assert_eq!(c["risk"], "live");
    let call = c["call"].as_str().unwrap().to_string();
    r.h.command(Command::AiConfirm { session: "s".into(), call, approve: false }).await.unwrap();
    let evs = r.events("s", |e| e["type"] == "done").await;
    assert!(evs.iter().any(|e| e["type"] == "tool" && e["state"] == "denied"), "{evs:?}");
    // nothing ran
    let rt_blackout = r.h.ai_state().await.unwrap().rt.blackout;
    assert!(!rt_blackout);
    let g = rec.lock();
    let last = &g.reqs.last().unwrap().2;
    assert!(last["messages"].to_string().contains("denegado"), "the model is told the user said no");
}

#[tokio::test]
async fn approved_live_action_runs() {
    let (url, _rec) = mock().await;
    let mut r = rig("approve").await;
    r.cfg(provider_in("anthropic", AiProviderKind::Anthropic, &url, AKEY)).await;
    r.send("s", "[demo:confirm]").await;
    let evs = r.events("s", |e| e["type"] == "confirm").await;
    let call = evs.last().unwrap()["call"].as_str().unwrap().to_string();
    r.h.command(Command::AiConfirm { session: "s".into(), call, approve: true }).await.unwrap();
    r.events("s", |e| e["type"] == "done").await;
    assert!(r.h.ai_state().await.unwrap().rt.blackout);
}

#[tokio::test]
async fn ask_user_round_trip_and_cancel() {
    let (url, _rec) = mock().await;
    let mut r = rig("ask").await;
    r.cfg(provider_in("anthropic", AiProviderKind::Anthropic, &url, AKEY)).await;
    r.send("s", "[demo:ask]").await;
    let evs = r.events("s", |e| e["type"] == "ask").await;
    let ask = evs.last().unwrap();
    assert_eq!(ask["options"], json!(["Rojo", "Azul"]));
    let call = ask["call"].as_str().unwrap().to_string();
    r.h.command(Command::AiAnswer { session: "s".into(), call, answer: "Rojo".into() }).await.unwrap();
    let evs = r.events("s", |e| e["type"] == "done").await;
    assert!(evs.iter().any(|e| e["type"] == "tool" && e["state"] == "done" && e["result"] == "Rojo"));
    // cancel while waiting for an answer
    r.send("s", "[demo:ask]").await;
    r.events("s", |e| e["type"] == "ask").await;
    r.h.command(Command::AiCancel { session: "s".into() }).await.unwrap();
    let evs = r.events("s", |e| e["type"] == "done").await;
    assert!(evs.iter().any(|e| e["type"] == "status" && e["text"] == "Cancelado."));
    // the session is usable again and a second concurrent send is rejected while running
    let evs = r.turn("s", "hola").await;
    assert_eq!(evs.last().unwrap()["type"], "done");
}

#[tokio::test]
async fn busy_session_rejects_second_message() {
    let (url, _rec) = mock().await;
    let mut r = rig("busy").await;
    r.cfg(provider_in("anthropic", AiProviderKind::Anthropic, &url, AKEY)).await;
    r.send("s", "[demo:ask]").await;
    r.events("s", |e| e["type"] == "ask").await;
    let e = r.h.command(Command::AiSend { session: "s".into(), text: "otra".into(), images: vec![], context: AiContext { workspace: "live".into(), selection: vec![], lang: "es".into() } }).await.unwrap_err();
    assert!(e.contains("respondiendo"), "{e}");
    r.h.command(Command::AiCancel { session: "s".into() }).await.unwrap();
    r.events("s", |e| e["type"] == "done").await;
}

#[tokio::test]
async fn openai_wire_tool_loop_and_provider_switch_keeps_history() {
    let (url, rec) = mock().await;
    let mut r = rig("switch").await;
    r.cfg(provider_in("anthropic", AiProviderKind::Anthropic, &url, AKEY)).await;
    r.cfg(provider_in("openai", AiProviderKind::OpenaiCompat, &url, OKEY)).await;
    r.cfg(AiConfigIn { provider: None, remove_provider: None, active_provider: Some("anthropic".into()), model: None, enabled: None, effort: None, mode: Some(AiMode::Auto) }).await;
    let evs = r.turn("sw", "Crea una escena roja [demo:scene]").await;
    assert_eq!(evs.last().unwrap()["type"], "done", "{evs:?}");
    // switch provider mid-conversation
    r.cfg(AiConfigIn { provider: None, remove_provider: None, active_provider: Some("openai".into()), model: None, enabled: None, effort: None, mode: None }).await;
    let evs = r.turn("sw", "gracias").await;
    assert_eq!(evs.last().unwrap()["type"], "done", "{evs:?}");
    {
        let g = rec.lock();
        let (wire, auth, body) = g.reqs.last().unwrap();
        assert_eq!(*wire, Wire::OpenAi);
        assert_eq!(auth, &format!("Bearer {OKEY}"));
        let s = body.to_string();
        assert!(s.contains("Voy a crear la escena"), "neutral history survives the switch");
        assert!(s.contains("call_") || s.contains("toolu_"), "tool calls survive as tool_calls");
        assert!(!s.contains("\"signature\"") && !s.contains("thinking"), "provider-specific thinking blocks are dropped");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["stream_options"]["include_usage"], true);
    }
    // a fresh OpenAI-wire tool loop works too
    let evs = r.turn("oa", "otra escena [demo:scene]").await;
    assert!(evs.iter().any(|e| e["type"] == "tool" && e["state"] == "done"), "{evs:?}");
    assert_eq!(evs.last().unwrap()["type"], "done");
    r.no_leaks();
}

#[tokio::test]
async fn invalid_tool_json_and_provider_errors_are_reported() {
    let (url, _rec) = mock().await;
    let mut r = rig("err").await;
    r.cfg(provider_in("anthropic", AiProviderKind::Anthropic, &url, AKEY)).await;
    r.cfg(AiConfigIn { provider: None, remove_provider: None, active_provider: None, model: None, enabled: None, effort: None, mode: Some(AiMode::Auto) }).await;
    let evs = r.turn("e", "[demo:badjson]").await;
    assert!(evs.iter().any(|e| e["type"] == "tool" && e["state"] == "error" && e["result"].as_str().unwrap().contains("JSON")), "{evs:?}");
    assert_eq!(evs.last().unwrap()["type"], "done");
    let evs = r.turn("e2", "[demo:error]").await;
    let last = evs.last().unwrap();
    assert_eq!(last["type"], "error");
    assert!(last["msg"].as_str().unwrap().contains("modelo rechazado"), "{last}");
    // wrong key → friendly auth message, key not echoed
    r.cfg(provider_in("anthropic", AiProviderKind::Anthropic, &url, "sk-wrong-key-000")).await;
    let evs = r.turn("e3", "hola").await;
    let last = evs.last().unwrap();
    assert_eq!(last["type"], "error");
    assert!(last["msg"].as_str().unwrap().contains("API key rechazada"));
    assert!(!last["msg"].as_str().unwrap().contains("sk-wrong"));
}

#[tokio::test]
async fn unconfigured_assistant_explains_itself() {
    let mut r = rig("none").await;
    let evs = r.turn("s", "hola").await;
    let last = evs.last().unwrap();
    assert_eq!(last["type"], "error");
    assert!(last["msg"].as_str().unwrap().contains("no está configurado"));
}

#[tokio::test]
async fn model_list_and_connection_test() {
    let (url, _rec) = mock().await;
    let mut r = rig("models").await;
    r.cfg(provider_in("openai", AiProviderKind::OpenaiCompat, &url, OKEY)).await;
    r.h.command(Command::AiListModels { provider: "openai".into() }).await.unwrap();
    let mut cfg = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while cfg.is_none() {
        if let Ok(Ok(Out::Json(j))) = tokio::time::timeout_at(deadline, r.rx.recv()).await {
            let v: Value = serde_json::from_str(&j).unwrap();
            if v["t"] == "ai_config" && v["cfg"]["providers"][0]["models"].as_array().map_or(false, |m| m.iter().any(|x| x["id"] == "gpt-mock-1")) {
                cfg = Some(v);
            }
        }
    }
    let ids: Vec<String> = cfg.unwrap()["cfg"]["providers"][0]["models"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap().to_string()).collect();
    assert!(ids.contains(&"gpt-mock-2".to_string()));
    assert!(!ids.iter().any(|i| i.contains("embedding")), "non-chat models are filtered: {ids:?}");
    assert!(ids.contains(&"gpt-5".to_string()), "preset models are kept");
    // connection test against the OpenAI route (mock answers SSE; the test only needs a 2xx)
    r.h.command(Command::AiTestProvider { provider: "openai".into() }).await.unwrap();
    let t = loop {
        if let Ok(Out::Json(j)) = r.rx.recv().await {
            let v: Value = serde_json::from_str(&j).unwrap();
            if v["t"] == "ai_test" {
                break v;
            }
        }
    };
    assert_eq!(t["ok"], true, "{t}");
    r.no_leaks();
}

#[tokio::test]
async fn new_clients_receive_config_without_keys() {
    let (url, _rec) = mock().await;
    let r = rig("snap").await;
    r.cfg(provider_in("anthropic", AiProviderKind::Anthropic, &url, AKEY)).await;
    let snap = r.h.snapshot().await.join("\n");
    assert!(snap.contains("\"t\":\"ai_config\""));
    assert!(!snap.contains(AKEY));
    assert!(snap.contains("\"has_key\":true"));
}

/// The assistant can inspect a misconfigured Art-Net output on a live engine and gets the concrete fix.
#[tokio::test]
async fn diagnose_outputs_reads_a_real_engine() {
    let r = rig("diag").await;
    r.h.command(Command::UniverseSave {
        universe: UniverseCfg { id: 1, name: "Frente".into(), outputs: vec![OutputCfg { enabled: true, kind: OutputKind::ArtNet { host: "203.0.113.9".into(), port: 6455, net: 0, subnet: 0, universe: 0 } }] },
    })
    .await
    .unwrap();
    let out = super::tools::execute(&r.h, "diagnose_outputs", &json!({ "scan": false })).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let text = v["hallazgos"].to_string();
    assert!(text.contains("6454"), "{text}");
    assert!(text.contains("no ha respondido a ArtPoll"), "{text}");
    assert_eq!(v["universos"][0]["outputs"][0]["kind"], "artnet");
    assert!(v["universos"][0]["fixtures"].as_u64().unwrap() >= 4);
    let only = super::tools::execute(&r.h, "diagnose_outputs", &json!({ "universe": 99, "scan": false })).await.unwrap();
    assert!(only.contains("no existe en el show"));
    // read-only: never needs confirmation
    assert_eq!(super::tools::tool_risk("diagnose_outputs", &json!({}), false), AiRisk::Read);
    for t in ["script_save", "locate_save"] {
        let c = match t {
            "script_save" => json!({"type":"script_save","script":{"id":1,"name":"x","blocks":[]}}),
            _ => json!({"type":"locate_save","profile":"p","attrs":null}),
        };
        assert_eq!(super::tools::tool_risk("send_command", &json!({ "command": c }), false), AiRisk::Edit, "{t}");
    }
}

/// Every console example the assistant is taught (docs, tool description, prompt) must parse, and the
/// `send_command` recipes of the prompt must really run on the engine.
#[tokio::test]
async fn taught_examples_actually_work() {
    use crate::engine::cmdline_check;
    // 1. ```cmd blocks of the help corpus
    let mut n = 0;
    for (id, _, md) in super::help::TOPICS {
        let mut inside = false;
        for line in md.lines() {
            if line.trim_start().starts_with("```") {
                inside = line.trim() == "```cmd" && !inside;
                continue;
            }
            if !inside {
                continue;
            }
            // `a ; b   # comment` — the interpreter strips comments itself
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            cmdline_check(t).unwrap_or_else(|e| panic!("help `{id}`: «{t}» → {e}"));
            n += 1;
        }
    }
    assert!(n >= 25, "docs examples seen: {n}");
    // 2. grammar examples shown in the console_command description come from the interpreter table
    let g = crate::engine::cmdline_grammar();
    assert!(g.contains("osc <host>") && g.contains("script|guion"), "{g}");
    let desc = super::tools::definitions().into_iter().find(|d| d.name == "console_command").unwrap().description;
    assert!(desc.contains("does NOT patch") && desc.contains("fixture 1 thru 4 at 50"));
    // 3. examples quoted in the prompt
    for ex in ["fixture 1 thru 8 at 50; red at 100; green at 0; blue at 0; record scene 1 Rojo", "fixture 1 thru 4 at 70; red at 100", "record cue 3 name Entrada", "go", "scene 2 on", "gm 80", "blackout on", "undo", "clear"] {
        cmdline_check(ex).unwrap_or_else(|e| panic!("prompt example «{ex}» → {e}"));
    }
    // the exact patch syntax the model tried must be rejected with a useful hint
    assert!(cmdline_check("fixture 4 profile \"generic/rgb-par\" mode \"7ch\" universe 1 address 1 name \"PAR1\"").is_err());

    // 4. prompt recipes run for real
    let r = rig("recipes").await;
    let recipes = [
        json!({"type":"fixture_add","profile":"generic/rgb-par","mode":"3ch","count":2,"universe":1,"address":null,"name":"PAR"}),
        json!({"type":"universe_save","universe":{"id":1,"name":"Frente","outputs":[{"enabled":true,"kind":{"type":"art_net","host":"192.168.1.50","port":6454,"net":0,"subnet":0,"universe":0}}]}}),
        json!({"type":"universe_save","universe":{"id":2,"name":"Sala","outputs":[{"enabled":false,"kind":{"type":"sacn","universe":1,"priority":100,"unicast":null}},{"enabled":false,"kind":{"type":"serial","driver":"enttec_pro","port":"COM3"}}]}}),
        json!({"type":"group_save","group":{"id":0,"name":"Frontales","fixtures":[1,2]}}),
        json!({"type":"output_test","mode":{"type":"level","universe":1,"level":30}}),
        json!({"type":"output_test","mode":{"type":"off"}}),
        json!({"type":"dmx_set","universe":1,"channel":5,"value":255}),
        json!({"type":"dmx_set","universe":1,"channel":5,"value":null}),
    ];
    for c in recipes {
        let out = super::tools::execute(&r.h, "send_command", &json!({ "command": c.clone() })).await;
        assert!(out.is_ok(), "{c} → {out:?}");
    }
    let sh = r.show().await;
    assert!(sh.fixtures.len() >= 6 && sh.groups.iter().any(|g| g.name == "Frontales"));
    assert!(matches!(sh.universes.iter().find(|u| u.id == 1).unwrap().outputs[0].kind, OutputKind::ArtNet { .. }));
    // and a console statement end-to-end with the explained error hint
    let e = super::tools::execute(&r.h, "console_command", &json!({"text":"fixture 4 profile x universe 1 address 1"})).await.unwrap_err();
    assert!(e.contains("no parchea"), "{e}");
    let ok = super::tools::execute(&r.h, "console_command", &json!({"text":"fixture 1 thru 2 at 50; red at 100; record group 9 Test"})).await;
    assert!(ok.is_ok(), "{ok:?}");
}
