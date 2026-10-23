//! Manual crossfade (`CuelistManual`, `ExecKind::XFade`) and auto-mark ("move on dark").
//!
//! # Manual crossfade
//! A fader position `q` (0..1) drives the fade of the next cue. The first movement away from an end
//! starts the transition to the next cue; reaching the other end completes it. Moving back to the origin
//! end before completion cancels it (the previous look returns). After a completed transition the
//! fader rests at the opposite end, so the next move runs the other way (`xf_top`): the usual A/B
//! crossfader behaviour. `progress: None` hands a transition in flight back to the clock.
//!
//! # Auto-mark
//! With `Cuelist.auto_mark`, while cue *k* plays, fixtures that are dark in *k* but lit in *k+1* get the
//! non-intensity attributes of *k+1* applied early (fading in at least one second), so moving heads are
//! in position before their light arrives. Lit fixtures are never touched.

use super::*;

/// Is the fixture producing light in this (tracked) state?
fn lit(pf: &PatchFixture, st: Option<&Attrs>) -> bool {
    let Some(st) = st else { return false };
    let d = st.get("dimmer").copied();
    if pf.has_dimmer {
        return d.map_or(false, |v| v > 0.001);
    }
    if let Some(v) = d {
        if v <= 0.001 {
            return false;
        }
    }
    COLOR_ATTRS.iter().any(|a| st.get(*a).map_or(false, |v| *v > 0.001))
}

impl Engine {
    /// Attributes to pre-position while cue `idx` plays: `(fixture, attr, value)`.
    pub(super) fn compute_marks(&self, cl: &Cuelist, idx: usize, cur: &PerFixture) -> Vec<(u32, String, f32)> {
        if !cl.auto_mark {
            return vec![];
        }
        let next = if idx + 1 < cl.cues.len() {
            idx + 1
        } else if cl.looped && cl.cues.len() > 1 {
            0
        } else {
            return vec![];
        };
        let next_state = tracked_state(cl, next);
        let mut out = Vec::new();
        for (f, attrs) in &next_state {
            let Some(pf) = self.patch.get(*f) else { continue };
            if lit(pf, cur.get(f)) || !lit(pf, Some(attrs)) {
                continue;
            }
            for (a, v) in attrs {
                if attr_group(a) != "intensity" && pf.has_attr(a) {
                    out.push((*f, a.clone(), *v));
                }
            }
        }
        out
    }

    pub(super) fn manual_active(&self, id: u32) -> bool {
        self.runs.get(&id).map_or(false, |r| r.manual.is_some())
    }

    /// A GO arrived while a manual transition was in flight: let it complete.
    pub(super) fn finish_manual(&mut self, id: u32) {
        let now = self.now();
        let inverted = self.prog.xf_top.contains(&id);
        let patch = &self.patch;
        let disc = |f: u32, a: &str| patch.get(f).map_or(false, |p| p.is_discrete(a));
        let Some(r) = self.runs.get_mut(&id) else { return };
        if r.manual.take().is_some() {
            r.start = now - (r.t_end + 1.0) as f64;
            r.settled = false;
            r.update(now, &disc);
            if inverted {
                self.prog.xf_top.remove(&id);
            } else {
                self.prog.xf_top.insert(id);
            }
        }
    }

    pub(super) fn cuelist_manual(&mut self, id: u32, progress: Option<f32>) -> Result<(), String> {
        let now = self.now();
        let cl = self.require_cuelist(id)?;
        let len = cl.cues.len();
        if len == 0 {
            return Err("Cuelist has no cues".into());
        }
        let looped = cl.looped;
        let cur_idx = self.runs.get(&id).and_then(|r| r.current).and_then(|n| index_of(cl, n));

        let Some(q) = progress else {
            if let Some(r) = self.runs.get_mut(&id) {
                if let Some(p) = r.manual.take() {
                    let span = (r.t_end - r.t_min).max(0.001);
                    r.start = now - (r.t_min + p * span) as f64;
                }
            }
            return Ok(());
        };

        let q = clamp01(q);
        let inverted = self.prog.xf_top.contains(&id);
        let p = if inverted { 1.0 - q } else { q };
        if !self.manual_active(id) {
            if p <= 0.001 {
                return Ok(());
            }
            let idx = match cur_idx {
                None => 0,
                Some(i) if i + 1 < len => i + 1,
                Some(_) if looped => 0,
                Some(_) => return Ok(()),
            };
            self.start_cue(id, idx);
            if let Some(r) = self.runs.get_mut(&id) {
                r.manual = Some(p);
                r.follow_at = None;
            }
        }
        let Some(r) = self.runs.get_mut(&id) else { return Ok(()) };
        if p >= 0.999 {
            r.manual = Some(1.0);
            if inverted {
                self.prog.xf_top.remove(&id);
            } else {
                self.prog.xf_top.insert(id);
            }
        } else if p <= 0.001 {
            // cancelled: the previous look returns
            r.values = std::mem::take(&mut r.prev_values);
            r.current = r.prev_current;
            r.manual = None;
            r.drop.clear();
            r.settled = true;
            r.progress = 1.0;
            r.state = if r.current.is_some() { PlayState::Running } else { PlayState::Idle };
        } else {
            r.manual = Some(p);
            r.settled = false;
        }
        self.dirty = true;
        Ok(())
    }
}
