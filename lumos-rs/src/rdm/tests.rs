use super::*;
use std::net::UdpSocket;

fn msg(pd: Vec<u8>) -> Msg {
    Msg { dest: Uid { man: 0x1234, dev: 0xAABB_CCDD }, src: OWN_UID, tn: 7, port_or_resp: 1, msg_count: 0, sub: 0, class: GET_COMMAND, pid: pid::DEVICE_INFO, pd }
}

#[test]
fn packets_round_trip_with_the_right_length_and_checksum() {
    let m = msg(vec![]);
    let b = encode(&m, true);
    assert_eq!(b.len(), 26, "a GET without data is 24 + 2 checksum bytes… plus the start code is counted in 24");
    assert_eq!(&b[..3], &[0xCC, 0x01, 24]);
    assert_eq!(&b[3..9], &[0x12, 0x34, 0xAA, 0xBB, 0xCC, 0xDD]);
    assert_eq!(b[20], GET_COMMAND);
    assert_eq!(&b[21..23], &[0x00, 0x60]);
    let sum: u16 = b[..24].iter().map(|x| *x as u16).sum();
    assert_eq!(&b[24..], &sum.to_be_bytes());
    assert_eq!(decode(&b).unwrap(), m);
    // without the start code (what ArtRdm carries) it decodes the same
    assert_eq!(decode(&encode(&m, false)).unwrap(), m);
    let with_data = msg(vec![1, 2, 3, 4, 5]);
    assert_eq!(decode(&encode(&with_data, false)).unwrap(), with_data);
    assert_eq!(encode(&with_data, true)[2], 29);
}

#[test]
fn corrupt_packets_are_rejected() {
    let mut b = encode(&msg(vec![9, 9]), true);
    let n = b.len();
    b[n - 1] ^= 1;
    assert!(decode(&b).unwrap_err().contains("comprobación"));
    assert!(decode(&b[..10]).is_err());
    let mut c = encode(&msg(vec![9, 9]), true);
    c[1] = 0x02;
    assert!(decode(&c).is_err());
    let mut d = encode(&msg(vec![9, 9]), true);
    d[23] = 7; // data length no longer matches the message length
    assert!(decode(&d).is_err());
}

#[test]
fn uid_text_and_bytes() {
    let u = Uid::parse("7ff0:0000abcd").unwrap();
    assert_eq!(u.to_string(), "7FF0:0000ABCD");
    assert_eq!(Uid::from_bytes(&u.to_bytes()), Some(u));
    assert!(Uid::parse("nope").is_none() && Uid::parse("12:zz").is_none());
    assert!(Uid::BROADCAST > u);
}

#[test]
fn art_net_wrappers_have_the_documented_layout() {
    let r = artnet::tod_request(0x0215);
    assert_eq!(&r[..8], b"Art-Net\0");
    assert_eq!(&r[8..10], &[0x00, 0x80], "OpTodRequest 0x8000 little endian");
    assert_eq!(&r[10..12], &[0, 14]);
    assert_eq!((r[21], r[22], r[23], r[24]), (2, 0, 1, 0x15), "net, TodFull, one address, sub-uni");
    let c = artnet::tod_control(0x0001, true);
    assert_eq!((&c[8..10], c[21], c[22], c[23]), (&[0x00u8, 0x82][..], 0, 1, 1), "flush");
    let rdm_bytes = encode(&msg(vec![]), false);
    let a = artnet::rdm(0x0003, &rdm_bytes);
    assert_eq!(&a[8..10], &[0x00, 0x83]);
    assert_eq!((a[12], a[23]), (1, 3));
    assert_eq!(&a[24..], &rdm_bytes[..]);
    let uids = [Uid { man: 1, dev: 2 }, Uid { man: 3, dev: 4 }];
    let t = artnet::tod_data(0x0105, &uids);
    assert_eq!(artnet::parse(&t), Some(artnet::Packet::TodData { port_address: 0x0105, uids: uids.to_vec() }));
    assert_eq!(artnet::parse(&a), Some(artnet::Packet::Rdm { port_address: 3, rdm: rdm_bytes }));
    assert_eq!(artnet::parse(b"Art-Net\0\x00\x50\x00\x0e"), None);
    assert_eq!(artnet::parse(b"xx"), None);
}

/// A simulated Art-Net node with two RDM fixtures on one port, speaking the real wire format over UDP.
struct Sim {
    sock: UdpSocket,
    devs: Vec<SimDev>,
    pa: u16,
}
struct SimDev {
    uid: Uid,
    model: &'static str,
    label: String,
    address: u16,
    personality: u8,
    identify: bool,
    /// answer GET LAMP_HOURS with NACK (unsupported) — like most LED fixtures
    drop_next: u32,
}

impl Sim {
    fn new(pa: u16) -> (Sim, u16) {
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        sock.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        let port = sock.local_addr().unwrap().port();
        let devs = vec![
            SimDev { uid: Uid { man: 0x1234, dev: 1 }, model: "LED Par 7", label: "Par 1".into(), address: 1, personality: 1, identify: false, drop_next: 0 },
            SimDev { uid: Uid { man: 0x1234, dev: 2 }, model: "LED Par 7", label: "Par 2".into(), address: 8, personality: 2, identify: false, drop_next: 0 },
        ];
        (Sim { sock, devs, pa }, port)
    }

    fn pd_for(d: &SimDev, m: &Msg) -> (u8, Vec<u8>) {
        let mut pd = Vec::new();
        match (m.class, m.pid) {
            (GET_COMMAND, pid::DEVICE_INFO) => {
                pd.extend_from_slice(&[1, 0, 0, 5, 1, 0x00, 0, 0, 0, 1]); // rdm ver, model, category 0x0100.., sw id
                pd[4] = 0x01;
                pd.extend_from_slice(&[0, 7, d.personality, 2]);
                pd.extend_from_slice(&d.address.to_be_bytes());
                pd.extend_from_slice(&[0, 0, 0]);
                (ACK, pd)
            }
            (GET_COMMAND, pid::MANUFACTURER_LABEL) => (ACK, b"Acme Lighting".to_vec()),
            (GET_COMMAND, pid::MODEL_DESCRIPTION) => (ACK, d.model.as_bytes().to_vec()),
            (GET_COMMAND, pid::DEVICE_LABEL) => (ACK, d.label.as_bytes().to_vec()),
            (GET_COMMAND, pid::SOFTWARE_VERSION) => (ACK, b"1.4".to_vec()),
            (GET_COMMAND, pid::DEVICE_HOURS) => (ACK, 1234u32.to_be_bytes().to_vec()),
            (GET_COMMAND, pid::LAMP_HOURS) => (NACK, vec![0, 1]),
            (GET_COMMAND, pid::DMX_START_ADDRESS) => (ACK, d.address.to_be_bytes().to_vec()),
            (GET_COMMAND, pid::IDENTIFY_DEVICE) => (ACK, vec![d.identify as u8]),
            _ => (NACK, vec![0, 1]),
        }
    }

    /// Serve until `n` datagrams were answered (or ~2 s pass). Returns how many ArtRdm requests were handled.
    fn serve(&mut self, n: usize) -> usize {
        let mut handled = 0;
        let t0 = Instant::now();
        let mut buf = [0u8; 600];
        while handled < n && t0.elapsed() < Duration::from_secs(2) {
            let Ok((len, from)) = self.sock.recv_from(&mut buf) else { break };
            let p = &buf[..len];
            match u16::from_le_bytes([p[8], p[9]]) {
                artnet::OP_TOD_REQUEST => {
                    let uids: Vec<Uid> = self.devs.iter().map(|d| d.uid).collect();
                    self.sock.send_to(&artnet::tod_data(self.pa, &uids), from).unwrap();
                }
                artnet::OP_RDM => {
                    let Ok(m) = decode(&p[24..]) else { continue };
                    let Some(i) = self.devs.iter().position(|d| d.uid == m.dest) else { continue };
                    if self.devs[i].drop_next > 0 {
                        self.devs[i].drop_next -= 1; // a lost datagram: no answer
                        continue;
                    }
                    let d = &mut self.devs[i];
                    let (resp, pd) = if m.class == SET_COMMAND {
                        match m.pid {
                            pid::DMX_START_ADDRESS if m.pd.len() == 2 => {
                                d.address = u16::from_be_bytes([m.pd[0], m.pd[1]]);
                                (ACK, vec![])
                            }
                            pid::IDENTIFY_DEVICE if m.pd.len() == 1 => {
                                d.identify = m.pd[0] != 0;
                                (ACK, vec![])
                            }
                            pid::DEVICE_LABEL => {
                                d.label = String::from_utf8_lossy(&m.pd).to_string();
                                (ACK, vec![])
                            }
                            pid::DMX_PERSONALITY if m.pd.len() == 1 && (1..=2).contains(&m.pd[0]) => {
                                d.personality = m.pd[0];
                                (ACK, vec![])
                            }
                            _ => (NACK, vec![0, 6]),
                        }
                    } else {
                        Self::pd_for(d, &m)
                    };
                    let reply = Msg { dest: m.src, src: m.dest, tn: m.tn, port_or_resp: resp, msg_count: 0, sub: 0, class: m.class + 1, pid: m.pid, pd };
                    self.sock.send_to(&artnet::rdm(self.pa, &encode(&reply, false)), from).unwrap();
                    handled += 1;
                }
                _ => {}
            }
        }
        handled
    }
}

/// Run `ctl` against the simulator until it has nothing left to do.
fn pump(ctl: &mut Controller, sim: &mut Sim, from_port: &UdpSocket) {
    let t0 = Instant::now();
    from_port.set_read_timeout(Some(Duration::from_millis(30))).unwrap();
    let mut buf = [0u8; 700];
    while t0.elapsed() < Duration::from_secs(5) {
        for o in ctl.tick() {
            from_port.send_to(&o.data, o.to).unwrap();
        }
        // the node answers whatever is waiting
        sim.sock.set_nonblocking(true).unwrap();
        sim.serve(8);
        sim.sock.set_nonblocking(false).unwrap();
        while let Ok((n, from)) = from_port.recv_from(&mut buf) {
            ctl.on_packet(from, &buf[..n]);
        }
        if !ctl.busy() {
            break;
        }
    }
}

fn setup() -> (Controller, Sim, UdpSocket) {
    let (sim, port) = Sim::new(0x0001);
    let mut ctl = Controller::new(port);
    let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
    let outs = ctl.scan(&[(Ipv4Addr::LOCALHOST, vec![0x0001])], false);
    assert_eq!(outs.len(), 1);
    sock.send_to(&outs[0].data, outs[0].to).unwrap();
    (ctl, sim, sock)
}

#[test]
fn scan_finds_devices_and_reads_everything_they_report() {
    let (mut ctl, mut sim, sock) = setup();
    pump(&mut ctl, &mut sim, &sock);
    let d = ctl.devices();
    assert_eq!(d.len(), 2, "{d:?}");
    let p1 = &d[0];
    assert_eq!((p1.uid.as_str(), p1.manufacturer.as_str(), p1.model.as_str(), p1.label.as_str()), ("1234:00000001", "Acme Lighting", "LED Par 7", "Par 1"));
    assert_eq!((p1.address, p1.footprint, p1.personality, p1.personalities, p1.software.as_str()), (1, 7, 1, 2, "1.4"));
    assert_eq!(p1.device_hours, Some(1234));
    assert_eq!(p1.lamp_hours, None, "an unsupported optional parameter is not an error");
    assert!(p1.error.is_empty() && p1.status == RdmStatus::Ok, "{p1:?}");
    assert_eq!(d[1].address, 8);
    assert_eq!(p1.port_address, 1);
    assert!(!ctl.busy());
}

#[test]
fn identify_address_label_and_personality_are_applied_and_read_back() {
    let (mut ctl, mut sim, sock) = setup();
    pump(&mut ctl, &mut sim, &sock);
    let u1 = Uid { man: 0x1234, dev: 1 };
    ctl.identify(u1, true).unwrap();
    ctl.set_address(u1, 100).unwrap();
    ctl.set_label(u1, "Frente izq.").unwrap();
    ctl.set_personality(u1, 2).unwrap();
    pump(&mut ctl, &mut sim, &sock);
    assert_eq!((sim.devs[0].identify, sim.devs[0].address, sim.devs[0].label.as_str(), sim.devs[0].personality), (true, 100, "Frente izq.", 2));
    let d = ctl.devices().into_iter().find(|d| d.uid == u1.to_string()).unwrap();
    assert_eq!((d.address, d.label.as_str(), d.personality, d.identify), (100, "Frente izq.", 2, true), "read back from the device, not assumed");
    ctl.identify(u1, false).unwrap();
    pump(&mut ctl, &mut sim, &sock);
    let ci = ctl.devices().iter().find(|d| d.uid == u1.to_string()).unwrap().identify;
    assert!(!sim.devs[0].identify && !ci, "sim {} ctl {} err {:?}", sim.devs[0].identify, ci, ctl.devices());
    // validation
    assert!(ctl.set_address(u1, 0).unwrap_err().contains("1 y 512") && ctl.set_address(u1, 513).is_err());
    assert!(ctl.set_personality(u1, 9).unwrap_err().contains("personalidades"));
    assert!(ctl.identify(Uid { man: 9, dev: 9 }, true).unwrap_err().contains("escanea"));
}

#[test]
fn a_rejected_set_is_reported_and_a_silent_device_is_marked_lost_after_retries() {
    let (mut ctl, mut sim, sock) = setup();
    pump(&mut ctl, &mut sim, &sock);
    let u2 = Uid { man: 0x1234, dev: 2 };
    ctl.request(u2, SET_COMMAND, pid::MANUFACTURER_LABEL, b"x".to_vec()).unwrap(); // read-only for the sim → NACK 6
    pump(&mut ctl, &mut sim, &sock);
    let d = ctl.devices().into_iter().find(|d| d.uid == u2.to_string()).unwrap();
    assert!(d.error.contains("solo lectura"), "{}", d.error);
    // now the device stops answering: three tries, then Lost; the other device keeps working
    sim.devs[1].drop_next = 99;
    ctl.refresh(u2);
    pump(&mut ctl, &mut sim, &sock);
    let d = ctl.devices().into_iter().find(|d| d.uid == u2.to_string()).unwrap();
    assert_eq!(d.status, RdmStatus::Lost, "{d:?}");
    assert!(d.error.contains("Sin respuesta"));
    let ok = ctl.devices().into_iter().find(|d| d.uid != u2.to_string()).unwrap();
    assert_eq!(ok.status, RdmStatus::Ok);
}

#[test]
fn a_dropped_datagram_is_retried_and_stray_packets_are_ignored() {
    let (mut ctl, mut sim, sock) = setup();
    sim.devs[0].drop_next = 1; // the first question is lost
    pump(&mut ctl, &mut sim, &sock);
    assert_eq!(ctl.devices()[0].model, "LED Par 7", "the retry got through");
    // a response from nobody we asked, and garbage, change nothing
    let before = ctl.devices();
    let stray = Msg { dest: OWN_UID, src: Uid { man: 0x1234, dev: 1 }, tn: 250, port_or_resp: ACK, msg_count: 0, sub: 0, class: GET_RESPONSE, pid: pid::DEVICE_LABEL, pd: b"HACK".to_vec() };
    ctl.on_packet("127.0.0.1:6454".parse().unwrap(), &artnet::rdm(1, &encode(&stray, false)));
    ctl.on_packet("127.0.0.1:6454".parse().unwrap(), b"garbage");
    assert_eq!(ctl.devices()[0].label, before[0].label);
}

#[test]
fn flush_scan_sends_a_flush_before_the_request() {
    let mut ctl = Controller::new(6454);
    let outs = ctl.scan(&[(Ipv4Addr::new(10, 0, 0, 5), vec![0x0001, 0x0102])], true);
    assert_eq!(outs.len(), 4);
    assert_eq!(&outs[0].data[8..10], &[0x00, 0x82]);
    assert_eq!(&outs[1].data[8..10], &[0x00, 0x80]);
    assert_eq!(outs[2].data[21], 1, "net of the second port-address");
    assert_eq!(outs[0].to, "10.0.0.5:6454".parse::<SocketAddr>().unwrap());
}
