//! Scenes: recorded looks layered between the cuelists and the programmer.
//!
//! Layer order in the render: cuelists → scenes → scene morph → effects → programmer → masters.
//! * `dimmer` is HTP against what is below (scene dimmer × level).
//! * Continuous attributes are LTP blended by the scene level: `lerp(below, scene, level)` — the most
//!   recently activated scene wins and fading a scene crossfades colour/position too.
//! * Discrete attributes (those backed by a channel with named capabilities) snap at level ≥ 0.5.

use super::{clamp01, next_id, Engine};
use crate::model::*;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Debug)]
pub struct SceneRun {
    pub level: f32,
    pub from: f32,
    pub to: f32,
    pub start: f64,
    /// Seconds for the ramp `from → to`.
    pub dur: f32,
    /// Activation order (higher = more recent = wins LTP).
    pub seq: u64,
}

#[derive(Default)]
pub struct ScenesState {
    pub runs: HashMap<u32, SceneRun>,
    /// Scenes held at level 1 while a flash button is down.
    pub flash: BTreeSet<u32>,
    pub morph: Option<(u32, u32, f32)>,
    seq: u64,
}

impl ScenesState {
    pub fn ramping(&self) -> bool {
        self.runs.values().any(|r| r.level != r.to)
    }

    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    /// Effective level used by the render (flash forces 1.0).
    pub fn effective(&self, id: u32) -> f32 {
        if self.flash.contains(&id) {
            1.0
        } else {
            self.runs.get(&id).map_or(0.0, |r| r.level)
        }
    }

    pub fn is_active(&self, id: u32) -> bool {
        self.flash.contains(&id) || self.runs.get(&id).map_or(false, |r| r.level > 0.0 || r.to > 0.0)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Engine {
    pub(super) fn require_scene(&self, id: u32) -> Result<&Scene, String> {
        self.show.scenes.iter().find(|s| s.id == id).ok_or_else(|| format!("Scene {id} does not exist"))
    }

    /// Profile default (home) value of an attribute for a fixture.
    fn attr_home(&self, f: u32, attr: &str) -> f32 {
        self.patch
            .get(f)
            .and_then(|p| p.chans.iter().find(|c| c.attr == attr).map(|c| c.default))
            .unwrap_or_else(|| attr_default(attr))
    }

    // ───────────── playback ─────────────

    pub(super) fn scene_go_at(&mut self, id: u32, on: bool, now: f64) -> Result<(), String> {
        let sc = self.require_scene(id)?;
        let dur = if on { sc.fade_in } else { sc.fade_out };
        let dur = if dur.is_finite() { dur.max(0.0) } else { 0.0 };
        let cur = self.scenes.runs.get(&id).map_or(0.0, |r| r.level);
        if !on && !self.scenes.runs.contains_key(&id) {
            return Ok(());
        }
        let target = if on { 1.0 } else { 0.0 };
        let seq = match self.scenes.runs.get(&id) {
            Some(r) if !on || r.to > 0.0 => r.seq,
            _ => self.scenes.next_seq(),
        };
        self.scenes.runs.insert(id, SceneRun { level: cur, from: cur, to: target, start: now, dur, seq });
        self.scene_update(now);
        self.dirty = true;
        Ok(())
    }

    pub(super) fn scene_go(&mut self, id: u32, on: bool) -> Result<(), String> {
        let now = self.now();
        self.scene_go_at(id, on, now)
    }

    pub(super) fn scene_toggle(&mut self, id: u32) -> Result<(), String> {
        let on = !self.scene_active(id);
        self.scene_go(id, on)
    }

    pub(super) fn scene_active(&self, id: u32) -> bool {
        self.scenes.runs.get(&id).map_or(false, |r| r.to > 0.0)
    }

    pub(super) fn scene_solo_at(&mut self, id: u32, now: f64) -> Result<(), String> {
        self.require_scene(id)?;
        let others: Vec<u32> = self.scenes.runs.iter().filter(|(k, r)| **k != id && r.to > 0.0).map(|(k, _)| *k).collect();
        for o in others {
            self.scene_go_at(o, false, now)?;
        }
        self.scene_go_at(id, true, now)
    }

    pub(super) fn scene_level(&mut self, id: u32, level: f32) -> Result<(), String> {
        self.require_scene(id)?;
        let l = clamp01(level);
        if l <= 0.0 {
            self.scenes.runs.remove(&id);
        } else {
            let seq = match self.scenes.runs.get(&id) {
                Some(r) if r.to > 0.0 => r.seq,
                _ => self.scenes.next_seq(),
            };
            self.scenes.runs.insert(id, SceneRun { level: l, from: l, to: l, start: 0.0, dur: 0.0, seq });
        }
        self.dirty = true;
        Ok(())
    }

    pub(super) fn scene_set_flash(&mut self, id: u32, down: bool) -> Result<(), String> {
        self.require_scene(id)?;
        if down {
            self.scenes.flash.insert(id);
        } else {
            self.scenes.flash.remove(&id);
        }
        self.dirty = true;
        Ok(())
    }

    pub(super) fn scene_morph(&mut self, a: u32, b: u32, t: Option<f32>) -> Result<(), String> {
        match t {
            None => self.scenes.morph = None,
            Some(t) => {
                self.require_scene(a)?;
                self.require_scene(b)?;
                if a == b {
                    return Err("Morph needs two different scenes".into());
                }
                self.scenes.morph = Some((a, b, clamp01(if t.is_finite() { t } else { 0.0 })));
            }
        }
        self.dirty = true;
        Ok(())
    }

    pub(super) fn scenes_release_at(&mut self, now: f64) {
        let ids: Vec<u32> = self.scenes.runs.iter().filter(|(_, r)| r.to > 0.0).map(|(k, _)| *k).collect();
        for id in ids {
            let _ = self.scene_go_at(id, false, now);
        }
        self.scenes.morph = None;
        self.scenes.flash.clear();
        self.dirty = true;
    }

    /// Advance all fade ramps to `now`.
    pub(super) fn scene_update(&mut self, now: f64) {
        let mut changed = false;
        let mut done = Vec::new();
        for (id, r) in self.scenes.runs.iter_mut() {
            if r.level != r.to {
                let t = if r.dur <= 0.0 { 1.0 } else { ((now - r.start) / r.dur as f64).clamp(0.0, 1.0) as f32 };
                let nl = if t >= 1.0 { r.to } else { lerp(r.from, r.to, t) };
                if nl != r.level {
                    changed = true;
                }
                r.level = nl;
            }
            if r.level == 0.0 && r.to == 0.0 {
                done.push(*id);
            }
        }
        for id in done {
            self.scenes.runs.remove(&id);
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
    }

    // ───────────── render layer ─────────────

    /// Blend active scenes and the morph layer into `merged`.
    pub(super) fn apply_scenes(&self, merged: &mut PerFixture) {
        let morph = self.scenes.morph;
        let skip = |id: u32| morph.map_or(false, |(a, b, _)| id == a || id == b);
        let mut layers: Vec<(u64, u32, f32)> = Vec::new();
        for (id, r) in &self.scenes.runs {
            let l = self.scenes.effective(*id);
            if l > 0.0 && !skip(*id) {
                layers.push((r.seq, *id, l));
            }
        }
        for id in &self.scenes.flash {
            if !self.scenes.runs.contains_key(id) && !skip(*id) {
                layers.push((u64::MAX, *id, 1.0));
            }
        }
        layers.sort_by_key(|l| l.0);
        for (_, id, level) in layers {
            let Some(sc) = self.show.scenes.iter().find(|s| s.id == id) else { continue };
            for (f, attrs) in &sc.values {
                let discrete = |a: &str| self.patch.get(*f).map_or(false, |p| p.is_discrete(a));
                let e = merged.entry(*f).or_default();
                for (a, v) in attrs {
                    if a == "dimmer" {
                        let nv = v * level;
                        let cur = e.entry(a.clone()).or_insert(0.0);
                        if nv > *cur {
                            *cur = nv;
                        }
                    } else if discrete(a) {
                        if level >= 0.5 {
                            e.insert(a.clone(), *v);
                        }
                    } else {
                        let below = e.get(a).copied().unwrap_or_else(|| self.attr_home(*f, a));
                        e.insert(a.clone(), lerp(below, *v, level));
                    }
                }
            }
        }
        if let Some((a, b, t)) = morph {
            let (Some(sa), Some(sb)) =
                (self.show.scenes.iter().find(|s| s.id == a), self.show.scenes.iter().find(|s| s.id == b))
            else {
                return;
            };
            let fixtures: BTreeSet<u32> = sa.values.keys().chain(sb.values.keys()).copied().collect();
            for f in fixtures {
                let (ma, mb) = (sa.values.get(&f), sb.values.get(&f));
                let attrs: BTreeSet<&String> = ma.into_iter().flat_map(|m| m.keys()).chain(mb.into_iter().flat_map(|m| m.keys())).collect();
                let discrete = |x: &str| self.patch.get(f).map_or(false, |p| p.is_discrete(x));
                let e = merged.entry(f).or_default();
                for attr in attrs {
                    let va = ma.and_then(|m| m.get(attr)).copied();
                    let vb = mb.and_then(|m| m.get(attr)).copied();
                    if attr == "dimmer" {
                        let v = match (va, vb) {
                            (Some(x), Some(y)) => lerp(x, y, t),
                            (Some(x), None) => x * (1.0 - t),
                            (None, Some(y)) => y * t,
                            (None, None) => continue,
                        };
                        let cur = e.entry(attr.clone()).or_insert(0.0);
                        if v > *cur {
                            *cur = v;
                        }
                    } else if discrete(attr) {
                        let v = if t < 0.5 { va.or(vb) } else { vb.or(va) };
                        if let Some(v) = v {
                            e.insert(attr.clone(), v);
                        }
                    } else {
                        let v = match (va, vb) {
                            (Some(x), Some(y)) => lerp(x, y, t),
                            (Some(x), None) => x,
                            (None, Some(y)) => y,
                            (None, None) => continue,
                        };
                        e.insert(attr.clone(), v);
                    }
                }
            }
        }
    }

    // ───────────── editing ─────────────

    pub(super) fn scene_record(
        &mut self,
        id: Option<u32>,
        name: String,
        from_output: bool,
        fixtures: Option<Vec<u32>>,
        merge: bool,
    ) -> Result<(), String> {
        let wanted: Option<BTreeSet<u32>> = fixtures.map(|v| v.into_iter().collect());
        let known: BTreeSet<u32> = self.show.fixtures.iter().map(|f| f.id).collect();
        let source: PerFixture = if from_output {
            let now = self.now();
            self.compute_look(now)
        } else {
            self.programmer.clone()
        };
        let mut data = PerFixture::new();
        for (f, attrs) in source {
            if !known.contains(&f) || wanted.as_ref().map_or(false, |w| !w.contains(&f)) {
                continue;
            }
            let mut m = Attrs::new();
            for (a, v) in attrs {
                if !v.is_finite() {
                    continue;
                }
                let v = clamp01(v);
                if from_output && (v - self.attr_home(f, &a)).abs() < 1e-4 {
                    continue;
                }
                m.insert(a, v);
            }
            if !m.is_empty() {
                data.insert(f, m);
            }
        }
        if data.is_empty() {
            return Err(if from_output {
                "Nothing is lit — there is no output to record".into()
            } else {
                "Programmer is empty — nothing to record".into()
            });
        }
        let sid = id.unwrap_or_else(|| next_id(self.show.scenes.iter().map(|s| s.id)));
        match self.show.scenes.iter_mut().find(|s| s.id == sid) {
            Some(sc) => {
                if merge {
                    for (f, attrs) in data {
                        let e = sc.values.entry(f).or_default();
                        for (a, v) in attrs {
                            e.insert(a, v);
                        }
                    }
                } else {
                    sc.values = data;
                }
                if !name.trim().is_empty() {
                    sc.name = name.trim().to_string();
                }
            }
            None => {
                let name = if name.trim().is_empty() { format!("Scene {sid}") } else { name.trim().to_string() };
                self.show.scenes.push(Scene { id: sid, name, values: data, fade_in: 1.0, fade_out: 1.0, color: String::new(), refs: Default::default(), kind: Default::default() });
            }
        }
        self.touch();
        Ok(())
    }

    pub(super) fn scene_save(&mut self, mut scene: Scene) -> Result<(), String> {
        if scene.id == 0 {
            scene.id = next_id(self.show.scenes.iter().map(|s| s.id));
        }
        for attrs in scene.values.values_mut() {
            for v in attrs.values_mut() {
                *v = clamp01(*v);
            }
        }
        scene.values.retain(|_, m| !m.is_empty());
        for t in [&mut scene.fade_in, &mut scene.fade_out] {
            *t = if t.is_finite() { t.clamp(0.0, 3600.0) } else { 1.0 };
        }
        if scene.name.trim().is_empty() {
            scene.name = format!("Scene {}", scene.id);
        }
        match self.show.scenes.iter_mut().find(|s| s.id == scene.id) {
            Some(s) => *s = scene,
            None => self.show.scenes.push(scene),
        }
        self.touch();
        Ok(())
    }

    pub(super) fn scene_remove(&mut self, id: u32) {
        self.show.scenes.retain(|s| s.id != id);
        self.scenes.runs.remove(&id);
        self.scenes.flash.remove(&id);
        if self.scenes.morph.map_or(false, |(a, b, _)| a == id || b == id) {
            self.scenes.morph = None;
        }
        for e in &mut self.show.executors {
            if matches!(e.kind, ExecKind::Scene { scene } if scene == id) {
                e.kind = ExecKind::Empty;
            }
        }
        for p in &mut self.show.vconsole {
            for w in &mut p.widgets {
                let dead = match &w.action {
                    VcAction::Scene { scene, .. } => *scene == id,
                    VcAction::SceneMorph { a, b } => *a == id || *b == id,
                    _ => false,
                };
                if dead {
                    w.action = VcAction::None;
                }
            }
        }
        self.touch();
    }

    pub(super) fn scene_rt(&self) -> Vec<SceneRt> {
        self.show
            .scenes
            .iter()
            .map(|s| SceneRt { id: s.id, level: self.scenes.effective(s.id), active: self.scenes.is_active(s.id) })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::super::*;

    fn dimmer(e: &mut Engine, f: u32) -> u8 {
        let fr = frame(e);
        let pf = e.patch.get(f).unwrap();
        fr[pf.address as usize - 1 + pf.chans.iter().find(|c| c.attr == "dimmer").unwrap().offset as usize]
    }

    fn rgb_scene(e: &mut Engine, name: &str, r: f32, g: f32, b: f32) -> u32 {
        for (a, v) in [("red", r), ("green", g), ("blue", b)] {
            set(e, 1, a, v);
        }
        e.handle(Command::SceneRecord { id: None, name: name.into(), from_output: false, fixtures: None, merge: false, refs: true }).unwrap();
        e.programmer.clear();
        e.show.scenes.last().unwrap().id
    }

    #[test]
    fn record_from_programmer_and_activate() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        let s = rgb_scene(&mut e, "Red", 1.0, 0.0, 0.0);
        assert_eq!(frame(&mut e)[0], 0);
        e.scene_level(s, 1.0).unwrap();
        let fr = frame(&mut e);
        assert_eq!((fr[0], fr[1], fr[2]), (255, 0, 0));
        e.handle(Command::SceneLevel { id: s, level: 0.0 }).unwrap();
        assert_eq!(frame(&mut e)[0], 0);
    }

    #[test]
    fn empty_programmer_is_an_error() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        assert!(e.handle(Command::SceneRecord { id: None, name: "x".into(), from_output: false, fixtures: None, merge: false, refs: true }).is_err());
        assert!(e.handle(Command::SceneGo { id: 99, on: true }).is_err());
    }

    #[test]
    fn fade_in_and_out_follow_times() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        set(&mut e, 1, "dimmer", 1.0);
        e.handle(Command::SceneRecord { id: None, name: "A".into(), from_output: false, fixtures: None, merge: false, refs: true }).unwrap();
        e.programmer.clear();
        let s = e.show.scenes[0].id;
        e.show.scenes[0].fade_in = 2.0;
        e.show.scenes[0].fade_out = 4.0;
        e.scene_go_at(s, true, 10.0).unwrap();
        e.scene_update(11.0);
        assert!((e.scenes.effective(s) - 0.5).abs() < 1e-4);
        assert_eq!(dimmer(&mut e, 1), 128);
        e.scene_update(12.0);
        assert_eq!(e.scenes.effective(s), 1.0);
        assert!(!e.scenes.ramping());
        e.scene_go_at(s, false, 20.0).unwrap();
        e.scene_update(22.0);
        assert!((e.scenes.effective(s) - 0.5).abs() < 1e-4);
        assert!(e.scene_rt()[0].active);
        e.scene_update(24.0);
        assert_eq!(e.scenes.effective(s), 0.0);
        assert!(!e.scene_rt()[0].active);
        assert_eq!(dimmer(&mut e, 1), 0);
    }

    #[test]
    fn dimmer_is_htp_with_cuelist_and_colour_is_ltp_blend() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par-dimstrobe", "6ch", 1);
        let prof = e.profiles.get("generic/rgb-par-dimstrobe").unwrap().modes[0].clone();
        assert!(prof.channels.iter().any(|c| c.attr == "dimmer"));
        // cue: dimmer .8, red 1
        set(&mut e, 1, "dimmer", 0.8);
        set(&mut e, 1, "red", 1.0);
        e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: "".into(), release: true, refs: true }).unwrap();
        e.show.cuelists[0].cues[0].fade_in = 0.0;
        e.go(1).unwrap();
        e.update_playback(0.1);
        // scene: dimmer .5, red 0, blue 1
        set(&mut e, 1, "dimmer", 0.5);
        set(&mut e, 1, "red", 0.0);
        set(&mut e, 1, "blue", 1.0);
        e.handle(Command::SceneRecord { id: None, name: "S".into(), from_output: false, fixtures: None, merge: false, refs: true }).unwrap();
        e.programmer.clear();
        let s = e.show.scenes[0].id;
        e.scene_level(s, 0.5).unwrap();
        let look = e.compute_look(0.0);
        let a = &look[&1];
        assert!((a["dimmer"] - 0.8).abs() < 1e-4, "HTP keeps the cue's dimmer: {a:?}");
        assert!((a["red"] - 0.5).abs() < 1e-4, "red blends 1 → 0 at 50%: {a:?}");
        assert!((a["blue"] - 0.5).abs() < 1e-4, "blue blends 0 → 1 at 50%: {a:?}");
        e.scene_level(s, 1.0).unwrap();
        let look = e.compute_look(0.0);
        assert!((look[&1]["red"] - 0.0).abs() < 1e-4);
    }

    #[test]
    fn newer_scene_wins_ltp() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        let a = rgb_scene(&mut e, "R", 1.0, 0.0, 0.0);
        let b = rgb_scene(&mut e, "G", 0.0, 1.0, 0.0);
        e.scene_level(a, 1.0).unwrap();
        e.scene_level(b, 1.0).unwrap();
        let fr = frame(&mut e);
        assert_eq!((fr[0], fr[1]), (0, 255));
        // re-activating a moves it on top
        e.scene_level(a, 0.0).unwrap();
        e.scene_level(a, 1.0).unwrap();
        let fr = frame(&mut e);
        assert_eq!((fr[0], fr[1]), (255, 0));
    }

    #[test]
    fn solo_releases_others_with_their_fade_out() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        let a = rgb_scene(&mut e, "R", 1.0, 0.0, 0.0);
        let b = rgb_scene(&mut e, "G", 0.0, 1.0, 0.0);
        for s in &mut e.show.scenes {
            s.fade_in = 0.0;
            s.fade_out = 2.0;
        }
        e.scene_go_at(a, true, 0.0).unwrap();
        e.scene_solo_at(b, 5.0).unwrap();
        e.scene_update(6.0);
        assert!((e.scenes.effective(a) - 0.5).abs() < 1e-4);
        assert_eq!(e.scenes.effective(b), 1.0);
        e.scene_update(7.5);
        assert!(!e.scenes.runs.contains_key(&a));
    }

    #[test]
    fn discrete_attributes_snap_at_half() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 1);
        set(&mut e, 1, "dimmer", 1.0);
        set(&mut e, 1, "gobo", 0.6);
        e.handle(Command::SceneRecord { id: None, name: "G".into(), from_output: false, fixtures: None, merge: false, refs: true }).unwrap();
        e.programmer.clear();
        let s = e.show.scenes[0].id;
        e.scene_level(s, 0.4).unwrap();
        assert!(e.compute_look(0.0)[&1].get("gobo").is_none());
        e.scene_level(s, 0.5).unwrap();
        assert_eq!(e.compute_look(0.0)[&1]["gobo"], 0.6);
    }

    #[test]
    fn morph_interpolates_and_replaces_individual_scenes() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        let a = rgb_scene(&mut e, "R", 1.0, 0.0, 0.0);
        let b = rgb_scene(&mut e, "B", 0.0, 0.0, 1.0);
        e.scene_level(a, 1.0).unwrap();
        e.scene_level(b, 1.0).unwrap(); // would normally win LTP
        e.handle(Command::SceneMorph { a, b, t: Some(0.25) }).unwrap();
        let look = e.compute_look(0.0);
        assert!((look[&1]["red"] - 0.75).abs() < 1e-4);
        assert!((look[&1]["blue"] - 0.25).abs() < 1e-4);
        assert_eq!(e.build_rt().morph, Some((a, b, 0.25)));
        e.handle(Command::SceneMorph { a, b, t: None }).unwrap();
        assert!(e.build_rt().morph.is_none());
        assert!(e.handle(Command::SceneMorph { a, b: a, t: Some(0.5) }).is_err());
    }

    #[test]
    fn morph_one_sided_dimmer_scales_and_other_attrs_hold() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par-dimstrobe", "6ch", 2);
        set(&mut e, 1, "dimmer", 1.0);
        set(&mut e, 1, "red", 1.0);
        e.handle(Command::SceneRecord { id: None, name: "A".into(), from_output: false, fixtures: Some(vec![1]), merge: false, refs: true }).unwrap();
        e.programmer.clear();
        set(&mut e, 2, "dimmer", 1.0);
        set(&mut e, 2, "green", 1.0);
        e.handle(Command::SceneRecord { id: None, name: "B".into(), from_output: false, fixtures: Some(vec![2]), merge: false, refs: true }).unwrap();
        e.programmer.clear();
        let (a, b) = (e.show.scenes[0].id, e.show.scenes[1].id);
        e.handle(Command::SceneMorph { a, b, t: Some(0.25) }).unwrap();
        let look = e.compute_look(0.0);
        assert!((look[&1]["dimmer"] - 0.75).abs() < 1e-4);
        assert!((look[&2]["dimmer"] - 0.25).abs() < 1e-4);
        assert_eq!(look[&1]["red"], 1.0);
        assert_eq!(look[&2]["green"], 1.0);
    }

    #[test]
    fn record_from_output_snapshots_final_state_without_defaults() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 2);
        set(&mut e, 1, "dimmer", 1.0);
        set(&mut e, 1, "pan", 0.25);
        set(&mut e, 2, "dimmer", 0.5);
        e.handle(Command::SceneRecord { id: None, name: "Snap".into(), from_output: true, fixtures: Some(vec![1]), merge: false, refs: true }).unwrap();
        let id = e.show.scenes[0].id;
        {
            let sc = &e.show.scenes[0];
            assert_eq!(sc.name, "Snap");
            assert_eq!(sc.values.len(), 1);
            assert_eq!(sc.values[&1]["pan"], 0.25);
            assert!(!sc.values[&1].contains_key("tilt"), "home position is not recorded");
        }
        // update with merge keeps other fixtures; replace does not
        e.programmer.clear();
        set(&mut e, 2, "dimmer", 0.5);
        e.handle(Command::SceneRecord { id: Some(id), name: "".into(), from_output: false, fixtures: None, merge: true, refs: true }).unwrap();
        assert_eq!(e.show.scenes[0].values.len(), 2);
        e.handle(Command::SceneRecord { id: Some(id), name: "".into(), from_output: false, fixtures: None, merge: false, refs: true }).unwrap();
        assert_eq!(e.show.scenes[0].values.len(), 1);
        assert_eq!(e.show.scenes[0].name, "Snap");
        // nothing lit → error
        e.programmer.clear();
        let mut e2 = engine();
        add(&mut e2, "generic/mh-spot", "16ch", 1);
        assert!(e2.handle(Command::SceneRecord { id: None, name: "".into(), from_output: true, fixtures: None, merge: false, refs: true }).is_err());
    }

    #[test]
    fn remove_scene_clears_executors_and_widgets() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        let s = rgb_scene(&mut e, "R", 1.0, 0.0, 0.0);
        e.handle(Command::ExecSave { executor: Executor { id: 0, page: 1, slot: 0, label: "S".into(), kind: ExecKind::Scene { scene: s } } }).unwrap();
        e.handle(Command::SceneRemove { id: s }).unwrap();
        assert!(e.show.scenes.is_empty());
        assert!(matches!(e.show.executors[0].kind, ExecKind::Empty));
    }

    #[test]
    fn executor_wiring_level_toggle_flash() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        let s = rgb_scene(&mut e, "R", 1.0, 0.0, 0.0);
        e.show.scenes[0].fade_in = 0.0;
        e.show.scenes[0].fade_out = 0.0;
        e.handle(Command::ExecSave { executor: Executor { id: 5, page: 1, slot: 0, label: "S".into(), kind: ExecKind::Scene { scene: s } } }).unwrap();
        e.handle(Command::ExecLevel { id: 5, level: 0.5 }).unwrap();
        let rt = e.build_rt();
        let ex = rt.executors.iter().find(|x| x.id == 5).unwrap();
        assert!((ex.level - 0.5).abs() < 1e-4 && ex.active);
        e.handle(Command::ExecLevel { id: 5, level: 0.0 }).unwrap();
        assert!(!e.build_rt().executors[0].active);
        e.handle(Command::ExecGo { id: 5 }).unwrap();
        assert!(e.build_rt().executors[0].active, "button toggles on");
        assert_eq!(e.build_rt().scenes[0].level, 1.0);
        e.handle(Command::ExecGo { id: 5 }).unwrap();
        e.scene_update(e.now() + 1.0);
        assert!(!e.build_rt().executors[0].active, "button toggles off");
        e.handle(Command::ExecFlash { id: 5, down: true }).unwrap();
        let fr = frame(&mut e);
        assert_eq!(fr[0], 255);
        e.handle(Command::ExecFlash { id: 5, down: false }).unwrap();
        assert_eq!(frame(&mut e)[0], 0);
    }

    #[test]
    fn vc_scene_widgets() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        let a = rgb_scene(&mut e, "R", 1.0, 0.0, 0.0);
        let b = rgb_scene(&mut e, "B", 0.0, 0.0, 1.0);
        for s in &mut e.show.scenes {
            s.fade_in = 0.0;
            s.fade_out = 0.0;
        }
        let w = |id: u32, kind: VcKind, action: VcAction, mode: VcMode| VcWidget {
            id,
            kind,
            label: "".into(),
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            color: "#fff".into(),
            action,
            mode,
            orientation: "vertical".into(),
            inputs: vec![],
            feedback: vec![],
            out_min: 0.0,
            out_max: 1.0,
        };
        e.handle(Command::VcSave {
            pages: vec![VcPage {
                id: 1,
                name: "p".into(),
                cols: 8,
                rows: 8,
                widgets: vec![
                    w(1, VcKind::Button, VcAction::Scene { scene: a, op: VcOp::Go }, VcMode::Trigger),
                    w(2, VcKind::Button, VcAction::Scene { scene: a, op: VcOp::Toggle }, VcMode::Trigger),
                    w(3, VcKind::Slider, VcAction::Scene { scene: b, op: VcOp::Level }, VcMode::Trigger),
                    w(4, VcKind::Slider, VcAction::SceneMorph { a, b }, VcMode::Trigger),
                    w(5, VcKind::Button, VcAction::Scene { scene: a, op: VcOp::Flash }, VcMode::Momentary),
                    w(6, VcKind::Button, VcAction::Scene { scene: a, op: VcOp::Back }, VcMode::Trigger),
                ],
            }],
        })
        .unwrap();
        e.handle(Command::VcPress { widget: 1, down: true }).unwrap();
        e.handle(Command::VcPress { widget: 1, down: false }).unwrap();
        assert_eq!(frame(&mut e)[0], 255);
        let st = e.vc_states();
        assert!(st.iter().find(|s| s.id == 1).unwrap().active);
        e.handle(Command::VcPress { widget: 6, down: true }).unwrap();
        assert_eq!(frame(&mut e)[0], 0);
        e.handle(Command::VcPress { widget: 2, down: true }).unwrap();
        assert_eq!(frame(&mut e)[0], 255, "toggle on");
        e.handle(Command::VcPress { widget: 2, down: true }).unwrap();
        assert_eq!(frame(&mut e)[0], 0, "toggle off");
        e.handle(Command::VcValue { widget: 3, value: 0.5 }).unwrap();
        assert_eq!(frame(&mut e)[2], 128);
        let s3 = e.vc_states().into_iter().find(|s| s.id == 3).unwrap();
        assert!((s3.value - 0.5).abs() < 1e-3 && s3.active);
        e.handle(Command::VcValue { widget: 3, value: 0.0 }).unwrap();
        e.handle(Command::VcValue { widget: 4, value: 0.5 }).unwrap();
        assert_eq!(e.build_rt().morph, Some((a, b, 0.5)));
        let fr = frame(&mut e);
        assert_eq!((fr[0], fr[2]), (128, 128));
        let s4 = e.vc_states().into_iter().find(|s| s.id == 4).unwrap();
        assert!((s4.value - 0.5).abs() < 1e-3 && s4.active);
        e.handle(Command::VcPress { widget: 5, down: true }).unwrap();
        assert!(e.vc_states().iter().find(|s| s.id == 5).unwrap().active);
        e.handle(Command::VcPress { widget: 5, down: false }).unwrap();
        assert!(!e.vc_states().iter().find(|s| s.id == 5).unwrap().active);
    }

    #[test]
    fn release_all_clears_everything() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        let a = rgb_scene(&mut e, "R", 1.0, 0.0, 0.0);
        let b = rgb_scene(&mut e, "B", 0.0, 0.0, 1.0);
        e.show.scenes[0].fade_out = 0.0;
        e.show.scenes[1].fade_out = 0.0;
        e.scene_level(a, 1.0).unwrap();
        e.scene_morph(a, b, Some(0.5)).unwrap();
        e.handle(Command::ScenesRelease).unwrap();
        assert!(e.build_rt().morph.is_none());
        e.scene_update(e.now() + 1.0);
        assert_eq!(frame(&mut e)[0], 0);
    }

    #[test]
    fn json_round_trip_and_defaults() {
        let s: Scene = serde_json::from_str(r#"{"id":3,"name":"x"}"#).unwrap();
        assert_eq!((s.fade_in, s.fade_out), (1.0, 1.0));
        let c: ClientMsg = serde_json::from_str(
            r#"{"t":"cmd","id":"1","cmd":{"type":"scene_record","id":null,"name":"A","from_output":true,"fixtures":null,"merge":false}}"#,
        )
        .unwrap();
        assert!(matches!(c.cmd, Command::SceneRecord { from_output: true, .. }));
        let c: ClientMsg = serde_json::from_str(r#"{"t":"cmd","id":"2","cmd":{"type":"scene_morph","a":1,"b":2,"t":0.5}}"#).unwrap();
        assert!(matches!(c.cmd, Command::SceneMorph { t: Some(_), .. }));
        let c: ClientMsg = serde_json::from_str(r#"{"t":"cmd","id":"3","cmd":{"type":"scenes_release"}}"#).unwrap();
        assert!(matches!(c.cmd, Command::ScenesRelease));
        let c: ClientMsg = serde_json::from_str(
            r#"{"t":"cmd","id":"4","cmd":{"type":"scene_save","scene":{"id":0,"name":"n","values":{"1":{"red":1.0}},"fade_in":1,"fade_out":2,"color":""}}}"#,
        )
        .unwrap();
        let Command::SceneSave { scene } = c.cmd else { panic!() };
        assert_eq!(scene.values[&1]["red"], 1.0);
    }
}
