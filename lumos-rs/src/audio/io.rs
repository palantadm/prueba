//! Device layer: opens the chosen input / output devices (cpal), runs the input analysis thread (levels, reactive bands,
//! LTC decode, voice feed) and fills the output through the [`Mixer`]. The state both sides share lives in [`Shared`],
//! which is always compiled so the engine code and its tests do not need a sound card.

use super::dsp::{Analyzer, Bands};
use super::ltc::{Decoder, Tc};
use super::mixer::{Mixer, Sound};
use crate::io::IoEvent;
use crate::model::*;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

pub const VOICE_RATE: u32 = 16_000;
/// Decoded sounds kept in memory (bytes of f32 samples).
const CACHE_BYTES: usize = 256 * 1024 * 1024;
/// A single file longer than this is refused.
pub const MAX_SOUND_S: f32 = 20.0 * 60.0;

#[derive(Clone, Debug, Default)]
pub struct Info {
    pub in_channels: u16,
    pub out_channels: u16,
    pub in_rate: u32,
    pub out_rate: u32,
    pub input_running: bool,
    pub output_running: bool,
    pub in_device: String,
    pub out_device: String,
    pub error: String,
}

pub struct Shared {
    pub mixer: Mutex<Mixer>,
    pub info: Mutex<Info>,
    pub in_peaks: Mutex<Vec<f32>>,
    pub reactive: Mutex<[f32; 4]>,
    pub bpm: Mutex<f32>,
    /// Last LTC frame seen and when.
    pub ltc: Mutex<Option<(Tc, Instant)>>,
    pub voice_tap: Mutex<VecDeque<f32>>,
    pub voice_wanted: AtomicBool,
    pub ltc_fps: AtomicU32,
    cache: Mutex<HashMap<String, Arc<Sound>>>,
}

impl Shared {
    pub fn new(out_rate: u32, out_channels: usize) -> Arc<Self> {
        Arc::new(Shared {
            mixer: Mutex::new(Mixer::new(out_rate.max(8000), out_channels.max(1))),
            info: Mutex::new(Info { out_rate, out_channels: out_channels as u16, ..Default::default() }),
            in_peaks: Mutex::new(Vec::new()),
            reactive: Mutex::new([0.0; 4]),
            bpm: Mutex::new(0.0),
            ltc: Mutex::new(None),
            voice_tap: Mutex::new(VecDeque::new()),
            voice_wanted: AtomicBool::new(false),
            ltc_fps: AtomicU32::new(25),
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// A sound from the media folder, decoded to the output rate (cached).
    pub fn sound(&self, media: &Path, file: &str) -> Result<Arc<Sound>, String> {
        let rate = self.mixer.lock().rate;
        let key = format!("{file}@{rate}");
        if let Some(s) = self.cache.lock().get(&key) {
            return Ok(s.clone());
        }
        if file.contains(['/', '\\']) || file.starts_with('.') {
            return Err("Nombre de sonido no válido".into());
        }
        let path = media.join(file);
        if !path.is_file() {
            return Err(format!("No existe el sonido «{file}» en la carpeta de medios."));
        }
        let (data, channels) = decode_file(&path, rate)?;
        let snd = Arc::new(Sound { file: file.to_string(), channels, data });
        let mut c = self.cache.lock();
        let mut total: usize = c.values().map(|s| s.data.len() * 4).sum::<usize>() + snd.data.len() * 4;
        while total > CACHE_BYTES && !c.is_empty() {
            let k = c.keys().next().cloned().unwrap();
            if let Some(old) = c.remove(&k) {
                total -= old.data.len() * 4;
            }
        }
        c.insert(key, snd.clone());
        Ok(snd)
    }

    pub fn forget_sounds(&self) {
        self.cache.lock().clear();
    }
}

/// Linear resample of interleaved audio.
pub fn resample(data: &[f32], channels: usize, from: u32, to: u32) -> Vec<f32> {
    if from == to || data.is_empty() || channels == 0 {
        return data.to_vec();
    }
    let frames = data.len() / channels;
    let out_frames = ((frames as u64 * to as u64) / from as u64) as usize;
    let mut out = Vec::with_capacity(out_frames * channels);
    for i in 0..out_frames {
        let pos = i as f64 * from as f64 / to as f64;
        let a = pos.floor() as usize;
        let b = (a + 1).min(frames - 1);
        let t = (pos - a as f64) as f32;
        for c in 0..channels {
            let x = data[a.min(frames - 1) * channels + c];
            let y = data[b * channels + c];
            out.push(x + (y - x) * t);
        }
    }
    out
}

#[cfg(feature = "audio")]
fn decode_file(path: &Path, target_rate: u32) -> Result<(Vec<f32>, u8), String> {
    device::decode(path, target_rate)
}

#[cfg(not(feature = "audio"))]
fn decode_file(_path: &Path, _target_rate: u32) -> Result<(Vec<f32>, u8), String> {
    Err("Esta compilación de Lumos no incluye audio.".into())
}

/// A running audio setup (threads + shared state).
pub struct AudioIo {
    pub shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl AudioIo {
    /// State without devices (tests, or output closed): the mixer still renders when asked.
    pub fn headless(out_rate: u32, out_channels: usize) -> Self {
        AudioIo { shared: Shared::new(out_rate, out_channels), stop: Arc::new(AtomicBool::new(false)), handle: None }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    #[cfg(feature = "audio")]
    pub fn start(cfg: AudioCfg, input: bool, output: bool, tx: UnboundedSender<IoEvent>, ltc_fps: u32) -> Result<AudioIo, String> {
        device::start(cfg, input, output, tx, ltc_fps)
    }

    #[cfg(not(feature = "audio"))]
    pub fn start(_cfg: AudioCfg, _input: bool, _output: bool, _tx: UnboundedSender<IoEvent>, _ltc_fps: u32) -> Result<AudioIo, String> {
        Err("Esta compilación de Lumos no incluye audio.".into())
    }

    pub fn from_parts(shared: Arc<Shared>, stop: Arc<AtomicBool>, handle: Option<std::thread::JoinHandle<()>>) -> Self {
        AudioIo { shared, stop, handle }
    }
}

impl Drop for AudioIo {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn list_devices() -> Vec<AudioDevice> {
    #[cfg(feature = "audio")]
    {
        return device::list();
    }
    #[cfg(not(feature = "audio"))]
    Vec::new()
}

/// Everything the input thread does with one 10 ms block of de-interleaved channels. Pure enough to test without a card.
pub struct InputProc {
    cfg: Vec<AudioIn>,
    channels: usize,
    rate: u32,
    peaks: Vec<f32>,
    analyzer: Analyzer,
    ltc: Option<Decoder>,
    ltc_fps: u32,
    voice_rs: super::super::speech::worker::Resampler,
    last_send: Instant,
    pending_beat: bool,
    last_bands: Bands,
}

/// What a processed block produced.
#[derive(Default, Debug)]
pub struct InputOut {
    pub ltc: Vec<Tc>,
    /// 16 kHz mono samples for the voice recogniser.
    pub voice: Vec<f32>,
    /// Throttled reactive update (level, bass, mid, treble, beat, bpm).
    pub bands: Option<Bands>,
}

fn gain(db: f32) -> f32 {
    10f32.powf(db.clamp(-24.0, 24.0) / 20.0)
}

impl InputProc {
    pub fn new(cfg: &[AudioIn], channels: usize, rate: u32, ltc_fps: u32) -> Self {
        InputProc {
            cfg: cfg.to_vec(),
            channels: channels.max(1),
            rate,
            peaks: vec![0.0; channels.max(1)],
            analyzer: Analyzer::new(rate),
            ltc: cfg.iter().any(|c| c.role == AudioRole::Ltc).then(|| Decoder::new(rate, ltc_fps)),
            ltc_fps,
            voice_rs: super::super::speech::worker::Resampler::new(rate),
            last_send: Instant::now(),
            pending_beat: false,
            last_bands: Bands::default(),
        }
    }

    pub fn peaks(&self) -> &[f32] {
        &self.peaks
    }

    /// `interleaved`: frames × channels. `voice_wanted`: also produce the voice feed.
    pub fn process(&mut self, interleaved: &[f32], voice_wanted: bool) -> InputOut {
        let ch = self.channels;
        let frames = interleaved.len() / ch;
        let mut out = InputOut::default();
        if frames == 0 {
            return out;
        }
        let role_gain = |c: usize| self.cfg.iter().find(|x| x.channel as usize == c + 1).map(|x| (x.role, gain(x.gain_db)));
        // meters (all channels, post-trim for configured ones)
        for c in 0..ch {
            let g = role_gain(c).map_or(1.0, |r| r.1);
            let p = (0..frames).map(|f| (interleaved[f * ch + c] * g).abs()).fold(0.0f32, f32::max);
            let unit = if p < 1e-6 { 0.0 } else { ((20.0 * p.log10() + 60.0) / 60.0).clamp(0.0, 1.0) };
            self.peaks[c] = unit.max(self.peaks[c] * 0.85);
        }
        let mix = |role: AudioRole| -> Option<Vec<f32>> {
            let chans: Vec<(usize, f32)> = (0..ch).filter_map(|c| role_gain(c).filter(|r| r.0 == role).map(|r| (c, r.1))).collect();
            if chans.is_empty() {
                return None;
            }
            Some((0..frames).map(|f| chans.iter().map(|(c, g)| interleaved[f * ch + c] * g).sum::<f32>() / chans.len() as f32).collect())
        };
        if let Some(mono) = mix(AudioRole::Reactive) {
            let mut b = Vec::new();
            self.analyzer.push(&mono, &mut b);
            for x in b {
                self.pending_beat |= x.beat;
                self.last_bands = x;
            }
            if self.last_send.elapsed() >= Duration::from_millis(20) {
                self.last_send = Instant::now();
                let mut x = self.last_bands;
                x.beat = std::mem::take(&mut self.pending_beat);
                out.bands = Some(x);
            }
        }
        if let (Some(mono), Some(d)) = (mix(AudioRole::Ltc), self.ltc.as_mut()) {
            out.ltc = d.push(&mono);
        }
        if voice_wanted {
            // channels with role voice; none assigned = the first channel
            let mono = mix(AudioRole::Voice).unwrap_or_else(|| (0..frames).map(|f| interleaved[f * ch]).collect());
            self.voice_rs.push(&mono, &mut out.voice);
        }
        let _ = (self.rate, self.ltc_fps);
        out
    }
}

#[cfg(feature = "audio")]
mod device {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    pub fn list() -> Vec<AudioDevice> {
        let host = cpal::default_host();
        let din = host.default_input_device().and_then(|d| d.description().ok().map(|x| x.name().to_string()));
        let dout = host.default_output_device().and_then(|d| d.description().ok().map(|x| x.name().to_string()));
        let mut v: Vec<AudioDevice> = Vec::new();
        if let Ok(devs) = host.devices() {
            for d in devs {
                let Some(name) = d.description().ok().map(|x| x.name().to_string()) else { continue };
                let inputs = d.supported_input_configs().map(|c| c.map(|x| x.channels()).max().unwrap_or(0)).unwrap_or(0);
                let outputs = d.supported_output_configs().map(|c| c.map(|x| x.channels()).max().unwrap_or(0)).unwrap_or(0);
                if inputs == 0 && outputs == 0 {
                    continue;
                }
                v.push(AudioDevice { default_in: Some(&name) == din.as_ref(), default_out: Some(&name) == dout.as_ref(), name, inputs, outputs });
            }
        }
        v
    }

    fn find(host: &cpal::Host, name: &str, input: bool) -> Option<cpal::Device> {
        if name.trim().is_empty() {
            return if input { host.default_input_device() } else { host.default_output_device() };
        }
        host.devices().ok()?.find(|d| d.description().map(|x| x.name() == name).unwrap_or(false))
    }

    /// The widest configuration of the device, preferring 48 kHz.
    fn best(dev: &cpal::Device, input: bool) -> Result<cpal::SupportedStreamConfig, String> {
        let mut cfgs: Vec<cpal::SupportedStreamConfigRange> = if input { dev.supported_input_configs().map_err(|e| e.to_string())?.collect() } else { dev.supported_output_configs().map_err(|e| e.to_string())?.collect() };
        cfgs.retain(|c| matches!(c.sample_format(), cpal::SampleFormat::F32 | cpal::SampleFormat::I16 | cpal::SampleFormat::U16 | cpal::SampleFormat::I32));
        cfgs.sort_by_key(|c| (c.channels(), c.sample_format() == cpal::SampleFormat::F32));
        let c = cfgs.pop().ok_or("El dispositivo no ofrece un formato admitido")?;
        let rate = if (c.min_sample_rate()..=c.max_sample_rate()).contains(&48_000) { 48_000 } else { c.max_sample_rate().min(96_000) };
        Ok(c.with_sample_rate(rate))
    }

    pub fn decode(path: &Path, target_rate: u32) -> Result<(Vec<f32>, u8), String> {
        use symphonia::core::{audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint};
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).map_err(|e| format!("Formato de audio no reconocido: {e}"))?;
        let mut format = probed.format;
        let track = format.tracks().iter().find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL).ok_or("El archivo no tiene pista de audio")?;
        let id = track.id;
        let mut dec = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| format!("Códec no admitido: {e}"))?;
        let (mut rate, mut chans) = (track.codec_params.sample_rate.unwrap_or(target_rate), track.codec_params.channels.map(|c| c.count()).unwrap_or(2));
        let mut samples: Vec<f32> = Vec::new();
        let mut sb: Option<SampleBuffer<f32>> = None;
        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(Error::ResetRequired) => break,
                Err(e) => return Err(format!("Error leyendo el audio: {e}")),
            };
            if packet.track_id() != id {
                continue;
            }
            match dec.decode(&packet) {
                Ok(buf) => {
                    let spec = *buf.spec();
                    rate = spec.rate;
                    chans = spec.channels.count();
                    let s = sb.get_or_insert_with(|| SampleBuffer::<f32>::new(buf.capacity() as u64, spec));
                    s.copy_interleaved_ref(buf);
                    samples.extend_from_slice(s.samples());
                    if samples.len() as f32 > MAX_SOUND_S * rate as f32 * chans as f32 {
                        return Err("El audio es demasiado largo (máximo 20 minutos).".into());
                    }
                }
                Err(Error::DecodeError(_)) => continue,
                Err(e) => return Err(format!("Error decodificando el audio: {e}")),
            }
        }
        if samples.is_empty() || chans == 0 {
            return Err("El archivo no contiene audio".into());
        }
        // keep at most a stereo pair
        let (data, out_ch) = if chans <= 2 {
            (samples, chans)
        } else {
            let frames = samples.len() / chans;
            ((0..frames).flat_map(|f| [samples[f * chans], samples[f * chans + 1]]).collect(), 2)
        };
        Ok((resample(&data, out_ch, rate, target_rate), out_ch as u8))
    }

    pub fn start(cfg: AudioCfg, want_in: bool, want_out: bool, tx: UnboundedSender<IoEvent>, ltc_fps: u32) -> Result<AudioIo, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<Arc<Shared>, String>>(1);
        let stop2 = stop.clone();
        let handle = std::thread::Builder::new()
            .name("lumos-audio".into())
            .spawn(move || run(cfg, want_in, want_out, tx, ltc_fps, stop2, ready_tx))
            .map_err(|e| e.to_string())?;
        match ready_rx.recv_timeout(Duration::from_secs(15)) {
            Ok(Ok(shared)) => Ok(AudioIo::from_parts(shared, stop, Some(handle))),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                Err("El dispositivo de audio no responde".into())
            }
        }
    }

    fn run(cfg: AudioCfg, want_in: bool, want_out: bool, tx: UnboundedSender<IoEvent>, ltc_fps: u32, stop: Arc<AtomicBool>, ready: std::sync::mpsc::SyncSender<Result<Arc<Shared>, String>>) {
        let host = cpal::default_host();
        let err_cb = |e| tracing::warn!("audio: {e}");
        // ── output ──
        let mut out_stream = None;
        let (mut out_rate, mut out_ch, mut out_name) = (48_000u32, 2usize, String::new());
        let mut out_dev_cfg = None;
        if want_out {
            let Some(dev) = find(&host, &cfg.output_device, false) else {
                let _ = ready.send(Err(format!("No se encuentra la salida de audio «{}».", cfg.output_device)));
                return;
            };
            match best(&dev, false) {
                Ok(c) => {
                    out_rate = c.sample_rate();
                    out_ch = c.channels() as usize;
                    out_name = dev.description().map(|d| d.name().to_string()).unwrap_or_default();
                    out_dev_cfg = Some((dev, c));
                }
                Err(e) => {
                    let _ = ready.send(Err(format!("La salida de audio no se puede abrir: {e}")));
                    return;
                }
            }
        }
        let shared = Shared::new(out_rate, out_ch);
        shared.ltc_fps.store(ltc_fps, Ordering::Relaxed);
        shared.mixer.lock().configure(&cfg);
        if let Some((dev, c)) = out_dev_cfg {
            let sh = shared.clone();
            let sc: cpal::StreamConfig = c.clone().into();
            macro_rules! out {
                ($t:ty, $conv:expr) => {{
                    let mut tmp: Vec<f32> = Vec::new();
                    dev.build_output_stream(&sc, move |d: &mut [$t], _| {
                        tmp.resize(d.len(), 0.0);
                        sh.mixer.lock().render(&mut tmp);
                        for (o, s) in d.iter_mut().zip(tmp.iter()) {
                            *o = $conv(*s);
                        }
                    }, err_cb, None)
                }};
            }
            let s = match c.sample_format() {
                cpal::SampleFormat::F32 => out!(f32, |s: f32| s),
                cpal::SampleFormat::I16 => out!(i16, |s: f32| (s * 32767.0) as i16),
                cpal::SampleFormat::U16 => out!(u16, |s: f32| ((s * 32767.0) + 32768.0) as u16),
                cpal::SampleFormat::I32 => out!(i32, |s: f32| (s * 2147483647.0) as i32),
                other => {
                    let _ = ready.send(Err(format!("Formato de salida no admitido: {other:?}")));
                    return;
                }
            };
            match s.map_err(|e| e.to_string()).and_then(|s| s.play().map(|_| s).map_err(|e| e.to_string())) {
                Ok(s) => out_stream = Some(s),
                Err(e) => {
                    let _ = ready.send(Err(format!("No se pudo abrir la salida de audio: {e}")));
                    return;
                }
            }
        }
        // ── input ──
        let raw: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let mut in_stream = None;
        let (mut in_rate, mut in_ch, mut in_name) = (48_000u32, 1usize, String::new());
        if want_in {
            let Some(dev) = find(&host, &cfg.input_device, true) else {
                let _ = ready.send(Err(format!("No se encuentra la entrada de audio «{}».", cfg.input_device)));
                return;
            };
            let c = match best(&dev, true) {
                Ok(c) => c,
                Err(e) => {
                    let _ = ready.send(Err(format!("La entrada de audio no se puede abrir: {e}")));
                    return;
                }
            };
            in_rate = c.sample_rate();
            in_ch = c.channels() as usize;
            in_name = dev.description().map(|d| d.name().to_string()).unwrap_or_default();
            let q = raw.clone();
            let sc: cpal::StreamConfig = c.clone().into();
            macro_rules! inp {
                ($t:ty, $conv:expr) => {
                    dev.build_input_stream(&sc, move |d: &[$t], _| {
                        let mut g = q.lock();
                        g.extend(d.iter().map($conv));
                        let cap = in_rate as usize * in_ch * 5;
                        while g.len() > cap {
                            g.pop_front();
                        }
                    }, err_cb, None)
                };
            }
            let s = match c.sample_format() {
                cpal::SampleFormat::F32 => inp!(f32, |x: &f32| *x),
                cpal::SampleFormat::I16 => inp!(i16, |x: &i16| *x as f32 / 32768.0),
                cpal::SampleFormat::U16 => inp!(u16, |x: &u16| (*x as f32 - 32768.0) / 32768.0),
                cpal::SampleFormat::I32 => inp!(i32, |x: &i32| *x as f32 / 2147483648.0),
                other => {
                    let _ = ready.send(Err(format!("Formato de entrada no admitido: {other:?}")));
                    return;
                }
            };
            match s.map_err(|e| e.to_string()).and_then(|s| s.play().map(|_| s).map_err(|e| e.to_string())) {
                Ok(s) => in_stream = Some(s),
                Err(e) => {
                    let _ = ready.send(Err(format!("No se pudo abrir la entrada de audio: {e}")));
                    return;
                }
            }
        }
        {
            let mut i = shared.info.lock();
            *i = Info { in_channels: if want_in { in_ch as u16 } else { 0 }, out_channels: if want_out { out_ch as u16 } else { 0 }, in_rate, out_rate, input_running: want_in, output_running: want_out, in_device: in_name, out_device: out_name, error: String::new() };
        }
        let _ = ready.send(Ok(shared.clone()));
        // ── input analysis loop ──
        let mut proc = InputProc::new(&cfg.inputs, in_ch, in_rate, ltc_fps);
        let mut last_ltc_fps = ltc_fps;
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(10));
            if !want_in {
                continue;
            }
            let fps = shared.ltc_fps.load(Ordering::Relaxed);
            if fps != last_ltc_fps {
                last_ltc_fps = fps;
                proc = InputProc::new(&cfg.inputs, in_ch, in_rate, fps);
            }
            let block: Vec<f32> = raw.lock().drain(..).collect();
            if block.is_empty() {
                continue;
            }
            let usable = block.len() / in_ch * in_ch;
            let o = proc.process(&block[..usable], shared.voice_wanted.load(Ordering::Relaxed));
            *shared.in_peaks.lock() = proc.peaks().to_vec();
            if !o.voice.is_empty() {
                let mut t = shared.voice_tap.lock();
                t.extend(o.voice);
                while t.len() > VOICE_RATE as usize * 60 {
                    t.pop_front();
                }
            }
            if let Some(b) = o.bands {
                *shared.reactive.lock() = [b.level, b.bass, b.mid, b.treble];
                *shared.bpm.lock() = b.bpm;
                let _ = tx.send(IoEvent::AudioBands { bands: [b.level, b.bass, b.mid, b.treble], beat: b.beat, bpm: b.bpm });
            }
            for tc in o.ltc {
                *shared.ltc.lock() = Some((tc, Instant::now()));
                let _ = tx.send(IoEvent::AudioLtc { frames: tc.to_frames(fps), fps });
            }
        }
        drop(in_stream);
        drop(out_stream);
        let _ = tx.send(IoEvent::AudioStatus { input_running: false, output_running: false, error: String::new() });
    }
}

pub fn media_path(store_dir: &Path) -> PathBuf {
    store_dir.join("media")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::ltc::Encoder;

    fn cfg_in(ch: u16, role: AudioRole, gain_db: f32) -> AudioIn {
        AudioIn { channel: ch, label: String::new(), role, gain_db }
    }

    fn sine_block(rate: u32, hz: f32, secs: f32, amp: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize).map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin()).collect()
    }

    #[test]
    fn resample_changes_length_not_pitch_shape() {
        let a = vec![0.0, 1.0, 0.0, -1.0];
        let b = resample(&a, 1, 1000, 2000);
        assert_eq!(b.len(), 8);
        assert!((b[1] - 0.5).abs() < 1e-6 && (b[2] - 1.0).abs() < 1e-6);
        let st = resample(&[0.0, 1.0, 1.0, 0.0], 2, 48_000, 48_000);
        assert_eq!(st.len(), 4);
    }

    /// Four device channels: ch1 voice, ch2 audio-reactive, ch3 LTC, ch4 unused. Each goes where its role says.
    #[test]
    fn input_roles_route_each_channel_to_its_consumer() {
        let rate = 48_000;
        let cfg = vec![cfg_in(1, AudioRole::Voice, 0.0), cfg_in(2, AudioRole::Reactive, 0.0), cfg_in(3, AudioRole::Ltc, 0.0)];
        let mut p = InputProc::new(&cfg, 4, rate, 25);
        let mut enc = Encoder::new(rate, 25, 0.5);
        let ltc: Vec<f32> = (0..50).flat_map(|n| enc.frame(Tc::from_frames(25 * 7200 + n, 25))).collect();
        let kick = sine_block(rate, 70.0, ltc.len() as f32 / rate as f32, 0.6);
        let voice = sine_block(rate, 300.0, ltc.len() as f32 / rate as f32, 0.3);
        let mut got_ltc = Vec::new();
        let mut voice_samples = 0usize;
        let mut bands = None;
        for i in (0..ltc.len() - 480).step_by(480) {
            let mut block = Vec::new();
            for k in 0..480 {
                block.extend([voice[i + k], kick[i + k], ltc[i + k], 0.0]);
            }
            let o = p.process(&block, true);
            got_ltc.extend(o.ltc);
            voice_samples += o.voice.len();
            if o.bands.is_some() {
                bands = o.bands;
            }
        }
        assert!(got_ltc.len() >= 45, "LTC from channel 3 only: {}", got_ltc.len());
        assert_eq!(got_ltc.last().unwrap().to_frames(25), 25 * 7200 + 48, "the last frame is still pending its closing edge");
        let secs = ltc.len() as f32 / rate as f32;
        assert!((voice_samples as f32 - secs * 16_000.0).abs() < 200.0, "voice feed is 16 kHz mono: {voice_samples}");
        let b = bands.expect("reactive update");
        assert!(b.bass > 0.4, "the 70 Hz tone on channel 2 shows as bass: {b:?}");
        let peaks = p.peaks();
        assert!(peaks[0] > 0.3 && peaks[1] > 0.5 && peaks[3] == 0.0, "{peaks:?}");
    }

    #[test]
    fn trim_scales_the_level_and_unassigned_voice_uses_channel_one() {
        let rate = 48_000;
        let tone = sine_block(rate, 440.0, 0.2, 0.1);
        let run = |db: f32| {
            let mut p = InputProc::new(&[cfg_in(2, AudioRole::Reactive, db)], 2, rate, 25);
            let block: Vec<f32> = tone.iter().flat_map(|s| [0.0, *s]).collect();
            p.process(&block, false);
            p.peaks()[1]
        };
        assert!(run(12.0) > run(0.0) && run(0.0) > run(-12.0));
        let mut p = InputProc::new(&[], 2, rate, 25);
        let block: Vec<f32> = tone.iter().flat_map(|s| [*s, 0.0]).collect();
        let o = p.process(&block, true);
        assert!(o.voice.len() > 1000 && o.voice.iter().any(|s| s.abs() > 0.05), "channel 1 feeds the voice when none is assigned");
        assert!(p.process(&block, false).voice.is_empty());
    }

    #[test]
    fn shared_reports_missing_and_invalid_sounds() {
        let sh = Shared::new(48_000, 2);
        let dir = std::env::temp_dir().join(format!("lumos-snd-sh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(sh.sound(&dir, "../etc/passwd").unwrap_err().contains("no válido"));
        assert!(sh.sound(&dir, "nope.wav").unwrap_err().contains("No existe"));
    }
}
