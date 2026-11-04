//! Art-Net packets that carry RDM: ArtTodRequest (0x8000), ArtTodData (0x8100), ArtTodControl (0x8200), ArtRdm (0x8300).

use super::Uid;

const ID: &[u8; 8] = b"Art-Net\0";
pub const OP_TOD_REQUEST: u16 = 0x8000;
pub const OP_TOD_DATA: u16 = 0x8100;
pub const OP_TOD_CONTROL: u16 = 0x8200;
pub const OP_RDM: u16 = 0x8300;

fn header(op: u16, prot: u8) -> Vec<u8> {
    let mut v = ID.to_vec();
    v.extend_from_slice(&op.to_le_bytes());
    v.push(0);
    v.push(prot);
    v
}

/// ArtTodRequest, full table of devices of one port-address (net 7 bits + sub-uni 8 bits).
pub fn tod_request(port_address: u16) -> Vec<u8> {
    let mut v = header(OP_TOD_REQUEST, 14);
    v.extend_from_slice(&[0, 0]); // Filler1, Filler2
    v.extend_from_slice(&[0; 7]); // Spare
    v.push((port_address >> 8) as u8 & 0x7F); // Net
    v.push(0); // Command: TodFull
    v.push(1); // AddCount
    v.push(port_address as u8); // Address[0]
    v
}

/// ArtTodControl; `flush` = AtcFlush: clear the node's table and run discovery again.
pub fn tod_control(port_address: u16, flush: bool) -> Vec<u8> {
    let mut v = header(OP_TOD_CONTROL, 14);
    v.extend_from_slice(&[0, 0]);
    v.extend_from_slice(&[0; 7]);
    v.push((port_address >> 8) as u8 & 0x7F);
    v.push(if flush { 1 } else { 0 });
    v.push(port_address as u8);
    v
}

/// ArtRdm wrapping an RDM packet without the DMX start code (checksum included).
pub fn rdm(port_address: u16, rdm_packet: &[u8]) -> Vec<u8> {
    let mut v = header(OP_RDM, 14);
    v.push(1); // RdmVer
    v.push(0); // Filler2
    v.extend_from_slice(&[0; 7]); // Spare
    v.push((port_address >> 8) as u8 & 0x7F);
    v.push(0); // Command: ArProcess
    v.push(port_address as u8);
    v.extend_from_slice(rdm_packet);
    v
}

/// ArtTodData as a node sends it (used by tests and by the simulator).
pub fn tod_data(port_address: u16, uids: &[Uid]) -> Vec<u8> {
    let mut v = header(OP_TOD_DATA, 14);
    v.push(1); // RdmVer
    v.push(1); // Port
    v.extend_from_slice(&[0; 6]); // Spare
    v.push(0); // BindIndex
    v.push((port_address >> 8) as u8 & 0x7F);
    v.push(0); // CommandResponse: TodFull
    v.push(port_address as u8);
    v.extend_from_slice(&(uids.len() as u16).to_be_bytes());
    v.push(0); // BlockCount
    v.push(uids.len().min(200) as u8);
    for u in uids.iter().take(200) {
        v.extend_from_slice(&u.to_bytes());
    }
    v
}

#[derive(Debug, PartialEq)]
pub enum Packet {
    TodData { port_address: u16, uids: Vec<Uid> },
    Rdm { port_address: u16, rdm: Vec<u8> },
}

pub fn parse(p: &[u8]) -> Option<Packet> {
    if p.len() < 12 || &p[..8] != ID {
        return None;
    }
    match u16::from_le_bytes([p[8], p[9]]) {
        OP_TOD_DATA if p.len() >= 28 => {
            let pa = (((p[21] & 0x7F) as u16) << 8) | p[23] as u16;
            let (cmd, count) = (p[22], p[27] as usize);
            if cmd != 0 || p.len() < 28 + count * 6 {
                return None;
            }
            Some(Packet::TodData { port_address: pa, uids: (0..count).filter_map(|i| Uid::from_bytes(&p[28 + i * 6..])).collect() })
        }
        OP_RDM if p.len() >= 24 => {
            let pa = (((p[21] & 0x7F) as u16) << 8) | p[23] as u16;
            Some(Packet::Rdm { port_address: pa, rdm: p[24..].to_vec() })
        }
        _ => None,
    }
}
