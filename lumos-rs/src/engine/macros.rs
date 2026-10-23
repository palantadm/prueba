//! Macros: sequences of command-line statements with waits, run on the engine clock.
//!
//! A macro run executes steps one after another; `wait` seconds after a step delay the next one
//! without blocking the engine. Macros may start other macros (nesting limit [`MAX_DEPTH`]).
//! A step that fails aborts that run (the error is in `Runtime.cmd_log`). Selection persists across
//! the steps of one run (one interpreter session).

use super::cmdline::{parse, CmdLog, Session};
use super::timeline::TlState;
use super::{next_id, Engine};
use crate::model::*;
use std::collections::HashMap;

pub const MAX_DEPTH: u8 = 8;

#[derive(Debug, Clone)]
pub struct MacroRun {
    pub id: u32,
    pub idx: usize,
    /// Engine time at which the next step is due.
    pub next_at: f64,
    pub session: Session,
    pub depth: u8,
}

#[derive(Default)]
pub struct Automation {
    pub log: CmdLog,
    pub macros: Vec<MacroRun>,
    /// Ids stopped while `update_macros` was iterating (`None` = all).
    pub stopped: Vec<Option<u32>>,
    pub tl: HashMap<u32, TlState>,
    /// MSC LOAD: cue armed for the next MSC GO, per cuelist.
    pub msc_loaded: HashMap<u32, f64>,
    /// Nesting depth of the macro step currently executing (0 = user command).
    pub depth: u8,
    /// Macro steps being executed right now (outermost first) with a "cancelled" flag, so a step can stop its own run.
    pub stack: Vec<(u32, bool)>,
}

impl Automation {
    pub fn reset(&mut self) {
        self.macros.clear();
        self.stopped.clear();
        self.tl.clear();
        self.msc_loaded.clear();
        self.depth = 0;
        self.stack.clear();
    }
}

impl Engine {
    pub(super) fn macro_save(&mut self, mut m: Macro) -> Result<(), String> {
        if m.id == 0 {
            m.id = next_id(self.show.macros.iter().map(|x| x.id));
        }
        for (i, s) in m.steps.iter().enumerate() {
            if let Err(e) = parse(&s.text) {
                return Err(format!("step {}: {}", i + 1, e.render(&s.text)));
            }
            if !s.wait.is_finite() || s.wait < 0.0 || s.wait > 86400.0 {
                return Err(format!("step {}: wait must be 0..86400 seconds", i + 1));
            }
        }
        match self.show.macros.iter_mut().find(|x| x.id == m.id) {
            Some(x) => *x = m,
            None => self.show.macros.push(m),
        }
        self.touch();
        Ok(())
    }

    pub(super) fn macro_remove(&mut self, id: u32) {
        self.show.macros.retain(|m| m.id != id);
        self.macro_stop(Some(id));
        self.touch();
    }

    pub(super) fn macro_run(&mut self, id: u32) -> Result<(), String> {
        if !self.show.macros.iter().any(|m| m.id == id) {
            return Err(format!("Macro {id} does not exist"));
        }
        let depth = self.auto.depth + 1;
        if depth > MAX_DEPTH {
            return Err(format!("Macro nesting too deep (max {MAX_DEPTH})"));
        }
        self.auto.macros.retain(|r| r.id != id);
        let now = self.now();
        let mut run = MacroRun { id, idx: 0, next_at: now, session: Session::default(), depth };
        if !self.step_macro(&mut run, now) {
            self.auto.macros.push(run);
        }
        self.dirty = true;
        Ok(())
    }

    pub(super) fn macro_stop(&mut self, id: Option<u32>) {
        for (rid, cancelled) in self.auto.stack.iter_mut() {
            if id.map_or(true, |i| i == *rid) {
                *cancelled = true;
            }
        }
        match id {
            Some(i) => self.auto.macros.retain(|r| r.id != i),
            None => self.auto.macros.clear(),
        }
        self.auto.stopped.push(id);
    }

    /// Execute due steps of `run`. Returns true when the run is finished.
    fn step_macro(&mut self, run: &mut MacroRun, now: f64) -> bool {
        let Some(steps) = self.show.macros.iter().find(|m| m.id == run.id).map(|m| m.steps.clone()) else { return true };
        while run.next_at <= now {
            let Some(step) = steps.get(run.idx) else { return true };
            run.idx += 1;
            let saved = std::mem::replace(&mut self.auto.depth, run.depth);
            self.auto.stack.push((run.id, false));
            let r = self.run_cmdline(&step.text, &mut run.session);
            let cancelled = self.auto.stack.pop().map_or(false, |(_, c)| c);
            self.auto.depth = saved;
            if r.is_err() || cancelled {
                return true;
            }
            if step.wait > 0.0 {
                run.next_at += step.wait as f64;
            }
        }
        run.idx >= steps.len() && now >= run.next_at
    }

    pub(super) fn update_macros(&mut self, now: f64) {
        if self.auto.macros.is_empty() {
            return;
        }
        let cur = std::mem::take(&mut self.auto.macros);
        self.auto.stopped.clear();
        let mut keep = Vec::new();
        for mut r in cur {
            let stopped = self.auto.stopped.iter().any(|s| s.map_or(true, |i| i == r.id));
            let restarted = self.auto.macros.iter().any(|n| n.id == r.id);
            if stopped || restarted {
                continue;
            }
            if !self.step_macro(&mut r, now) {
                keep.push(r);
            }
        }
        let stopped = std::mem::take(&mut self.auto.stopped);
        let started = std::mem::take(&mut self.auto.macros);
        keep.retain(|r| !stopped.iter().any(|s| s.map_or(true, |i| i == r.id)));
        keep.extend(started);
        self.auto.macros = keep;
    }

    pub(super) fn macros_running_ids(&self) -> Vec<u32> {
        let mut v: Vec<u32> = self.auto.macros.iter().map(|r| r.id).collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::tests_support::*;
    use crate::model::*;

    fn macro_of(id: u32, steps: &[(&str, f32)]) -> Macro {
        Macro { id, name: format!("M{id}"), steps: steps.iter().map(|(t, w)| MacroStep { text: t.to_string(), wait: *w }).collect(), color: String::new() }
    }

    #[test]
    fn save_validates_syntax_and_assigns_ids() {
        let mut e = engine();
        e.handle(Command::MacroSave { r#macro: macro_of(0, &[("go", 0.0)]) }).unwrap();
        assert_eq!(e.show.macros[0].id, 1);
        let err = e.handle(Command::MacroSave { r#macro: macro_of(0, &[("go", 0.0), ("wibble", 0.0)]) }).unwrap_err();
        assert!(err.starts_with("step 2:"), "{err}");
        assert_eq!(e.show.macros.len(), 1);
        let err = e.handle(Command::MacroSave { r#macro: macro_of(0, &[("go", -1.0)]) }).unwrap_err();
        assert!(err.contains("wait"), "{err}");
    }

    #[test]
    fn run_executes_immediate_steps_then_waits() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 2);
        e.handle(Command::MacroSave { r#macro: macro_of(1, &[("fixture 1 at 100", 1.0), ("fixture 2 at 100", 0.0)]) }).unwrap();
        e.handle(Command::MacroRun { id: 1 }).unwrap();
        assert!(e.programmer.get(&1).is_some());
        assert!(e.programmer.get(&2).is_none());
        assert_eq!(e.macros_running_ids(), vec![1]);
        let t0 = e.now();
        e.update_macros(t0 + 0.5);
        assert!(e.programmer.get(&2).is_none());
        e.update_macros(t0 + 1.1);
        assert!(e.programmer.get(&2).is_some());
        assert!(e.macros_running_ids().is_empty());
    }

    #[test]
    fn selection_persists_across_steps() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 3);
        e.handle(Command::MacroSave { r#macro: macro_of(1, &[("fixture 1 thru 3", 0.0), ("red at 100", 0.0)]) }).unwrap();
        e.handle(Command::MacroRun { id: 1 }).unwrap();
        assert_eq!(e.programmer.len(), 3);
    }

    #[test]
    fn failing_step_aborts_the_run() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        e.handle(Command::MacroSave { r#macro: macro_of(1, &[("fixture 9 at 100", 0.0), ("fixture 1 at 100", 0.0)]) }).unwrap();
        e.handle(Command::MacroRun { id: 1 }).unwrap();
        assert!(e.programmer.is_empty());
        assert!(e.macros_running_ids().is_empty());
        assert!(e.auto.log.0.iter().any(|l| !l.ok));
    }

    #[test]
    fn macros_nest_and_depth_is_limited() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        e.handle(Command::MacroSave { r#macro: macro_of(1, &[("macro 2", 0.0)]) }).unwrap();
        e.handle(Command::MacroSave { r#macro: macro_of(2, &[("fixture 1 at 40", 0.0)]) }).unwrap();
        e.handle(Command::MacroRun { id: 1 }).unwrap();
        assert!((e.programmer[&1]["dimmer"] - 0.4).abs() < 1e-6);
        // self-recursion: bounded by the nesting limit, no stack overflow
        e.handle(Command::MacroSave { r#macro: macro_of(3, &[("macro 3", 0.0)]) }).unwrap();
        e.handle(Command::MacroRun { id: 3 }).unwrap();
        assert!(e.auto.log.0.iter().any(|l| l.msg.contains("nesting too deep")), "{:?}", e.auto.log.0.back());
    }

    #[test]
    fn stop_and_remove() {
        let mut e = engine();
        e.handle(Command::MacroSave { r#macro: macro_of(1, &[("blackout on", 5.0), ("blackout off", 0.0)]) }).unwrap();
        e.handle(Command::MacroRun { id: 1 }).unwrap();
        assert!(e.blackout);
        assert_eq!(e.macros_running_ids(), vec![1]);
        e.handle(Command::MacroStop { id: Some(1) }).unwrap();
        assert!(e.macros_running_ids().is_empty());
        let t = e.now();
        e.update_macros(t + 10.0);
        assert!(e.blackout, "stopped macro must not continue");
        e.handle(Command::MacroRun { id: 1 }).unwrap();
        e.handle(Command::MacroStop { id: None }).unwrap();
        assert!(e.macros_running_ids().is_empty());
        e.handle(Command::MacroRemove { id: 1 }).unwrap();
        assert!(e.show.macros.is_empty());
        assert!(e.handle(Command::MacroRun { id: 1 }).is_err());
    }

    #[test]
    fn rerun_restarts() {
        let mut e = engine();
        e.handle(Command::MacroSave { r#macro: macro_of(1, &[("blackout toggle", 5.0), ("go", 0.0)]) }).unwrap();
        e.handle(Command::MacroRun { id: 1 }).unwrap();
        e.handle(Command::MacroRun { id: 1 }).unwrap();
        assert_eq!(e.macros_running_ids(), vec![1]);
        assert!(!e.blackout, "two toggles");
    }

    #[test]
    fn macro_can_stop_itself_from_a_step() {
        let mut e = engine();
        e.handle(Command::MacroSave { r#macro: macro_of(1, &[("blackout on", 1.0), ("macro stop 1", 0.0), ("blackout off", 0.0)]) }).unwrap();
        e.handle(Command::MacroRun { id: 1 }).unwrap();
        let t = e.now();
        e.update_macros(t + 1.5);
        assert!(e.blackout, "steps after 'macro stop' must not run");
        assert!(e.macros_running_ids().is_empty());
    }
}
