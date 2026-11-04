//! OSC input server (UDP). Runs on its own thread; pushes [`IoEvent::Osc`] to the engine.

use std::io::ErrorKind;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rosc::{OscMessage, OscPacket, OscType};
use tokio::sync::mpsc::UnboundedSender;

use super::{push_recent, IoEvent, Recent};

pub struct OscServer {
    stop: Arc<AtomicBool>,
}

impl Drop for OscServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// First numeric argument as f32 (int → f32, bool → 0|1).
pub fn first_value(args: &[OscType]) -> Option<f32> {
    args.iter().find_map(|a| match a {
        OscType::Float(f) => Some(*f),
        OscType::Double(d) => Some(*d as f32),
        OscType::Int(i) => Some(*i as f32),
        OscType::Long(l) => Some(*l as f32),
        OscType::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    })
}

fn handle(packet: OscPacket, tx: &UnboundedSender<IoEvent>, recent: &Recent) {
    match packet {
        OscPacket::Message(OscMessage { addr, args }) => {
            let value = first_value(&args);
            let line = match value {
                Some(v) => format!("OSC {addr} {v:.3}"),
                None => format!("OSC {addr}"),
            };
            push_recent(recent, line);
            let _ = tx.send(IoEvent::Osc { addr, value });
        }
        OscPacket::Bundle(b) => {
            for p in b.content {
                handle(p, tx, recent);
            }
        }
    }
}

impl OscServer {
    /// Binds synchronously (so bind errors are reported immediately) and spawns the receive thread.
    pub fn start(port: u16, tx: UnboundedSender<IoEvent>, recent: Recent) -> Result<Self, String> {
        let sock = UdpSocket::bind(("0.0.0.0", port)).map_err(|e| format!("OSC :{port}: {e}"))?;
        sock.set_read_timeout(Some(Duration::from_millis(200))).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        std::thread::Builder::new()
            .name("osc-in".into())
            .spawn(move || {
                let mut buf = vec![0u8; 65536];
                while !stop2.load(Ordering::Relaxed) {
                    match sock.recv_from(&mut buf) {
                        Ok((n, _)) => {
                            if let Ok((_, pkt)) = rosc::decoder::decode_udp(&buf[..n]) {
                                handle(pkt, &tx, &recent);
                            }
                        }
                        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                        Err(_) => std::thread::sleep(Duration::from_millis(50)),
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(OscServer { stop })
    }
}
