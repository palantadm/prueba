//! Incremental Server-Sent-Events parser (chunk-boundary safe).

#[derive(Debug, Clone, PartialEq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub struct SseParser {
    buf: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
    started: bool,
}

impl SseParser {
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(nl) = self.buf.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=nl).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let mut text = String::from_utf8_lossy(&line).into_owned();
            if !self.started {
                self.started = true;
                if let Some(t) = text.strip_prefix('\u{feff}') {
                    text = t.to_string();
                }
            }
            self.line(&text, &mut out);
        }
        out
    }

    /// Flush a final event when the stream ends without a trailing blank line.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut out = Vec::new();
        if !self.buf.is_empty() {
            let t = String::from_utf8_lossy(&self.buf).into_owned();
            self.buf.clear();
            self.line(t.trim_end_matches('\r'), &mut out);
        }
        self.dispatch(&mut out);
        out
    }

    fn line(&mut self, l: &str, out: &mut Vec<SseEvent>) {
        if l.is_empty() {
            self.dispatch(out);
        } else if l.starts_with(':') {
            // comment / keep-alive
        } else {
            let (field, value) = match l.split_once(':') {
                Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
                None => (l, ""),
            };
            match field {
                "event" => self.event = Some(value.to_string()),
                "data" => self.data.push(value.to_string()),
                _ => {}
            }
        }
    }

    fn dispatch(&mut self, out: &mut Vec<SseEvent>) {
        if self.data.is_empty() && self.event.is_none() {
            return;
        }
        out.push(SseEvent { event: self.event.take(), data: std::mem::take(&mut self.data).join("\n") });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STREAM: &str = "event: a\ndata: {\"x\":1}\n\n: ping\n\ndata: l1\ndata: l2\n\nevent: b\r\ndata: [DONE]\r\n\r\n";

    fn all(chunks: &[&[u8]]) -> Vec<SseEvent> {
        let mut p = SseParser::default();
        let mut v = Vec::new();
        for c in chunks {
            v.extend(p.feed(c));
        }
        v.extend(p.finish());
        v
    }

    #[test]
    fn whole_and_chunked_agree() {
        let whole = all(&[STREAM.as_bytes()]);
        assert_eq!(whole.len(), 3);
        assert_eq!(whole[0], SseEvent { event: Some("a".into()), data: "{\"x\":1}".into() });
        assert_eq!(whole[1].data, "l1\nl2");
        assert_eq!(whole[2], SseEvent { event: Some("b".into()), data: "[DONE]".into() });
        for n in 1..9 {
            let c: Vec<&[u8]> = STREAM.as_bytes().chunks(n).collect();
            assert_eq!(all(&c), whole, "chunk size {n}");
        }
    }

    #[test]
    fn utf8_split_across_chunks_and_bom() {
        let s = "\u{feff}data: caf\u{e9} \u{1f4a1}\n\n";
        let b = s.as_bytes();
        for cut in 0..b.len() {
            let v = all(&[&b[..cut], &b[cut..]]);
            assert_eq!(v.len(), 1);
            assert_eq!(v[0].data, "caf\u{e9} \u{1f4a1}", "cut {cut}");
        }
    }

    #[test]
    fn missing_trailing_blank_line_is_flushed() {
        let v = all(&[b"data: last"]);
        assert_eq!(v, vec![SseEvent { event: None, data: "last".into() }]);
    }

    #[test]
    fn think_filter_across_chunks() {
        let mut f = crate::ai::types::ThinkFilter::default();
        let mut s = String::new();
        for c in ["Hola <th", "ink>secreto</th", "ink> mundo", " <b>x</b>"] {
            s.push_str(&f.push(c));
        }
        s.push_str(&f.finish());
        assert_eq!(s, "Hola  mundo <b>x</b>");
    }
}
