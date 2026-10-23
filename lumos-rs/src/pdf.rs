//! Minimal PDF 1.4 writer: A4 pages, Helvetica / Helvetica-Bold (WinAnsi, so Spanish accents work),
//! text, lines and filled rectangles. Uncompressed streams keep the output inspectable and dependency-free.

use std::fmt::Write as _;

pub const PAGE_W: f32 = 595.28;
pub const PAGE_H: f32 = 841.89;

/// Helvetica advance widths (1/1000 em) for ASCII 32..=126.
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556, 556,
    556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556,
    556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];
/// Helvetica-Bold advance widths for ASCII 32..=126.
const HELV_B: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556, 556,
    556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611,
    611, 278, 278, 556, 278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

/// Unicode → Windows-1252 byte (what WinAnsiEncoding means); unknown characters become `?`.
pub fn win_ansi(c: char) -> u8 {
    match c {
        ' '..='~' => c as u8,
        '\u{A0}'..='\u{FF}' => c as u32 as u8,
        '€' => 0x80,
        '…' => 0x85,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        _ => b'?',
    }
}

fn glyph_w(b: u8, bold: bool) -> f32 {
    let t = if bold { &HELV_B } else { &HELV };
    match b {
        32..=126 => t[(b - 32) as usize] as f32,
        0x85 => 1000.0,
        0x95 => 350.0,
        0x96 => 556.0,
        0x97 => 1000.0,
        0xC0..=0xFF => {
            // accented Latin letters: width of the base letter (close enough for layout)
            let base = match b {
                0xC0..=0xC5 => b'A',
                0xC7 => b'C',
                0xC8..=0xCB => b'E',
                0xCC..=0xCF => b'I',
                0xD1 => b'N',
                0xD2..=0xD6 => b'O',
                0xD9..=0xDC => b'U',
                0xE0..=0xE5 => b'a',
                0xE7 => b'c',
                0xE8..=0xEB => b'e',
                0xEC..=0xEF => b'i',
                0xF1 => b'n',
                0xF2..=0xF6 => b'o',
                0xF9..=0xFC => b'u',
                _ => b'x',
            };
            t[(base - 32) as usize] as f32
        }
        _ => 556.0,
    }
}

pub fn text_width(s: &str, size: f32, bold: bool) -> f32 {
    s.chars().map(|c| glyph_w(win_ansi(c), bold)).sum::<f32>() * size / 1000.0
}

/// Truncate `s` with an ellipsis so it fits in `width` points.
pub fn fit(s: &str, width: f32, size: f32, bold: bool) -> String {
    if text_width(s, size, bold) <= width {
        return s.to_string();
    }
    let mut out = String::new();
    let ell = text_width("…", size, bold);
    for c in s.chars() {
        let mut t = out.clone();
        t.push(c);
        if text_width(&t, size, bold) + ell > width {
            break;
        }
        out = t;
    }
    out.push('…');
    out
}

/// The literal-string form of `s` as written into content streams (`(…)`), for tests and searches.
pub fn pdf_string(s: &str) -> Vec<u8> {
    let mut v = Vec::new();
    push_pdf_string(&mut v, s);
    v
}

fn push_pdf_string(buf: &mut Vec<u8>, s: &str) {
    buf.push(b'(');
    for c in s.chars() {
        let b = win_ansi(c);
        match b {
            b'(' | b')' | b'\\' => {
                buf.push(b'\\');
                buf.push(b);
            }
            0x20..=0x7E => buf.push(b),
            _ => buf.extend_from_slice(format!("\\{:03o}", b).as_bytes()),
        }
    }
    buf.push(b')');
}

#[derive(Default, Clone)]
pub struct Page {
    ops: Vec<u8>,
}

impl Page {
    /// `y` is measured from the bottom of the page (PDF convention).
    pub fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, s: &str) {
        let f = if bold { "F2" } else { "F1" };
        let _ = write!(self.ops_str(), "BT /{f} {size:.2} Tf {x:.2} {y:.2} Td ");
        push_pdf_string(&mut self.ops, s);
        self.ops.extend_from_slice(b" Tj ET\n");
    }

    pub fn text_right(&mut self, x_right: f32, y: f32, size: f32, bold: bool, s: &str) {
        let w = text_width(s, size, bold);
        self.text(x_right - w, y, size, bold, s);
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, gray: f32) {
        let _ = write!(self.ops_str(), "{gray:.2} G {width:.2} w {x1:.2} {y1:.2} m {x2:.2} {y2:.2} l S\n");
    }

    pub fn rect_fill(&mut self, x: f32, y: f32, w: f32, h: f32, rgb: (f32, f32, f32)) {
        let _ = write!(self.ops_str(), "{:.3} {:.3} {:.3} rg {x:.2} {y:.2} {w:.2} {h:.2} re f\n", rgb.0, rgb.1, rgb.2);
    }

    fn ops_str(&mut self) -> OpsWriter<'_> {
        OpsWriter(&mut self.ops)
    }
}

struct OpsWriter<'a>(&'a mut Vec<u8>);

impl std::fmt::Write for OpsWriter<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

pub struct Doc {
    pub pages: Vec<Page>,
    pub title: String,
}

impl Doc {
    pub fn new(title: &str) -> Self {
        Doc { pages: vec![], title: title.to_string() }
    }

    pub fn add_page(&mut self) -> usize {
        self.pages.push(Page::default());
        self.pages.len() - 1
    }

    /// Serialise: objects 1 catalog, 2 pages, 3/4 fonts, 5 info, then (page, content) pairs.
    pub fn finish(&self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut offsets: Vec<usize> = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

        let n_pages = self.pages.len().max(1);
        let first_page_obj = 6usize;
        let kids: String = (0..n_pages).map(|i| format!("{} 0 R", first_page_obj + i * 2)).collect::<Vec<_>>().join(" ");

        let mut obj = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
            offsets.push(out.len());
            let n = offsets.len();
            out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        };

        obj(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
        obj(&mut out, &mut offsets, format!("<< /Type /Pages /Kids [{kids}] /Count {n_pages} >>").as_bytes());
        obj(
            &mut out,
            &mut offsets,
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        );
        obj(
            &mut out,
            &mut offsets,
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>",
        );
        let mut info = Vec::new();
        info.extend_from_slice(b"<< /Producer (Lumos) /Title ");
        push_pdf_string(&mut info, &self.title);
        info.extend_from_slice(b" >>");
        obj(&mut out, &mut offsets, &info);

        let empty = Page::default();
        for i in 0..n_pages {
            let page = self.pages.get(i).unwrap_or(&empty);
            let content_obj = first_page_obj + i * 2 + 1;
            obj(
                &mut out,
                &mut offsets,
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {content_obj} 0 R >>"
                )
                .as_bytes(),
            );
            let mut stream = format!("<< /Length {} >>\nstream\n", page.ops.len()).into_bytes();
            stream.extend_from_slice(&page.ops);
            stream.extend_from_slice(b"endstream");
            obj(&mut out, &mut offsets, &stream);
        }

        let xref_pos = out.len();
        let count = offsets.len() + 1;
        out.extend_from_slice(format!("xref\n0 {count}\n0000000000 65535 f \n").as_bytes());
        for o in &offsets {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!("trailer\n<< /Size {count} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes(),
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_and_fit() {
        assert!((text_width("Hello", 10.0, false) - 22.78).abs() < 0.01);
        assert!(text_width("Hello", 10.0, true) > text_width("Hello", 10.0, false));
        let f = fit("Un nombre bastante largo para una columna", 60.0, 8.0, false);
        assert!(f.ends_with('…'));
        assert!(text_width(&f, 8.0, false) <= 60.0);
        assert_eq!(fit("corto", 100.0, 8.0, false), "corto");
    }

    #[test]
    fn winansi_accents() {
        assert_eq!(win_ansi('á'), 0xE1);
        assert_eq!(win_ansi('ñ'), 0xF1);
        assert_eq!(win_ansi('¿'), 0xBF);
        assert_eq!(win_ansi('€'), 0x80);
        assert_eq!(win_ansi('日'), b'?');
    }

    #[test]
    fn valid_structure() {
        let mut d = Doc::new("Prueba ñ");
        let a = d.add_page();
        d.pages[a].text(40.0, 800.0, 12.0, true, "Hola (mundo) \\ ñ");
        d.pages[a].line(40.0, 790.0, 500.0, 790.0, 0.5, 0.5);
        d.add_page();
        let pdf = d.finish();
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let find = |needle: &[u8]| pdf.windows(needle.len()).position(|w| w == needle);
        let count = |needle: &[u8]| pdf.windows(needle.len()).filter(|w| *w == needle).count();
        assert_eq!(count(b"/Type /Page "), 2);
        assert_eq!(count(b"/Count 2"), 1);
        assert!(find(b"(Hola \\(mundo\\) \\\\ \\361)").is_some());
        // xref offsets are byte offsets pointing at "N 0 obj"
        let sx = find(b"startxref\n").unwrap();
        let tail = String::from_utf8_lossy(&pdf[sx + 10..]).to_string();
        let xref_pos: usize = tail.lines().next().unwrap().parse().unwrap();
        assert!(pdf[xref_pos..].starts_with(b"xref"));
        let table = String::from_utf8_lossy(&pdf[xref_pos..]).to_string();
        for (i, l) in table.lines().skip(3).take(5).enumerate() {
            let off: usize = l[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()), "obj {} at {off}", i + 1);
        }
    }
}
