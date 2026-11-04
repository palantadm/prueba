//! Art-Net node discovery: ArtPoll out, ArtPollReply in → a table of nodes (ip, names, output port-addresses).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::artnet::{bind_shared, ID, OP_POLL, OP_POLL_REPLY};
use crate::model::ArtNode;

pub const NODE_TTL: Duration = Duration::from_secs(20);

/// ArtPoll packet (14 bytes): TalkToMe = 0x02 (also reply when conditions change), priority 0.
pub fn build_poll() -> [u8; 14] {
    let mut p = [0u8; 14];
    p[..8].copy_from_slice(ID);
    p[8] = (OP_POLL & 0xFF) as u8;
    p[9] = (OP_POLL >> 8) as u8;
    p[11] = 14;
    p[12] = 0x02;
    p
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeInfo {
    pub ip: Ipv4Addr,
    pub name: String,
    pub long_name: String,
    pub bind: u8,
    /// Output port-addresses (ports whose type says "can output DMX512").
    pub universes: Vec<u16>,
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

pub fn parse_poll_reply(p: &[u8]) -> Option<NodeInfo> {
    if p.len() < 207 || &p[..8] != ID || u16::from_le_bytes([p[8], p[9]]) != OP_POLL_REPLY {
        return None;
    }
    let ip = Ipv4Addr::new(p[10], p[11], p[12], p[13]);
    let net = p[18] as u16 & 0x7F;
    let sub = p[19] as u16 & 0x0F;
    let n = (((p[172] as usize) << 8) | p[173] as usize).min(4);
    let universes = (0..n)
        .filter(|i| p[174 + i] & 0x80 != 0)
        .map(|i| (net << 8) | (sub << 4) | (p[190 + i] as u16 & 0x0F))
        .collect();
    Some(NodeInfo { ip, name: cstr(&p[26..44]), long_name: cstr(&p[44..108]), bind: p.get(211).copied().unwrap_or(0), universes })
}

struct NodeEntry {
    name: String,
    long_name: String,
    binds: BTreeMap<u8, BTreeSet<u16>>,
    seen: Instant,
}

/// Nodes seen via ArtPollReply. Entries expire after [`NODE_TTL`]; `version` bumps on structural changes.
pub struct NodeTable {
    inner: Mutex<HashMap<Ipv4Addr, NodeEntry>>,
    version: AtomicU32,
    /// Ignore replies from this machine's own addresses (our own node answering a poll).
    pub ignore_local: AtomicBool,
}

impl Default for NodeTable {
    fn default() -> Self {
        NodeTable { inner: Mutex::new(HashMap::new()), version: AtomicU32::new(0), ignore_local: AtomicBool::new(true) }
    }
}

impl NodeTable {
    fn is_local(ip: Ipv4Addr) -> bool {
        ip.is_unspecified()
            || local_ip_address::list_afinet_netifas().map(|v| v.into_iter().any(|(_, a)| a == IpAddr::V4(ip))).unwrap_or(false)
    }

    pub fn update(&self, info: NodeInfo) {
        if self.ignore_local.load(Ordering::Relaxed) && Self::is_local(info.ip) {
            return;
        }
        let unis: BTreeSet<u16> = info.universes.iter().copied().collect();
        let mut g = self.inner.lock();
        let changed = match g.get_mut(&info.ip) {
            Some(e) => {
                let ch = e.name != info.name || e.long_name != info.long_name || e.binds.get(&info.bind) != Some(&unis);
                e.name = info.name;
                e.long_name = info.long_name;
                e.binds.insert(info.bind, unis);
                e.seen = Instant::now();
                ch
            }
            None => {
                let mut binds = BTreeMap::new();
                binds.insert(info.bind, unis);
                g.insert(info.ip, NodeEntry { name: info.name, long_name: info.long_name, binds, seen: Instant::now() });
                true
            }
        };
        if changed {
            self.version.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn ips(&self) -> Vec<Ipv4Addr> {
        self.inner.lock().keys().copied().collect()
    }

    /// Prunes expired nodes; returns `(version, nodes sorted by ip)`.
    pub fn snapshot(&self) -> (u32, Vec<ArtNode>) {
        let mut g = self.inner.lock();
        let before = g.len();
        g.retain(|_, e| e.seen.elapsed() < NODE_TTL);
        if g.len() != before {
            self.version.fetch_add(1, Ordering::Relaxed);
        }
        let mut v: Vec<(Ipv4Addr, ArtNode)> = g
            .iter()
            .map(|(ip, e)| {
                let mut u: Vec<u16> = e.binds.values().flatten().copied().collect();
                u.sort_unstable();
                u.dedup();
                (
                    *ip,
                    ArtNode {
                        ip: ip.to_string(),
                        name: e.name.clone(),
                        long_name: e.long_name.clone(),
                        universes: u,
                        last_seen_ms: e.seen.elapsed().as_millis().min(u32::MAX as u128) as u32,
                    },
                )
            })
            .collect();
        v.sort_by_key(|(ip, _)| *ip);
        (self.version.load(Ordering::Relaxed), v.into_iter().map(|(_, n)| n).collect())
    }
}

/// Where to send ArtPoll: limited + directed broadcasts of the local interfaces, the Art-Net 2.x / 10.x
/// broadcast ranges, and unicast to every node already known.
pub fn poll_targets(port: u16, known: &[Ipv4Addr]) -> Vec<SocketAddr> {
    let mut ips: BTreeSet<Ipv4Addr> = BTreeSet::new();
    ips.insert(Ipv4Addr::BROADCAST);
    if let Ok(list) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in list {
            if let IpAddr::V4(v4) = ip {
                if v4.is_loopback() {
                    continue;
                }
                let o = v4.octets();
                ips.insert(Ipv4Addr::new(o[0], o[1], o[2], 255));
                if o[0] == 2 || o[0] == 10 {
                    ips.insert(Ipv4Addr::new(o[0], 255, 255, 255));
                }
            }
        }
    }
    ips.extend(known.iter().copied());
    ips.into_iter().map(|ip| SocketAddr::new(IpAddr::V4(ip), port)).collect()
}

/// Sends ArtPoll and collects the replies into a [`NodeTable`] on background threads.
pub struct NodeScanner {
    sock: UdpSocket,
    stop: Arc<AtomicBool>,
}

impl Drop for NodeScanner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn reader(sock: UdpSocket, table: Arc<NodeTable>, stop: Arc<AtomicBool>, name: &str) {
    let _ = std::thread::Builder::new().name(name.into()).spawn(move || {
        let mut buf = [0u8; 1024];
        while !stop.load(Ordering::Relaxed) {
            match sock.recv_from(&mut buf) {
                Ok((n, _)) => {
                    if let Some(info) = parse_poll_reply(&buf[..n]) {
                        table.update(info);
                    }
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    });
}

impl NodeScanner {
    /// `listen_port`: also listen (best effort, shared) for broadcast replies on the Art-Net port.
    pub fn start(table: Arc<NodeTable>, listen_port: Option<u16>) -> std::io::Result<Self> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.set_broadcast(true)?;
        sock.set_read_timeout(Some(Duration::from_millis(200)))?;
        let stop = Arc::new(AtomicBool::new(false));
        reader(sock.try_clone()?, table.clone(), stop.clone(), "artnet-scan");
        if let Some(p) = listen_port {
            if let Ok(s) = bind_shared(p) {
                reader(s, table, stop.clone(), "artnet-scan-bc");
            }
        }
        Ok(NodeScanner { sock, stop })
    }

    pub fn poll(&self, targets: &[SocketAddr]) {
        let pkt = build_poll();
        for t in targets {
            let _ = self.sock.send_to(&pkt, t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::artnet::build_poll_reply;
    use super::*;

    #[test]
    fn poll_packet_and_reply_round_trip() {
        let p = build_poll();
        assert_eq!(&p[..8], b"Art-Net\0");
        assert_eq!(u16::from_le_bytes([p[8], p[9]]), 0x2000);
        assert_eq!(p[11], 14);
        let r = build_poll_reply(Ipv4Addr::new(192, 168, 1, 77), "Test Node", &[0x0001, 0x0002, 0x0105]);
        let i = parse_poll_reply(&r).unwrap();
        assert_eq!(i.ip, Ipv4Addr::new(192, 168, 1, 77));
        assert_eq!(i.name, "Test Node");
        assert!(i.long_name.starts_with("Test Node"));
        // net/sub come from the first port; swout from each: (0,0,1) (0,0,2) (0,0,5)
        assert_eq!(i.universes, vec![0x0001, 0x0002, 0x0005]);
        assert!(parse_poll_reply(&r[..100]).is_none());
        assert!(parse_poll_reply(&build_poll()).is_none());
    }

    #[test]
    fn table_merges_binds_expires_and_versions() {
        let t = NodeTable::default();
        t.ignore_local.store(false, Ordering::Relaxed);
        let mk = |bind: u8, u: &[u16]| NodeInfo { ip: Ipv4Addr::new(10, 0, 0, 5), name: "N".into(), long_name: "Long".into(), bind, universes: u.to_vec() };
        t.update(mk(1, &[0, 1]));
        let (v1, n1) = t.snapshot();
        assert_eq!(n1.len(), 1);
        assert_eq!(n1[0].universes, vec![0, 1]);
        t.update(mk(1, &[0, 1]));
        assert_eq!(t.snapshot().0, v1, "a repeated identical reply is not a change");
        t.update(mk(2, &[4, 5]));
        let (v2, n2) = t.snapshot();
        assert!(v2 > v1);
        assert_eq!(n2[0].universes, vec![0, 1, 4, 5]);
        // expire
        t.inner.lock().get_mut(&Ipv4Addr::new(10, 0, 0, 5)).unwrap().seen = Instant::now() - NODE_TTL - Duration::from_secs(1);
        let (v3, n3) = t.snapshot();
        assert!(n3.is_empty() && v3 > v2);
    }

    #[test]
    fn local_replies_are_ignored_by_default() {
        let t = NodeTable::default();
        t.update(NodeInfo { ip: Ipv4Addr::new(127, 0, 0, 1), name: "me".into(), long_name: String::new(), bind: 0, universes: vec![] });
        assert!(t.snapshot().1.is_empty());
    }

    /// A fake node on loopback answers an ArtPoll with an ArtPollReply (unicast to the poller).
    #[test]
    fn scanner_discovers_unicast_node() {
        let node = UdpSocket::bind("127.0.0.1:0").unwrap();
        node.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let node_addr = node.local_addr().unwrap();
        let h = std::thread::spawn(move || {
            let mut buf = [0u8; 64];
            let (n, src) = node.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[..n], &build_poll());
            let reply = build_poll_reply(Ipv4Addr::new(10, 1, 2, 3), "Fake Node", &[0x0010, 0x0011]);
            node.send_to(&reply, src).unwrap();
        });
        let table = Arc::new(NodeTable::default());
        table.ignore_local.store(false, Ordering::Relaxed);
        let sc = NodeScanner::start(table.clone(), None).unwrap();
        sc.poll(&[node_addr]);
        h.join().unwrap();
        let mut found = vec![];
        for _ in 0..40 {
            found = table.snapshot().1;
            if !found.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].ip.as_str(), found[0].name.as_str()), ("10.1.2.3", "Fake Node"));
        assert_eq!(found[0].universes, vec![0x10, 0x11]);
    }

    #[test]
    fn poll_targets_include_broadcast_and_known() {
        let t = poll_targets(6454, &[Ipv4Addr::new(192, 168, 9, 9)]);
        assert!(t.contains(&"255.255.255.255:6454".parse().unwrap()));
        assert!(t.contains(&"192.168.9.9:6454".parse().unwrap()));
    }
}
