//! Anthropic Messages API adapter (Claude): adaptive thinking, effort, auto prompt caching,
//! server-side refusal fallback, SSE streaming. Raw HTTPS (no official Rust SDK).

use super::client::*;
use super::sse::SseEvent;
use super::types::*;
use crate::model::AiProviderKind;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

fn has_fallback(model: &str) -> bool {
    matches!(model, "claude-opus-5-5" | "claude-sonnet-5-5" | "claude-fable-5-1")
}

pub fn build_body(t: &Target, r: &ChatReq<'_>) -> Value {
    let mut b = json!({
        "model": t.model.id,
        "max_tokens": r.max_tokens,
        "stream": true,
        "system": r.system,
        "messages": to_messages(t, r.msgs),
        // top-level automatic prompt caching: the stable prefix (tools + system + history) is cached
        "cache_control": { "type": "ephemeral" },
    });
    if t.model.tools && !r.tools.is_empty() {
        b["tools"] = Value::Array(r.tools.iter().map(|d| json!({ "name": d.name, "description": d.description, "input_schema": d.schema })).collect());
    }
    if t.model.reasoning {
        b["thinking"] = json!({ "type": "adaptive" });
        b["output_config"] = json!({ "effort": r.effort });
    }
    if has_fallback(&t.model.id) {
        b["fallbacks"] = json!("default");
    }
    b
}

/// Neutral history → Anthropic `messages`. Raw blocks of another provider/model are dropped.
pub fn to_messages(t: &Target, msgs: &[Msg]) -> Vec<Value> {
    let origin = t.origin();
    let mut out = Vec::new();
    for m in msgs {
        let mut content: Vec<Value> = Vec::new();
        let mut results: Vec<Value> = Vec::new();
        for b in &m.blocks {
            match b {
                Block::Text(s) => {
                    if !s.is_empty() {
                        content.push(json!({ "type": "text", "text": s }));
                    }
                }
                Block::Image { media_type, data } => {
                    if t.model.vision {
                        content.push(json!({ "type": "image", "source": { "type": "base64", "media_type": media_type, "data": data } }));
                    } else {
                        content.push(json!({ "type": "text", "text": "[imagen omitida: el modelo no admite visión]" }));
                    }
                }
                Block::ToolUse { id, name, input } => content.push(json!({ "type": "tool_use", "id": id, "name": name, "input": input })),
                Block::ToolResult { id, content: c, is_error } => {
                    let mut v = json!({ "type": "tool_result", "tool_use_id": id, "content": c });
                    if *is_error {
                        v["is_error"] = json!(true);
                    }
                    results.push(v);
                }
                Block::Raw { origin: o, value } => {
                    if *o == origin {
                        content.push(value.clone());
                    }
                }
            }
        }
        // tool_result blocks must come first in a user message
        results.extend(content);
        if results.is_empty() {
            continue;
        }
        out.push(json!({ "role": match m.role { Role::User => "user", Role::Assistant => "assistant" }, "content": results }));
    }
    out
}

pub async fn stream(t: &Target, r: &ChatReq<'_>, on: &mut (dyn FnMut(Ev) + Send), cancel: &mut Cancel) -> Result<Turn, ApiError> {
    let body = build_body(t, r);
    let mut rb = http().post(format!("{}/v1/messages", t.base_url)).header("anthropic-version", "2023-06-01").header("content-type", "application/json").json(&body);
    if let Some(k) = &t.api_key {
        rb = rb.header("x-api-key", k);
    }
    if has_fallback(&t.model.id) {
        rb = rb.header("anthropic-beta", FALLBACK_BETA);
    }
    let resp = send(rb, cancel).await?;
    let mut acc = Accum::new(t.origin());
    read_sse(resp, cancel, |ev| acc.handle(&ev, on)).await.map_err(|mut e| {
        e.partial = acc.got_content;
        e
    })?;
    acc.finish()
}

enum Acc {
    Text(String),
    Tool { id: String, name: String, json: String },
    Raw(Value),
}

/// Pure SSE → turn accumulator (tested without network).
pub struct Accum {
    origin: String,
    blocks: BTreeMap<usize, Acc>,
    stop: Option<Stop>,
    usage: Usage,
    pub got_content: bool,
    done: bool,
}

impl Accum {
    pub fn new(origin: String) -> Self {
        Accum { origin, blocks: BTreeMap::new(), stop: None, usage: Usage::default(), got_content: false, done: false }
    }

    pub fn handle(&mut self, ev: &SseEvent, on: &mut (dyn FnMut(Ev) + Send)) -> Result<(), ApiError> {
        let Ok(v) = serde_json::from_str::<Value>(&ev.data) else { return Ok(()) };
        let ty = v.get("type").and_then(|x| x.as_str()).map(|s| s.to_string()).or_else(|| ev.event.clone()).unwrap_or_default();
        let idx = v.get("index").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        match ty.as_str() {
            "message_start" => {
                if let Some(u) = v.pointer("/message/usage") {
                    let g = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                    self.usage.input = g("input_tokens") + g("cache_read_input_tokens") + g("cache_creation_input_tokens");
                    self.usage.output = g("output_tokens");
                }
            }
            "content_block_start" => {
                self.got_content = true;
                let cb = v.get("content_block").cloned().unwrap_or(Value::Null);
                let kind = cb.get("type").and_then(|x| x.as_str()).unwrap_or("");
                let acc = match kind {
                    "text" => {
                        let t = cb.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        if !t.is_empty() {
                            on(Ev::Text(t.clone()));
                        }
                        Acc::Text(t)
                    }
                    "tool_use" => Acc::Tool {
                        id: cb.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                        name: cb.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                        json: String::new(),
                    },
                    "thinking" => {
                        on(Ev::Status("Pensando…".into()));
                        let mut c = cb.clone();
                        if c.get("thinking").is_none() {
                            c["thinking"] = json!("");
                        }
                        if c.get("signature").is_none() {
                            c["signature"] = json!("");
                        }
                        Acc::Raw(c)
                    }
                    _ => Acc::Raw(cb),
                };
                self.blocks.insert(idx, acc);
            }
            "content_block_delta" => {
                let d = v.get("delta").cloned().unwrap_or(Value::Null);
                match (d.get("type").and_then(|x| x.as_str()).unwrap_or(""), self.blocks.get_mut(&idx)) {
                    ("text_delta", Some(Acc::Text(s))) => {
                        let t = d.get("text").and_then(|x| x.as_str()).unwrap_or("");
                        s.push_str(t);
                        if !t.is_empty() {
                            on(Ev::Text(t.to_string()));
                        }
                    }
                    ("input_json_delta", Some(Acc::Tool { json, .. })) => json.push_str(d.get("partial_json").and_then(|x| x.as_str()).unwrap_or("")),
                    ("thinking_delta", Some(Acc::Raw(c))) => {
                        let cur = c.get("thinking").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        c["thinking"] = json!(cur + d.get("thinking").and_then(|x| x.as_str()).unwrap_or(""));
                    }
                    ("signature_delta", Some(Acc::Raw(c))) => {
                        let cur = c.get("signature").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        c["signature"] = json!(cur + d.get("signature").and_then(|x| x.as_str()).unwrap_or(""));
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(sr) = v.pointer("/delta/stop_reason").and_then(|x| x.as_str()) {
                    let cat = v.pointer("/delta/stop_details/category").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    self.stop = Some(match sr {
                        "end_turn" | "stop_sequence" => Stop::EndTurn,
                        "tool_use" => Stop::ToolUse,
                        "max_tokens" => Stop::MaxTokens,
                        "pause_turn" => Stop::PauseTurn,
                        "refusal" => Stop::Refusal(cat),
                        o => Stop::Other(o.to_string()),
                    });
                }
                if let Some(o) = v.pointer("/usage/output_tokens").and_then(|x| x.as_u64()) {
                    self.usage.output = o as u32;
                }
                if let Some(i) = v.pointer("/usage/input_tokens").and_then(|x| x.as_u64()) {
                    if i > 0 {
                        self.usage.input = i as u32;
                    }
                }
            }
            "message_stop" => self.done = true,
            "error" => {
                let msg = v.pointer("/error/message").and_then(|x| x.as_str()).unwrap_or("error del proveedor");
                let et = v.pointer("/error/type").and_then(|x| x.as_str()).unwrap_or("");
                let kind = match et {
                    "overloaded_error" | "api_error" => ErrKind::Server,
                    "rate_limit_error" => ErrKind::RateLimit,
                    "authentication_error" | "permission_error" => ErrKind::Auth,
                    "not_found_error" => ErrKind::NotFound,
                    _ => ErrKind::Bad,
                };
                return Err(ApiError::new(kind, format!("{et}: {msg}")));
            }
            _ => {}
        }
        Ok(())
    }

    pub fn finish(self) -> Result<Turn, ApiError> {
        if self.stop.is_none() && !self.done {
            let mut e = ApiError::new(ErrKind::Network, "la respuesta se cortó antes de terminar");
            e.partial = self.got_content;
            return Err(e);
        }
        let mut blocks = Vec::new();
        for (_, a) in self.blocks {
            match a {
                Acc::Text(s) => {
                    if !s.is_empty() {
                        blocks.push(Block::Text(s));
                    }
                }
                Acc::Tool { id, name, json } => {
                    let input = if json.trim().is_empty() { Ok(json!({})) } else { serde_json::from_str::<Value>(&json) };
                    let input = match input {
                        Ok(v) if v.is_object() => v,
                        _ => json!({ super::agent::INVALID_JSON_KEY: json }),
                    };
                    blocks.push(Block::ToolUse { id, name: super::tools::clean_tool_name(&name), input });
                }
                Acc::Raw(v) => blocks.push(Block::Raw { origin: self.origin.clone(), value: v }),
            }
        }
        let stop = self.stop.unwrap_or(if blocks.iter().any(|b| matches!(b, Block::ToolUse { .. })) { Stop::ToolUse } else { Stop::EndTurn });
        Ok(Turn { blocks, stop, usage: self.usage })
    }
}

pub fn kind() -> AiProviderKind {
    AiProviderKind::Anthropic
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AiModelInfo;

    fn target(model: &str, reasoning: bool, vision: bool) -> Target {
        Target {
            kind: AiProviderKind::Anthropic,
            base_url: "http://x".into(),
            api_key: Some("k".into()),
            model: AiModelInfo { id: model.into(), label: model.into(), note: String::new(), vision, tools: true, reasoning },
        }
    }

    fn ev(event: &str, data: Value) -> SseEvent {
        SseEvent { event: Some(event.into()), data: data.to_string() }
    }

    fn run(evs: Vec<SseEvent>) -> (Result<Turn, ApiError>, String) {
        let mut acc = Accum::new("Anthropic:claude-opus-5-5".into());
        let mut text = String::new();
        let mut on = |e: Ev| {
            if let Ev::Text(t) = e {
                text.push_str(&t)
            }
        };
        for e in evs {
            if let Err(e) = acc.handle(&e, &mut on) {
                return (Err(e), text);
            }
        }
        (acc.finish(), text)
    }

    #[test]
    fn body_has_adaptive_thinking_effort_cache_and_fallback() {
        let tools = vec![ToolDef { name: "get_runtime".into(), description: "d".into(), schema: json!({"type":"object","properties":{}}) }];
        let msgs = vec![Msg::user_text("hola")];
        let b = build_body(&target("claude-opus-5-5", true, true), &ChatReq { system: "sys", tools: &tools, msgs: &msgs, effort: "medium", max_tokens: 32000 });
        assert_eq!(b["thinking"], json!({"type":"adaptive"}));
        assert_eq!(b["output_config"]["effort"], "medium");
        assert_eq!(b["cache_control"]["type"], "ephemeral");
        assert_eq!(b["fallbacks"], "default");
        assert_eq!(b["stream"], true);
        assert!(b.get("tool_choice").is_none(), "forced tool_choice is a 400 on this model");
        assert_eq!(b["tools"][0]["input_schema"]["type"], "object");
        // non-reasoning custom model: no thinking/effort/fallbacks
        let b = build_body(&target("claude-3-haiku", false, true), &ChatReq { system: "sys", tools: &tools, msgs: &msgs, effort: "high", max_tokens: 1000 });
        assert!(b.get("thinking").is_none() && b.get("output_config").is_none() && b.get("fallbacks").is_none());
    }

    #[test]
    fn stream_text_thinking_and_parallel_tools() {
        let evs = vec![
            ev("message_start", json!({"type":"message_start","message":{"usage":{"input_tokens":100,"output_tokens":1}}})),
            ev("content_block_start", json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}})),
            ev("content_block_delta", json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hmm"}})),
            ev("content_block_delta", json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"SIG"}})),
            ev("content_block_stop", json!({"type":"content_block_stop","index":0})),
            ev("content_block_start", json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}})),
            ev("content_block_delta", json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Voy "}})),
            ev("content_block_delta", json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"a mirarlo."}})),
            ev("content_block_start", json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"get_show","input":{}}})),
            ev("content_block_delta", json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"sect"}})),
            ev("content_block_delta", json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"ion\":\"fixtures\"}"}})),
            ev("content_block_start", json!({"type":"content_block_start","index":3,"content_block":{"type":"tool_use","id":"toolu_2","name":"get_runtime","input":{}}})),
            ev("message_delta", json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":42}})),
            ev("message_stop", json!({"type":"message_stop"})),
        ];
        let (r, text) = run(evs);
        let t = r.unwrap();
        assert_eq!(text, "Voy a mirarlo.");
        assert_eq!(t.stop, Stop::ToolUse);
        assert_eq!(t.usage, Usage { input: 100, output: 42 });
        assert_eq!(t.blocks.len(), 4);
        match &t.blocks[0] {
            Block::Raw { value, .. } => {
                assert_eq!(value["thinking"], "hmm");
                assert_eq!(value["signature"], "SIG");
            }
            o => panic!("{o:?}"),
        }
        assert!(matches!(&t.blocks[2], Block::ToolUse { id, input, .. } if id == "toolu_1" && input["section"] == "fixtures"));
        assert!(matches!(&t.blocks[3], Block::ToolUse { id, input, .. } if id == "toolu_2" && input == &json!({})));
    }

    #[test]
    fn invalid_tool_json_is_flagged_not_dropped() {
        let evs = vec![
            ev("content_block_start", json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t","name":"x","input":{}}})),
            ev("content_block_delta", json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"a\":"}})),
            ev("message_delta", json!({"type":"message_delta","delta":{"stop_reason":"max_tokens"},"usage":{"output_tokens":5}})),
            ev("message_stop", json!({"type":"message_stop"})),
        ];
        let t = run(evs).0.unwrap();
        assert_eq!(t.stop, Stop::MaxTokens);
        assert!(matches!(&t.blocks[0], Block::ToolUse { input, .. } if input.get(super::super::agent::INVALID_JSON_KEY).is_some()));
    }

    #[test]
    fn refusal_error_and_truncation() {
        let evs = vec![ev("message_delta", json!({"type":"message_delta","delta":{"stop_reason":"refusal","stop_details":{"type":"refusal","category":"cyber"}},"usage":{"output_tokens":1}})), ev("message_stop", json!({"type":"message_stop"}))];
        assert_eq!(run(evs).0.unwrap().stop, Stop::Refusal("cyber".into()));
        let e = run(vec![ev("error", json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}))]).0.unwrap_err();
        assert_eq!(e.kind, ErrKind::Server);
        assert!(e.retryable());
        let e = run(vec![ev("content_block_start", json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"a"}}))]).0.unwrap_err();
        assert!(e.partial && !e.retryable(), "partial output must not be retried");
    }

    #[test]
    fn history_conversion_keeps_raw_only_for_same_origin() {
        let t = target("claude-opus-5-5", true, true);
        let msgs = vec![
            Msg::user_text("hola"),
            Msg {
                role: Role::Assistant,
                blocks: vec![
                    Block::Raw { origin: t.origin(), value: json!({"type":"thinking","thinking":"","signature":"S"}) },
                    Block::Raw { origin: "OpenaiCompat:gpt".into(), value: json!({"type":"thinking","thinking":"x","signature":"Z"}) },
                    Block::Text("ok".into()),
                    Block::ToolUse { id: "a".into(), name: "get_runtime".into(), input: json!({}) },
                ],
            },
            Msg { role: Role::User, blocks: vec![Block::Text("nota".into()), Block::ToolResult { id: "a".into(), content: "r".into(), is_error: true }] },
        ];
        let m = to_messages(&t, &msgs);
        assert_eq!(m.len(), 3);
        let asst = m[1]["content"].as_array().unwrap();
        assert_eq!(asst.len(), 3, "foreign raw block dropped");
        assert_eq!(asst[0]["signature"], "S");
        let user = m[2]["content"].as_array().unwrap();
        assert_eq!(user[0]["type"], "tool_result", "tool_result first");
        assert_eq!(user[0]["is_error"], true);
        // another model → thinking dropped as well
        let other = target("claude-sonnet-5-5", true, true);
        assert_eq!(to_messages(&other, &msgs)[1]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn images_become_blocks_or_notes() {
        let msgs = vec![Msg { role: Role::User, blocks: vec![Block::Image { media_type: "image/png".into(), data: "AAA".into() }, Block::Text("mira".into())] }];
        let m = to_messages(&target("claude-opus-5-5", true, true), &msgs);
        assert_eq!(m[0]["content"][0]["source"]["data"], "AAA");
        let m = to_messages(&target("claude-opus-5-5", true, false), &msgs);
        assert!(m[0]["content"][0]["text"].as_str().unwrap().contains("visión"));
    }
}
