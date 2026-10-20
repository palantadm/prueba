//! Art-Net 4: ArtDmx output, ArtPoll/ArtPollReply discovery and ArtDmx input.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio::sync::mpsc::UnboundedSender;

use super::{push_recent, IoEvent, Recent, Sink, Stat};
use crate::model::{ArtNetInMap, OutState};

pub const ARTNET_PORT: u16 = 6454;
const ID: &[u8; 8] = b"Art-Net\0";
const OP_POLL: u16 = 0x2000;
const OP_POLL_REPLY: u16 = 0x2100;
const OP_DMX: u16 = 0x5000;

/// 15-bit port-address: net(7) | subnet(4) | universe(4).
pub fn port_address(net: u8, subnet: u8, universe: u8) -> u16 {
    ((net as u16 & 0x7F) << 8) | ((subnet as u16 & 0x0F) << 4) | (universe as u16 & 0x0F)
}

/// ArtDmx packet: 18-byte header + 512 channels.
pub fn build_artdmx(seq: u8, port_address: u16, data: &[u8; 512]) -> [u8; 530] {
    let mut p = [0u8; 530];
    p[..8].copy_from_slice(ID);
    p[8] = (OP_DMX & 0xFF) as u8;
    p[9] = (OP_DMX >> 8) as u8;
    p[10] = 0; // ProtVer hi
    p[11] = 14; // ProtVer lo
    p[12] = seq;
    p[13] = 0; // physical
    p[14] = (port_address & 0xFF) as u8; // SubUni
    p[15] = (port_address >> 8) as u8; // Net
    p[16] = 0x02; // length hi (512)
    p[17] = 0x00;
    p[18..].copy_from_slice(data);
    p
}

/// Parsed ArtDmx: (port-address, channel data).
pub fn parse_artdmx(buf: &[u8]) -> Option<(u16, &[u8])> {
    if buf.len() < 18 || &buf[..8] != ID {
        return None;
    }
    if u16::from_le_bytes([buf[8], buf[9]]) != OP_DMX {
        return None;
    }
    let len = ((buf[16] as usize) << 8) | buf[17] as usize;
    if len == 0 || len > 512 || buf.len() < 18 + len {
        return None;
    }
    let pa = ((buf[15] as u16 & 0x7F) << 8) | buf[14] as u16;
    Some((pa, &buf[18..18 + len]))
}

fn parse_poll(buf: &[u8]) -> bool {
    buf.len() >= 12 && &buf[..8] == ID && u16::from_le_bytes([buf[8], buf[9]]) == OP_POLL
}

fn fixed<const N: usize>(dst: &mut [u8], s: &str) {
    let b = s.as_bytes();
    let n = b.len().min(N - 1);
    dst[..n].copy_from_slice(&b[..n]);
}

/// 239-byte ArtPollReply advertising up to four DMX output ports.
pub fn build_poll_reply(ip: Ipv4Addr, node_name: &str, out_ports: &[u16]) -> [u8; 239] {
    let mut p = [0u8; 239];
    p[..8].copy_from_slice(ID);
    p[8] = (OP_POLL_REPLY & 0xFF) as u8;
    p[9] = (OP_POLL_REPLY >> 8) as u8;
    p[10..14].copy_from_slice(&ip.octets());
    p[14] = (ARTNET_PORT & 0xFF) as u8; // port, low byte first
    p[15] = (ARTNET_PORT >> 8) as u8;
    p[16] = 0; // firmware hi
    p[17] = 1; // firmware lo
    let first = out_ports.first().copied().unwrap_or(0);
    p[18] = (first >> 8) as u8; // NetSwitch
    p[19] = ((first >> 4) & 0x0F) as u8; // SubSwitch
    p[20] = 0x00; // OEM hi
    p[21] = 0xFF; // OEM lo (unknown)
    p[22] = 0; // UBEA
    p[23] = 0xE0; // Status1: indicators normal, port-address programmed by network
    p[24] = 0x7F; // ESTA lo ("no manufacturer" prototype range)
    p[25] = 0xF0; // ESTA hi
    fixed::<18>(&mut p[26..44], node_name);
    fixed::<64>(&mut p[44..108], &format!("{node_name} - Lumos lighting control"));
    fixed::<64>(&mut p[108..172], "#0001 [0000] Lumos running");
    let n = out_ports.len().min(4);
    p[172] = 0;
    p[173] = n as u8; // NumPorts
    for (i, pa) in out_ports.iter().take(4).enumerate() {
        p[174 + i] = 0x80; // PortTypes: can output DMX512
        p[182 + i] = 0x80; // GoodOutput: data being transmitted
        p[190 + i] = (pa & 0x0F) as u8; // SwOut
    }
    p[200] = 0; // Style: StNode
    p[211] = 1; // BindIndex
    p[212] = 0x08; // Status2: supports 15-bit port-address
    p
}

// ───────────────────────────── output ─────────────────────────────

pub struct ArtNetSink {
    sock: UdpSocket,
    primary: SocketAddr,
    fallbacks: Vec<SocketAddr>,
    broadcast: bool,
    pa: u16,
    pub(crate) seq: u8,
    stat: Stat,
    label: String,
}

impl ArtNetSink {
    pub fn new(host: &str, port: u16, net: u8, subnet: u8, universe: u8) -> Result<Self, String> {
        let host = host.trim();
        let broadcast = host.is_empty();
        let primary = if broadcast {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), port)
        } else if let Ok(ip) = host.parse::<IpAddr>() {
            SocketAddr::new(ip, port)
        } else {
            (host, port)
                .to_socket_addrs()
                .map_err(|e| format!("{host}: {e}"))?
                .find(|a| a.is_ipv4())
                .ok_or_else(|| format!("{host}: no IPv4 address"))?
        };
        let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        sock.set_broadcast(true).map_err(|e| e.to_string())?;
        sock.set_nonblocking(true).map_err(|e| e.to_string())?;
        let fallbacks = if broadcast { directed_broadcasts(port) } else { vec![] };
        let label = if broadcast {
            format!("→ broadcast :{port}")
        } else {
            format!("→ {primary}")
        };
        Ok(ArtNetSink {
            sock,
            primary,
            fallbacks,
            broadcast,
            pa: port_address(net, subnet, universe),
            seq: 0,
            stat: Stat::new(OutState::Ok, label.clone()),
            label,
        })
    }

    fn next_seq(&mut self) -> u8 {
        self.seq = if self.seq == 255 { 1 } else { self.seq + 1 };
        self.seq
    }
}

/// `/24` directed broadcast of every local IPv4 interface (netmasks are not exposed portably).
fn directed_broadcasts(port: u16) -> Vec<SocketAddr> {
    let mut v = Vec::new();
    if let Ok(ifs) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in ifs {
            if let IpAddr::V4(a) = ip {
                if a.is_loopback() || a.is_link_local() {
                    continue;
                }
                let o = a.octets();
                v.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(o[0], o[1], o[2], 255)), port));
            }
        }
    }
    v
}

impl Sink for ArtNetSink {
    fn send(&mut self, _universe: u16, data: &[u8; 512]) {
        let seq = self.next_seq();
        let pkt = build_artdmx(seq, self.pa, data);
        let mut res = self.sock.send_to(&pkt, self.primary);
        if res.is_err() && self.broadcast {
            // 255.255.255.255 can be unreachable on some hosts — fall back to directed broadcasts.
            for t in &self.fallbacks {
                if let Ok(n) = self.sock.send_to(&pkt, t) {
                    res = Ok(n);
                }
            }
        }
        match res {
            Ok(_) => {
                self.stat.frames = self.stat.frames.wrapping_add(1);
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

    fn stat(&self) -> Stat {
        self.stat.clone()
    }
}

// ───────────────────────────── listener (poll + input) ─────────────────────────────

pub struct ArtShared {
    pub node_name: Mutex<String>,
    pub in_enabled: AtomicBool,
    pub map: Mutex<Vec<ArtNetInMap>>,
    /// Port-addresses of enabled outputs (advertised in ArtPollReply).
    pub out_ports: Mutex<Vec<u16>>,
}

impl Default for ArtShared {
    fn default() -> Self {
        ArtShared {
            node_name: Mutex::new("Lumos".into()),
            in_enabled: AtomicBool::new(false),
            map: Mutex::new(vec![]),
            out_ports: Mutex::new(vec![]),
        }
    }
}

impl ArtShared {
    pub fn in_enabled_now(&self) -> bool {
        self.in_enabled.load(Ordering::Relaxed)
    }
}

pub struct ArtNetListener {
    stop: Arc<AtomicBool>,
}

impl Drop for ArtNetListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn bind_shared(port: u16) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_reuse_address(true)?;
    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    s.set_reuse_port(true)?;
    s.set_broadcast(true)?;
    s.bind(&SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port).into())?;
    let u: UdpSocket = s.into();
    u.set_read_timeout(Some(Duration::from_millis(200)))?;
    Ok(u)
}

impl ArtNetListener {
    pub fn start(
        port: u16,
        shared: Arc<ArtShared>,
        tx: UnboundedSender<IoEvent>,
        recent: Recent,
    ) -> Result<Self, String> {
        let sock = bind_shared(port).map_err(|e| format!("Art-Net :{port}: {e}"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        std::thread::Builder::new()
            .name("artnet-in".into())
            .spawn(move || listener_loop(sock, shared, tx, recent, stop2))
            .map_err(|e| e.to_string())?;
        Ok(ArtNetListener { stop })
    }
}

struct PortState {
    last: Vec<u8>,
    sent: Instant,
    logged: Instant,
}

fn listener_loop(
    sock: UdpSocket,
    shared: Arc<ArtShared>,
    tx: UnboundedSender<IoEvent>,
    recent: Recent,
    stop: Arc<AtomicBool>,
) {
    let mut buf = [0u8; 1024];
    let mut ports: HashMap<(u16, u16), PortState> = HashMap::new();
    while !stop.load(Ordering::Relaxed) {
        let (n, src) = match sock.recv_from(&mut buf) {
            Ok(x) => x,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(_) => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        let pkt = &buf[..n];
        if parse_poll(pkt) {
            // Reply with the address of the interface that reaches the poller.
            let ip = UdpSocket::bind("0.0.0.0:0")
                .and_then(|s| s.connect(src).map(|_| s))
                .and_then(|s| s.local_addr())
                .ok()
                .and_then(|a| match a.ip() {
                    IpAddr::V4(v) => Some(v),
                    _ => None,
                })
                .unwrap_or(Ipv4Addr::UNSPECIFIED);
            let reply = build_poll_reply(ip, &shared.node_name.lock(), &shared.out_ports.lock());
            let _ = sock.send_to(&reply, src);
            continue;
        }
        if !shared.in_enabled.load(Ordering::Relaxed) {
            continue;
        }
        let Some((pa, data)) = parse_artdmx(pkt) else { continue };
        let targets: Vec<u16> = shared
            .map
            .lock()
            .iter()
            .filter(|m| port_address(m.net, m.subnet, m.universe) == pa)
            .map(|m| m.target)
            .collect();
        for target in targets {
            let mut frame = data.to_vec();
            frame.resize(512, 0);
            let now = Instant::now();
            let st = ports.entry((pa, target)).or_insert_with(|| PortState {
                last: vec![],
                sent: now - Duration::from_secs(10),
                logged: now - Duration::from_secs(10),
            });
            let changed = st.last != frame;
            if changed || now.duration_since(st.sent) >= Duration::from_secs(1) {
                st.last = frame.clone();
                st.sent = now;
                if changed && now.duration_since(st.logged) >= Duration::from_secs(1) {
                    st.logged = now;
                    push_recent(
                        &recent,
                        format!("Art-Net {}:{}:{} → U{}", pa >> 8, (pa >> 4) & 0xF, pa & 0xF, target),
                    );
                }
                let _ = tx.send(IoEvent::ExternalDmx { universe: target, data: frame });
            }
        }
    }
}
