//! RDM inside the engine: sockets, the scan sequence, and the glue between commands, the network and [`crate::rdm::Controller`].

use super::*;
use crate::io::port_address;
use crate::rdm::{self, Controller, Uid};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct RdmHub {
    pub ctl: Controller,
    sock: Option<UdpSocket>,
    stop: Arc<AtomicBool>,
    scan_at: Option<(Instant, bool)>,
    pub art_port: u16,
}

impl Default for RdmHub {
    fn default() -> Self {
        RdmHub { ctl: Controller::new(crate::io::ARTNET_PORT), sock: None, stop: Arc::new(AtomicBool::new(false)), scan_at: None, art_port: crate::io::ARTNET_PORT }
    }
}

impl Drop for RdmHub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn parse_uid(s: &str) -> Result<Uid, String> {
    Uid::parse(s).ok_or_else(|| format!("UID RDM no válido «{s}» (formato 7FF0:12345678)"))
}

impl Engine {
    /// Open the RDM socket (and a listener on the Art-Net port for broadcast replies) the first time it is needed.
    fn rdm_io(&mut self) -> Result<&UdpSocket, String> {
        if self.rdm.sock.is_none() {
            let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("No se pudo abrir el socket RDM: {e}"))?;
            sock.set_broadcast(true).map_err(|e| e.to_string())?;
            sock.set_read_timeout(Some(Duration::from_millis(200))).map_err(|e| e.to_string())?;
            self.rdm.stop.store(false, Ordering::Relaxed);
            let tx = self.io.sender();
            let spawn = |s: UdpSocket, name: &str, tx: tokio::sync::mpsc::UnboundedSender<IoEvent>, stop: Arc<AtomicBool>| {
                let _ = std::thread::Builder::new().name(name.into()).spawn(move || {
                    let mut buf = [0u8; 1500];
                    while !stop.load(Ordering::Relaxed) {
                        match s.recv_from(&mut buf) {
                            Ok((n, from)) if n > 12 && &buf[..8] == b"Art-Net\0" && matches!(u16::from_le_bytes([buf[8], buf[9]]), 0x8100 | 0x8300) => {
                                let _ = tx.send(IoEvent::Rdm { from, data: buf[..n].to_vec() });
                            }
                            Ok(_) => {}
                            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                            Err(_) => std::thread::sleep(Duration::from_millis(50)),
                        }
                    }
                });
            };
            if let Ok(c) = sock.try_clone() {
                spawn(c, "rdm-rx", tx.clone(), self.rdm.stop.clone());
            }
            // nodes answer to the Art-Net port itself: listen there too (best effort, shared)
            if let Ok(shared) = crate::io::bind_shared_port(self.rdm.art_port) {
                spawn(shared, "rdm-rx-bc", tx, self.rdm.stop.clone());
            }
            self.rdm.sock = Some(sock);
        }
        Ok(self.rdm.sock.as_ref().expect("opened above"))
    }

    fn rdm_send(&mut self, outs: Vec<rdm::Out>) {
        if outs.is_empty() {
            return;
        }
        if let Some(s) = &self.rdm.sock {
            for o in outs {
                let _ = s.send_to(&o.data, o.to);
            }
        }
    }

    /// `(ip, port-addresses)` of every Art-Net node we know, plus the nodes our own Art-Net outputs point at.
    fn rdm_nodes(&self) -> Vec<(Ipv4Addr, Vec<u16>)> {
        let mut map: BTreeMap<Ipv4Addr, BTreeSet<u16>> = BTreeMap::new();
        for n in self.io.nodes().1 {
            if let Ok(ip) = n.ip.parse::<Ipv4Addr>() {
                map.entry(ip).or_default().extend(n.universes.iter().copied());
            }
        }
        for u in &self.show.universes {
            for o in u.outputs.iter().filter(|o| o.enabled) {
                if let OutputKind::ArtNet { host, net, subnet, universe, .. } = &o.kind {
                    if let Ok(ip) = host.trim().parse::<Ipv4Addr>() {
                        map.entry(ip).or_default().insert(port_address(*net, *subnet, *universe));
                    }
                }
            }
        }
        map.into_iter().map(|(ip, s)| (ip, s.into_iter().collect())).collect()
    }

    pub(super) fn rdm_command(&mut self, cmd: Command) -> Result<(), String> {
        match cmd {
            Command::RdmScan { flush } => {
                self.rdm_io()?;
                self.io.nodes_scan(); // refresh the node table first: replies arrive within a second or two
                self.rdm.scan_at = Some((Instant::now() + Duration::from_millis(1200), flush));
                let nodes = self.rdm_nodes();
                let outs = self.rdm.ctl.scan(&nodes, flush);
                self.rdm_send(outs);
                self.dirty = true;
                Ok(())
            }
            Command::RdmRefresh { uid } => {
                self.rdm_io()?;
                self.rdm.ctl.refresh(parse_uid(&uid)?);
                Ok(())
            }
            Command::RdmIdentify { uid, on } => {
                self.rdm_io()?;
                self.rdm.ctl.identify(parse_uid(&uid)?, on)
            }
            Command::RdmSetAddress { uid, address } => {
                self.rdm_io()?;
                self.rdm.ctl.set_address(parse_uid(&uid)?, address)
            }
            Command::RdmSetLabel { uid, label } => {
                self.rdm_io()?;
                self.rdm.ctl.set_label(parse_uid(&uid)?, &label)
            }
            Command::RdmSetPersonality { uid, personality } => {
                self.rdm_io()?;
                self.rdm.ctl.set_personality(parse_uid(&uid)?, personality)
            }
            Command::RdmClear => {
                self.rdm.ctl.clear();
                self.dirty = true;
                Ok(())
            }
            _ => Err("Comando RDM desconocido".into()),
        }
    }

    pub(super) fn on_rdm_packet(&mut self, from: SocketAddr, data: Vec<u8>) {
        self.rdm.ctl.on_packet(from, &data);
        self.dirty = true;
    }

    /// Every tick: second half of a scan, and the request queue.
    pub(super) fn rdm_tick(&mut self) {
        if self.rdm.sock.is_none() {
            return;
        }
        if let Some((at, flush)) = self.rdm.scan_at {
            if Instant::now() >= at {
                self.rdm.scan_at = None;
                let nodes = self.rdm_nodes();
                let outs = self.rdm.ctl.scan(&nodes, flush);
                self.rdm_send(outs);
            }
        }
        let outs = self.rdm.ctl.tick();
        if !outs.is_empty() {
            self.rdm_send(outs);
            self.dirty = true;
        }
    }

    pub(super) fn rdm_rt(&self) -> RdmRt {
        let mut devices = self.rdm.ctl.devices();
        if !devices.is_empty() {
            // port-address → Lumos universe, through the Art-Net outputs
            let mut by_pa: HashMap<u16, u16> = HashMap::new();
            for u in &self.show.universes {
                for o in &u.outputs {
                    if let OutputKind::ArtNet { net, subnet, universe, .. } = &o.kind {
                        by_pa.entry(port_address(*net, *subnet, *universe)).or_insert(u.id);
                    }
                }
            }
            for d in &mut devices {
                d.seen_s = d.seen_s / 5 * 5;
                if let Some(u) = by_pa.get(&d.port_address) {
                    d.fixture = self.show.fixtures.iter().find(|f| f.universe == *u && f.address == d.address && d.address > 0).map(|f| f.id);
                }
            }
        }
        RdmRt { busy: self.rdm.ctl.busy() || self.rdm.scan_at.is_some(), devices, error: self.rdm.ctl.error.clone() }
    }
}
