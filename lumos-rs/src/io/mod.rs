//! I/O hub: DMX outputs (Art-Net, sACN, OSC, serial/USB) and external inputs (OSC, Art-Net).
//!
//! OWNER: io agent. The public surface below is the contract with the engine —
//! keep these signatures stable (extend, don't change).
//!
//! Design notes
//! * `IoHub::send` never blocks: UDP sinks use non-blocking sockets, serial/USB sinks hand the
//!   frame to a dedicated worker thread through a "latest frame" slot.
//! * `IoHub::configure` diffs by serialised config; unchanged outputs/inputs keep their sockets.
//! * Input listeners run on their own OS threads (no tokio runtime needed) and push
//!   [`IoEvent`]s into the engine's unbounded channel.
#![allow(dead_code)]

mod artnet;
mod inputs;
mod mtc;
mod oscout;
mod sacn;
mod serial;
mod usb;
mod worker;

use std::collections::{BTreeMap, VecDeque};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc::UnboundedSender;

use crate::model::*;

#[allow(unused_imports)] // consumed by the engine
pub use mtc::MtcDecoder;

/// Events produced by external inputs, consumed by the engine task.
#[derive(Debug, Clone)]
pub enum IoEvent {
    /// Incoming OSC message. `value` = first numeric argument (int/float/bool→0|1), if any.
    Osc { addr: String, value: Option<f32> },
    /// Incoming Art-Net DMX already translated through `ArtNetInCfg.map`
    /// to a Lumos universe id; engine merges it HTP.
    ExternalDmx { universe: u16, data: Vec<u8> },
    /// Raw MIDI bytes from an enabled native input port (same path as `Command::MidiIn`).
    Midi { port: String, bytes: Vec<u8> },
    /// Native MIDI port lists / enabled state changed.
    MidiPorts { inputs: Vec<MidiPortInfo>, outputs: Vec<MidiPortInfo> },
}

/// Snapshot of one output's health.
#[derive(Clone, Debug)]
pub(crate) struct Stat {
    pub state: OutState,
    pub message: String,
    pub frames: u32,
}

impl Stat {
    pub fn new(state: OutState, message: impl Into<String>) -> Self {
        Stat { state, message: message.into(), frames: 0 }
    }
}

/// One concrete output device/socket for a single (universe, index) slot.
pub(crate) trait Sink: Send {
    /// Non-blocking. Called once per engine tick.
    fn send(&mut self, universe: u16, data: &[u8; 512]);
    fn stat(&self) -> Stat;
}

/// Shared "recent input events" log (newest first, capped).
pub(crate) type Recent = Arc<Mutex<VecDeque<String>>>;

pub(crate) fn push_recent(r: &Recent, line: String) {
    let mut q = r.lock();
    q.push_front(line);
    q.truncate(20);
}

struct OutEntry {
    key: String,
    sink: Option<Box<dyn Sink>>,
    /// Status for outputs without a sink (disabled).
    fixed: Stat,
}

/// Counters used by tests to prove that `configure` does not rebind unchanged resources.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Counters {
    pub osc_binds: u32,
    pub artnet_binds: u32,
    pub outputs_opened: u32,
}

pub struct IoHub {
    tx: UnboundedSender<IoEvent>,
    outputs: BTreeMap<(u16, u8), OutEntry>,
    node_name: String,
    recent: Recent,

    osc_in: Option<inputs::OscServer>,
    osc_in_key: Option<u16>,
    osc_error: String,

    artnet_shared: Arc<artnet::ArtShared>,
    artnet_listener: Option<artnet::ArtNetListener>,
    artnet_key: Option<u16>,
    artnet_error: String,
    pub(crate) artnet_port: u16,

    feedback: Option<(UdpSocket, SocketAddr)>,
    feedback_key: String,

    /// Shared non-blocking socket + resolved-address cache for ad-hoc OSC sends (virtual-console feedback).
    osc_tx: Option<UdpSocket>,
    osc_resolved: Mutex<std::collections::HashMap<(String, u16), Option<SocketAddr>>>,

    pub(crate) counters: Counters,
}

fn cfg_key(o: &OutputCfg) -> String {
    serde_json::to_string(o).unwrap_or_default()
}

impl IoHub {
    pub fn new(tx: UnboundedSender<IoEvent>) -> Self {
        IoHub {
            tx,
            outputs: BTreeMap::new(),
            node_name: String::new(),
            recent: Arc::new(Mutex::new(VecDeque::new())),
            osc_in: None,
            osc_in_key: None,
            osc_error: String::new(),
            artnet_shared: Arc::new(artnet::ArtShared::default()),
            artnet_listener: None,
            artnet_key: None,
            artnet_error: String::new(),
            artnet_port: artnet::ARTNET_PORT,
            feedback: None,
            feedback_key: String::new(),
            osc_tx: UdpSocket::bind("0.0.0.0:0").ok().and_then(|s| s.set_nonblocking(true).ok().map(|_| s)),
            osc_resolved: Mutex::new(std::collections::HashMap::new()),
            counters: Counters::default(),
        }
    }

    /// Idempotently (re)apply configuration: open/close outputs, (re)bind input sockets.
    /// Only touch outputs/inputs whose config actually changed.
    pub fn configure(&mut self, universes: &[UniverseCfg], settings: &Settings) {
        let name_changed = self.node_name != settings.node_name;
        self.node_name = settings.node_name.clone();
        *self.artnet_shared.node_name.lock() = settings.node_name.clone();

        // ── outputs ──
        let mut wanted: BTreeMap<(u16, u8), &OutputCfg> = BTreeMap::new();
        for u in universes {
            for (i, o) in u.outputs.iter().enumerate().take(255) {
                wanted.insert((u.id, i as u8), o);
            }
        }
        self.outputs.retain(|k, _| wanted.contains_key(k));
        let mut art_ports: Vec<u16> = Vec::new();
        for (&(uid, idx), cfg) in &wanted {
            // sACN embeds the node name in every packet.
            let mut key = cfg_key(cfg);
            if matches!(cfg.kind, OutputKind::Sacn { .. }) {
                key.push_str(&self.node_name);
            }
            if let OutputKind::ArtNet { net, subnet, universe, .. } = &cfg.kind {
                if cfg.enabled {
                    art_ports.push(artnet::port_address(*net, *subnet, *universe));
                }
            }
            if self.outputs.get(&(uid, idx)).map(|e| e.key == key).unwrap_or(false) {
                continue;
            }
            let entry = if !cfg.enabled {
                OutEntry { key, sink: None, fixed: Stat::new(OutState::Off, "disabled") }
            } else {
                self.counters.outputs_opened += 1;
                match open_sink(&cfg.kind, &self.node_name) {
                    Ok(s) => OutEntry { key, sink: Some(s), fixed: Stat::new(OutState::Off, "") },
                    Err(e) => OutEntry { key, sink: None, fixed: Stat::new(OutState::Error, e) },
                }
            };
            self.outputs.insert((uid, idx), entry);
        }
        let _ = name_changed;
        *self.artnet_shared.out_ports.lock() = art_ports.clone();

        // ── OSC input ──
        let want_osc = settings.osc_in.enabled.then_some(settings.osc_in.port);
        if want_osc != self.osc_in_key || (want_osc.is_some() && self.osc_in.is_none()) {
            self.osc_in = None;
            self.osc_error.clear();
            self.osc_in_key = want_osc;
            if let Some(port) = want_osc {
                self.counters.osc_binds += 1;
                match inputs::OscServer::start(port, self.tx.clone(), self.recent.clone()) {
                    Ok(s) => self.osc_in = Some(s),
                    Err(e) => self.osc_error = e,
                }
            }
        }

        // ── Art-Net listener (poll replies + input) ──
        self.artnet_shared
            .in_enabled
            .store(settings.artnet_in.enabled, std::sync::atomic::Ordering::Relaxed);
        *self.artnet_shared.map.lock() = settings.artnet_in.map.clone();
        let want_art = (settings.artnet_in.enabled || !art_ports.is_empty()).then_some(self.artnet_port);
        if want_art != self.artnet_key || (want_art.is_some() && self.artnet_listener.is_none()) {
            self.artnet_listener = None;
            self.artnet_error.clear();
            self.artnet_key = want_art;
            if let Some(port) = want_art {
                self.counters.artnet_binds += 1;
                match artnet::ArtNetListener::start(
                    port,
                    self.artnet_shared.clone(),
                    self.tx.clone(),
                    self.recent.clone(),
                ) {
                    Ok(l) => self.artnet_listener = Some(l),
                    Err(e) => self.artnet_error = e,
                }
            }
        }

        // ── OSC feedback ──
        let fb = &settings.osc_feedback;
        let fb_key = if fb.enabled { format!("{}:{}", fb.host, fb.port) } else { String::new() };
        if fb_key != self.feedback_key {
            self.feedback_key = fb_key;
            self.feedback = None;
            if fb.enabled {
                self.feedback = (fb.host.as_str(), fb.port)
                    .to_socket_addrs()
                    .ok()
                    .and_then(|mut a| a.find(|s| s.is_ipv4()))
                    .and_then(|addr| {
                        let s = UdpSocket::bind("0.0.0.0:0").ok()?;
                        s.set_nonblocking(true).ok()?;
                        Some((s, addr))
                    });
            }
        }
    }

    /// Non-blocking: queue one universe frame (512 bytes) to all its outputs.
    /// Called once per engine tick per universe.
    pub fn send(&mut self, universe: u16, data: &[u8; 512]) {
        for (_, e) in self.outputs.range_mut((universe, 0)..=(universe, u8::MAX)) {
            if let Some(s) = e.sink.as_mut() {
                s.send(universe, data);
            }
        }
    }

    pub fn status(&self) -> (Vec<OutputStatus>, InputStatus) {
        let outs = self
            .outputs
            .iter()
            .map(|(&(universe, index), e)| {
                let st = e.sink.as_ref().map(|s| s.stat()).unwrap_or_else(|| e.fixed.clone());
                OutputStatus { universe, index, state: st.state, message: st.message, frames: st.frames }
            })
            .collect();
        let inputs = InputStatus {
            osc_listening: self.osc_in.is_some(),
            osc_port: self.osc_in_key.unwrap_or(0),
            osc_error: self.osc_error.clone(),
            artnet_listening: self.artnet_listener.is_some() && self.artnet_shared.in_enabled_now(),
            artnet_error: self.artnet_error.clone(),
            recent: self.recent.lock().iter().cloned().collect(),
        };
        (outs, inputs)
    }

    /// Send one float OSC message to an arbitrary target (non-blocking; address resolved once and cached).
    /// Returns whether the datagram was handed to the socket.
    pub fn osc_send(&self, host: &str, port: u16, addr: &str, value: f32) -> bool {
        let Some(sock) = &self.osc_tx else { return false };
        let target = {
            let mut cache = self.osc_resolved.lock();
            *cache.entry((host.to_string(), port)).or_insert_with(|| {
                (host, port).to_socket_addrs().ok().and_then(|mut a| a.find(|s| s.is_ipv4()))
            })
        };
        let Some(target) = target else { return false };
        let pkt = rosc::OscPacket::Message(rosc::OscMessage {
            addr: addr.to_string(),
            args: vec![rosc::OscType::Float(value)],
        });
        match rosc::encoder::encode(&pkt) {
            Ok(buf) => sock.send_to(&buf, target).is_ok(),
            Err(_) => false,
        }
    }

    /// Send one OSC message to the configured feedback target (no-op when disabled).
    pub fn osc_feedback(&self, addr: &str, value: f32) {
        if let Some((sock, target)) = &self.feedback {
            let pkt = rosc::OscPacket::Message(rosc::OscMessage {
                addr: addr.to_string(),
                args: vec![rosc::OscType::Float(value)],
            });
            if let Ok(buf) = rosc::encoder::encode(&pkt) {
                let _ = sock.send_to(&buf, target);
            }
        }
    }
}

fn open_sink(kind: &OutputKind, node_name: &str) -> Result<Box<dyn Sink>, String> {
    Ok(match kind {
        OutputKind::ArtNet { host, port, net, subnet, universe } => {
            Box::new(artnet::ArtNetSink::new(host, *port, *net, *subnet, *universe)?)
        }
        OutputKind::Sacn { universe, priority, unicast } => Box::new(sacn::SacnSink::new(
            *universe,
            *priority,
            unicast.as_deref(),
            node_name,
            sacn::SACN_PORT,
        )?),
        OutputKind::Osc { host, port, pattern, mode, only_nonzero } => {
            Box::new(oscout::OscSink::new(host, *port, pattern, *mode, *only_nonzero)?)
        }
        OutputKind::Serial { driver, port } => Box::new(serial::SerialSink::new(*driver, port)),
        OutputKind::Usb { driver, serial } => usb::new_usb(driver, serial.as_deref()),
    })
}

/// Available serial ports (for the settings UI): `(port name, description)`.
pub fn list_serial_ports() -> Vec<(String, String)> {
    serial::list_ports()
}

#[cfg(test)]
mod tests;
