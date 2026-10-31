//! LTC (SMPTE linear timecode): biphase-mark encoder and decoder. Pure — samples in, frames out.
//!
//! An LTC frame is 80 bits: BCD frame/second/minute/hour, flags, a polarity bit and the sync word `0x3FFD`
//! (bits 64..79). Each bit cell starts with a transition; a `1` has a second transition in the middle.

use std::f32::consts::PI;

pub const SYNC: [bool; 16] = [false, false, true, true, true, true, true, true, true, true, true, true, true, true, false, true];

/// Hours:minutes:seconds:frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tc {
    pub h: u8,
    pub m: u8,
    pub s: u8,
    pub f: u8,
}

impl Tc {
    pub fn from_frames(total: u64, fps: u32) -> Tc {
        let fps = fps.max(1) as u64;
        let f = (total % fps) as u8;
        let secs = total / fps;
        Tc { h: ((secs / 3600) % 24) as u8, m: ((secs / 60) % 60) as u8, s: (secs % 60) as u8, f }
    }
    pub fn to_frames(&self, fps: u32) -> u64 {
        ((self.h as u64 * 3600 + self.m as u64 * 60 + self.s as u64) * fps as u64) + self.f as u64
    }
    pub fn text(&self) -> String {
        format!("{:02}:{:02}:{:02}:{:02}", self.h, self.m, self.s, self.f)
    }
}

fn put(bits: &mut [bool; 80], at: usize, n: usize, v: u8) {
    for i in 0..n {
        bits[at + i] = (v >> i) & 1 == 1;
    }
}

fn get(bits: &[bool], at: usize, n: usize) -> u8 {
    (0..n).fold(0u8, |a, i| a | ((bits[at + i] as u8) << i))
}

/// The 80 data bits of a frame (with the polarity-correction bit so the frame holds an even number of zeros).
pub fn frame_bits(tc: Tc, fps: u32) -> [bool; 80] {
    let mut b = [false; 80];
    put(&mut b, 0, 4, tc.f % 10);
    put(&mut b, 8, 2, tc.f / 10);
    put(&mut b, 16, 4, tc.s % 10);
    put(&mut b, 24, 3, tc.s / 10);
    put(&mut b, 32, 4, tc.m % 10);
    put(&mut b, 40, 3, tc.m / 10);
    put(&mut b, 48, 4, tc.h % 10);
    put(&mut b, 56, 2, tc.h / 10);
    b[64..80].copy_from_slice(&SYNC);
    let pol = if fps == 25 { 59 } else { 27 };
    let zeros = b.iter().filter(|x| !**x).count();
    if zeros % 2 == 1 {
        b[pol] = true; // flipping a 0 into a 1 makes the count of zeros even
    }
    b
}

/// Parse 80 bits (sync already matched) into a timecode.
pub fn parse_bits(b: &[bool]) -> Option<Tc> {
    if b.len() != 80 || b[64..80] != SYNC {
        return None;
    }
    let tc = Tc { f: get(b, 0, 4) + 10 * get(b, 8, 2), s: get(b, 16, 4) + 10 * get(b, 24, 3), m: get(b, 32, 4) + 10 * get(b, 40, 3), h: get(b, 48, 4) + 10 * get(b, 56, 2) };
    (tc.f < 40 && tc.s < 60 && tc.m < 60 && tc.h < 24 && get(b, 0, 4) < 10 && get(b, 16, 4) < 10 && get(b, 32, 4) < 10 && get(b, 48, 4) < 10).then_some(tc)
}

pub struct Encoder {
    rate: f32,
    fps: u32,
    level: f32,
    /// Output polarity (flips on every transition).
    high: bool,
    /// Fraction of a sample carried between bit cells.
    carry: f32,
}

impl Encoder {
    pub fn new(rate: u32, fps: u32, amplitude: f32) -> Self {
        Encoder { rate: rate as f32, fps: fps.max(1), level: amplitude, high: false, carry: 0.0 }
    }

    pub fn samples_per_frame(&self) -> f32 {
        self.rate / self.fps as f32
    }

    /// Samples of one frame (≈ rate / fps of them; the fractional part is carried into the next frame).
    pub fn frame(&mut self, tc: Tc) -> Vec<f32> {
        let bits = frame_bits(tc, self.fps);
        let cell = self.samples_per_frame() / 80.0;
        let mut out = Vec::with_capacity(self.samples_per_frame() as usize + 2);
        let mut push_half = |out: &mut Vec<f32>, len: f32, high: bool, carry: &mut f32, level: f32| {
            let total = len + *carry;
            let n = total.floor();
            *carry = total - n;
            out.extend(std::iter::repeat(if high { level } else { -level }).take(n as usize));
        };
        for bit in bits {
            self.high = !self.high; // every cell starts with a transition
            if bit {
                push_half(&mut out, cell / 2.0, self.high, &mut self.carry, self.level);
                self.high = !self.high;
                push_half(&mut out, cell / 2.0, self.high, &mut self.carry, self.level);
            } else {
                push_half(&mut out, cell, self.high, &mut self.carry, self.level);
            }
        }
        // soften the edges a little (real LTC is band limited): 3-tap smoothing keeps zero crossings in place
        let mut sm = out.clone();
        for i in 1..out.len().saturating_sub(1) {
            sm[i] = 0.25 * out[i - 1] + 0.5 * out[i] + 0.25 * out[i + 1];
        }
        sm
    }
}

pub struct Decoder {
    /// Nominal samples per bit cell.
    cell: f32,
    prev_pos: bool,
    since: f32,
    /// A short interval waiting for its pair (the two halves of a `1`).
    half: bool,
    reg: Vec<bool>,
    gain: f32,
    floor: f32,
    pub last: Option<Tc>,
}

impl Decoder {
    pub fn new(rate: u32, fps: u32) -> Self {
        Decoder { cell: rate as f32 / (fps.max(1) as f32 * 80.0), prev_pos: false, since: 0.0, half: false, reg: Vec::with_capacity(96), gain: 0.0, floor: 0.0, last: None }
    }

    /// Feed samples; returns every frame completed by them.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Tc> {
        let mut out = Vec::new();
        for &x in samples {
            // track peak to set a hysteresis threshold, remove slow DC
            self.floor += (x - self.floor) * 0.001;
            let v = x - self.floor;
            self.gain = (self.gain * 0.9995).max(v.abs());
            let thr = (self.gain * 0.2).max(0.004);
            let pos = if self.prev_pos { v > -thr } else { v > thr };
            self.since += 1.0;
            if pos != self.prev_pos {
                self.prev_pos = pos;
                let d = self.since;
                self.since = 0.0;
                if let Some(t) = self.interval(d) {
                    out.push(t);
                }
            }
            if self.since > self.cell * 4.0 {
                // signal lost
                self.reg.clear();
                self.half = false;
            }
        }
        out
    }

    fn interval(&mut self, d: f32) -> Option<Tc> {
        let long = d > self.cell * 0.75 && d < self.cell * 1.4;
        let short = d > self.cell * 0.3 && d <= self.cell * 0.75;
        if long {
            if self.half {
                // a long interval in the middle of a pair: lost sync
                self.half = false;
                self.reg.clear();
                return None;
            }
            // slowly follow the speed of the source
            self.cell += (d - self.cell) * 0.02;
            return self.bit(false);
        }
        if short {
            if self.half {
                self.half = false;
                self.cell += (d * 2.0 - self.cell) * 0.02;
                return self.bit(true);
            }
            self.half = true;
            return None;
        }
        self.half = false;
        self.reg.clear();
        None
    }

    fn bit(&mut self, b: bool) -> Option<Tc> {
        self.reg.push(b);
        if self.reg.len() > 80 {
            self.reg.remove(0);
        }
        if self.reg.len() == 80 && self.reg[64..] == SYNC {
            let tc = parse_bits(&self.reg);
            self.reg.clear();
            if tc.is_some() {
                self.last = tc;
            }
            return tc;
        }
        None
    }
}

/// Simple test-signal helper.
pub fn sine(rate: u32, hz: f32, secs: f32) -> Vec<f32> {
    (0..(rate as f32 * secs) as usize).map(|i| (2.0 * PI * hz * i as f32 / rate as f32).sin()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(rate: u32, fps: u32, start: u64, frames: u64) -> (Vec<f32>, Vec<Tc>) {
        let mut e = Encoder::new(rate, fps, 0.5);
        let mut audio = Vec::new();
        let mut tcs = Vec::new();
        for n in start..start + frames {
            let tc = Tc::from_frames(n, fps);
            tcs.push(tc);
            audio.extend(e.frame(tc));
        }
        (audio, tcs)
    }

    #[test]
    fn bits_roundtrip_and_parity() {
        for fps in [24, 25, 30] {
            for total in [0u64, 1, 24, 86_399 * fps as u64 + fps as u64 - 1, 3600 * fps as u64 * 13 + 5] {
                let tc = Tc::from_frames(total, fps);
                let bits = frame_bits(tc, fps);
                assert_eq!(parse_bits(&bits), Some(tc));
                assert_eq!(bits.iter().filter(|b| !**b).count() % 2, 0, "even zeros keeps the polarity");
                assert_eq!(tc.to_frames(fps), total % (24 * 3600 * fps as u64));
            }
        }
        assert_eq!(Tc::from_frames(25 * 3661 + 7, 25).text(), "01:01:01:07");
    }

    #[test]
    fn decodes_what_it_encodes_at_common_rates() {
        for (rate, fps) in [(48_000, 25), (48_000, 30), (44_100, 25), (44_100, 24), (96_000, 30)] {
            let (audio, tcs) = stream(rate, fps, 25 * 60 * 5 + 3, 90);
            let mut d = Decoder::new(rate, fps);
            let got = d.push(&audio);
            assert!(got.len() >= 85, "{rate}/{fps}: decoded {} of {}", got.len(), tcs.len());
            // frames come out in order and without gaps
            for w in got.windows(2) {
                assert_eq!(w[1].to_frames(fps), w[0].to_frames(fps) + 1, "{rate}/{fps}: {:?} → {:?}", w[0], w[1]);
            }
            // the last cell of a frame completes with the next frame's first edge, so the final frame is still pending
            assert_eq!(got.last().unwrap(), &tcs[tcs.len() - 2], "{rate}/{fps}");
        }
    }

    #[test]
    fn tolerates_low_level_dc_offset_noise_and_speed_drift() {
        let (mut audio, tcs) = stream(48_000, 25, 1000, 100);
        let mut seed = 12345u32;
        for s in audio.iter_mut() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed >> 16) as f32 / 65535.0 - 0.5) * 0.04;
            *s = *s * 0.05 + 0.02 + noise; // −26 dB, DC offset, noise
        }
        let mut d = Decoder::new(48_000, 25);
        let got = d.push(&audio);
        assert!(got.len() >= 90, "decoded {}", got.len());
        assert_eq!(got.last().unwrap(), &tcs[tcs.len() - 2]);
        // source running 3 % fast (a slightly different clock)
        let (fast, tcs) = stream(48_000 + 1440, 25, 500, 100);
        let got = Decoder::new(48_000, 25).push(&fast);
        assert!(got.len() >= 90 && got.last() == Some(&tcs[tcs.len() - 2]), "drift: {} frames", got.len());
    }

    #[test]
    fn silence_and_music_do_not_decode() {
        let mut d = Decoder::new(48_000, 25);
        assert!(d.push(&vec![0.0; 48_000]).is_empty());
        assert!(d.push(&sine(48_000, 440.0, 2.0)).is_empty());
        let mut seed = 1u32;
        let noise: Vec<f32> = (0..96_000).map(|_| { seed = seed.wrapping_mul(1103515245).wrapping_add(12345); (seed >> 16) as f32 / 32768.0 - 1.0 }).collect();
        assert!(d.push(&noise).is_empty());
    }

    #[test]
    fn resyncs_after_a_dropout() {
        let (a, _) = stream(48_000, 25, 100, 40);
        let (b, tcs) = stream(48_000, 25, 200, 40);
        let mut all = a;
        all.extend(vec![0.0; 24_000]);
        all.extend(b);
        let got = Decoder::new(48_000, 25).push(&all);
        assert_eq!(got.last().unwrap(), &tcs[tcs.len() - 2]);
    }
}
