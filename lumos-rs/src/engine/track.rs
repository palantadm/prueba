//! Tracking input (PosiStageNet / BlackTrax): tracker positions in stage coordinates and the "follow" rules that point
//! groups of moving heads at them. The follow is a render-time layer (like a channel group), so it neither fills the
//! undo history nor stays in the programmer.

use super::aim::aim_at;
use super::*;
use crate::psn::{self, Packet};

/// A tracker that has not sent anything for this long is considered lost.
const LOST: Duration = Duration::from_millis(1000);

struct Trk {
    name: String,
    stage: [f32; 3],
    seen: Instant,
}

#[derive(Default)]
pub struct TrackState {
    trackers: BTreeMap<u16, Trk>,
    system: String,
    packets: u32,
    /// Smoothed aim point per follow.
    smooth: HashMap<u32, [f32; 3]>,
    tracking: HashMap<u32, bool>,
    last_render: Option<f64>,
}

/// Tracker coordinates → stage coordinates (x right, y up, z depth).
pub fn map_pos(m: &PsnMap, p: [f32; 3]) -> [f32; 3] {
    let pick = |a: PsnAxis| match a {
        PsnAxis::X => p[0],
        PsnAxis::Y => p[1],
        PsnAxis::Z => p[2],
    };
    let sgn = |inv: bool| if inv { -1.0 } else { 1.0 };
    let s = if m.scale.is_finite() && m.scale != 0.0 { m.scale } else { 1.0 };
    [pick(m.to_x) * sgn(m.invert_x) * s + m.offset[0], pick(m.to_y) * sgn(m.invert_y) * s + m.offset[1], pick(m.to_z) * sgn(m.invert_z) * s + m.offset[2]]
}

impl Engine {
    pub(super) fn on_psn(&mut self, data: &[u8]) {
        if !self.show.settings.psn.enabled {
            return;
        }
        let map = self.show.settings.psn.map.clone();
        match psn::parse(data) {
            Some(Packet::Data { trackers, .. }) => {
                self.track.packets = self.track.packets.wrapping_add(1);
                let now = Instant::now();
                for t in trackers {
                    // a tracker that says "not valid" keeps its last position and ages out
                    let Some(pos) = t.pos else { continue };
                    if t.validity.map_or(false, |v| v <= 0.0) {
                        continue;
                    }
                    let stage = map_pos(&map, pos);
                    let e = self.track.trackers.entry(t.id).or_insert_with(|| Trk { name: String::new(), stage, seen: now });
                    e.stage = stage;
                    e.seen = now;
                }
                if self.track.trackers.len() > 512 {
                    self.track.trackers.retain(|_, t| t.seen.elapsed() < Duration::from_secs(60));
                }
            }
            Some(Packet::Info { system, names }) => {
                self.track.system = system;
                for (id, name) in names {
                    if let Some(t) = self.track.trackers.get_mut(&id) {
                        t.name = name;
                    } else {
                        // names can arrive before the first position
                        self.track.trackers.insert(id, Trk { name, stage: [0.0; 3], seen: Instant::now() - Duration::from_secs(3600) });
                    }
                }
            }
            None => {}
        }
        self.dirty = true;
    }

    pub(super) fn follow_command(&mut self, c: Command) -> Result<(), String> {
        match c {
            Command::FollowSave { mut follow } => {
                follow.name = follow.name.trim().chars().take(60).collect();
                if follow.name.is_empty() {
                    follow.name = format!("Seguimiento tracker {}", follow.tracker);
                }
                if !self.show.groups.iter().any(|g| g.id == follow.group) {
                    return Err(format!("El grupo {} no existe; crea antes el grupo con los móviles que deben seguir", follow.group));
                }
                if follow.height.is_finite() {
                    follow.height = follow.height.clamp(-5.0, 20.0);
                } else {
                    follow.height = 0.0;
                }
                follow.smooth_ms = follow.smooth_ms.min(10_000);
                follow.intensity = follow.intensity.filter(|v| v.is_finite()).map(clamp01);
                if follow.id == 0 {
                    follow.id = next_id(self.show.follows.iter().map(|f| f.id));
                }
                self.track.smooth.remove(&follow.id);
                match self.show.follows.iter_mut().find(|f| f.id == follow.id) {
                    Some(f) => *f = follow,
                    None => self.show.follows.push(follow),
                }
                self.touch();
            }
            Command::FollowRemove { id } => {
                self.show.follows.retain(|f| f.id != id);
                self.track.smooth.remove(&id);
                self.track.tracking.remove(&id);
                self.touch();
            }
            Command::FollowSet { id, enabled } => {
                let f = self.show.follows.iter_mut().find(|f| f.id == id).ok_or_else(|| format!("El seguimiento {id} no existe"))?;
                f.enabled = enabled;
                self.touch();
            }
            _ => {}
        }
        Ok(())
    }

    /// Point the group of every enabled follow at its tracker (called from `render`, after the look is built).
    pub(super) fn apply_follows(&mut self, merged: &mut PerFixture, now: f64) {
        if self.show.follows.is_empty() {
            self.track.last_render = Some(now);
            return;
        }
        let dt = self.track.last_render.map_or(0.0, |l| (now - l).clamp(0.0, 0.5)) as f32;
        self.track.last_render = Some(now);
        let follows = self.show.follows.clone();
        for f in follows.iter().filter(|f| f.enabled) {
            let Some(group) = self.show.groups.iter().find(|g| g.id == f.group) else { continue };
            let present = self.track.trackers.get(&f.tracker).filter(|t| t.seen.elapsed() < LOST).map(|t| t.stage);
            self.track.tracking.insert(f.id, present.is_some());
            let aim = match present {
                Some(p) => {
                    let target = [p[0], p[1] + f.height, p[2]];
                    let alpha = if f.smooth_ms == 0 { 1.0 } else { 1.0 - (-dt * 1000.0 / f.smooth_ms as f32).exp() };
                    let cur = self.track.smooth.entry(f.id).or_insert(target);
                    for i in 0..3 {
                        cur[i] += (target[i] - cur[i]) * alpha;
                    }
                    *cur
                }
                // lost: hold the last aim (the heads must not snap back to centre); the light goes out if asked to
                None => match self.track.smooth.get(&f.id) {
                    Some(a) => *a,
                    None => continue,
                },
            };
            for id in &group.fixtures {
                let Some(fx) = self.show.fixtures.iter().find(|x| x.id == *id) else { continue };
                let Some((pr, tr)) = self.mover_ranges(fx) else { continue };
                let (pan, tilt) = aim_at(fx.pos, fx.rot, pr, tr, aim);
                let a = merged.entry(*id).or_default();
                a.insert("pan".into(), pan);
                a.insert("tilt".into(), tilt);
                if let Some(i) = f.intensity {
                    a.insert("dimmer".into(), if present.is_some() { i } else { 0.0 });
                }
            }
        }
    }

    pub(super) fn psn_rt(&self, inputs: &InputStatus) -> PsnRt {
        let trackers = self
            .track
            .trackers
            .iter()
            .map(|(id, t)| {
                let age = t.seen.elapsed();
                PsnTrackerRt { id: *id, name: t.name.clone(), x: t.stage[0], y: t.stage[1], z: t.stage[2], age_ms: age.as_millis().min(u32::MAX as u128) as u32, active: age < LOST }
            })
            .collect();
        let follows = self.show.follows.iter().map(|f| FollowRt { id: f.id, tracking: f.enabled && self.track.tracking.get(&f.id).copied().unwrap_or(false) }).collect();
        PsnRt { listening: inputs.psn_listening, error: inputs.psn_error.clone(), system: self.track.system.clone(), packets: self.track.packets, trackers, follows }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::*;

    fn rig() -> Engine {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 2);
        e.show.fixtures[0].pos = [-2.0, 6.0, 0.0];
        e.show.fixtures[1].pos = [2.0, 6.0, 0.0];
        e.handle(Command::GroupSave { group: Group { id: 0, name: "movers".into(), fixtures: vec![1, 2] } }).unwrap();
        e.show.settings.psn.enabled = true;
        e
    }
    fn feed(e: &mut Engine, id: u16, p: [f32; 3]) {
        e.on_psn(&psn::build::data(1, &[(id, p)]));
    }
    fn follow(tracker: u16, height: f32, smooth_ms: u32, intensity: Option<f32>) -> Command {
        Command::FollowSave { follow: TrackFollow { id: 0, name: String::new(), enabled: true, tracker, group: 1, height, smooth_ms, intensity } }
    }
    /// Expected 8-bit coarse pan/tilt for fixture `i` aiming at `target`.
    fn expect(e: &Engine, i: usize, target: [f32; 3]) -> (u8, u8) {
        let f = &e.show.fixtures[i];
        let (pr, tr) = e.mover_ranges(f).unwrap();
        let (p, t) = aim_at(f.pos, f.rot, pr, tr, target);
        ((p * 255.0).round() as u8, (t * 255.0).round() as u8)
    }
    fn near(a: u8, b: u8) -> bool {
        a.abs_diff(b) <= 1
    }

    #[test]
    fn axes_are_mapped_to_stage_coordinates() {
        let m = PsnMap::default();
        assert_eq!(map_pos(&m, [1.0, 2.0, 3.0]), [1.0, 3.0, 2.0], "PSN z is up → stage y");
        let m = PsnMap { invert_x: true, offset: [0.5, 0.0, -1.0], scale: 2.0, ..PsnMap::default() };
        assert_eq!(map_pos(&m, [1.0, 2.0, 3.0]), [-1.5, 6.0, 3.0]);
        let bad = PsnMap { scale: f32::NAN, ..PsnMap::default() };
        assert_eq!(map_pos(&bad, [1.0, 2.0, 3.0]), [1.0, 3.0, 2.0], "a broken scale never poisons the position");
    }

    #[test]
    fn heads_follow_the_tracker_and_the_dimmer_comes_with_it() {
        let mut e = rig();
        e.handle(follow(7, 1.5, 0, Some(1.0))).unwrap();
        // tracker on the floor at stage (1, 0, 2): PSN z up → [x, z, y] = [1, 0, 2]
        feed(&mut e, 7, [1.0, 2.0, 0.0]);
        let f = frame(&mut e);
        let (p0, t0) = expect(&e, 0, [1.0, 1.5, 2.0]);
        let (p1, t1) = expect(&e, 1, [1.0, 1.5, 2.0]);
        assert!(near(f[0], p0) && near(f[2], t0), "head 1: {} {} vs {p0} {t0}", f[0], f[2]);
        let b = 16; // second head starts at channel 17
        assert!(near(f[b], p1) && near(f[b + 2], t1), "head 2");
        assert_eq!((f[5], f[b + 5]), (255, 255), "dimmer up while tracked");
        // the performer walks: heads follow
        feed(&mut e, 7, [-3.0, 0.0, 0.0]);
        let f = frame(&mut e);
        let (p0, _) = expect(&e, 0, [-3.0, 1.5, 0.0]);
        assert!(near(f[0], p0));
        assert_eq!(e.psn_rt(&e.io.status().1).trackers.len(), 1);
    }

    #[test]
    fn a_lost_tracker_holds_the_aim_and_switches_the_light_off() {
        let mut e = rig();
        e.handle(follow(1, 0.0, 0, Some(1.0))).unwrap();
        feed(&mut e, 1, [0.0, 1.0, 0.0]);
        let f = frame(&mut e);
        let (pan, dim) = (f[0], f[5]);
        assert_eq!(dim, 255);
        e.track.trackers.get_mut(&1).unwrap().seen = Instant::now() - Duration::from_secs(5);
        let f = frame(&mut e);
        assert_eq!(f[0], pan, "holds the last position");
        assert_eq!(f[5], 0, "light out");
        assert!(!e.psn_rt(&e.io.status().1).follows[0].tracking);
        // back again
        feed(&mut e, 1, [0.0, 1.0, 0.0]);
        assert_eq!(frame(&mut e)[5], 255);
    }

    #[test]
    fn smoothing_moves_part_of_the_way_and_a_disabled_follow_does_nothing() {
        let mut e = rig();
        e.handle(follow(1, 0.0, 500, None)).unwrap();
        feed(&mut e, 1, [0.0, 0.0, 0.0]);
        e.render(0.0);
        let before = e.frames.get(&1).unwrap()[0];
        feed(&mut e, 1, [0.0, 4.0, 0.0]);
        e.render(0.1);
        let mid = e.frames.get(&1).unwrap()[0];
        for i in 0..80 {
            feed(&mut e, 1, [0.0, 4.0, 0.0]);
            e.render(0.2 + 0.05 * i as f64);
        }
        let (target, _) = expect(&e, 0, [0.0, 0.0, 4.0]);
        let end = e.frames.get(&1).unwrap()[0];
        assert!(mid != before && !near(mid, target), "partway: {before} → {mid} (target {target})");
        assert!(near(end, target) || end.abs_diff(target) < 6, "converges: {end} vs {target}");
        e.handle(Command::FollowSet { id: 1, enabled: false }).unwrap();
        let f = frame(&mut e);
        assert_eq!(f[0], 128, "released: the head returns to its default");
    }

    #[test]
    fn follow_validation_and_settings_gate() {
        let mut e = rig();
        assert!(e.handle(Command::FollowSave { follow: TrackFollow { id: 0, name: "x".into(), enabled: true, tracker: 1, group: 99, height: 0.0, smooth_ms: 0, intensity: None } }).unwrap_err().contains("grupo"));
        e.handle(follow(1, 999.0, 99_999, Some(7.0))).unwrap();
        let f = &e.show.follows[0];
        assert_eq!((f.height, f.smooth_ms, f.intensity), (20.0, 10_000, Some(1.0)), "values are clamped");
        // with the input switched off, packets are ignored
        e.show.settings.psn.enabled = false;
        feed(&mut e, 5, [1.0, 1.0, 1.0]);
        assert!(e.track.trackers.is_empty());
        // garbage is ignored
        e.show.settings.psn.enabled = true;
        e.on_psn(b"not psn");
        assert!(e.track.trackers.is_empty());
        // names arrive in an info packet
        feed(&mut e, 5, [1.0, 1.0, 1.0]);
        e.on_psn(&psn::build::info("BlackTrax", &[(5, "Ana")]));
        let rt = e.psn_rt(&e.io.status().1);
        assert_eq!((rt.system.as_str(), rt.trackers[0].name.as_str()), ("BlackTrax", "Ana"));
    }
}
