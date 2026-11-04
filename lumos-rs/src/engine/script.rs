//! Script / running order: text blocks with cue marks, a reader pointer and the actions a mark fires.
//!
//! * A [`Script`] is an ordered list of [`ScriptBlock`]s; each block may carry [`CueMark`]s. Marks are
//!   ordered by (block order, `at_word`, order inside the block) — the *mark list*.
//! * Each script has a reader pointer ([`Reader`]): `ScriptGo` fires the next mark of the list and
//!   moves on, `ScriptBack` re-arms the previous one without firing, `ScriptGoto` places the pointer
//!   (the next GO fires the mark it points at, or the first mark at/after the block), `ScriptFire`
//!   fires one specific mark (test / jump), `ScriptReset` rewinds.
//! * Firing a mark runs its linked Lumos cue (`GoTo`) and then its [`ScriptAction`]s in order. Errors are
//!   written to `cmd_log` and the input activity strip and never stop the show or the remaining actions.
//! * FOLLOW MODE (`ScriptFollow`): when any cue that a mark links to starts (GO, GO TO, timecode,
//!   follow…), the reader jumps to that mark *without* firing it. Cues started by the script itself are
//!   ignored (no loops).
//! * Reader state is runtime-only; the scripts themselves live in the show (undoable).

use super::cmdline::parse;
use super::*;
use std::collections::{BTreeSet, HashMap};

pub const MAX_BLOCKS: usize = 20_000;
pub const MAX_TEXT_CHARS: usize = 8_000;
pub const MAX_MARKS_PER_BLOCK: usize = 50;
pub const MAX_ACTIONS: usize = 20;
/// A mark whose actions run `script N go` again may nest at most this deep.
pub const MAX_DEPTH: u8 = 8;

/// What the next `ScriptGo` will fire.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Cur {
    /// Before the first mark.
    #[default]
    Start,
    /// This (block id, mark id) fires next.
    Next(u32, u32),
    /// Placed at the start of a block: the first mark at/after it fires next.
    Block(u32),
    /// Past the last mark.
    End,
}

#[derive(Clone, Debug, Default)]
pub struct Reader {
    pub cur: Cur,
    /// Block id the reader is on (0 = before the start).
    pub block: u32,
    pub last_mark: Option<u32>,
    pub label: String,
}

#[derive(Default)]
pub struct ScriptState {
    pub runs: HashMap<u32, Reader>,
    pub follow: BTreeSet<u32>,
    /// True while a mark's link/actions execute (blocks the follow hook).
    pub firing: bool,
    pub depth: u8,
}

/// `(block id, mark id)` of every mark in firing order.
pub fn flat(script: &Script) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for b in &script.blocks {
        let mut ms: Vec<&CueMark> = b.marks.iter().collect();
        ms.sort_by_key(|m| m.at_word); // stable: marks at the same word keep their order
        out.extend(ms.into_iter().map(|m| (b.id, m.id)));
    }
    out
}

fn block_pos(script: &Script, id: u32) -> Option<usize> {
    script.blocks.iter().position(|b| b.id == id)
}

/// Index into [`flat`] of the mark that fires next.
fn next_index(script: &Script, r: &Reader, list: &[(u32, u32)]) -> usize {
    let at_block = |id: u32| -> usize {
        match block_pos(script, id) {
            Some(p) => list.iter().position(|(b, _)| block_pos(script, *b).map_or(false, |q| q >= p)).unwrap_or(list.len()),
            None => 0,
        }
    };
    match r.cur {
        Cur::Start => 0,
        Cur::End => list.len(),
        Cur::Next(b, m) => list.iter().position(|x| *x == (b, m)).unwrap_or_else(|| at_block(r.block)),
        Cur::Block(b) => at_block(b),
    }
}

fn find_mark(script: &Script, block: u32, mark: u32) -> Option<&CueMark> {
    script.blocks.iter().find(|b| b.id == block)?.marks.iter().find(|m| m.id == mark)
}

fn mark_label(m: &CueMark) -> String {
    if !m.label.trim().is_empty() {
        return m.label.trim().to_string();
    }
    let k = match m.kind {
        MarkKind::Light => "Luz",
        MarkKind::Sound => "Sonido",
        MarkKind::Video => "Vídeo",
        MarkKind::Set => "Escena",
        MarkKind::Other => "Cue",
    };
    format!("{k} {}", m.id)
}

/// MIDI wire bytes for a script action (`channel` 1..=16).
pub fn midi_bytes(kind: MidiKind, channel: u8, number: u8, value: u8) -> Vec<u8> {
    let ch = channel.clamp(1, 16) - 1;
    let n = number.min(127);
    let v = value.min(127);
    match kind {
        MidiKind::Note => vec![0x90 | ch, n, v],
        MidiKind::Cc => vec![0xB0 | ch, n, v],
        MidiKind::Program => vec![0xC0 | ch, n],
    }
}

/// Typed OSC argument from text: `true`/`false`, integers, decimals, else string. A `s:`/`i:`/`f:`
/// prefix forces the type (`s:007`).
pub fn osc_arg(s: &str) -> rosc::OscType {
    use rosc::OscType;
    if let Some(r) = s.strip_prefix("s:") {
        return OscType::String(r.to_string());
    }
    if let Some(r) = s.strip_prefix("i:") {
        if let Ok(i) = r.trim().parse::<i32>() {
            return OscType::Int(i);
        }
        return OscType::String(s.to_string());
    }
    if let Some(r) = s.strip_prefix("f:") {
        if let Ok(f) = r.trim().parse::<f32>() {
            if f.is_finite() {
                return OscType::Float(f);
            }
        }
        return OscType::String(s.to_string());
    }
    match s {
        "true" => return OscType::Bool(true),
        "false" => return OscType::Bool(false),
        _ => {}
    }
    if let Ok(i) = s.parse::<i32>() {
        return OscType::Int(i);
    }
    if s.contains(|c: char| c.is_ascii_digit()) {
        if let Ok(f) = s.parse::<f32>() {
            if f.is_finite() {
                return OscType::Float(f);
            }
        }
    }
    OscType::String(s.to_string())
}

pub fn valid_osc_target(host: &str, port: u16, address: &str, args: &[String]) -> Result<(), String> {
    if host.is_empty() || host.len() > 253 || host.chars().any(|c| c.is_whitespace() || c.is_control() || c == '/') {
        return Err(format!("OSC host '{host}' is not valid"));
    }
    if port == 0 {
        return Err("OSC port must be 1-65535".into());
    }
    if !address.starts_with('/') || address.len() > 256 || address.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("OSC address '{address}' must start with '/' and contain no spaces"));
    }
    if args.len() > 16 || args.iter().any(|a| a.len() > 256) {
        return Err("OSC: at most 16 arguments of 256 characters".into());
    }
    Ok(())
}

pub fn valid_http(method: &str, url: &str) -> Result<(reqwest::Method, reqwest::Url), String> {
    let m = match method.trim().to_ascii_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "PATCH" => reqwest::Method::PATCH,
        "DELETE" => reqwest::Method::DELETE,
        other => return Err(format!("HTTP method '{other}' not allowed (GET, POST, PUT, PATCH, DELETE)")),
    };
    if url.len() > 2048 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("HTTP url is not valid".into());
    }
    let u = reqwest::Url::parse(url).map_err(|e| format!("HTTP url '{url}': {e}"))?;
    if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none() {
        return Err("HTTP url must be http:// or https://".into());
    }
    Ok((m, u))
}

fn validate_action(a: &ScriptAction) -> Result<(), String> {
    match a {
        ScriptAction::Console { text } => {
            if text.len() > 2000 {
                return Err("console action too long (max 2000 bytes)".into());
            }
            parse(text).map(|_| ()).map_err(|e| e.render(text))
        }
        ScriptAction::Osc { host, port, address, args } => valid_osc_target(host, *port, address, args),
        ScriptAction::Midi { channel, number, value, .. } => {
            if !(1..=16).contains(channel) || *number > 127 || *value > 127 {
                Err("MIDI: channel 1-16, number and value 0-127".into())
            } else {
                Ok(())
            }
        }
        ScriptAction::Http { method, url, body } => {
            if body.len() > 16_384 {
                return Err("HTTP body too long (max 16 KB)".into());
            }
            valid_http(method, url).map(|_| ())
        }
    }
}

impl ScriptState {
    pub fn reset(&mut self) {
        self.runs.clear();
        self.follow.clear();
        self.firing = false;
        self.depth = 0;
    }
}

impl Engine {
    // ───────────── show edits ─────────────

    pub(super) fn script_save(&mut self, mut s: Script) -> Result<(), String> {
        s.name = s.name.trim().chars().take(200).collect();
        if s.name.is_empty() {
            s.name = "Guion".into();
        }
        if s.blocks.len() > MAX_BLOCKS {
            return Err(format!("Script has too many blocks (max {MAX_BLOCKS})"));
        }
        if let Some(p) = &s.pdf {
            if !p.starts_with("/media/") || p.contains("..") || p.contains('\\') || p.len() > 300 {
                return Err("pdf must be a /media/<file> path".into());
            }
        }
        // stable unique block ids, then unique mark ids (script-wide)
        let mut used: BTreeSet<u32> = BTreeSet::new();
        let mut max_b = s.blocks.iter().map(|b| b.id).max().unwrap_or(0);
        for b in &mut s.blocks {
            if b.id == 0 || !used.insert(b.id) {
                max_b += 1;
                b.id = max_b;
                used.insert(b.id);
            }
        }
        let mut used_m: BTreeSet<u32> = BTreeSet::new();
        let mut max_m = s.blocks.iter().flat_map(|b| b.marks.iter().map(|m| m.id)).max().unwrap_or(0);
        for b in &mut s.blocks {
            if b.text.chars().count() > MAX_TEXT_CHARS || b.speaker.chars().count() > 200 {
                return Err(format!("Block {}: text too long (max {MAX_TEXT_CHARS} characters)", b.id));
            }
            if b.marks.len() > MAX_MARKS_PER_BLOCK {
                return Err(format!("Block {}: too many marks (max {MAX_MARKS_PER_BLOCK})", b.id));
            }
            let words = b.text.split_whitespace().count() as u32;
            for m in &mut b.marks {
                if m.id == 0 || !used_m.insert(m.id) {
                    max_m += 1;
                    m.id = max_m;
                    used_m.insert(m.id);
                }
                m.at_word = m.at_word.min(words.saturating_sub(1));
                if m.actions.len() > MAX_ACTIONS {
                    return Err(format!("Mark {}: too many actions (max {MAX_ACTIONS})", m.id));
                }
                if let Some(l) = &m.link {
                    if !l.number.is_finite() {
                        return Err(format!("Mark {}: cue link is not a number", m.id));
                    }
                }
                for (i, a) in m.actions.iter().enumerate() {
                    validate_action(a).map_err(|e| format!("Mark {} action {}: {e}", m.id, i + 1))?;
                }
            }
        }
        if s.id == 0 {
            s.id = next_id(self.show.scripts.iter().map(|x| x.id));
        }
        match self.show.scripts.iter_mut().find(|x| x.id == s.id) {
            Some(x) => *x = s,
            None => self.show.scripts.push(s),
        }
        self.touch();
        Ok(())
    }

    pub(super) fn script_remove(&mut self, id: u32) {
        self.show.scripts.retain(|s| s.id != id);
        self.script.runs.remove(&id);
        self.script.follow.remove(&id);
        self.touch();
    }

    fn require_script(&self, id: u32) -> Result<&Script, String> {
        self.show.scripts.iter().find(|s| s.id == id).ok_or_else(|| format!("Script {id} does not exist"))
    }

    // ───────────── pointer ─────────────

    pub(super) fn script_go(&mut self, id: u32) -> Result<(), String> {
        let sc = self.require_script(id)?;
        let r = self.script.runs.get(&id).cloned().unwrap_or_default();
        let list = flat(sc);
        let idx = next_index(sc, &r, &list);
        let Some(&(b, m)) = list.get(idx) else {
            return Err("End of script: no more cue marks".into());
        };
        self.script_fire(id, b, m)
    }

    pub(super) fn script_back(&mut self, id: u32) -> Result<(), String> {
        let sc = self.require_script(id)?;
        let r = self.script.runs.get(&id).cloned().unwrap_or_default();
        let list = flat(sc);
        let idx = next_index(sc, &r, &list);
        if idx == 0 {
            return Ok(());
        }
        let (rb, rm) = list[idx - 1];
        let prev = if idx >= 2 { Some(list[idx - 2]) } else { None };
        let label = prev.and_then(|(b, m)| find_mark(sc, b, m)).map(mark_label).unwrap_or_default();
        let rd = Reader { cur: Cur::Next(rb, rm), block: prev.map_or(0, |p| p.0), last_mark: prev.map(|p| p.1), label };
        self.script.runs.insert(id, rd);
        self.dirty = true;
        Ok(())
    }

    pub(super) fn script_goto(&mut self, id: u32, block: u32, mark: Option<u32>) -> Result<(), String> {
        let sc = self.require_script(id)?;
        let label = self.script.runs.get(&id).map(|r| r.label.clone()).unwrap_or_default();
        let rd = if block == 0 {
            Reader { cur: Cur::Start, block: 0, last_mark: None, label }
        } else {
            if block_pos(sc, block).is_none() {
                return Err(format!("Script {id} has no block {block}"));
            }
            match mark {
                Some(m) => {
                    if find_mark(sc, block, m).is_none() {
                        return Err(format!("Block {block} has no mark {m}"));
                    }
                    Reader { cur: Cur::Next(block, m), block, last_mark: None, label }
                }
                None => Reader { cur: Cur::Block(block), block, last_mark: None, label },
            }
        };
        self.script.runs.insert(id, rd);
        self.dirty = true;
        Ok(())
    }

    /// Voice-follow: the speaker said `heard` (normalised words). If that phrase sits ahead of the reader, move the
    /// reader there (`fire`: also GO the next mark when it lies at or before the spoken block). Returns whether it moved.
    pub(super) fn script_move_by_words(&mut self, id: u32, heard: &[String], fire: bool) -> Result<bool, String> {
        let sc = self.require_script(id)?;
        let r = self.script.runs.get(&id).cloned().unwrap_or_default();
        let cur = if r.block == 0 { None } else { block_pos(sc, r.block) };
        let Some(target) = crate::speech::matcher::target_block(sc, cur.unwrap_or(0), heard) else { return Ok(false) };
        if Some(target) == cur {
            return Ok(false);
        }
        let list = flat(sc);
        let idx = next_index(sc, &r, &list);
        let next_block = list.get(idx).and_then(|(b, _)| block_pos(sc, *b));
        let target_id = sc.blocks[target].id;
        match next_block {
            Some(nb) if fire && nb <= target => {
                self.script_go(id)?;
            }
            _ => self.script_goto(id, target_id, None)?,
        }
        Ok(true)
    }

    pub(super) fn script_reset(&mut self, id: u32) -> Result<(), String> {
        self.require_script(id)?;
        self.script.runs.remove(&id);
        self.dirty = true;
        Ok(())
    }

    pub(super) fn script_follow_set(&mut self, id: u32, on: bool) -> Result<(), String> {
        self.require_script(id)?;
        if on {
            self.script.follow.insert(id);
        } else {
            self.script.follow.remove(&id);
        }
        self.dirty = true;
        Ok(())
    }

    /// Fire one mark and move the pointer past it.
    pub(super) fn script_fire(&mut self, id: u32, block: u32, mark: u32) -> Result<(), String> {
        let sc = self.require_script(id)?;
        let m = find_mark(sc, block, mark).ok_or_else(|| format!("Script {id} has no mark {mark} in block {block}"))?.clone();
        let list = flat(sc);
        let idx = list.iter().position(|x| *x == (block, mark)).unwrap_or(0);
        let cur = list.get(idx + 1).map_or(Cur::End, |&(b, m)| Cur::Next(b, m));
        let label = mark_label(&m);
        let name = sc.name.clone();
        self.script.runs.insert(id, Reader { cur, block, last_mark: Some(mark), label: label.clone() });
        self.dirty = true;
        self.script_fire_mark(&name, &label, &m)
    }

    /// Run a mark's cue link and actions; failures are logged, never fatal.
    fn script_fire_mark(&mut self, script: &str, label: &str, m: &CueMark) -> Result<(), String> {
        if self.script.depth >= MAX_DEPTH {
            return Err("script marks are nested too deeply".into());
        }
        self.script.depth += 1;
        let was = self.script.firing;
        self.script.firing = true;
        let mut errors: Vec<String> = Vec::new();
        let mut done = 0usize;
        if let Some(l) = &m.link {
            match self.go_to(l.cuelist, l.number) {
                Ok(()) => done += 1,
                Err(e) => errors.push(format!("cue {}/{}: {e}", l.cuelist, l.number)),
            }
        }
        for (i, a) in m.actions.iter().enumerate() {
            match self.script_action(a) {
                Ok(()) => done += 1,
                Err(e) => errors.push(format!("action {}: {e}", i + 1)),
            }
        }
        self.script.firing = was;
        self.script.depth -= 1;
        let text = format!("script «{script}»: {label}");
        let ok = errors.is_empty();
        let msg = if ok { format!("{done} action{}", if done == 1 { "" } else { "s" }) } else { errors.join("; ") };
        self.auto.log.push(&text, ok, msg.clone());
        self.recent.push_front(format!("{}{text}{}", if ok { "▶ " } else { "⚠ " }, if ok { String::new() } else { format!(" — {msg}") }));
        self.recent.truncate(8);
        self.dirty = true;
        Ok(())
    }

    // ───────────── actions ─────────────

    /// Execute one external action now (also used by the `osc`/`midi`/`http` command-line verbs).
    pub(super) fn script_action(&mut self, a: &ScriptAction) -> Result<(), String> {
        match a {
            ScriptAction::Console { text } => self.run_cmdline_new(text),
            ScriptAction::Osc { host, port, address, args } => {
                valid_osc_target(host, *port, address, args)?;
                let osc_args: Vec<rosc::OscType> = args.iter().map(|s| osc_arg(s)).collect();
                if self.io.osc_send_args(host, *port, address, osc_args) {
                    Ok(())
                } else {
                    Err(format!("OSC to {host}:{port} could not be sent (unknown host?)"))
                }
            }
            ScriptAction::Midi { port, kind, channel, number, value } => {
                if !(1..=16).contains(channel) || *number > 127 || *value > 127 {
                    return Err("MIDI: channel 1-16, number and value 0-127".into());
                }
                let bytes = midi_bytes(*kind, *channel, *number, *value);
                if !self.midi.send(port.as_deref(), &bytes) {
                    self.emit(&ServerMsg::MidiOut { port: port.clone(), bytes });
                }
                Ok(())
            }
            ScriptAction::Http { method, url, body } => self.script_http(method, url, body),
        }
    }

    fn script_http(&mut self, method: &str, url: &str, body: &str) -> Result<(), String> {
        let (m, u) = valid_http(method, url)?;
        let rt = tokio::runtime::Handle::try_current().map_err(|_| "HTTP actions need the async runtime".to_string())?;
        let out = self.out.clone();
        let body = body.to_string();
        let shown = format!("{} {}", m, u.host_str().unwrap_or(""));
        rt.spawn(async move {
            let log = |level: &str, msg: String| {
                let _ = out.send(Out::Json(Arc::from(json(&ServerMsg::Log { level: level.into(), msg }))));
            };
            let host = u.host_str().map(|h| h.to_string());
            let policy = reqwest::redirect::Policy::custom(move |att| {
                if att.previous().len() >= 3 || att.url().host_str().map(|h| h.to_string()) != host {
                    att.stop()
                } else {
                    att.follow()
                }
            });
            let _ = rustls::crypto::ring::default_provider().install_default();
            let client = match reqwest::Client::builder().timeout(Duration::from_secs(5)).redirect(policy).build() {
                Ok(c) => c,
                Err(e) => return log("error", format!("HTTP client: {e}")),
            };
            let mut req = client.request(m.clone(), u);
            if !body.is_empty() && m != reqwest::Method::GET {
                let ct = if body.trim_start().starts_with(['{', '[']) { "application/json" } else { "text/plain; charset=utf-8" };
                req = req.header("content-type", ct).body(body);
            }
            match req.send().await {
                Ok(r) if r.status().is_success() || r.status().is_redirection() => log("info", format!("{shown}: {}", r.status())),
                Ok(r) => log("error", format!("{shown}: HTTP {}", r.status())),
                Err(e) => log("error", format!("{shown}: {e}")),
            }
        });
        Ok(())
    }

    // ───────────── follow + runtime ─────────────

    /// Called whenever a cue starts. In follow mode the reader jumps to the mark linked to it.
    pub(super) fn script_follow_cue(&mut self, cuelist: u32, number: f64) {
        if self.script.firing || self.script.follow.is_empty() {
            return;
        }
        let ids: Vec<u32> = self.script.follow.iter().copied().collect();
        for id in ids {
            let Some(sc) = self.show.scripts.iter().find(|s| s.id == id) else { continue };
            let list = flat(sc);
            let hit = list.iter().position(|&(b, m)| {
                find_mark(sc, b, m).and_then(|mk| mk.link.as_ref()).map_or(false, |l| l.cuelist == cuelist && (l.number - number).abs() < 1e-6)
            });
            let Some(i) = hit else { continue };
            let (b, m) = list[i];
            let label = find_mark(sc, b, m).map(mark_label).unwrap_or_default();
            let cur = list.get(i + 1).map_or(Cur::End, |&(nb, nm)| Cur::Next(nb, nm));
            self.script.runs.insert(id, Reader { cur, block: b, last_mark: Some(m), label });
            self.dirty = true;
        }
    }

    pub(super) fn scripts_rt(&self) -> Vec<ScriptRt> {
        self.show
            .scripts
            .iter()
            .map(|sc| {
                let r = self.script.runs.get(&sc.id).cloned().unwrap_or_default();
                let list = flat(sc);
                let idx = next_index(sc, &r, &list);
                ScriptRt {
                    id: sc.id,
                    block: r.block,
                    mark: r.last_mark,
                    next: list.get(idx).copied(),
                    follow: self.script.follow.contains(&sc.id),
                    last_label: r.label,
                }
            })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::tests_support::*;
    use super::*;
    use std::net::UdpSocket;

    pub(crate) fn mark(id: u32, at_word: u32, actions: Vec<ScriptAction>) -> CueMark {
        CueMark { id, label: format!("m{id}"), kind: MarkKind::Light, at_word, actions, link: None, standby_blocks: 0, note: String::new() }
    }

    pub(crate) fn block(id: u32, text: &str, marks: Vec<CueMark>) -> ScriptBlock {
        ScriptBlock { id, kind: ScriptKind::Speech, speaker: "ANA".into(), text: text.into(), page: 0, marks }
    }

    fn con(t: &str) -> ScriptAction {
        ScriptAction::Console { text: t.into() }
    }

    fn script(blocks: Vec<ScriptBlock>) -> Script {
        Script { id: 0, name: "Obra".into(), blocks, pdf: None }
    }

    fn rt(e: &Engine, id: u32) -> ScriptRt {
        e.scripts_rt().into_iter().find(|s| s.id == id).unwrap()
    }

    #[test]
    fn save_assigns_ids_and_clamps() {
        let mut e = engine();
        let s = script(vec![block(0, "uno dos tres", vec![mark(0, 99, vec![]), mark(0, 1, vec![])]), block(0, "cuatro", vec![])]);
        e.handle(Command::ScriptSave { script: s }).unwrap();
        let sc = &e.show.scripts[0];
        assert_eq!(sc.id, 1);
        let ids: Vec<u32> = sc.blocks.iter().map(|b| b.id).collect();
        assert_eq!(ids, vec![1, 2]);
        let ms: Vec<(u32, u32)> = sc.blocks[0].marks.iter().map(|m| (m.id, m.at_word)).collect();
        assert_eq!(ms, vec![(1, 2), (2, 1)]); // unique ids, at_word clamped to the last word
    }

    #[test]
    fn save_validates_actions() {
        let mut e = engine();
        for bad in [
            ScriptAction::Console { text: "wibble wobble".into() },
            ScriptAction::Osc { host: "".into(), port: 1, address: "/a".into(), args: vec![] },
            ScriptAction::Osc { host: "h".into(), port: 1, address: "nope".into(), args: vec![] },
            ScriptAction::Midi { port: None, kind: MidiKind::Note, channel: 17, number: 1, value: 1 },
            ScriptAction::Http { method: "TRACE".into(), url: "http://x".into(), body: "".into() },
            ScriptAction::Http { method: "GET".into(), url: "ftp://x".into(), body: "".into() },
        ] {
            let s = script(vec![block(1, "a", vec![mark(1, 0, vec![bad.clone()])])]);
            assert!(e.handle(Command::ScriptSave { script: s }).is_err(), "{bad:?}");
        }
        assert!(e.show.scripts.is_empty());
    }

    #[test]
    fn go_back_goto_reset_pointer() {
        let mut e = engine();
        let s = script(vec![
            block(1, "uno dos", vec![mark(1, 0, vec![]), mark(2, 1, vec![])]),
            block(2, "tres", vec![]),
            block(3, "cuatro", vec![mark(3, 0, vec![])]),
        ]);
        e.handle(Command::ScriptSave { script: s }).unwrap();
        assert_eq!(rt(&e, 1).next, Some((1, 1)));
        e.handle(Command::ScriptGo { id: 1 }).unwrap();
        let r = rt(&e, 1);
        assert_eq!((r.block, r.mark, r.next), (1, Some(1), Some((1, 2))));
        assert_eq!(r.last_label, "m1");
        e.handle(Command::ScriptGo { id: 1 }).unwrap();
        e.handle(Command::ScriptGo { id: 1 }).unwrap();
        let r = rt(&e, 1);
        assert_eq!((r.block, r.mark, r.next), (3, Some(3), None));
        assert!(e.handle(Command::ScriptGo { id: 1 }).is_err(), "end of script");
        // back re-arms the last fired mark without firing
        e.handle(Command::ScriptBack { id: 1 }).unwrap();
        let r = rt(&e, 1);
        assert_eq!((r.block, r.mark, r.next), (1, Some(2), Some((3, 3))));
        // goto a mark: next fires it
        e.handle(Command::ScriptGoto { id: 1, block: 1, mark: Some(2) }).unwrap();
        assert_eq!(rt(&e, 1).next, Some((1, 2)));
        // goto a block start: next = first mark at/after the block
        e.handle(Command::ScriptGoto { id: 1, block: 2, mark: None }).unwrap();
        let r = rt(&e, 1);
        assert_eq!((r.block, r.next), (2, Some((3, 3))));
        assert!(e.handle(Command::ScriptGoto { id: 1, block: 9, mark: None }).is_err());
        assert!(e.handle(Command::ScriptGoto { id: 1, block: 1, mark: Some(3) }).is_err());
        e.handle(Command::ScriptReset { id: 1 }).unwrap();
        let r = rt(&e, 1);
        assert_eq!((r.block, r.mark, r.next), (0, None, Some((1, 1))));
    }

    #[test]
    fn marks_sort_by_word_inside_a_block() {
        let mut e = engine();
        let s = script(vec![block(1, "a b c d", vec![mark(1, 3, vec![]), mark(2, 0, vec![]), mark(3, 3, vec![])])]);
        e.handle(Command::ScriptSave { script: s }).unwrap();
        let list = flat(&e.show.scripts[0]);
        assert_eq!(list, vec![(1, 2), (1, 1), (1, 3)]);
    }

    #[test]
    fn fire_runs_link_and_console_actions() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        e.handle(Command::CuelistSave {
            cuelist: Cuelist { id: 1, name: "L".into(), cues: vec![], looped: false, tracking: true, priority: 0, auto_mark: false, rate: 1.0, release_others: false, release_at_end: false },
        })
        .unwrap();
        set(&mut e, 1, "dimmer", 0.5);
        e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: "".into(), release: true, refs: true }).unwrap();
        let mut m = mark(1, 0, vec![con("fixture 1 at 100")]);
        m.link = Some(CueLink { cuelist: 1, number: 1.0 });
        e.handle(Command::ScriptSave { script: script(vec![block(1, "x", vec![m])]) }).unwrap();
        e.handle(Command::ScriptGo { id: 1 }).unwrap();
        assert_eq!(e.runs.get(&1).and_then(|r| r.current), Some(1.0), "linked cue started");
        assert_eq!(e.programmer.get(&1).and_then(|a| a.get("dimmer")).copied(), Some(1.0), "console action ran");
        let last = e.auto.log.0.back().unwrap();
        assert!(last.ok && last.text.contains("m1"), "{last:?}");
    }

    #[test]
    fn a_failing_action_does_not_stop_the_others() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        // second action is valid at save time but fails at run time (selection needs fixtures that exist)
        let s = script(vec![block(1, "x", vec![mark(1, 0, vec![con("fixture 9 at 50"), con("fixture 1 at 40")])])]);
        e.handle(Command::ScriptSave { script: s }).unwrap();
        e.handle(Command::ScriptGo { id: 1 }).unwrap();
        assert_eq!(e.programmer.get(&1).and_then(|a| a.get("dimmer")).copied(), Some(0.4));
        let last = e.auto.log.0.back().unwrap();
        assert!(!last.ok && last.msg.contains("action 1"), "{last:?}");
        assert_eq!(rt(&e, 1).block, 1, "pointer still advanced");
    }

    #[test]
    fn follow_jumps_without_firing_and_ignores_own_cues() {
        let mut e = engine();
        e.handle(Command::CuelistSave {
            cuelist: Cuelist { id: 1, name: "L".into(), cues: vec![], looped: false, tracking: true, priority: 0, auto_mark: false, rate: 1.0, release_others: false, release_at_end: false },
        })
        .unwrap();
        for n in 1..=3 {
            e.handle(Command::CueSave {
                cuelist: 1,
                cue: Cue {
                    number: n as f64, label: String::new(), fade_in: 0.0, fade_out: 0.0, delay: 0.0, follow: None, timecode: None,
                    changes: PerFixture::new(), block: false, refs: Default::default(), timing: Default::default(),
                    curve: Default::default(), stagger: 0.0, notes: String::new(),
                },
            })
            .unwrap();
        }
        let linked = |id: u32, n: f64| {
            let mut m = mark(id, 0, vec![con("clear")]);
            m.link = Some(CueLink { cuelist: 1, number: n });
            m
        };
        let s = script(vec![block(1, "a", vec![linked(1, 1.0)]), block(2, "b", vec![linked(2, 2.0)]), block(3, "c", vec![linked(3, 3.0)])]);
        e.handle(Command::ScriptSave { script: s }).unwrap();
        e.handle(Command::ScriptFollow { id: 1, on: true }).unwrap();
        assert!(rt(&e, 1).follow);
        // a cue started from outside moves the reader to its mark, nothing fires
        e.handle(Command::GoTo { cuelist: 1, number: 2.0 }).unwrap();
        let r = rt(&e, 1);
        assert_eq!((r.block, r.mark, r.next), (2, Some(2), Some((3, 3))));
        assert!(e.auto.log.0.iter().all(|l| !l.text.starts_with("script")), "follow must not fire");
        // firing a mark moves the reader exactly once (its own cue does not re-trigger follow)
        e.handle(Command::ScriptGo { id: 1 }).unwrap();
        let r = rt(&e, 1);
        assert_eq!((r.block, r.mark, r.next), (3, Some(3), None));
        // follow off: external cues leave the reader alone
        e.handle(Command::ScriptFollow { id: 1, on: false }).unwrap();
        e.handle(Command::GoTo { cuelist: 1, number: 1.0 }).unwrap();
        assert_eq!(rt(&e, 1).block, 3);
    }

    #[test]
    fn osc_action_sends_typed_args() {
        let sink = UdpSocket::bind("127.0.0.1:0").unwrap();
        sink.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let port = sink.local_addr().unwrap().port();
        let mut e = engine();
        let a = ScriptAction::Osc {
            host: "127.0.0.1".into(),
            port,
            address: "/cue/7/start".into(),
            args: vec!["3".into(), "0.5".into(), "true".into(), "hola".into(), "s:007".into()],
        };
        e.script_action(&a).unwrap();
        let mut buf = [0u8; 512];
        let (n, _) = sink.recv_from(&mut buf).unwrap();
        let (_, pkt) = rosc::decoder::decode_udp(&buf[..n]).unwrap();
        let rosc::OscPacket::Message(m) = pkt else { panic!("not a message") };
        assert_eq!(m.addr, "/cue/7/start");
        assert_eq!(
            m.args,
            vec![
                rosc::OscType::Int(3),
                rosc::OscType::Float(0.5),
                rosc::OscType::Bool(true),
                rosc::OscType::String("hola".into()),
                rosc::OscType::String("007".into())
            ]
        );
    }

    #[test]
    fn midi_action_goes_to_the_feedback_channel_when_no_native_port() {
        let mut e = engine();
        let mut rx = e.out.subscribe();
        e.script_action(&ScriptAction::Midi { port: Some("Pad".into()), kind: MidiKind::Note, channel: 2, number: 60, value: 100 }).unwrap();
        e.script_action(&ScriptAction::Midi { port: None, kind: MidiKind::Program, channel: 1, number: 5, value: 0 }).unwrap();
        let got = drain_midi(&mut rx);
        assert_eq!(got, vec![(Some("Pad".to_string()), vec![0x91, 60, 100]), (None, vec![0xC0, 5])]);
    }

    #[tokio::test]
    async fn http_action_posts_and_reports_failures() {
        use axum::{routing::post, Router};
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, String)>();
        let app = Router::new().route(
            "/go",
            post(move |headers: axum::http::HeaderMap, body: String| {
                let tx = tx.clone();
                async move {
                    let ct = headers.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                    let _ = tx.send((ct, body));
                    "ok"
                }
            }),
        );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        let mut e = engine();
        e.script_action(&ScriptAction::Http { method: "post".into(), url: format!("http://127.0.0.1:{port}/go"), body: "{\"cue\":1}".into() }).unwrap();
        let (ct, body) = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap();
        assert_eq!(ct, "application/json");
        assert_eq!(body, "{\"cue\":1}");
        // an unreachable target is reported asynchronously on the log channel, never as a hard failure
        let mut logs = e.out.subscribe();
        e.script_action(&ScriptAction::Http { method: "GET".into(), url: "http://127.0.0.1:1/".into(), body: String::new() }).unwrap();
        let msg = tokio::time::timeout(Duration::from_secs(6), async {
            loop {
                if let Ok(Out::Json(j)) = logs.recv().await {
                    // skip the success line of the first request
                    if let Ok(ServerMsg::Log { level, msg }) = serde_json::from_str::<ServerMsg>(&j) {
                        if level == "error" {
                            return (level, msg);
                        }
                    }
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(msg.0, "error");
    }

    #[test]
    fn osc_arg_types() {
        use rosc::OscType::*;
        assert_eq!(osc_arg("12"), Int(12));
        assert_eq!(osc_arg("-1.5"), Float(-1.5));
        assert_eq!(osc_arg("false"), Bool(false));
        assert_eq!(osc_arg("inf"), String("inf".into()));
        assert_eq!(osc_arg("i:7"), Int(7));
        assert_eq!(osc_arg("f:2"), Float(2.0));
        assert_eq!(osc_arg("s:12"), String("12".into()));
        assert_eq!(osc_arg("cue-1"), String("cue-1".into()));
    }

    #[test]
    fn undo_restores_scripts_and_remove_clears_state() {
        let mut e = engine();
        e.handle(Command::ScriptSave { script: script(vec![block(1, "a", vec![mark(1, 0, vec![])])]) }).unwrap();
        e.handle(Command::ScriptGo { id: 1 }).unwrap();
        e.handle(Command::ScriptRemove { id: 1 }).unwrap();
        assert!(e.show.scripts.is_empty() && e.script.runs.is_empty());
        e.handle(Command::Undo).unwrap();
        assert_eq!(e.show.scripts.len(), 1);
    }
}
