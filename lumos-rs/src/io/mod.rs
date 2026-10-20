//! I/O hub: DMX outputs (Art-Net, sACN, OSC, serial/USB) and external inputs (OSC, Art-Net).
//!
//! OWNER: io agent. The public surface below is the contract with the engine —
//! keep these signatures stable (extend, don't change).

use crate::model::*;
use tokio::sync::mpsc::UnboundedSender;

/// Events produced by external inputs, consumed by the engine task.
#[derive(Debug, Clone)]
pub enum IoEvent {
    /// Incoming OSC message. `value` = first numeric argument (int/float/bool→0|1), if any.
    Osc { addr: String, value: Option<f32> },
    /// Incoming Art-Net DMX already translated through `ArtNetInCfg.map`
    /// to a Lumos universe id; engine merges it HTP.
    ExternalDmx { universe: u16, data: Vec<u8> },
}

pub struct IoHub {
    _tx: UnboundedSender<IoEvent>,
}

impl IoHub {
    /// Must be called from within a tokio runtime (input listeners are spawned tasks).
    pub fn new(tx: UnboundedSender<IoEvent>) -> Self {
        IoHub { _tx: tx }
    }

    /// Idempotently (re)apply configuration: open/close outputs, (re)bind input sockets.
    /// Only touch outputs/inputs whose config actually changed.
    pub fn configure(&mut self, _universes: &[UniverseCfg], _settings: &Settings) {}

    /// Non-blocking: queue one universe frame (512 bytes) to all its outputs.
    /// Called once per engine tick per universe.
    pub fn send(&mut self, _universe: u16, _data: &[u8; 512]) {}

    pub fn status(&self) -> (Vec<OutputStatus>, InputStatus) {
        (
            vec![],
            InputStatus {
                osc_listening: false,
                osc_port: 0,
                osc_error: String::new(),
                artnet_listening: false,
                artnet_error: String::new(),
                recent: vec![],
            },
        )
    }

    /// Send one OSC message to the configured feedback target (no-op when disabled).
    pub fn osc_feedback(&self, _addr: &str, _value: f32) {}
}

/// MIDI Time Code quarter-frame decoder. Feed raw MIDI bytes; returns the
/// assembled absolute frame count (at the given fps) whenever a full timecode is known.
#[derive(Default)]
pub struct MtcDecoder;

impl MtcDecoder {
    pub fn feed(&mut self, _bytes: &[u8], _fps: f32) -> Option<u32> {
        None
    }
}

/// Available serial ports (for the settings UI): `(port name, description)`.
pub fn list_serial_ports() -> Vec<(String, String)> {
    vec![]
}
