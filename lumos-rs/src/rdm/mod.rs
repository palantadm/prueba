//! RDM (ANSI E1.20, Remote Device Management) carried over Art-Net: find the fixtures on the DMX lines, read their
//! model, label, DMX address and personality, and change address / label / personality or make one flash to be found.
//!
//! * [`Uid`], [`Msg`], [`encode`] / [`decode`] – the RDM packet (checksum included).
//! * [`artnet`] – the Art-Net wrappers: ArtTodRequest / ArtTodControl / ArtTodData / ArtRdm.
//! * [`Controller`] – the state machine: scan nodes, learn devices, run one request at a time with retries.
//!   It does no I/O: it returns datagrams to send and is fed what arrives, so it is fully testable.

pub mod artnet;

use crate::model::{RdmDevice, RdmStatus};
use std::collections::{BTreeMap, VecDeque};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::{Duration, Instant};

pub const START_CODE: u8 = 0xCC;
pub const SUB_START: u8 = 0x01;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Uid {
    pub man: u16,
    pub dev: u32,
}

impl Uid {
    pub const BROADCAST: Uid = Uid { man: 0xFFFF, dev: 0xFFFF_FFFF };
    pub fn to_bytes(self) -> [u8; 6] {
        let m = self.man.to_be_bytes();
        let d = self.dev.to_be_bytes();
        [m[0], m[1], d[0], d[1], d[2], d[3]]
    }
    pub fn from_bytes(b: &[u8]) -> Option<Uid> {
        (b.len() >= 6).then(|| Uid { man: u16::from_be_bytes([b[0], b[1]]), dev: u32::from_be_bytes([b[2], b[3], b[4], b[5]]) })
    }
    pub fn parse(s: &str) -> Option<Uid> {
        let (m, d) = s.trim().split_once(':')?;
        Some(Uid { man: u16::from_str_radix(m, 16).ok()?, dev: u32::from_str_radix(d, 16).ok()? })
    }
}

impl std::fmt::Display for Uid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04X}:{:08X}", self.man, self.dev)
    }
}

/// Command classes.
pub const DISC_COMMAND: u8 = 0x10;
pub const GET_COMMAND: u8 = 0x20;
pub const GET_RESPONSE: u8 = 0x21;
pub const SET_COMMAND: u8 = 0x30;
pub const SET_RESPONSE: u8 = 0x31;
/// Response types.
pub const ACK: u8 = 0x00;
pub const ACK_TIMER: u8 = 0x01;
pub const NACK: u8 = 0x02;
pub const ACK_OVERFLOW: u8 = 0x03;
/// Parameter ids.
pub mod pid {
    pub const DISC_UNIQUE_BRANCH: u16 = 0x0001;
    pub const DISC_MUTE: u16 = 0x0002;
    pub const SUPPORTED_PARAMETERS: u16 = 0x0050;
    pub const DEVICE_INFO: u16 = 0x0060;
    pub const MODEL_DESCRIPTION: u16 = 0x0080;
    pub const MANUFACTURER_LABEL: u16 = 0x0081;
    pub const DEVICE_LABEL: u16 = 0x0082;
    pub const SOFTWARE_VERSION: u16 = 0x00C0;
    pub const DMX_PERSONALITY: u16 = 0x00E0;
    pub const DMX_START_ADDRESS: u16 = 0x00F0;
    pub const DEVICE_HOURS: u16 = 0x0400;
    pub const LAMP_HOURS: u16 = 0x0401;
    pub const IDENTIFY_DEVICE: u16 = 0x1000;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msg {
    pub dest: Uid,
    pub src: Uid,
    pub tn: u8,
    /// Controller: port id. Responder: response type.
    pub port_or_resp: u8,
    pub msg_count: u8,
    pub sub: u16,
    pub class: u8,
    pub pid: u16,
    pub pd: Vec<u8>,
}

/// RDM packet bytes. `start_code`: include the DMX start code 0xCC (serial transports) or not (Art-Net's ArtRdm).
pub fn encode(m: &Msg, start_code: bool) -> Vec<u8> {
    let mut b = Vec::with_capacity(26 + m.pd.len());
    b.push(START_CODE);
    b.push(SUB_START);
    b.push((24 + m.pd.len()) as u8); // message length: start code … parameter data
    b.extend_from_slice(&m.dest.to_bytes());
    b.extend_from_slice(&m.src.to_bytes());
    b.push(m.tn);
    b.push(m.port_or_resp);
    b.push(m.msg_count);
    b.extend_from_slice(&m.sub.to_be_bytes());
    b.push(m.class);
    b.extend_from_slice(&m.pid.to_be_bytes());
    b.push(m.pd.len() as u8);
    b.extend_from_slice(&m.pd);
    let sum = b.iter().fold(0u16, |a, x| a.wrapping_add(*x as u16));
    b.extend_from_slice(&sum.to_be_bytes());
    if !start_code {
        b.remove(0);
    }
    b
}

pub fn decode(data: &[u8]) -> Result<Msg, String> {
    let mut v = data.to_vec();
    if v.first() != Some(&START_CODE) {
        v.insert(0, START_CODE);
    }
    if v.len() < 26 || v[1] != SUB_START {
        return Err("Paquete RDM demasiado corto o sin sub-start code".into());
    }
    let len = v[2] as usize;
    if len < 24 || v.len() < len + 2 {
        return Err("Longitud RDM no válida".into());
    }
    let sum = v[..len].iter().fold(0u16, |a, x| a.wrapping_add(*x as u16));
    if sum != u16::from_be_bytes([v[len], v[len + 1]]) {
        return Err("Suma de comprobación RDM incorrecta".into());
    }
    let pdl = v[23] as usize;
    if 24 + pdl != len {
        return Err("Longitud de datos RDM incoherente".into());
    }
    Ok(Msg {
        dest: Uid::from_bytes(&v[3..9]).unwrap(),
        src: Uid::from_bytes(&v[9..15]).unwrap(),
        tn: v[15],
        port_or_resp: v[16],
        msg_count: v[17],
        sub: u16::from_be_bytes([v[18], v[19]]),
        class: v[20],
        pid: u16::from_be_bytes([v[21], v[22]]),
        pd: v[24..24 + pdl].to_vec(),
    })
}

/// A datagram to send.
#[derive(Clone, Debug, PartialEq)]
pub struct Out {
    pub to: SocketAddr,
    pub data: Vec<u8>,
}

fn nack_text(code: u16) -> &'static str {
    match code {
        0 => "formato de paquete no válido",
        1 => "el aparato no admite esa función",
        2 => "fuera de rango",
        3 => "buffer lleno",
        4 => "tamaño no admitido",
        5 => "datos fuera de rango",
        6 => "solo lectura",
        7 => "no se puede cambiar ahora",
        _ => "rechazado por el aparato",
    }
}

fn text(pd: &[u8]) -> String {
    String::from_utf8_lossy(pd).trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string()
}

struct Dev {
    d: RdmDevice,
    node: SocketAddrV4,
    pa: u16,
    seen: Instant,
}

#[derive(Clone)]
struct Job {
    uid: Uid,
    class: u8,
    pid: u16,
    pd: Vec<u8>,
}

struct Inflight {
    job: Job,
    tn: u8,
    at: Instant,
    tries: u8,
    delay_until: Option<Instant>,
}

const TIMEOUT: Duration = Duration::from_millis(700);
const MAX_TRIES: u8 = 3;
/// The controller's own UID (prototyping manufacturer range 0x7FF0).
pub const OWN_UID: Uid = Uid { man: 0x7FF0, dev: 0x4C55_4D4F };

pub struct Controller {
    pub art_port: u16,
    devices: BTreeMap<Uid, Dev>,
    queue: VecDeque<Job>,
    inflight: Option<Inflight>,
    tn: u8,
    pub busy_until: Option<Instant>,
    /// Last error to show (cleared on a new scan).
    pub error: String,
}

/// Facts a device reports, in the order they are asked.
const INFO_PIDS: &[u16] = &[pid::DEVICE_INFO, pid::MANUFACTURER_LABEL, pid::MODEL_DESCRIPTION, pid::DEVICE_LABEL, pid::SOFTWARE_VERSION, pid::DEVICE_HOURS, pid::LAMP_HOURS];

impl Controller {
    pub fn new(art_port: u16) -> Self {
        Controller { art_port, devices: BTreeMap::new(), queue: VecDeque::new(), inflight: None, tn: 0, busy_until: None, error: String::new() }
    }

    pub fn busy(&self) -> bool {
        self.inflight.is_some() || !self.queue.is_empty()
    }

    /// Ask every node for its table of devices (ArtTodRequest, full). `nodes`: (ip, output port-addresses).
    pub fn scan(&mut self, nodes: &[(Ipv4Addr, Vec<u16>)], flush: bool) -> Vec<Out> {
        self.error.clear();
        let mut out = Vec::new();
        for (ip, pas) in nodes {
            let to = SocketAddr::V4(SocketAddrV4::new(*ip, self.art_port));
            for pa in pas {
                if flush {
                    out.push(Out { to, data: artnet::tod_control(*pa, true) });
                }
                out.push(Out { to, data: artnet::tod_request(*pa) });
            }
        }
        out
    }

    pub fn devices(&self) -> Vec<RdmDevice> {
        let now = Instant::now();
        let mut v: Vec<RdmDevice> = self
            .devices
            .values()
            .map(|x| {
                let mut d = x.d.clone();
                d.seen_s = now.saturating_duration_since(x.seen).as_secs().min(u32::MAX as u64) as u32;
                d
            })
            .collect();
        v.sort_by(|a, b| (a.port_address, a.address, &a.uid).cmp(&(b.port_address, b.address, &b.uid)));
        v
    }

    fn blank(uid: Uid, node: SocketAddrV4, pa: u16) -> Dev {
        Dev {
            d: RdmDevice {
                uid: uid.to_string(),
                node: node.ip().to_string(),
                port_address: pa,
                manufacturer: String::new(),
                model: String::new(),
                label: String::new(),
                software: String::new(),
                footprint: 0,
                address: 0,
                personality: 0,
                personalities: 0,
                category: 0,
                identify: false,
                device_hours: None,
                lamp_hours: None,
                status: RdmStatus::Loading,
                error: String::new(),
                seen_s: 0,
                fixture: None,
            },
            node,
            pa,
            seen: Instant::now(),
        }
    }

    fn next_tn(&mut self) -> u8 {
        self.tn = self.tn.wrapping_add(1);
        self.tn
    }

    /// Queue the questions that describe a device.
    pub fn refresh(&mut self, uid: Uid) {
        for p in INFO_PIDS {
            self.queue.push_back(Job { uid, class: GET_COMMAND, pid: *p, pd: vec![] });
        }
        self.queue.push_back(Job { uid, class: GET_COMMAND, pid: pid::DMX_START_ADDRESS, pd: vec![] });
        self.queue.push_back(Job { uid, class: GET_COMMAND, pid: pid::IDENTIFY_DEVICE, pd: vec![] });
        if let Some(d) = self.devices.get_mut(&uid) {
            d.d.status = RdmStatus::Loading;
        }
    }

    /// A SET (or any request) on one device. The reply refreshes what changed.
    pub fn request(&mut self, uid: Uid, class: u8, pid: u16, pd: Vec<u8>) -> Result<(), String> {
        if !self.devices.contains_key(&uid) {
            return Err(format!("No hay ningún dispositivo RDM {uid}: escanea primero"));
        }
        self.queue.push_back(Job { uid, class, pid, pd });
        Ok(())
    }

    pub fn identify(&mut self, uid: Uid, on: bool) -> Result<(), String> {
        self.request(uid, SET_COMMAND, pid::IDENTIFY_DEVICE, vec![on as u8])
    }

    pub fn set_address(&mut self, uid: Uid, address: u16) -> Result<(), String> {
        if !(1..=512).contains(&address) {
            return Err("La dirección DMX debe estar entre 1 y 512".into());
        }
        self.request(uid, SET_COMMAND, pid::DMX_START_ADDRESS, address.to_be_bytes().to_vec())?;
        self.queue.push_back(Job { uid, class: GET_COMMAND, pid: pid::DMX_START_ADDRESS, pd: vec![] });
        Ok(())
    }

    pub fn set_label(&mut self, uid: Uid, label: &str) -> Result<(), String> {
        let l: String = label.chars().filter(|c| c.is_ascii() && !c.is_control()).take(32).collect();
        self.request(uid, SET_COMMAND, pid::DEVICE_LABEL, l.into_bytes())?;
        self.queue.push_back(Job { uid, class: GET_COMMAND, pid: pid::DEVICE_LABEL, pd: vec![] });
        Ok(())
    }

    pub fn set_personality(&mut self, uid: Uid, n: u8) -> Result<(), String> {
        let d = self.devices.get(&uid).ok_or_else(|| format!("No hay ningún dispositivo RDM {uid}"))?;
        if n < 1 || (d.d.personalities > 0 && n > d.d.personalities) {
            return Err(format!("El aparato tiene {} personalidades (1–{})", d.d.personalities, d.d.personalities));
        }
        self.request(uid, SET_COMMAND, pid::DMX_PERSONALITY, vec![n])?;
        self.refresh(uid);
        Ok(())
    }

    fn send_job(&mut self, job: Job, tries: u8) -> Option<Out> {
        let dev = self.devices.get(&job.uid)?;
        let (node, pa) = (dev.node, dev.pa);
        let tn = self.next_tn();
        let msg = Msg { dest: job.uid, src: OWN_UID, tn, port_or_resp: 1, msg_count: 0, sub: 0, class: job.class, pid: job.pid, pd: job.pd.clone() };
        self.inflight = Some(Inflight { job, tn, at: Instant::now(), tries, delay_until: None });
        Some(Out { to: SocketAddr::V4(SocketAddrV4::new(*node.ip(), self.art_port)), data: artnet::rdm(pa, &encode(&msg, false)) })
    }

    /// Time-driven work: start the next request, retry a lost one, give up after [`MAX_TRIES`].
    pub fn tick(&mut self) -> Vec<Out> {
        let mut out = Vec::new();
        if let Some(f) = &self.inflight {
            if f.delay_until.map_or(false, |t| Instant::now() < t) {
                return out;
            }
            if f.at.elapsed() >= TIMEOUT || f.delay_until.is_some() {
                let (job, tries) = (f.job.clone(), f.tries);
                self.inflight = None;
                if tries + 1 < MAX_TRIES {
                    out.extend(self.send_job(job, tries + 1));
                } else if let Some(d) = self.devices.get_mut(&job.uid) {
                    d.d.status = RdmStatus::Lost;
                    d.d.error = format!("Sin respuesta ({})", pid_name(job.pid));
                    // do not keep hammering a device that is gone
                    self.queue.retain(|j| j.uid != job.uid);
                }
            }
            return out;
        }
        while let Some(job) = self.queue.pop_front() {
            if let Some(o) = self.send_job(job, 0) {
                out.push(o);
                break;
            }
        }
        out
    }

    /// Feed a datagram received from the network (any Art-Net packet; others are ignored).
    pub fn on_packet(&mut self, from: SocketAddr, data: &[u8]) {
        match artnet::parse(data) {
            Some(artnet::Packet::TodData { port_address, uids }) => {
                let SocketAddr::V4(node) = from else { return };
                for u in uids {
                    let known = self.devices.contains_key(&u);
                    let e = self.devices.entry(u).or_insert_with(|| Self::blank(u, SocketAddrV4::new(*node.ip(), self.art_port), port_address));
                    e.seen = Instant::now();
                    e.node = SocketAddrV4::new(*node.ip(), self.art_port);
                    e.pa = port_address;
                    e.d.node = node.ip().to_string();
                    e.d.port_address = port_address;
                    if !known {
                        self.refresh(u);
                    } else if e.d.status == RdmStatus::Lost {
                        e.d.status = RdmStatus::Ok;
                    }
                }
            }
            Some(artnet::Packet::Rdm { rdm, .. }) => {
                if let Ok(m) = decode(&rdm) {
                    self.on_response(m);
                }
            }
            None => {}
        }
    }

    fn on_response(&mut self, m: Msg) {
        let Some(f) = &self.inflight else { return };
        if m.src != f.job.uid || m.tn != f.tn || !matches!(m.class, GET_RESPONSE | SET_RESPONSE) {
            return;
        }
        let job = f.job.clone();
        match m.port_or_resp {
            ACK_TIMER => {
                // the device needs a moment: wait the advertised time (units of 100 ms) and ask again
                let wait = if m.pd.len() >= 2 { u16::from_be_bytes([m.pd[0], m.pd[1]]) as u64 * 100 } else { 500 };
                if let Some(f) = self.inflight.as_mut() {
                    f.delay_until = Some(Instant::now() + Duration::from_millis(wait.clamp(100, 5000)));
                }
                return;
            }
            NACK => {
                let code = if m.pd.len() >= 2 { u16::from_be_bytes([m.pd[0], m.pd[1]]) } else { 0 };
                self.inflight = None;
                if let Some(d) = self.devices.get_mut(&job.uid) {
                    // optional facts (lamp hours, device hours…) are simply not supported: not an error
                    if job.class == SET_COMMAND {
                        d.d.error = format!("{}: {}", pid_name(job.pid), nack_text(code));
                    } else if !matches!(job.pid, pid::DEVICE_HOURS | pid::LAMP_HOURS) {
                        d.d.error = format!("{}: {}", pid_name(job.pid), nack_text(code));
                    }
                    d.d.status = RdmStatus::Ok;
                }
                return;
            }
            _ => {}
        }
        self.inflight = None;
        let Some(d) = self.devices.get_mut(&job.uid) else { return };
        d.seen = Instant::now();
        d.d.status = RdmStatus::Ok;
        if job.class == SET_COMMAND {
            d.d.error.clear();
            if job.pid == pid::IDENTIFY_DEVICE {
                d.d.identify = job.pd.first().map_or(false, |b| *b != 0);
            }
            return;
        }
        let pd = &m.pd;
        match job.pid {
            pid::DEVICE_INFO if pd.len() >= 19 => {
                d.d.category = u16::from_be_bytes([pd[4], pd[5]]);
                d.d.footprint = u16::from_be_bytes([pd[10], pd[11]]);
                d.d.personality = pd[12];
                d.d.personalities = pd[13];
                let a = u16::from_be_bytes([pd[14], pd[15]]);
                d.d.address = if a == 0xFFFF { 0 } else { a };
            }
            pid::MANUFACTURER_LABEL => d.d.manufacturer = text(pd),
            pid::MODEL_DESCRIPTION => d.d.model = text(pd),
            pid::DEVICE_LABEL => d.d.label = text(pd),
            pid::SOFTWARE_VERSION => d.d.software = text(pd),
            pid::DMX_START_ADDRESS if pd.len() >= 2 => d.d.address = u16::from_be_bytes([pd[0], pd[1]]),
            pid::IDENTIFY_DEVICE if !pd.is_empty() => d.d.identify = pd[0] != 0,
            pid::DEVICE_HOURS if pd.len() >= 4 => d.d.device_hours = Some(u32::from_be_bytes([pd[0], pd[1], pd[2], pd[3]])),
            pid::LAMP_HOURS if pd.len() >= 4 => d.d.lamp_hours = Some(u32::from_be_bytes([pd[0], pd[1], pd[2], pd[3]])),
            _ => {}
        }
    }

    /// Forget everything (a new show or a manual reset).
    pub fn clear(&mut self) {
        self.devices.clear();
        self.queue.clear();
        self.inflight = None;
    }
}

pub fn pid_name(p: u16) -> &'static str {
    match p {
        pid::DEVICE_INFO => "información del aparato",
        pid::MANUFACTURER_LABEL => "fabricante",
        pid::MODEL_DESCRIPTION => "modelo",
        pid::DEVICE_LABEL => "etiqueta",
        pid::SOFTWARE_VERSION => "versión de software",
        pid::DMX_START_ADDRESS => "dirección DMX",
        pid::DMX_PERSONALITY => "personalidad",
        pid::IDENTIFY_DEVICE => "identificar",
        pid::DEVICE_HOURS => "horas del aparato",
        pid::LAMP_HOURS => "horas de lámpara",
        _ => "parámetro",
    }
}

#[cfg(test)]
mod tests;
