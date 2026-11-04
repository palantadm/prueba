//! Voice-follow of the Guion with local speech recognition (Whisper).
//!
//! The engine owns the microphone worker, so following keeps working with the UI closed. Recognised words arrive as
//! `IoEvent::Speech`; they are matched against the words that come next in the script ([`crate::speech::matcher`]) and
//! move the reader. Cue marks are fired only when `fire_cues` is on **and** the phrase is final (a live partial may
//! still be corrected, a fired cue cannot).

use super::*;
use crate::speech::{matcher, models, worker};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

const FILE: &str = "speech.json";
const HISTORY_WORDS: usize = 12;

pub struct Voice {
    pub cfg: SpeechCfg,
    worker: Option<worker::Worker>,
    listening: bool,
    heard: String,
    error: String,
    /// Words of the last final phrases (context for matching across utterances).
    history: Vec<String>,
    dl: Option<(String, Arc<AtomicU32>)>,
    level: Arc<AtomicU32>,
    prompt: Arc<Mutex<String>>,
    installed: Vec<bool>,
}

impl Voice {
    pub fn load(store: &Store) -> Self {
        let cfg = std::fs::read_to_string(store.dir.join(FILE)).ok().and_then(|t| serde_json::from_str::<SpeechCfg>(&t).ok()).unwrap_or_default();
        let mut v = Voice {
            cfg,
            worker: None,
            listening: false,
            heard: String::new(),
            error: String::new(),
            history: Vec::new(),
            dl: None,
            level: Arc::new(AtomicU32::new(0)),
            prompt: Arc::new(Mutex::new(String::new())),
            installed: Vec::new(),
        };
        v.refresh_installed(&store.dir);
        v
    }

    fn refresh_installed(&mut self, data: &std::path::Path) {
        self.installed = models::MODELS.iter().map(|m| models::installed(data, m.id)).collect();
    }

    fn save(&self, store: &Store) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.cfg).map_err(|e| e.to_string())?;
        std::fs::write(store.dir.join(FILE), text).map_err(|e| e.to_string())
    }
}

fn valid_lang(l: &str) -> bool {
    (2..=8).contains(&l.len()) && l.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
}

impl Engine {
    pub(super) fn voice_command(&mut self, cmd: Command) -> Result<(), String> {
        match cmd {
            Command::SpeechConfigure { mut cfg } => {
                cfg.lang = cfg.lang.trim().to_lowercase();
                cfg.custom_model = cfg.custom_model.trim().to_string();
                if !valid_lang(&cfg.lang) {
                    return Err("Idioma no válido (código de 2–3 letras: es, gl, ca, en…)".into());
                }
                if models::find(&cfg.model).is_none() && cfg.custom_model.is_empty() {
                    return Err(format!("Modelo «{}» desconocido", cfg.model));
                }
                let restart = self.voice.listening && (self.voice.cfg.lang != cfg.lang || self.voice.cfg.model != cfg.model || self.voice.cfg.custom_model != cfg.custom_model);
                self.voice.cfg = cfg;
                self.voice.save(&self.store)?;
                self.dirty = true;
                if restart {
                    self.voice_stop();
                    self.voice_start()?;
                }
                Ok(())
            }
            Command::SpeechListen { on: true } => self.voice_start(),
            Command::SpeechListen { on: false } => {
                self.voice_stop();
                Ok(())
            }
            Command::SpeechModelGet { id } => {
                if models::find(&id).is_none() {
                    return Err(format!("Modelo «{id}» desconocido"));
                }
                if self.voice.dl.is_some() {
                    return Err("Ya hay una descarga en curso".into());
                }
                if models::installed(&self.store.dir, &id) {
                    return Ok(());
                }
                let Ok(rt) = tokio::runtime::Handle::try_current() else { return Err("Motor sin runtime asíncrono".into()) };
                let progress = Arc::new(AtomicU32::new(0));
                self.voice.dl = Some((id.clone(), progress.clone()));
                self.voice.error.clear();
                let (dir, tx) = (self.store.dir.clone(), self.io.sender());
                rt.spawn(async move {
                    let r = models::download(&dir, &id, models::BASE_URL, progress).await;
                    let _ = tx.send(IoEvent::SpeechDownloaded { id, error: r.err().unwrap_or_default() });
                });
                self.dirty = true;
                Ok(())
            }
            Command::SpeechModelRemove { id } => {
                if self.voice.listening && self.voice.cfg.model == id && self.voice.cfg.custom_model.is_empty() {
                    return Err("Ese modelo se está usando: para primero la escucha".into());
                }
                models::remove(&self.store.dir, &id)?;
                self.voice.refresh_installed(&self.store.dir);
                self.dirty = true;
                Ok(())
            }
            _ => Err("Comando de voz desconocido".into()),
        }
    }

    fn voice_script_id(&self) -> Option<u32> {
        let want = self.voice.cfg.script;
        self.show.scripts.iter().find(|s| want == 0 || s.id == want).or(self.show.scripts.first()).map(|s| s.id)
    }

    fn voice_start(&mut self) -> Result<(), String> {
        if self.voice.worker.is_some() {
            return Ok(());
        }
        if !worker::AVAILABLE {
            return Err("Esta compilación de Lumos no incluye el reconocimiento de voz local.".into());
        }
        if self.voice_script_id().is_none() {
            return Err("No hay ningún guion que seguir: crea o importa uno en el espacio Guion.".into());
        }
        let path = models::resolve(&self.store.dir, &self.voice.cfg.model, &self.voice.cfg.custom_model)?;
        self.voice.error.clear();
        self.voice.history.clear();
        self.voice_update_prompt();
        let feed = self.audio_voice_feed()?;
        let w = match worker::start(path, self.voice.cfg.clone(), self.io.sender(), self.voice.level.clone(), self.voice.prompt.clone(), feed) {
            Ok(w) => w,
            Err(e) => {
                self.audio_voice_release();
                return Err(e);
            }
        };
        self.voice.worker = Some(w);
        self.dirty = true;
        Ok(())
    }

    fn voice_stop(&mut self) {
        if let Some(mut w) = self.voice.worker.take() {
            // joining waits for a transcription in flight (up to a few seconds): never block the engine on it
            std::thread::spawn(move || w.stop());
        }
        self.voice.listening = false;
        self.voice.level.store(0, Ordering::Relaxed);
        self.audio_voice_release();
        self.dirty = true;
    }

    /// The next words of the script bias the recogniser towards the right vocabulary (names, rare words).
    fn voice_update_prompt(&mut self) {
        let Some(id) = self.voice_script_id() else { return };
        let Some(sc) = self.show.scripts.iter().find(|s| s.id == id) else { return };
        let from = self.script.runs.get(&id).and_then(|r| sc.blocks.iter().position(|b| b.id == r.block)).unwrap_or(0);
        let mut out = String::new();
        'outer: for b in sc.blocks.iter().skip(from) {
            if matches!(b.kind, ScriptKind::Direction | ScriptKind::Note) {
                continue;
            }
            for w in b.text.split_whitespace() {
                if out.len() + w.len() > 220 {
                    break 'outer;
                }
                out.push_str(w);
                out.push(' ');
            }
        }
        *self.voice.prompt.lock() = out.trim().to_string();
    }

    pub(super) fn on_voice_event(&mut self, ev: IoEvent) {
        match ev {
            IoEvent::Speech { text, partial } => self.on_speech(&text, partial),
            IoEvent::SpeechStatus { listening, error } => {
                self.voice.listening = listening;
                self.voice.error = error;
                if !listening {
                    if let Some(w) = self.voice.worker.take() {
                        std::thread::spawn(move || drop(w));
                    }
                    self.voice.level.store(0, Ordering::Relaxed);
                }
                self.dirty = true;
            }
            IoEvent::SpeechDownloaded { id, error } => {
                self.voice.dl = None;
                self.voice.refresh_installed(&self.store.dir);
                if !error.is_empty() {
                    self.voice.error = format!("Modelo «{id}»: {error}");
                }
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn on_speech(&mut self, text: &str, partial: bool) {
        let words = matcher::words_of(text);
        if words.is_empty() {
            return;
        }
        let mut ctx: Vec<String> = self.voice.history.clone();
        ctx.extend(words.iter().cloned());
        let shown: Vec<&String> = ctx.iter().rev().take(HISTORY_WORDS).collect::<Vec<_>>().into_iter().rev().collect();
        self.voice.heard = shown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ");
        if let Some(id) = self.voice_script_id() {
            let fire = self.voice.cfg.fire_cues && !partial;
            match self.script_move_by_words(id, &ctx, fire) {
                Ok(true) => {
                    // consumed: start fresh so the same words cannot move the reader twice
                    self.voice.history.clear();
                    self.voice_update_prompt();
                    self.voice.heard = words.join(" ");
                    self.dirty = true;
                    return;
                }
                Ok(false) => {}
                Err(e) => self.voice.error = e,
            }
        }
        if !partial {
            self.voice.history = ctx.into_iter().rev().take(HISTORY_WORDS).collect::<Vec<_>>().into_iter().rev().collect();
        }
        self.dirty = true;
    }

    pub(super) fn voice_rt(&self) -> SpeechRt {
        let devices: Vec<String> = Vec::new(); // the microphone is chosen in Settings → Audio
        let model_ready = if self.voice.cfg.custom_model.is_empty() {
            self.voice.installed.get(models::MODELS.iter().position(|m| m.id == self.voice.cfg.model).unwrap_or(usize::MAX)).copied().unwrap_or(false)
        } else {
            std::path::Path::new(&self.voice.cfg.custom_model).is_file()
        };
        let lv = f32::from_bits(self.voice.level.load(Ordering::Relaxed));
        SpeechRt {
            available: worker::AVAILABLE,
            listening: self.voice.listening,
            cfg: self.voice.cfg.clone(),
            models: models::MODELS
                .iter()
                .enumerate()
                .map(|(i, m)| SpeechModelInfo { id: m.id.into(), label: m.label.into(), size_mb: (m.bytes / 1_000_000) as u32, installed: self.voice.installed.get(i).copied().unwrap_or(false) })
                .collect(),
            devices,
            model_ready,
            download: self.voice.dl.as_ref().map(|(id, p)| (id.clone(), p.load(Ordering::Relaxed) as f32 / 1000.0)),
            heard: self.voice.heard.clone(),
            // 5 % steps keep the runtime from changing on every audio callback
            level: if self.voice.listening { (lv * 20.0).round() / 20.0 } else { 0.0 },
            error: self.voice.error.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::script::tests::{block, mark};
    use super::super::tests_support::*;
    use super::*;

    fn obra(e: &mut Engine) {
        let blocks = vec![
            block(0, "Buenas noches a todos los presentes", vec![]),
            block(0, "Perdón por llegar tarde a la reunión", vec![mark(0, 0, vec![ScriptAction::Console { text: "blackout on".into() }])]),
            block(0, "No pasa nada siéntate con nosotros", vec![mark(0, 3, vec![ScriptAction::Console { text: "blackout off".into() }])]),
        ];
        e.handle(Command::ScriptSave { script: Script { id: 0, name: "Obra".into(), blocks, pdf: None } }).unwrap();
    }

    fn say(e: &mut Engine, text: &str, partial: bool) {
        e.on_io(IoEvent::Speech { text: text.into(), partial });
    }

    fn reader_block(e: &Engine) -> u32 {
        e.script.runs.get(&1).map_or(0, |r| r.block)
    }

    #[test]
    fn voice_moves_the_reader_forward_without_firing() {
        let mut e = engine();
        obra(&mut e);
        say(&mut e, "a todos los presentes", true);
        assert_eq!(reader_block(&e), 1);
        say(&mut e, "por llegar tarde a la", false);
        assert_eq!(reader_block(&e), 2);
        assert!(!e.blackout, "cues stay untouched while fire_cues is off");
        // going back in the text never rewinds the reader
        say(&mut e, "buenas noches a todos los", false);
        assert_eq!(reader_block(&e), 2);
        // a phrase split across two utterances still matches thanks to the history
        say(&mut e, "no pasa", false);
        assert_eq!(reader_block(&e), 2);
        say(&mut e, "nada siéntate con", false);
        assert_eq!(reader_block(&e), 3);
    }

    #[test]
    fn voice_fires_cues_only_on_final_phrases_when_enabled() {
        let mut e = engine();
        obra(&mut e);
        e.handle(Command::SpeechConfigure { cfg: SpeechCfg { fire_cues: true, ..SpeechCfg::default() } }).unwrap();
        // the partial may still change its mind: reader moves, nothing fires
        say(&mut e, "por llegar tarde a la reunion", true);
        assert_eq!(reader_block(&e), 2);
        assert!(!e.blackout);
        // fresh reader, final phrase: the mark of the spoken block fires
        e.handle(Command::ScriptReset { id: 1 }).unwrap();
        say(&mut e, "por llegar tarde a la reunion", false);
        assert!(e.blackout, "mark 1 fired on the final phrase");
    }

    #[test]
    fn voice_status_download_and_errors_reach_the_runtime() {
        let mut e = engine();
        assert_eq!(e.voice_rt().available, worker::AVAILABLE);
        assert!(e.voice_rt().models.iter().any(|m| m.id == "small-q5_1" && !m.installed));
        // listening needs a script, then a downloaded model
        assert!(e.handle(Command::SpeechListen { on: true }).unwrap_err().contains("guion") || !worker::AVAILABLE);
        obra(&mut e);
        let err = e.handle(Command::SpeechListen { on: true }).unwrap_err();
        assert!(err.contains("no está descargado") || err.contains("no incluye"), "{err}");
        e.on_io(IoEvent::SpeechStatus { listening: false, error: "No hay micrófono disponible.".into() });
        assert_eq!(e.voice_rt().error, "No hay micrófono disponible.");
        e.on_io(IoEvent::SpeechDownloaded { id: "tiny".into(), error: "HTTP 404".into() });
        assert!(e.voice_rt().error.contains("tiny") && e.voice_rt().download.is_none());
        // validation
        assert!(e.handle(Command::SpeechConfigure { cfg: SpeechCfg { lang: "español!".into(), ..Default::default() } }).is_err());
        assert!(e.handle(Command::SpeechConfigure { cfg: SpeechCfg { model: "nope".into(), ..Default::default() } }).is_err());
        assert!(e.handle(Command::SpeechModelGet { id: "nope".into() }).is_err());
        // preferences persist
        e.handle(Command::SpeechConfigure { cfg: SpeechCfg { lang: "GL".into(), fire_cues: true, ..Default::default() } }).unwrap();
        let again = Voice::load(&e.store);
        assert_eq!((again.cfg.lang.as_str(), again.cfg.fire_cues), ("gl", true));
    }

    #[test]
    fn prompt_carries_the_next_words_of_the_script() {
        let mut e = engine();
        obra(&mut e);
        e.voice_update_prompt();
        assert!(e.voice.prompt.lock().starts_with("Buenas noches a todos"));
        say(&mut e, "por llegar tarde a la", true);
        assert!(e.voice.prompt.lock().starts_with("Perdón por llegar"), "{}", e.voice.prompt.lock());
    }
}
