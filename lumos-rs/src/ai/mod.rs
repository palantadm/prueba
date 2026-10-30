//! In-app assistant: a provider-agnostic agent that drives the console through the same commands the UI uses.
//!
//! * `types`     – neutral conversation model (blocks/messages/turns) shared by all adapters.
//! * `anthropic` / `openai` – wire adapters (Claude Messages API, OpenAI-compatible Chat Completions).
//! * `client`    – HTTP plumbing: SSE, retries, error mapping, model listing, connection test.
//! * `config`    – machine-level providers/keys/model in `<data>/ai.json` (0600).
//! * `tools`     – the tools the model can call, their risk classes and executors.
//! * `agent`     – the turn loop (tools, confirmations, undo accounting, cancel).
//! * `prompt` / `help` – system prompt, per-turn context and the Spanish documentation corpus.
//!
//! The hub lives outside the engine task: the engine only forwards `Command::Ai*` to [`AiHub::dispatch`] and
//! answers `EngineIn::AiState` queries. Events travel back as `ServerMsg::AiEvent` on the engine's broadcast.

pub mod agent;
pub mod anthropic;
pub mod client;
pub mod config;
pub mod help;
pub mod openai;
pub mod prompt;
pub mod sse;
pub mod tools;
pub mod types;

use crate::engine::{EngineIn, Handle, Out};
use crate::model::*;
use crate::persist::Store;
use config::AiFile;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use types::*;

/// Read-only copy of what the engine knows, taken on demand for tools and the per-turn context.
pub struct AiState {
    pub show: Show,
    pub rt: Runtime,
    /// Built-in + show profiles.
    pub profiles: Vec<FixtureProfile>,
    /// Current output frame per universe.
    pub frames: Vec<(u16, Vec<u8>)>,
    /// Programmer + playback merged values, before DMX encoding.
    pub merged: PerFixture,
}

impl AiState {
    pub fn profile_mode(&self, profile: &str, mode: &str) -> (Option<&FixtureProfile>, Option<&Mode>) {
        let p = self.profiles.iter().find(|p| p.id == profile);
        (p, p.and_then(|p| p.modes.iter().find(|m| m.name == mode)))
    }
}

/// Answer to a pending question for the user.
pub(crate) enum Reply {
    Confirm(bool),
    Answer(String),
}

#[derive(Default)]
pub(crate) struct SessState {
    pub history: Vec<Msg>,
    pub running: bool,
    pub cancel: Option<watch::Sender<bool>>,
    pub waiting: HashMap<String, oneshot::Sender<Reply>>,
    /// Undo steps the assistant created in this session (and that are still undoable).
    pub steps: u32,
    /// Undo-stack size right after the assistant's last change; later user edits move it.
    pub undo_end: u16,
    /// Bumped by `AiReset`: a turn that started earlier must not write its history back.
    pub gen: u64,
    pub last_used: Option<Instant>,
}

pub(crate) struct Session {
    pub st: Mutex<SessState>,
}

pub(crate) struct Inner {
    store: Store,
    path: std::path::PathBuf,
    pub cfg: Mutex<AiFile>,
    out: broadcast::Sender<Out>,
    engine: Mutex<Option<mpsc::WeakUnboundedSender<EngineIn>>>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

pub const MAX_SESSIONS: usize = 32;
/// How long a confirmation / question waits for the user before it counts as denied / unanswered.
pub const ASK_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct AiHub {
    pub(crate) inner: Arc<Inner>,
}

fn session_id_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
}

impl AiHub {
    pub fn new(store: Store, out: broadcast::Sender<Out>) -> AiHub {
        let path = store.dir.join("ai.json");
        let cfg = AiFile::load(&path);
        AiHub { inner: Arc::new(Inner { store, path, cfg: Mutex::new(cfg), out, engine: Mutex::new(None), sessions: Mutex::new(HashMap::new()) }) }
    }

    /// Give the hub a (weak) way back into the engine. Weak so the hub never keeps the engine alive.
    pub fn attach(&self, tx: mpsc::WeakUnboundedSender<EngineIn>) {
        *self.inner.engine.lock() = Some(tx);
    }

    /// `ServerMsg::AiConfig` as JSON (sent to every new client).
    pub fn config_json(&self) -> String {
        serde_json::to_string(&ServerMsg::AiConfig { cfg: self.inner.cfg.lock().out() }).unwrap_or_default()
    }

    /// Execute one `Command::Ai*`. Never blocks: long work runs on spawned tasks.
    pub fn dispatch(&self, cmd: Command) -> Result<(), String> {
        let me = &self.inner;
        match cmd {
            Command::AiSend { session, text, images, context } => me.send(session, text, images, context),
            Command::AiCancel { session } => me.cancel(&session),
            Command::AiConfirm { session, call, approve } => me.reply(&session, &call, Reply::Confirm(approve)),
            Command::AiAnswer { session, call, answer } => me.reply(&session, &call, Reply::Answer(answer)),
            Command::AiReset { session } => me.reset(&session),
            Command::AiUndo { session } => me.undo(&session),
            Command::AiConfigure { cfg } => me.configure(cfg),
            Command::AiListModels { provider } => me.list_models(provider),
            Command::AiTestProvider { provider } => me.test_provider(provider),
            _ => Err("No es un comando del asistente".into()),
        }
    }
}

impl Inner {
    pub fn handle(&self) -> Option<Handle> {
        let tx = self.engine.lock().as_ref()?.upgrade()?;
        Some(Handle { tx, out: self.out.clone(), store: self.store.clone() })
    }

    pub fn emit(&self, session: &str, ev: AiEvent) {
        if let Ok(j) = serde_json::to_string(&ServerMsg::AiEvent { session: session.to_string(), ev }) {
            let _ = self.out.send(Out::Json(Arc::from(j)));
        }
    }

    fn emit_msg(&self, msg: &ServerMsg) {
        if let Ok(j) = serde_json::to_string(msg) {
            let _ = self.out.send(Out::Json(Arc::from(j)));
        }
    }

    pub fn emit_config(&self) {
        let cfg = self.cfg.lock().out();
        self.emit_msg(&ServerMsg::AiConfig { cfg });
    }

    fn session(&self, sid: &str) -> Arc<Session> {
        let mut map = self.sessions.lock();
        if let Some(s) = map.get(sid) {
            s.st.lock().last_used = Some(Instant::now());
            return s.clone();
        }
        if map.len() >= MAX_SESSIONS {
            let oldest = map.iter().filter(|(_, s)| !s.st.lock().running).min_by_key(|(_, s)| s.st.lock().last_used).map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                map.remove(&k);
            }
        }
        let s = Arc::new(Session { st: Mutex::new(SessState { last_used: Some(Instant::now()), ..Default::default() }) });
        map.insert(sid.to_string(), s.clone());
        s
    }

    fn existing(&self, sid: &str) -> Result<Arc<Session>, String> {
        self.sessions.lock().get(sid).cloned().ok_or_else(|| "Sesión del asistente desconocida".to_string())
    }

    fn send(self: &Arc<Self>, sid: String, text: String, images: Vec<AiImage>, context: AiContext) -> Result<(), String> {
        if !session_id_ok(&sid) {
            return Err("Identificador de sesión no válido".into());
        }
        if text.trim().is_empty() && images.is_empty() {
            return Err("Mensaje vacío".into());
        }
        if text.len() > 40_000 {
            return Err("El mensaje es demasiado largo".into());
        }
        if images.len() > 6 || images.iter().any(|i| i.data.len() > 10_000_000 || !matches!(i.media_type.as_str(), "image/jpeg" | "image/png" | "image/webp" | "image/gif")) {
            return Err("Imágenes no válidas (máx. 6, JPEG/PNG/WebP/GIF, ≤ 7 MB)".into());
        }
        if self.handle().is_none() {
            return Err("El motor no está disponible".into());
        }
        let sess = self.session(&sid);
        let (tx, rx) = watch::channel(false);
        let gen = {
            let mut st = sess.st.lock();
            if st.running {
                return Err("El asistente aún está respondiendo: espera o pulsa Detener.".into());
            }
            st.running = true;
            st.cancel = Some(tx);
            st.gen
        };
        let me = self.clone();
        tokio::spawn(agent::run_turn(me, sid, sess, agent::TurnInput { text, images, context }, rx, gen));
        Ok(())
    }

    fn cancel(&self, sid: &str) -> Result<(), String> {
        let sess = self.existing(sid)?;
        let mut st = sess.st.lock();
        if let Some(c) = &st.cancel {
            let _ = c.send(true);
        }
        st.waiting.clear();
        Ok(())
    }

    fn reply(&self, sid: &str, call: &str, r: Reply) -> Result<(), String> {
        let sess = self.existing(sid)?;
        let w = sess.st.lock().waiting.remove(call);
        match w {
            Some(tx) => {
                let _ = tx.send(r);
                Ok(())
            }
            None => Err("No hay ninguna pregunta pendiente con ese identificador (¿ha caducado?)".into()),
        }
    }

    fn reset(&self, sid: &str) -> Result<(), String> {
        let sess = self.session(sid);
        let mut st = sess.st.lock();
        if let Some(c) = &st.cancel {
            let _ = c.send(true);
        }
        st.waiting.clear();
        st.history.clear();
        st.steps = 0;
        st.gen += 1;
        Ok(())
    }

    /// Undo what the assistant changed in this session, via the show's undo history.
    fn undo(self: &Arc<Self>, sid: &str) -> Result<(), String> {
        let sess = self.existing(sid)?;
        let (n, end) = {
            let st = sess.st.lock();
            if st.running {
                return Err("Espera a que el asistente termine (o púlsale Detener) antes de deshacer.".into());
            }
            (st.steps, st.undo_end)
        };
        if n == 0 {
            return Err("El asistente no ha cambiado nada en esta sesión.".into());
        }
        let h = self.handle().ok_or("El motor no está disponible")?;
        let me = self.clone();
        let sid = sid.to_string();
        tokio::spawn(async move {
            let cur = h.ai_state().await.map(|s| s.rt.undo).unwrap_or(0);
            if cur > end {
                me.emit(&sid, AiEvent::Error { msg: "Has hecho cambios después del asistente: usa Deshacer (Ctrl+Z) paso a paso para no perder los tuyos.".into() });
                return;
            }
            let k = n.min(cur as u32);
            let mut done = 0;
            for _ in 0..k {
                if h.command(Command::Undo).await.is_err() {
                    break;
                }
                done += 1;
            }
            {
                let mut st = sess.st.lock();
                st.steps = 0;
                st.undo_end = cur.saturating_sub(done as u16);
                st.history.push(Msg::user_text(format!("[aviso de la aplicación] El usuario ha deshecho {done} cambio(s) que hiciste en el show. Vuelve a comprobar el estado con get_show antes de seguir.")));
            }
            me.emit(&sid, AiEvent::Status { text: format!("Deshechos {done} cambios del asistente (lo que se lanzó en directo no se deshace).") });
            me.emit(&sid, AiEvent::Done { input_tokens: 0, output_tokens: 0, changes: 0 });
        });
        Ok(())
    }

    fn configure(&self, c: AiConfigIn) -> Result<(), String> {
        {
            let mut cfg = self.cfg.lock();
            let mut next = cfg.clone();
            next.apply(c)?;
            next.save(&self.path)?;
            *cfg = next;
        }
        self.emit_config();
        Ok(())
    }

    /// Flip a model's `tools` flag off (the service rejected tool definitions) and persist it.
    pub fn mark_no_tools(&self, provider: &str, model: &str) {
        {
            let mut cfg = self.cfg.lock();
            if let Some(m) = cfg.providers.iter_mut().find(|p| p.id == provider).and_then(|p| p.models.iter_mut().find(|m| m.id == model)) {
                m.tools = false;
            }
            let _ = cfg.save(&self.path);
        }
        self.emit_config();
    }

    fn list_models(self: &Arc<Self>, provider: String) -> Result<(), String> {
        let t = {
            let mut cfg = self.cfg.lock().clone();
            if let Some(p) = cfg.providers.iter_mut().find(|p| p.id == provider) {
                if p.models.is_empty() {
                    p.models.push(AiModelInfo { id: "-".into(), label: "-".into(), note: String::new(), vision: false, tools: true, reasoning: false });
                }
            }
            cfg.target_for(&provider, None)?
        };
        if t.api_key.is_none() && config::needs_key(self.cfg.lock().provider(&provider).ok_or("Proveedor desconocido")?) {
            return Err("Falta la clave de API de este proveedor.".into());
        }
        let me = self.clone();
        tokio::spawn(async move {
            let started = Instant::now();
            match client::list_models(&t).await {
                Ok(fresh) => {
                    let n = fresh.len();
                    {
                        let mut cfg = me.cfg.lock();
                        if let Some(p) = cfg.providers.iter_mut().find(|p| p.id == provider) {
                            p.models = client::merge_models(&p.models, fresh);
                        }
                        let _ = cfg.save(&me.path);
                    }
                    me.emit_config();
                    me.emit_msg(&ServerMsg::AiTest { provider, ok: true, message: format!("Lista de modelos actualizada: {n} modelos."), ms: started.elapsed().as_millis() as u32 });
                }
                Err(e) => me.emit_msg(&ServerMsg::AiTest { provider, ok: false, message: format!("No se pudo obtener la lista de modelos: {}", client::user_message(&e)), ms: started.elapsed().as_millis() as u32 }),
            }
        });
        Ok(())
    }

    fn test_provider(self: &Arc<Self>, provider: String) -> Result<(), String> {
        let t = {
            let cfg = self.cfg.lock();
            // test the active model when this is the active provider
            let model = if cfg.active_provider == provider { Some(cfg.model.clone()) } else { None };
            cfg.target_for(&provider, model.as_deref())?
        };
        if t.api_key.is_none() && config::needs_key(self.cfg.lock().provider(&provider).ok_or("Proveedor desconocido")?) {
            return Err("Falta la clave de API de este proveedor.".into());
        }
        let me = self.clone();
        tokio::spawn(async move {
            let started = Instant::now();
            let r = client::test(&t).await;
            let ms = started.elapsed().as_millis() as u32;
            let msg = match r {
                Ok(()) => ServerMsg::AiTest { provider, ok: true, message: format!("Conexión correcta con {} ({}).", t.model.label, t.model.id), ms },
                Err(e) => ServerMsg::AiTest { provider, ok: false, message: client::user_message(&e), ms },
            };
            me.emit_msg(&msg);
        });
        Ok(())
    }

    /// Wait for the user's answer to a confirmation / question. `None` = timeout or cancelled.
    pub async fn wait_reply(&self, sess: &Arc<Session>, call: &str, cancel: &mut Cancel) -> Option<Reply> {
        let (tx, rx) = oneshot::channel();
        sess.st.lock().waiting.insert(call.to_string(), tx);
        let r = tokio::select! {
            r = rx => r.ok(),
            _ = tokio::time::sleep(ASK_TIMEOUT) => None,
            _ = wait_cancel(cancel) => None,
        };
        sess.st.lock().waiting.remove(call);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids() {
        assert!(session_id_ok("main"));
        assert!(session_id_ok("a-b_c.d:1"));
        assert!(!session_id_ok(""));
        assert!(!session_id_ok("a b"));
        assert!(!session_id_ok(&"x".repeat(65)));
    }
}
