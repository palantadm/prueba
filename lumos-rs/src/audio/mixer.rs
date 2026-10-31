//! Output mixer: sounds and the LTC generator are summed into the device's output channels through the routing matrix.
//! Pure (no device); the cpal output callback just calls [`Mixer::render`].

use super::ltc::{Encoder, Tc};
use crate::model::{AudioCfg, AudioRoute, AudioSource};
use std::sync::Arc;

/// A decoded sound at the device sample rate, 1 or 2 channels, interleaved.
#[derive(Debug)]
pub struct Sound {
    pub file: String,
    pub channels: u8,
    pub data: Vec<f32>,
}

impl Sound {
    pub fn frames(&self) -> usize {
        self.data.len() / self.channels.max(1) as usize
    }
}

pub struct Voice {
    pub id: u32,
    pub bus: AudioSource,
    pub sound: Arc<Sound>,
    pub pos: usize,
    pub gain: f32,
    pub looped: bool,
    /// Remaining frames of a fade-out (the voice ends when it reaches 0).
    fade_left: Option<usize>,
    fade_len: usize,
}

fn db(g: f32) -> f32 {
    10f32.powf(g / 20.0)
}

/// Default matrix: sounds and timeline stereo to outputs 1/2, LTC to output 3 (or 1 on a mono device).
pub fn default_routes() -> Vec<AudioRoute> {
    vec![
        AudioRoute { source: AudioSource::Sounds, src_ch: 1, out_ch: 1, gain_db: 0.0 },
        AudioRoute { source: AudioSource::Sounds, src_ch: 2, out_ch: 2, gain_db: 0.0 },
        AudioRoute { source: AudioSource::Timeline, src_ch: 1, out_ch: 1, gain_db: 0.0 },
        AudioRoute { source: AudioSource::Timeline, src_ch: 2, out_ch: 2, gain_db: 0.0 },
    ]
}

fn bus_ix(b: AudioSource) -> usize {
    match b {
        AudioSource::Sounds => 0,
        AudioSource::Timeline => 1,
        AudioSource::Ltc => 2,
    }
}

struct LtcGen {
    enc: Encoder,
    fps: u32,
    next: u64,
    running: bool,
    buf: Vec<f32>,
    at: usize,
}

pub struct Mixer {
    pub rate: u32,
    pub out_channels: usize,
    voices: Vec<Voice>,
    next_id: u32,
    /// Per bus: (bus channel index, output index, linear gain incl. master).
    routes: [Vec<(usize, usize, f32)>; 3],
    master: f32,
    /// Peak per output channel with decay (linear).
    peak: Vec<f32>,
    ltc: Option<LtcGen>,
}

impl Mixer {
    pub fn new(rate: u32, out_channels: usize) -> Self {
        let mut m = Mixer { rate, out_channels, voices: Vec::new(), next_id: 1, routes: Default::default(), master: 1.0, peak: vec![0.0; out_channels], ltc: None };
        m.configure(&AudioCfg::default());
        m
    }

    pub fn configure(&mut self, cfg: &AudioCfg) {
        self.master = db(cfg.master_db.clamp(-60.0, 12.0));
        let routes = if cfg.routes.is_empty() { default_routes() } else { cfg.routes.clone() };
        self.routes = Default::default();
        for r in routes {
            let (s, o) = (r.src_ch as usize, r.out_ch as usize);
            if s >= 1 && o >= 1 && o <= self.out_channels {
                self.routes[bus_ix(r.source)].push((s - 1, o - 1, db(r.gain_db.clamp(-60.0, 12.0))));
            }
        }
        // a mono device: fold the stereo pair onto its only output
        if self.out_channels == 1 && cfg.routes.is_empty() {
            self.routes[0].push((1, 0, 1.0));
            self.routes[1].push((1, 0, 1.0));
        }
    }

    pub fn play(&mut self, bus: AudioSource, sound: Arc<Sound>, gain: f32, looped: bool, from_s: f32) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let pos = ((from_s.max(0.0) * self.rate as f32) as usize).min(sound.frames());
        self.voices.push(Voice { id, bus, sound, pos, gain: gain.clamp(0.0, 4.0), looped, fade_left: None, fade_len: 1 });
        id
    }

    /// Stop with a short fade (no click). `file`: only that file; `bus`: only that bus.
    pub fn stop(&mut self, file: Option<&str>, bus: Option<AudioSource>) {
        let n = (self.rate as usize / 25).max(1);
        for v in &mut self.voices {
            if file.map_or(true, |f| v.sound.file == f) && bus.map_or(true, |b| v.bus == b) {
                v.fade_left.get_or_insert(n);
                v.fade_len = n;
            }
        }
    }

    pub fn playing(&self) -> Vec<(u32, String, f32, f32, bool, AudioSource)> {
        self.voices.iter().filter(|v| v.fade_left.is_none()).map(|v| (v.id, v.sound.file.clone(), v.pos as f32 / self.rate as f32, v.sound.frames() as f32 / self.rate as f32, v.looped, v.bus)).collect()
    }

    pub fn peaks(&self) -> &[f32] {
        &self.peak
    }

    /// LTC output: `running` follows the show clock; the generator re-aligns when it drifts more than 2 frames.
    pub fn set_ltc(&mut self, running: bool, frame: u64, fps: u32) {
        let fps = fps.clamp(24, 30);
        match &mut self.ltc {
            Some(g) if g.fps == fps => {
                g.running = running;
                if running && g.next.abs_diff(frame) > 2 {
                    g.next = frame;
                    g.buf.clear();
                    g.at = 0;
                }
                if !running {
                    g.next = frame;
                }
            }
            _ => self.ltc = Some(LtcGen { enc: Encoder::new(self.rate, fps, 0.6), fps, next: frame, running, buf: Vec::new(), at: 0 }),
        }
    }

    /// Fill `out` (interleaved, `out_channels` per frame).
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let ch = self.out_channels.max(1);
        let frames = out.len() / ch;
        for v in &mut self.voices {
            let routes = &self.routes[bus_ix(v.bus)];
            let sc = v.sound.channels.max(1) as usize;
            let total = v.sound.frames();
            for f in 0..frames {
                if v.pos >= total {
                    if v.looped && total > 0 {
                        v.pos = 0;
                    } else {
                        v.fade_left = Some(0);
                        break;
                    }
                }
                let fade = match &mut v.fade_left {
                    Some(0) => break,
                    Some(n) => {
                        *n -= 1;
                        *n as f32 / v.fade_len as f32
                    }
                    None => 1.0,
                };
                let base = v.pos * sc;
                for &(src, o, g) in routes {
                    let s = v.sound.data[base + src.min(sc - 1)];
                    out[f * ch + o] += s * g * v.gain * fade * self.master;
                }
                v.pos += 1;
            }
        }
        self.voices.retain(|v| !matches!(v.fade_left, Some(0)) && !(v.pos >= v.sound.frames() && !v.looped));
        // LTC bus (mono)
        if let Some(g) = &mut self.ltc {
            if g.running {
                let routes = &self.routes[2];
                let targets: Vec<(usize, f32)> = if routes.is_empty() { vec![(self.out_channels.saturating_sub(1).min(2), 1.0)] } else { routes.iter().map(|(_, o, gn)| (*o, *gn)).collect() };
                for f in 0..frames {
                    if g.at >= g.buf.len() {
                        g.buf = g.enc.frame(Tc::from_frames(g.next, g.fps));
                        g.next += 1;
                        g.at = 0;
                    }
                    let s = g.buf[g.at];
                    g.at += 1;
                    for &(o, gn) in &targets {
                        if o < ch {
                            out[f * ch + o] += s * gn * self.master;
                        }
                    }
                }
            }
        }
        // meters
        let decay = 0.9f32.powf(frames as f32 / (self.rate as f32 * 0.01));
        for c in 0..ch.min(self.peak.len()) {
            let mut p = 0.0f32;
            for f in 0..frames {
                p = p.max(out[f * ch + c].abs());
            }
            self.peak[c] = p.max(self.peak[c] * decay);
        }
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::ltc::Decoder;

    fn ramp(file: &str, frames: usize, ch: u8) -> Arc<Sound> {
        Arc::new(Sound { file: file.into(), channels: ch, data: (0..frames * ch as usize).map(|i| 0.5 * (1.0 - 2.0 * (i % 2) as f32 * (ch - 1) as f32 * 0.0)).collect() })
    }

    fn stereo(file: &str, l: f32, r: f32, frames: usize) -> Arc<Sound> {
        let mut d = Vec::new();
        for _ in 0..frames {
            d.push(l);
            d.push(r);
        }
        Arc::new(Sound { file: file.into(), channels: 2, data: d })
    }

    fn route(source: AudioSource, src: u8, out: u16, gain: f32) -> AudioRoute {
        AudioRoute { source, src_ch: src, out_ch: out, gain_db: gain }
    }

    #[test]
    fn matrix_routes_left_and_right_to_chosen_outputs() {
        let mut m = Mixer::new(48_000, 4);
        m.configure(&AudioCfg { routes: vec![route(AudioSource::Sounds, 1, 3, 0.0), route(AudioSource::Sounds, 2, 4, -6.0206)], ..Default::default() });
        m.play(AudioSource::Sounds, stereo("a.wav", 0.8, 0.4, 100), 1.0, false, 0.0);
        let mut out = vec![0.0; 4 * 64];
        m.render(&mut out);
        for f in 0..64 {
            assert_eq!(&out[f * 4..f * 4 + 2], &[0.0, 0.0], "outputs 1-2 stay silent");
            assert!((out[f * 4 + 2] - 0.8).abs() < 1e-5);
            assert!((out[f * 4 + 3] - 0.2).abs() < 1e-3, "right at −6 dB");
        }
        assert!(m.peaks()[2] > 0.79 && m.peaks()[0] == 0.0);
    }

    #[test]
    fn default_matrix_master_gain_and_mono_sounds() {
        let mut m = Mixer::new(48_000, 2);
        m.play(AudioSource::Sounds, Arc::new(Sound { file: "m.wav".into(), channels: 1, data: vec![0.5; 100] }), 0.5, false, 0.0);
        let mut out = vec![0.0; 2 * 10];
        m.render(&mut out);
        assert!((out[0] - 0.25).abs() < 1e-6 && (out[1] - 0.25).abs() < 1e-6, "a mono file feeds both channels");
        m.configure(&AudioCfg { master_db: -6.0206, ..Default::default() });
        let mut out = vec![0.0; 2 * 10];
        m.render(&mut out);
        assert!((out[0] - 0.125).abs() < 1e-3);
        // a mono device folds the pair
        let mut m = Mixer::new(48_000, 1);
        m.play(AudioSource::Sounds, stereo("s.wav", 0.2, 0.3, 50), 1.0, false, 0.0);
        let mut out = vec![0.0; 10];
        m.render(&mut out);
        assert!((out[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn voices_end_loop_and_stop_with_a_fade() {
        let mut m = Mixer::new(48_000, 2);
        m.play(AudioSource::Sounds, stereo("short.wav", 0.5, 0.5, 100), 1.0, false, 0.0);
        let mut out = vec![0.0; 2 * 64];
        m.render(&mut out);
        assert_eq!(m.playing().len(), 1);
        m.render(&mut out);
        assert!(m.playing().is_empty(), "ended on its own");
        assert!(out[2 * 40..].iter().all(|s| *s == 0.0));
        // looping keeps going
        m.play(AudioSource::Sounds, stereo("loop.wav", 0.5, 0.5, 100), 1.0, true, 0.0);
        for _ in 0..10 {
            m.render(&mut out);
        }
        assert_eq!(m.playing().len(), 1);
        // stop fades in ~40 ms with no discontinuity larger than the step
        m.stop(Some("loop.wav"), None);
        let mut big = vec![0.0; 2 * 4800];
        m.render(&mut big);
        let mut prev = 0.5f32;
        for f in 0..4800 {
            let s = big[f * 2];
            assert!((prev - s).abs() < 0.01, "click at {f}: {prev} → {s}");
            prev = s;
        }
        assert_eq!(big[2 * 4799], 0.0);
        assert!(m.playing().is_empty());
    }

    #[test]
    fn start_offset_and_bus_selective_stop() {
        let mut m = Mixer::new(1000, 2);
        m.play(AudioSource::Timeline, Arc::new(Sound { file: "t.wav".into(), channels: 1, data: (0..2000).map(|i| i as f32 / 4000.0).collect() }), 1.0, false, 0.5);
        let mut out = vec![0.0; 2];
        m.render(&mut out);
        assert!((out[0] - 0.125).abs() < 1e-6, "starts 0.5 s in: {}", out[0]);
        m.play(AudioSource::Sounds, stereo("s.wav", 0.1, 0.1, 5000), 1.0, true, 0.0);
        m.stop(None, Some(AudioSource::Timeline));
        let mut big = vec![0.0; 2 * 200];
        m.render(&mut big);
        let p = m.playing();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].1, "s.wav");
    }

    #[test]
    fn ltc_bus_is_a_decodable_signal_on_the_routed_output() {
        let rate = 48_000;
        let mut m = Mixer::new(rate, 4);
        m.configure(&AudioCfg { routes: vec![route(AudioSource::Ltc, 1, 4, 0.0)], ..Default::default() });
        m.set_ltc(true, 25 * 3600, 25);
        let mut audio = Vec::new();
        for _ in 0..(rate as usize * 3 / 480) {
            let mut out = vec![0.0; 4 * 480];
            m.render(&mut out);
            assert!(out.chunks(4).all(|f| f[0] == 0.0 && f[1] == 0.0 && f[2] == 0.0));
            audio.extend(out.chunks(4).map(|f| f[3]));
        }
        let got = Decoder::new(rate, 25).push(&audio);
        assert!(got.len() >= 70, "{}", got.len());
        assert_eq!(got[0].to_frames(25), 25 * 3600, "starts at the frame the show clock was at");
        for w in got.windows(2) {
            assert_eq!(w[1].to_frames(25), w[0].to_frames(25) + 1);
        }
        // not running = silence on the LTC output
        m.set_ltc(false, 0, 25);
        let mut out = vec![0.0; 4 * 480];
        m.render(&mut out);
        assert!(out.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn unused_ramp_helper_compiles() {
        assert_eq!(ramp("x", 4, 1).frames(), 4);
    }
}
