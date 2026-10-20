//! sACN / ANSI E1.31 data packets (unicast or multicast 239.255.hi.lo:5568).

use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::OnceLock;

use super::{Sink, Stat};
use crate::model::OutState;

pub const SACN_PORT: u16 = 5568;
pub const PACKET_LEN: usize = 638;

/// Component identifier, stable for the lifetime of the process.
pub fn cid() -> [u8; 16] {
    static CID: OnceLock<[u8; 16]> = OnceLock::new();
    *CID.get_or_init(|| {
        use std::hash::{BuildHasher, Hasher};
        let rs = std::collections::hash_map::RandomState::new();
        let mut out = [0u8; 16];
        for (i, chunk) in out.chunks_mut(8).enumerate() {
            let mut h = rs.build_hasher();
            h.write_u32(std::process::id());
            h.write_u8(i as u8);
            chunk.copy_from_slice(&h.finish().to_be_bytes());
        }
        out[6] = (out[6] & 0x0F) | 0x40;
        out[8] = (out[8] & 0x3F) | 0x80;
        out
    })
}

pub fn multicast_addr(universe: u16) -> Ipv4Addr {
    Ipv4Addr::new(239, 255, (universe >> 8) as u8, (universe & 0xFF) as u8)
}

pub fn build_packet(
    cid: &[u8; 16],
    source_name: &str,
    priority: u8,
    seq: u8,
    universe: u16,
    data: &[u8; 512],
) -> [u8; PACKET_LEN] {
    let mut p = [0u8; PACKET_LEN];
    // Root layer
    p[0..2].copy_from_slice(&0x0010u16.to_be_bytes()); // preamble size
    p[2..4].copy_from_slice(&0x0000u16.to_be_bytes()); // postamble size
    p[4..16].copy_from_slice(b"ASC-E1.17\0\0\0");
    p[16..18].copy_from_slice(&(0x7000u16 | (PACKET_LEN as u16 - 16)).to_be_bytes());
    p[18..22].copy_from_slice(&0x0000_0004u32.to_be_bytes()); // VECTOR_ROOT_E131_DATA
    p[22..38].copy_from_slice(cid);
    // Framing layer
    p[38..40].copy_from_slice(&(0x7000u16 | (PACKET_LEN as u16 - 38)).to_be_bytes());
    p[40..44].copy_from_slice(&0x0000_0002u32.to_be_bytes()); // VECTOR_E131_DATA_PACKET
    let name = source_name.as_bytes();
    let n = name.len().min(63);
    p[44..44 + n].copy_from_slice(&name[..n]);
    p[108] = priority.min(200);
    p[109..111].copy_from_slice(&0u16.to_be_bytes()); // sync address
    p[111] = seq;
    p[112] = 0; // options
    p[113..115].copy_from_slice(&universe.to_be_bytes());
    // DMP layer
    p[115..117].copy_from_slice(&(0x7000u16 | (PACKET_LEN as u16 - 115)).to_be_bytes());
    p[117] = 0x02; // VECTOR_DMP_SET_PROPERTY
    p[118] = 0xA1; // address & data type
    p[119..121].copy_from_slice(&0u16.to_be_bytes()); // first property address
    p[121..123].copy_from_slice(&1u16.to_be_bytes()); // address increment
    p[123..125].copy_from_slice(&513u16.to_be_bytes()); // property value count
    p[125] = 0; // DMX start code
    p[126..].copy_from_slice(data);
    p
}

pub struct SacnSink {
    sock: UdpSocket,
    target: SocketAddr,
    universe: u16,
    priority: u8,
    name: String,
    seq: u8,
    stat: Stat,
    label: String,
}

impl SacnSink {
    pub fn new(
        universe: u16,
        priority: u8,
        unicast: Option<&str>,
        source_name: &str,
        port: u16,
    ) -> Result<Self, String> {
        use socket2::{Domain, Protocol, Socket, Type};
        let unicast = unicast.map(str::trim).filter(|s| !s.is_empty());
        let (target, label) = match unicast {
            Some(ip) => {
                let ip: IpAddr = ip.parse().map_err(|_| format!("invalid IP address: {ip}"))?;
                (SocketAddr::new(ip, port), format!("→ {ip}:{port} (unicast)"))
            }
            None => {
                let g = multicast_addr(universe);
                (SocketAddr::new(IpAddr::V4(g), port), format!("→ {g}:{port} (multicast)"))
            }
        };
        let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| e.to_string())?;
        s.set_multicast_ttl_v4(8).map_err(|e| e.to_string())?;
        s.set_multicast_loop_v4(true).map_err(|e| e.to_string())?;
        s.bind(&SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0).into())
            .map_err(|e| e.to_string())?;
        s.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(SacnSink {
            sock: s.into(),
            target,
            universe,
            priority,
            name: source_name.to_string(),
            seq: 0,
            stat: Stat::new(OutState::Ok, label.clone()),
            label,
        })
    }
}

impl Sink for SacnSink {
    fn send(&mut self, _universe: u16, data: &[u8; 512]) {
        self.seq = self.seq.wrapping_add(1);
        let pkt = build_packet(&cid(), &self.name, self.priority, self.seq, self.universe, data);
        match self.sock.send_to(&pkt, self.target) {
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
