//! Speed masters: shared BPM values that effects and chases can follow, with tap tempo.
//!
//! The live BPM is stored in `Show.speed_masters[..].bpm`. Changing it at runtime (fader, tap, `SpeedSet`)
//! mutates the show silently — no revision bump / broadcast, only an autosave is scheduled — so a fader
//! drag does not flood clients with `Show` messages; clients read the live value from `Runtime.speed`.

use super::{next_id, Engine};
use crate::model::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const MIN_BPM: f32 = 20.0;
pub const MAX_BPM: f32 = 240.0;

/// Taps further apart than this restart the tap sequence.
const TAP_RESET: f64 = 2.0;
/// Taps closer than this are treated as contact bounce and ignored.
const TAP_DEBOUNCE: f64 = 0.05;
/// Number of most recent taps averaged.
const TAP_WINDOW: usize = 4;

#[derive(Default)]
pub struct SpeedState {
    master_taps: HashMap<u32, Vec<f64>>,
    chase_taps: HashMap<u32, Vec<f64>>,
}

pub fn clamp_bpm(b: f32) -> f32 {
    if b.is_finite() {
        b.clamp(MIN_BPM, MAX_BPM)
    } else {
        120.0
    }
}

/// Executor fader position (0..1) → BPM (linear 20..240).
pub fn level_to_bpm(level: f32) -> f32 {
    MIN_BPM + level.clamp(0.0, 1.0) * (MAX_BPM - MIN_BPM)
}

pub fn bpm_to_level(bpm: f32) -> f32 {
    ((clamp_bpm(bpm) - MIN_BPM) / (MAX_BPM - MIN_BPM)).clamp(0.0, 1.0)
}

/// Register a tap at `now`; returns the new BPM once at least two taps are known.
/// Averages the intervals of the last 4 taps; a gap above 2 s restarts the sequence.
pub fn tap_bpm(taps: &mut Vec<f64>, now: f64) -> Option<f32> {
    if let Some(&last) = taps.last() {
        if now - last > TAP_RESET {
            taps.clear();
        } else if now - last < TAP_DEBOUNCE {
            return None;
        }
    }
    taps.push(now);
    if taps.len() > TAP_WINDOW {
        taps.remove(0);
    }
    if taps.len() < 2 {
        return None;
    }
    let avg = (taps[taps.len() - 1] - taps[0]) / (taps.len() - 1) as f64;
    if avg <= 0.0 {
        return None;
    }
    Some(clamp_bpm((60.0 / avg) as f32))
}

impl SpeedState {
    pub fn tap_master(&mut self, id: u32, now: f64) -> Option<f32> {
        tap_bpm(self.master_taps.entry(id).or_default(), now)
    }
    pub fn tap_chase(&mut self, id: u32, now: f64) -> Option<f32> {
        tap_bpm(self.chase_taps.entry(id).or_default(), now)
    }
}

impl Engine {
    /// Schedule an autosave without announcing a show change (see module docs).
    pub(super) fn persist_quiet(&mut self) {
        self.dirty = true;
        if self.autosave_due.is_none() {
            self.autosave_due = Some(Instant::now() + Duration::from_secs(2));
        }
    }

    pub(super) fn speed_bpm(&self, id: u32) -> Option<f32> {
        self.show.speed_masters.iter().find(|m| m.id == id).map(|m| m.bpm)
    }

    /// BPM a consumer should run at: its speed master when it has a valid one, else its own value.
    pub(super) fn bpm_for(&self, master: Option<u32>, own: f32) -> f32 {
        master.and_then(|m| self.speed_bpm(m)).unwrap_or(own)
    }

    pub(super) fn speed_save(&mut self, mut master: SpeedMaster) -> Result<(), String> {
        master.bpm = clamp_bpm(master.bpm);
        if master.id == 0 {
            master.id = next_id(self.show.speed_masters.iter().map(|m| m.id));
        }
        if master.name.trim().is_empty() {
            master.name = format!("Speed {}", master.id);
        }
        match self.show.speed_masters.iter_mut().find(|m| m.id == master.id) {
            Some(m) => *m = master,
            None => self.show.speed_masters.push(master),
        }
        self.touch();
        Ok(())
    }

    pub(super) fn speed_remove(&mut self, id: u32) -> Result<(), String> {
        let before = self.show.speed_masters.len();
        self.show.speed_masters.retain(|m| m.id != id);
        if self.show.speed_masters.len() == before {
            return Err(format!("Speed master {id} does not exist"));
        }
        for e in self.show.effects.iter_mut().filter(|e| e.speed_master == Some(id)) {
            e.speed_master = None;
        }
        for c in self.show.chases.iter_mut().filter(|c| c.speed_master == Some(id)) {
            c.speed_master = None;
        }
        for x in self.show.executors.iter_mut() {
            if matches!(x.kind, ExecKind::Speed { master } if master == id) {
                x.kind = ExecKind::Empty;
            }
        }
        self.speed.master_taps.remove(&id);
        self.touch();
        Ok(())
    }

    pub(super) fn speed_set(&mut self, id: u32, bpm: f32) -> Result<(), String> {
        let bpm = clamp_bpm(bpm);
        let m = self.show.speed_masters.iter_mut().find(|m| m.id == id).ok_or_else(|| format!("Speed master {id} does not exist"))?;
        m.bpm = bpm;
        self.persist_quiet();
        Ok(())
    }

    pub(super) fn speed_tap_at(&mut self, id: u32, now: f64) -> Result<(), String> {
        if self.speed_bpm(id).is_none() {
            return Err(format!("Speed master {id} does not exist"));
        }
        if let Some(bpm) = self.speed.tap_master(id, now) {
            self.speed_set(id, bpm)?;
        }
        Ok(())
    }

    pub(super) fn speed_tap(&mut self, id: u32) -> Result<(), String> {
        let now = self.now();
        self.speed_tap_at(id, now)
    }

    pub(super) fn speed_rt(&self) -> Vec<SpeedRt> {
        self.show.speed_masters.iter().map(|m| SpeedRt { id: m.id, bpm: m.bpm }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::*;

    #[test]
    fn tap_tempo_averages_last_four_and_resets() {
        let mut taps = vec![];
        assert_eq!(tap_bpm(&mut taps, 0.0), None);
        // 0.5 s apart → 120 BPM
        assert!((tap_bpm(&mut taps, 0.5).unwrap() - 120.0).abs() < 0.01);
        assert!((tap_bpm(&mut taps, 1.0).unwrap() - 120.0).abs() < 0.01);
        // a slow tap only moves the average of the last four taps
        let b = tap_bpm(&mut taps, 2.5).unwrap(); // taps 0,0.5,1.0,2.5 → avg 0.833 s
        assert!((b - 72.0).abs() < 0.01, "{b}");
        let b = tap_bpm(&mut taps, 3.0).unwrap(); // window 0.5,1.0,2.5,3.0 → avg 0.833
        assert!((b - 72.0).abs() < 0.01, "{b}");
        // > 2 s idle restarts the sequence
        assert_eq!(tap_bpm(&mut taps, 6.0), None);
        assert!((tap_bpm(&mut taps, 6.25).unwrap() - 240.0).abs() < 0.01);
        // clamps
        let mut t = vec![];
        tap_bpm(&mut t, 0.0);
        assert!((tap_bpm(&mut t, 1.9).unwrap() - 31.58).abs() < 0.01); // slowest reachable (>2 s resets)
        let mut t = vec![];
        tap_bpm(&mut t, 0.0);
        assert_eq!(tap_bpm(&mut t, 0.1).unwrap(), MAX_BPM);
        // bounce is ignored
        let mut t = vec![];
        tap_bpm(&mut t, 0.0);
        assert_eq!(tap_bpm(&mut t, 0.01), None);
    }

    #[test]
    fn level_maps_linearly() {
        assert_eq!(level_to_bpm(0.0), 20.0);
        assert_eq!(level_to_bpm(1.0), 240.0);
        assert!((bpm_to_level(130.0) - 0.5).abs() < 1e-6);
        assert!(clamp_bpm(f32::NAN) == 120.0);
    }

    #[test]
    fn save_set_tap_remove() {
        let mut e = engine();
        e.handle(Command::SpeedSave { master: SpeedMaster { id: 0, name: String::new(), bpm: 500.0 } }).unwrap();
        assert_eq!(e.show.speed_masters[0].id, 1);
        assert_eq!(e.show.speed_masters[0].bpm, MAX_BPM);
        assert_eq!(e.show.speed_masters[0].name, "Speed 1");
        e.handle(Command::SpeedSet { id: 1, bpm: 90.0 }).unwrap();
        assert_eq!(e.speed_rt()[0].bpm, 90.0);
        e.speed_tap_at(1, 10.0).unwrap();
        e.speed_tap_at(1, 10.5).unwrap();
        assert!((e.speed_bpm(1).unwrap() - 120.0).abs() < 0.01);
        assert!(e.handle(Command::SpeedSet { id: 9, bpm: 90.0 }).is_err());
        assert!(e.handle(Command::SpeedTap { id: 9 }).is_err());
        // consumers lose the link when the master goes away
        e.handle(Command::EffectSave {
            effect: Effect { id: 0, name: "x".into(), attr: "dimmer".into(), shape: Shape::Sine, rate: 1.0, depth: 1.0, phase_spread: 0.0, fixtures: vec![], attr2: None, speed_master: Some(1), bpm: 0.0, rotation: 0.0 },
        })
        .unwrap();
        e.handle(Command::ExecSave { executor: Executor { id: 0, page: 0, slot: 0, label: "S".into(), kind: ExecKind::Speed { master: 1 } } }).unwrap();
        e.handle(Command::SpeedRemove { id: 1 }).unwrap();
        assert!(e.show.speed_masters.is_empty());
        assert_eq!(e.show.effects[0].speed_master, None);
        assert!(matches!(e.show.executors[0].kind, ExecKind::Empty));
        assert!(e.handle(Command::SpeedRemove { id: 1 }).is_err());
    }
}
