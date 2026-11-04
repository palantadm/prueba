//! Audio-reactive analysis: smoothed level, bass / mid / treble and beat + tempo detection from one input channel.
//! Pure (no device): feed it mono samples at any sample rate.

use std::collections::VecDeque;

/// One analysis result every [`BLOCK_S`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bands {
    /// 0..1 (−60..0 dBFS mapped), smoothed.
    pub level: f32,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    /// A beat was detected in this block.
    pub beat: bool,
    /// Tempo estimate in BPM (0 until enough beats were seen).
    pub bpm: f32,
}

pub const BLOCK_S: f32 = 0.01;

fn to_unit(rms: f32) -> f32 {
    if rms < 1e-6 {
        return 0.0;
    }
    ((20.0 * rms.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

struct OnePole {
    a: f32,
    y: f32,
}

impl OnePole {
    fn lowpass(rate: f32, fc: f32) -> Self {
        OnePole { a: 1.0 - (-2.0 * std::f32::consts::PI * fc / rate).exp(), y: 0.0 }
    }
    fn run(&mut self, x: f32) -> f32 {
        self.y += self.a * (x - self.y);
        self.y
    }
}

pub struct Analyzer {
    rate: f32,
    block: usize,
    fill: usize,
    lp_bass: OnePole,
    lp_treble: OnePole,
    sums: [f32; 4],
    smooth: [f32; 4],
    // beat detection on the bass energy
    energy: VecDeque<f32>,
    since_beat: f32,
    last_beat_t: Option<f32>,
    clock: f32,
    ibis: VecDeque<f32>,
    bpm: f32,
}

impl Analyzer {
    pub fn new(rate: u32) -> Self {
        let rate = rate.max(8000) as f32;
        Analyzer {
            rate,
            block: ((rate * BLOCK_S) as usize).max(16),
            fill: 0,
            lp_bass: OnePole::lowpass(rate, 200.0),
            lp_treble: OnePole::lowpass(rate, 3500.0),
            sums: [0.0; 4],
            smooth: [0.0; 4],
            energy: VecDeque::new(),
            since_beat: 1.0,
            last_beat_t: None,
            clock: 0.0,
            ibis: VecDeque::new(),
            bpm: 0.0,
        }
    }

    pub fn reset(&mut self) {
        *self = Analyzer::new(self.rate as u32);
    }

    /// Feed mono samples; returns one [`Bands`] per completed 10 ms block.
    pub fn push(&mut self, mono: &[f32], out: &mut Vec<Bands>) {
        for &x in mono {
            let low = self.lp_bass.run(x);
            let upto = self.lp_treble.run(x);
            let treble = x - upto;
            let mid = upto - low;
            self.sums[0] += x * x;
            self.sums[1] += low * low;
            self.sums[2] += mid * mid;
            self.sums[3] += treble * treble;
            self.fill += 1;
            if self.fill >= self.block {
                let n = self.fill as f32;
                let rms = [self.sums[0], self.sums[1], self.sums[2], self.sums[3]].map(|s| (s / n).sqrt());
                self.sums = [0.0; 4];
                self.fill = 0;
                out.push(self.block_done(rms));
            }
        }
    }

    fn block_done(&mut self, rms: [f32; 4]) -> Bands {
        self.clock += BLOCK_S;
        self.since_beat += BLOCK_S;
        for i in 0..4 {
            let v = to_unit(rms[i]);
            // fast attack, slower release: lights should punch in and fade out
            let k = if v > self.smooth[i] { 0.6 } else { 0.08 };
            self.smooth[i] += (v - self.smooth[i]) * k;
        }
        // beat = bass energy well above its own recent average
        let e = rms[1];
        self.energy.push_back(e);
        if self.energy.len() > 100 {
            self.energy.pop_front();
        }
        let avg = self.energy.iter().sum::<f32>() / self.energy.len() as f32;
        let mut beat = false;
        if self.energy.len() > 30 && e > avg * 1.5 && e > 0.008 && self.since_beat > 0.25 {
            beat = true;
            self.since_beat = 0.0;
            if let Some(prev) = self.last_beat_t {
                let ibi = self.clock - prev;
                if (0.28..=1.5).contains(&ibi) {
                    self.ibis.push_back(ibi);
                    if self.ibis.len() > 12 {
                        self.ibis.pop_front();
                    }
                    self.bpm = self.tempo();
                }
            }
            self.last_beat_t = Some(self.clock);
        }
        // tempo is stale after a long silence
        if self.since_beat > 3.0 {
            self.bpm = 0.0;
            self.ibis.clear();
        }
        Bands { level: self.smooth[0], bass: self.smooth[1], mid: self.smooth[2], treble: self.smooth[3], beat, bpm: self.bpm }
    }

    fn tempo(&self) -> f32 {
        if self.ibis.len() < 3 {
            return 0.0;
        }
        let mut v: Vec<f32> = self.ibis.iter().copied().collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med = v[v.len() / 2];
        let mut bpm = 60.0 / med;
        while bpm < 70.0 {
            bpm *= 2.0;
        }
        while bpm > 190.0 {
            bpm /= 2.0;
        }
        bpm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: u32 = 48_000;

    fn run(a: &mut Analyzer, samples: &[f32]) -> Vec<Bands> {
        let mut out = Vec::new();
        for c in samples.chunks(480) {
            a.push(c, &mut out);
        }
        out
    }

    fn tone(hz: f32, secs: f32, amp: f32) -> Vec<f32> {
        (0..(R as f32 * secs) as usize).map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / R as f32).sin()).collect()
    }

    #[test]
    fn bands_separate_bass_from_treble() {
        let mut a = Analyzer::new(R);
        let last = *run(&mut a, &tone(80.0, 1.0, 0.5)).last().unwrap();
        assert!(last.bass > 0.6 && last.bass > last.treble + 0.4, "{last:?}");
        let mut a = Analyzer::new(R);
        let last = *run(&mut a, &tone(8000.0, 1.0, 0.5)).last().unwrap();
        assert!(last.treble > 0.6 && last.treble > last.bass + 0.4, "{last:?}");
        let mut a = Analyzer::new(R);
        let last = *run(&mut a, &tone(1000.0, 1.0, 0.5)).last().unwrap();
        assert!(last.mid > last.bass && last.mid > last.treble, "{last:?}");
    }

    #[test]
    fn level_follows_amplitude_and_silence_is_zero() {
        let mut a = Analyzer::new(R);
        let loud = run(&mut a, &tone(440.0, 0.5, 0.5)).last().unwrap().level;
        let quiet = run(&mut a, &tone(440.0, 0.5, 0.02)).last().unwrap().level;
        let none = run(&mut a, &vec![0.0; R as usize]).last().unwrap().level;
        assert!(loud > quiet && quiet > none, "{loud} {quiet} {none}");
        assert!(none < 0.02);
    }

    fn kick_track(bpm: f32, secs: f32) -> Vec<f32> {
        let n = (R as f32 * secs) as usize;
        let period = (R as f32 * 60.0 / bpm) as usize;
        (0..n)
            .map(|i| {
                let t = (i % period) as f32 / R as f32;
                0.8 * (2.0 * std::f32::consts::PI * 55.0 * t).sin() * (-t * 25.0).exp() + 0.01 * ((i * 7919) % 17) as f32 / 17.0 * 0.1
            })
            .collect()
    }

    #[test]
    fn detects_tempo_of_a_kick_pattern() {
        for bpm in [100.0f32, 120.0, 140.0] {
            let mut a = Analyzer::new(R);
            let out = run(&mut a, &kick_track(bpm, 10.0));
            let beats = out.iter().filter(|b| b.beat).count() as f32;
            let expect = 10.0 * bpm / 60.0;
            assert!((beats - expect).abs() <= 2.0, "{bpm}: {beats} beats, expected ≈ {expect}");
            let est = out.last().unwrap().bpm;
            assert!((est - bpm).abs() < 4.0, "{bpm}: estimated {est}");
        }
    }

    #[test]
    fn steady_tone_has_no_beats_and_other_rates_work() {
        let mut a = Analyzer::new(44_100);
        let samples: Vec<f32> = (0..44_100 * 3).map(|i| 0.3 * (i as f32 * 0.05).sin()).collect();
        let mut out = Vec::new();
        a.push(&samples, &mut out);
        assert!(out.iter().filter(|b| b.beat).count() <= 1);
        assert!((out.len() as i32 - 300).abs() <= 2, "10 ms blocks: {}", out.len());
    }
}
