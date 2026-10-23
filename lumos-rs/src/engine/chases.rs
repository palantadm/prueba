//! Chases: beat-clocked sequences of looks, layered like scenes.
//!
//! Render layer order: cuelists → scenes → **chases** → scene morph → programmer → effects → masters.
//! * A step's content = the referenced scene's look (if any) overlaid by the step's own values; palette
//!   links in `refs` are resolved live (edit the palette, the chase follows). If a palette is gone the
//!   stored absolute value is the fallback.
//! * `dimmer` is HTP against what is below (× chase level); every other attribute is LTP (the most
//!   recently started chase wins).
//! * Time is counted in beats: `beats += dt · bpm / 60`, so a BPM change (tap tempo, speed master)
//!   never makes a step jump. Step *i* lasts `hold` beats; its `fade` is the fraction of that hold used to
//!   crossfade *into* the step from the previous one (0 = snap, 1 = continuous).
//! * Attributes present in only one side of a crossfade: `dimmer` fades (1−t / t); any other attribute
//!   is applied while its own step is current and released when it isn't.
//! * Discrete attributes (named capabilities) snap at t ≥ 0.5.

use super::speed::clamp_bpm;
use super::{clamp01, next_id, Engine};
use crate::model::*;
use std::collections::HashMap;

/// Most steps a single update may advance (guards against a stalled clock + tiny holds).
const MAX_ADVANCE: usize = 256;
const MIN_HOLD: f32 = 0.01;
const MAX_HOLD: f32 = 1024.0;

/// Value of palette `pal` for `fixture`/`attr` (per-fixture override first, then the universal value).
pub fn resolve_ref(palettes: &[Palette], fixture: u32, attr: &str, pal: u32) -> Option<f32> {
    let p = palettes.iter().find(|p| p.id == pal)?;
    p.per_fixture.get(&fixture).and_then(|m| m.get(attr)).or_else(|| p.values.get(attr)).copied()
}

/// `values` with every palette link in `refs` replaced by the palette's current value.
pub fn resolve_with_refs(values: &PerFixture, refs: &PerFixtureRefs, palettes: &[Palette]) -> PerFixture {
    let mut out = values.clone();
    for (f, m) in refs {
        for (attr, pal) in m {
            if let Some(v) = resolve_ref(palettes, *f, attr, *pal) {
                out.entry(*f).or_default().insert(attr.clone(), v);
            }
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct ChaseRun {
    pub step: usize,
    /// Step the current one crossfades from (== `step` when there is nothing to fade from).
    pub prev: usize,
    /// Beats elapsed in the current step.
    pub beats: f64,
    /// Engine time of the last update.
    pub last: f64,
    /// Travel direction for `Bounce` (+1 / −1).
    pub dir: i32,
    /// Start order (higher = more recent = wins LTP).
    pub seq: u64,
    rng: u64,
    /// Started by holding a flash button: stops when released.
    pub by_flash: bool,
}

#[derive(Default)]
pub struct ChasesState {
    pub runs: HashMap<u32, ChaseRun>,
    seq: u64,
}

impl ChasesState {
    pub fn any_running(&self) -> bool {
        !self.runs.is_empty()
    }
    pub fn is_running(&self, id: u32) -> bool {
        self.runs.contains_key(&id)
    }
}

fn xorshift(s: &mut u64) -> u64 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *s = x;
    x
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Index of the step that follows `step` for the chase direction (updates bounce direction / rng).
fn next_step(dir_mode: ChaseDir, n: usize, step: usize, dir: &mut i32, rng: &mut u64) -> usize {
    if n <= 1 {
        return 0;
    }
    match dir_mode {
        ChaseDir::Forward => (step + 1) % n,
        ChaseDir::Backward => (step + n - 1) % n,
        ChaseDir::Bounce => {
            if *dir >= 0 {
                if step + 1 < n {
                    step + 1
                } else {
                    *dir = -1;
                    step - 1
                }
            } else if step > 0 {
                step - 1
            } else {
                *dir = 1;
                1
            }
        }
        ChaseDir::Random => {
            let mut k = (xorshift(rng) % (n as u64 - 1)) as usize;
            if k >= step {
                k += 1;
            }
            k
        }
    }
}

/// Crossfade two step looks. See the module docs for the rules.
pub fn blend(a: &PerFixture, b: &PerFixture, t: f32, discrete: &dyn Fn(u32, &str) -> bool) -> PerFixture {
    if t >= 1.0 {
        return b.clone();
    }
    let mut out = PerFixture::new();
    let fixtures: std::collections::BTreeSet<u32> = a.keys().chain(b.keys()).copied().collect();
    for f in fixtures {
        let (ma, mb) = (a.get(&f), b.get(&f));
        let attrs: std::collections::BTreeSet<&String> =
            ma.into_iter().flat_map(|m| m.keys()).chain(mb.into_iter().flat_map(|m| m.keys())).collect();
        let o = out.entry(f).or_default();
        for attr in attrs {
            let va = ma.and_then(|m| m.get(attr)).copied();
            let vb = mb.and_then(|m| m.get(attr)).copied();
            let v = if attr == "dimmer" {
                match (va, vb) {
                    (Some(x), Some(y)) => lerp(x, y, t),
                    (Some(x), None) => x * (1.0 - t),
                    (None, Some(y)) => y * t,
                    (None, None) => continue,
                }
            } else if discrete(f, attr) {
                match (va, vb) {
                    (Some(x), Some(y)) => {
                        if t < 0.5 {
                            x
                        } else {
                            y
                        }
                    }
                    (_, Some(y)) => y,
                    (Some(_), None) | (None, None) => continue,
                }
            } else {
                match (va, vb) {
                    (Some(x), Some(y)) => lerp(x, y, t),
                    (_, Some(y)) => y,
                    (Some(_), None) | (None, None) => continue,
                }
            };
            o.insert(attr.clone(), v);
        }
    }
    out
}

impl Engine {
    pub(super) fn require_chase(&self, id: u32) -> Result<&Chase, String> {
        self.show.chases.iter().find(|c| c.id == id).ok_or_else(|| format!("Chase {id} does not exist"))
    }

    /// BPM a chase currently runs at (speed master wins over its own value).
    pub(super) fn chase_bpm_eff(&self, c: &Chase) -> f32 {
        clamp_bpm(self.bpm_for(c.speed_master, c.bpm))
    }

    // ───────────── editing ─────────────

    pub(super) fn chase_save(&mut self, mut chase: Chase) -> Result<(), String> {
        if chase.id == 0 {
            chase.id = next_id(self.show.chases.iter().map(|c| c.id));
        }
        if chase.name.trim().is_empty() {
            chase.name = format!("Chase {}", chase.id);
        }
        chase.bpm = clamp_bpm(chase.bpm);
        for s in &mut chase.steps {
            s.hold = if s.hold.is_finite() { s.hold.clamp(MIN_HOLD, MAX_HOLD) } else { 1.0 };
            s.fade = clamp01(s.fade);
            for attrs in s.values.values_mut() {
                for v in attrs.values_mut() {
                    *v = clamp01(*v);
                }
            }
        }
        let (id, n) = (chase.id, chase.steps.len());
        match self.show.chases.iter_mut().find(|c| c.id == id) {
            Some(c) => *c = chase,
            None => self.show.chases.push(chase),
        }
        if n == 0 {
            self.chases.runs.remove(&id);
        } else if let Some(r) = self.chases.runs.get_mut(&id) {
            if r.step >= n || r.prev >= n {
                r.step = 0;
                r.prev = 0;
                r.beats = 0.0;
            }
        }
        self.touch();
        Ok(())
    }

    pub(super) fn chase_remove(&mut self, id: u32) -> Result<(), String> {
        let before = self.show.chases.len();
        self.show.chases.retain(|c| c.id != id);
        if self.show.chases.len() == before {
            return Err(format!("Chase {id} does not exist"));
        }
        self.chases.runs.remove(&id);
        for x in self.show.executors.iter_mut() {
            if matches!(x.kind, ExecKind::Chase { chase } if chase == id) {
                x.kind = ExecKind::Empty;
            }
        }
        self.touch();
        Ok(())
    }

    pub(super) fn chase_set_bpm(&mut self, id: u32, bpm: f32) -> Result<(), String> {
        self.require_chase(id)?;
        let bpm = clamp_bpm(bpm);
        if let Some(c) = self.show.chases.iter_mut().find(|c| c.id == id) {
            c.bpm = bpm;
        }
        self.persist_quiet();
        Ok(())
    }

    pub(super) fn chase_tap_at(&mut self, id: u32, now: f64) -> Result<(), String> {
        self.require_chase(id)?;
        if let Some(bpm) = self.speed.tap_chase(id, now) {
            self.chase_set_bpm(id, bpm)?;
        }
        Ok(())
    }

    pub(super) fn chase_tap(&mut self, id: u32) -> Result<(), String> {
        let now = self.now();
        self.chase_tap_at(id, now)
    }

    // ───────────── playback ─────────────

    pub(super) fn chase_run_at(&mut self, id: u32, on: bool, now: f64) -> Result<(), String> {
        let c = self.require_chase(id)?;
        if !on {
            self.chases.runs.remove(&id);
            self.dirty = true;
            return Ok(());
        }
        if c.steps.is_empty() {
            return Err(format!("Chase '{}' has no steps", c.name));
        }
        if self.chases.runs.contains_key(&id) {
            return Ok(());
        }
        let start = if c.dir == ChaseDir::Backward { c.steps.len() - 1 } else { 0 };
        let dir = if c.dir == ChaseDir::Backward { -1 } else { 1 };
        self.chases.seq += 1;
        let seq = self.chases.seq;
        self.chases.runs.insert(
            id,
            ChaseRun { step: start, prev: start, beats: 0.0, last: now, dir, seq, rng: 0x9E37_79B9_7F4A_7C15 ^ (id as u64 + 1).wrapping_mul(0xA24B_AED4_963E_E407), by_flash: false },
        );
        self.dirty = true;
        Ok(())
    }

    pub(super) fn chase_run(&mut self, id: u32, on: bool) -> Result<(), String> {
        let now = self.now();
        self.chase_run_at(id, on, now)
    }

    pub(super) fn chase_toggle(&mut self, id: u32) -> Result<(), String> {
        let on = !self.chases.is_running(id);
        self.chase_run(id, on)
    }

    pub(super) fn chase_nudge(&mut self, id: u32, delta: i32) -> Result<(), String> {
        let n = self.require_chase(id)?.steps.len();
        let r = self.chases.runs.get_mut(&id).ok_or_else(|| format!("Chase {id} is not running"))?;
        if n == 0 {
            return Ok(());
        }
        r.prev = r.step;
        r.step = (r.step as i64 + delta as i64).rem_euclid(n as i64) as usize;
        r.beats = 0.0;
        self.dirty = true;
        Ok(())
    }

    /// Chase executor flash: run while held (only stops on release if the flash started it).
    pub(super) fn chase_flash(&mut self, id: u32, down: bool) -> Result<(), String> {
        let now = self.now();
        if down {
            if !self.chases.is_running(id) {
                self.chase_run_at(id, true, now)?;
                if let Some(r) = self.chases.runs.get_mut(&id) {
                    r.by_flash = true;
                }
            }
        } else if self.chases.runs.get(&id).map_or(false, |r| r.by_flash) {
            self.chase_run_at(id, false, now)?;
        }
        Ok(())
    }

    /// Advance every running chase to `now` (beat clock).
    pub(super) fn chase_update(&mut self, now: f64) {
        if self.chases.runs.is_empty() {
            return;
        }
        let mut changed = false;
        let ids: Vec<u32> = self.chases.runs.keys().copied().collect();
        for id in ids {
            let Some(c) = self.show.chases.iter().find(|c| c.id == id) else {
                self.chases.runs.remove(&id);
                continue;
            };
            let n = c.steps.len();
            if n == 0 {
                self.chases.runs.remove(&id);
                continue;
            }
            let bpm = self.chase_bpm_eff(c) as f64;
            let c = c.clone();
            let Some(r) = self.chases.runs.get_mut(&id) else { continue };
            let dt = (now - r.last).max(0.0);
            r.last = r.last.max(now);
            if dt <= 0.0 {
                continue;
            }
            r.beats += dt * bpm / 60.0;
            let mut guard = 0;
            loop {
                let hold = c.steps[r.step.min(n - 1)].hold.max(MIN_HOLD) as f64;
                if r.beats < hold {
                    break;
                }
                guard += 1;
                if guard > MAX_ADVANCE {
                    r.beats = 0.0;
                    break;
                }
                r.beats -= hold;
                r.prev = r.step;
                r.step = next_step(c.dir, n, r.step, &mut r.dir, &mut r.rng);
            }
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
    }

    // ───────────── render layer ─────────────

    /// Executor fader of a chase (1.0 without an executor).
    pub(super) fn chase_level(&self, id: u32) -> f32 {
        self.show
            .executors
            .iter()
            .find(|e| matches!(e.kind, ExecKind::Chase { chase } if chase == id))
            .and_then(|e| self.exec.get(&e.id))
            .map_or(1.0, |s| s.level)
    }

    /// Resolved look of step `idx` of `c`.
    pub(super) fn chase_step_look(&self, c: &Chase, idx: usize) -> PerFixture {
        let Some(s) = c.steps.get(idx) else { return PerFixture::new() };
        let mut out = PerFixture::new();
        if let Some(sc) = s.scene.and_then(|sid| self.show.scenes.iter().find(|x| x.id == sid)) {
            out = resolve_with_refs(&sc.values, &sc.refs, &self.show.palettes);
        }
        let own = resolve_with_refs(&s.values, &s.refs, &self.show.palettes);
        for (f, attrs) in own {
            let e = out.entry(f).or_default();
            for (a, v) in attrs {
                e.insert(a, v);
            }
        }
        out
    }

    /// Current (crossfaded) look of a running chase.
    pub(super) fn chase_look(&self, c: &Chase, r: &ChaseRun) -> PerFixture {
        let cur = self.chase_step_look(c, r.step);
        let s = &c.steps[r.step.min(c.steps.len() - 1)];
        if r.prev == r.step || s.fade <= 0.0 {
            return cur;
        }
        let span = (s.hold.max(MIN_HOLD) * s.fade) as f64;
        let t = (r.beats / span).clamp(0.0, 1.0) as f32;
        if t >= 1.0 {
            return cur;
        }
        let prev = self.chase_step_look(c, r.prev);
        let discrete = |f: u32, a: &str| self.patch.get(f).map_or(false, |p| p.is_discrete(a));
        blend(&prev, &cur, t, &discrete)
    }

    /// Layer every running chase into `merged` (oldest first so the newest wins LTP).
    pub(super) fn apply_chases(&self, merged: &mut PerFixture) {
        if self.chases.runs.is_empty() {
            return;
        }
        let mut order: Vec<(&u32, &ChaseRun)> = self.chases.runs.iter().collect();
        order.sort_by_key(|(_, r)| r.seq);
        for (id, r) in order {
            let Some(c) = self.show.chases.iter().find(|c| c.id == *id) else { continue };
            if c.steps.is_empty() {
                continue;
            }
            let level = self.chase_level(*id);
            for (f, attrs) in self.chase_look(c, r) {
                let e = merged.entry(f).or_default();
                for (a, v) in attrs {
                    if a == "dimmer" {
                        let nv = v * level;
                        let cur = e.entry(a).or_insert(0.0);
                        if nv > *cur {
                            *cur = nv;
                        }
                    } else if level > 0.0 {
                        e.insert(a, v);
                    }
                }
            }
        }
    }

    pub(super) fn chase_rt(&self) -> Vec<ChaseRt> {
        self.show
            .chases
            .iter()
            .map(|c| {
                let r = self.chases.runs.get(&c.id);
                ChaseRt { id: c.id, running: r.is_some(), step: r.map_or(0, |r| r.step as u32), bpm: self.chase_bpm_eff(c) }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::super::*;

    fn step(f: u32, dimmer: f32, hold: f32, fade: f32) -> ChaseStep {
        let mut v = PerFixture::new();
        v.entry(f).or_default().insert("dimmer".into(), dimmer);
        ChaseStep { values: v, refs: Default::default(), scene: None, hold, fade }
    }

    fn two_dimmers() -> Engine {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 3);
        e
    }

    fn save(e: &mut Engine, steps: Vec<ChaseStep>, dir: ChaseDir, bpm: f32) {
        e.handle(Command::ChaseSave { chase: Chase { id: 0, name: String::new(), steps, bpm, speed_master: None, dir } }).unwrap();
    }

    fn out(e: &mut Engine, now: f64) -> [u8; 512] {
        e.chase_update(now);
        e.render(now);
        **e.frames.get(&1).unwrap()
    }

    #[test]
    fn steps_advance_on_the_beat_clock() {
        let mut e = two_dimmers();
        save(&mut e, vec![step(1, 1.0, 1.0, 0.0), step(2, 1.0, 1.0, 0.0)], ChaseDir::Forward, 120.0);
        assert_eq!(e.show.chases[0].name, "Chase 1");
        e.chase_run_at(1, true, 0.0).unwrap();
        let f = out(&mut e, 0.1);
        assert_eq!((f[0], f[1]), (255, 0));
        // 120 BPM, hold 1 beat → 0.5 s per step
        let f = out(&mut e, 0.6);
        assert_eq!((f[0], f[1]), (0, 255));
        let f = out(&mut e, 1.1);
        assert_eq!((f[0], f[1]), (255, 0));
        assert_eq!(e.chase_rt()[0].step, 0);
        assert!(e.chase_rt()[0].running);
        // stopping releases the output
        e.chase_run_at(1, false, 1.2).unwrap();
        let f = out(&mut e, 1.3);
        assert_eq!((f[0], f[1]), (0, 0));
        assert!(!e.chase_rt()[0].running);
    }

    #[test]
    fn bpm_change_does_not_jump_the_step() {
        let mut e = two_dimmers();
        save(&mut e, vec![step(1, 1.0, 2.0, 0.0), step(2, 1.0, 2.0, 0.0)], ChaseDir::Forward, 60.0);
        e.chase_run_at(1, true, 0.0).unwrap();
        out(&mut e, 1.0); // 1 beat in
        e.handle(Command::ChaseBpm { id: 1, bpm: 240.0 }).unwrap();
        let f = out(&mut e, 1.25); // +1 beat at 240 BPM → step 0 ends exactly
        assert_eq!((f[0], f[1]), (0, 255));
        assert_eq!(e.show.chases[0].bpm, 240.0);
    }

    #[test]
    fn speed_master_beats_own_bpm_and_tap_sets_own() {
        let mut e = two_dimmers();
        save(&mut e, vec![step(1, 1.0, 1.0, 0.0), step(2, 1.0, 1.0, 0.0)], ChaseDir::Forward, 240.0);
        e.handle(Command::SpeedSave { master: SpeedMaster { id: 0, name: "M".into(), bpm: 60.0 } }).unwrap();
        let mut c = e.show.chases[0].clone();
        c.speed_master = Some(1);
        e.handle(Command::ChaseSave { chase: c }).unwrap();
        e.chase_run_at(1, true, 0.0).unwrap();
        // master 60 BPM → 1 s per step even though own bpm is 240
        let f = out(&mut e, 0.6);
        assert_eq!((f[0], f[1]), (255, 0));
        let f = out(&mut e, 1.1);
        assert_eq!((f[0], f[1]), (0, 255));
        e.handle(Command::SpeedSet { id: 1, bpm: 120.0 }).unwrap();
        assert_eq!(e.chase_rt()[0].bpm, 120.0);
        // removing the master falls back to the chase's own bpm
        e.handle(Command::SpeedRemove { id: 1 }).unwrap();
        assert_eq!(e.chase_rt()[0].bpm, 240.0);
        // tap tempo on the chase
        e.chase_tap_at(1, 5.0).unwrap();
        e.chase_tap_at(1, 5.5).unwrap();
        assert!((e.show.chases[0].bpm - 120.0).abs() < 0.01);
    }

    fn visited(e: &mut Engine, n: usize) -> Vec<usize> {
        let mut v = vec![];
        let mut t = 0.0;
        for _ in 0..n {
            e.chase_update(t);
            v.push(e.chases.runs[&1].step);
            t += 0.5;
        }
        v
    }

    #[test]
    fn directions() {
        let steps = || (1..=3).map(|f| step(f, 1.0, 1.0, 0.0)).collect::<Vec<_>>();
        let mut e = two_dimmers();
        save(&mut e, steps(), ChaseDir::Forward, 120.0);
        e.chase_run_at(1, true, 0.0).unwrap();
        assert_eq!(visited(&mut e, 7), vec![0, 1, 2, 0, 1, 2, 0]);

        let mut e = two_dimmers();
        save(&mut e, steps(), ChaseDir::Backward, 120.0);
        e.chase_run_at(1, true, 0.0).unwrap();
        assert_eq!(visited(&mut e, 7), vec![2, 1, 0, 2, 1, 0, 2]);

        let mut e = two_dimmers();
        save(&mut e, steps(), ChaseDir::Bounce, 120.0);
        e.chase_run_at(1, true, 0.0).unwrap();
        assert_eq!(visited(&mut e, 9), vec![0, 1, 2, 1, 0, 1, 2, 1, 0]);

        let mut e = two_dimmers();
        save(&mut e, steps(), ChaseDir::Random, 120.0);
        e.chase_run_at(1, true, 0.0).unwrap();
        let seq = visited(&mut e, 40);
        assert!(seq.windows(2).all(|w| w[0] != w[1]), "random never repeats the same step: {seq:?}");
        assert!(seq.iter().all(|s| *s < 3));
        assert!((0..3).all(|s| seq.contains(&s)));
    }

    #[test]
    fn crossfade_between_steps() {
        let mut e = two_dimmers();
        // step 1 fades in over its whole hold
        save(&mut e, vec![step(1, 1.0, 1.0, 0.0), step(2, 1.0, 1.0, 1.0)], ChaseDir::Forward, 120.0);
        e.chase_run_at(1, true, 0.0).unwrap();
        out(&mut e, 0.1);
        let f = out(&mut e, 0.75); // halfway through step 1: f1 1→0 and f2 0→1 at t = 0.5
        assert!((f[0] as i32 - 128).abs() <= 1 && (f[1] as i32 - 128).abs() <= 1, "{} {}", f[0], f[1]);
        let f = out(&mut e, 0.95); // t = 0.9
        assert!((f[0] as i32 - 26).abs() <= 2 && (f[1] as i32 - 230).abs() <= 2, "{} {}", f[0], f[1]);
    }

    #[test]
    fn partial_fade_completes_early() {
        let mut e = two_dimmers();
        save(&mut e, vec![step(1, 1.0, 1.0, 0.0), step(1, 0.0, 1.0, 0.5)], ChaseDir::Forward, 120.0);
        e.chase_run_at(1, true, 0.0).unwrap();
        out(&mut e, 0.1);
        let f = out(&mut e, 0.625); // 0.125 s into step 1 = 25 % of the hold → t = 0.5 of the fade
        assert!((f[0] as i32 - 128).abs() <= 1, "{}", f[0]);
        let f = out(&mut e, 0.8); // past the fade window
        assert_eq!(f[0], 0);
    }

    #[test]
    fn scene_and_palette_refs_resolve() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "4ch", 2);
        // scene: fixture 1 red 0.4 (+ dimmer 0.6)
        set(&mut e, 1, "red", 0.4);
        set(&mut e, 1, "dimmer", 0.6);
        e.handle(Command::SceneRecord { id: None, name: "S".into(), from_output: false, fixtures: None, merge: false, refs: true }).unwrap();
        e.handle(Command::ProgRelease { fixtures: None, attrs: None }).unwrap();
        // palette: red 0.9
        e.show.palettes.push(Palette { id: 1, name: "R".into(), kind: PaletteKind::Color, values: [("red".to_string(), 0.9)].into_iter().collect(), per_fixture: Default::default() });
        // step 0 = scene; step 1 = fixture 2 red linked to the palette (stored fallback 0.1)
        let mut s1 = step(2, 1.0, 1.0, 0.0);
        s1.values.get_mut(&2).unwrap().insert("red".into(), 0.1);
        s1.refs.entry(2).or_default().insert("red".into(), 1);
        let s0 = ChaseStep { values: Default::default(), refs: Default::default(), scene: Some(1), hold: 1.0, fade: 0.0 };
        save(&mut e, vec![s0, s1], ChaseDir::Forward, 120.0);
        e.chase_run_at(1, true, 0.0).unwrap();
        let f = out(&mut e, 0.1);
        // 4ch rgb-par: [dimmer, red, green, blue]; fixture 1 at 1..4 carries the scene, fixture 2 at 5..8 is dark
        assert_eq!((f[0], f[1]), (153, 102), "{:?}", &f[..8]);
        assert!(f[4..8].iter().all(|b| *b == 0));
        let f = out(&mut e, 0.6);
        assert_eq!((f[4], f[5]), (255, 230), "{:?}", &f[..8]); // palette 0.9 · 255
        assert_eq!(f[0], 0);
        // editing the palette changes the chase output (live link)
        e.show.palettes[0].values.insert("red".into(), 0.5);
        let f = out(&mut e, 0.62);
        assert_eq!(f[5], 128);
        // palette removed → stored fallback 0.1
        e.show.palettes.clear();
        let f = out(&mut e, 0.64);
        assert_eq!(f[5], 26);
    }

    #[test]
    fn nudge_and_errors() {
        let mut e = two_dimmers();
        save(&mut e, (1..=3).map(|f| step(f, 1.0, 100.0, 0.0)).collect(), ChaseDir::Forward, 120.0);
        assert!(e.handle(Command::ChaseNudge { id: 1, delta: 1 }).is_err(), "not running");
        assert!(e.handle(Command::ChaseRun { id: 9, on: true }).is_err());
        e.handle(Command::ChaseRun { id: 1, on: true }).unwrap();
        e.handle(Command::ChaseNudge { id: 1, delta: 1 }).unwrap();
        assert_eq!(e.chases.runs[&1].step, 1);
        e.handle(Command::ChaseNudge { id: 1, delta: -2 }).unwrap();
        assert_eq!(e.chases.runs[&1].step, 2); // wraps
        // empty chase cannot start; shrinking a running chase keeps it valid
        save(&mut e, vec![], ChaseDir::Forward, 120.0);
        assert!(e.handle(Command::ChaseRun { id: 2, on: true }).is_err());
        let mut c = e.show.chases[0].clone();
        c.steps.truncate(1);
        e.handle(Command::ChaseSave { chase: c }).unwrap();
        assert_eq!(e.chases.runs[&1].step, 0);
        e.handle(Command::ChaseRemove { id: 1 }).unwrap();
        assert!(!e.chases.is_running(1));
        assert!(e.handle(Command::ChaseRemove { id: 1 }).is_err());
    }

    #[test]
    fn executor_toggle_level_and_flash() {
        let mut e = two_dimmers();
        save(&mut e, vec![step(1, 1.0, 100.0, 0.0)], ChaseDir::Forward, 120.0);
        e.handle(Command::ExecSave { executor: Executor { id: 0, page: 0, slot: 0, label: "C".into(), kind: ExecKind::Chase { chase: 1 } } }).unwrap();
        let eid = e.show.executors[0].id;
        e.handle(Command::ExecGo { id: eid }).unwrap();
        assert!(e.chases.is_running(1));
        e.handle(Command::ExecLevel { id: eid, level: 0.5 }).unwrap();
        e.chase_update(0.0);
        e.render(0.0);
        assert_eq!(e.frames[&1][0], 128);
        let rt = e.build_rt();
        assert!(rt.executors[0].active);
        assert_eq!(rt.chases[0].running, true);
        e.handle(Command::ExecGo { id: eid }).unwrap(); // toggle off
        assert!(!e.chases.is_running(1));
        // flash = run while held
        e.handle(Command::ExecFlash { id: eid, down: true }).unwrap();
        assert!(e.chases.is_running(1));
        e.handle(Command::ExecFlash { id: eid, down: false }).unwrap();
        assert!(!e.chases.is_running(1));
        // flash does not stop a chase that was already running
        e.handle(Command::ExecGo { id: eid }).unwrap();
        e.handle(Command::ExecFlash { id: eid, down: true }).unwrap();
        e.handle(Command::ExecFlash { id: eid, down: false }).unwrap();
        assert!(e.chases.is_running(1));
        e.handle(Command::ExecBack { id: eid }).unwrap();
        assert!(!e.chases.is_running(1));
    }

    #[test]
    fn htp_with_cuelists_and_ltp_between_chases() {
        let mut e = two_dimmers();
        // programmer-free baseline: chase A holds f1 at 0.3; chase B holds f1 at 0.8 and f2 red
        save(&mut e, vec![step(1, 0.3, 100.0, 0.0)], ChaseDir::Forward, 120.0);
        save(&mut e, vec![step(1, 0.8, 100.0, 0.0)], ChaseDir::Forward, 120.0);
        e.handle(Command::ChaseRun { id: 1, on: true }).unwrap();
        e.handle(Command::ChaseRun { id: 2, on: true }).unwrap();
        e.chase_update(0.0);
        e.render(0.0);
        assert_eq!(e.frames[&1][0], 204); // dimmer HTP → 0.8
    }
}
