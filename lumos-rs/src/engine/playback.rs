//! Cuelist playback: tracking and fade math.

use crate::model::*;

/// Value of one attribute during a cue transition.
/// `elapsed` is seconds since GO. Returns the value; rising uses `fade_in`, falling `fade_out`.
/// Discrete attributes snap to the target when the delay has passed.
pub fn fade_value(from: f32, to: f32, elapsed: f32, delay: f32, fade_in: f32, fade_out: f32, discrete: bool) -> f32 {
    let e = elapsed - delay;
    if e < 0.0 {
        return from;
    }
    if discrete {
        return to;
    }
    let fade = if to > from { fade_in } else { fade_out };
    if fade <= 0.0 {
        return to;
    }
    let t = (e / fade).clamp(0.0, 1.0);
    from + (to - from) * t
}

/// Cue ordering key helper.
pub fn same_number(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

pub fn index_of(cl: &Cuelist, number: f64) -> Option<usize> {
    cl.cues.iter().position(|c| same_number(c.number, number))
}

pub fn sort_cues(cl: &mut Cuelist) {
    cl.cues.sort_by(|a, b| a.number.partial_cmp(&b.number).unwrap_or(std::cmp::Ordering::Equal));
}

/// State of the cuelist at cue `idx` (cues must be sorted).
/// Tracking lists accumulate changes since the last `block` cue; cue-only lists use the cue alone.
pub fn tracked_state(cl: &Cuelist, idx: usize) -> PerFixture {
    let mut out = PerFixture::new();
    let Some(cue) = cl.cues.get(idx) else { return out };
    let start = if cl.tracking {
        (0..=idx).rev().find(|&i| cl.cues[i].block).unwrap_or(0)
    } else {
        idx
    };
    for c in &cl.cues[start..=idx] {
        for (f, attrs) in &c.changes {
            let e = out.entry(*f).or_default();
            for (a, v) in attrs {
                e.insert(a.clone(), *v);
            }
        }
    }
    let _ = cue;
    out
}

/// A running (or released) cuelist layer.
pub struct CueRun {
    pub cuelist: u32,
    pub state: PlayState,
    pub current: Option<f64>,
    pub from: PerFixture,
    pub to: PerFixture,
    /// Attributes removed from the layer once the fade completes.
    pub drop: Vec<(u32, String)>,
    pub start: f64,
    pub delay: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub paused_at: Option<f64>,
    pub go_seq: u64,
    pub values: PerFixture,
    pub follow_at: Option<f64>,
    pub progress: f32,
    pub settled: bool,
    pub releasing: bool,
}

impl CueRun {
    pub fn new(cuelist: u32) -> Self {
        CueRun {
            cuelist,
            state: PlayState::Idle,
            current: None,
            from: PerFixture::new(),
            to: PerFixture::new(),
            drop: vec![],
            start: 0.0,
            delay: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            paused_at: None,
            go_seq: 0,
            values: PerFixture::new(),
            follow_at: None,
            progress: 1.0,
            settled: true,
            releasing: false,
        }
    }

    pub fn clock(&self, now: f64) -> f64 {
        self.paused_at.unwrap_or(now)
    }

    /// Recompute `values` for time `now`. `discrete(fixture, attr)` tells which attributes snap.
    pub fn update(&mut self, now: f64, discrete: &dyn Fn(u32, &str) -> bool) {
        if self.settled {
            return;
        }
        let elapsed = (self.clock(now) - self.start) as f32;
        for (f, attrs) in &self.to {
            let from_f = self.from.get(f);
            let vals = self.values.entry(*f).or_default();
            for (a, &tv) in attrs {
                let fv = from_f.and_then(|m| m.get(a)).copied().unwrap_or(tv);
                vals.insert(
                    a.clone(),
                    fade_value(fv, tv, elapsed, self.delay, self.fade_in, self.fade_out, discrete(*f, a)),
                );
            }
        }
        let max_fade = self.fade_in.max(self.fade_out);
        let e = elapsed - self.delay;
        self.progress = if e < 0.0 {
            0.0
        } else if max_fade <= 0.0 {
            1.0
        } else {
            (e / max_fade).clamp(0.0, 1.0)
        };
        if self.progress >= 1.0 {
            for (f, a) in self.drop.drain(..) {
                if let Some(m) = self.values.get_mut(&f) {
                    m.remove(&a);
                }
            }
            self.values.retain(|_, m| !m.is_empty());
            self.settled = true;
            if self.releasing {
                self.state = PlayState::Idle;
                self.current = None;
                self.releasing = false;
                self.values.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(n: f64, f: u32, a: &str, v: f32) -> Cue {
        let mut ch = PerFixture::new();
        ch.entry(f).or_default().insert(a.into(), v);
        Cue { refs: Default::default(), timing: Default::default(), curve: Default::default(), stagger: 0.0, notes: String::new(), number: n, label: String::new(), fade_in: 2.0, fade_out: 2.0, delay: 0.0, follow: None, timecode: None, changes: ch, block: false }
    }

    #[test]
    fn fades() {
        assert_eq!(fade_value(0.0, 1.0, 0.0, 0.0, 2.0, 2.0, false), 0.0);
        assert!((fade_value(0.0, 1.0, 1.0, 0.0, 2.0, 2.0, false) - 0.5).abs() < 1e-6);
        assert_eq!(fade_value(0.0, 1.0, 5.0, 0.0, 2.0, 2.0, false), 1.0);
        // delay holds the old value
        assert_eq!(fade_value(0.2, 1.0, 0.5, 1.0, 2.0, 2.0, false), 0.2);
        // rising uses fade_in, falling uses fade_out
        assert!((fade_value(0.0, 1.0, 1.0, 0.0, 4.0, 1.0, false) - 0.25).abs() < 1e-6);
        assert_eq!(fade_value(1.0, 0.0, 1.0, 0.0, 4.0, 1.0, false), 0.0);
        // zero fade is instant, discrete snaps after delay
        assert_eq!(fade_value(0.0, 1.0, 0.0, 0.0, 0.0, 0.0, false), 1.0);
        assert_eq!(fade_value(0.0, 0.7, 0.0, 0.0, 9.0, 9.0, true), 0.7);
        assert_eq!(fade_value(0.0, 0.7, 0.5, 1.0, 9.0, 9.0, true), 0.0);
    }

    #[test]
    fn tracking_accumulates_and_blocks() {
        let mut cl = Cuelist { auto_mark: false, id: 1, name: "t".into(), cues: vec![cue(1.0, 1, "dimmer", 1.0), cue(2.0, 1, "red", 0.5), cue(3.0, 2, "dimmer", 0.3)], looped: false, tracking: true, priority: 0 };
        let s = tracked_state(&cl, 2);
        assert_eq!(s[&1]["dimmer"], 1.0);
        assert_eq!(s[&1]["red"], 0.5);
        assert_eq!(s[&2]["dimmer"], 0.3);
        cl.cues[2].block = true;
        let s = tracked_state(&cl, 2);
        assert!(!s.contains_key(&1));
        cl.tracking = false;
        let s = tracked_state(&cl, 1);
        assert!(!s[&1].contains_key("dimmer"));
        assert_eq!(s[&1]["red"], 0.5);
    }

    #[test]
    fn run_update_reaches_target() {
        let mut r = CueRun::new(1);
        r.to.entry(1).or_default().insert("dimmer".into(), 1.0);
        r.from.entry(1).or_default().insert("dimmer".into(), 0.0);
        r.fade_in = 2.0;
        r.fade_out = 2.0;
        r.settled = false;
        r.state = PlayState::Running;
        r.update(1.0, &|_, _| false);
        assert!((r.values[&1]["dimmer"] - 0.5).abs() < 1e-5);
        assert!((r.progress - 0.5).abs() < 1e-5);
        r.update(3.0, &|_, _| false);
        assert_eq!(r.values[&1]["dimmer"], 1.0);
        assert!(r.settled);
    }
}
