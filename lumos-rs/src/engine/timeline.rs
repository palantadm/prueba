//! Timelines: command-line events fired when the playhead passes their time.
//!
//! * `TimelineRun{id, from}` starts the playhead (`from: None` resumes where it stopped, or restarts at 0
//!   when it had finished); an event at exactly the start time fires. `TimelineStop` pauses, keeping the playhead.
//! * `TimelineSeek` moves the playhead **without firing** the events it skips (and an event exactly at the
//!   seek position does not fire either).
//! * `looped` wraps at `length`. Events fire once per pass.
//! * With `tc_start` set the playhead follows show timecode (`t = timecode − tc_start`); while the show
//!   timecode is not running the playhead holds. A timecode jump of more than 2 s (or backwards) is treated
//!   as a seek: nothing is fired.
//!
//! The advance logic is pure ([`advance`]) so it is tested with a fake clock.

use super::bindings::parse_tc;
use super::{next_id, Engine};
use crate::model::*;

#[derive(Debug, Clone, Default)]
pub struct TlState {
    pub running: bool,
    pub t: f64,
    /// Events with `last < at <= t` fire on the next advance.
    pub last: f64,
}

const EPS: f64 = 1e-6;

/// Where the playhead should be this tick.
pub enum Clock {
    /// Free-running: advance by `dt` seconds.
    Dt(f64),
    /// Following timecode: absolute playhead position in seconds (may be negative before `tc_start`),
    /// or `None` while timecode is not running (hold).
    Tc(Option<f64>),
}

/// Advance a timeline and return the texts of the events that fired (in time order).
pub fn advance(tl: &Timeline, st: &mut TlState, clock: Clock) -> Vec<String> {
    let mut fired = Vec::new();
    if !st.running {
        return fired;
    }
    let len = tl.length.max(0.0) as f64;
    let new_t = match clock {
        Clock::Dt(dt) => st.t + dt.max(0.0),
        Clock::Tc(None) => return fired,
        Clock::Tc(Some(t)) => {
            if t < 0.0 {
                st.t = 0.0;
                st.last = -EPS;
                return fired;
            }
            if t < st.t || t - st.t > 2.0 {
                // jump: behave like a seek
                st.t = t.min(len);
                st.last = st.t;
                if t >= len && !tl.looped {
                    st.running = false;
                }
                return fired;
            }
            t
        }
    };
    let mut events: Vec<&TlEvent> = tl.events.iter().collect();
    events.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
    let fire = |lo: f64, hi: f64, out: &mut Vec<String>| {
        for e in &events {
            let at = e.at as f64;
            if at > lo && at <= hi {
                out.push(e.text.clone());
            }
        }
    };
    if new_t >= len && len > 0.0 {
        fire(st.last, len, &mut fired);
        if tl.looped {
            let over = (new_t - len).min(len);
            st.last = -EPS;
            fire(st.last, over, &mut fired);
            st.t = over;
            st.last = over;
        } else {
            st.t = len;
            st.last = len;
            st.running = false;
        }
    } else {
        fire(st.last, new_t, &mut fired);
        st.t = new_t;
        st.last = new_t;
    }
    fired
}

impl Engine {
    pub(super) fn timeline_save(&mut self, mut tl: Timeline) -> Result<(), String> {
        if tl.id == 0 {
            tl.id = next_id(self.show.timelines.iter().map(|x| x.id));
        }
        if !tl.length.is_finite() || tl.length < 0.0 || tl.length > 86400.0 {
            return Err("Timeline length must be 0..86400 seconds".into());
        }
        if tl.events.len() > 2000 {
            return Err("Too many events (max 2000)".into());
        }
        for (i, ev) in tl.events.iter().enumerate() {
            if !ev.at.is_finite() || ev.at < 0.0 {
                return Err(format!("event {}: time must be ≥ 0", i + 1));
            }
            if let Err(e) = super::cmdline::parse(&ev.text) {
                return Err(format!("event {}: {}", i + 1, e.render(&ev.text)));
            }
        }
        if let Some(tc) = &tl.tc_start {
            if parse_tc(tc, self.show.settings.timecode.fps).is_none() {
                return Err(format!("Invalid timecode start '{tc}' (use HH:MM:SS:FF)"));
            }
        }
        tl.events.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
        match self.show.timelines.iter_mut().find(|x| x.id == tl.id) {
            Some(x) => *x = tl,
            None => self.show.timelines.push(tl),
        }
        self.touch();
        Ok(())
    }

    pub(super) fn timeline_remove(&mut self, id: u32) {
        self.show.timelines.retain(|t| t.id != id);
        self.auto.tl.remove(&id);
        self.touch();
    }

    fn timeline_len(&self, id: u32) -> Result<f64, String> {
        self.show.timelines.iter().find(|t| t.id == id).map(|t| t.length as f64).ok_or_else(|| format!("Timeline {id} does not exist"))
    }

    pub(super) fn timeline_run(&mut self, id: u32, from: Option<f32>) -> Result<(), String> {
        let len = self.timeline_len(id)?;
        let st = self.auto.tl.entry(id).or_default();
        let start = match from {
            Some(f) => Some((f.max(0.0) as f64).min(len)),
            None if st.t >= len || (st.t == 0.0 && !st.running) => Some(0.0),
            None => None,
        };
        match start {
            Some(s) => {
                st.t = s;
                st.last = s - EPS;
            }
            None => st.last = st.t,
        }
        st.running = true;
        self.dirty = true;
        Ok(())
    }

    pub(super) fn timeline_stop(&mut self, id: u32) -> Result<(), String> {
        self.timeline_len(id)?;
        if let Some(st) = self.auto.tl.get_mut(&id) {
            st.running = false;
        }
        self.dirty = true;
        Ok(())
    }

    pub(super) fn timeline_seek(&mut self, id: u32, t: f32) -> Result<(), String> {
        let len = self.timeline_len(id)?;
        if !t.is_finite() {
            return Err("Invalid time".into());
        }
        let st = self.auto.tl.entry(id).or_default();
        st.t = (t.max(0.0) as f64).min(len);
        st.last = st.t;
        self.dirty = true;
        Ok(())
    }

    /// Per-tick: advance running timelines and execute the events that fired.
    pub(super) fn update_timelines(&mut self, dt: f64) {
        if self.auto.tl.values().all(|s| !s.running) {
            return;
        }
        let fps = self.show.settings.timecode.fps as f64;
        let tc_on = self.tc.running && self.show.settings.timecode.source != TcSource::Off;
        let tc_now = self.tc_display() as f64;
        let mut texts: Vec<String> = Vec::new();
        for tl in &self.show.timelines {
            let Some(st) = self.auto.tl.get_mut(&tl.id) else { continue };
            if !st.running {
                continue;
            }
            let clock = match &tl.tc_start {
                Some(tc) => {
                    let start = parse_tc(tc, self.show.settings.timecode.fps).unwrap_or(0) as f64;
                    Clock::Tc(if tc_on { Some((tc_now - start) / fps.max(1.0)) } else { None })
                }
                None => Clock::Dt(dt),
            };
            texts.extend(advance(tl, st, clock));
        }
        for t in texts {
            let _ = self.run_cmdline_new(&t);
        }
        self.dirty = true;
    }

    pub(super) fn timelines_rt(&self) -> Vec<TimelineRt> {
        self.show
            .timelines
            .iter()
            .map(|t| {
                let s = self.auto.tl.get(&t.id);
                TimelineRt { id: t.id, running: s.map_or(false, |s| s.running), t: s.map_or(0.0, |s| s.t as f32) }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests_support::*;

    fn tl(events: &[(f32, &str)], length: f32, looped: bool) -> Timeline {
        Timeline {
            id: 1,
            name: "T".into(),
            length,
            events: events.iter().map(|(at, t)| TlEvent { at: *at, text: t.to_string(), label: String::new() }).collect(),
            looped,
            tc_start: None,
            audio: None,
        }
    }

    fn run_from(st: &mut TlState, from: f64) {
        st.running = true;
        st.t = from;
        st.last = from - EPS;
    }

    #[test]
    fn fires_events_in_order_as_time_passes() {
        let t = tl(&[(1.0, "a"), (0.5, "b"), (2.0, "c")], 10.0, false);
        let mut st = TlState::default();
        run_from(&mut st, 0.0);
        assert!(advance(&t, &mut st, Clock::Dt(0.4)).is_empty());
        assert_eq!(advance(&t, &mut st, Clock::Dt(0.2)), vec!["b"]);
        assert_eq!(advance(&t, &mut st, Clock::Dt(1.5)), vec!["a", "c"]);
        assert!(advance(&t, &mut st, Clock::Dt(1.0)).is_empty());
        assert!((st.t - 3.1).abs() < 1e-9);
    }

    #[test]
    fn event_at_start_fires_once() {
        let t = tl(&[(0.0, "start"), (1.0, "x")], 5.0, false);
        let mut st = TlState::default();
        run_from(&mut st, 0.0);
        assert_eq!(advance(&t, &mut st, Clock::Dt(0.05)), vec!["start"]);
        assert!(advance(&t, &mut st, Clock::Dt(0.05)).is_empty());
    }

    #[test]
    fn ends_and_stops_at_length() {
        let t = tl(&[(4.0, "last")], 5.0, false);
        let mut st = TlState::default();
        run_from(&mut st, 3.9);
        assert_eq!(advance(&t, &mut st, Clock::Dt(10.0)), vec!["last"]);
        assert!(!st.running);
        assert_eq!(st.t, 5.0);
        assert!(advance(&t, &mut st, Clock::Dt(1.0)).is_empty());
    }

    #[test]
    fn event_exactly_at_length_fires() {
        let t = tl(&[(5.0, "end")], 5.0, false);
        let mut st = TlState::default();
        run_from(&mut st, 4.0);
        assert_eq!(advance(&t, &mut st, Clock::Dt(1.0)), vec!["end"]);
        assert!(!st.running);
    }

    #[test]
    fn loop_wraps_and_refires() {
        let t = tl(&[(0.0, "zero"), (1.5, "mid"), (2.0, "end")], 2.0, true);
        let mut st = TlState::default();
        run_from(&mut st, 0.0);
        assert_eq!(advance(&t, &mut st, Clock::Dt(0.1)), vec!["zero"]);
        // wrap at exactly `length`: the closing events fire, then the new pass starts and its t=0 event fires
        assert_eq!(advance(&t, &mut st, Clock::Dt(1.9)), vec!["mid", "end", "zero"]);
        assert!(st.running);
        assert!(advance(&t, &mut st, Clock::Dt(0.2)).is_empty());
        assert_eq!(advance(&t, &mut st, Clock::Dt(1.6)), vec!["mid"]);
    }

    #[test]
    fn loop_overshoot_fires_early_events_of_next_pass() {
        let t = tl(&[(0.0, "zero"), (0.2, "early"), (1.9, "late")], 2.0, true);
        let mut st = TlState::default();
        run_from(&mut st, 1.8);
        let f = advance(&t, &mut st, Clock::Dt(0.5));
        assert_eq!(f, vec!["late", "zero", "early"]);
        assert!((st.t - 0.3).abs() < 1e-9);
    }

    #[test]
    fn stopped_timeline_does_not_advance() {
        let t = tl(&[(0.1, "x")], 5.0, false);
        let mut st = TlState::default();
        assert!(advance(&t, &mut st, Clock::Dt(1.0)).is_empty());
        assert_eq!(st.t, 0.0);
    }

    #[test]
    fn seek_never_fires_skipped_events() {
        let t = tl(&[(1.0, "a"), (3.0, "b"), (5.0, "c")], 10.0, false);
        let mut st = TlState::default();
        run_from(&mut st, 0.0);
        // seek forward over a and b
        st.t = 4.0;
        st.last = st.t;
        assert!(advance(&t, &mut st, Clock::Dt(0.5)).is_empty());
        assert_eq!(advance(&t, &mut st, Clock::Dt(1.0)), vec!["c"]);
        // seek backwards does not refire
        st.t = 0.5;
        st.last = st.t;
        assert!(advance(&t, &mut st, Clock::Dt(0.1)).is_empty());
        assert_eq!(advance(&t, &mut st, Clock::Dt(0.5)), vec!["a"]);
    }

    #[test]
    fn timecode_follow_fires_and_holds() {
        let mut t = tl(&[(1.0, "a"), (2.0, "b")], 10.0, false);
        t.tc_start = Some("00:00:10:00".into());
        let mut st = TlState::default();
        st.running = true;
        st.last = -EPS;
        // timecode not running → hold
        assert!(advance(&t, &mut st, Clock::Tc(None)).is_empty());
        assert_eq!(st.t, 0.0);
        // before the start → hold at 0
        assert!(advance(&t, &mut st, Clock::Tc(Some(-3.0))).is_empty());
        // in range, small steps
        assert!(advance(&t, &mut st, Clock::Tc(Some(0.5))).is_empty());
        assert_eq!(advance(&t, &mut st, Clock::Tc(Some(1.2))), vec!["a"]);
        // a jump of more than 2 s is a seek: nothing fires
        assert!(advance(&t, &mut st, Clock::Tc(Some(8.0))).is_empty());
        assert_eq!(st.t, 8.0);
        // backwards = seek
        assert!(advance(&t, &mut st, Clock::Tc(Some(1.5))).is_empty());
        assert_eq!(advance(&t, &mut st, Clock::Tc(Some(2.1))), vec!["b"]);
    }

    // ── engine integration ──

    #[test]
    fn save_validates_and_sorts() {
        let mut e = engine();
        e.handle(Command::TimelineSave { timeline: Timeline { id: 0, ..tl(&[(2.0, "go"), (1.0, "back")], 5.0, false) } }).unwrap();
        assert_eq!(e.show.timelines[0].id, 1);
        assert_eq!(e.show.timelines[0].events[0].text, "back");
        let err = e.handle(Command::TimelineSave { timeline: tl(&[(1.0, "wibble")], 5.0, false) }).unwrap_err();
        assert!(err.starts_with("event 1:"), "{err}");
        let err = e.handle(Command::TimelineSave { timeline: tl(&[(-1.0, "go")], 5.0, false) }).unwrap_err();
        assert!(err.contains("≥ 0"), "{err}");
        let mut bad = tl(&[], 5.0, false);
        bad.tc_start = Some("nonsense".into());
        assert!(e.handle(Command::TimelineSave { timeline: bad }).is_err());
        let mut long = tl(&[], 100000.0, false);
        long.id = 9;
        assert!(e.handle(Command::TimelineSave { timeline: long }).is_err());
    }

    #[test]
    fn engine_runs_events_and_reports_rt() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        e.handle(Command::TimelineSave { timeline: tl(&[(0.5, "fixture 1 at 100"), (1.0, "blackout on")], 3.0, false) }).unwrap();
        e.handle(Command::TimelineRun { id: 1, from: None }).unwrap();
        assert_eq!(e.timelines_rt()[0].running, true);
        e.update_timelines(0.4);
        assert!(e.programmer.is_empty());
        e.update_timelines(0.2);
        assert!(e.programmer.get(&1).is_some());
        assert!(!e.blackout);
        e.update_timelines(0.5);
        assert!(e.blackout);
        e.update_timelines(5.0);
        let rt = e.build_rt();
        assert_eq!(rt.timelines.len(), 1);
        assert!(!rt.timelines[0].running);
        assert_eq!(rt.timelines[0].t, 3.0);
    }

    #[test]
    fn stop_resume_and_seek_commands() {
        let mut e = engine();
        e.handle(Command::TimelineSave { timeline: tl(&[(1.0, "blackout on"), (4.0, "blackout off")], 6.0, false) }).unwrap();
        e.handle(Command::TimelineRun { id: 1, from: None }).unwrap();
        e.update_timelines(0.6);
        e.handle(Command::TimelineStop { id: 1 }).unwrap();
        e.update_timelines(5.0); // stopped: nothing happens
        assert!(!e.blackout);
        assert!((e.timelines_rt()[0].t - 0.6).abs() < 1e-4);
        e.handle(Command::TimelineRun { id: 1, from: None }).unwrap(); // resume
        e.update_timelines(0.6);
        assert!(e.blackout);
        // seek far ahead: skipped 'blackout on' (already) and the 4.0 event are not fired
        e.handle(Command::TimelineSeek { id: 1, t: 5.0 }).unwrap();
        e.update_timelines(0.5);
        assert!(e.blackout, "skipped event must not fire");
        // run from a time
        e.handle(Command::TimelineRun { id: 1, from: Some(3.9) }).unwrap();
        e.update_timelines(0.3);
        assert!(!e.blackout);
        assert!(e.handle(Command::TimelineRun { id: 99, from: None }).is_err());
        assert!(e.handle(Command::TimelineSeek { id: 99, t: 1.0 }).is_err());
        assert!(e.handle(Command::TimelineStop { id: 99 }).is_err());
    }

    #[test]
    fn finished_timeline_restarts_from_zero() {
        let mut e = engine();
        e.handle(Command::TimelineSave { timeline: tl(&[(0.0, "blackout toggle")], 1.0, false) }).unwrap();
        e.handle(Command::TimelineRun { id: 1, from: None }).unwrap();
        e.update_timelines(0.1);
        assert!(e.blackout);
        e.update_timelines(2.0);
        assert!(!e.timelines_rt()[0].running);
        e.handle(Command::TimelineRun { id: 1, from: None }).unwrap();
        e.update_timelines(0.1);
        assert!(!e.blackout, "toggled again from the start");
    }

    #[test]
    fn follows_show_timecode() {
        let mut e = engine();
        e.show.settings.timecode.source = TcSource::Internal;
        e.show.settings.timecode.fps = 25.0;
        let mut t = tl(&[(1.0, "blackout on")], 10.0, false);
        t.tc_start = Some("00:00:05:00".into());
        e.handle(Command::TimelineSave { timeline: t }).unwrap();
        e.handle(Command::TimelineRun { id: 1, from: None }).unwrap();
        // timecode stopped: holds
        e.update_timelines(0.5);
        assert!(!e.blackout);
        e.tc.running = true;
        e.tc.frames_f = 5.0 * 25.0 + 20.0; // 5.8 s → t = 0.8
        e.update_timelines(0.04);
        assert!(!e.blackout);
        e.tc.frames_f = 5.0 * 25.0 + 30.0; // 6.2 s → t = 1.2
        e.update_timelines(0.04);
        assert!(e.blackout);
    }

    #[test]
    fn removing_a_timeline_drops_its_state() {
        let mut e = engine();
        e.handle(Command::TimelineSave { timeline: tl(&[], 1.0, false) }).unwrap();
        e.handle(Command::TimelineRun { id: 1, from: None }).unwrap();
        e.handle(Command::TimelineRemove { id: 1 }).unwrap();
        assert!(e.auto.tl.is_empty());
        assert!(e.timelines_rt().is_empty());
    }
}
