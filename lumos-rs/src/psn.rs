//! PosiStageNet (PSN) v2 — tracking data over UDP multicast (`236.10.10.10:56565`).
//!
//! BlackTrax, Zactrack, Stage Precision, Kinesys, Notch… send it. Only the receiving side is implemented, and only what a
//! lighting desk needs: tracker id, name and position (metres). Everything is little-endian chunks:
//! `u32 header = id (16 bits) | data_len (15 bits) << 16 | has_subchunks << 31`, then `data_len` bytes.

pub const PSN_PORT: u16 = 56565;
pub const PSN_GROUP: &str = "236.10.10.10";

const DATA_PACKET: u16 = 0x6755;
const INFO_PACKET: u16 = 0x6756;
// data packet children
const DATA_HEADER: u16 = 0x0000;
const DATA_TRACKER_LIST: u16 = 0x0001;
const TRACKER_POS: u16 = 0x0000;
const TRACKER_STATUS: u16 = 0x0003;
// info packet children
const INFO_SYSTEM_NAME: u16 = 0x0001;
const INFO_TRACKER_LIST: u16 = 0x0002;
const TRACKER_NAME: u16 = 0x0000;

#[derive(Clone, Debug, PartialEq)]
pub struct TrackerData {
    pub id: u16,
    /// Metres, in the tracking system's own axes.
    pub pos: Option<[f32; 3]>,
    /// 0..1 confidence when the system sends it.
    pub validity: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Packet {
    Data { timestamp_us: u64, trackers: Vec<TrackerData> },
    Info { system: String, names: Vec<(u16, String)> },
}

struct Chunk<'a> {
    id: u16,
    data: &'a [u8],
    nested: bool,
}

/// Split `buf` into consecutive chunks; `None` when one runs past the end (damaged or truncated packet).
fn chunks(mut buf: &[u8]) -> Option<Vec<Chunk<'_>>> {
    let mut out = Vec::new();
    while !buf.is_empty() {
        if buf.len() < 4 {
            return None;
        }
        let h = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let (id, len, nested) = ((h & 0xFFFF) as u16, ((h >> 16) & 0x7FFF) as usize, h >> 31 == 1);
        buf = &buf[4..];
        if len > buf.len() {
            return None;
        }
        out.push(Chunk { id, data: &buf[..len], nested });
        buf = &buf[len..];
    }
    Some(out)
}

fn f32s<const N: usize>(d: &[u8]) -> Option<[f32; N]> {
    if d.len() < N * 4 {
        return None;
    }
    let mut v = [0.0; N];
    for (i, x) in v.iter_mut().enumerate() {
        *x = f32::from_le_bytes([d[i * 4], d[i * 4 + 1], d[i * 4 + 2], d[i * 4 + 3]]);
    }
    Some(v)
}

fn text(d: &[u8]) -> String {
    let end = d.iter().position(|b| *b == 0).unwrap_or(d.len());
    String::from_utf8_lossy(&d[..end]).trim().chars().filter(|c| !c.is_control()).take(64).collect()
}

pub fn parse(buf: &[u8]) -> Option<Packet> {
    let top = chunks(buf)?;
    let root = top.first()?;
    if !root.nested {
        return None;
    }
    match root.id {
        DATA_PACKET => {
            let mut ts = 0u64;
            let mut trackers = Vec::new();
            for c in chunks(root.data)? {
                match c.id {
                    DATA_HEADER if c.data.len() >= 8 => ts = u64::from_le_bytes(c.data[..8].try_into().ok()?),
                    DATA_TRACKER_LIST => {
                        for t in chunks(c.data)? {
                            let (mut pos, mut validity) = (None, None);
                            for f in chunks(t.data)? {
                                match f.id {
                                    TRACKER_POS => pos = f32s::<3>(f.data),
                                    TRACKER_STATUS => validity = f32s::<1>(f.data).map(|v| v[0]),
                                    _ => {}
                                }
                            }
                            // NaN / infinite positions mean "no data" in practice
                            let pos = pos.filter(|p| p.iter().all(|v| v.is_finite()));
                            trackers.push(TrackerData { id: t.id, pos, validity });
                        }
                    }
                    _ => {}
                }
            }
            Some(Packet::Data { timestamp_us: ts, trackers })
        }
        INFO_PACKET => {
            let (mut system, mut names) = (String::new(), Vec::new());
            for c in chunks(root.data)? {
                match c.id {
                    INFO_SYSTEM_NAME => system = text(c.data),
                    INFO_TRACKER_LIST => {
                        for t in chunks(c.data)? {
                            for f in chunks(t.data)? {
                                if f.id == TRACKER_NAME {
                                    names.push((t.id, text(f.data)));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some(Packet::Info { system, names })
        }
        _ => None,
    }
}

/// Builders (used by tests and by the simulator in the docs/examples).
pub mod build {
    fn chunk(id: u16, nested: bool, data: &[u8]) -> Vec<u8> {
        let h = id as u32 | ((data.len() as u32) << 16) | ((nested as u32) << 31);
        let mut v = h.to_le_bytes().to_vec();
        v.extend_from_slice(data);
        v
    }

    /// A data packet with `(tracker id, [x, y, z])` entries.
    pub fn data(timestamp_us: u64, trackers: &[(u16, [f32; 3])]) -> Vec<u8> {
        let mut header = timestamp_us.to_le_bytes().to_vec();
        header.extend_from_slice(&[2, 3, 0, 1]);
        let mut list = Vec::new();
        for (id, p) in trackers {
            let mut pos = Vec::new();
            for v in p {
                pos.extend_from_slice(&v.to_le_bytes());
            }
            list.extend(chunk(*id, true, &chunk(0, false, &pos)));
        }
        let mut body = chunk(0, false, &header);
        body.extend(chunk(1, true, &list));
        chunk(0x6755, true, &body)
    }

    pub fn info(system: &str, names: &[(u16, &str)]) -> Vec<u8> {
        let mut header = 0u64.to_le_bytes().to_vec();
        header.extend_from_slice(&[2, 3, 0, 1]);
        let mut list = Vec::new();
        for (id, n) in names {
            list.extend(chunk(*id, true, &chunk(0, false, n.as_bytes())));
        }
        let mut body = chunk(0, false, &header);
        body.extend(chunk(1, false, system.as_bytes()));
        body.extend(chunk(2, true, &list));
        chunk(0x6756, true, &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_packet_round_trip() {
        let b = build::data(123_456, &[(1, [1.5, -2.0, 0.25]), (7, [10.0, 0.0, 3.0])]);
        match parse(&b).unwrap() {
            Packet::Data { timestamp_us, trackers } => {
                assert_eq!(timestamp_us, 123_456);
                assert_eq!(trackers.len(), 2);
                assert_eq!((trackers[0].id, trackers[0].pos), (1, Some([1.5, -2.0, 0.25])));
                assert_eq!((trackers[1].id, trackers[1].pos), (7, Some([10.0, 0.0, 3.0])));
            }
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn info_packet_gives_names() {
        let b = build::info("BlackTrax", &[(1, "Ana"), (2, "Luis")]);
        assert_eq!(parse(&b), Some(Packet::Info { system: "BlackTrax".into(), names: vec![(1, "Ana".into()), (2, "Luis".into())] }));
    }

    #[test]
    fn header_layout_matches_the_spec() {
        // chunk id 0x6755, 8 bytes of data, has sub-chunks: 0x6755 | 8<<16 | 1<<31 = 0x80086755 little endian
        let mut v = 0x8008_6755u32.to_le_bytes().to_vec();
        v.extend_from_slice(&[0; 8]);
        assert!(parse(&v).is_some());
        // an empty data packet: header chunk (4 + 12 bytes) + empty tracker list (4 bytes) = 20 bytes under the root
        assert_eq!(&build::data(0, &[])[..4], &[0x55, 0x67, 0x14, 0x80]);
    }

    #[test]
    fn damaged_packets_are_ignored_without_panicking() {
        let good = build::data(1, &[(1, [0.0, 0.0, 0.0])]);
        for cut in 0..good.len() {
            let _ = parse(&good[..cut]);
        }
        assert!(parse(b"").is_none());
        assert!(parse(b"garbage garbage").is_none());
        // a length field that points past the end
        let mut bad = good.clone();
        bad[2] = 0xFF;
        bad[3] = 0x7F;
        assert!(parse(&bad).is_none());
        // NaN positions are dropped, not propagated
        let nan = build::data(1, &[(3, [f32::NAN, 0.0, 0.0])]);
        match parse(&nan).unwrap() {
            Packet::Data { trackers, .. } => assert_eq!(trackers[0].pos, None),
            p => panic!("{p:?}"),
        }
    }
}
