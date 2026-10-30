//! Script (guion / escaleta) import and cue-mark suggestions.
//!
//! [`import`] turns a `.pdf`, `.txt`, `.md` or `.fountain` file into draft [`ScriptBlock`]s using heuristics
//! tuned for Spanish theatre (`ACTO I`, `ESCENA 2`, `JUAN.- Hola`, `DOÑA INÉS: …`, `(Entra Pedro.)`) that also cope
//! with Fountain and Markdown. Nothing is saved: the UI shows the draft and the user edits it.
//! [`suggest`] scans blocks for lighting / sound / video / set keywords and proposes cue marks.

use crate::model::*;
use std::collections::HashSet;
use ts_rs::TS;

#[derive(Debug)]
pub enum ImportError {
    /// File type we do not read (HTTP 415).
    Unsupported(String),
    /// A PDF with no text layer (scanned) — needs OCR (HTTP 422).
    Scanned,
    /// Broken file or nothing recognisable inside (HTTP 422).
    Unreadable(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Unsupported(e) => write!(f, "Unsupported file type '{e}' (use .pdf, .txt, .md or .fountain)"),
            ImportError::Scanned => write!(f, "The PDF has no text layer (probably a scan). Run OCR on it first, or paste the text."),
            ImportError::Unreadable(m) => write!(f, "{m}"),
        }
    }
}

const MAX_BLOCKS: usize = 5000;
const MAX_TEXT: usize = 6000;
/// Names that look like a speaker but are just shouted words.
const NOT_NAMES: &[&str] = &["NO", "SI", "SÍ", "YA", "Y", "A", "O", "EL", "LA", "LOS", "LAS", "UN", "UNA", "OH", "AH", "HA", "VAYA", "BUENO", "BIEN", "FIN"];
const HEADING_WORDS: &[&str] = &["acto", "escena", "cuadro", "jornada", "prologo", "epilogo", "intermedio", "act", "scene", "prologue", "epilogue", "interlude"];
const ORDINALS: &[&str] = &[
    "primero", "primera", "segundo", "segunda", "tercero", "tercera", "cuarto", "cuarta", "quinto", "quinta", "unico", "unica", "final", "first", "second",
    "third", "fourth", "fifth", "one", "two", "three", "four", "five",
];

pub fn fold(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    match c {
        'á' | 'à' | 'ä' | 'â' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        _ => c,
    }
}

fn fold_str(s: &str) -> String {
    s.chars().map(fold).collect()
}

// ───────────────────────────── text extraction ─────────────────────────────

/// Page texts of the file (one entry per page; plain text files use form feeds as page breaks).
fn pages_of(filename: &str, bytes: &[u8]) -> Result<(Vec<String>, bool), ImportError> {
    let ext = filename.rsplit('.').next().map(|e| e.to_ascii_lowercase()).filter(|_| filename.contains('.')).unwrap_or_default();
    match ext.as_str() {
        "pdf" => {
            let data = bytes.to_vec();
            let pages = std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem_by_pages(&data))
                .map_err(|_| ImportError::Unreadable("The PDF could not be read (unsupported or damaged file).".into()))?
                .map_err(|e| ImportError::Unreadable(format!("The PDF could not be read: {e}")))?;
            let letters: usize = pages.iter().map(|p| p.chars().filter(|c| c.is_alphanumeric()).count()).sum();
            if letters < 20 {
                return Err(ImportError::Scanned);
            }
            Ok((pages, true))
        }
        "txt" | "text" | "md" | "markdown" | "fountain" | "spmd" | "" => {
            let mut s = match std::str::from_utf8(bytes) {
                Ok(s) => s.to_string(),
                // legacy Windows-1252 / Latin-1 exports
                Err(_) => bytes.iter().map(|&b| b as char).collect(),
            };
            if s.starts_with('\u{feff}') {
                s.remove(0);
            }
            if s.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
                return Err(ImportError::Unreadable("The file is empty.".into()));
            }
            Ok((s.split('\x0c').map(|p| p.to_string()).collect(), false))
        }
        other => Err(ImportError::Unsupported(other.to_string())),
    }
}

struct Line {
    text: String,
    page: u32,
}

fn clean_line(raw: &str) -> String {
    let mut s = String::with_capacity(raw.len());
    let mut space = false;
    for c in raw.chars() {
        if c.is_whitespace() || c == '\u{a0}' {
            space = true;
        } else if c.is_control() || c == '\u{fffd}' {
            continue;
        } else {
            if space && !s.is_empty() {
                s.push(' ');
            }
            space = false;
            s.push(c);
        }
    }
    s
}

fn digits_stripped(s: &str) -> String {
    s.chars().filter(|c| !c.is_ascii_digit()).collect::<String>().trim().to_string()
}

/// Lines with page numbers; repeating headers / footers and bare page numbers of PDFs are dropped.
fn lines_of(pages: &[String], is_pdf: bool, warnings: &mut Vec<String>) -> Vec<Line> {
    let per_page: Vec<Vec<String>> = pages.iter().map(|p| p.lines().map(clean_line).collect()).collect();
    let mut repeating: HashSet<String> = HashSet::new();
    if is_pdf && pages.len() >= 3 {
        let mut count = std::collections::HashMap::<String, usize>::new();
        for lines in &per_page {
            let non_empty: Vec<&String> = lines.iter().filter(|l| !l.is_empty()).collect();
            let mut edge: Vec<&String> = non_empty.iter().take(2).copied().collect();
            edge.extend(non_empty.iter().rev().take(2).copied());
            let mut seen = HashSet::new();
            for l in edge {
                let k = digits_stripped(l);
                if k.chars().count() >= 3 && seen.insert(k.clone()) {
                    *count.entry(k).or_default() += 1;
                }
            }
        }
        let need = (pages.len() / 2).max(3);
        repeating = count.into_iter().filter(|(_, n)| *n >= need).map(|(k, _)| k).collect();
    }
    let mut dropped = 0;
    let mut out = Vec::new();
    for (pi, lines) in per_page.iter().enumerate() {
        let page = if pages.len() > 1 || is_pdf { pi as u32 + 1 } else { 0 };
        let first = lines.iter().position(|l| !l.is_empty());
        let last = lines.iter().rposition(|l| !l.is_empty());
        for (li, l) in lines.iter().enumerate() {
            let edge = Some(li) == first || Some(li) == last || lines.get(li + 1).map_or(false, |n| n.is_empty() && li < first.unwrap_or(0) + 2);
            let bare_number = is_pdf && !l.is_empty() && l.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '.' || c == ' ') && l.len() <= 5 && edge;
            if bare_number || (is_pdf && edge && repeating.contains(&digits_stripped(l))) {
                dropped += 1;
                continue;
            }
            out.push(Line { text: l.clone(), page });
        }
        out.push(Line { text: String::new(), page });
    }
    if dropped > 0 {
        warnings.push(format!("{dropped} header / footer / page-number lines were skipped."));
    }
    out
}

// ───────────────────────────── line classification ─────────────────────────────

fn is_roman(w: &str) -> bool {
    !w.is_empty() && w.len() <= 6 && w.chars().all(|c| matches!(c.to_ascii_lowercase(), 'i' | 'v' | 'x' | 'l' | 'c'))
}

fn is_upper(s: &str) -> bool {
    let mut letters = 0;
    for c in s.chars() {
        if c.is_lowercase() {
            return false;
        }
        if c.is_uppercase() {
            letters += 1;
        }
    }
    letters > 0
}

fn is_heading(line: &str) -> Option<String> {
    let t = line.trim();
    if let Some(h) = t.strip_prefix('#') {
        let h = h.trim_start_matches('#').trim();
        return (!h.is_empty()).then(|| h.to_string());
    }
    if let Some(h) = t.strip_prefix('.') {
        if h.chars().next().map_or(false, |c| c.is_alphanumeric()) {
            return Some(h.trim().to_string());
        }
    }
    let up = t.to_uppercase();
    if up.starts_with("INT.") || up.starts_with("EXT.") || up.starts_with("INT/EXT") || up.starts_with("EST.") || up.starts_with("I/E") {
        return Some(t.to_string());
    }
    let words: Vec<String> = t.split_whitespace().map(|w| fold_str(w.trim_matches(|c: char| !c.is_alphanumeric()))).collect();
    let first = words.first()?;
    if !HEADING_WORDS.contains(&first.as_str()) || t.chars().count() > 70 {
        return None;
    }
    let numbered = words.get(1).map_or(false, |w| w.chars().all(|c| c.is_ascii_digit()) && !w.is_empty() || is_roman(w) || ORDINALS.contains(&w.as_str()));
    if is_upper(t) || (numbered && t.chars().count() <= 60) {
        Some(t.trim_end_matches(['.', ':']).to_string())
    } else {
        None
    }
}

/// `NAME` / `NAME (extension)` where NAME is 1-4 upper-case words.
fn speaker_name(s: &str) -> Option<String> {
    let name = s.trim();
    if name.is_empty() || name.chars().count() > 32 || !is_upper(name) {
        return None;
    }
    if name.chars().any(|c| !(c.is_alphabetic() || c == ' ' || c == '\'' || c == '’' || c == '.' || c == '-')) {
        return None;
    }
    if name.split_whitespace().count() > 4 || NOT_NAMES.contains(&name) {
        return None;
    }
    Some(name.trim_end_matches('.').trim().to_string())
}

/// `(speaker, rest)` when the line starts with a speaker label: `JUAN.- Hola`, `DOÑA INÉS: …`, `JUAN (bajo). …`,
/// `JUAN — Hola`, or (with `known` speakers) `Juan: Hola`.
fn split_speaker(line: &str, known: &HashSet<String>) -> Option<(String, String, String)> {
    let t = line.trim();
    // longest label before the first terminator
    let mut idx = None;
    for (i, c) in t.char_indices() {
        if i > 48 {
            break;
        }
        let is_dash = matches!(c, '—' | '–') || (c == '-' && t[..i].ends_with(' '));
        if c == ':' || is_dash || (c == '.' && t[i + 1..].chars().next().map_or(true, |n| n == ' ' || n == '-' || n == '—' || n == '–' || n == '(')) {
            idx = Some(i);
            break;
        }
    }
    let i = idx?;
    let mut label = t[..i].trim();
    let c = t[i..].chars().next()?;
    let rest = t[i + c.len_utf8()..].trim_start_matches(|c: char| c == '-' || c == '—' || c == '–' || c == ' ').to_string();
    // optional "(extension)" glued to the label
    let mut ext = String::new();
    if let Some(p) = label.find('(') {
        if label.ends_with(')') {
            ext = label[p..].to_string();
            label = label[..p].trim();
        }
    }
    let name = match speaker_name(label) {
        Some(n) => n,
        None => {
            let key = fold_str(label.trim_end_matches('.'));
            if !key.is_empty() && key.split_whitespace().count() <= 4 && known.contains(&key) {
                label.trim_end_matches('.').to_uppercase()
            } else {
                return None;
            }
        }
    };
    Some((name, ext, rest))
}

/// A line that is just a speaker cue: `JUAN`, `JUAN (V.O.)`, `JUAN:`, `@Juan`.
fn cue_only(line: &str) -> Option<String> {
    let t = line.trim();
    if let Some(n) = t.strip_prefix('@') {
        return Some(n.trim().to_uppercase());
    }
    let t = t.trim_end_matches([':', '.']);
    let t = match (t.find('('), t.ends_with(')')) {
        (Some(p), true) => t[..p].trim(),
        _ => t,
    };
    speaker_name(t)
}

fn is_note(line: &str) -> Option<String> {
    let t = line.trim();
    if t.starts_with("[[") && t.ends_with("]]") {
        return Some(t.trim_start_matches('[').trim_end_matches(']').trim().to_string());
    }
    let f = fold_str(t);
    for p in ["nota:", "nota -", "notas:", "note:", "n.b.", "nb:"] {
        if f.starts_with(p) {
            return Some(t[p.len().min(t.len())..].trim_start_matches([':', '-', ' ']).trim().to_string());
        }
    }
    None
}

fn is_meta(line: &str) -> bool {
    let f = fold_str(line);
    ["title:", "author:", "credit:", "source:", "draft date:", "contact:", "copyright:", "titulo:", "autor:"].iter().any(|p| f.starts_with(p))
}

fn list_item(line: &str) -> Option<String> {
    let t = line.trim();
    for p in ["- ", "• ", "* ", "· "] {
        if let Some(r) = t.strip_prefix(p) {
            return Some(r.trim().to_string());
        }
    }
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() <= 3 {
        let r = &t[digits.len()..];
        if let Some(r) = r.strip_prefix(". ").or_else(|| r.strip_prefix(") ")) {
            return Some(r.trim().to_string());
        }
    }
    None
}

fn strip_markup(s: &str) -> String {
    let t = s.replace("**", "").replace('*', "");
    let t = t.trim();
    let t = t.strip_prefix("> ").unwrap_or(t);
    let t = t.strip_prefix('!').unwrap_or(t);
    t.trim().to_string()
}

// ───────────────────────────── block builder ─────────────────────────────

struct Builder {
    blocks: Vec<ScriptBlock>,
    cur: Option<ScriptBlock>,
    /// Speaker that continues after a parenthetical line inside their speech.
    resume: Option<String>,
    speakers: HashSet<String>,
}

impl Builder {
    fn new() -> Self {
        Builder { blocks: vec![], cur: None, resume: None, speakers: HashSet::new() }
    }

    fn flush(&mut self) {
        if let Some(mut b) = self.cur.take() {
            b.text = b.text.trim().to_string();
            if b.text.chars().count() > MAX_TEXT {
                b.text = b.text.chars().take(MAX_TEXT).collect();
            }
            if !b.text.is_empty() && self.blocks.len() < MAX_BLOCKS {
                b.id = self.blocks.len() as u32 + 1;
                self.blocks.push(b);
            }
        }
    }

    fn start(&mut self, kind: ScriptKind, speaker: &str, text: &str, page: u32) {
        self.flush();
        self.cur = Some(ScriptBlock { id: 0, kind, speaker: speaker.to_string(), text: text.to_string(), page, marks: vec![] });
    }

    fn append(&mut self, text: &str) {
        if let Some(b) = &mut self.cur {
            if !b.text.is_empty() {
                b.text.push(' ');
            }
            b.text.push_str(text);
        }
    }

    fn single(&mut self, kind: ScriptKind, speaker: &str, text: &str, page: u32) {
        self.start(kind, speaker, text, page);
        self.flush();
    }

    fn cur_kind(&self) -> Option<ScriptKind> {
        self.cur.as_ref().map(|b| b.kind)
    }
}

/// Split a leading `( … )` off `s` → `(inner, remainder)`.
fn leading_paren(s: &str) -> Option<(String, String)> {
    let t = s.trim_start();
    if !t.starts_with('(') {
        return None;
    }
    let mut depth = 0;
    for (i, c) in t.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((t[1..i].trim().to_string(), t[i + 1..].trim().to_string()));
                }
            }
            _ => {}
        }
    }
    None
}

fn build(lines: &[Line], known: &HashSet<String>, warnings: &mut Vec<String>) -> Builder {
    let mut b = Builder::new();
    let mut in_paren = false;
    let mut after_blank = true;
    let mut front_matter = true;
    for (i, line) in lines.iter().enumerate() {
        let raw = line.text.trim();
        if raw.is_empty() {
            b.flush();
            b.resume = None;
            in_paren = false;
            after_blank = true;
            continue;
        }
        if front_matter && is_meta(raw) {
            continue;
        }
        if raw.starts_with("===") {
            continue;
        }
        if raw.starts_with("= ") && after_blank {
            continue; // fountain synopsis
        }
        let text = strip_markup(raw);
        if text.is_empty() {
            continue;
        }
        let next_nonblank = lines.get(i + 1).map_or(false, |n| !n.text.trim().is_empty());
        let page = line.page;

        // multi-line parenthetical in progress
        if in_paren {
            b.append(&text);
            if text.contains(')') {
                in_paren = false;
                b.flush();
            }
            after_blank = false;
            continue;
        }
        if let Some(n) = is_note(&text) {
            b.single(ScriptKind::Note, "", &n, page);
            after_blank = false;
            continue;
        }
        if let Some(h) = is_heading(&text) {
            front_matter = false;
            b.single(ScriptKind::Heading, "", &h, page);
            after_blank = false;
            continue;
        }
        // stage direction in parentheses / brackets
        if text.starts_with('(') || text.starts_with('[') {
            front_matter = false;
            let resume = b.cur.as_ref().filter(|c| c.kind == ScriptKind::Speech).map(|c| c.speaker.clone());
            let closed = text.contains(')') || text.contains(']');
            if let Some((inner, rest)) = leading_paren(&text).filter(|(_, r)| !r.is_empty() && !r.starts_with('(')) {
                // "(Se levanta) Hola, Juan"
                let sp = resume.clone().or_else(|| b.resume.clone());
                b.single(ScriptKind::Direction, "", &format!("({inner})"), page);
                if let Some(sp) = sp {
                    b.start(ScriptKind::Speech, &sp, &rest, page);
                } else {
                    b.start(ScriptKind::Direction, "", &rest, page);
                }
            } else {
                b.start(ScriptKind::Direction, "", &text, page);
                if !closed {
                    in_paren = true;
                } else {
                    b.flush();
                }
                if resume.is_some() && !after_blank {
                    b.resume = resume;
                }
            }
            after_blank = false;
            continue;
        }
        // speaker label with text on the same line
        if let Some((name, ext, rest)) = split_speaker(&text, known) {
            front_matter = false;
            b.speakers.insert(fold_str(&name));
            b.resume = None;
            if let Some((dir, remainder)) = leading_paren(&rest) {
                b.single(ScriptKind::Direction, "", &format!("({dir})"), page);
                b.start(ScriptKind::Speech, &name, &format!("{ext} {remainder}").trim().to_string(), page);
            } else {
                b.start(ScriptKind::Speech, &name, &format!("{ext} {rest}").trim().to_string(), page);
            }
            after_blank = false;
            continue;
        }
        // fountain-style cue on its own line, dialogue below
        if after_blank && next_nonblank {
            if let Some(name) = cue_only(&text) {
                front_matter = false;
                b.speakers.insert(fold_str(&name));
                b.resume = None;
                b.start(ScriptKind::Speech, &name, "", page);
                after_blank = false;
                continue;
            }
        }
        // novel-style dialogue dash without a name
        if let Some(r) = text.strip_prefix('—').or_else(|| text.strip_prefix('–')) {
            front_matter = false;
            b.start(ScriptKind::Speech, "", r.trim(), page);
            after_blank = false;
            continue;
        }
        // list items (escaletas) are one block each
        if let Some(item) = list_item(&text) {
            front_matter = false;
            b.single(ScriptKind::Direction, "", &item, page);
            after_blank = false;
            continue;
        }
        // plain text: continues the open block, resumes an interrupted speech, or is a stage direction
        front_matter = false;
        match b.cur_kind() {
            Some(ScriptKind::Speech | ScriptKind::Direction | ScriptKind::Note) if !after_blank => b.append(&text),
            _ => {
                if let (Some(sp), false) = (b.resume.clone(), after_blank) {
                    b.start(ScriptKind::Speech, &sp, &text, page);
                } else {
                    b.start(ScriptKind::Direction, "", &text, page);
                }
            }
        }
        after_blank = false;
    }
    b.flush();
    if b.blocks.len() >= MAX_BLOCKS {
        warnings.push(format!("Only the first {MAX_BLOCKS} blocks were imported."));
    }
    b
}

/// Import a script file. `filename` decides the format.
pub fn import(filename: &str, bytes: &[u8]) -> Result<ScriptImport, ImportError> {
    let (pages, is_pdf) = pages_of(filename, bytes)?;
    let mut warnings = Vec::new();
    let lines = lines_of(&pages, is_pdf, &mut warnings);
    // pass 1 finds upper-case speakers; pass 2 also accepts "Juan: …" for those names
    let first = build(&lines, &HashSet::new(), &mut Vec::new());
    let known = first.speakers.clone();
    let done = build(&lines, &known, &mut warnings);
    let blocks = done.blocks;
    if blocks.is_empty() {
        return Err(if is_pdf { ImportError::Scanned } else { ImportError::Unreadable("No text found in the file.".into()) });
    }
    if done.speakers.is_empty() {
        warnings.push("No character names were detected: the text was imported as stage directions. Edit the block types if needed.".into());
    }
    let name = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .rsplit_once('.')
        .map_or(filename, |(n, _)| n)
        .replace(['_', '-'], " ")
        .trim()
        .to_string();
    Ok(ScriptImport {
        name: if name.is_empty() { "Guion".into() } else { name },
        blocks,
        pages: pages.len() as u32,
        warnings,
        pdf: None,
    })
}

// ───────────────────────────── suggestions ─────────────────────────────

/// A proposed cue mark (never applied automatically).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct ScriptSuggestion {
    pub block: u32,
    pub at_word: u32,
    pub kind: MarkKind,
    pub label: String,
    /// The words that triggered it.
    pub matched: String,
}

/// `(tokens, kind, label)`; a token ending in `*` matches by prefix. Tokens are lower-case, accent-free.
const KEYWORDS: &[(&[&str], MarkKind, &str)] = &[
    // light
    (&["apagon"], MarkKind::Light, "Apagón"),
    (&["oscur*"], MarkKind::Light, "Oscuro"),
    (&["blackout"], MarkKind::Light, "Blackout"),
    (&["se", "apaga*"], MarkKind::Light, "Se apaga la luz"),
    (&["se", "apagan"], MarkKind::Light, "Se apagan las luces"),
    (&["se", "enciende*"], MarkKind::Light, "Se enciende la luz"),
    (&["luz"], MarkKind::Light, "Luz"),
    (&["luces"], MarkKind::Light, "Luces"),
    (&["ilumina*"], MarkKind::Light, "Iluminación"),
    (&["relampago*"], MarkKind::Light, "Relámpago"),
    (&["amanece*"], MarkKind::Light, "Amanecer"),
    (&["anochece*"], MarkKind::Light, "Anochecer"),
    (&["atardece*"], MarkKind::Light, "Atardecer"),
    (&["penumbra"], MarkKind::Light, "Penumbra"),
    (&["foco"], MarkKind::Light, "Foco"),
    (&["fundido"], MarkKind::Light, "Fundido"),
    (&["lights"], MarkKind::Light, "Lights"),
    (&["lightning"], MarkKind::Light, "Lightning"),
    (&["dark"], MarkKind::Light, "Dark"),
    (&["fade*"], MarkKind::Light, "Fade"),
    // sound
    (&["suena*"], MarkKind::Sound, "Suena"),
    (&["sonido*"], MarkKind::Sound, "Sonido"),
    (&["musica*"], MarkKind::Sound, "Música"),
    (&["cancion"], MarkKind::Sound, "Canción"),
    (&["telefono*"], MarkKind::Sound, "Teléfono"),
    (&["timbre"], MarkKind::Sound, "Timbre"),
    (&["portazo"], MarkKind::Sound, "Portazo"),
    (&["llaman*", "a", "la", "puerta"], MarkKind::Sound, "Llaman a la puerta"),
    (&["golpes", "en", "la", "puerta"], MarkKind::Sound, "Golpes en la puerta"),
    (&["disparo*"], MarkKind::Sound, "Disparo"),
    (&["trueno*"], MarkKind::Sound, "Trueno"),
    (&["aplauso*"], MarkKind::Sound, "Aplausos"),
    (&["campana*"], MarkKind::Sound, "Campanas"),
    (&["sirena*"], MarkKind::Sound, "Sirena"),
    (&["explosion*"], MarkKind::Sound, "Explosión"),
    (&["ruido*"], MarkKind::Sound, "Ruido"),
    (&["se", "oye*"], MarkKind::Sound, "Se oye"),
    (&["se", "escucha*"], MarkKind::Sound, "Se escucha"),
    (&["voz", "en", "off"], MarkKind::Sound, "Voz en off"),
    (&["music"], MarkKind::Sound, "Music"),
    (&["sound*"], MarkKind::Sound, "Sound"),
    (&["rings"], MarkKind::Sound, "Rings"),
    (&["thunder"], MarkKind::Sound, "Thunder"),
    // video
    (&["video*"], MarkKind::Video, "Vídeo"),
    (&["proyecta*"], MarkKind::Video, "Proyección"),
    (&["proyeccion*"], MarkKind::Video, "Proyección"),
    (&["pantalla*"], MarkKind::Video, "Pantalla"),
    (&["pelicula*"], MarkKind::Video, "Película"),
    (&["grabacion"], MarkKind::Video, "Grabación"),
    (&["projection"], MarkKind::Video, "Projection"),
    (&["footage"], MarkKind::Video, "Footage"),
    // set / scene
    (&["telon*"], MarkKind::Set, "Telón"),
    (&["decorado*"], MarkKind::Set, "Decorado"),
    (&["cambio", "de", "escena"], MarkKind::Set, "Cambio de escena"),
    (&["escenografia"], MarkKind::Set, "Escenografía"),
    (&["curtain*"], MarkKind::Set, "Curtain"),
];

fn tok_matches(pat: &str, tok: &str) -> bool {
    match pat.strip_suffix('*') {
        Some(p) => tok.starts_with(p),
        None => tok == pat,
    }
}

fn tokens(text: &str) -> Vec<String> {
    text.split_whitespace().map(|w| fold_str(w.trim_matches(|c: char| !c.is_alphanumeric()))).collect()
}

/// The text of `(…)` groups inside `s`, with the word offset of each group's first word.
fn parentheticals(s: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let mut open_at = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => {
                if depth == 0 {
                    start = i + 1;
                    open_at = i;
                }
                depth += 1;
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push((s[..open_at].split_whitespace().count(), s[start..i].to_string()));
                }
            }
            _ => {}
        }
    }
    out
}

/// Keyword-based mark suggestions. Stage directions, notes and headings are scanned in full; for spoken text only
/// the words inside parentheses count (ordinary dialogue says "luz" all the time). At most one suggestion per
/// block and kind, and none for kinds the block already has a mark for.
pub fn suggest(blocks: &[ScriptBlock]) -> Vec<ScriptSuggestion> {
    let mut out = Vec::new();
    for b in blocks {
        let mut windows: Vec<(usize, String)> = Vec::new();
        match b.kind {
            ScriptKind::Speech => windows.extend(parentheticals(&b.text)),
            ScriptKind::Heading => {
                if is_heading(&b.text).is_some() {
                    out.push(ScriptSuggestion { block: b.id, at_word: 0, kind: MarkKind::Set, label: b.text.clone(), matched: b.text.clone() });
                }
                continue;
            }
            _ => windows.push((0, b.text.clone())),
        }
        let mut seen: Vec<MarkKind> = b.marks.iter().map(|m| m.kind).collect();
        let mut found: Vec<ScriptSuggestion> = Vec::new();
        for (offset, text) in windows {
            let toks = tokens(&text);
            for i in 0..toks.len() {
                for (pat, kind, label) in KEYWORDS {
                    if seen.contains(kind) || i + pat.len() > toks.len() {
                        continue;
                    }
                    if pat.iter().enumerate().all(|(k, p)| tok_matches(p, &toks[i + k])) {
                        seen.push(*kind);
                        found.push(ScriptSuggestion {
                            block: b.id,
                            at_word: (offset + i) as u32,
                            kind: *kind,
                            label: (*label).to_string(),
                            matched: text.split_whitespace().skip(i).take(pat.len()).collect::<Vec<_>>().join(" "),
                        });
                    }
                }
            }
        }
        found.sort_by_key(|s| s.at_word);
        out.extend(found);
        if out.len() >= 500 {
            break;
        }
    }
    out.truncate(500);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAY: &str = "\
ACTO I
ESCENA 1

(Una cocina oscura. Suena un teléfono. Entra DOÑA INÉS.)

DOÑA INÉS.- ¡Qué hora es! Nadie
contesta en esta casa.
(Descuelga.) ¿Diga?
JUAN (desde dentro): Soy yo.
(Pausa.)
Abre la puerta, por favor.

Se apagan las luces y cae el telón.

ESCENA II
JUAN. Buenas noches.
DOÑA INÉS: Buenas.
Juan: ¿Cenamos?
NOTA: aquí entra el vídeo de la tormenta
";

    fn kinds(s: &ScriptImport) -> Vec<(ScriptKind, String, String)> {
        s.blocks.iter().map(|b| (b.kind, b.speaker.clone(), b.text.clone())).collect()
    }

    #[test]
    fn spanish_play_golden() {
        let r = import("la_casa.txt", PLAY.as_bytes()).unwrap();
        assert_eq!(r.name, "la casa");
        use ScriptKind::*;
        let got = kinds(&r);
        let want = vec![
            (Heading, "", "ACTO I"),
            (Heading, "", "ESCENA 1"),
            (Direction, "", "(Una cocina oscura. Suena un teléfono. Entra DOÑA INÉS.)"),
            (Speech, "DOÑA INÉS", "¡Qué hora es! Nadie contesta en esta casa."),
            (Direction, "", "(Descuelga.)"),
            (Speech, "DOÑA INÉS", "¿Diga?"),
            (Speech, "JUAN", "(desde dentro) Soy yo."),
            (Direction, "", "(Pausa.)"),
            (Speech, "JUAN", "Abre la puerta, por favor."),
            (Direction, "", "Se apagan las luces y cae el telón."),
            (Heading, "", "ESCENA II"),
            (Speech, "JUAN", "Buenas noches."),
            (Speech, "DOÑA INÉS", "Buenas."),
            (Speech, "JUAN", "¿Cenamos?"),
            (Note, "", "aquí entra el vídeo de la tormenta"),
        ];
        let want: Vec<_> = want.into_iter().map(|(k, s, t)| (k, s.to_string(), t.to_string())).collect();
        assert_eq!(got, want);
        assert_eq!(r.blocks.iter().map(|b| b.id).collect::<Vec<_>>(), (1..=15).collect::<Vec<_>>());
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn fountain_cues_and_forced_elements() {
        let src = "Title: Prueba\nAuthor: Ana\n\n.EL PATIO\n\n@Ana\n(susurrando)\nHola, ¿hay alguien?\n\nLUIS\nSí.\n\n!Cae la noche.\n\n[[comprobar sonido]]\n";
        let r = import("prueba.fountain", src.as_bytes()).unwrap();
        use ScriptKind::*;
        let got = kinds(&r);
        assert_eq!(got[0], (Heading, "".into(), "EL PATIO".into()));
        assert_eq!(got[1].0, Direction);
        assert_eq!(got[1].2, "(susurrando)");
        assert_eq!(got[2], (Speech, "ANA".into(), "Hola, ¿hay alguien?".into()));
        assert_eq!(got[3], (Speech, "LUIS".into(), "Sí.".into()));
        assert_eq!(got[4], (Direction, "".into(), "Cae la noche.".into()));
        assert_eq!(got[5], (Note, "".into(), "comprobar sonido".into()));
        assert_eq!(got.len(), 6);
    }

    #[test]
    fn markdown_headings_and_escaleta_lists() {
        let src = "# Acto 1\n\n- Entra Ana por la izquierda\n- Suena el teléfono\n2. Apagón\n";
        let r = import("escaleta.md", src.as_bytes()).unwrap();
        assert_eq!(r.blocks[0].kind, ScriptKind::Heading);
        assert_eq!(r.blocks.len(), 4);
        assert_eq!(r.blocks[3].text, "Apagón");
        assert!(r.warnings.iter().any(|w| w.contains("No character names")));
    }

    #[test]
    fn latin1_text_is_decoded() {
        let bytes: Vec<u8> = "ANA: ¡Canción!\n".chars().map(|c| c as u32 as u8).collect();
        let r = import("a.txt", &bytes).unwrap();
        assert_eq!(r.blocks[0].text, "¡Canción!");
    }

    #[test]
    fn rejects_unknown_and_empty() {
        assert!(matches!(import("a.docx", b"x"), Err(ImportError::Unsupported(_))));
        assert!(matches!(import("a.txt", b"  \n "), Err(ImportError::Unreadable(_))));
        assert!(matches!(import("a.pdf", b"not a pdf"), Err(ImportError::Unreadable(_))));
    }

    fn pdf_of(pages: &[Vec<&str>]) -> Vec<u8> {
        let mut d = crate::pdf::Doc::new("Obra");
        for lines in pages {
            let p = d.add_page();
            let mut y = 780.0;
            for l in lines {
                d.pages[p].text(60.0, y, 11.0, false, l);
                y -= 16.0;
            }
        }
        d.finish()
    }

    #[test]
    fn pdf_import_keeps_pages_and_drops_headers() {
        let pdf = pdf_of(&[
            vec!["LA CASA DE LOS TEATROS", "ACTO I", "ANA.- Buenas tardes.", "LUIS.- Buenas.", "1"],
            vec!["LA CASA DE LOS TEATROS", "(Se apaga la luz.)", "ANA.- Qué oscuro.", "2"],
            vec!["LA CASA DE LOS TEATROS", "LUIS.- Enciende la vela.", "ESCENA 2", "3"],
        ]);
        let r = import("obra.pdf", &pdf).unwrap();
        assert_eq!(r.pages, 3);
        assert!(r.warnings.iter().any(|w| w.contains("header")), "{:?}", r.warnings);
        let got: Vec<_> = r.blocks.iter().map(|b| (b.kind, b.speaker.as_str(), b.text.as_str(), b.page)).collect();
        use ScriptKind::*;
        assert_eq!(
            got,
            vec![
                (Heading, "", "ACTO I", 1),
                (Speech, "ANA", "Buenas tardes.", 1),
                (Speech, "LUIS", "Buenas.", 1),
                (Direction, "", "(Se apaga la luz.)", 2),
                (Speech, "ANA", "Qué oscuro.", 2),
                (Speech, "LUIS", "Enciende la vela.", 3),
                (Heading, "", "ESCENA 2", 3),
            ]
        );
    }

    #[test]
    fn scanned_pdf_is_reported() {
        let pdf = pdf_of(&[vec![], vec![]]);
        assert!(matches!(import("scan.pdf", &pdf), Err(ImportError::Scanned)));
    }

    #[test]
    fn suggestions_by_keyword() {
        let r = import("la_casa.txt", PLAY.as_bytes()).unwrap();
        let s = suggest(&r.blocks);
        let find = |b: u32, k: MarkKind| s.iter().find(|x| x.block == b && x.kind == k);
        // block 3: "(Una cocina oscura. Suena un teléfono. …)"
        let b3 = find(3, MarkKind::Sound).expect("sound in the first direction");
        assert_eq!(b3.at_word, 3);
        assert!(b3.matched.to_lowercase().starts_with("suena"));
        // "Se apagan las luces y cae el telón"
        assert_eq!(find(10, MarkKind::Light).unwrap().at_word, 0);
        assert_eq!(find(10, MarkKind::Set).unwrap().label, "Telón");
        // scene headings propose a scene-change mark; plain dialogue about the light does not
        assert!(find(1, MarkKind::Set).is_some());
        let talk = vec![ScriptBlock { id: 1, kind: ScriptKind::Speech, speaker: "ANA".into(), text: "La luz de tus ojos".into(), page: 0, marks: vec![] }];
        assert!(suggest(&talk).is_empty());
        let talk = vec![ScriptBlock { id: 1, kind: ScriptKind::Speech, speaker: "ANA".into(), text: "Dime (suena el timbre) algo".into(), page: 0, marks: vec![] }];
        let s = suggest(&talk);
        assert_eq!((s[0].kind, s[0].at_word), (MarkKind::Sound, 1));
    }

    #[test]
    fn suggestions_skip_kinds_already_marked() {
        let mark = CueMark { id: 1, label: "x".into(), kind: MarkKind::Light, at_word: 0, actions: vec![], link: None, standby_blocks: 0, note: String::new() };
        let b = ScriptBlock { id: 1, kind: ScriptKind::Direction, speaker: String::new(), text: "Se apaga la luz y suena una campana".into(), page: 0, marks: vec![mark] };
        let s = suggest(&[b]);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].kind, MarkKind::Sound);
    }
}
