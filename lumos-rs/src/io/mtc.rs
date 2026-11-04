//! MIDI Time Code decoder (quarter-frame and full-frame SysEx).

/// Feed raw MIDI bytes (whole messages or a stream); returns the assembled absolute frame
/// count whenever a complete timecode is known. Frame counting is non-drop at the nominal
/// rate (24 / 25 / 30; 29.97 counts as 30). A completed quarter-frame sequence is
/// compensated by +2 frames (the 8 pieces span two frames).
#[derive(Default)]
pub struct MtcDecoder {
    pieces: [u8; 8],
    /// Bit `n` set once piece `n` of the current sequence has arrived in order.
    seen: u8,
    last_piece: Option<u8>,
}

fn nominal(fps: f32) -> u32 {
    fps.round().max(1.0) as u32
}

fn to_frames(h: u32, m: u32, s: u32, f: u32, fps: f32) -> u32 {
    ((h * 60 + m) * 60 + s) * nominal(fps) + f
}

impl MtcDecoder {
    pub fn feed(&mut self, bytes: &[u8], fps: f32) -> Option<u32> {
        let mut out = None;
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                0xF1 if i + 1 < bytes.len() && bytes[i + 1] < 0x80 => {
                    if let Some(f) = self.quarter(bytes[i + 1], fps) {
                        out = Some(f);
                    }
                    i += 2;
                }
                0xF0 => {
                    let end = bytes[i..].iter().position(|&b| b == 0xF7).map(|p| i + p);
                    let sysex = &bytes[i..end.map(|e| e + 1).unwrap_or(bytes.len())];
                    if let Some(f) = full_frame(sysex, fps) {
                        self.seen = 0;
                        self.last_piece = None;
                        out = Some(f);
                    }
                    i += sysex.len().max(1);
                }
                _ => i += 1, // clock, active sensing, noise…
            }
        }
        out
    }

    fn quarter(&mut self, data: u8, fps: f32) -> Option<u32> {
        let piece = data >> 4;
        let val = data & 0x0F;
        if piece > 7 {
            return None;
        }
        // Pieces must arrive in ascending order; any gap restarts assembly at piece 0.
        let in_order = match self.last_piece {
            Some(p) => piece == (p + 1) % 8,
            None => piece == 0,
        };
        if !in_order {
            self.seen = 0;
            if piece != 0 {
                self.last_piece = None;
                return None;
            }
        }
        if piece == 0 {
            self.seen = 0;
        }
        self.last_piece = Some(piece);
        self.pieces[piece as usize] = val;
        self.seen |= 1 << piece;
        if piece == 7 && self.seen == 0xFF {
            let p = &self.pieces;
            let f = (p[0] | (p[1] << 4)) as u32;
            let s = (p[2] | (p[3] << 4)) as u32;
            let m = (p[4] | (p[5] << 4)) as u32;
            let h = (p[6] | ((p[7] & 0x01) << 4)) as u32;
            self.seen = 0;
            return Some(to_frames(h, m, s, f, fps) + 2);
        }
        None
    }
}

/// `F0 7F <id> 01 01 hh mm ss ff F7` (hh bits 5-6 carry the rate code).
fn full_frame(m: &[u8], fps: f32) -> Option<u32> {
    if m.len() >= 10 && m[0] == 0xF0 && m[1] == 0x7F && m[3] == 0x01 && m[4] == 0x01 && m[9] == 0xF7 {
        let h = (m[5] & 0x1F) as u32;
        return Some(to_frames(h, m[6] as u32, m[7] as u32, m[8] as u32, fps));
    }
    None
}
