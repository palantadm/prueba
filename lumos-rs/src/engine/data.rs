//! Data & network features of the engine: show backups, Art-Net node table, output test pattern,
//! fixture identify. Kept in one place; `mod.rs` only has small hooks calling into here.

use super::*;
use crate::backups::Backups;

/// Output test auto-off.
const TEST_MAX: Duration = Duration::from_secs(300);
/// Automatic backup interval (only when the show changed).
const AUTO_BACKUP: Duration = Duration::from_secs(600);
/// ArtPoll cadence while the settings UI asks for discovery, and how long one `NodesScan` keeps it going.
const POLL_EVERY: Duration = Duration::from_secs(5);
const SCAN_FOR: Duration = Duration::from_secs(30);

pub(super) struct TestState {
    pub mode: TestMode,
    pub started: Instant,
}

pub(super) struct DataRt {
    pub backups: Backups,
    pub backups_dirty: bool,
    /// The show changed since the last backup.
    pub changed: bool,
    pub last_auto: Instant,
    /// Label for the automatic backup taken when the show is replaced (restore sets a specific one).
    pub replace_label: Option<&'static str>,
    pub scan_until: Option<Instant>,
    pub last_poll: Option<Instant>,
    pub nodes_version: Option<u32>,
    pub nodes_sent: Instant,
    pub test: Option<TestState>,
    pub identifying: BTreeSet<u32>,
}

impl DataRt {
    pub fn new(store: &Store) -> Self {
        DataRt {
            backups: Backups::new(store.backups_dir()),
            backups_dirty: true,
            changed: false,
            last_auto: Instant::now(),
            replace_label: None,
            scan_until: None,
            last_poll: None,
            nodes_version: None,
            nodes_sent: Instant::now(),
            test: None,
            identifying: BTreeSet::new(),
        }
    }
}

/// Normalised "open / no effect" position of a channel with named capabilities.
fn open_value(c: &ChannelDef) -> Option<f32> {
    if c.capabilities.is_empty() {
        return None;
    }
    let center = |k: &Capability| (k.min as f32 + k.max as f32) / 2.0 / 255.0;
    let lower = |k: &Capability| k.label.to_lowercase();
    c.capabilities
        .iter()
        .find(|k| lower(k).contains("open"))
        .or_else(|| c.capabilities.iter().find(|k| lower(k).contains("white") || lower(k).contains("no function")))
        .or_else(|| {
            c.capabilities.iter().find(|k| {
                let l = lower(k);
                !["clos", "black", "strobe", "pulse", "random", "ramp", "rotat", "spin", "macro"].iter().any(|w| l.contains(w))
            })
        })
        .map(center)
}

impl Engine {
    // ───────────── commands ─────────────

    pub(super) fn data_command(&mut self, cmd: Command) -> Result<(), String> {
        use Command::*;
        match cmd {
            BackupCreate { label } => {
                let l = if label.trim().is_empty() { "Manual".to_string() } else { label };
                self.data.backups.create(&self.show, &l, "manual")?;
                self.data.backups_dirty = true;
            }
            BackupRestore { id } => {
                let show = self.data.backups.load(&id)?;
                self.data.replace_label = Some("Antes de restaurar");
                self.set_show(show);
                self.data.backups_dirty = true;
            }
            BackupDelete { id } => {
                self.data.backups.delete(&id)?;
                self.data.backups_dirty = true;
            }
            NodesScan => {
                self.data.scan_until = Some(Instant::now() + SCAN_FOR);
                self.data.last_poll = Some(Instant::now());
                self.io.nodes_scan();
            }
            OutputTest { mode } => {
                match &mode {
                    TestMode::Off => self.data.test = None,
                    TestMode::Level { universe, .. } | TestMode::Chase { universe } => {
                        if let Some(u) = universe {
                            if !self.frames.contains_key(u) {
                                return Err(format!("Universe {u} does not exist"));
                            }
                        }
                        self.data.test = Some(TestState { mode, started: Instant::now() });
                    }
                    TestMode::Channel { universe, channel, .. } => {
                        if !self.frames.contains_key(universe) {
                            return Err(format!("Universe {universe} does not exist"));
                        }
                        if !(1..=512).contains(channel) {
                            return Err("Channel must be 1-512".into());
                        }
                        self.data.test = Some(TestState { mode, started: Instant::now() });
                    }
                }
                self.dirty = true;
            }
            Identify { fixture, on } => {
                if on {
                    if !self.show.fixtures.iter().any(|f| f.id == fixture) {
                        return Err(format!("Fixture {fixture} does not exist"));
                    }
                    self.data.identifying.insert(fixture);
                } else {
                    self.data.identifying.remove(&fixture);
                }
                self.dirty = true;
            }
            _ => {}
        }
        Ok(())
    }

    // ───────────── messages ─────────────

    pub(super) fn backups_json(&self) -> String {
        json(&ServerMsg::Backups { items: self.data.backups.list() })
    }

    pub(super) fn nodes_json(&self) -> String {
        json(&ServerMsg::Nodes { nodes: self.io.nodes().1 })
    }

    // ───────────── per tick ─────────────

    pub(super) fn data_tick(&mut self) {
        let now = Instant::now();
        if self.data.changed && now.duration_since(self.data.last_auto) >= AUTO_BACKUP {
            self.backup_auto("Automática");
        }
        if self.data.backups_dirty {
            self.data.backups_dirty = false;
            let j = self.backups_json();
            let _ = self.out.send(Out::Json(Arc::from(j)));
        }
        let scanning = self.data.scan_until.map_or(false, |t| now < t);
        if scanning {
            if self.data.last_poll.map_or(true, |t| now.duration_since(t) >= POLL_EVERY) {
                self.data.last_poll = Some(now);
                self.io.nodes_scan();
            }
        } else {
            self.data.scan_until = None;
        }
        let (v, nodes) = self.io.nodes();
        if self.data.nodes_version != Some(v) || (scanning && now.duration_since(self.data.nodes_sent) >= POLL_EVERY) {
            self.data.nodes_version = Some(v);
            self.data.nodes_sent = now;
            let j = json(&ServerMsg::Nodes { nodes });
            let _ = self.out.send(Out::Json(Arc::from(j)));
        }
        if self.data.test.as_ref().map_or(false, |t| t.started.elapsed() >= TEST_MAX) {
            self.data.test = None;
            self.dirty = true;
        }
    }

    pub(super) fn data_continuous(&self) -> bool {
        self.data.test.is_some() || !self.data.identifying.is_empty()
    }

    /// Automatic snapshot of the show that is about to be replaced (load / new / import / restore).
    pub(super) fn data_before_replace(&mut self) {
        let label = self.data.replace_label.take().unwrap_or("Antes de cargar otro show");
        if self.data.changed || !self.show.fixtures.is_empty() {
            self.backup_auto(label);
        }
        self.data.changed = false;
    }

    pub(super) fn data_on_exit(&mut self) {
        if self.data.changed {
            self.backup_auto("Al cerrar");
        }
    }

    fn backup_auto(&mut self, label: &str) {
        match self.data.backups.create(&self.show, label, "auto") {
            Ok(_) => {}
            Err(e) => tracing::warn!("backup failed: {e}"),
        }
        self.data.changed = false;
        self.data.last_auto = Instant::now();
        self.data.backups_dirty = true;
    }

    // ───────────── render layers ─────────────

    /// Locate layer: above masters (works in blackout). Replaces the fixture's attributes entirely.
    pub(super) fn identify_layer(&self, merged: &mut PerFixture, now: f64) {
        if self.data.identifying.is_empty() {
            return;
        }
        let on = (now * 2.0).fract() < 0.5; // 2 Hz strobe
        let level = if on { 1.0 } else { 0.0 };
        for id in &self.data.identifying {
            let Some(pf) = self.patch.get(*id) else { continue };
            let mut a = Attrs::new();
            if pf.has_dimmer {
                a.insert("dimmer".into(), level);
                for c in COLOR_ATTRS.iter().filter(|c| **c != "uv") {
                    if pf.has_attr(c) {
                        a.insert((*c).into(), 1.0);
                    }
                }
            } else {
                let mut any = false;
                for c in COLOR_ATTRS.iter().filter(|c| **c != "uv") {
                    if pf.has_attr(c) {
                        a.insert((*c).into(), level);
                        any = true;
                    }
                }
                if !any {
                    a.insert("dimmer".into(), level);
                }
            }
            for ch in &pf.chans {
                match ch.attr.as_str() {
                    "pan" | "tilt" => {
                        a.insert(ch.attr.clone(), if ch.default.is_finite() { ch.default.clamp(0.0, 1.0) } else { 0.5 });
                    }
                    "shutter" => {
                        a.insert("shutter".into(), open_value(ch).unwrap_or(1.0));
                    }
                    "colorwheel" | "gobo" | "gobo2" | "prism" | "iris" | "frost" => {
                        if let Some(v) = open_value(ch) {
                            a.insert(ch.attr.clone(), v);
                        } else {
                            a.insert(ch.attr.clone(), ch.default);
                        }
                    }
                    _ => {}
                }
            }
            merged.insert(*id, a);
        }
    }

    /// Output-test pattern over the finished frames (runtime-only).
    pub(super) fn apply_output_test(&self, frames: &mut BTreeMap<u16, Box<[u8; 512]>>) {
        if let Some(t) = &self.data.test {
            apply_test(&t.mode, t.started.elapsed().as_millis() as u64, frames);
        }
    }
}

/// Pure pattern generator (testable): `elapsed_ms` drives the chase.
pub(super) fn apply_test(mode: &TestMode, elapsed_ms: u64, frames: &mut BTreeMap<u16, Box<[u8; 512]>>) {
    match mode {
        TestMode::Off => {}
        TestMode::Level { universe, level } => {
            for (u, fr) in frames.iter_mut() {
                if universe.map_or(true, |x| x == *u) {
                    fr.fill(*level);
                }
            }
        }
        TestMode::Chase { universe } => {
            let step = ((elapsed_ms / 150) % 512) as usize;
            for (u, fr) in frames.iter_mut() {
                if universe.map_or(true, |x| x == *u) {
                    fr.fill(0);
                    fr[step] = 255;
                }
            }
        }
        TestMode::Channel { universe, channel, level } => {
            if let Some(fr) = frames.get_mut(universe) {
                fr.fill(0);
                if (1..=512).contains(channel) {
                    fr[*channel as usize - 1] = *level;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::*;

    #[test]
    fn backups_create_restore_delete_and_auto_on_replace() {
        let mut e = engine();
        e.show.name = "Uno".into();
        add(&mut e, "generic/rgb-par", "3ch", 2);
        e.handle(Command::BackupCreate { label: "  base ".into() }).unwrap();
        let l = e.data.backups.list();
        assert_eq!((l.len(), l[0].kind.as_str(), l[0].label.as_str()), (1, "manual", "base"));
        let id = l[0].id.clone();
        // change the show, then restore the manual backup
        e.handle(Command::ShowRename { name: "Dos".into() }).unwrap();
        e.handle(Command::FixtureRemove { ids: vec![1] }).unwrap();
        assert_eq!(e.show.fixtures.len(), 1);
        e.handle(Command::BackupRestore { id: id.clone() }).unwrap();
        assert_eq!((e.show.name.as_str(), e.show.fixtures.len()), ("Uno", 2));
        let l = e.data.backups.list();
        assert_eq!(l.len(), 2, "the replaced state was snapshotted first");
        let auto = l.iter().find(|b| b.kind == "auto").unwrap();
        assert_eq!(auto.label, "Antes de restaurar");
        assert_eq!(e.data.backups.load(&auto.id).unwrap().name, "Dos");
        // messages
        assert!(e.backups_json().contains("\"t\":\"backups\""));
        e.handle(Command::BackupDelete { id }).unwrap();
        assert_eq!(e.data.backups.list().len(), 1);
        assert!(e.handle(Command::BackupDelete { id: "../x".into() }).is_err());
        assert!(e.handle(Command::BackupRestore { id: "nope".into() }).is_err());
        // loading another show also snapshots the current one
        e.handle(Command::ShowNew { name: "Otro".into() }).unwrap();
        assert!(e.data.backups.list().iter().any(|b| b.label == "Antes de cargar otro show"));
    }

    #[test]
    fn untouched_empty_show_is_not_backed_up_on_replace() {
        let mut e = engine();
        e.handle(Command::ShowNew { name: "A".into() }).unwrap();
        assert!(e.data.backups.list().is_empty());
    }

    #[test]
    fn auto_backup_after_interval_only_if_changed() {
        let mut e = engine();
        e.data.last_auto = Instant::now() - AUTO_BACKUP - Duration::from_secs(1);
        e.data_tick();
        assert!(e.data.backups.list().is_empty(), "nothing changed");
        e.handle(Command::ShowRename { name: "Cambiado".into() }).unwrap();
        e.data.last_auto = Instant::now() - AUTO_BACKUP - Duration::from_secs(1);
        e.data_tick();
        let l = e.data.backups.list();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].kind, "auto");
        assert!(!e.data.changed);
        e.data_on_exit();
        assert_eq!(e.data.backups.list().len(), 1, "no change since the last backup");
    }

    #[test]
    fn output_test_overrides_frames_and_expires() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 2);
        set(&mut e, 1, "red", 1.0);
        assert_eq!(frame(&mut e)[0], 255);
        e.handle(Command::OutputTest { mode: TestMode::Level { universe: Some(1), level: 77 } }).unwrap();
        let f = frame(&mut e);
        assert!(f.iter().all(|b| *b == 77));
        // other universes untouched
        e.handle(Command::OutputTest { mode: TestMode::Channel { universe: 1, channel: 5, level: 200 } }).unwrap();
        let f = frame(&mut e);
        assert_eq!((f[4], f.iter().filter(|b| **b != 0).count()), (200, 1));
        assert!(e.handle(Command::OutputTest { mode: TestMode::Channel { universe: 1, channel: 513, level: 1 } }).is_err());
        assert!(e.handle(Command::OutputTest { mode: TestMode::Level { universe: Some(99), level: 1 } }).is_err());
        assert!(e.build_rt().testing);
        // auto-off after 5 minutes
        e.data.test.as_mut().unwrap().started = Instant::now() - TEST_MAX - Duration::from_secs(1);
        e.data_tick();
        assert!(e.data.test.is_none() && !e.build_rt().testing);
        assert_eq!(frame(&mut e)[0], 255, "fixture output is back");
        e.handle(Command::OutputTest { mode: TestMode::Chase { universe: None } }).unwrap();
        e.handle(Command::OutputTest { mode: TestMode::Off }).unwrap();
        assert!(e.data.test.is_none());
    }

    #[test]
    fn test_patterns() {
        let mut fr: BTreeMap<u16, Box<[u8; 512]>> = BTreeMap::new();
        fr.insert(1, Box::new([9; 512]));
        fr.insert(2, Box::new([9; 512]));
        apply_test(&TestMode::Chase { universe: Some(2) }, 150 * 7 + 20, &mut fr);
        assert_eq!(fr[&1][0], 9, "universe 1 untouched");
        assert_eq!((fr[&2][7], fr[&2].iter().filter(|b| **b == 255).count(), fr[&2][0]), (255, 1, 0));
        apply_test(&TestMode::Chase { universe: None }, 150 * 513, &mut fr);
        assert_eq!((fr[&1][1], fr[&2][1], fr[&1][0]), (255, 255, 0), "wraps after 512 steps");
        apply_test(&TestMode::Level { universe: None, level: 3 }, 0, &mut fr);
        assert!(fr.values().all(|f| f.iter().all(|b| *b == 3)));
    }

    #[test]
    fn identify_layers_above_masters_and_strobes() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        add(&mut e, "generic/mh-spot", "16ch", 1);
        e.handle(Command::Blackout { on: true }).unwrap();
        let mover_pan = e.patch.get(2).unwrap().chans.iter().find(|c| c.attr == "pan").unwrap().clone();
        set(&mut e, 2, "pan", 0.0);
        e.handle(Command::Identify { fixture: 1, on: true }).unwrap();
        e.handle(Command::Identify { fixture: 2, on: true }).unwrap();
        assert!(e.handle(Command::Identify { fixture: 99, on: true }).is_err());
        assert_eq!(e.build_rt().identifying, vec![1, 2]);
        e.render(0.0); // strobe phase "on"
        let f = **e.frames.get(&1).unwrap();
        assert_eq!(&f[0..3], &[255, 255, 255], "RGB par full even in blackout");
        let m = e.patch.get(2).unwrap();
        let dim = m.chans.iter().find(|c| c.attr == "dimmer").unwrap().offset as usize + m.address as usize - 1;
        let pan = mover_pan.offset as usize + m.address as usize - 1;
        assert_eq!(f[dim], 255, "mover dimmer full");
        assert_eq!(f[pan], channel_bytes(&mover_pan, mover_pan.default).0, "pan homed (programmer value replaced)");
        e.render(0.3); // phase "off"
        let f = **e.frames.get(&1).unwrap();
        assert_eq!(&f[0..3], &[0, 0, 0]);
        assert_eq!(f[dim], 0);
        e.handle(Command::Identify { fixture: 1, on: false }).unwrap();
        e.handle(Command::Identify { fixture: 2, on: false }).unwrap();
        e.render(0.0);
        assert_eq!(**e.frames.get(&1).unwrap().get(0..3).map(|_| &e.frames[&1]).unwrap().as_ref().get(0..1).map(|_| &e.frames[&1]).unwrap(), *e.frames[&1].as_ref());
        assert_eq!(e.frames[&1][0], 0, "blackout applies again once identify is off");
        assert!(e.build_rt().identifying.is_empty());
    }

    #[test]
    fn start_blackout_applies_at_engine_start() {
        let dir = std::env::temp_dir().join(format!("lumos-sb-{}", std::process::id()));
        let store = Store::new(dir.clone()).unwrap();
        let mut s = Show::default();
        s.settings.start_blackout = true;
        store.autosave(&s).unwrap();
        let (out, _) = broadcast::channel(4);
        let (tx, _rx) = mpsc::unbounded_channel();
        let e = Engine::new(store, out, IoHub::new(tx));
        assert!(e.blackout);
        assert!(e.build_rt().blackout);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn open_value_picks_open_slots() {
        let cap = |min: u8, max: u8, l: &str| Capability { min, max, label: l.into(), color: None, image: None, wheel: None, slot: None };
        let mut c = ChannelDef { attr: "shutter".into(), offset: 0, fine: None, default: 0.0, invert: false, capabilities: vec![cap(0, 3, "Closed"), cap(4, 7, "Open"), cap(8, 255, "Strobe")] };
        assert!((open_value(&c).unwrap() - 5.5 / 255.0).abs() < 1e-6);
        c.capabilities = vec![cap(0, 9, "Closed"), cap(10, 255, "Strobe slow-fast")];
        assert!(open_value(&c).is_none());
        c.capabilities.clear();
        assert!(open_value(&c).is_none());
    }
}
