//! MIDI Show Control (MSC) — the theatre standard for triggering cues from a show-control system.
//!
//! SysEx frame: `F0 7F <device> 02 <command_format> <command> [data…] F7`.
//! * `command_format`: 01 lighting (general) … 06 chasers, 7F all types.
//! * Commands: 01 GO, 02 STOP, 03 RESUME, 04 TIMED_GO, 05 LOAD, 08 ALL_OFF, 09 RESTORE, 0A RESET.
//! * GO/STOP/RESUME/LOAD data: ASCII cue number, `00`, ASCII cue list, `00`, ASCII cue path
//!   (every field optional, trailing separators optional). TIMED_GO has a 5-byte time first.
//!
//! Semantics in Lumos (`Settings.msc`: enabled, device id, list map):
//! * GO — run the next cue of the list (or `goto` the numbered cue); a cue armed with LOAD runs first.
//!   TIMED_GO is treated as GO (the transition time is parsed but the cue's own fade is used).
//! * STOP — pause running cuelists (the given one, or all). RESUME — continue paused ones.
//! * LOAD — arm a cue: the next MSC GO on that list runs it. Nothing is output yet.
//! * ALL_OFF — release every cuelist, scenes and effects. RESET — ALL_OFF + clear the programmer and blackout.
//! * RESTORE — accepted and ignored. Unknown commands are logged and ignored.
//! * The cue list text is looked up in `msc.map` (`msc_list` → cuelist); an unmapped list that is a plain
//!   number equal to an existing cuelist id is used directly; no list = the first cuelist.

use super::{index_of, Engine};
use crate::model::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MscCmd {
    Go,
    Stop,
    Resume,
    TimedGo,
    Load,
    AllOff,
    Restore,
    Reset,
    Other(u8),
}

impl MscCmd {
    pub fn name(self) -> String {
        match self {
            MscCmd::Go => "GO".into(),
            MscCmd::Stop => "STOP".into(),
            MscCmd::Resume => "RESUME".into(),
            MscCmd::TimedGo => "TIMED_GO".into(),
            MscCmd::Load => "LOAD".into(),
            MscCmd::AllOff => "ALL_OFF".into(),
            MscCmd::Restore => "RESTORE".into(),
            MscCmd::Reset => "RESET".into(),
            MscCmd::Other(c) => format!("cmd {c:02X}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MscMsg {
    pub device: u8,
    pub format: u8,
    pub cmd: MscCmd,
    pub cue: Option<String>,
    pub list: Option<String>,
    /// TIMED_GO transition time in seconds (approximate frame rate).
    pub time_s: Option<f32>,
}

/// Does this SysEx look like an MSC message (header only)?
pub fn is_msc(bytes: &[u8]) -> bool {
    bytes.len() >= 6 && bytes[0] == 0xF0 && bytes[1] == 0x7F && bytes[3] == 0x02
}

pub fn parse_msc(bytes: &[u8]) -> Option<MscMsg> {
    if !is_msc(bytes) {
        return None;
    }
    let end = if bytes.last() == Some(&0xF7) { bytes.len() - 1 } else { bytes.len() };
    if end < 6 {
        return None;
    }
    let data = &bytes[6..end];
    if bytes[2] > 0x7F || bytes[4] > 0x7F || bytes[5] > 0x7F || data.iter().any(|b| *b > 0x7F) {
        return None;
    }
    let cmd = match bytes[5] {
        0x01 => MscCmd::Go,
        0x02 => MscCmd::Stop,
        0x03 => MscCmd::Resume,
        0x04 => MscCmd::TimedGo,
        0x05 => MscCmd::Load,
        0x08 => MscCmd::AllOff,
        0x09 => MscCmd::Restore,
        0x0A => MscCmd::Reset,
        c => MscCmd::Other(c),
    };
    let mut time_s = None;
    let mut fields: &[u8] = data;
    if cmd == MscCmd::TimedGo {
        if data.len() < 5 {
            return None;
        }
        let (hr, mn, sc, fr) = (data[0], data[1], data[2], data[3]);
        let fps = match (hr >> 5) & 0x03 {
            0 => 24.0,
            1 => 25.0,
            _ => 30.0,
        };
        time_s = Some((hr & 0x1F) as f32 * 3600.0 + mn as f32 * 60.0 + sc as f32 + fr as f32 / fps);
        fields = &data[5..];
    }
    let parts: Vec<&[u8]> = if matches!(cmd, MscCmd::Go | MscCmd::Stop | MscCmd::Resume | MscCmd::TimedGo | MscCmd::Load) {
        fields.split(|b| *b == 0x00).collect()
    } else {
        vec![]
    };
    let mut it = parts.into_iter();
    let mut field = || -> Option<String> {
        let s = String::from_utf8_lossy(it.next().unwrap_or(&[])).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    };
    let cue = field();
    let list = field();
    Some(MscMsg { device: bytes[2], format: bytes[4], cmd, cue, list, time_s })
}

impl Engine {
    fn msc_note(&mut self, line: String) {
        self.recent.push_front(line);
        self.recent.truncate(8);
        self.dirty = true;
    }

    fn msc_cuelist(&self, list: &Option<String>) -> Result<Option<u32>, String> {
        let Some(l) = list else { return Ok(None) };
        if let Some(m) = self.show.settings.msc.map.iter().find(|m| m.msc_list.trim() == l) {
            return Ok(Some(m.cuelist));
        }
        match l.parse::<u32>() {
            Ok(n) if self.show.cuelists.iter().any(|c| c.id == n) => Ok(Some(n)),
            _ => Err(format!("cue list '{l}' is not mapped")),
        }
    }

    fn msc_default_list(&self, list: Option<u32>) -> Result<u32, String> {
        list.or_else(|| self.show.cuelists.first().map(|c| c.id)).ok_or_else(|| "no cuelists".to_string())
    }

    fn msc_release_all(&mut self) {
        let now = self.now();
        let ids: Vec<u32> = self.show.cuelists.iter().map(|c| c.id).collect();
        for id in ids {
            self.release_cuelist(id);
        }
        self.scenes_release_at(now);
        self.effects_on.clear();
        self.effect_flash.clear();
        self.dirty = true;
    }

    /// Handle an MSC SysEx. Returns `true` when the bytes were MSC-shaped (consumed).
    pub(super) fn msc_in(&mut self, _port: &str, bytes: &[u8]) -> bool {
        if !is_msc(bytes) {
            return false;
        }
        let cfg = self.show.settings.msc.clone();
        if !cfg.enabled {
            return true;
        }
        let Some(m) = parse_msc(bytes) else {
            self.msc_note("MSC: malformed message".into());
            return true;
        };
        let for_us = m.device == 0x7F || cfg.device_id == 0x7F || m.device == cfg.device_id;
        let fmt_ok = (0x01..=0x06).contains(&m.format) || m.format == 0x7F;
        if !for_us || !fmt_ok {
            return true;
        }
        let label = format!(
            "MSC {}{}{}",
            m.cmd.name(),
            m.cue.as_ref().map_or(String::new(), |c| format!(" cue {c}")),
            m.list.as_ref().map_or(String::new(), |l| format!(" list {l}"))
        );
        match self.msc_exec(&m) {
            Ok(()) => self.msc_note(label),
            Err(e) => {
                tracing::debug!("{label}: {e}");
                self.msc_note(format!("{label} — {e}"));
            }
        }
        true
    }

    fn msc_exec(&mut self, m: &MscMsg) -> Result<(), String> {
        let mapped = self.msc_cuelist(&m.list)?;
        let cue_no: Option<f64> = match &m.cue {
            Some(c) => Some(c.parse::<f64>().map_err(|_| format!("bad cue number '{c}'"))?),
            None => None,
        };
        match m.cmd {
            MscCmd::Go | MscCmd::TimedGo => {
                let cl = self.msc_default_list(mapped)?;
                match cue_no.or_else(|| self.auto.msc_loaded.remove(&cl)) {
                    Some(n) => {
                        self.auto.msc_loaded.remove(&cl);
                        self.go_to(cl, n)
                    }
                    None => self.go(cl),
                }
            }
            MscCmd::Stop => {
                let ids = self.msc_targets(mapped, true);
                for id in ids {
                    self.pause(id)?;
                }
                Ok(())
            }
            MscCmd::Resume => {
                let ids = self.msc_targets(mapped, false);
                for id in ids {
                    self.go(id)?;
                }
                Ok(())
            }
            MscCmd::Load => {
                let cl = self.msc_default_list(mapped)?;
                let n = cue_no.ok_or("LOAD needs a cue number")?;
                let list = self.require_cuelist(cl)?;
                if index_of(list, n).is_none() {
                    return Err(format!("cue {n} does not exist"));
                }
                self.auto.msc_loaded.insert(cl, n);
                Ok(())
            }
            MscCmd::AllOff => {
                self.auto.msc_loaded.clear();
                self.msc_release_all();
                Ok(())
            }
            MscCmd::Reset => {
                self.auto.msc_loaded.clear();
                self.msc_release_all();
                self.programmer.clear();
                self.blackout = false;
                Ok(())
            }
            MscCmd::Restore => Ok(()),
            MscCmd::Other(c) => Err(format!("unsupported command {c:02X}")),
        }
    }

    /// Cuelists a STOP/RESUME applies to: the mapped one, or every running (stop) / paused (resume) list.
    fn msc_targets(&self, mapped: Option<u32>, stopping: bool) -> Vec<u32> {
        use crate::model::PlayState;
        let want = if stopping { PlayState::Running } else { PlayState::Paused };
        match mapped {
            Some(id) => vec![id],
            None => self.runs.values().filter(|r| r.state == want).map(|r| r.cuelist).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests_support::*;

    /// `F0 7F <dev> 02 <fmt> <cmd> <data…> F7`
    fn frame(dev: u8, fmt: u8, cmd: u8, data: &[u8]) -> Vec<u8> {
        let mut v = vec![0xF0, 0x7F, dev, 0x02, fmt, cmd];
        v.extend_from_slice(data);
        v.push(0xF7);
        v
    }

    fn cue_data(cue: &str, list: Option<&str>) -> Vec<u8> {
        let mut d = cue.as_bytes().to_vec();
        if let Some(l) = list {
            d.push(0);
            d.extend_from_slice(l.as_bytes());
        }
        d
    }

    // ── parser (byte-exact) ──

    #[test]
    fn parse_go_without_data() {
        let m = parse_msc(&[0xF0, 0x7F, 0x01, 0x02, 0x01, 0x01, 0xF7]).unwrap();
        assert_eq!((m.device, m.format, m.cmd, m.cue, m.list), (1, 1, MscCmd::Go, None, None));
    }

    #[test]
    fn parse_go_cue_and_list() {
        // GO cue "1.5" list "1": F0 7F 7F 02 01 01 31 2E 35 00 31 F7
        let m = parse_msc(&[0xF0, 0x7F, 0x7F, 0x02, 0x01, 0x01, 0x31, 0x2E, 0x35, 0x00, 0x31, 0xF7]).unwrap();
        assert_eq!(m.device, 0x7F);
        assert_eq!(m.cmd, MscCmd::Go);
        assert_eq!(m.cue.as_deref(), Some("1.5"));
        assert_eq!(m.list.as_deref(), Some("1"));
    }

    #[test]
    fn parse_cue_path_is_ignored_and_empty_cue_with_list() {
        let m = parse_msc(&[0xF0, 0x7F, 0x00, 0x02, 0x01, 0x01, 0x32, 0x00, 0x33, 0x00, 0x34, 0x2E, 0x31, 0xF7]).unwrap();
        assert_eq!((m.cue.as_deref(), m.list.as_deref()), (Some("2"), Some("3")));
        let m = parse_msc(&[0xF0, 0x7F, 0x00, 0x02, 0x01, 0x01, 0x00, 0x35, 0xF7]).unwrap();
        assert_eq!((m.cue, m.list.as_deref()), (None, Some("5")));
    }

    #[test]
    fn parse_other_commands() {
        for (c, want) in [(2, MscCmd::Stop), (3, MscCmd::Resume), (5, MscCmd::Load), (8, MscCmd::AllOff), (9, MscCmd::Restore), (0x0A, MscCmd::Reset)] {
            assert_eq!(parse_msc(&frame(1, 1, c, &[])).unwrap().cmd, want);
        }
        assert_eq!(parse_msc(&frame(1, 1, 0x20, &[])).unwrap().cmd, MscCmd::Other(0x20));
    }

    #[test]
    fn parse_timed_go() {
        // time 00:00:02:12 @25fps (type bits 01 in hr) then cue "7"
        let m = parse_msc(&frame(1, 1, 0x04, &[0x20, 0x00, 0x02, 12, 0x00, 0x37])).unwrap();
        assert_eq!(m.cmd, MscCmd::TimedGo);
        assert!((m.time_s.unwrap() - 2.48).abs() < 1e-3);
        assert_eq!(m.cue.as_deref(), Some("7"));
        assert!(parse_msc(&frame(1, 1, 0x04, &[0x20, 0x00])).is_none());
    }

    #[test]
    fn parse_without_trailing_f7() {
        let m = parse_msc(&[0xF0, 0x7F, 0x01, 0x02, 0x01, 0x01, 0x34]).unwrap();
        assert_eq!(m.cue.as_deref(), Some("4"));
    }

    #[test]
    fn parse_rejects_non_msc_and_bad_bytes() {
        // MTC full frame (universal real-time, sub-id 01) is not MSC
        let mtc = [0xF0, 0x7F, 0x7F, 0x01, 0x01, 0x01, 0x02, 0x03, 0x04, 0xF7];
        assert!(!is_msc(&mtc));
        assert!(parse_msc(&mtc).is_none());
        assert!(parse_msc(&[0xF0, 0x7E, 0x01, 0x02, 0x01, 0x01, 0xF7]).is_none());
        assert!(parse_msc(&[0xF0, 0x7F, 0x01, 0x02, 0x01, 0x01, 0x90, 0xF7]).is_none());
        assert!(parse_msc(&[0xB0, 0x07, 0x7F]).is_none());
        assert!(parse_msc(&[0xF0, 0x7F, 0x01]).is_none());
    }

    // ── engine ──

    fn show_with_lists() -> crate::engine::Engine {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        for (list, vals) in [(1u32, [100.0f32, 50.0, 20.0]), (2, [10.0, 20.0, 30.0])] {
            for (i, v) in vals.iter().enumerate() {
                e.handle(Command::CommandLine { text: format!("fixture 1 at {v}; record cue {} cuelist {list} release", i + 1) }).unwrap();
            }
        }
        for cl in e.show.cuelists.iter_mut() {
            for c in cl.cues.iter_mut() {
                c.fade_in = 0.0;
                c.fade_out = 0.0;
            }
        }
        e.show.settings.msc = MscCfg { enabled: true, device_id: 1, map: vec![MscMap { msc_list: "7".into(), cuelist: 2 }] };
        e
    }

    fn cur(e: &crate::engine::Engine, cl: u32) -> Option<f64> {
        e.runs.get(&cl).and_then(|r| r.current)
    }

    fn send(e: &mut crate::engine::Engine, bytes: Vec<u8>) {
        e.handle(Command::MidiIn { port: "t".into(), bytes }).unwrap();
    }

    #[test]
    fn disabled_is_ignored() {
        let mut e = show_with_lists();
        e.show.settings.msc.enabled = false;
        send(&mut e, frame(1, 1, 0x01, &[]));
        assert_eq!(cur(&e, 1), None);
    }

    #[test]
    fn go_runs_next_cue_of_first_list() {
        let mut e = show_with_lists();
        send(&mut e, frame(1, 1, 0x01, &[]));
        assert_eq!(cur(&e, 1), Some(1.0));
        send(&mut e, frame(1, 1, 0x01, &[]));
        assert_eq!(cur(&e, 1), Some(2.0));
        assert!(e.recent.iter().any(|l| l.starts_with("MSC GO")), "{:?}", e.recent);
    }

    #[test]
    fn go_with_cue_number_and_mapped_list() {
        let mut e = show_with_lists();
        send(&mut e, frame(1, 1, 0x01, &cue_data("3", None)));
        assert_eq!(cur(&e, 1), Some(3.0));
        send(&mut e, frame(1, 1, 0x01, &cue_data("2", Some("7")))); // list "7" → cuelist 2
        assert_eq!(cur(&e, 2), Some(2.0));
        send(&mut e, frame(1, 1, 0x01, &cue_data("1", Some("2")))); // numeric fallback → cuelist 2
        assert_eq!(cur(&e, 2), Some(1.0));
    }

    #[test]
    fn unmapped_list_is_reported_not_run() {
        let mut e = show_with_lists();
        send(&mut e, frame(1, 1, 0x01, &cue_data("1", Some("99"))));
        assert_eq!(cur(&e, 1), None);
        assert!(e.recent.front().unwrap().contains("not mapped"), "{:?}", e.recent);
    }

    #[test]
    fn unknown_cue_is_reported() {
        let mut e = show_with_lists();
        send(&mut e, frame(1, 1, 0x01, &cue_data("42", None)));
        assert_eq!(cur(&e, 1), None);
        assert!(e.recent.front().unwrap().contains("does not exist"), "{:?}", e.recent);
        send(&mut e, frame(1, 1, 0x01, &cue_data("abc", None)));
        assert!(e.recent.front().unwrap().contains("bad cue number"));
    }

    #[test]
    fn device_id_filtering() {
        let mut e = show_with_lists();
        send(&mut e, frame(2, 1, 0x01, &[]));
        assert_eq!(cur(&e, 1), None, "other device");
        send(&mut e, frame(0x7F, 1, 0x01, &[]));
        assert_eq!(cur(&e, 1), Some(1.0), "all-call");
        e.show.settings.msc.device_id = 0x7F;
        send(&mut e, frame(55, 1, 0x01, &[]));
        assert_eq!(cur(&e, 1), Some(2.0), "we listen to everything");
    }

    #[test]
    fn command_format_filtering() {
        let mut e = show_with_lists();
        send(&mut e, frame(1, 0x20, 0x01, &[]));
        assert_eq!(cur(&e, 1), None, "unsupported device type");
        send(&mut e, frame(1, 0x02, 0x01, &[]));
        assert_eq!(cur(&e, 1), Some(1.0), "moving lights format accepted");
        send(&mut e, frame(1, 0x7F, 0x01, &[]));
        assert_eq!(cur(&e, 1), Some(2.0), "all types accepted");
    }

    #[test]
    fn load_arms_the_next_go() {
        let mut e = show_with_lists();
        send(&mut e, frame(1, 1, 0x05, &cue_data("3", None)));
        assert_eq!(cur(&e, 1), None, "LOAD outputs nothing");
        send(&mut e, frame(1, 1, 0x01, &[]));
        assert_eq!(cur(&e, 1), Some(3.0));
        send(&mut e, frame(1, 1, 0x01, &[]));
        assert_eq!(cur(&e, 1), Some(3.0), "armed cue is consumed (end of list, no loop)");
        // LOAD errors
        send(&mut e, frame(1, 1, 0x05, &cue_data("9", None)));
        assert!(e.recent.front().unwrap().contains("does not exist"));
        send(&mut e, frame(1, 1, 0x05, &[]));
        assert!(e.recent.front().unwrap().contains("needs a cue number"));
    }

    #[test]
    fn stop_and_resume() {
        let mut e = show_with_lists();
        e.show.cuelists[0].cues[0].fade_in = 5.0;
        send(&mut e, frame(1, 1, 0x01, &[]));
        assert_eq!(e.runs[&1].state, PlayState::Running);
        send(&mut e, frame(1, 1, 0x02, &[]));
        assert_eq!(e.runs[&1].state, PlayState::Paused);
        send(&mut e, frame(1, 1, 0x03, &[]));
        assert_eq!(e.runs[&1].state, PlayState::Running);
        // explicit list
        send(&mut e, frame(1, 1, 0x02, &cue_data("", Some("1"))));
        assert_eq!(e.runs[&1].state, PlayState::Paused);
        send(&mut e, frame(1, 1, 0x03, &cue_data("", Some("1"))));
        assert_eq!(e.runs[&1].state, PlayState::Running);
    }

    #[test]
    fn all_off_releases_everything() {
        let mut e = show_with_lists();
        send(&mut e, frame(1, 1, 0x01, &[]));
        send(&mut e, frame(1, 1, 0x01, &cue_data("1", Some("7"))));
        e.effects_on.insert(1);
        send(&mut e, frame(1, 1, 0x08, &[]));
        assert_eq!(cur(&e, 1), None);
        assert_eq!(cur(&e, 2), None);
        assert!(e.effects_on.is_empty());
        send(&mut e, frame(1, 1, 0x01, &[]));
        assert_eq!(cur(&e, 1), Some(1.0), "can start again");
    }

    #[test]
    fn reset_also_clears_programmer_and_blackout() {
        let mut e = show_with_lists();
        e.handle(Command::CommandLine { text: "fixture 1 at 70; blackout on".into() }).unwrap();
        send(&mut e, frame(1, 1, 0x0A, &[]));
        assert!(e.programmer.is_empty());
        assert!(!e.blackout);
    }

    #[test]
    fn restore_and_unknown_are_harmless() {
        let mut e = show_with_lists();
        send(&mut e, frame(1, 1, 0x09, &[]));
        assert_eq!(cur(&e, 1), None);
        send(&mut e, frame(1, 1, 0x21, &[]));
        assert!(e.recent.front().unwrap().contains("unsupported command"));
    }

    #[test]
    fn go_with_no_cuelists_is_reported() {
        let mut e = engine();
        e.show.settings.msc = MscCfg { enabled: true, device_id: 1, map: vec![] };
        send(&mut e, frame(1, 1, 0x01, &[]));
        assert!(e.recent.front().unwrap().contains("no cuelists"), "{:?}", e.recent);
    }

    #[test]
    fn msc_does_not_disturb_other_midi() {
        let mut e = show_with_lists();
        e.show.settings.msc.enabled = true;
        // a normal CC still goes through the regular path (no panic, no MSC side effects)
        send(&mut e, vec![0xB0, 7, 100]);
        send(&mut e, vec![0xF0, 0x43, 0x10, 0x4C, 0xF7]); // vendor SysEx: ignored
        assert_eq!(cur(&e, 1), None);
    }

    #[test]
    fn timed_go_acts_as_go() {
        let mut e = show_with_lists();
        send(&mut e, frame(1, 1, 0x04, &[0x20, 0x00, 0x02, 0x00, 0x00, 0x32]));
        assert_eq!(cur(&e, 1), Some(2.0));
    }
}
