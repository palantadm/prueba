use std::net::UdpSocket;
use std::time::{Duration, Instant};

use rosc::{OscMessage, OscPacket, OscType};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use super::artnet::{build_artdmx, build_poll_reply, parse_artdmx, port_address, ArtNetSink};
use super::sacn::{build_packet, cid, multicast_addr, SacnSink};
use super::serial::build_enttec_pro;
use super::*;

fn rx_sock() -> (UdpSocket, u16) {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(Duration::from_millis(1000))).unwrap();
    let p = s.local_addr().unwrap().port();
    (s, p)
}

fn free_port() -> u16 {
    UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port()
}

fn frame(f: impl Fn(usize) -> u8) -> [u8; 512] {
    let mut d = [0u8; 512];
    for (i, v) in d.iter_mut().enumerate() {
        *v = f(i);
    }
    d
}

fn recv_event(rx: &mut UnboundedReceiver<IoEvent>, ms: u64) -> Option<IoEvent> {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        if let Ok(e) = rx.try_recv() {
            return Some(e);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}

fn no_io_settings() -> Settings {
    let mut s = Settings::default();
    s.osc_in.enabled = false;
    s
}

// ── Art-Net ──

#[test]
fn artnet_port_address_layout() {
    assert_eq!(port_address(1, 2, 3), 0x0123);
    assert_eq!(port_address(0x7F, 0xF, 0xF), 0x7FFF);
}

#[test]
fn artnet_packet_exact_bytes() {
    let d = frame(|i| (i % 251) as u8);
    let p = build_artdmx(9, port_address(1, 2, 3), &d);
    assert_eq!(p.len(), 530);
    assert_eq!(&p[..8], b"Art-Net\0");
    assert_eq!(&p[8..10], &[0x00, 0x50]); // OpDmx, little endian
    assert_eq!(&p[10..12], &[0, 14]); // ProtVer
    assert_eq!(p[12], 9); // sequence
    assert_eq!(p[13], 0); // physical
    assert_eq!(p[14], 0x23); // SubUni (subnet 2, universe 3)
    assert_eq!(p[15], 0x01); // Net
    assert_eq!(&p[16..18], &[0x02, 0x00]); // length 512 BE
    assert_eq!(&p[18..], &d[..]);
    let (pa, data) = parse_artdmx(&p).unwrap();
    assert_eq!(pa, 0x0123);
    assert_eq!(data, &d[..]);
}

#[test]
fn artnet_loopback_and_sequence() {
    let (rx, port) = rx_sock();
    let mut sink = ArtNetSink::new("127.0.0.1", port, 0, 1, 2).unwrap();
    let d = frame(|i| i as u8);
    sink.send(1, &d);
    sink.send(1, &d);
    let mut buf = [0u8; 1024];
    let (n, _) = rx.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..n], &build_artdmx(1, port_address(0, 1, 2), &d)[..]);
    let (n, _) = rx.recv_from(&mut buf).unwrap();
    assert_eq!(buf[12], 2);
    assert_eq!(n, 530);
    let st = sink.stat();
    assert_eq!(st.state, OutState::Ok);
    assert_eq!(st.frames, 2);
    assert!(st.message.contains("127.0.0.1"));
}

#[test]
fn artnet_sequence_wraps_to_one() {
    let mut sink = ArtNetSink::new("127.0.0.1", free_port(), 0, 0, 0).unwrap();
    sink.seq = 254;
    let d = [0u8; 512];
    sink.send(1, &d);
    assert_eq!(sink.seq, 255);
    sink.send(1, &d);
    assert_eq!(sink.seq, 1);
}

#[test]
fn artnet_poll_reply_layout() {
    let r = build_poll_reply("10.0.0.5".parse().unwrap(), "Lumos", &[0x0102]);
    assert_eq!(r.len(), 239);
    assert_eq!(&r[..8], b"Art-Net\0");
    assert_eq!(&r[8..10], &[0x00, 0x21]);
    assert_eq!(&r[10..14], &[10, 0, 0, 5]);
    assert_eq!(&r[14..16], &[0x36, 0x19]);
    assert_eq!(r[18], 0x01); // net
    assert_eq!(r[19], 0x00); // subnet
    assert_eq!(&r[26..31], b"Lumos");
    assert_eq!(r[173], 1); // one port
    assert_eq!(r[174], 0x80);
    assert_eq!(r[190], 0x02);
}

// ── sACN ──

#[test]
fn sacn_packet_layout() {
    let c = [0xAB; 16];
    let d = frame(|i| (255 - i % 255) as u8);
    let p = build_packet(&c, "Lumos", 100, 7, 0x0102, &d);
    assert_eq!(p.len(), 638);
    assert_eq!(&p[0..2], &[0x00, 0x10]);
    assert_eq!(&p[2..4], &[0, 0]);
    assert_eq!(&p[4..16], b"ASC-E1.17\0\0\0");
    assert_eq!(&p[16..18], &[0x72, 0x6E]); // 0x7000 | 622
    assert_eq!(&p[18..22], &[0, 0, 0, 4]);
    assert_eq!(&p[22..38], &c);
    assert_eq!(&p[38..40], &[0x72, 0x58]); // 0x7000 | 600
    assert_eq!(&p[40..44], &[0, 0, 0, 2]);
    assert_eq!(&p[44..49], b"Lumos");
    assert_eq!(p[49], 0);
    assert_eq!(p[108], 100);
    assert_eq!(&p[109..111], &[0, 0]);
    assert_eq!(p[111], 7);
    assert_eq!(p[112], 0);
    assert_eq!(&p[113..115], &[0x01, 0x02]);
    assert_eq!(&p[115..117], &[0x72, 0x0B]); // 0x7000 | 523
    assert_eq!(p[117], 0x02);
    assert_eq!(p[118], 0xA1);
    assert_eq!(&p[119..121], &[0, 0]);
    assert_eq!(&p[121..123], &[0, 1]);
    assert_eq!(&p[123..125], &[0x02, 0x01]); // 513
    assert_eq!(p[125], 0); // start code
    assert_eq!(&p[126..], &d[..]);
}

#[test]
fn sacn_multicast_address_and_cid_stable() {
    assert_eq!(multicast_addr(0x0102).octets(), [239, 255, 1, 2]);
    assert_eq!(cid(), cid());
}

#[test]
fn sacn_unicast_loopback() {
    let (rx, port) = rx_sock();
    let mut sink = SacnSink::new(5, 150, Some("127.0.0.1"), "Test", port).unwrap();
    let d = frame(|i| (i * 3) as u8);
    sink.send(5, &d);
    let mut buf = [0u8; 1024];
    let (n, _) = rx.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..n], &build_packet(&cid(), "Test", 150, 1, 5, &d)[..]);
    assert_eq!(sink.stat().frames, 1);
}

#[test]
fn sacn_bad_unicast_is_error() {
    assert!(SacnSink::new(1, 100, Some("not-an-ip"), "x", 5568).is_err());
}

// ── OSC out ──

fn recv_osc(rx: &UdpSocket) -> Option<OscMessage> {
    let mut buf = [0u8; 2048];
    let (n, _) = rx.recv_from(&mut buf).ok()?;
    match rosc::decoder::decode_udp(&buf[..n]).ok()?.1 {
        OscPacket::Message(m) => Some(m),
        _ => None,
    }
}

#[test]
fn osc_out_per_channel_only_changes() {
    let (rx, port) = rx_sock();
    rx.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    let mut sink =
        oscout::OscSink::new("127.0.0.1", port, "/dmx/{universe}/{channel}", OscMode::PerChannel, true).unwrap();
    let mut d = [0u8; 512];
    d[0] = 255;
    d[9] = 51;
    sink.send(3, &d);
    let m = recv_osc(&rx).unwrap();
    assert_eq!(m.addr, "/dmx/3/1");
    assert_eq!(m.args, vec![OscType::Float(1.0)]);
    let m = recv_osc(&rx).unwrap();
    assert_eq!(m.addr, "/dmx/3/10");
    assert_eq!(m.args, vec![OscType::Float(0.2)]);
    // unchanged → nothing
    sink.send(3, &d);
    assert!(recv_osc(&rx).is_none());
    // channel 1 returns to 0 → sent exactly once
    d[0] = 0;
    sink.send(3, &d);
    let m = recv_osc(&rx).unwrap();
    assert_eq!((m.addr.as_str(), &m.args[..]), ("/dmx/3/1", &[OscType::Float(0.0)][..]));
    assert!(recv_osc(&rx).is_none());
}

#[test]
fn osc_out_blob_mode() {
    let (rx, port) = rx_sock();
    let mut sink = oscout::OscSink::new("127.0.0.1", port, "", OscMode::Blob, true).unwrap();
    let d = frame(|i| (i % 7) as u8);
    sink.send(2, &d);
    // rosc's decoder mis-handles 4-byte-aligned blobs, so check the wire bytes directly.
    let mut buf = [0u8; 2048];
    let (n, _) = rx.recv_from(&mut buf).unwrap();
    assert_eq!(n, 8 + 4 + 4 + 512);
    assert_eq!(&buf[..8], b"/dmx/2\0\0");
    assert_eq!(&buf[8..12], b",b\0\0");
    assert_eq!(&buf[12..16], &512u32.to_be_bytes());
    assert_eq!(&buf[16..n], &d[..]);
}

// ── OSC in (via hub) ──

#[test]
fn osc_in_roundtrip_and_bundle() {
    let (tx, mut rx) = unbounded_channel();
    let mut hub = IoHub::new(tx);
    let port = free_port();
    let mut s = Settings::default();
    s.osc_in.enabled = true;
    s.osc_in.port = port;
    hub.configure(&[], &s);
    let (_, ins) = hub.status();
    assert!(ins.osc_listening, "{}", ins.osc_error);
    assert_eq!(ins.osc_port, port);

    let tx_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
    let send = |pkt: OscPacket| {
        tx_sock
            .send_to(&rosc::encoder::encode(&pkt).unwrap(), ("127.0.0.1", port))
            .unwrap();
    };
    send(OscPacket::Message(OscMessage { addr: "/exec/1/level".into(), args: vec![OscType::Float(0.75)] }));
    match recv_event(&mut rx, 1000) {
        Some(IoEvent::Osc { addr, value }) => {
            assert_eq!(addr, "/exec/1/level");
            assert_eq!(value, Some(0.75));
        }
        e => panic!("unexpected {e:?}"),
    }
    send(OscPacket::Bundle(rosc::OscBundle {
        timetag: (0, 1).into(),
        content: vec![
            OscPacket::Message(OscMessage { addr: "/a".into(), args: vec![OscType::Int(3)] }),
            OscPacket::Message(OscMessage { addr: "/b".into(), args: vec![OscType::Bool(true)] }),
            OscPacket::Message(OscMessage { addr: "/c".into(), args: vec![] }),
        ],
    }));
    let vals: Vec<_> = (0..3)
        .map(|_| match recv_event(&mut rx, 1000) {
            Some(IoEvent::Osc { addr, value }) => (addr, value),
            e => panic!("unexpected {e:?}"),
        })
        .collect();
    assert_eq!(
        vals,
        vec![("/a".into(), Some(3.0)), ("/b".into(), Some(1.0)), ("/c".into(), None)]
    );
    let (_, ins) = hub.status();
    assert_eq!(ins.recent.len(), 4);
    assert!(ins.recent[0].contains("/c")); // newest first
}

#[test]
fn osc_in_bind_error_reported() {
    let busy = UdpSocket::bind("0.0.0.0:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let (tx, _rx) = unbounded_channel();
    let mut hub = IoHub::new(tx);
    let mut s = Settings::default();
    s.osc_in.enabled = true;
    s.osc_in.port = port;
    hub.configure(&[], &s);
    let (_, ins) = hub.status();
    assert!(!ins.osc_listening);
    assert!(!ins.osc_error.is_empty());
}

// ── Art-Net in (via hub) ──

#[test]
fn artnet_in_to_external_dmx_and_poll() {
    let (tx, mut rx) = unbounded_channel();
    let mut hub = IoHub::new(tx);
    hub.artnet_port = free_port();
    let mut s = no_io_settings();
    s.artnet_in.enabled = true;
    s.artnet_in.map = vec![ArtNetInMap { net: 0, subnet: 1, universe: 2, target: 7 }];
    hub.configure(&[], &s);
    assert!(hub.status().1.artnet_listening, "{}", hub.status().1.artnet_error);

    let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
    sock.set_read_timeout(Some(Duration::from_millis(1000))).unwrap();
    let target = ("127.0.0.1", hub.artnet_port);
    let d = frame(|i| (i % 200) as u8);
    // unmapped port-address is ignored
    sock.send_to(&build_artdmx(1, port_address(0, 0, 9), &d), target).unwrap();
    sock.send_to(&build_artdmx(1, port_address(0, 1, 2), &d), target).unwrap();
    match recv_event(&mut rx, 1000) {
        Some(IoEvent::ExternalDmx { universe, data }) => {
            assert_eq!(universe, 7);
            assert_eq!(data, d.to_vec());
        }
        e => panic!("unexpected {e:?}"),
    }
    // identical data within a second is suppressed
    sock.send_to(&build_artdmx(2, port_address(0, 1, 2), &d), target).unwrap();
    assert!(recv_event(&mut rx, 150).is_none());
    // changed data goes through
    let mut d2 = d;
    d2[0] = 99;
    sock.send_to(&build_artdmx(3, port_address(0, 1, 2), &d2), target).unwrap();
    assert!(matches!(recv_event(&mut rx, 1000), Some(IoEvent::ExternalDmx { .. })));
    assert!(hub.status().1.recent.iter().any(|l| l.contains("Art-Net 0:1:2 → U7")));

    // ArtPoll → ArtPollReply
    let mut poll = [0u8; 14];
    poll[..8].copy_from_slice(b"Art-Net\0");
    poll[8] = 0x00;
    poll[9] = 0x20;
    poll[11] = 14;
    sock.send_to(&poll, target).unwrap();
    let mut buf = [0u8; 512];
    let (n, _) = sock.recv_from(&mut buf).unwrap();
    assert_eq!(n, 239);
    assert_eq!(&buf[8..10], &[0x00, 0x21]);
    assert_eq!(&buf[26..31], b"Lumos");
}

// ── MTC ──

#[test]
fn mtc_quarter_frames_25fps() {
    // 01:02:03:04 @ 25 fps
    let qf = [0x04, 0x10, 0x23, 0x30, 0x42, 0x50, 0x61, 0x72];
    let mut d = MtcDecoder::default();
    let mut got = None;
    for (i, b) in qf.iter().enumerate() {
        let r = d.feed(&[0xF1, *b], 25.0);
        if i < 7 {
            assert!(r.is_none());
        } else {
            got = r;
        }
    }
    assert_eq!(got, Some(((3600 + 2 * 60 + 3) * 25 + 4) + 2));
}

#[test]
fn mtc_resyncs_after_gap_and_ignores_noise() {
    let mut d = MtcDecoder::default();
    // noise + incomplete sequence, then a gap
    assert!(d.feed(&[0xF8, 0xFE, 0xF1, 0x04, 0xF1, 0x10, 0xF1, 0x50], 30.0).is_none());
    // clean sequence for 00:00:01:00 @ 30
    let stream = [0xF1, 0x00, 0xF1, 0x10, 0xF1, 0x21, 0xF1, 0x30, 0xF1, 0x40, 0xF1, 0x50, 0xF1, 0x60, 0xF1, 0x76];
    assert_eq!(d.feed(&stream, 30.0), Some(30 + 2));
}

#[test]
fn mtc_full_frame_sysex() {
    let mut d = MtcDecoder::default();
    let msg = [0xF0, 0x7F, 0x7F, 0x01, 0x01, 0x21, 10, 20, 5, 0xF7]; // 01:10:20:05 @25 (rate bits set)
    assert_eq!(d.feed(&msg, 25.0), Some(((3600 + 600 + 20) * 25) + 5));
    // malformed sysex is ignored
    assert_eq!(d.feed(&[0xF0, 0x7E, 0x00, 0xF7], 25.0), None);
}

// ── hub: configure diffing and status ──

fn art_uni(id: u16, host: &str, port: u16) -> UniverseCfg {
    UniverseCfg {
        id,
        name: format!("U{id}"),
        outputs: vec![OutputCfg {
            enabled: true,
            kind: OutputKind::ArtNet { host: host.into(), port, net: 0, subnet: 0, universe: id as u8 },
        }],
    }
}

#[test]
fn configure_is_idempotent_and_diffs() {
    let (tx, _rx) = unbounded_channel();
    let mut hub = IoHub::new(tx);
    hub.artnet_port = free_port();
    let mut s = no_io_settings();
    s.osc_in.enabled = true;
    s.osc_in.port = free_port();
    let (rxs, rxp) = rx_sock();
    let unis = vec![art_uni(1, "127.0.0.1", rxp), art_uni(2, "127.0.0.1", rxp)];

    hub.configure(&unis, &s);
    let c1 = hub.counters;
    assert_eq!(c1.outputs_opened, 2);
    assert_eq!(c1.osc_binds, 1);
    assert_eq!(c1.artnet_binds, 1);

    hub.configure(&unis, &s);
    assert_eq!(hub.counters, c1, "identical configure must not reopen/rebind anything");

    // change only universe 2 → exactly one more output opened, inputs untouched
    let mut unis2 = unis.clone();
    if let OutputKind::ArtNet { universe, .. } = &mut unis2[1].outputs[0].kind {
        *universe = 9;
    }
    hub.configure(&unis2, &s);
    assert_eq!(hub.counters.outputs_opened, 3);
    assert_eq!(hub.counters.osc_binds, 1);

    // data flows to the right universe only
    let d = frame(|_| 5);
    hub.send(1, &d);
    let mut buf = [0u8; 1024];
    rxs.recv_from(&mut buf).unwrap();
    assert_eq!(buf[14], 0x01 & 0x0F);
    let (outs, _) = hub.status();
    assert_eq!(outs.len(), 2);
    assert_eq!((outs[0].universe, outs[0].frames, outs[0].state), (1, 1, OutState::Ok));
    assert_eq!(outs[1].frames, 0);

    // port change rebinds OSC once
    s.osc_in.port = free_port();
    hub.configure(&unis2, &s);
    assert_eq!(hub.counters.osc_binds, 2);

    // removing universes closes outputs and the Art-Net listener
    hub.configure(&[], &s);
    assert!(hub.status().0.is_empty());
    assert!(!hub.status().1.artnet_listening);
}

#[test]
fn disabled_and_usb_and_bad_outputs_report_status() {
    let (tx, _rx) = unbounded_channel();
    let mut hub = IoHub::new(tx);
    let unis = vec![UniverseCfg {
        id: 1,
        name: "U1".into(),
        outputs: vec![
            OutputCfg {
                enabled: false,
                kind: OutputKind::Sacn { universe: 1, priority: 100, unicast: None },
            },
            OutputCfg { enabled: true, kind: OutputKind::Usb { driver: "udmx".into(), serial: None } },
            OutputCfg {
                enabled: true,
                kind: OutputKind::Sacn { universe: 1, priority: 100, unicast: Some("bogus".into()) },
            },
        ],
    }];
    hub.configure(&unis, &no_io_settings());
    let (outs, _) = hub.status();
    assert_eq!(outs.len(), 3);
    assert_eq!(outs[0].state, OutState::Off);
    assert_eq!(outs[0].message, "disabled");
    #[cfg(not(feature = "usb"))]
    {
        assert_eq!(outs[1].state, OutState::Error);
        assert_eq!(outs[1].message, "built without usb support");
    }
    assert_eq!(outs[2].state, OutState::Error);
    hub.send(1, &[0u8; 512]); // must not panic
}

// ── serial ──

#[test]
fn enttec_pro_framing() {
    let d = frame(|i| i as u8);
    let p = build_enttec_pro(&d);
    assert_eq!(&p[..5], &[0x7E, 6, 0x01, 0x02, 0x00]); // label 6, len 513 LE, start code
    assert_eq!(&p[5..517], &d[..]);
    assert_eq!(p[517], 0xE7);
}

#[test]
fn serial_missing_port_reports_error_and_send_never_blocks() {
    let mut sink = serial::SerialSink::new(SerialDriver::EnttecOpen, "/dev/lumos-no-such-port");
    let t = Instant::now();
    for _ in 0..1000 {
        sink.send(1, &[1u8; 512]);
    }
    assert!(t.elapsed() < Duration::from_millis(200));
    let end = Instant::now() + Duration::from_secs(2);
    while sink.stat().state != OutState::Error && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    let st = sink.stat();
    assert_eq!(st.state, OutState::Error);
    assert!(!st.message.is_empty());
    assert_eq!(serial::SerialSink::new(SerialDriver::EnttecPro, "").stat().state, OutState::Error);
    let _ = list_serial_ports(); // must not panic without hardware
}
