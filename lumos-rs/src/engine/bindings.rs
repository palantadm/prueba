//! Input bindings: MIDI parsing, source matching, edge triggering. Timecode helpers.

use crate::model::*;

#[derive(Debug, Clone, PartialEq)]
pub struct MidiMsg {
    pub kind: MidiKind,
    /// 1..=16
    pub channel: u8,
    pub number: u8,
    /// Normalised 0..1 (note-off and velocity-0 note-on → 0).
    pub value: f32,
}

/// Parse a channel-voice message (note on/off, CC, program change). Other messages → `None`.
pub fn parse_midi(bytes: &[u8]) -> Option<MidiMsg> {
    let st = *bytes.first()?;
    if st < 0x80 || st >= 0xF0 {
        return None;
    }
    let ch = (st & 0x0F) + 1;
    let d1 = bytes.get(1).copied().unwrap_or(0) & 0x7F;
    let d2 = bytes.get(2).copied().unwrap_or(0) & 0x7F;
    match st & 0xF0 {
        0x80 => bytes.get(1).map(|_| MidiMsg { kind: MidiKind::Note, channel: ch, number: d1, value: 0.0 }),
        0x90 => bytes.get(2).map(|_| MidiMsg { kind: MidiKind::Note, channel: ch, number: d1, value: d2 as f32 / 127.0 }),
        0xB0 => bytes.get(2).map(|_| MidiMsg { kind: MidiKind::Cc, channel: ch, number: d1, value: d2 as f32 / 127.0 }),
        0xC0 => bytes.get(1).map(|_| MidiMsg { kind: MidiKind::Program, channel: ch, number: d1, value: 1.0 }),
        _ => None,
    }
}

pub enum InEvent<'a> {
    Midi { port: &'a str, msg: MidiMsg },
    Osc { addr: &'a str, value: Option<f32> },
}

impl InEvent<'_> {
    pub fn value(&self) -> f32 {
        match self {
            InEvent::Midi { msg, .. } => msg.value,
            InEvent::Osc { value, .. } => value.unwrap_or(1.0).clamp(0.0, 1.0),
        }
    }

    pub fn as_source(&self) -> Source {
        match self {
            InEvent::Midi { port, msg } => Source::Midi {
                port: Some(port.to_string()),
                kind: msg.kind,
                channel: Some(msg.channel),
                number: msg.number,
            },
            InEvent::Osc { addr, .. } => Source::Osc { address: addr.to_string() },
        }
    }

    /// Whether this event may be captured in learn mode (ignore note-offs).
    pub fn learnable(&self) -> bool {
        match self {
            InEvent::Midi { msg, .. } => !(msg.kind == MidiKind::Note && msg.value == 0.0),
            InEvent::Osc { .. } => true,
        }
    }

    pub fn log_line(&self) -> String {
        match self {
            InEvent::Midi { port, msg } => format!(
                "MIDI {} {:?} ch{} #{} = {:.2}",
                port, msg.kind, msg.channel, msg.number, msg.value
            ),
            InEvent::Osc { addr, value } => format!("OSC {addr} {}", value.map(|v| format!("{v:.3}")).unwrap_or_default()),
        }
    }
}

pub fn source_matches(src: &Source, ev: &InEvent) -> bool {
    match (src, ev) {
        (Source::Midi { port, kind, channel, number }, InEvent::Midi { port: p, msg }) => {
            *kind == msg.kind
                && *number == msg.number
                && channel.map_or(true, |c| c == msg.channel)
                && port.as_ref().map_or(true, |x| x.is_empty() || x == p)
        }
        (Source::Osc { address }, InEvent::Osc { addr, .. }) => address == addr,
        _ => false,
    }
}

/// Trigger actions fire when the value crosses upward through 0.5.
pub fn rising_edge(prev: Option<f32>, now: f32) -> bool {
    now >= 0.5 && prev.map_or(true, |p| p < 0.5)
}

// ───────────── timecode helpers ─────────────

pub fn fps_int(fps: f32) -> u32 {
    (fps.round() as u32).max(1)
}

pub fn fmt_tc(frames: u32, fps: f32) -> String {
    let f = fps_int(fps);
    let s = frames / f;
    format!("{:02}:{:02}:{:02}:{:02}", (s / 3600) % 100, (s / 60) % 60, s % 60, frames % f)
}

pub fn parse_tc(text: &str, fps: f32) -> Option<u32> {
    let p: Vec<u32> = text.trim().split([':', ';', '.']).map(|x| x.parse().ok()).collect::<Option<_>>()?;
    if p.len() != 4 {
        return None;
    }
    let f = fps_int(fps);
    if p[1] > 59 || p[2] > 59 || p[3] >= f {
        return None;
    }
    Some((p[0] * 3600 + p[1] * 60 + p[2]) * f + p[3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_parse() {
        let m = parse_midi(&[0xB0, 7, 127]).unwrap();
        assert_eq!((m.kind, m.channel, m.number, m.value), (MidiKind::Cc, 1, 7, 1.0));
        let m = parse_midi(&[0x93, 60, 100]).unwrap();
        assert_eq!((m.kind, m.channel, m.number), (MidiKind::Note, 4, 60));
        assert_eq!(parse_midi(&[0x93, 60, 0]).unwrap().value, 0.0);
        assert_eq!(parse_midi(&[0x80, 60, 64]).unwrap().value, 0.0);
        assert_eq!(parse_midi(&[0xC5, 9]).unwrap().kind, MidiKind::Program);
        assert!(parse_midi(&[0xF8]).is_none());
        assert!(parse_midi(&[]).is_none());
        assert!(parse_midi(&[0xB0, 1]).is_none());
    }

    #[test]
    fn matching() {
        let ev = InEvent::Midi { port: "KeyLab", msg: parse_midi(&[0xB1, 20, 64]).unwrap() };
        let any = Source::Midi { port: None, kind: MidiKind::Cc, channel: None, number: 20 };
        let ch2 = Source::Midi { port: None, kind: MidiKind::Cc, channel: Some(2), number: 20 };
        let ch3 = Source::Midi { port: None, kind: MidiKind::Cc, channel: Some(3), number: 20 };
        let port = Source::Midi { port: Some("Other".into()), kind: MidiKind::Cc, channel: None, number: 20 };
        assert!(source_matches(&any, &ev));
        assert!(source_matches(&ch2, &ev));
        assert!(!source_matches(&ch3, &ev));
        assert!(!source_matches(&port, &ev));
        let o = InEvent::Osc { addr: "/a/b", value: Some(0.3) };
        assert!(source_matches(&Source::Osc { address: "/a/b".into() }, &o));
        assert!(!source_matches(&Source::Osc { address: "/a/c".into() }, &o));
        assert!(!source_matches(&any, &o));
    }

    #[test]
    fn edge() {
        assert!(rising_edge(None, 1.0));
        assert!(rising_edge(Some(0.0), 0.5));
        assert!(!rising_edge(Some(0.7), 1.0));
        assert!(!rising_edge(Some(0.0), 0.4));
        assert!(rising_edge(Some(0.0), 1.0));
    }

    #[test]
    fn tc() {
        assert_eq!(fmt_tc(25 * 3661 + 7, 25.0), "01:01:01:07");
        assert_eq!(parse_tc("01:01:01:07", 25.0), Some(25 * 3661 + 7));
        assert_eq!(parse_tc("00:00:00:25", 25.0), None);
        assert_eq!(parse_tc("bad", 25.0), None);
    }
}
