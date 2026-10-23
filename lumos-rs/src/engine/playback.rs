//! Cuelist playback: tracking and fade math.

use crate::model::*;

/// Value of one attribute during a cue transition.
/// `elapsed` is seconds since GO. Returns the value; rising uses `fade_in`, falling `fade_out`.
/// Discrete attributes snap to the target when the delay has passed.
#[cfg(test)]
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

/// Attribute group name used by `Cue.timing` keys (`intensity|position|color|beam|control`).
pub fn attr_group(attr: &str) -> &'static str {
    ATTRS.iter().find(|(n, _, _)| *n == attr).map(|(_, g, _)| *g).unwrap_or("control")
}

/// Shape the 0..1 fade fraction.
pub fn curve_apply(c: FadeCurve, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match c {
        FadeCurve::Linear => t,
        FadeCurve::SCurve => t * t * (3.0 - 2.0 * t),
        FadeCurve::Square => t * t,
        FadeCurve::InvSquare => 1.0 - (1.0 - t) * (1.0 - t),
        FadeCurve::Sine => (1.0 - (t * std::f32::consts::PI).cos()) * 0.5,
    }
}

/// Like [`fade_value`] with a fade curve.
#[allow(clippy::too_many_arguments)]
pub fn fade_value_c(from: f32, to: f32, elapsed: f32, delay: f32, fade_in: f32, fade_out: f32, discrete: bool, curve: FadeCurve) -> f32 {
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
    let t = curve_apply(curve, (e / fade).clamp(0.0, 1.0));
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
    /// Per attribute-group timing overrides of the running cue.
    pub timing: std::collections::BTreeMap<String, AttrTiming>,
    pub curve: FadeCurve,
    pub stagger: f32,
    /// Attributes pre-positioned by auto-mark (fade with `mark_fade`, no delay).
    pub marks: std::collections::BTreeSet<(u32, String)>,
    pub mark_fade: f32,
    /// Earliest attribute start / latest attribute end (seconds since GO).
    pub t_min: f32,
    pub t_end: f32,
    /// `Some(p)`: a manual crossfader drives the fade (0..1) instead of time.
    pub manual: Option<f32>,
    /// State to return to if a manual transition is cancelled.
    pub prev_current: Option<f64>,
    pub prev_values: PerFixture,
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
            timing: Default::default(),
            curve: FadeCurve::Linear,
            stagger: 0.0,
            marks: Default::default(),
            mark_fade: 1.0,
            t_min: 0.0,
            t_end: 0.0,
            manual: None,
            prev_current: None,
            prev_values: PerFixture::new(),
        }
    }

    pub fn clock(&self, now: f64) -> f64 {
        self.paused_at.unwrap_or(now)
    }

    /// (delay, fade_in, fade_out) of one attribute: marks, then group timing, then the cue's own.
    fn attr_timing(&self, f: u32, a: &str, rank: usize) -> (f32, f32, f32) {
        if self.marks.contains(&(f, a.to_string())) {
            return (0.0, self.mark_fade, self.mark_fade);
        }
        let extra = self.stagger.max(0.0) * rank as f32;
        match self.timing.get(attr_group(a)) {
            Some(t) => (t.delay.max(0.0) + extra, t.fade.max(0.0), t.fade.max(0.0)),
            None => (self.delay + extra, self.fade_in, self.fade_out),
        }
    }

    /// Recompute `t_min`/`t_end` from the cue timing. Call after the run fields are set.
    pub fn compute_span(&mut self) {
        let mut lo = f32::MAX;
        let mut hi = 0.0f32;
        for (rank, (f, attrs)) in self.to.iter().enumerate() {
            for a in attrs.keys() {
                let (d, fi, fo) = self.attr_timing(*f, a, rank);
                lo = lo.min(d);
                hi = hi.max(d + fi.max(fo));
            }
        }
        if lo == f32::MAX {
            lo = self.delay;
            hi = self.delay + self.fade_in.max(self.fade_out);
        }
        self.t_min = lo;
        self.t_end = hi;
    }

    /// Recompute `values` for time `now`. `discrete(fixture, attr)` tells which attributes snap.
    pub fn update(&mut self, now: f64, discrete: &dyn Fn(u32, &str) -> bool) {
        if self.settled {
            return;
        }
        if self.t_end <= 0.0 {
            self.compute_span();
        }
        let elapsed = (self.clock(now) - self.start) as f32;
        let manual = self.manual;
        let mut out: Vec<(u32, String, f32)> = Vec::new();
        for (rank, (f, attrs)) in self.to.iter().enumerate() {
            let from_f = self.from.get(f);
            for (a, &tv) in attrs {
                let fv = from_f.and_then(|m| m.get(a)).copied().unwrap_or(tv);
                let v = match manual {
                    Some(p) => {
                        if discrete(*f, a) {
                            if p >= 0.5 {
                                tv
                            } else {
                                fv
                            }
                        } else {
                            fv + (tv - fv) * curve_apply(self.curve, p)
                        }
                    }
                    None => {
                        let (d, fi, fo) = self.attr_timing(*f, a, rank);
                        fade_value_c(fv, tv, elapsed, d, fi, fo, discrete(*f, a), self.curve)
                    }
                };
                out.push((*f, a.clone(), v));
            }
        }
        for (f, a, v) in out {
            self.values.entry(f).or_default().insert(a, v);
        }
        self.progress = match manual {
            Some(p) => p,
            None => {
                let e = elapsed - self.t_min;
                let span = self.t_end - self.t_min;
                if e < 0.0 {
                    0.0
                } else if span <= 0.0 {
                    1.0
                } else {
                    (e / span).clamp(0.0, 1.0)
                }
            }
        };
        if self.progress >= 1.0 {
            self.manual = None;
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
