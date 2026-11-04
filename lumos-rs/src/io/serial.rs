//! Serial DMX interfaces: Enttec Open DMX (raw, software break) and Enttec DMX USB Pro
//! (widget protocol, also DMXking ultraDMX). One worker thread per port, always sending the
//! latest frame; auto-reconnect with exponential backoff.

use std::io::Write;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serialport::{DataBits, Parity, SerialPort, SerialPortType, StopBits};

use super::worker::{Slot, SharedStat, MIN_FRAME_GAP};
use super::{Sink, Stat};
use crate::model::{OutState, SerialDriver};

pub fn list_ports() -> Vec<(String, String)> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let desc = match p.port_type {
                SerialPortType::UsbPort(u) => {
                    let name = [u.manufacturer, u.product]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ");
                    let name = if name.is_empty() { "USB serial".to_string() } else { name };
                    format!("{name} ({:04x}:{:04x})", u.vid, u.pid)
                }
                SerialPortType::PciPort => "PCI serial".into(),
                SerialPortType::BluetoothPort => "Bluetooth".into(),
                SerialPortType::Unknown => "serial".into(),
            };
            (p.port_name, desc)
        })
        .collect()
}

/// Enttec DMX USB Pro "Send DMX" widget message (label 6).
pub fn build_enttec_pro(data: &[u8; 512]) -> [u8; 518] {
    let mut p = [0u8; 518];
    p[0] = 0x7E;
    p[1] = 6;
    p[2] = (513 & 0xFF) as u8;
    p[3] = (513 >> 8) as u8;
    p[4] = 0; // start code
    p[5..517].copy_from_slice(data);
    p[517] = 0xE7;
    p
}

pub struct SerialSink {
    slot: Arc<Slot>,
    stat: SharedStat,
    frames: Arc<AtomicU32>,
}

impl SerialSink {
    pub fn new(driver: SerialDriver, port: &str) -> Self {
        let slot = Slot::new();
        let stat: SharedStat = Arc::new(Mutex::new(Stat::new(OutState::Off, "connecting…")));
        let frames = Arc::new(AtomicU32::new(0));
        let port = port.trim().to_string();
        if port.is_empty() {
            *stat.lock() = Stat::new(OutState::Error, "no serial port selected");
            return SerialSink { slot, stat, frames };
        }
        let (s2, st2, f2) = (slot.clone(), stat.clone(), frames.clone());
        if let Err(e) = std::thread::Builder::new()
            .name(format!("dmx-serial-{port}"))
            .spawn(move || run(driver, port, s2, st2, f2))
        {
            *stat.lock() = Stat::new(OutState::Error, e.to_string());
        }
        SerialSink { slot, stat, frames }
    }
}

impl Drop for SerialSink {
    fn drop(&mut self) {
        self.slot.close();
    }
}

impl Sink for SerialSink {
    fn send(&mut self, _universe: u16, data: &[u8; 512]) {
        self.slot.put(data);
    }

    fn stat(&self) -> Stat {
        let mut s = self.stat.lock().clone();
        s.frames = self.frames.load(Ordering::Relaxed);
        s
    }
}

fn open(driver: SerialDriver, path: &str) -> Result<Box<dyn SerialPort>, String> {
    let b = match driver {
        SerialDriver::EnttecOpen => serialport::new(path, 250_000)
            .data_bits(DataBits::Eight)
            .stop_bits(StopBits::Two)
            .parity(Parity::None),
        SerialDriver::EnttecPro => serialport::new(path, 57_600)
            .data_bits(DataBits::Eight)
            .stop_bits(StopBits::One)
            .parity(Parity::None),
    };
    b.timeout(Duration::from_millis(500)).open().map_err(|e| e.description)
}

fn run(driver: SerialDriver, path: String, slot: Arc<Slot>, stat: SharedStat, frames: Arc<AtomicU32>) {
    let mut backoff = Duration::from_millis(500);
    while !slot.stopped() {
        let mut port = match open(driver, &path) {
            Ok(p) => {
                backoff = Duration::from_millis(500);
                *stat.lock() = Stat::new(OutState::Ok, format!("open {path}"));
                p
            }
            Err(e) => {
                *stat.lock() = Stat::new(OutState::Error, e);
                if !slot.sleep(backoff) {
                    return;
                }
                backoff = (backoff * 2).min(Duration::from_secs(5));
                continue;
            }
        };
        let mut last_send = Instant::now() - MIN_FRAME_GAP;
        loop {
            if slot.stopped() {
                return;
            }
            // Wake on a new frame, otherwise refresh the latest one so receivers never time out.
            slot.wait(MIN_FRAME_GAP);
            let Some(frame) = slot.latest() else { continue };
            let since = last_send.elapsed();
            if since < MIN_FRAME_GAP {
                std::thread::sleep(MIN_FRAME_GAP - since);
            }
            last_send = Instant::now();
            let res = match driver {
                SerialDriver::EnttecOpen => write_open(port.as_mut(), &frame),
                SerialDriver::EnttecPro => port.write_all(&build_enttec_pro(&frame)).map_err(|e| e.to_string()),
            };
            match res {
                Ok(()) => {
                    frames.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    *stat.lock() = Stat::new(OutState::Error, e);
                    break; // reopen
                }
            }
        }
    }
}

fn write_open(port: &mut dyn SerialPort, frame: &[u8; 512]) -> Result<(), String> {
    port.set_break().map_err(|e| e.description)?;
    std::thread::sleep(Duration::from_micros(120)); // break ≥ 88 µs
    port.clear_break().map_err(|e| e.description)?;
    std::thread::sleep(Duration::from_micros(16)); // mark-after-break ≥ 8 µs
    let mut buf = [0u8; 513];
    buf[1..].copy_from_slice(frame);
    port.write_all(&buf).map_err(|e| e.to_string())
}
