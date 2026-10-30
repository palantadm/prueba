//! Provider-neutral conversation model shared by every adapter.

use serde_json::Value;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Text(String),
    Image { media_type: String, data: String },
    ToolUse { id: String, name: String, input: Value },
    ToolResult { id: String, content: String, is_error: bool },
    /// Provider-specific block kept verbatim (Anthropic thinking / redacted thinking).
    /// `origin` = `"<kind>:<model>"`; dropped when provider or model changes.
    Raw { origin: String, value: Value },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Msg {
    pub role: Role,
    pub blocks: Vec<Block>,
}

impl Msg {
    pub fn user_text(t: impl Into<String>) -> Msg {
        Msg { role: Role::User, blocks: vec![Block::Text(t.into())] }
    }
}

#[derive(Clone, Debug)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON schema of the arguments (portable subset: type/properties/required/enum/items/description).
    pub schema: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stop {
    EndTurn,
    ToolUse,
    MaxTokens,
    PauseTurn,
    Refusal(String),
    Other(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    pub input: u32,
    pub output: u32,
}

#[derive(Clone, Debug)]
pub struct Turn {
    pub blocks: Vec<Block>,
    pub stop: Stop,
    pub usage: Usage,
}

/// Resolved target of one request.
#[derive(Clone)]
pub struct Target {
    pub kind: crate::model::AiProviderKind,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: crate::model::AiModelInfo,
}

impl std::fmt::Debug for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Target").field("kind", &self.kind).field("base_url", &self.base_url).field("model", &self.model.id).finish()
    }
}

impl Target {
    /// Marker stored in `Block::Raw.origin`.
    pub fn origin(&self) -> String {
        format!("{:?}:{}", self.kind, self.model.id)
    }
}

/// Streamed output of an adapter while a turn is being produced.
#[derive(Clone, Debug)]
pub enum Ev {
    Text(String),
    Status(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ErrKind {
    Auth,
    RateLimit,
    /// 5xx / overloaded: retryable.
    Server,
    Network,
    Timeout,
    /// 4xx other than auth/rate: not retryable.
    Bad,
    NotFound,
    /// Model rejected tool definitions.
    NoTools,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct ApiError {
    pub kind: ErrKind,
    pub msg: String,
    pub retry_after: Option<Duration>,
    /// Content was already streamed: retrying would duplicate output.
    pub partial: bool,
}

impl ApiError {
    pub fn new(kind: ErrKind, msg: impl Into<String>) -> Self {
        ApiError { kind, msg: msg.into(), retry_after: None, partial: false }
    }
    pub fn retryable(&self) -> bool {
        !self.partial && matches!(self.kind, ErrKind::RateLimit | ErrKind::Server | ErrKind::Network | ErrKind::Timeout)
    }
}

/// Cancellation handle: `true` once the user pressed stop.
pub type Cancel = tokio::sync::watch::Receiver<bool>;

pub async fn wait_cancel(c: &mut Cancel) {
    loop {
        if *c.borrow() {
            return;
        }
        if c.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

/// Remove `<think>…</think>` sections some servers inline into content while streaming.
#[derive(Default)]
pub struct ThinkFilter {
    in_think: bool,
    pending: String,
}

impl ThinkFilter {
    pub fn push(&mut self, chunk: &str) -> String {
        self.pending.push_str(chunk);
        let mut out = String::new();
        loop {
            let tag = if self.in_think { "</think>" } else { "<think>" };
            if let Some(i) = self.pending.find(tag) {
                if !self.in_think {
                    out.push_str(&self.pending[..i]);
                }
                self.pending.drain(..i + tag.len());
                self.in_think = !self.in_think;
                continue;
            }
            // keep a possible partial tag at the end
            let keep = (1..tag.len())
                .rev()
                .find(|&k| {
                    self.pending.len() >= k
                        && self.pending.is_char_boundary(self.pending.len() - k)
                        && tag.starts_with(&self.pending[self.pending.len() - k..])
                })
                .unwrap_or(0);
            let cut = self.pending.len() - keep;
            if !self.in_think {
                out.push_str(&self.pending[..cut]);
            }
            self.pending.drain(..cut);
            break;
        }
        out
    }

    pub fn finish(&mut self) -> String {
        let r = if self.in_think { String::new() } else { std::mem::take(&mut self.pending) };
        self.pending.clear();
        r
    }
}
