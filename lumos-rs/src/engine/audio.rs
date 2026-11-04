//! Audio I/O inside the engine: preferences (`audio.json`), opening/closing the devices, sounds (`sound` console
//! statement, Guion marks, macros, bindings), the timeline's audio track, LTC in/out and the audio-reactive input.

use super::*;
use crate::audio::{self, io::AudioIo, ltc::Tc};
use std::sync::atomic::Ordering;

const FILE: &str = "audio.json";
const SOUNDS_REFRESH: Duration = Duration::from_secs(3);

pub struct AudioState {
    pub cfg: AudioCfg,
    io: Option<AudioIo>,
    /// What the open `io` covers.
    in_open: bool,
    out_open: bool,
    error: String,
    reactive: [f32; 4],
    bpm: f32,
    beat_high: bool,
    last_band_send: [f32; 4],
    devices: parking_lot::Mutex<Option<Vec<AudioDevice>>>,
    sounds: parking_lot::Mutex<(Vec<String>, Option<Instant>)>,
    /// Timeline whose audio plays (id, file) and when the decode was requested.
    tl: Option<(u32, String)>,
    tl_loading: Option<(u32, String, Instant)>,
}

impl AudioState {
    pub fn load(store: &Store) -> Self {
        let cfg = std::fs::read_to_string(store.dir.join(FILE)).ok().and_then(|t| serde_json::from_str::<AudioCfg>(&t).ok()).unwrap_or_default();
        AudioState {
            cfg,
            io: None,
            in_open: false,
            out_open: false,
            error: String::new(),
            reactive: [0.0; 4],
            bpm: 0.0,
            beat_high: false,
            last_band_send: [0.0; 4],
            devices: parking_lot::Mutex::new(None),
            sounds: parking_lot::Mutex::new((Vec::new(), None)),
            tl: None,
            tl_loading: None,
        }
    }

    fn save(&self, store: &Store) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.cfg).map_err(|e| e.to_string())?;
        std::fs::write(store.dir.join(FILE), text).map_err(|e| e.to_string())
    }
}

fn clean(mut cfg: AudioCfg) -> Result<AudioCfg, String> {
    cfg.master_db = cfg.master_db.clamp(-60.0, 12.0);
    let mut seen = std::collections::BTreeSet::new();
    for i in &cfg.inputs {
        if !(1..=64).contains(&i.channel) {
            return Err(format!("Canal de entrada {} fuera de rango (1–64)", i.channel));
        }
        if !seen.insert(i.channel) {
            return Err(format!("El canal de entrada {} está repetido", i.channel));
        }
    }
    for i in &mut cfg.inputs {
        i.gain_db = i.gain_db.clamp(-24.0, 24.0);
        i.label = i.label.chars().take(40).collect();
    }
    let mut cells = std::collections::BTreeSet::new();
    for r in &mut cfg.routes {
        let max_src = if r.source == AudioSource::Ltc { 1 } else { 2 };
        if r.src_ch < 1 || r.src_ch > max_src {
            return Err(format!("Canal de origen {} no válido para este bus (1–{max_src})", r.src_ch));
        }
        if !(1..=64).contains(&r.out_ch) {
            return Err(format!("Canal de salida {} fuera de rango (1–64)", r.out_ch));
        }
        if !cells.insert((format!("{:?}", r.source), r.src_ch, r.out_ch)) {
            return Err(format!("La ruta {:?} {} → salida {} está repetida", r.source, r.src_ch, r.out_ch));
        }
        r.gain_db = r.gain_db.clamp(-60.0, 12.0);
    }
    cfg.input_device = cfg.input_device.trim().to_string();
    cfg.output_device = cfg.output_device.trim().to_string();
    Ok(cfg)
}

impl Engine {
    pub(super) fn audio_command(&mut self, cmd: Command) -> Result<(), String> {
        match cmd {
            Command::AudioConfigure { cfg } => {
                let cfg = clean(cfg)?;
                let old = std::mem::replace(&mut self.audio.cfg, cfg);
                self.audio.save(&self.store)?;
                *self.audio.devices.lock() = None;
                let devices_changed = old.input_device != self.audio.cfg.input_device || old.output_device != self.audio.cfg.output_device || old.inputs != self.audio.cfg.inputs;
                if self.audio.io.is_some() {
                    if devices_changed {
                        let (i, o) = (self.audio.in_open, self.audio.out_open);
                        self.audio_close();
                        self.audio_open(i, o)?;
                    } else if let Some(io) = &self.audio.io {
                        io.shared.mixer.lock().configure(&self.audio.cfg);
                    }
                }
                self.dirty = true;
                Ok(())
            }
            Command::AudioRun { on: true } => {
                let want_in = !self.audio.cfg.inputs.is_empty();
                self.audio_close();
                self.audio_open(want_in, true)
            }
            Command::AudioRun { on: false } => {
                self.audio_close();
                Ok(())
            }
            Command::SoundPlay { file, volume, looped } => {
                // `sound gong` → `gong.wav` when the extension is left out
                let file = if audio::is_sound(&file) { file } else { audio::list_sounds(&self.store.media_dir()).into_iter().find(|f| f.rsplit_once('.').map_or(false, |(stem, _)| stem.eq_ignore_ascii_case(&file))).unwrap_or(file) };
                if !audio::is_sound(&file) {
                    return Err(format!("«{file}» no es un archivo de audio de la carpeta de medios (wav, mp3, flac, ogg, m4a, aac, aiff)"));
                }
                if !self.store.media_dir().join(&file).is_file() {
                    return Err(format!("No existe el sonido «{file}» en la carpeta de medios. Súbelo en Ajustes → Audio."));
                }
                if !self.audio.out_open {
                    let keep_in = self.audio.in_open;
                    self.audio_close();
                    self.audio_open(keep_in, true)?;
                }
                let io = self.audio.io.as_ref().ok_or("El audio no está abierto")?;
                let (shared, media, tx) = (io.shared.clone(), self.store.media_dir(), self.io.sender());
                let vol = if volume.is_finite() { volume.clamp(0.0, 4.0) } else { 1.0 };
                std::thread::spawn(move || match shared.sound(&media, &file) {
                    Ok(snd) => {
                        shared.mixer.lock().play(AudioSource::Sounds, snd, vol, looped, 0.0);
                    }
                    Err(e) => {
                        let _ = tx.send(IoEvent::AudioError { error: format!("{file}: {e}") });
                    }
                });
                Ok(())
            }
            Command::SoundStop { file } => {
                if let Some(io) = &self.audio.io {
                    io.shared.mixer.lock().stop(file.as_deref(), Some(AudioSource::Sounds));
                }
                Ok(())
            }
            _ => Err("Comando de audio desconocido".into()),
        }
    }

    /// Open the devices of the setup (`input`: the configured input device; `output`: the output device).
    pub(super) fn audio_open(&mut self, input: bool, output: bool) -> Result<(), String> {
        if self.audio.io.is_some() {
            return Ok(());
        }
        let fps = self.show.settings.timecode.fps.round().clamp(24.0, 30.0) as u32;
        match AudioIo::start(self.audio.cfg.clone(), input, output, self.io.sender(), fps) {
            Ok(io) => {
                self.audio.error.clear();
                self.audio.io = Some(io);
                self.audio.in_open = input;
                self.audio.out_open = output;
                self.dirty = true;
                Ok(())
            }
            Err(e) => {
                self.audio.error = e.clone();
                self.dirty = true;
                Err(e)
            }
        }
    }

    pub(super) fn audio_close(&mut self) {
        if let Some(mut io) = self.audio.io.take() {
            // joining waits for the device threads: not on the engine thread
            std::thread::spawn(move || io.stop());
        }
        self.audio.in_open = false;
        self.audio.out_open = false;
        self.audio.reactive = [0.0; 4];
        self.audio.bpm = 0.0;
        self.audio.tl = None;
        self.audio.tl_loading = None;
        self.dirty = true;
    }

    /// The voice recogniser needs the microphone: open the input if it is not (keeping the output).
    pub(super) fn audio_voice_feed(&mut self) -> Result<Arc<audio::io::Shared>, String> {
        if !self.audio.in_open {
            let keep_out = self.audio.out_open;
            self.audio_close();
            self.audio_open(true, keep_out)?;
        }
        let io = self.audio.io.as_ref().ok_or("El audio no está abierto")?;
        io.shared.voice_tap.lock().clear();
        io.shared.voice_wanted.store(true, Ordering::Relaxed);
        Ok(io.shared.clone())
    }

    pub(super) fn audio_voice_release(&mut self) {
        if let Some(io) = &self.audio.io {
            io.shared.voice_wanted.store(false, Ordering::Relaxed);
            io.shared.voice_tap.lock().clear();
        }
    }

    pub(super) fn on_audio_event(&mut self, ev: IoEvent) {
        match ev {
            IoEvent::AudioBands { bands, beat, bpm } => {
                self.audio.reactive = bands;
                self.audio.bpm = bpm;
                let names = [AudioBand::Level, AudioBand::Bass, AudioBand::Mid, AudioBand::Treble];
                for (i, band) in names.into_iter().enumerate() {
                    // only what changed visibly: bindings re-run their action on every event
                    if (bands[i] - self.audio.last_band_send[i]).abs() >= 0.01 {
                        self.audio.last_band_send[i] = bands[i];
                        self.run_bindings(&InEvent::Audio { band, value: bands[i] });
                    }
                }
                if beat {
                    self.audio.beat_high = true;
                    self.run_bindings(&InEvent::Audio { band: AudioBand::Beat, value: 1.0 });
                } else if self.audio.beat_high {
                    self.audio.beat_high = false;
                    self.run_bindings(&InEvent::Audio { band: AudioBand::Beat, value: 0.0 });
                }
                self.dirty = true;
            }
            IoEvent::AudioLtc { frames, fps } => {
                if self.show.settings.timecode.source == TcSource::Ltc {
                    let _ = fps;
                    self.tc.frames_f = frames as f64;
                    self.tc.running = true;
                    self.tc.last_mtc = Some(self.now());
                }
            }
            IoEvent::AudioStatus { input_running, output_running, error } => {
                if !input_running && !output_running {
                    self.audio.io = None;
                    self.audio.in_open = false;
                    self.audio.out_open = false;
                }
                if !error.is_empty() {
                    self.audio.error = error;
                }
                self.dirty = true;
            }
            IoEvent::AudioError { error } => {
                self.audio.error = error;
                self.dirty = true;
            }
            _ => {}
        }
    }

    /// Every tick: LTC output follows the show clock; the timeline's audio file follows the timeline.
    pub(super) fn audio_tick(&mut self) {
        let Some(io) = &self.audio.io else { return };
        if !self.audio.out_open {
            return;
        }
        let fps = self.show.settings.timecode.fps.round().clamp(24.0, 30.0) as u32;
        io.shared.ltc_fps.store(fps, Ordering::Relaxed);
        let ltc_routed = self.audio.cfg.routes.iter().any(|r| r.source == AudioSource::Ltc);
        io.shared.mixer.lock().set_ltc(ltc_routed && self.tc.running, self.tc.frames_f.max(0.0) as u64, fps);
        self.audio_timeline_sync();
    }

    fn audio_timeline_sync(&mut self) {
        let Some(io) = &self.audio.io else { return };
        let shared = io.shared.clone();
        let running: Option<(u32, String, f32)> = self.show.timelines.iter().find_map(|t| {
            let st = self.auto.tl.get(&t.id)?;
            (st.running && t.audio.is_some()).then(|| (t.id, t.audio.clone().unwrap().rsplit('/').next().unwrap_or("").to_string(), st.t as f32))
        });
        let playing: Vec<(u32, String, f32, f32, bool, AudioSource)> = shared.mixer.lock().playing().into_iter().filter(|p| p.5 == AudioSource::Timeline).collect();
        match running {
            None => {
                if !playing.is_empty() {
                    shared.mixer.lock().stop(None, Some(AudioSource::Timeline));
                }
                self.audio.tl = None;
                self.audio.tl_loading = None;
            }
            Some((id, file, t)) => {
                if !audio::is_sound(&file) {
                    return;
                }
                let in_sync = playing.first().map_or(false, |p| p.1 == file && (p.2 - t).abs() < 0.35);
                if in_sync {
                    self.audio.tl = Some((id, file));
                    return;
                }
                if let Some((lid, lf, at)) = &self.audio.tl_loading {
                    if playing.is_empty() && *lid == id && *lf == file && at.elapsed() < Duration::from_secs(8) {
                        return; // decoding
                    }
                }
                shared.mixer.lock().stop(None, Some(AudioSource::Timeline));
                self.audio.tl_loading = Some((id, file.clone(), Instant::now()));
                let (media, tx, started) = (self.store.media_dir(), self.io.sender(), Instant::now());
                std::thread::spawn(move || match shared.sound(&media, &file) {
                    // the timeline kept running while the file decoded: start where it is now
                    Ok(snd) => {
                        shared.mixer.lock().play(AudioSource::Timeline, snd, 1.0, false, t + started.elapsed().as_secs_f32());
                    }
                    Err(e) => {
                        let _ = tx.send(IoEvent::AudioError { error: format!("{file}: {e}") });
                    }
                });
            }
        }
    }

    fn audio_sounds(&self) -> Vec<String> {
        let mut g = self.audio.sounds.lock();
        if g.1.map_or(true, |t| t.elapsed() > SOUNDS_REFRESH) {
            g.0 = audio::list_sounds(&self.store.media_dir());
            g.1 = Some(Instant::now());
        }
        g.0.clone()
    }

    pub(super) fn audio_rt(&self) -> AudioRt {
        let devices = {
            let mut g = self.audio.devices.lock();
            if g.is_none() && audio::AVAILABLE {
                *g = Some(audio::io::list_devices());
            }
            g.clone().unwrap_or_default()
        };
        let (mut in_ch, mut out_ch, mut in_levels, mut out_levels, mut ltc, mut playing) = (0, 0, Vec::new(), Vec::new(), LtcRt::default(), Vec::new());
        let (mut input_running, mut output_running) = (false, false);
        if let Some(io) = &self.audio.io {
            let info = io.shared.info.lock().clone();
            in_ch = info.in_channels;
            out_ch = info.out_channels;
            input_running = info.input_running;
            output_running = info.output_running;
            in_levels = io.shared.in_peaks.lock().iter().map(|p| (p * 20.0).round() / 20.0).collect();
            let m = io.shared.mixer.lock();
            out_levels = m.peaks().iter().map(|p| if *p < 1e-6 { 0.0f32 } else { (((20.0f32 * p.log10() + 60.0) / 60.0).clamp(0.0, 1.0) * 20.0).round() / 20.0 }).collect();
            out_levels.truncate(out_ch as usize);
            playing = m.playing().into_iter().filter(|p| p.5 != AudioSource::Ltc).map(|p| SoundRt { id: p.0, file: p.1, pos_s: (p.2 * 10.0).round() / 10.0, len_s: (p.3 * 10.0).round() / 10.0, looped: p.4 }).collect();
            if let Some((tc, at)) = *io.shared.ltc.lock() {
                ltc = LtcRt { locked: at.elapsed() < Duration::from_millis(600), text: tc.text() };
            }
        }
        let q = |v: f32| (v * 20.0).round() / 20.0;
        AudioRt {
            available: audio::AVAILABLE,
            input_running,
            output_running,
            cfg: self.audio.cfg.clone(),
            devices,
            in_channels: in_ch,
            out_channels: out_ch,
            in_levels,
            out_levels,
            reactive: self.audio.reactive.map(q),
            bpm: self.audio.bpm.round(),
            ltc,
            playing,
            sounds: self.audio_sounds(),
            error: self.audio.error.clone(),
        }
    }

    /// Start of the engine: honour `autostart`.
    pub(super) fn audio_autostart(&mut self) {
        if self.audio.cfg.autostart && audio::AVAILABLE {
            let want_in = !self.audio.cfg.inputs.is_empty();
            let _ = self.audio_open(want_in, true);
        }
    }
}

#[allow(dead_code)]
fn _tc(t: Tc) -> String {
    t.text()
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::*;

    fn cfg_with(inputs: Vec<AudioIn>, routes: Vec<AudioRoute>) -> AudioCfg {
        AudioCfg { inputs, routes, ..AudioCfg::default() }
    }

    #[test]
    fn configuration_is_validated_saved_and_reloaded() {
        let mut e = engine();
        let ok = cfg_with(vec![AudioIn { channel: 1, label: "Micro".into(), role: AudioRole::Voice, gain_db: 40.0 }], vec![AudioRoute { source: AudioSource::Sounds, src_ch: 1, out_ch: 3, gain_db: -3.0 }]);
        e.handle(Command::AudioConfigure { cfg: ok }).unwrap();
        assert_eq!(e.audio.cfg.inputs[0].gain_db, 24.0, "trim is clamped");
        assert_eq!(AudioState::load(&e.store).cfg.routes[0].out_ch, 3, "persisted");
        let dup = cfg_with(vec![AudioIn { channel: 2, label: String::new(), role: AudioRole::Ltc, gain_db: 0.0 }, AudioIn { channel: 2, label: String::new(), role: AudioRole::Voice, gain_db: 0.0 }], vec![]);
        assert!(e.handle(Command::AudioConfigure { cfg: dup }).unwrap_err().contains("repetido"));
        let bad_src = cfg_with(vec![], vec![AudioRoute { source: AudioSource::Ltc, src_ch: 2, out_ch: 1, gain_db: 0.0 }]);
        assert!(e.handle(Command::AudioConfigure { cfg: bad_src }).unwrap_err().contains("no válido"));
        let bad_out = cfg_with(vec![], vec![AudioRoute { source: AudioSource::Sounds, src_ch: 1, out_ch: 99, gain_db: 0.0 }]);
        assert!(e.handle(Command::AudioConfigure { cfg: bad_out }).unwrap_err().contains("fuera de rango"));
        let twice = cfg_with(vec![], vec![AudioRoute { source: AudioSource::Sounds, src_ch: 1, out_ch: 1, gain_db: 0.0 }, AudioRoute { source: AudioSource::Sounds, src_ch: 1, out_ch: 1, gain_db: -6.0 }]);
        assert!(e.handle(Command::AudioConfigure { cfg: twice }).unwrap_err().contains("repetida"));
        assert_eq!(e.audio_rt().available, audio::AVAILABLE);
    }

    #[test]
    fn sounds_need_a_real_audio_file_and_report_clearly() {
        let mut e = engine();
        assert!(e.handle(Command::SoundPlay { file: "plano.pdf".into(), volume: 1.0, looped: false }).unwrap_err().contains("no es un archivo de audio"));
        assert!(e.handle(Command::SoundPlay { file: "nada.wav".into(), volume: 1.0, looped: false }).unwrap_err().contains("No existe el sonido"));
        e.handle(Command::SoundStop { file: None }).unwrap();
    }

    #[test]
    fn ltc_input_drives_the_show_timecode_only_when_selected() {
        let mut e = engine();
        e.show.settings.timecode.source = TcSource::Internal;
        e.on_io(IoEvent::AudioLtc { frames: 25 * 3600, fps: 25 });
        assert!(!e.tc.running, "ignored unless the timecode source is LTC");
        e.show.settings.timecode.source = TcSource::Ltc;
        e.on_io(IoEvent::AudioLtc { frames: 25 * 3600 + 7, fps: 25 });
        assert!(e.tc.running && e.tc.frames_f as u64 == 25 * 3600 + 7);
        assert!(e.tc.last_mtc.is_some());
    }

    #[test]
    fn audio_reactive_values_drive_bindings() {
        let mut e = engine();
        e.show.bindings.push(Binding { id: 1, label: "Nivel → GM".into(), enabled: true, source: Source::Audio { band: AudioBand::Level }, action: Action::GrandMaster, out_min: 0.0, out_max: 1.0 });
        e.show.bindings.push(Binding { id: 2, label: "Beat → blackout".into(), enabled: true, source: Source::Audio { band: AudioBand::Beat }, action: Action::BlackoutToggle, out_min: 0.0, out_max: 1.0 });
        e.on_io(IoEvent::AudioBands { bands: [0.5, 0.2, 0.1, 0.0], beat: false, bpm: 0.0 });
        assert!((e.grand_master - 0.5).abs() < 1e-6, "{}", e.grand_master);
        assert!(!e.blackout);
        e.on_io(IoEvent::AudioBands { bands: [0.5, 0.9, 0.1, 0.0], beat: true, bpm: 120.0 });
        assert!(e.blackout, "a beat is a rising edge");
        e.on_io(IoEvent::AudioBands { bands: [0.5, 0.9, 0.1, 0.0], beat: true, bpm: 120.0 });
        assert!(e.blackout, "still the same high beat: no double toggle");
        e.on_io(IoEvent::AudioBands { bands: [0.5, 0.2, 0.1, 0.0], beat: false, bpm: 120.0 });
        e.on_io(IoEvent::AudioBands { bands: [0.5, 0.9, 0.1, 0.0], beat: true, bpm: 120.0 });
        assert!(!e.blackout, "next beat toggles back");
        let rt = e.audio_rt();
        assert_eq!(rt.bpm, 120.0);
        assert!(rt.reactive[1] > 0.0);
    }

    /// 16-bit PCM WAV bytes.
    #[cfg(feature = "audio")]
    fn wav(rate: u32, channels: u16, secs: f32, hz: f32) -> Vec<u8> {
        let n = (rate as f32 * secs) as usize;
        let mut pcm = Vec::new();
        for i in 0..n {
            let v = ((2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin() * 0.5 * 32767.0) as i16;
            for _ in 0..channels {
                pcm.extend_from_slice(&v.to_le_bytes());
            }
        }
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&channels.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        b.extend_from_slice(&(channels * 2).to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        b.extend_from_slice(&pcm);
        b
    }

    #[cfg(feature = "audio")]
    fn headless(e: &mut Engine) {
        e.audio.io = Some(AudioIo::headless(48_000, 4));
        e.audio.out_open = true;
    }

    #[cfg(feature = "audio")]
    fn wait_for(f: impl Fn() -> bool) -> bool {
        for _ in 0..200 {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[cfg(feature = "audio")]
    #[test]
    fn wav_decodes_resamples_and_plays_through_the_routing_matrix() {
        let mut e = engine();
        headless(&mut e);
        std::fs::write(e.store.media_dir().join("tono.wav"), wav(44_100, 2, 0.5, 440.0)).unwrap();
        // matrix: left → output 3, right → output 4
        let routes = vec![AudioRoute { source: AudioSource::Sounds, src_ch: 1, out_ch: 3, gain_db: 0.0 }, AudioRoute { source: AudioSource::Sounds, src_ch: 2, out_ch: 4, gain_db: 0.0 }];
        e.handle(Command::AudioConfigure { cfg: cfg_with(vec![], routes) }).unwrap();
        e.handle(Command::SoundPlay { file: "tono".into(), volume: 0.5, looped: false }).unwrap(); // bare name finds tono.wav
        let shared = e.audio.io.as_ref().unwrap().shared.clone();
        assert!(wait_for(|| !shared.mixer.lock().playing().is_empty()), "decoded and started");
        let rt = e.audio_rt();
        assert_eq!(rt.playing.len(), 1);
        assert!((rt.playing[0].len_s - 0.5).abs() < 0.05, "{:?}", rt.playing[0]);
        let mut out = vec![0.0; 4 * 2400];
        shared.mixer.lock().render(&mut out);
        let peak = |c: usize| out.chunks(4).map(|f| f[c].abs()).fold(0.0f32, f32::max);
        assert!(peak(0) == 0.0 && peak(1) == 0.0, "outputs 1-2 silent");
        assert!(peak(2) > 0.2 && peak(3) > 0.2 && peak(2) <= 0.26, "440 Hz at volume 0.5 on outputs 3-4: {} {}", peak(2), peak(3));
        e.handle(Command::SoundStop { file: Some("tono.wav".into()) }).unwrap();
        let mut more = vec![0.0; 4 * 4800];
        shared.mixer.lock().render(&mut more);
        assert!(shared.mixer.lock().playing().is_empty());
        // a corrupt file is reported, not played
        std::fs::write(e.store.media_dir().join("roto.mp3"), b"no es audio").unwrap();
        e.handle(Command::SoundPlay { file: "roto.mp3".into(), volume: 1.0, looped: false }).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(shared.mixer.lock().playing().is_empty());
    }

    #[cfg(feature = "audio")]
    #[test]
    fn a_running_timeline_plays_its_audio_and_stops_with_it() {
        let mut e = engine();
        headless(&mut e);
        std::fs::write(e.store.media_dir().join("pista.wav"), wav(48_000, 1, 3.0, 220.0)).unwrap();
        e.handle(Command::TimelineSave { timeline: Timeline { id: 0, name: "Show".into(), length: 3.0, events: vec![], looped: false, tc_start: None, audio: Some("/media/pista.wav".into()) } }).unwrap();
        let id = e.show.timelines[0].id;
        let shared = e.audio.io.as_ref().unwrap().shared.clone();
        e.audio_tick();
        assert!(shared.mixer.lock().playing().is_empty(), "stopped timeline: no audio");
        e.handle(Command::TimelineRun { id, from: Some(1.0) }).unwrap();
        e.audio_tick();
        assert!(wait_for(|| shared.mixer.lock().playing().iter().any(|p| p.5 == AudioSource::Timeline)), "audio follows the timeline");
        let pos = shared.mixer.lock().playing()[0].2;
        assert!(pos >= 1.0 && pos < 1.6, "starts at the timeline position: {pos}");
        // seeking far away re-aligns
        e.handle(Command::TimelineSeek { id, t: 2.4 }).unwrap();
        e.audio_tick();
        assert!(wait_for(|| shared.mixer.lock().playing().first().map_or(false, |p| p.2 > 2.3)), "re-aligned after a seek");
        e.handle(Command::TimelineStop { id }).unwrap();
        e.audio_tick();
        let mut out = vec![0.0; 4 * 4800];
        shared.mixer.lock().render(&mut out);
        assert!(shared.mixer.lock().playing().is_empty(), "stops with the timeline");
    }
}
