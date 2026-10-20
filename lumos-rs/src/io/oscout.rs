//! OSC output: DMX as per-channel float messages and/or a 512-byte blob per universe.
//!
//! `only_nonzero = true`  → per-channel messages only for channels that changed since the last
//!   frame (first frame: only non-zero channels), so a channel returning to 0 is sent once.
//! `only_nonzero = false` → all 512 channels whenever the frame changed (and once per second).
//! Blob mode sends `/dmx/<universe>` on change and once per second.

use std::io::ErrorKind;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use rosc::{encoder, OscMessage, OscPacket, OscType};

use super::{Sink, Stat};
use crate::model::{OscMode, OutState};

pub struct OscSink {
    sock: UdpSocket,
    target: SocketAddr,
    pattern: String,
    mode: OscMode,
    only_nonzero: bool,
    last: Option<[u8; 512]>,
    last_full: Instant,
    stat: Stat,
    label: String,
}

impl OscSink {
    pub fn new(host: &str, port: u16, pattern: &str, mode: OscMode, only_nonzero: bool) -> Result<Self, String> {
        let host = host.trim();
        let host = if host.is_empty() { "127.0.0.1" } else { host };
        let target = (host, port)
            .to_socket_addrs()
            .map_err(|e| format!("{host}: {e}"))?
            .find(|a| a.is_ipv4())
            .ok_or_else(|| format!("{host}: no IPv4 address"))?;
        let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        sock.set_nonblocking(true).map_err(|e| e.to_string())?;
        let label = format!("→ {target}");
        Ok(OscSink {
            sock,
            target,
            pattern: if pattern.is_empty() { "/dmx/{universe}/{channel}".into() } else { pattern.into() },
            mode,
            only_nonzero,
            last: None,
            last_full: Instant::now() - Duration::from_secs(10),
            stat: Stat::new(OutState::Ok, label.clone()),
            label,
        })
    }

    fn emit(&mut self, addr: String, arg: OscType) {
        let pkt = OscPacket::Message(OscMessage { addr, args: vec![arg] });
        let Ok(buf) = encoder::encode(&pkt) else { return };
        match self.sock.send_to(&buf, self.target) {
            Ok(_) => {
                if self.stat.state != OutState::Ok {
                    self.stat.state = OutState::Ok;
                    self.stat.message = self.label.clone();
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => {
                self.stat.state = OutState::Error;
                self.stat.message = e.to_string();
            }
        }
    }
}

impl Sink for OscSink {
    fn send(&mut self, universe: u16, data: &[u8; 512]) {
        let now = Instant::now();
        let changed = self.last.as_ref() != Some(data);
        let refresh = now.duration_since(self.last_full) >= Duration::from_secs(1);
        if !changed && !refresh {
            return;
        }
        if matches!(self.mode, OscMode::PerChannel | OscMode::Both) {
            let u = universe.to_string();
            for ch in 0..512usize {
                let v = data[ch];
                let prev = self.last.as_ref().map(|l| l[ch]);
                let send = if self.only_nonzero {
                    match prev {
                        Some(p) => p != v,
                        None => v != 0,
                    }
                } else {
                    true
                };
                if send {
                    let addr = self.pattern.replace("{universe}", &u).replace("{channel}", &(ch + 1).to_string());
                    self.emit(addr, OscType::Float(v as f32 / 255.0));
                }
            }
        }
        if matches!(self.mode, OscMode::Blob | OscMode::Both) {
            self.emit(format!("/dmx/{universe}"), OscType::Blob(data.to_vec()));
        }
        if refresh {
            self.last_full = now;
        }
        self.last = Some(*data);
        self.stat.frames = self.stat.frames.wrapping_add(1);
    }

    fn stat(&self) -> Stat {
        self.stat.clone()
    }
}
