//! Native MIDI (server side).
//!
//! The desktop app shows the UI in a system webview (WKWebView / WebKitGTK) that has no Web MIDI, so
//! MIDI ports are opened by the server with `midir` (cargo feature `midi`). Without the feature the hub
//! is inert (`native = false`) and the UI falls back to Web MIDI.
//!
//! * Enabled ports are a machine-level setting persisted in `<data>/midi.json` (not in the show).
//! * A single worker thread owns every midir object: it rescans ports every ~2 s (hot-plug), opens
//!   enabled ports as they appear and drops them when they vanish. The engine only talks to it through
//!   channels, so it never blocks.
//! * Input bytes become `IoEvent::Midi` and follow exactly the path of `Command::MidiIn`.
//! * Feedback (`MidiOut`) is written to matching enabled outputs; when none handled it, the engine
//!   still broadcasts it to browsers (Web MIDI relay).

use crate::io::IoEvent;
use crate::model::MidiPortInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc::UnboundedSender;

// ───────────────────────────── pure helpers (always compiled) ─────────────────────────────

/// Machine-level MIDI settings: names of the ports the user enabled.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiConfig {
    #[serde(default)]
    pub inputs: BTreeSet<String>,
    #[serde(default)]
    pub outputs: BTreeSet<String>,
}

impl MidiConfig {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join("midi.json")
    }

    /// Missing or corrupt file → empty config (never an error).
    pub fn load(dir: &Path) -> MidiConfig {
        std::fs::read_to_string(Self::path(dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let tmp = dir.join("midi.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).unwrap_or_default())?;
        std::fs::rename(tmp, Self::path(dir))
    }

    pub fn set(&mut self, output: bool, name: &str, enabled: bool) -> bool {
        let set = if output { &mut self.outputs } else { &mut self.inputs };
        if enabled {
            set.insert(name.to_string())
        } else {
            set.remove(name)
        }
    }

    pub fn enabled(&self, output: bool) -> &BTreeSet<String> {
        if output {
            &self.outputs
        } else {
            &self.inputs
        }
    }
}

/// Ports that should never be offered: kernel "through" ports and our own virtual ports.
pub fn skip_port(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("midi through") || n.starts_with("lumos")
}

/// ALSA appends the client:port ids (`"... 20:0"`), which change between sessions. Strip them so
/// names are stable for persistence and for matching.
pub fn normalize_name(raw: &str) -> String {
    let t = raw.trim();
    if let Some(idx) = t.rfind(' ') {
        let tail = &t[idx + 1..];
        let mut parts = tail.split(':');
        let (a, b, c) = (parts.next(), parts.next(), parts.next());
        if let (Some(a), Some(b), None) = (a, b, c) {
            if !a.is_empty() && !b.is_empty() && a.bytes().all(|x| x.is_ascii_digit()) && b.bytes().all(|x| x.is_ascii_digit()) {
                return t[..idx].trim_end().to_string();
            }
        }
    }
    t.to_string()
}

/// Normalise, drop skipped ports and make duplicates unique (`"X"`, `"X (2)"`…), keeping order.
pub fn clean_port_names(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in raw {
        if skip_port(r) {
            out.push(String::new()); // keep index alignment; filtered by caller
            continue;
        }
        let base = normalize_name(r);
        let mut name = base.clone();
        let mut k = 2;
        while out.contains(&name) {
            name = format!("{base} ({k})");
            k += 1;
        }
        out.push(name);
    }
    out
}

/// What changed between two scans.
pub fn diff_ports(old: &[String], new: &[String]) -> (Vec<String>, Vec<String>) {
    let added = new.iter().filter(|n| !old.contains(n)).cloned().collect();
    let removed = old.iter().filter(|o| !new.contains(o)).cloned().collect();
    (added, removed)
}

/// Port list for the UI: every present port plus enabled-but-currently-missing ones (so they can be
/// switched off), sorted by name.
pub fn build_infos(present: &[String], enabled: &BTreeSet<String>) -> Vec<MidiPortInfo> {
    let mut names: BTreeSet<String> = present.iter().cloned().collect();
    names.extend(enabled.iter().cloned());
    names.into_iter().map(|n| MidiPortInfo { enabled: enabled.contains(&n), name: n }).collect()
}

/// Which connected native outputs a feedback message goes to. `None` = every connected output.
/// A named port matches case-insensitively by equality or containment (Web MIDI and native names differ
/// slightly). Empty result → nobody handled it natively.
pub fn route_feedback(port: Option<&str>, connected: &[String]) -> Vec<String> {
    match port {
        None => connected.to_vec(),
        Some(p) => {
            let p = p.to_ascii_lowercase();
            connected
                .iter()
                .filter(|c| {
                    let c = c.to_ascii_lowercase();
                    c == p || c.contains(&p) || p.contains(&c)
                })
                .cloned()
                .collect()
        }
    }
}

/// Real-time bytes that only flood the engine (clock, active sensing).
pub fn is_noise(bytes: &[u8]) -> bool {
    matches!(bytes.first(), Some(0xF8) | Some(0xFE)) || bytes.is_empty()
}

// ───────────────────────────── hub ─────────────────────────────

/// Snapshot used by the engine for `ServerMsg::MidiPorts`.
#[derive(Clone, Debug, Default)]
pub struct PortsView {
    pub native: bool,
    pub inputs: Vec<MidiPortInfo>,
    pub outputs: Vec<MidiPortInfo>,
}

pub struct MidiHub {
    #[cfg(feature = "midi")]
    inner: Option<imp::Worker>,
    #[cfg(not(feature = "midi"))]
    _unused: (),
}

impl MidiHub {
    /// Inert hub (no native MIDI) — used by tests and by builds without the `midi` feature.
    pub fn disabled() -> Self {
        #[cfg(feature = "midi")]
        {
            MidiHub { inner: None }
        }
        #[cfg(not(feature = "midi"))]
        {
            MidiHub { _unused: () }
        }
    }

    /// Start the native MIDI worker (no-op without the `midi` feature).
    pub fn start(data_dir: PathBuf, tx: UnboundedSender<IoEvent>) -> Self {
        #[cfg(feature = "midi")]
        {
            MidiHub { inner: imp::Worker::start(data_dir, tx) }
        }
        #[cfg(not(feature = "midi"))]
        {
            let _ = (data_dir, tx);
            MidiHub::disabled()
        }
    }

    pub fn native(&self) -> bool {
        #[cfg(feature = "midi")]
        {
            self.inner.is_some()
        }
        #[cfg(not(feature = "midi"))]
        {
            false
        }
    }

    pub fn ports(&self) -> PortsView {
        #[cfg(feature = "midi")]
        if let Some(w) = &self.inner {
            return w.view();
        }
        PortsView::default()
    }

    pub fn set_enabled(&self, output: bool, name: &str, enabled: bool) -> Result<(), String> {
        #[cfg(feature = "midi")]
        if let Some(w) = &self.inner {
            return w.set_enabled(output, name, enabled);
        }
        let _ = (output, name, enabled);
        Err("Native MIDI is not available in this build".into())
    }

    /// Write feedback to matching enabled native outputs. Returns `true` when at least one native output
    /// took it (the caller then skips the browser relay).
    pub fn send(&self, port: Option<&str>, bytes: &[u8]) -> bool {
        #[cfg(feature = "midi")]
        if let Some(w) = &self.inner {
            return w.send(port, bytes);
        }
        let _ = (port, bytes);
        false
    }
}

#[cfg(feature = "midi")]
mod imp {
    use super::*;
    use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
    use std::collections::{HashMap, HashSet};
    use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    enum Cmd {
        Enable { output: bool, name: String, enabled: bool },
        Send { names: Vec<String>, bytes: Vec<u8> },
        Quit,
    }

    #[derive(Default)]
    struct Shared {
        view: PortsView,
        connected_out: Vec<String>,
    }

    pub struct Worker {
        tx: Sender<Cmd>,
        shared: Arc<Mutex<Shared>>,
        join: Option<std::thread::JoinHandle<()>>,
    }

    impl Worker {
        /// `None` when MIDI cannot be initialised on this machine (no ALSA sequencer, no CoreMIDI…).
        pub fn start(dir: PathBuf, ev_tx: UnboundedSender<IoEvent>) -> Option<Worker> {
            let in_scan = MidiInput::new("Lumos scan in").ok()?;
            let out_scan = MidiOutput::new("Lumos scan out").ok()?;
            let shared = Arc::new(Mutex::new(Shared::default()));
            shared.lock().unwrap().view.native = true;
            let (tx, rx) = channel::<Cmd>();
            let sh = shared.clone();
            let join = std::thread::Builder::new()
                .name("lumos-midi".into())
                .spawn(move || {
                    let mut w = Thread {
                        cfg: MidiConfig::load(&dir),
                        dir,
                        in_scan,
                        out_scan,
                        ins: HashMap::new(),
                        outs: HashMap::new(),
                        failed: HashSet::new(),
                        shared: sh,
                        ev_tx,
                        last_sig: String::new(),
                    };
                    w.sync();
                    loop {
                        match rx.recv_timeout(Duration::from_secs(2)) {
                            Ok(Cmd::Enable { output, name, enabled }) => {
                                if w.cfg.set(output, &name, enabled) {
                                    if let Err(e) = w.cfg.save(&w.dir) {
                                        tracing::warn!("could not save midi.json: {e}");
                                    }
                                }
                                w.failed.remove(&name);
                                w.sync();
                            }
                            Ok(Cmd::Send { names, bytes }) => {
                                for n in names {
                                    if let Some(c) = w.outs.get_mut(&n) {
                                        if let Err(e) = c.send(&bytes) {
                                            tracing::debug!("midi send to {n} failed: {e}");
                                        }
                                    }
                                }
                            }
                            Ok(Cmd::Quit) | Err(RecvTimeoutError::Disconnected) => break,
                            Err(RecvTimeoutError::Timeout) => w.sync(),
                        }
                    }
                })
                .ok()?;
            Some(Worker { tx, shared, join: Some(join) })
        }

        pub fn view(&self) -> PortsView {
            self.shared.lock().map(|s| s.view.clone()).unwrap_or_default()
        }

        pub fn set_enabled(&self, output: bool, name: &str, enabled: bool) -> Result<(), String> {
            if name.trim().is_empty() {
                return Err("Port name required".into());
            }
            self.tx
                .send(Cmd::Enable { output, name: name.to_string(), enabled })
                .map_err(|_| "MIDI worker stopped".to_string())
        }

        pub fn send(&self, port: Option<&str>, bytes: &[u8]) -> bool {
            let names = match self.shared.lock() {
                Ok(s) => route_feedback(port, &s.connected_out),
                Err(_) => return false,
            };
            if names.is_empty() {
                return false;
            }
            self.tx.send(Cmd::Send { names, bytes: bytes.to_vec() }).is_ok()
        }
    }

    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = self.tx.send(Cmd::Quit);
            if let Some(j) = self.join.take() {
                let _ = j.join();
            }
        }
    }

    struct Thread {
        cfg: MidiConfig,
        dir: PathBuf,
        in_scan: MidiInput,
        out_scan: MidiOutput,
        ins: HashMap<String, MidiInputConnection<()>>,
        outs: HashMap<String, MidiOutputConnection>,
        /// Names whose last connect attempt failed (log once, retry on the next scans).
        failed: HashSet<String>,
        shared: Arc<Mutex<Shared>>,
        ev_tx: UnboundedSender<IoEvent>,
        last_sig: String,
    }

    impl Thread {
        fn scan_inputs(&self) -> Vec<(String, midir::MidiInputPort)> {
            let ports = self.in_scan.ports();
            let raw: Vec<String> = ports.iter().map(|p| self.in_scan.port_name(p).unwrap_or_default()).collect();
            clean_port_names(&raw).into_iter().zip(ports).filter(|(n, _)| !n.is_empty()).collect()
        }

        fn scan_outputs(&self) -> Vec<(String, midir::MidiOutputPort)> {
            let ports = self.out_scan.ports();
            let raw: Vec<String> = ports.iter().map(|p| self.out_scan.port_name(p).unwrap_or_default()).collect();
            clean_port_names(&raw).into_iter().zip(ports).filter(|(n, _)| !n.is_empty()).collect()
        }

        /// Reconcile open connections with (present ∩ enabled) and publish changes.
        fn sync(&mut self) {
            let ins = self.scan_inputs();
            let outs = self.scan_outputs();

            // inputs
            let want_in: HashSet<&String> = ins.iter().map(|(n, _)| n).filter(|n| self.cfg.inputs.contains(*n)).collect();
            let gone: Vec<String> = self.ins.keys().filter(|k| !want_in.contains(k)).cloned().collect();
            for g in gone {
                self.ins.remove(&g); // dropping closes the connection
            }
            for (name, port) in &ins {
                if !self.cfg.inputs.contains(name) || self.ins.contains_key(name) {
                    continue;
                }
                let mut mi = match MidiInput::new("Lumos in") {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                mi.ignore(Ignore::None);
                let tx = self.ev_tx.clone();
                let pname = name.clone();
                match mi.connect(
                    port,
                    "lumos-in",
                    move |_ts, data, _| {
                        if !is_noise(data) {
                            let _ = tx.send(IoEvent::Midi { port: pname.clone(), bytes: data.to_vec() });
                        }
                    },
                    (),
                ) {
                    Ok(c) => {
                        self.failed.remove(name);
                        self.ins.insert(name.clone(), c);
                    }
                    Err(e) => {
                        if self.failed.insert(name.clone()) {
                            tracing::warn!("cannot open MIDI input {name}: {e}");
                        }
                    }
                }
            }

            // outputs
            let want_out: HashSet<&String> = outs.iter().map(|(n, _)| n).filter(|n| self.cfg.outputs.contains(*n)).collect();
            let gone: Vec<String> = self.outs.keys().filter(|k| !want_out.contains(k)).cloned().collect();
            for g in gone {
                if let Some(c) = self.outs.remove(&g) {
                    let _ = c.close();
                }
            }
            for (name, port) in &outs {
                if !self.cfg.outputs.contains(name) || self.outs.contains_key(name) {
                    continue;
                }
                let mo = match MidiOutput::new("Lumos out") {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                match mo.connect(port, "lumos-out") {
                    Ok(c) => {
                        self.failed.remove(name);
                        self.outs.insert(name.clone(), c);
                    }
                    Err(e) => {
                        if self.failed.insert(name.clone()) {
                            tracing::warn!("cannot open MIDI output {name}: {e}");
                        }
                    }
                }
            }

            let in_names: Vec<String> = ins.iter().map(|(n, _)| n.clone()).collect();
            let out_names: Vec<String> = outs.iter().map(|(n, _)| n.clone()).collect();
            let view = PortsView {
                native: true,
                inputs: build_infos(&in_names, &self.cfg.inputs),
                outputs: build_infos(&out_names, &self.cfg.outputs),
            };
            let mut connected: Vec<String> = self.outs.keys().cloned().collect();
            connected.sort();
            let sig = format!("{:?}|{:?}|{:?}", view.inputs, view.outputs, connected);
            if let Ok(mut s) = self.shared.lock() {
                s.view = view.clone();
                s.connected_out = connected;
            }
            if sig != self.last_sig {
                self.last_sig = sig;
                let _ = self.ev_tx.send(IoEvent::MidiPorts { inputs: view.inputs, outputs: view.outputs });
            }
        }
    }
}

// ───────────────────────────── tests ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn normalize_strips_alsa_ids_only() {
        assert_eq!(normalize_name("APC40 mkII:APC40 mkII MIDI 1 20:0"), "APC40 mkII:APC40 mkII MIDI 1");
        assert_eq!(normalize_name("nanoKONTROL2 24:1"), "nanoKONTROL2");
        assert_eq!(normalize_name("Launchpad Mini"), "Launchpad Mini");
        assert_eq!(normalize_name("IAC Driver Bus 1"), "IAC Driver Bus 1");
        assert_eq!(normalize_name("Port 12:ab"), "Port 12:ab");
        assert_eq!(normalize_name("  X  "), "X");
    }

    #[test]
    fn skips_through_and_own_ports() {
        assert!(skip_port("Midi Through:Midi Through Port-0 14:0"));
        assert!(skip_port("Lumos in"));
        assert!(!skip_port("APC40 mkII"));
    }

    #[test]
    fn clean_names_dedupes_and_filters() {
        let raw = s(&["Midi Through:Midi Through Port-0 14:0", "Pad 20:0", "Pad 21:0", "Lumos out 130:0"]);
        let c = clean_port_names(&raw);
        assert_eq!(c[0], "");
        assert_eq!(c[1], "Pad");
        assert_eq!(c[2], "Pad (2)");
        assert_eq!(c[3], "");
    }

    #[test]
    fn diff_detects_hotplug() {
        let (a, r) = diff_ports(&s(&["A", "B"]), &s(&["B", "C"]));
        assert_eq!(a, s(&["C"]));
        assert_eq!(r, s(&["A"]));
        let (a, r) = diff_ports(&s(&["A"]), &s(&["A"]));
        assert!(a.is_empty() && r.is_empty());
    }

    #[test]
    fn infos_include_enabled_but_missing_ports() {
        let en: BTreeSet<String> = ["B".to_string(), "Gone".to_string()].into_iter().collect();
        let v = build_infos(&s(&["B", "A"]), &en);
        let names: Vec<_> = v.iter().map(|p| (p.name.as_str(), p.enabled)).collect();
        assert_eq!(names, vec![("A", false), ("B", true), ("Gone", true)]);
    }

    #[test]
    fn feedback_routing() {
        let c = s(&["APC40 mkII:APC40 mkII MIDI 1", "nanoKONTROL2"]);
        assert_eq!(route_feedback(None, &c), c);
        assert_eq!(route_feedback(Some("nanokontrol2"), &c), s(&["nanoKONTROL2"]));
        assert_eq!(route_feedback(Some("APC40 mkII"), &c), s(&["APC40 mkII:APC40 mkII MIDI 1"]));
        assert!(route_feedback(Some("Launchpad"), &c).is_empty());
        assert!(route_feedback(None, &[]).is_empty());
    }

    #[test]
    fn noise_filter() {
        assert!(is_noise(&[0xF8]));
        assert!(is_noise(&[0xFE]));
        assert!(is_noise(&[]));
        assert!(!is_noise(&[0xF1, 0x20]));
        assert!(!is_noise(&[0x90, 36, 100]));
        assert!(!is_noise(&[0xF0, 0x7F, 0xF7]));
    }

    #[test]
    fn config_persists_and_survives_corruption() {
        let dir = std::env::temp_dir().join(format!("lumos-midi-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(MidiConfig::load(&dir), MidiConfig::default());
        let mut c = MidiConfig::default();
        assert!(c.set(false, "APC", true));
        assert!(!c.set(false, "APC", true)); // already enabled → no change
        assert!(c.set(true, "APC", true));
        c.save(&dir).unwrap();
        assert_eq!(MidiConfig::load(&dir), c);
        assert!(c.set(false, "APC", false));
        c.save(&dir).unwrap();
        assert!(MidiConfig::load(&dir).inputs.is_empty());
        std::fs::write(MidiConfig::path(&dir), "{not json").unwrap();
        assert_eq!(MidiConfig::load(&dir), MidiConfig::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disabled_hub_is_inert() {
        let h = MidiHub::disabled();
        assert!(!h.native());
        assert!(!h.send(None, &[0x90, 1, 1]));
        assert!(h.set_enabled(false, "x", true).is_err());
        assert!(!h.ports().native);
    }
}
