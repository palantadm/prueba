//! Microphone → 16 kHz mono → energy VAD → whisper.cpp. Runs on its own threads; talks to the engine through
//! [`IoEvent`]s. Without the `whisper` feature everything here is a stub.

use crate::io::IoEvent;
use crate::model::SpeechCfg;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// Whether this build can recognise speech locally.
pub const AVAILABLE: bool = cfg!(feature = "whisper");

/// Utterances shorter than this are ignored (clicks, breaths).
const MIN_SPEECH_S: f32 = 0.35;
/// Silence that closes an utterance.
const END_SILENCE_S: f32 = 0.7;
/// Re-run recognition on the growing utterance this often (live partial text).
const PARTIAL_EVERY_S: f32 = 1.1;
/// Hard cap of one utterance (whisper works on ≤ 30 s; shorter keeps latency low).
const MAX_UTTERANCE_S: f32 = 12.0;
const RATE: u32 = 16_000;

pub struct Worker {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Worker {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Names of the audio input devices.
pub fn list_devices() -> Vec<String> {
    #[cfg(feature = "whisper")]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        return host.input_devices().map(|d| d.filter_map(|d| d.description().ok().map(|x| x.name().to_string())).collect()).unwrap_or_default();
    }
    #[cfg(not(feature = "whisper"))]
    Vec::new()
}

/// Texts Whisper invents on silence / noise; never useful for following a script.
pub fn is_hallucination(t: &str) -> bool {
    let n = super::matcher::norm(t);
    if n.is_empty() {
        return true;
    }
    let t = t.trim();
    if (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')')) || (t.starts_with('♪') || t.ends_with('♪')) {
        return true;
    }
    const JUNK: &[&str] = &["subtitulos realizados por", "subtitulos por", "amara org", "gracias por ver", "suscribete", "thanks for watching", "subtitulado por", "legendas pela comunidade", "obrigado por assistir"];
    JUNK.iter().any(|j| n.contains(j))
}

/// Linear-interpolating resampler to 16 kHz that keeps its phase between callbacks.
pub struct Resampler {
    step: f64,
    pos: f64,
    last: f32,
}

impl Resampler {
    pub fn new(from_rate: u32) -> Self {
        Resampler { step: from_rate as f64 / RATE as f64, pos: 0.0, last: 0.0 }
    }
    /// `mono` at the source rate → appends 16 kHz samples to `out`.
    pub fn push(&mut self, mono: &[f32], out: &mut Vec<f32>) {
        if mono.is_empty() {
            return;
        }
        // position 0 is the previous call's last sample; mono[i] sits at i+1
        let n = mono.len() as f64;
        while self.pos < n {
            let i = self.pos.floor();
            let frac = (self.pos - i) as f32;
            let a = if i < 0.5 && self.pos < 1.0 && i == 0.0 { self.last } else { mono[(i as usize).saturating_sub(1)] };
            let b = mono[(i as usize).min(mono.len() - 1)];
            out.push(a + (b - a) * frac);
            self.pos += self.step;
        }
        self.pos -= n;
        self.last = *mono.last().unwrap();
    }
}

/// Energy-based voice activity: adaptive noise floor, hysteresis, utterance assembly.
pub struct Segmenter {
    pub floor: f32,
    speaking: bool,
    quiet_s: f32,
    voiced_s: f32,
    pub buf: Vec<f32>,
    since_partial_s: f32,
}

pub enum Cut {
    /// Nothing to do.
    None,
    /// Utterance is still going: transcribe `buf` as a partial.
    Partial,
    /// Utterance ended: transcribe `buf`, then call `reset`.
    Final,
}

impl Segmenter {
    pub fn new() -> Self {
        Segmenter { floor: 0.003, speaking: false, quiet_s: 0.0, voiced_s: 0.0, buf: Vec::new(), since_partial_s: 0.0 }
    }

    pub fn reset(&mut self, keep_tail: usize) {
        let n = self.buf.len();
        if keep_tail > 0 && n > keep_tail {
            self.buf.drain(..n - keep_tail);
        } else if keep_tail == 0 {
            self.buf.clear();
        }
        self.speaking = false;
        self.quiet_s = 0.0;
        self.voiced_s = 0.0;
        self.since_partial_s = 0.0;
    }

    pub fn speech_seconds(&self) -> f32 {
        self.voiced_s
    }

    /// Feed 16 kHz samples; returns what to do with the buffer now.
    pub fn feed(&mut self, samples: &[f32]) -> Cut {
        // 20 ms frames
        for fr in samples.chunks(320) {
            let dt = fr.len() as f32 / RATE as f32;
            let rms = (fr.iter().map(|x| x * x).sum::<f32>() / fr.len().max(1) as f32).sqrt();
            let thr = (self.floor * 3.5).max(0.012);
            let loud = rms > if self.speaking { thr * 0.7 } else { thr };
            if !loud {
                // follow the ambient level slowly (never up while someone speaks)
                self.floor = self.floor * 0.995 + rms * 0.005;
            }
            if loud {
                self.speaking = true;
                self.quiet_s = 0.0;
                self.voiced_s += dt;
            } else if self.speaking {
                self.quiet_s += dt;
            }
            if self.speaking || !self.buf.is_empty() {
                self.buf.extend_from_slice(fr);
            }
            if !self.speaking {
                // keep a short pre-roll so the first syllable is not cut
                let keep = (RATE as f32 * 0.25) as usize;
                if self.buf.len() > keep {
                    let n = self.buf.len() - keep;
                    self.buf.drain(..n);
                }
                continue;
            }
            self.since_partial_s += dt;
        }
        let secs = self.buf.len() as f32 / RATE as f32;
        if self.speaking && (self.quiet_s >= END_SILENCE_S || secs >= MAX_UTTERANCE_S) {
            return if self.voiced_s >= MIN_SPEECH_S { Cut::Final } else {
                self.reset(0);
                Cut::None
            };
        }
        if self.speaking && self.voiced_s >= MIN_SPEECH_S && self.since_partial_s >= PARTIAL_EVERY_S {
            self.since_partial_s = 0.0;
            return Cut::Partial;
        }
        Cut::None
    }
}

#[cfg(feature = "whisper")]
mod real {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use parking_lot::Mutex;
    use std::collections::VecDeque;
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    pub struct Recognizer {
        ctx: WhisperContext,
        lang: String,
        threads: i32,
    }

    impl Recognizer {
        pub fn load(model: &std::path::Path, lang: &str) -> Result<Self, String> {
            let ctx = WhisperContext::new_with_params(model.to_str().ok_or("Ruta de modelo no válida")?, WhisperContextParameters::default()).map_err(|e| format!("No se pudo cargar el modelo: {e}"))?;
            let threads = std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(2).clamp(1, 6);
            Ok(Recognizer { ctx, lang: lang.to_string(), threads })
        }

        /// Transcribe 16 kHz mono samples. `prompt` biases the vocabulary (the next words of the script).
        pub fn transcribe(&self, audio: &[f32], prompt: &str) -> Result<String, String> {
            let mut state = self.ctx.create_state().map_err(|e| e.to_string())?;
            let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            p.set_language(Some(if self.lang.is_empty() { "es" } else { &self.lang }));
            p.set_translate(false);
            p.set_no_context(true);
            p.set_single_segment(true);
            p.set_print_special(false);
            p.set_print_progress(false);
            p.set_print_realtime(false);
            p.set_print_timestamps(false);
            p.set_suppress_blank(true);
            p.set_suppress_nst(true);
            p.set_temperature_inc(0.0);
            p.set_no_speech_thold(0.6);
            p.set_n_threads(self.threads);
            if !prompt.trim().is_empty() {
                p.set_initial_prompt(prompt);
            }
            // whisper needs ≥ 1 s of audio; pad short utterances with silence
            let mut padded;
            let data: &[f32] = if audio.len() < RATE as usize + 1600 {
                padded = audio.to_vec();
                padded.resize(RATE as usize + 1600, 0.0);
                &padded
            } else {
                audio
            };
            state.full(p, data).map_err(|e| format!("Error de reconocimiento: {e}"))?;
            let mut out = String::new();
            for seg in state.as_iter() {
                out.push_str(&seg.to_string());
                out.push(' ');
            }
            Ok(out.trim().to_string())
        }
    }

    pub fn start(model: PathBuf, cfg: SpeechCfg, tx: UnboundedSender<IoEvent>, level: Arc<AtomicU32>, prompt: Arc<Mutex<String>>) -> Result<Worker, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let handle = std::thread::Builder::new()
            .name("lumos-speech".into())
            .spawn(move || {
                let fail = |msg: String| {
                    let _ = tx.send(IoEvent::SpeechStatus { listening: false, error: msg });
                };
                let rec = match Recognizer::load(&model, &cfg.lang) {
                    Ok(r) => r,
                    Err(e) => return fail(e),
                };
                let host = cpal::default_host();
                let dev = if cfg.device.trim().is_empty() {
                    host.default_input_device()
                } else {
                    host.input_devices().ok().and_then(|mut d| d.find(|d| d.description().map(|x| x.name() == cfg.device).unwrap_or(false))).or_else(|| host.default_input_device())
                };
                let Some(dev) = dev else { return fail("No hay micrófono disponible.".into()) };
                let conf = match dev.default_input_config() {
                    Ok(c) => c,
                    Err(e) => return fail(format!("El micrófono no se puede abrir: {e}")),
                };
                let (channels, rate, fmt) = (conf.channels() as usize, conf.sample_rate(), conf.sample_format());
                let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
                let q = queue.clone();
                let lv = level.clone();
                let mut rs = Resampler::new(rate);
                let mut push = move |mono: Vec<f32>| {
                    let rms = (mono.iter().map(|x| x * x).sum::<f32>() / mono.len().max(1) as f32).sqrt();
                    lv.store((rms * 4.0).min(1.0).to_bits(), Ordering::Relaxed);
                    let mut out = Vec::with_capacity(mono.len() / 2 + 8);
                    rs.push(&mono, &mut out);
                    let mut g = q.lock();
                    g.extend(out);
                    // never let the queue grow beyond 60 s if the recogniser falls behind
                    while g.len() > RATE as usize * 60 {
                        g.pop_front();
                    }
                };
                let err = |e| tracing::warn!("audio input: {e}");
                let sc: cpal::StreamConfig = conf.clone().into();
                macro_rules! build {
                    ($t:ty, $conv:expr) => {
                        dev.build_input_stream(&sc, move |d: &[$t], _| push(d.chunks(channels.max(1)).map(|f| f.iter().map($conv).sum::<f32>() / f.len() as f32).collect()), err, None)
                    };
                }
                let stream = match fmt {
                    cpal::SampleFormat::F32 => build!(f32, |x: &f32| *x),
                    cpal::SampleFormat::I16 => build!(i16, |x: &i16| *x as f32 / 32768.0),
                    cpal::SampleFormat::U16 => build!(u16, |x: &u16| (*x as f32 - 32768.0) / 32768.0),
                    cpal::SampleFormat::I32 => build!(i32, |x: &i32| *x as f32 / 2147483648.0),
                    other => return fail(format!("Formato de audio no admitido: {other:?}")),
                };
                let stream = match stream {
                    Ok(s) => s,
                    Err(e) => return fail(format!("No se pudo abrir el micrófono: {e}")),
                };
                if let Err(e) = stream.play() {
                    return fail(format!("No se pudo iniciar el micrófono: {e}"));
                }
                let _ = tx.send(IoEvent::SpeechStatus { listening: true, error: String::new() });
                let mut seg = Segmenter::new();
                while !stop2.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(60));
                    let fresh: Vec<f32> = queue.lock().drain(..).collect();
                    if fresh.is_empty() {
                        continue;
                    }
                    let cut = seg.feed(&fresh);
                    let (partial, fin) = match cut {
                        Cut::None => continue,
                        Cut::Partial => (true, false),
                        Cut::Final => (false, true),
                    };
                    let p = prompt.lock().clone();
                    match rec.transcribe(&seg.buf, &p) {
                        Ok(text) if !is_hallucination(&text) => {
                            let _ = tx.send(IoEvent::Speech { text, partial });
                        }
                        Ok(_) => {}
                        Err(e) => {
                            let _ = tx.send(IoEvent::SpeechStatus { listening: true, error: e });
                        }
                    }
                    if fin {
                        seg.reset(0);
                    }
                }
                drop(stream);
                let _ = tx.send(IoEvent::SpeechStatus { listening: false, error: String::new() });
            })
            .map_err(|e| e.to_string())?;
        Ok(Worker { stop, handle: Some(handle) })
    }
}

#[cfg(feature = "whisper")]
pub use real::{start, Recognizer};

#[cfg(not(feature = "whisper"))]
pub fn start(_model: PathBuf, _cfg: SpeechCfg, _tx: UnboundedSender<IoEvent>, _level: Arc<AtomicU32>, _prompt: Arc<parking_lot::Mutex<String>>) -> Result<Worker, String> {
    Err("Esta compilación de Lumos no incluye el reconocimiento de voz local.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hallucinations_are_filtered() {
        for t in ["", "  ", "[Música]", "(aplausos)", "♪ ♪", "Subtítulos realizados por la comunidad de Amara.org", "Thanks for watching!"] {
            assert!(is_hallucination(t), "{t}");
        }
        for t in ["Se apagan las luces", "Onte pola noite chovía"] {
            assert!(!is_hallucination(t), "{t}");
        }
    }

    #[test]
    fn resampler_halves_48k_to_16k_and_keeps_phase() {
        let mut r = Resampler::new(48_000);
        let ramp: Vec<f32> = (0..4800).map(|i| i as f32 / 4800.0).collect();
        let mut out = Vec::new();
        for chunk in ramp.chunks(480) {
            r.push(chunk, &mut out);
        }
        assert!((out.len() as i32 - 1600).abs() <= 2, "{}", out.len());
        assert!(out.windows(2).all(|w| w[1] >= w[0] - 1e-4), "monotonic ramp stays monotonic across chunks");
        assert!((out[800] - 0.5).abs() < 0.01, "{}", out[800]);
    }

    fn tone(secs: f32, amp: f32) -> Vec<f32> {
        (0..(secs * RATE as f32) as usize).map(|i| amp * (i as f32 * 0.3).sin()).collect()
    }

    #[test]
    fn segmenter_cuts_an_utterance_after_silence() {
        let mut s = Segmenter::new();
        let mut cuts = Vec::new();
        // 0.5 s room noise, 2.0 s speech, 1.0 s silence
        for chunk in tone(0.5, 0.001).chunks(1600).chain(tone(2.0, 0.2).chunks(1600)).chain(vec![0.0f32; RATE as usize].chunks(1600)) {
            match s.feed(chunk) {
                Cut::None => {}
                Cut::Partial => cuts.push("p"),
                Cut::Final => {
                    cuts.push("f");
                    assert!(s.buf.len() as f32 / RATE as f32 > 2.0, "utterance kept ({} samples)", s.buf.len());
                    s.reset(0);
                }
            }
        }
        assert!(cuts.contains(&"p"), "live partial while speaking: {cuts:?}");
        assert_eq!(cuts.iter().filter(|c| **c == "f").count(), 1, "{cuts:?}");
    }

    /// Real recognition end to end (needs a ggml model and a 16 kHz mono 16-bit wav):
    /// `LUMOS_WHISPER_MODEL=ggml-tiny.bin LUMOS_WHISPER_WAV=jfk.wav cargo test --features whisper -- --ignored whisper_real`
    #[cfg(feature = "whisper")]
    #[test]
    #[ignore]
    fn whisper_real_utterance_through_the_segmenter() {
        let (Ok(model), Ok(wav)) = (std::env::var("LUMOS_WHISPER_MODEL"), std::env::var("LUMOS_WHISPER_WAV")) else {
            eprintln!("skipped: set LUMOS_WHISPER_MODEL and LUMOS_WHISPER_WAV");
            return;
        };
        let bytes = std::fs::read(wav).unwrap();
        let mut audio: Vec<f32> = bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
        audio.extend(vec![0.0f32; RATE as usize * 2]);
        let lang = std::env::var("LUMOS_WHISPER_LANG").unwrap_or_else(|_| "en".into());
        let rec = real::Recognizer::load(std::path::Path::new(&model), &lang).unwrap();
        let mut seg = Segmenter::new();
        let (mut partials, mut finals) = (Vec::<String>::new(), Vec::<String>::new());
        for chunk in audio.chunks(960) {
            match seg.feed(chunk) {
                Cut::None => {}
                Cut::Partial => partials.push(rec.transcribe(&seg.buf, "").unwrap()),
                Cut::Final => {
                    finals.push(rec.transcribe(&seg.buf, "").unwrap());
                    seg.reset(0);
                }
            }
        }
        eprintln!("partials: {partials:?}\nfinals: {finals:?}");
        let all = super::super::matcher::norm(&finals.join(" "));
        assert!(all.contains("ask not what your country can do for you"), "{all}");
        assert!(!partials.is_empty(), "live partial text while speaking");
    }

    #[test]
    fn segmenter_ignores_clicks_and_steady_noise() {
        let mut s = Segmenter::new();
        let mut any = false;
        for chunk in tone(3.0, 0.002).chunks(1600) {
            any |= !matches!(s.feed(chunk), Cut::None);
        }
        let click = tone(0.1, 0.3);
        for chunk in click.chunks(1600).chain(vec![0.0f32; RATE as usize * 2].chunks(1600)) {
            any |= matches!(s.feed(chunk), Cut::Final);
        }
        assert!(!any, "no utterance expected");
    }
}
