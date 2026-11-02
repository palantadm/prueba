//! PosiStageNet input: joins the multicast group and forwards raw datagrams to the engine ([`IoEvent::Psn`]).
//! A group that cannot be joined (no multicast on that card) is reported but unicast datagrams still arrive.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;

use super::IoEvent;

pub struct PsnListener {
    stop: Arc<AtomicBool>,
    /// Non-fatal note (multicast group not joined).
    pub warning: String,
}

impl Drop for PsnListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl PsnListener {
    pub fn start(port: u16, group: &str, iface: &str, tx: UnboundedSender<IoEvent>) -> Result<Self, String> {
        use socket2::{Domain, Protocol, Socket, Type};
        let group: Ipv4Addr = group.trim().parse().map_err(|_| format!("PSN: «{group}» no es una dirección IPv4 multicast"))?;
        if !group.is_multicast() {
            return Err(format!("PSN: {group} no es una dirección multicast (224.0.0.0–239.255.255.255)"));
        }
        let iface_ip: Ipv4Addr = if iface.trim().is_empty() { Ipv4Addr::UNSPECIFIED } else { iface.trim().parse().map_err(|_| format!("PSN: «{iface}» no es una IP de tarjeta de red válida"))? };
        let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| e.to_string())?;
        s.set_reuse_address(true).map_err(|e| e.to_string())?;
        #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
        let _ = s.set_reuse_port(true);
        s.bind(&SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port).into()).map_err(|e| format!("PSN :{port}: {e}"))?;
        let warning = match s.join_multicast_v4(&group, &iface_ip) {
            Ok(()) => String::new(),
            Err(e) => format!("No se pudo unir al grupo {group} ({e}); solo llegan datos enviados directamente a este equipo"),
        };
        let sock: UdpSocket = s.into();
        sock.set_read_timeout(Some(Duration::from_millis(200))).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        std::thread::Builder::new()
            .name("psn-in".into())
            .spawn(move || {
                let mut buf = vec![0u8; 65536];
                while !stop2.load(Ordering::Relaxed) {
                    match sock.recv_from(&mut buf) {
                        Ok((n, _)) => {
                            let _ = tx.send(IoEvent::Psn { data: buf[..n].to_vec() });
                        }
                        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                        Err(_) => std::thread::sleep(Duration::from_millis(50)),
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(PsnListener { stop, warning })
    }
}
