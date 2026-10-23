//! Console-style command line.
//!
//! `Command::CommandLine { text }` parses `text` into statements (separated by `;` or newlines),
//! resolves each one against the live show into ordinary [`Command`]s and executes them in order.
//! Keywords are case-insensitive and accent-insensitive; every verb has Spanish aliases. `#` starts a
//! comment. Results are appended to `Runtime.cmd_log`.
//!
//! Selection is part of the *interpreter session* (one `CommandLine` text, or one macro run):
//! `fixture 1 thru 4 at 50; red at 100` works inside a single text.
//!
//! ```text
//! SELECTION   fixture|fx|aparato 1 thru 10 + 15 - 4     group|grupo N     all|todos     none
//!             odd|impares   even|pares   (filter the selection built so far, or all fixtures)
//!             next|siguiente   prev|anterior          bare ranges work too:  1 thru 8
//! VALUES      [<sel>] at|@|en 50          full|max|pleno  out|off|cero  half|mitad  home|reposo
//!             [<sel>] at 0 thru 100       (fan: spreads the values across the selection)
//!             [attr] [<sel>] at <value>   dimmer 1 thru 8 at 70      red at 100     pan at 30
//!             <sel> colour|color red|rojo|…   or  colour <r> <g> <b>   (percent)
//!             <sel> locate|identify [on|off]
//! RECORD      record|grabar cue N [cuelist L] [name X] [replace|merge] [release]
//!             record palette [intensity|position|color|beam|all] [N] [name]
//!             record scene [N] [name] [output]          record group N [name]
//!             update cue [N] [cuelist L] [replace|track|cueonly]
//!             delete|borrar cue|cuelist|group|palette|scene|effect|macro|chase|timeline N [cuelist L]
//!             apply|aplicar palette N
//! PLAYBACK    go|back|pause|release [cuelist L | L]     cuelist L (sets the default list)
//!             goto [cue] N [cuelist L]                   release all
//!             scene|escena N [on|off|solo|toggle|at V]   scene solo N     scenes release
//!             chase N [on|off|tap|bpm V|next|prev]       macro N | macro stop [N]
//!             timeline N [run [from T]|stop|seek T]
//! SYSTEM      blackout|bo [on|off|toggle]   gm|master 80   clear|clr   park U/C at V   unpark [U/C]
//!             undo|deshacer   redo|rehacer   blind [on|off]
//! ```

use super::Engine;
use crate::model::*;
use std::collections::VecDeque;

// ───────────────────────────── errors ─────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct CmdError {
    /// Byte offset in the source text (`usize::MAX` = "the statement" — the caller fills it in).
    pub pos: usize,
    pub len: usize,
    pub msg: String,
}

impl CmdError {
    fn new(pos: usize, len: usize, msg: impl Into<String>) -> Self {
        CmdError { pos, len, msg: msg.into() }
    }
    fn stmt(msg: impl Into<String>) -> Self {
        CmdError { pos: usize::MAX, len: 0, msg: msg.into() }
    }
    /// Human readable error with 1-based column and the offending snippet.
    pub fn render(&self, text: &str) -> String {
        if self.pos == usize::MAX || self.pos > text.len() || !text.is_char_boundary(self.pos) {
            return self.msg.clone();
        }
        let col = text[..self.pos].chars().count() + 1;
        let mut end = (self.pos + self.len).min(text.len());
        while !text.is_char_boundary(end) {
            end += 1;
        }
        let snip = &text[self.pos..end];
        if snip.is_empty() {
            format!("{} (col {col}: end of input)", self.msg)
        } else {
            format!("{} (col {col}: «{snip}»)", self.msg)
        }
    }
}

type R<T> = Result<T, CmdError>;

// ───────────────────────────── keywords ─────────────────────────────

const KW_FIXTURE: &[&str] = &["fixture", "fixtures", "fx", "aparato", "aparatos", "ap", "foco", "focos"];
const KW_GROUP: &[&str] = &["group", "groups", "grupo", "grupos", "grp"];
const KW_ALL: &[&str] = &["all", "todos", "todo", "todas"];
const KW_NONE: &[&str] = &["none", "ninguno", "ninguna", "nada"];
const KW_ODD: &[&str] = &["odd", "impares", "impar"];
const KW_EVEN: &[&str] = &["even", "pares", "par"];
const KW_NEXT: &[&str] = &["next", "siguiente", "sig"];
const KW_PREV: &[&str] = &["prev", "previous", "anterior", "ant"];
const KW_THRU: &[&str] = &["thru", "through", "to", "hasta"];
const KW_AT: &[&str] = &["at", "en"];
const KW_AND: &[&str] = &["and", "y"];
const KW_MINUS: &[&str] = &["minus", "menos", "except", "excepto"];
const KW_FULL: &[&str] = &["full", "max", "maximo", "pleno"];
const KW_OUT: &[&str] = &["out", "off", "zero", "cero", "apagado"];
const KW_HALF: &[&str] = &["half", "mitad"];
const KW_HOME: &[&str] = &["home", "reposo", "default"];
const KW_COLOUR: &[&str] = &["colour", "color", "col"];
const KW_LOCATE: &[&str] = &["locate", "identify", "localizar", "identificar", "loc"];
const KW_RECORD: &[&str] = &["record", "rec", "store", "grabar", "guardar"];
const KW_UPDATE: &[&str] = &["update", "upd", "actualizar"];
const KW_DELETE: &[&str] = &["delete", "del", "remove", "borrar", "eliminar"];
const KW_APPLY: &[&str] = &["apply", "recall", "aplicar"];
const KW_GO: &[&str] = &["go", "ir", "adelante"];
const KW_BACK: &[&str] = &["back", "atras", "volver"];
const KW_PAUSE: &[&str] = &["pause", "pausa", "pausar"];
const KW_RELEASE: &[&str] = &["release", "rel", "liberar"];
const KW_GOTO: &[&str] = &["goto", "saltar"];
const KW_CUE: &[&str] = &["cue", "cues", "q"];
const KW_CUELIST: &[&str] = &["cuelist", "cuelists", "list", "lista", "cl"];
const KW_SCENE: &[&str] = &["scene", "scenes", "escena", "escenas", "sc"];
const KW_CHASE: &[&str] = &["chase", "chaser", "secuencia"];
const KW_MACRO: &[&str] = &["macro", "macros"];
const KW_TIMELINE: &[&str] = &["timeline", "timelines", "tl", "cronologia"];
const KW_BLACKOUT: &[&str] = &["blackout", "bo", "apagon", "negro"];
const KW_GM: &[&str] = &["gm", "master", "grand", "grandmaster", "maestro"];
const KW_CLEAR: &[&str] = &["clear", "clr", "limpiar"];
const KW_PARK: &[&str] = &["park", "aparcar"];
const KW_UNPARK: &[&str] = &["unpark", "desaparcar"];
const KW_UNDO: &[&str] = &["undo", "deshacer"];
const KW_REDO: &[&str] = &["redo", "rehacer"];
const KW_BLIND: &[&str] = &["blind", "ciego"];
const KW_PALETTE: &[&str] = &["palette", "paleta", "pal"];
const KW_EFFECT: &[&str] = &["effect", "efecto"];
const KW_NAME: &[&str] = &["name", "label", "nombre", "etiqueta"];
const KW_ON: &[&str] = &["on", "go", "run", "play", "activar", "encender", "si"];
const KW_OFF: &[&str] = &["off", "stop", "release", "apagar", "desactivar", "liberar", "no"];
const KW_TOGGLE: &[&str] = &["toggle", "alternar"];
const KW_SOLO: &[&str] = &["solo"];
const KW_REPLACE: &[&str] = &["replace", "reemplazar"];
const KW_MERGE: &[&str] = &["merge", "mezclar"];
const KW_TRACK: &[&str] = &["track", "tracking", "seguir"];
const KW_CUEONLY: &[&str] = &["cueonly", "soloq"];
const KW_OUTPUT: &[&str] = &["output", "salida"];
const KW_TAP: &[&str] = &["tap"];
const KW_BPM: &[&str] = &["bpm"];
const KW_RUN: &[&str] = &["run", "go", "play", "start", "iniciar"];
const KW_STOP: &[&str] = &["stop", "parar"];
const KW_SEEK: &[&str] = &["seek", "ir_a", "buscar"];
const KW_FROM: &[&str] = &["from", "desde"];

const KW_INTENSITY: &[&str] = &["intensity", "intensidad"];
const KW_POSITION: &[&str] = &["position", "posicion"];
const KW_BEAM: &[&str] = &["beam", "haz"];

/// Attribute → aliases (English + Spanish).
const ATTR_ALIASES: &[(&str, &[&str])] = &[
    ("dimmer", &["dimmer", "dim", "intensity", "int", "intensidad"]),
    ("pan", &["pan"]),
    ("tilt", &["tilt", "inclinacion"]),
    ("red", &["red", "rojo"]),
    ("green", &["green", "verde"]),
    ("blue", &["blue", "azul"]),
    ("white", &["white", "blanco"]),
    ("amber", &["amber", "ambar"]),
    ("uv", &["uv", "ultravioleta"]),
    ("lime", &["lime", "lima"]),
    ("cto", &["cto"]),
    ("colorwheel", &["colorwheel", "wheel", "rueda"]),
    ("gobo", &["gobo"]),
    ("gobo_rot", &["gobo_rot", "goborot"]),
    ("prism", &["prism", "prisma"]),
    ("prism_rot", &["prism_rot", "prismrot"]),
    ("frost", &["frost", "difusor"]),
    ("zoom", &["zoom"]),
    ("focus", &["focus", "enfoque"]),
    ("iris", &["iris"]),
    ("shutter", &["shutter", "strobe", "obturador", "estrobo"]),
    ("speed", &["speed", "velocidad"]),
    ("effect", &["effect", "efecto"]),
    ("control", &["control"]),
];

/// Colour names (English + Spanish) → RGB.
const COLOURS: &[(&[&str], [f32; 3])] = &[
    (&["red", "rojo"], [1.0, 0.0, 0.0]),
    (&["green", "verde"], [0.0, 1.0, 0.0]),
    (&["blue", "azul"], [0.0, 0.0, 1.0]),
    (&["white", "blanco"], [1.0, 1.0, 1.0]),
    (&["yellow", "amarillo"], [1.0, 1.0, 0.0]),
    (&["cyan", "cian"], [0.0, 1.0, 1.0]),
    (&["magenta"], [1.0, 0.0, 1.0]),
    (&["orange", "naranja"], [1.0, 0.5, 0.0]),
    (&["pink", "rosa"], [1.0, 0.4, 0.7]),
    (&["purple", "morado", "violeta"], [0.5, 0.0, 1.0]),
    (&["amber", "ambar"], [1.0, 0.75, 0.0]),
    (&["warm", "calido"], [1.0, 0.8, 0.5]),
    (&["cool", "frio"], [0.7, 0.85, 1.0]),
    (&["black", "negro"], [0.0, 0.0, 0.0]),
];

fn is(w: &str, set: &[&str]) -> bool {
    set.contains(&w)
}

fn attr_of(w: &str) -> Option<&'static str> {
    ATTR_ALIASES.iter().find(|(_, al)| al.contains(&w)).map(|(c, _)| *c)
}

fn colour_of(w: &str) -> Option<[f32; 3]> {
    COLOURS.iter().find(|(n, _)| n.contains(&w)).map(|(_, c)| *c)
}

/// Verbs that start a statement (used for suggestions and "unknown command" hints).
const VERB_GROUPS: &[&[&str]] = &[
    KW_FIXTURE, KW_GROUP, KW_ALL, KW_NONE, KW_ODD, KW_EVEN, KW_NEXT, KW_PREV, KW_AT, KW_COLOUR, KW_RECORD, KW_UPDATE,
    KW_DELETE, KW_APPLY, KW_GO, KW_BACK, KW_PAUSE, KW_RELEASE, KW_GOTO, KW_CUELIST, KW_SCENE, KW_CHASE, KW_MACRO,
    KW_TIMELINE, KW_BLACKOUT, KW_GM, KW_CLEAR, KW_PARK, KW_UNPARK, KW_UNDO, KW_REDO, KW_BLIND, KW_LOCATE,
];

fn all_keywords() -> Vec<&'static str> {
    let groups: &[&[&str]] = &[
        VERB_GROUPS[0], VERB_GROUPS[1], VERB_GROUPS[2], VERB_GROUPS[3], VERB_GROUPS[4], VERB_GROUPS[5], VERB_GROUPS[6],
        VERB_GROUPS[7], VERB_GROUPS[8], VERB_GROUPS[9], VERB_GROUPS[10], VERB_GROUPS[11], VERB_GROUPS[12], VERB_GROUPS[13],
        VERB_GROUPS[14], VERB_GROUPS[15], VERB_GROUPS[16], VERB_GROUPS[17], VERB_GROUPS[18], VERB_GROUPS[19],
        VERB_GROUPS[20], VERB_GROUPS[21], VERB_GROUPS[22], VERB_GROUPS[23], VERB_GROUPS[24], VERB_GROUPS[25],
        VERB_GROUPS[26], VERB_GROUPS[27], VERB_GROUPS[28], VERB_GROUPS[29], VERB_GROUPS[30], VERB_GROUPS[31],
        VERB_GROUPS[32], KW_THRU, KW_FULL, KW_OUT, KW_HALF, KW_HOME, KW_CUE, KW_PALETTE, KW_EFFECT, KW_NAME, KW_ON,
        KW_OFF, KW_TOGGLE, KW_SOLO, KW_REPLACE, KW_MERGE, KW_TRACK, KW_CUEONLY, KW_OUTPUT, KW_TAP, KW_BPM, KW_STOP,
        KW_SEEK, KW_FROM, KW_INTENSITY, KW_POSITION, KW_BEAM,
    ];
    let mut v: Vec<&'static str> = groups.iter().flat_map(|g| g.iter().copied()).collect();
    v.extend(ATTR_ALIASES.iter().flat_map(|(_, a)| a.iter().copied()));
    v.extend(COLOURS.iter().flat_map(|(n, _)| n.iter().copied()));
    v.sort_unstable();
    v.dedup();
    v
}

// ───────────────────────────── lexer ─────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum T {
    Word(String),
    Num(f64),
    Str(String),
    Plus,
    Minus,
    Slash,
    Comma,
    At,
    Sep,
}

#[derive(Debug, Clone)]
struct Tok {
    t: T,
    pos: usize,
    len: usize,
}

fn fold(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    match c {
        'á' | 'à' | 'ä' | 'â' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        _ => c,
    }
}

fn lex(text: &str) -> R<Vec<Tok>> {
    let mut out = Vec::new();
    let mut it = text.char_indices().peekable();
    while let Some(&(i, c)) = it.peek() {
        if c == '\n' || c == ';' {
            out.push(Tok { t: T::Sep, pos: i, len: 1 });
            it.next();
            continue;
        }
        if c.is_whitespace() || c == '%' || c == '=' || c == '.' {
            it.next();
            continue;
        }
        if c == '#' {
            while let Some(&(_, d)) = it.peek() {
                if d == '\n' {
                    break;
                }
                it.next();
            }
            continue;
        }
        let sym = match c {
            '+' => Some(T::Plus),
            '-' => Some(T::Minus),
            '/' => Some(T::Slash),
            ',' => Some(T::Comma),
            '@' => Some(T::At),
            _ => None,
        };
        if let Some(t) = sym {
            out.push(Tok { t, pos: i, len: 1 });
            it.next();
            continue;
        }
        if c == '"' || c == '\'' {
            let q = c;
            it.next();
            let mut s = String::new();
            let mut closed = false;
            let mut end = i + 1;
            for (j, d) in it.by_ref() {
                end = j + d.len_utf8();
                if d == q {
                    closed = true;
                    break;
                }
                s.push(d);
            }
            if !closed {
                return Err(CmdError::new(i, end - i, "unterminated quote"));
            }
            out.push(Tok { t: T::Str(s), pos: i, len: end - i });
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            let mut end = i;
            let mut s = String::new();
            let mut dot = false;
            while let Some(&(j, d)) = it.peek() {
                if d.is_ascii_digit() {
                    s.push(d);
                    end = j + 1;
                    it.next();
                } else if d == '.' && !dot {
                    let mut look = it.clone();
                    look.next();
                    if matches!(look.peek(), Some(&(_, e)) if e.is_ascii_digit()) {
                        dot = true;
                        s.push('.');
                        end = j + 1;
                        it.next();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            out.push(Tok { t: T::Num(s.parse().unwrap_or(0.0)), pos: start, len: end - start });
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            let mut end = i;
            let mut w = String::new();
            while let Some(&(j, d)) = it.peek() {
                if d.is_alphanumeric() || d == '_' {
                    w.push(fold(d));
                    end = j + d.len_utf8();
                    it.next();
                } else {
                    break;
                }
            }
            out.push(Tok { t: T::Word(w), pos: start, len: end - start });
            continue;
        }
        return Err(CmdError::new(i, c.len_utf8(), format!("unexpected character '{c}'")));
    }
    Ok(out)
}

// ───────────────────────────── AST ─────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum SelPart {
    /// `from..=to`; the flag says whether `thru` was written (a single id must exist).
    Range(u32, u32, bool),
    Group(u32),
    All,
    None,
    Odd,
    Even,
    Next,
    Prev,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelTerm {
    pub neg: bool,
    pub part: SelPart,
    pub pos: usize,
    pub len: usize,
}

pub type Sel = Vec<SelTerm>;

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Pct(f32),
    Full,
    Out,
    Half,
    Home,
    Fan(f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Cue,
    Cuelist,
    Group,
    Palette,
    Scene,
    Effect,
    Macro,
    Chase,
    Timeline,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneAct {
    On,
    Off,
    Solo,
    Toggle,
    Level(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChaseAct {
    On,
    Off,
    Tap,
    Bpm(f32),
    Next,
    Prev,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TlAct {
    Run(Option<f32>),
    Stop,
    Seek(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    UseCuelist(u32),
    Select(Sel),
    Set { sel: Option<Sel>, attr: String, val: Val },
    Colour { sel: Option<Sel>, rgb: [f32; 3] },
    Locate { sel: Option<Sel>, on: bool },
    RecordCue { number: Option<f64>, cuelist: Option<u32>, mode: RecordMode, label: String, release: bool },
    RecordPalette { kind: PaletteKind, id: Option<u32>, name: String },
    RecordScene { id: Option<u32>, name: String, output: bool },
    RecordGroup { id: Option<u32>, name: String },
    UpdateCue { number: Option<f64>, cuelist: Option<u32>, mode: UpdateMode },
    Delete { what: Target, id: f64, cuelist: Option<u32> },
    Apply { sel: Option<Sel>, palette: u32 },
    Go(Option<u32>),
    Back(Option<u32>),
    Pause(Option<u32>),
    Release { cuelist: Option<u32>, all: bool },
    GoTo { number: f64, cuelist: Option<u32> },
    Scene { id: u32, act: SceneAct },
    ScenesRelease,
    Chase { id: u32, act: ChaseAct },
    MacroRun(u32),
    MacroStop(Option<u32>),
    Timeline { id: u32, act: TlAct },
    Blackout(Option<bool>),
    Gm(Val),
    Clear,
    Park { universe: u16, channel: u16, val: Val },
    Unpark { universe: Option<u16>, channel: Option<u16> },
    Undo,
    Redo,
    Blind(bool),
}

/// One parsed statement with its source slice (for logging).
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub stmt: Stmt,
    pub text: String,
    pub pos: usize,
}

// ───────────────────────────── parser ─────────────────────────────

struct P<'a> {
    text: &'a str,
    toks: Vec<Tok>,
    i: usize,
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i)
    }
    fn peek_n(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.i + n)
    }
    fn at_end(&self) -> bool {
        matches!(self.peek(), None | Some(Tok { t: T::Sep, .. }))
    }
    fn word(&self) -> Option<&str> {
        match self.peek() {
            Some(Tok { t: T::Word(w), .. }) => Some(w.as_str()),
            _ => None,
        }
    }
    fn word_in(&self, set: &[&str]) -> bool {
        self.word().map_or(false, |w| is(w, set))
    }
    fn eat_kw(&mut self, set: &[&str]) -> bool {
        if self.word_in(set) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn is_at(&self) -> bool {
        matches!(self.peek(), Some(Tok { t: T::At, .. })) || self.word_in(KW_AT)
    }
    fn eat_at(&mut self) -> bool {
        if self.is_at() {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn here(&self, msg: impl Into<String>) -> CmdError {
        match self.peek() {
            Some(t) if t.t != T::Sep => CmdError::new(t.pos, t.len, msg),
            Some(t) => CmdError::new(t.pos, 0, msg),
            None => CmdError::new(self.text.len(), 0, msg),
        }
    }
    fn raw(&self, t: &Tok) -> String {
        self.text[t.pos..t.pos + t.len].to_string()
    }
    fn num(&mut self, what: &str) -> R<f64> {
        match self.peek() {
            Some(Tok { t: T::Num(n), .. }) => {
                let n = *n;
                self.i += 1;
                Ok(n)
            }
            _ => Err(self.here(format!("expected {what}"))),
        }
    }
    fn uint(&mut self, what: &str) -> R<u32> {
        let (pos, len) = self.peek().map_or((self.text.len(), 0), |t| (t.pos, t.len));
        let n = self.num(what)?;
        if n.fract() != 0.0 || n < 0.0 || n > u32::MAX as f64 {
            return Err(CmdError::new(pos, len, format!("{what} must be a whole number")));
        }
        Ok(n as u32)
    }
    fn unexpected(&self) -> CmdError {
        self.here("unexpected word — check the syntax (see /api/cmdline/help)")
    }
    /// Remaining tokens of the statement joined as free text (original spelling).
    fn rest_text(&mut self) -> String {
        let mut parts = Vec::new();
        while !self.at_end() {
            let t = self.toks[self.i].clone();
            match &t.t {
                T::Str(s) => parts.push(s.clone()),
                _ => parts.push(self.raw(&t)),
            }
            self.i += 1;
        }
        parts.join(" ")
    }

    // ── selection ──

    fn starts_selection(&self) -> bool {
        match self.peek() {
            Some(Tok { t: T::Num(_), .. }) => true,
            Some(Tok { t: T::Word(w), .. }) => {
                is(w, KW_FIXTURE) || is(w, KW_GROUP) || is(w, KW_ALL) || is(w, KW_NONE) || is(w, KW_ODD) || is(w, KW_EVEN)
                    || is(w, KW_NEXT) || is(w, KW_PREV)
            }
            _ => false,
        }
    }

    fn range(&mut self) -> R<(u32, u32, bool)> {
        let a = self.uint("a fixture number")?;
        if self.eat_kw(KW_THRU) {
            let b = self.uint("the last fixture number after 'thru'")?;
            Ok((a.min(b), a.max(b), true))
        } else {
            Ok((a, a, false))
        }
    }

    fn selection(&mut self) -> R<Sel> {
        let mut terms: Sel = Vec::new();
        let mut neg = false;
        loop {
            let (pos, len) = self.peek().map_or((self.text.len(), 0), |t| (t.pos, t.len));
            let part = match self.peek().map(|t| t.t.clone()) {
                Some(T::Word(w)) if is(&w, KW_FIXTURE) => {
                    self.i += 1;
                    let (a, b, th) = self.range()?;
                    SelPart::Range(a, b, th)
                }
                Some(T::Word(w)) if is(&w, KW_GROUP) => {
                    self.i += 1;
                    SelPart::Group(self.uint("a group number")?)
                }
                Some(T::Word(w)) if is(&w, KW_ALL) => {
                    self.i += 1;
                    SelPart::All
                }
                Some(T::Word(w)) if is(&w, KW_NONE) => {
                    self.i += 1;
                    SelPart::None
                }
                Some(T::Word(w)) if is(&w, KW_ODD) => {
                    self.i += 1;
                    SelPart::Odd
                }
                Some(T::Word(w)) if is(&w, KW_EVEN) => {
                    self.i += 1;
                    SelPart::Even
                }
                Some(T::Word(w)) if is(&w, KW_NEXT) => {
                    self.i += 1;
                    SelPart::Next
                }
                Some(T::Word(w)) if is(&w, KW_PREV) => {
                    self.i += 1;
                    SelPart::Prev
                }
                Some(T::Num(_)) => {
                    let (a, b, th) = self.range()?;
                    SelPart::Range(a, b, th)
                }
                _ => return Err(self.here("expected a selection (e.g. 'fixture 1 thru 10', 'group 2', 'all')")),
            };
            let end = self.toks[self.i - 1].pos + self.toks[self.i - 1].len;
            terms.push(SelTerm { neg, part, pos, len: end - pos });
            // operator or trailing odd/even
            match self.peek().map(|t| t.t.clone()) {
                Some(T::Plus) | Some(T::Comma) => {
                    self.i += 1;
                    neg = false;
                }
                Some(T::Word(w)) if is(&w, KW_AND) => {
                    self.i += 1;
                    neg = false;
                }
                Some(T::Minus) => {
                    self.i += 1;
                    neg = true;
                }
                Some(T::Word(w)) if is(&w, KW_MINUS) => {
                    self.i += 1;
                    neg = true;
                }
                Some(T::Word(w)) if is(&w, KW_ODD) || is(&w, KW_EVEN) => {
                    neg = false;
                }
                _ => break,
            }
        }
        Ok(terms)
    }

    // ── values ──

    fn val(&mut self) -> R<Val> {
        match self.peek().map(|t| t.t.clone()) {
            Some(T::Word(w)) if is(&w, KW_FULL) => {
                self.i += 1;
                Ok(Val::Full)
            }
            Some(T::Word(w)) if is(&w, KW_OUT) => {
                self.i += 1;
                Ok(Val::Out)
            }
            Some(T::Word(w)) if is(&w, KW_HALF) => {
                self.i += 1;
                Ok(Val::Half)
            }
            Some(T::Word(w)) if is(&w, KW_HOME) => {
                self.i += 1;
                Ok(Val::Home)
            }
            Some(T::Num(_)) => {
                let a = self.num("a value")? as f32;
                if self.eat_kw(KW_THRU) {
                    let b = self.num("the end value after 'thru'")? as f32;
                    Ok(Val::Fan(a, b))
                } else {
                    Ok(Val::Pct(a))
                }
            }
            _ => Err(self.here("expected a value (0-100, full, out, half or home)")),
        }
    }

    // ── statements ──

    fn stmt(&mut self) -> R<Stmt> {
        let Some(tok) = self.peek().cloned() else { return Err(self.here("empty statement")) };
        match &tok.t {
            T::Num(_) => self.select_led(None),
            T::At => {
                self.i += 1;
                let val = self.val()?;
                Ok(Stmt::Set { sel: None, attr: "dimmer".into(), val })
            }
            T::Word(w) => {
                let w = w.clone();
                self.word_stmt(&w)
            }
            T::Plus | T::Minus | T::Slash | T::Comma => Err(self.here("a statement cannot start with an operator")),
            T::Str(_) => Err(self.here("a statement cannot start with a quoted text")),
            T::Sep => Err(self.here("empty statement")),
        }
    }

    fn word_stmt(&mut self, w: &str) -> R<Stmt> {
        if is(w, KW_CUELIST) {
            self.i += 1;
            return Ok(Stmt::UseCuelist(self.uint("a cuelist number")?));
        }
        if is(w, KW_RECORD) {
            self.i += 1;
            return self.record();
        }
        if is(w, KW_UPDATE) {
            self.i += 1;
            return self.update();
        }
        if is(w, KW_DELETE) {
            self.i += 1;
            return self.delete();
        }
        if is(w, KW_APPLY) {
            self.i += 1;
            self.eat_kw(KW_PALETTE);
            let palette = self.uint("a palette number")?;
            return Ok(Stmt::Apply { sel: None, palette });
        }
        if is(w, KW_GO) {
            self.i += 1;
            return Ok(Stmt::Go(self.opt_cuelist()?));
        }
        if is(w, KW_BACK) {
            self.i += 1;
            return Ok(Stmt::Back(self.opt_cuelist()?));
        }
        if is(w, KW_PAUSE) {
            self.i += 1;
            return Ok(Stmt::Pause(self.opt_cuelist()?));
        }
        if is(w, KW_RELEASE) {
            self.i += 1;
            if self.eat_kw(KW_ALL) {
                return Ok(Stmt::Release { cuelist: None, all: true });
            }
            if self.eat_kw(KW_SCENE) {
                return Ok(Stmt::ScenesRelease);
            }
            return Ok(Stmt::Release { cuelist: self.opt_cuelist()?, all: false });
        }
        if is(w, KW_GOTO) {
            self.i += 1;
            self.eat_kw(KW_CUE);
            let number = self.num("a cue number")?;
            return Ok(Stmt::GoTo { number, cuelist: self.opt_cuelist()? });
        }
        if is(w, KW_SCENE) {
            self.i += 1;
            return self.scene();
        }
        if is(w, KW_CHASE) {
            self.i += 1;
            return self.chase();
        }
        if is(w, KW_MACRO) {
            self.i += 1;
            if self.eat_kw(KW_STOP) || self.eat_kw(KW_OFF) {
                let id = if self.at_end() { None } else { Some(self.uint("a macro number")?) };
                return Ok(Stmt::MacroStop(id));
            }
            return Ok(Stmt::MacroRun(self.uint("a macro number")?));
        }
        if is(w, KW_TIMELINE) {
            self.i += 1;
            return self.timeline();
        }
        if is(w, KW_BLACKOUT) {
            self.i += 1;
            if self.at_end() || self.eat_kw(KW_TOGGLE) {
                return Ok(Stmt::Blackout(None));
            }
            if self.eat_kw(KW_ON) || self.eat_kw(KW_HALF) && false {
                return Ok(Stmt::Blackout(Some(true)));
            }
            if self.eat_kw(KW_OFF) || self.eat_kw(KW_OUT) {
                return Ok(Stmt::Blackout(Some(false)));
            }
            return Err(self.here("expected on, off or toggle"));
        }
        if is(w, KW_GM) {
            self.i += 1;
            self.eat_at();
            let val = self.val()?;
            return Ok(Stmt::Gm(val));
        }
        if is(w, KW_CLEAR) {
            self.i += 1;
            return Ok(Stmt::Clear);
        }
        if is(w, KW_PARK) {
            self.i += 1;
            let (u, c) = self.uc()?;
            self.eat_at();
            let val = self.val()?;
            let c = c.ok_or_else(|| self.here("park needs a channel (park U/C at V)"))?;
            return Ok(Stmt::Park { universe: u.unwrap_or(1), channel: c, val });
        }
        if is(w, KW_UNPARK) {
            self.i += 1;
            let (u, c) = if self.at_end() { (None, None) } else { self.uc()? };
            return Ok(Stmt::Unpark { universe: u, channel: c });
        }
        if is(w, KW_UNDO) {
            self.i += 1;
            return Ok(Stmt::Undo);
        }
        if is(w, KW_REDO) {
            self.i += 1;
            return Ok(Stmt::Redo);
        }
        if is(w, KW_BLIND) {
            self.i += 1;
            if self.at_end() || self.eat_kw(KW_ON) {
                return Ok(Stmt::Blind(true));
            }
            if self.eat_kw(KW_OFF) || self.eat_kw(KW_OUT) {
                return Ok(Stmt::Blind(false));
            }
            return Err(self.here("expected on or off"));
        }
        if is(w, KW_LOCATE) {
            self.i += 1;
            let on = !(self.eat_kw(KW_OFF) || self.eat_kw(KW_OUT));
            self.eat_kw(KW_ON);
            return Ok(Stmt::Locate { sel: None, on });
        }
        if is(w, KW_AT) {
            self.i += 1;
            let val = self.val()?;
            return Ok(Stmt::Set { sel: None, attr: "dimmer".into(), val });
        }
        if is(w, KW_COLOUR) {
            self.i += 1;
            return self.colour_tail(None);
        }
        // selection-led or attribute-led
        let sel_start = is(w, KW_FIXTURE) || is(w, KW_GROUP) || is(w, KW_ALL) || is(w, KW_NONE) || is(w, KW_ODD)
            || is(w, KW_EVEN) || is(w, KW_NEXT) || is(w, KW_PREV);
        if sel_start {
            return self.select_led(None);
        }
        if let Some(attr) = attr_of(w) {
            self.i += 1;
            let sel = if self.starts_selection() { Some(self.selection()?) } else { None };
            if !self.eat_at() {
                return Err(self.here(format!("expected 'at <value>' after '{attr}'")));
            }
            let val = self.val()?;
            return Ok(Stmt::Set { sel, attr: attr.into(), val });
        }
        // unknown word followed by `at` = raw attribute name (e.g. imported GDTF attributes)
        if matches!(self.peek_n(1), Some(t) if t.t == T::At || matches!(&t.t, T::Word(x) if is(x, KW_AT))) {
            self.i += 1;
            self.eat_at();
            let val = self.val()?;
            return Ok(Stmt::Set { sel: None, attr: w.to_string(), val });
        }
        Err(self.unknown(w))
    }

    fn unknown(&self, w: &str) -> CmdError {
        let mut best: Vec<&str> = all_keywords().into_iter().filter(|k| k.starts_with(&w[..w.len().min(2)]) || lev(k, w) <= 2).collect();
        best.sort_by_key(|k| lev(k, w));
        best.truncate(4);
        let hint = if best.is_empty() { String::new() } else { format!(" — did you mean {}?", best.join(", ")) };
        self.here(format!("unknown command '{w}'{hint}"))
    }

    fn opt_cuelist(&mut self) -> R<Option<u32>> {
        if self.eat_kw(KW_CUELIST) {
            return Ok(Some(self.uint("a cuelist number")?));
        }
        if matches!(self.peek(), Some(Tok { t: T::Num(_), .. })) {
            return Ok(Some(self.uint("a cuelist number")?));
        }
        Ok(None)
    }

    /// `U/C` or `C` (universe optional).
    fn uc(&mut self) -> R<(Option<u16>, Option<u16>)> {
        let a = self.uint("a channel or universe/channel")?;
        let to16 = |n: u32, p: &P| -> R<u16> { u16::try_from(n).map_err(|_| p.here("number too large")) };
        if matches!(self.peek(), Some(Tok { t: T::Slash, .. })) {
            self.i += 1;
            let b = self.uint("a channel number")?;
            return Ok((Some(to16(a, self)?), Some(to16(b, self)?)));
        }
        Ok((None, Some(to16(a, self)?)))
    }

    fn select_led(&mut self, _unused: Option<()>) -> R<Stmt> {
        let sel = self.selection()?;
        if self.at_end() {
            return Ok(Stmt::Select(sel));
        }
        if self.eat_at() {
            let val = self.val()?;
            return Ok(Stmt::Set { sel: Some(sel), attr: "dimmer".into(), val });
        }
        if self.word_in(KW_COLOUR) {
            self.i += 1;
            return self.colour_tail(Some(sel));
        }
        if self.word_in(KW_LOCATE) {
            self.i += 1;
            let on = !(self.eat_kw(KW_OFF) || self.eat_kw(KW_OUT));
            self.eat_kw(KW_ON);
            return Ok(Stmt::Locate { sel: Some(sel), on });
        }
        if self.word_in(KW_APPLY) {
            self.i += 1;
            self.eat_kw(KW_PALETTE);
            let palette = self.uint("a palette number")?;
            return Ok(Stmt::Apply { sel: Some(sel), palette });
        }
        if let Some(w) = self.word().map(|s| s.to_string()) {
            if let Some(attr) = attr_of(&w) {
                self.i += 1;
                if !self.eat_at() {
                    return Err(self.here(format!("expected 'at <value>' after '{attr}'")));
                }
                let val = self.val()?;
                return Ok(Stmt::Set { sel: Some(sel), attr: attr.into(), val });
            }
        }
        Err(self.unexpected())
    }

    fn colour_tail(&mut self, sel: Option<Sel>) -> R<Stmt> {
        if matches!(self.peek(), Some(Tok { t: T::Num(_), .. })) {
            let r = self.num("red %")? as f32;
            let g = self.num("green %")? as f32;
            let b = self.num("blue %")? as f32;
            for v in [r, g, b] {
                if !(0.0..=100.0).contains(&v) {
                    return Err(self.here("colour components are percentages 0-100"));
                }
            }
            return Ok(Stmt::Colour { sel, rgb: [r / 100.0, g / 100.0, b / 100.0] });
        }
        match self.word().and_then(colour_of) {
            Some(rgb) => {
                self.i += 1;
                Ok(Stmt::Colour { sel, rgb })
            }
            None => {
                let names: Vec<&str> = COLOURS.iter().map(|(n, _)| n[0]).collect();
                Err(self.here(format!("unknown colour — try {}", names.join(", "))))
            }
        }
    }

    fn record(&mut self) -> R<Stmt> {
        if self.eat_kw(KW_CUE) || matches!(self.peek(), Some(Tok { t: T::Num(_), .. })) {
            let number = if matches!(self.peek(), Some(Tok { t: T::Num(_), .. })) { Some(self.num("a cue number")?) } else { None };
            let (mut cuelist, mut label, mut mode, mut release) = (None, String::new(), RecordMode::Merge, false);
            while !self.at_end() {
                if self.eat_kw(KW_CUELIST) {
                    cuelist = Some(self.uint("a cuelist number")?);
                } else if self.eat_kw(KW_NAME) {
                    label = self.rest_text();
                } else if self.eat_kw(KW_REPLACE) {
                    mode = RecordMode::Replace;
                } else if self.eat_kw(KW_MERGE) {
                    mode = RecordMode::Merge;
                } else if self.eat_kw(KW_RELEASE) {
                    release = true;
                } else {
                    return Err(self.unexpected());
                }
            }
            return Ok(Stmt::RecordCue { number, cuelist, mode, label, release });
        }
        if self.eat_kw(KW_PALETTE) {
            let mut kind = PaletteKind::All;
            if self.eat_kw(KW_INTENSITY) {
                kind = PaletteKind::Intensity;
            } else if self.eat_kw(KW_POSITION) {
                kind = PaletteKind::Position;
            } else if self.eat_kw(KW_COLOUR) {
                kind = PaletteKind::Color;
            } else if self.eat_kw(KW_BEAM) {
                kind = PaletteKind::Beam;
            } else if self.eat_kw(KW_ALL) {
                kind = PaletteKind::All;
            }
            let id = if matches!(self.peek(), Some(Tok { t: T::Num(_), .. })) { Some(self.uint("a palette number")?) } else { None };
            return Ok(Stmt::RecordPalette { kind, id, name: self.rest_text() });
        }
        if self.eat_kw(KW_SCENE) {
            let id = if matches!(self.peek(), Some(Tok { t: T::Num(_), .. })) { Some(self.uint("a scene number")?) } else { None };
            let output = self.eat_kw(KW_OUTPUT);
            let mut name = self.rest_text();
            if name.to_lowercase() == "output" || name.to_lowercase() == "salida" {
                return Ok(Stmt::RecordScene { id, name: String::new(), output: true });
            }
            if let Some(n) = name.strip_suffix(" output").or_else(|| name.strip_suffix(" salida")) {
                let n = n.to_string();
                return Ok(Stmt::RecordScene { id, name: n, output: true });
            }
            if output {
                name = name.trim().to_string();
            }
            return Ok(Stmt::RecordScene { id, name, output });
        }
        if self.eat_kw(KW_GROUP) {
            let id = if matches!(self.peek(), Some(Tok { t: T::Num(_), .. })) { Some(self.uint("a group number")?) } else { None };
            return Ok(Stmt::RecordGroup { id, name: self.rest_text() });
        }
        Err(self.here("record what? (cue, palette, scene or group)"))
    }

    fn update(&mut self) -> R<Stmt> {
        self.eat_kw(KW_CUE);
        let number = if matches!(self.peek(), Some(Tok { t: T::Num(_), .. })) { Some(self.num("a cue number")?) } else { None };
        let (mut cuelist, mut mode) = (None, UpdateMode::Merge);
        while !self.at_end() {
            if self.eat_kw(KW_CUELIST) {
                cuelist = Some(self.uint("a cuelist number")?);
            } else if self.eat_kw(KW_REPLACE) {
                mode = UpdateMode::Replace;
            } else if self.eat_kw(KW_MERGE) {
                mode = UpdateMode::Merge;
            } else if self.eat_kw(KW_TRACK) {
                mode = UpdateMode::TrackForward;
            } else if self.eat_kw(KW_CUEONLY) {
                mode = UpdateMode::CueOnly;
            } else {
                return Err(self.unexpected());
            }
        }
        Ok(Stmt::UpdateCue { number, cuelist, mode })
    }

    fn delete(&mut self) -> R<Stmt> {
        let what = if self.eat_kw(KW_CUE) {
            Target::Cue
        } else if self.eat_kw(KW_CUELIST) {
            Target::Cuelist
        } else if self.eat_kw(KW_GROUP) {
            Target::Group
        } else if self.eat_kw(KW_PALETTE) {
            Target::Palette
        } else if self.eat_kw(KW_SCENE) {
            Target::Scene
        } else if self.eat_kw(KW_EFFECT) {
            Target::Effect
        } else if self.eat_kw(KW_MACRO) {
            Target::Macro
        } else if self.eat_kw(KW_CHASE) {
            Target::Chase
        } else if self.eat_kw(KW_TIMELINE) {
            Target::Timeline
        } else {
            return Err(self.here("delete what? (cue, cuelist, group, palette, scene, effect, macro, chase or timeline)"));
        };
        let id = self.num("a number")?;
        let cuelist = if what == Target::Cue && self.eat_kw(KW_CUELIST) { Some(self.uint("a cuelist number")?) } else { None };
        Ok(Stmt::Delete { what, id, cuelist })
    }

    fn scene(&mut self) -> R<Stmt> {
        if self.eat_kw(KW_RELEASE) || self.eat_kw(KW_OFF) && self.at_end() {
            return Ok(Stmt::ScenesRelease);
        }
        if self.eat_kw(KW_SOLO) {
            return Ok(Stmt::Scene { id: self.uint("a scene number")?, act: SceneAct::Solo });
        }
        let id = self.uint("a scene number")?;
        let act = if self.at_end() || self.eat_kw(KW_TOGGLE) {
            SceneAct::Toggle
        } else if self.eat_kw(KW_SOLO) {
            SceneAct::Solo
        } else if self.eat_at() {
            match self.val()? {
                Val::Pct(p) if (0.0..=100.0).contains(&p) => SceneAct::Level(p / 100.0),
                Val::Full => SceneAct::Level(1.0),
                Val::Out => SceneAct::Level(0.0),
                Val::Half => SceneAct::Level(0.5),
                _ => return Err(self.here("scene level must be 0-100")),
            }
        } else if self.eat_kw(KW_ON) {
            SceneAct::On
        } else if self.eat_kw(KW_OFF) || self.eat_kw(KW_OUT) {
            SceneAct::Off
        } else {
            return Err(self.here("expected on, off, solo, toggle or 'at <level>'"));
        };
        Ok(Stmt::Scene { id, act })
    }

    fn chase(&mut self) -> R<Stmt> {
        let id = self.uint("a chase number")?;
        let act = if self.at_end() || self.eat_kw(KW_ON) {
            ChaseAct::On
        } else if self.eat_kw(KW_OFF) || self.eat_kw(KW_OUT) {
            ChaseAct::Off
        } else if self.eat_kw(KW_TAP) {
            ChaseAct::Tap
        } else if self.eat_kw(KW_BPM) {
            ChaseAct::Bpm(self.num("a tempo in BPM")? as f32)
        } else if self.eat_kw(KW_NEXT) {
            ChaseAct::Next
        } else if self.eat_kw(KW_PREV) {
            ChaseAct::Prev
        } else {
            return Err(self.here("expected on, off, tap, bpm <n>, next or prev"));
        };
        Ok(Stmt::Chase { id, act })
    }

    fn timeline(&mut self) -> R<Stmt> {
        let id = self.uint("a timeline number")?;
        let act = if self.at_end() || self.eat_kw(KW_RUN) {
            let from = if self.eat_kw(KW_FROM) { Some(self.num("a time in seconds")? as f32) } else { None };
            TlAct::Run(from)
        } else if self.eat_kw(KW_STOP) || self.eat_kw(KW_OFF) {
            TlAct::Stop
        } else if self.eat_kw(KW_SEEK) {
            TlAct::Seek(self.num("a time in seconds")? as f32)
        } else {
            return Err(self.here("expected run, stop or seek <seconds>"));
        };
        Ok(Stmt::Timeline { id, act })
    }
}

fn lev(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Parse a text into statements (syntax only — no show state needed).
pub fn parse(text: &str) -> Result<Vec<Parsed>, CmdError> {
    if text.len() > 8192 {
        return Err(CmdError::new(0, 0, "command line too long (max 8192 bytes)"));
    }
    let toks = lex(text)?;
    let mut p = P { text, toks, i: 0 };
    let mut out = Vec::new();
    loop {
        while matches!(p.peek(), Some(Tok { t: T::Sep, .. })) {
            p.i += 1;
        }
        if p.peek().is_none() {
            break;
        }
        let start = p.toks[p.i].pos;
        let stmt = p.stmt()?;
        let last = &p.toks[p.i - 1];
        let end = last.pos + last.len;
        let is_prefix = matches!(stmt, Stmt::UseCuelist(_));
        if !is_prefix && !p.at_end() {
            return Err(p.unexpected());
        }
        out.push(Parsed { stmt, text: text[start..end].to_string(), pos: start });
        if out.len() > 200 {
            return Err(CmdError::new(start, 0, "too many statements (max 200)"));
        }
    }
    Ok(out)
}

// ───────────────────────────── resolution ─────────────────────────────

/// Read-only view of the show the resolver needs.
#[derive(Default, Clone, Debug)]
pub struct Ctx {
    /// Patched fixture ids, ascending.
    pub fixtures: Vec<u32>,
    pub groups: Vec<(u32, Vec<u32>)>,
    pub cuelists: Vec<u32>,
    /// Current cue number per cuelist.
    pub current_cue: Vec<(u32, Option<f64>)>,
    /// Fixtures that hold programmer values.
    pub programmer: Vec<u32>,
    pub scenes_active: Vec<u32>,
    pub blackout: bool,
}

/// Interpreter session state (lives for one command-line text or one macro run).
#[derive(Default, Clone, Debug)]
pub struct Session {
    pub selection: Vec<u32>,
    pub cuelist: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Compiled {
    pub cmds: Vec<Command>,
    pub summary: String,
}

fn eval_sel(sel: &Sel, ctx: &Ctx, sess: &Session) -> R<Vec<u32>> {
    let mut acc: Vec<u32> = Vec::new();
    let mut started = false;
    let push = |acc: &mut Vec<u32>, id: u32| {
        if !acc.contains(&id) {
            acc.push(id);
        }
    };
    for t in sel {
        let err = |m: String| CmdError::new(t.pos, t.len, m);
        match &t.part {
            SelPart::Range(a, b, thru) => {
                let ids: Vec<u32> = ctx.fixtures.iter().copied().filter(|f| f >= a && f <= b).collect();
                if ids.is_empty() {
                    return Err(err(if *thru { format!("no fixtures between {a} and {b}") } else { format!("fixture {a} does not exist") }));
                }
                if !*thru && !t.neg && !ctx.fixtures.contains(a) {
                    return Err(err(format!("fixture {a} does not exist")));
                }
                started = true;
                if t.neg {
                    acc.retain(|x| !ids.contains(x));
                } else {
                    for id in ids {
                        push(&mut acc, id);
                    }
                }
            }
            SelPart::Group(n) => {
                let g = ctx.groups.iter().find(|(id, _)| id == n).ok_or_else(|| err(format!("group {n} does not exist")))?;
                started = true;
                if t.neg {
                    acc.retain(|x| !g.1.contains(x));
                } else {
                    for id in &g.1 {
                        if ctx.fixtures.contains(id) {
                            push(&mut acc, *id);
                        }
                    }
                }
            }
            SelPart::All => {
                started = true;
                if t.neg {
                    acc.clear();
                } else {
                    acc = ctx.fixtures.clone();
                }
            }
            SelPart::None => {
                started = true;
                acc.clear();
            }
            SelPart::Odd | SelPart::Even => {
                let odd = t.part == SelPart::Odd;
                let base: Vec<u32> = if started {
                    acc.clone()
                } else if !sess.selection.is_empty() {
                    sess.selection.clone()
                } else {
                    ctx.fixtures.clone()
                };
                let pick = |i: usize| (i % 2 == 0) == odd; // positions are 1-based: index 0 is "1" (odd)
                if t.neg {
                    let drop: Vec<u32> = base.iter().enumerate().filter(|(i, _)| pick(*i)).map(|(_, f)| *f).collect();
                    acc.retain(|x| !drop.contains(x));
                } else {
                    acc = base.iter().enumerate().filter(|(i, _)| pick(*i)).map(|(_, f)| *f).collect();
                }
                started = true;
            }
            SelPart::Next | SelPart::Prev => {
                if ctx.fixtures.is_empty() {
                    return Err(err("there are no fixtures".into()));
                }
                let cur = sess.selection.last().and_then(|f| ctx.fixtures.iter().position(|x| x == f));
                let n = ctx.fixtures.len();
                let idx = match (t.part == SelPart::Next, cur) {
                    (true, Some(i)) => (i + 1) % n,
                    (true, None) => 0,
                    (false, Some(i)) => (i + n - 1) % n,
                    (false, None) => n - 1,
                };
                acc = vec![ctx.fixtures[idx]];
                started = true;
            }
        }
    }
    Ok(acc)
}

fn unit(v: &Val, what: &str) -> R<f32> {
    match v {
        Val::Pct(p) if (0.0..=100.0).contains(p) => Ok(p / 100.0),
        Val::Pct(_) => Err(CmdError::stmt("value must be between 0 and 100")),
        Val::Full => Ok(1.0),
        Val::Out => Ok(0.0),
        Val::Half => Ok(0.5),
        Val::Home | Val::Fan(..) => Err(CmdError::stmt(format!("'{}' is not valid for {what}", if matches!(v, Val::Home) { "home" } else { "fan" }))),
    }
}

fn list_of(ctx: &Ctx, sess: &Session, given: Option<u32>) -> u32 {
    given.or(sess.cuelist).or_else(|| ctx.cuelists.first().copied()).unwrap_or(1)
}

fn whole(n: f64, what: &str) -> R<u32> {
    if n.fract() != 0.0 || n < 0.0 || n > u32::MAX as f64 {
        return Err(CmdError::stmt(format!("{what} must be a whole number")));
    }
    Ok(n as u32)
}

fn no_sel() -> CmdError {
    CmdError::stmt("no fixtures selected — start with e.g. 'fixture 1 thru 4' or 'group 2'")
}

fn selection_for(sel: &Option<Sel>, ctx: &Ctx, sess: &mut Session) -> R<Vec<u32>> {
    let fx = match sel {
        Some(s) => {
            let v = eval_sel(s, ctx, sess)?;
            sess.selection = v.clone();
            v
        }
        None => sess.selection.clone(),
    };
    if fx.is_empty() {
        return Err(no_sel());
    }
    Ok(fx)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Resolve one statement into engine commands, updating the session (selection, default cuelist).
pub fn resolve(stmt: &Stmt, ctx: &Ctx, sess: &mut Session) -> R<Compiled> {
    let one = |cmd: Command, summary: String| Ok(Compiled { cmds: vec![cmd], summary });
    match stmt {
        Stmt::UseCuelist(n) => {
            sess.cuelist = Some(*n);
            Ok(Compiled { cmds: vec![], summary: format!("cuelist {n}") })
        }
        Stmt::Select(sel) => {
            let v = eval_sel(sel, ctx, sess)?;
            let n = v.len();
            sess.selection = v;
            Ok(Compiled { cmds: vec![], summary: format!("selected {}", plural(n, "fixture", "fixtures")) })
        }
        Stmt::Set { sel, attr, val } => {
            let fx = selection_for(sel, ctx, sess)?;
            let attrs_of = |v: f32| -> Attrs {
                let mut a = Attrs::new();
                a.insert(attr.clone(), v);
                a
            };
            match val {
                Val::Fan(a, b) => {
                    if !(0.0..=100.0).contains(a) || !(0.0..=100.0).contains(b) {
                        return Err(CmdError::stmt("value must be between 0 and 100"));
                    }
                    let n = fx.len();
                    let cmds = fx
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            let t = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 };
                            Command::ProgSet { fixtures: vec![*f], attrs: attrs_of((a + (b - a) * t) / 100.0) }
                        })
                        .collect();
                    Ok(Compiled { cmds, summary: format!("{attr} fan {a}→{b}% over {}", plural(n, "fixture", "fixtures")) })
                }
                Val::Home => {
                    let v = if attr == "dimmer" { 0.0 } else { attr_default(attr) };
                    one(Command::ProgSet { fixtures: fx.clone(), attrs: attrs_of(v) }, format!("{attr} home on {}", plural(fx.len(), "fixture", "fixtures")))
                }
                v => {
                    let u = unit(v, "this attribute")?;
                    let n = fx.len();
                    one(Command::ProgSet { fixtures: fx, attrs: attrs_of(u) }, format!("{attr} at {:.0}% on {}", u * 100.0, plural(n, "fixture", "fixtures")))
                }
            }
        }
        Stmt::Colour { sel, rgb } => {
            let fx = selection_for(sel, ctx, sess)?;
            let mut a = Attrs::new();
            a.insert("red".into(), rgb[0]);
            a.insert("green".into(), rgb[1]);
            a.insert("blue".into(), rgb[2]);
            let n = fx.len();
            one(Command::ProgSet { fixtures: fx, attrs: a }, format!("colour set on {}", plural(n, "fixture", "fixtures")))
        }
        Stmt::Locate { sel, on } => {
            let fx = selection_for(sel, ctx, sess)?;
            let n = fx.len();
            let cmds = fx.into_iter().map(|fixture| Command::Identify { fixture, on: *on }).collect();
            Ok(Compiled { cmds, summary: format!("locate {} on {}", if *on { "on" } else { "off" }, plural(n, "fixture", "fixtures")) })
        }
        Stmt::RecordCue { number, cuelist, mode, label, release } => {
            let l = list_of(ctx, sess, *cuelist);
            one(
                Command::ProgRecordCue { cuelist: l, number: *number, mode: *mode, label: label.clone(), release: *release, refs: true },
                format!("recorded cue {} in cuelist {l}", number.map_or("(next)".to_string(), |n| n.to_string())),
            )
        }
        Stmt::RecordPalette { kind, id, name } => {
            let fx = if !sess.selection.is_empty() { sess.selection.clone() } else { ctx.programmer.clone() };
            if fx.is_empty() {
                return Err(CmdError::stmt("nothing to record — the programmer is empty and no fixtures are selected"));
            }
            one(Command::ProgRecordPalette { id: *id, fixtures: fx, kind: *kind, name: name.clone() }, format!("recorded palette {}", id.map_or(String::new(), |i| i.to_string())))
        }
        Stmt::RecordScene { id, name, output } => {
            let fixtures = if sess.selection.is_empty() { None } else { Some(sess.selection.clone()) };
            one(
                Command::SceneRecord { id: *id, name: name.clone(), from_output: *output, fixtures, merge: false, refs: true },
                format!("recorded scene {}", id.map_or(String::new(), |i| i.to_string())),
            )
        }
        Stmt::RecordGroup { id, name } => {
            if sess.selection.is_empty() {
                return Err(no_sel());
            }
            let id = id.unwrap_or(0);
            let name = if name.is_empty() { if id == 0 { "Group".to_string() } else { format!("Group {id}") } } else { name.clone() };
            one(Command::GroupSave { group: Group { id, name, fixtures: sess.selection.clone() } }, format!("recorded group {}", if id == 0 { String::new() } else { id.to_string() }))
        }
        Stmt::UpdateCue { number, cuelist, mode } => {
            let l = list_of(ctx, sess, *cuelist);
            let n = match number {
                Some(n) => *n,
                None => ctx
                    .current_cue
                    .iter()
                    .find(|(id, _)| *id == l)
                    .and_then(|(_, c)| *c)
                    .ok_or_else(|| CmdError::stmt("no current cue — give a cue number (update cue N)"))?,
            };
            one(Command::CueUpdate { cuelist: l, number: n, mode: *mode, refs: true }, format!("updated cue {n} in cuelist {l}"))
        }
        Stmt::Delete { what, id, cuelist } => {
            let name = match what {
                Target::Cue => "cue",
                Target::Cuelist => "cuelist",
                Target::Group => "group",
                Target::Palette => "palette",
                Target::Scene => "scene",
                Target::Effect => "effect",
                Target::Macro => "macro",
                Target::Chase => "chase",
                Target::Timeline => "timeline",
            };
            let cmd = if *what == Target::Cue {
                Command::CueRemove { cuelist: list_of(ctx, sess, *cuelist), number: *id }
            } else {
                let n = whole(*id, "the number")?;
                match what {
                    Target::Cuelist => Command::CuelistRemove { id: n },
                    Target::Group => Command::GroupRemove { id: n },
                    Target::Palette => Command::PaletteRemove { id: n },
                    Target::Scene => Command::SceneRemove { id: n },
                    Target::Effect => Command::EffectRemove { id: n },
                    Target::Macro => Command::MacroRemove { id: n },
                    Target::Chase => Command::ChaseRemove { id: n },
                    Target::Timeline => Command::TimelineRemove { id: n },
                    Target::Cue => unreachable!(),
                }
            };
            one(cmd, format!("deleted {name} {id}"))
        }
        Stmt::Apply { sel, palette } => {
            let fx = selection_for(sel, ctx, sess)?;
            one(Command::ProgApplyPalette { fixtures: fx, palette: *palette }, format!("applied palette {palette}"))
        }
        Stmt::Go(l) => {
            let l = list_of(ctx, sess, *l);
            one(Command::Go { cuelist: l }, format!("go cuelist {l}"))
        }
        Stmt::Back(l) => {
            let l = list_of(ctx, sess, *l);
            one(Command::Back { cuelist: l }, format!("back cuelist {l}"))
        }
        Stmt::Pause(l) => {
            let l = list_of(ctx, sess, *l);
            one(Command::Pause { cuelist: l }, format!("pause cuelist {l}"))
        }
        Stmt::Release { cuelist, all } => {
            if *all {
                let mut cmds: Vec<Command> = ctx.cuelists.iter().map(|c| Command::ReleaseCuelist { cuelist: *c }).collect();
                cmds.push(Command::ScenesRelease);
                return Ok(Compiled { cmds, summary: "released everything".into() });
            }
            let l = list_of(ctx, sess, *cuelist);
            one(Command::ReleaseCuelist { cuelist: l }, format!("released cuelist {l}"))
        }
        Stmt::GoTo { number, cuelist } => {
            let l = list_of(ctx, sess, *cuelist);
            one(Command::GoTo { cuelist: l, number: *number }, format!("goto cue {number} in cuelist {l}"))
        }
        Stmt::Scene { id, act } => {
            let active = ctx.scenes_active.contains(id);
            let (cmd, what) = match act {
                SceneAct::On => (Command::SceneGo { id: *id, on: true }, "on"),
                SceneAct::Off => (Command::SceneGo { id: *id, on: false }, "off"),
                SceneAct::Solo => (Command::SceneSolo { id: *id }, "solo"),
                SceneAct::Toggle => (Command::SceneGo { id: *id, on: !active }, if active { "off" } else { "on" }),
                SceneAct::Level(l) => (Command::SceneLevel { id: *id, level: *l }, "level"),
            };
            one(cmd, format!("scene {id} {what}"))
        }
        Stmt::ScenesRelease => one(Command::ScenesRelease, "scenes released".into()),
        Stmt::Chase { id, act } => {
            let (cmd, what) = match act {
                ChaseAct::On => (Command::ChaseRun { id: *id, on: true }, "on".to_string()),
                ChaseAct::Off => (Command::ChaseRun { id: *id, on: false }, "off".to_string()),
                ChaseAct::Tap => (Command::ChaseTap { id: *id }, "tap".to_string()),
                ChaseAct::Bpm(b) => (Command::ChaseBpm { id: *id, bpm: *b }, format!("{b} bpm")),
                ChaseAct::Next => (Command::ChaseNudge { id: *id, delta: 1 }, "next".to_string()),
                ChaseAct::Prev => (Command::ChaseNudge { id: *id, delta: -1 }, "prev".to_string()),
            };
            one(cmd, format!("chase {id} {what}"))
        }
        Stmt::MacroRun(id) => one(Command::MacroRun { id: *id }, format!("macro {id} started")),
        Stmt::MacroStop(id) => one(Command::MacroStop { id: *id }, "macro stopped".into()),
        Stmt::Timeline { id, act } => {
            let (cmd, what) = match act {
                TlAct::Run(from) => (Command::TimelineRun { id: *id, from: *from }, "run".to_string()),
                TlAct::Stop => (Command::TimelineStop { id: *id }, "stop".to_string()),
                TlAct::Seek(t) => (Command::TimelineSeek { id: *id, t: *t }, format!("seek {t}s")),
            };
            one(cmd, format!("timeline {id} {what}"))
        }
        Stmt::Blackout(v) => {
            let on = v.unwrap_or(!ctx.blackout);
            one(Command::Blackout { on }, format!("blackout {}", if on { "on" } else { "off" }))
        }
        Stmt::Gm(v) => {
            let u = unit(v, "the grand master")?;
            one(Command::GrandMaster { level: u }, format!("grand master {:.0}%", u * 100.0))
        }
        Stmt::Clear => {
            sess.selection.clear();
            one(Command::ProgRelease { fixtures: None, attrs: None }, "programmer cleared".into())
        }
        Stmt::Park { universe, channel, val } => {
            if *universe == 0 || !(1..=512).contains(channel) {
                return Err(CmdError::stmt("park: universe must be ≥ 1 and channel 1-512"));
            }
            let u = unit(val, "park")?;
            let v = (u * 255.0).round() as u8;
            one(Command::DmxSet { universe: *universe, channel: *channel, value: Some(v) }, format!("parked {universe}/{channel} at {v}"))
        }
        Stmt::Unpark { universe, channel } => one(Command::Unpark { universe: *universe, channel: *channel }, "unparked".into()),
        Stmt::Undo => one(Command::Undo, "undo".into()),
        Stmt::Redo => one(Command::Redo, "redo".into()),
        Stmt::Blind(on) => one(Command::BlindSet { on: *on }, format!("blind {}", if *on { "on" } else { "off" })),
    }
}

// ───────────────────────────── completion & help ─────────────────────────────

/// Words that make sense after a given previous word (empty fragment) — also filters by prefix.
const FOLLOW: &[(&[&str], &[&str])] = &[
    (&["at", "en", "gm", "master", "grand"], &["full", "out", "half", "home"]),
    (&["record", "rec", "store", "grabar", "guardar"], &["cue", "palette", "scene", "group"]),
    (&["delete", "del", "borrar", "eliminar"], &["cue", "cuelist", "group", "palette", "scene", "effect", "macro", "chase", "timeline"]),
    (&["scene", "escena"], &["solo", "release", "on", "off", "toggle", "at"]),
    (&["chase"], &["on", "off", "tap", "bpm", "next", "prev"]),
    (&["timeline", "tl"], &["run", "stop", "seek"]),
    (&["macro"], &["stop"]),
    (&["blackout", "bo"], &["on", "off", "toggle"]),
    (&["blind"], &["on", "off"]),
    (&["colour", "color"], &["red", "green", "blue", "white", "yellow", "cyan", "magenta", "orange", "pink", "purple", "amber", "warm", "cool"]),
    (&["update", "upd", "actualizar"], &["cue", "replace", "track", "cueonly"]),
    (&["fixture", "fx", "group"], &["thru", "at"]),
    (&["thru"], &[]),
];

const TOP_LEVEL: &[&str] = &[
    "fixture", "group", "all", "dimmer", "red", "green", "blue", "pan", "tilt", "at", "colour", "record", "update", "delete", "apply",
    "go", "back", "pause", "release", "goto", "cuelist", "scene", "chase", "macro", "timeline", "blackout", "gm", "clear", "park",
    "unpark", "undo", "redo", "blind", "locate",
];

/// Suggestions for the word being typed at the end of `prefix`.
pub fn complete(prefix: &str) -> Vec<String> {
    let cut = prefix
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let frag: String = prefix[cut..].chars().map(fold).collect();
    let before = prefix[..cut].trim_end_matches(|c: char| c == ' ' || c == '\t');
    let at_start = before.is_empty() || before.ends_with(';') || before.ends_with('\n');
    let prev: String = before
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .find(|s| !s.is_empty())
        .map(|s| s.chars().map(fold).collect())
        .unwrap_or_default();
    let pool: Vec<&str> = if at_start {
        TOP_LEVEL.to_vec()
    } else if let Some((_, f)) = FOLLOW.iter().find(|(k, _)| k.contains(&prev.as_str())) {
        if f.is_empty() { all_keywords() } else { f.to_vec() }
    } else {
        all_keywords()
    };
    let mut v: Vec<String> = pool.into_iter().filter(|k| k.starts_with(&frag) && *k != frag).map(|s| s.to_string()).collect();
    if !frag.is_empty() {
        v.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    }
    v.dedup();
    v.truncate(if frag.is_empty() { 40 } else { 16 });
    v
}

/// Grammar summary for `GET /api/cmdline/help`.
pub fn help_json() -> serde_json::Value {
    use serde_json::{json, Map, Value};
    // (name, syntax, description, examples)
    const STATEMENTS: &[(&str, &str, &str, &[&str])] = &[
        ("select", "fixture 1 thru 10 + 15 - 4 | group 2 | all | none | odd | even | next | prev | 1 thru 8", "Select fixtures (stays selected for the rest of the text)", &["fixture 1 thru 10 + 15 - 4", "group 2 odd", "1 thru 8"]),
        ("set", "[<sel>] [attr] at <value>", "Set a programmer value; attr defaults to dimmer; values 0-100, full, out, half, home or a fan 'A thru B'", &["fixture 1 thru 4 at 50", "dimmer 1 thru 8 at 70", "red at 100", "pan at 30", "fixture 1 thru 8 at 0 thru 100"]),
        ("colour", "[<sel>] colour <name>|<r> <g> <b>", "Set RGB colour", &["fixture 3 colour red", "group 1 color 100 40 0"]),
        ("record", "record cue [N] [cuelist L] [name X] [replace|merge] [release] | record palette [kind] [N] [name] | record scene [N] [name] [output] | record group N [name]", "Record programmer content", &["record cue 3", "grabar cue 5 lista 2 nombre Entrada", "record scene 2 Azul output", "fixture 1 thru 4; record group 5 Frontales"]),
        ("update", "update [cue] [N] [cuelist L] [replace|track|cueonly]", "Write the programmer into a cue", &["update cue 3", "update"]),
        ("delete", "delete cue|cuelist|group|palette|scene|effect|macro|chase|timeline N [cuelist L]", "Delete a pool item", &["delete cue 4", "borrar escena 2"]),
        ("apply", "[<sel>] apply palette N", "Apply a palette to the selection", &["group 1 apply palette 3"]),
        ("playback", "go|back|pause|release [cuelist L | L] | goto [cue] N [cuelist L] | cuelist L | release all", "Cuelist playback", &["go", "go cuelist 2", "cuelist 2; go; go", "goto cue 10"]),
        ("scene", "scene N [on|off|solo|toggle|at V] | scene solo N | scenes release", "Scenes", &["scene 1 on", "escena 3 at 60"]),
        ("chase", "chase N [on|off|tap|bpm V|next|prev]", "Chases", &["chase 1 on", "chase 2 bpm 128"]),
        ("macro", "macro N | macro stop [N]", "Macros", &["macro 3"]),
        ("timeline", "timeline N [run [from T]|stop|seek T]", "Timelines", &["timeline 1 run", "timeline 1 seek 30"]),
        ("system", "blackout [on|off|toggle] | gm V | clear | park U/C at V | unpark [U/C] | undo | redo | blind [on|off] | <sel> locate [on|off]", "Masters and utilities", &["gm 80", "blackout on", "park 1/5 at 100", "fixture 4 locate"]),
    ];
    let statements: Vec<Value> = STATEMENTS
        .iter()
        .map(|(n, syn, d, ex)| json!({"name": n, "syntax": syn, "description": d, "examples": ex}))
        .collect();
    let kws: &[(&str, &[&str])] = &[
        ("fixture", KW_FIXTURE), ("group", KW_GROUP), ("all", KW_ALL), ("none", KW_NONE), ("odd", KW_ODD), ("even", KW_EVEN),
        ("next", KW_NEXT), ("prev", KW_PREV), ("thru", KW_THRU), ("at", KW_AT), ("full", KW_FULL), ("out", KW_OUT),
        ("half", KW_HALF), ("home", KW_HOME), ("colour", KW_COLOUR), ("record", KW_RECORD), ("update", KW_UPDATE),
        ("delete", KW_DELETE), ("apply", KW_APPLY), ("go", KW_GO), ("back", KW_BACK), ("pause", KW_PAUSE),
        ("release", KW_RELEASE), ("goto", KW_GOTO), ("cue", KW_CUE), ("cuelist", KW_CUELIST), ("scene", KW_SCENE),
        ("chase", KW_CHASE), ("macro", KW_MACRO), ("timeline", KW_TIMELINE), ("blackout", KW_BLACKOUT), ("gm", KW_GM),
        ("clear", KW_CLEAR), ("park", KW_PARK), ("unpark", KW_UNPARK), ("undo", KW_UNDO), ("redo", KW_REDO),
        ("blind", KW_BLIND), ("locate", KW_LOCATE),
    ];
    let mut keywords = Map::new();
    for (k, v) in kws {
        keywords.insert((*k).to_string(), json!(v));
    }
    let attributes: Vec<Value> = ATTR_ALIASES.iter().map(|(c, a)| json!({"name": c, "aliases": a})).collect();
    let colours: Vec<Value> = COLOURS.iter().map(|(n, c)| json!({"names": n, "rgb": c})).collect();
    let mut root = Map::new();
    root.insert("version".into(), json!(1));
    root.insert("separators".into(), json!([";", "newline"]));
    root.insert("comments".into(), json!("# …"));
    root.insert("statements".into(), Value::Array(statements));
    root.insert("keywords".into(), Value::Object(keywords));
    root.insert("attributes".into(), Value::Array(attributes));
    root.insert("colours".into(), Value::Array(colours));
    Value::Object(root)
}

// ───────────────────────────── engine glue ─────────────────────────────

/// Last results shown in the UI (`Runtime.cmd_log`, newest last).
#[derive(Default)]
pub struct CmdLog(pub VecDeque<CmdLogLine>);

impl CmdLog {
    pub fn push(&mut self, text: &str, ok: bool, msg: impl Into<String>) {
        self.0.push_back(CmdLogLine { text: text.to_string(), ok, msg: msg.into() });
        while self.0.len() > 30 {
            self.0.pop_front();
        }
    }
}

impl Engine {
    fn cmd_ctx(&self) -> Ctx {
        let mut fixtures: Vec<u32> = self.show.fixtures.iter().map(|f| f.id).collect();
        fixtures.sort_unstable();
        Ctx {
            fixtures,
            groups: self.show.groups.iter().map(|g| (g.id, g.fixtures.clone())).collect(),
            cuelists: self.show.cuelists.iter().map(|c| c.id).collect(),
            current_cue: self.show.cuelists.iter().map(|c| (c.id, self.runs.get(&c.id).and_then(|r| r.current))).collect(),
            programmer: self.programmer.keys().copied().collect(),
            scenes_active: self.show.scenes.iter().map(|s| s.id).filter(|id| self.scenes.is_active(*id)).collect(),
            blackout: self.blackout,
        }
    }

    /// Run a command-line text in a fresh session.
    pub(super) fn run_cmdline_new(&mut self, text: &str) -> Result<(), String> {
        let mut s = Session::default();
        self.run_cmdline(text, &mut s)
    }

    /// Parse, resolve and execute `text`. Syntax errors abort before anything runs; a failing statement
    /// stops the rest (earlier statements stay applied). Every statement is logged to `cmd_log`.
    pub(super) fn run_cmdline(&mut self, text: &str, sess: &mut Session) -> Result<(), String> {
        let parsed = match parse(text) {
            Ok(p) => p,
            Err(e) => {
                let msg = e.render(text);
                self.auto.log.push(text.trim(), false, msg.clone());
                return Err(msg);
            }
        };
        for p in parsed {
            let ctx = self.cmd_ctx();
            let compiled = match resolve(&p.stmt, &ctx, sess) {
                Ok(c) => c,
                Err(mut e) => {
                    if e.pos == usize::MAX {
                        e.pos = p.pos;
                        e.len = p.text.len();
                    }
                    let msg = e.render(text);
                    self.auto.log.push(&p.text, false, msg.clone());
                    return Err(msg);
                }
            };
            for cmd in compiled.cmds {
                if let Err(e) = self.handle(cmd) {
                    let msg = format!("{e}");
                    self.auto.log.push(&p.text, false, msg.clone());
                    return Err(format!("{}: {msg}", p.text));
                }
            }
            self.auto.log.push(&p.text, true, compiled.summary);
        }
        self.dirty = true;
        Ok(())
    }
}

// ───────────────────────────── tests ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        Ctx {
            fixtures: (1..=12).collect(),
            groups: vec![(1, vec![1, 2, 3]), (2, vec![4, 5, 6, 7])],
            cuelists: vec![1, 2],
            current_cue: vec![(1, Some(3.0)), (2, None)],
            programmer: vec![1, 2],
            scenes_active: vec![5],
            blackout: false,
        }
    }

    /// Parse + resolve every statement of `text` in one session.
    fn run(text: &str) -> Result<Vec<Command>, String> {
        let mut s = Session::default();
        run_in(text, &mut s)
    }

    fn run_in(text: &str, s: &mut Session) -> Result<Vec<Command>, String> {
        let c = ctx();
        let mut out = vec![];
        for p in parse(text).map_err(|e| e.render(text))? {
            let r = resolve(&p.stmt, &c, s).map_err(|mut e| {
                if e.pos == usize::MAX {
                    e.pos = p.pos;
                    e.len = p.text.len();
                }
                e.render(text)
            })?;
            out.extend(r.cmds);
        }
        Ok(out)
    }

    fn progset(c: &Command) -> (Vec<u32>, Attrs) {
        match c {
            Command::ProgSet { fixtures, attrs } => (fixtures.clone(), attrs.clone()),
            o => panic!("expected ProgSet, got {o:?}"),
        }
    }

    fn sel_of(text: &str) -> Vec<u32> {
        let mut s = Session::default();
        run_in(text, &mut s).unwrap();
        s.selection
    }

    fn err_of(text: &str) -> String {
        run(text).unwrap_err()
    }

    // ── lexer ──

    #[test]
    fn lex_numbers_and_floats() {
        let t = lex("1.5 20 3.").unwrap();
        assert_eq!(t[0].t, T::Num(1.5));
        assert_eq!(t[1].t, T::Num(20.0));
        assert_eq!(t[2].t, T::Num(3.0));
    }

    #[test]
    fn lex_words_fold_accents_and_case() {
        let t = lex("ÁMBAR Atrás").unwrap();
        assert_eq!(t[0].t, T::Word("ambar".into()));
        assert_eq!(t[1].t, T::Word("atras".into()));
    }

    #[test]
    fn lex_strings_and_comments() {
        let t = lex("name \"Hola mundo\" # nothing here\ngo").unwrap();
        assert!(t.iter().any(|x| x.t == T::Str("Hola mundo".into())));
        assert!(t.iter().any(|x| x.t == T::Sep));
        assert_eq!(t.last().unwrap().t, T::Word("go".into()));
    }

    #[test]
    fn lex_unterminated_quote_and_bad_char() {
        assert!(err_of("record cue 1 name \"oops").contains("unterminated quote"));
        assert!(err_of("go $").contains("unexpected character '$'"));
    }

    #[test]
    fn parse_separators() {
        assert_eq!(parse("go; go\n back ;; pause").unwrap().len(), 4);
        assert!(parse("   \n ; ").unwrap().is_empty());
    }

    // ── selection ──

    #[test]
    fn select_thru() {
        assert_eq!(sel_of("fixture 1 thru 4"), vec![1, 2, 3, 4]);
    }

    #[test]
    fn select_plus_minus() {
        assert_eq!(sel_of("fixture 1 thru 10 + 12 - 4"), vec![1, 2, 3, 5, 6, 7, 8, 9, 10, 12]);
    }

    #[test]
    fn select_bare_numbers() {
        assert_eq!(sel_of("1 thru 3 + 9"), vec![1, 2, 3, 9]);
    }

    #[test]
    fn select_group_and_minus_group() {
        assert_eq!(sel_of("group 2"), vec![4, 5, 6, 7]);
        assert_eq!(sel_of("fixture 1 thru 8 - group 1"), vec![4, 5, 6, 7, 8]);
    }

    #[test]
    fn select_all_none() {
        assert_eq!(sel_of("all").len(), 12);
        assert_eq!(sel_of("all; none"), Vec::<u32>::new());
    }

    #[test]
    fn select_odd_even_trailing() {
        assert_eq!(sel_of("fixture 1 thru 8 odd"), vec![1, 3, 5, 7]);
        assert_eq!(sel_of("fixture 1 thru 8 even"), vec![2, 4, 6, 8]);
    }

    #[test]
    fn select_odd_even_standalone_filters_current_selection() {
        assert_eq!(sel_of("fixture 3 thru 8; odd"), vec![3, 5, 7]);
        assert_eq!(sel_of("fixture 3 thru 8; even"), vec![4, 6, 8]);
        assert_eq!(sel_of("odd"), vec![1, 3, 5, 7, 9, 11]);
    }

    #[test]
    fn select_next_prev_wrap() {
        assert_eq!(sel_of("fixture 5; next"), vec![6]);
        assert_eq!(sel_of("fixture 12; next"), vec![1]);
        assert_eq!(sel_of("fixture 1; prev"), vec![12]);
        assert_eq!(sel_of("next"), vec![1]);
        assert_eq!(sel_of("prev"), vec![12]);
    }

    #[test]
    fn select_spanish_aliases() {
        assert_eq!(sel_of("aparato 1 hasta 3 y 7"), vec![1, 2, 3, 7]);
        assert_eq!(sel_of("grupo 1"), vec![1, 2, 3]);
        assert_eq!(sel_of("todos menos grupo 1").len(), 9);
        assert_eq!(sel_of("aparatos 1 hasta 6 impares"), vec![1, 3, 5]);
    }

    #[test]
    fn select_errors() {
        assert!(err_of("fixture 99").contains("fixture 99 does not exist"));
        assert!(err_of("fixture 50 thru 60").contains("no fixtures between 50 and 60"));
        assert!(err_of("group 9").contains("group 9 does not exist"));
        assert!(err_of("fixture").contains("expected a fixture number"));
        assert!(err_of("fixture 1 thru").contains("after 'thru'"));
        assert!(err_of("fixture 1.5").contains("whole number"));
    }

    #[test]
    fn select_dedupes() {
        assert_eq!(sel_of("fixture 1 thru 3 + 2 + 3"), vec![1, 2, 3]);
    }

    // ── values ──

    #[test]
    fn at_percent_defaults_to_dimmer() {
        let c = run("fixture 1 thru 4 at 50").unwrap();
        let (fx, a) = progset(&c[0]);
        assert_eq!(fx, vec![1, 2, 3, 4]);
        assert_eq!(a["dimmer"], 0.5);
    }

    #[test]
    fn at_word_values() {
        assert_eq!(progset(&run("fixture 1 at full").unwrap()[0]).1["dimmer"], 1.0);
        assert_eq!(progset(&run("fixture 1 at out").unwrap()[0]).1["dimmer"], 0.0);
        assert_eq!(progset(&run("fixture 1 at half").unwrap()[0]).1["dimmer"], 0.5);
        assert_eq!(progset(&run("fixture 1 @ 25").unwrap()[0]).1["dimmer"], 0.25);
        assert_eq!(progset(&run("fixture 1 en 75").unwrap()[0]).1["dimmer"], 0.75);
        assert_eq!(progset(&run("fixture 1 at pleno").unwrap()[0]).1["dimmer"], 1.0);
    }

    #[test]
    fn at_home_uses_attribute_default() {
        assert_eq!(progset(&run("fixture 1 pan at home").unwrap()[0]).1["pan"], 0.5);
        assert_eq!(progset(&run("fixture 1 dimmer at home").unwrap()[0]).1["dimmer"], 0.0);
        assert_eq!(progset(&run("fixture 1 red at reposo").unwrap()[0]).1["red"], 0.0);
    }

    #[test]
    fn fan_spreads_values() {
        let c = run("fixture 1 thru 5 at 0 thru 100").unwrap();
        assert_eq!(c.len(), 5);
        let v: Vec<f32> = c.iter().map(|x| progset(x).1["dimmer"]).collect();
        assert_eq!(v, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        let one = run("fixture 1 at 20 thru 80").unwrap();
        assert!((progset(&one[0]).1["dimmer"] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn value_out_of_range() {
        assert!(err_of("fixture 1 at 150").contains("between 0 and 100"));
        assert!(err_of("fixture 1 at").contains("expected a value"));
        assert!(err_of("fixture 1 at banana").contains("expected a value"));
    }

    #[test]
    fn attribute_led_forms() {
        let c = run("dimmer 1 thru 8 at 70").unwrap();
        let (fx, a) = progset(&c[0]);
        assert_eq!(fx.len(), 8);
        assert_eq!(a["dimmer"], 0.7);
        let c = run("fixture 2; pan at 30").unwrap();
        assert_eq!(progset(&c[0]).0, vec![2]);
        assert!((progset(&c[0]).1["pan"] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn selection_persists_across_statements() {
        let c = run("fixture 1 thru 4 at 50; red at 100; blue at 20").unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(progset(&c[1]).0, vec![1, 2, 3, 4]);
        assert_eq!(progset(&c[2]).1["blue"], 0.2);
    }

    #[test]
    fn attribute_after_selection() {
        let c = run("fixture 3 red at 80").unwrap();
        assert_eq!(progset(&c[0]).1["red"], 0.8);
    }

    #[test]
    fn spanish_attributes() {
        assert_eq!(progset(&run("fixture 1; rojo en 100").unwrap()[0]).1["red"], 1.0);
        assert_eq!(progset(&run("fixture 1; azul en 40").unwrap()[0]).1["blue"], 0.4);
        assert_eq!(progset(&run("fixture 1; ámbar en 40").unwrap()[0]).1["amber"], 0.4);
        assert_eq!(progset(&run("fixture 1; intensidad en 10").unwrap()[0]).1["dimmer"], 0.1);
    }

    #[test]
    fn raw_attribute_names_for_imported_profiles() {
        let c = run("fixture 1; colorsub_c at 40").unwrap();
        assert_eq!(progset(&c[0]).1["colorsub_c"], 0.4);
    }

    #[test]
    fn bare_at_sets_dimmer_on_selection() {
        let c = run("fixture 1 thru 2; at 60").unwrap();
        assert_eq!(progset(&c[0]).1["dimmer"], 0.6);
    }

    #[test]
    fn set_without_selection_errors() {
        assert!(err_of("at 50").contains("no fixtures selected"));
        assert!(err_of("red at 100").contains("no fixtures selected"));
    }

    #[test]
    fn missing_at_after_attribute() {
        assert!(err_of("red").contains("expected 'at <value>' after 'red'"));
    }

    // ── colour ──

    #[test]
    fn colour_names_and_numbers() {
        let a = progset(&run("fixture 3 colour red").unwrap()[0]).1;
        assert_eq!((a["red"], a["green"], a["blue"]), (1.0, 0.0, 0.0));
        let a = progset(&run("fixture 3 color azul").unwrap()[0]).1;
        assert_eq!((a["red"], a["green"], a["blue"]), (0.0, 0.0, 1.0));
        let a = progset(&run("group 1 color 100 50 0").unwrap()[0]).1;
        assert_eq!((a["red"], a["green"], a["blue"]), (1.0, 0.5, 0.0));
        let a = progset(&run("fixture 1; colour blanco").unwrap()[0]).1;
        assert_eq!((a["red"], a["green"], a["blue"]), (1.0, 1.0, 1.0));
    }

    #[test]
    fn colour_errors() {
        assert!(err_of("fixture 1 colour banana").contains("unknown colour"));
        assert!(err_of("fixture 1 colour 120 0 0").contains("0-100"));
    }

    // ── record / update / delete / apply ──

    #[test]
    fn record_cue_variants() {
        match &run("record cue 3").unwrap()[0] {
            Command::ProgRecordCue { cuelist, number, mode, release, refs, .. } => {
                assert_eq!((*cuelist, *number, *mode, *release, *refs), (1, Some(3.0), RecordMode::Merge, false, true));
            }
            o => panic!("{o:?}"),
        }
        match &run("record cue 2.5 cuelist 2 replace release name Entrada Sala").unwrap()[0] {
            Command::ProgRecordCue { cuelist, number, mode, release, label, .. } => {
                assert_eq!((*cuelist, *number, *mode, *release), (2, Some(2.5), RecordMode::Replace, true));
                assert_eq!(label, "Entrada Sala");
            }
            o => panic!("{o:?}"),
        }
        match &run("grabar cue 5 lista 2 nombre \"Final Acto\"").unwrap()[0] {
            Command::ProgRecordCue { cuelist, label, .. } => assert_eq!((*cuelist, label.as_str()), (2, "Final Acto")),
            o => panic!("{o:?}"),
        }
        match &run("record cue").unwrap()[0] {
            Command::ProgRecordCue { number, .. } => assert_eq!(*number, None),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn record_uses_session_cuelist() {
        match &run("cuelist 2; record cue 4").unwrap()[0] {
            Command::ProgRecordCue { cuelist, .. } => assert_eq!(*cuelist, 2),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn record_palette_kinds() {
        match &run("record palette color 3 Rojos").unwrap()[0] {
            Command::ProgRecordPalette { id, kind, name, fixtures } => {
                assert_eq!((*id, *kind, name.as_str()), (Some(3), PaletteKind::Color, "Rojos"));
                assert_eq!(fixtures, &vec![1, 2]); // programmer fixtures from ctx
            }
            o => panic!("{o:?}"),
        }
        match &run("fixture 5 thru 6; record palette position").unwrap()[0] {
            Command::ProgRecordPalette { kind, fixtures, id, .. } => {
                assert_eq!((*kind, fixtures.clone(), *id), (PaletteKind::Position, vec![5, 6], None));
            }
            o => panic!("{o:?}"),
        }
        assert!(matches!(&run("record palette intensidad 1").unwrap()[0], Command::ProgRecordPalette { kind: PaletteKind::Intensity, .. }));
        assert!(matches!(&run("record palette haz 1").unwrap()[0], Command::ProgRecordPalette { kind: PaletteKind::Beam, .. }));
    }

    #[test]
    fn record_palette_with_empty_programmer_errors() {
        let mut c = ctx();
        c.programmer.clear();
        let p = parse("record palette 1").unwrap();
        let mut s = Session::default();
        assert!(resolve(&p[0].stmt, &c, &mut s).is_err());
    }

    #[test]
    fn record_scene_and_group() {
        match &run("record scene 2 Azul").unwrap()[0] {
            Command::SceneRecord { id, name, from_output, fixtures, .. } => {
                assert_eq!((*id, name.as_str(), *from_output, fixtures.clone()), (Some(2), "Azul", false, None));
            }
            o => panic!("{o:?}"),
        }
        match &run("fixture 1 thru 2; record scene 4 Look output").unwrap()[0] {
            Command::SceneRecord { id, name, from_output, fixtures, .. } => {
                assert_eq!((*id, name.as_str(), *from_output, fixtures.clone()), (Some(4), "Look", true, Some(vec![1, 2])));
            }
            o => panic!("{o:?}"),
        }
        match &run("fixture 1 thru 3; record group 5 Frontales").unwrap()[0] {
            Command::GroupSave { group } => assert_eq!((group.id, group.name.as_str(), group.fixtures.clone()), (5, "Frontales", vec![1, 2, 3])),
            o => panic!("{o:?}"),
        }
        assert!(err_of("record group 5").contains("no fixtures selected"));
        assert!(err_of("record banana").contains("record what?"));
    }

    #[test]
    fn update_cue() {
        match &run("update cue 3").unwrap()[0] {
            Command::CueUpdate { cuelist, number, mode, refs } => assert_eq!((*cuelist, *number, *mode, *refs), (1, 3.0, UpdateMode::Merge, true)),
            o => panic!("{o:?}"),
        }
        match &run("update").unwrap()[0] {
            Command::CueUpdate { number, .. } => assert_eq!(*number, 3.0), // current cue of list 1
            o => panic!("{o:?}"),
        }
        assert!(matches!(&run("update cue 2 replace").unwrap()[0], Command::CueUpdate { mode: UpdateMode::Replace, .. }));
        assert!(matches!(&run("update cue 2 track").unwrap()[0], Command::CueUpdate { mode: UpdateMode::TrackForward, .. }));
        assert!(matches!(&run("actualizar cue 2 cueonly").unwrap()[0], Command::CueUpdate { mode: UpdateMode::CueOnly, .. }));
        assert!(err_of("update cuelist 2").contains("no current cue"));
    }

    #[test]
    fn delete_variants() {
        assert!(matches!(&run("delete cue 4").unwrap()[0], Command::CueRemove { cuelist: 1, number } if *number == 4.0));
        assert!(matches!(&run("delete cue 4 cuelist 2").unwrap()[0], Command::CueRemove { cuelist: 2, .. }));
        assert!(matches!(&run("delete group 3").unwrap()[0], Command::GroupRemove { id: 3 }));
        assert!(matches!(&run("borrar escena 2").unwrap()[0], Command::SceneRemove { id: 2 }));
        assert!(matches!(&run("delete palette 1").unwrap()[0], Command::PaletteRemove { id: 1 }));
        assert!(matches!(&run("delete effect 1").unwrap()[0], Command::EffectRemove { id: 1 }));
        assert!(matches!(&run("delete macro 1").unwrap()[0], Command::MacroRemove { id: 1 }));
        assert!(matches!(&run("delete chase 1").unwrap()[0], Command::ChaseRemove { id: 1 }));
        assert!(matches!(&run("delete timeline 1").unwrap()[0], Command::TimelineRemove { id: 1 }));
        assert!(matches!(&run("delete cuelist 2").unwrap()[0], Command::CuelistRemove { id: 2 }));
        assert!(err_of("delete everything").contains("delete what?"));
        assert!(err_of("delete group 1.5").contains("whole number"));
    }

    #[test]
    fn apply_palette() {
        match &run("fixture 1 thru 3; apply palette 2").unwrap()[0] {
            Command::ProgApplyPalette { fixtures, palette } => assert_eq!((fixtures.clone(), *palette), (vec![1, 2, 3], 2)),
            o => panic!("{o:?}"),
        }
        assert!(matches!(&run("group 2 apply palette 5").unwrap()[0], Command::ProgApplyPalette { palette: 5, .. }));
        assert!(matches!(&run("aplicar paleta 1").unwrap_err().as_str(), s if s.contains("no fixtures selected")));
    }

    // ── playback ──

    #[test]
    fn go_back_pause_release() {
        assert!(matches!(&run("go").unwrap()[0], Command::Go { cuelist: 1 }));
        assert!(matches!(&run("go cuelist 2").unwrap()[0], Command::Go { cuelist: 2 }));
        assert!(matches!(&run("go 2").unwrap()[0], Command::Go { cuelist: 2 }));
        assert!(matches!(&run("back").unwrap()[0], Command::Back { cuelist: 1 }));
        assert!(matches!(&run("pause lista 2").unwrap()[0], Command::Pause { cuelist: 2 }));
        assert!(matches!(&run("release").unwrap()[0], Command::ReleaseCuelist { cuelist: 1 }));
        assert!(matches!(&run("liberar 2").unwrap()[0], Command::ReleaseCuelist { cuelist: 2 }));
        assert!(matches!(&run("ir").unwrap()[0], Command::Go { .. }));
        assert!(matches!(&run("atras").unwrap()[0], Command::Back { .. }));
        assert!(matches!(&run("pausa").unwrap()[0], Command::Pause { .. }));
    }

    #[test]
    fn cuelist_prefix_sets_default() {
        let c = run("cuelist 2; go; go; back").unwrap();
        assert!(matches!(&c[0], Command::Go { cuelist: 2 }));
        assert!(matches!(&c[2], Command::Back { cuelist: 2 }));
        assert!(matches!(&run("cuelist 2 go").unwrap()[0], Command::Go { cuelist: 2 }));
    }

    #[test]
    fn release_all_includes_scenes() {
        let c = run("release all").unwrap();
        assert_eq!(c.len(), 3);
        assert!(matches!(c.last().unwrap(), Command::ScenesRelease));
        assert!(matches!(&run("liberar todo").unwrap()[0], Command::ReleaseCuelist { .. }));
    }

    #[test]
    fn goto_cue() {
        assert!(matches!(&run("goto cue 10").unwrap()[0], Command::GoTo { cuelist: 1, number } if *number == 10.0));
        assert!(matches!(&run("goto 2.5 cuelist 2").unwrap()[0], Command::GoTo { cuelist: 2, number } if *number == 2.5));
        assert!(err_of("goto").contains("expected a cue number"));
    }

    #[test]
    fn scene_verbs() {
        assert!(matches!(&run("scene 1 on").unwrap()[0], Command::SceneGo { id: 1, on: true }));
        assert!(matches!(&run("scene 1 off").unwrap()[0], Command::SceneGo { id: 1, on: false }));
        assert!(matches!(&run("scene 1 solo").unwrap()[0], Command::SceneSolo { id: 1 }));
        assert!(matches!(&run("scene solo 3").unwrap()[0], Command::SceneSolo { id: 3 }));
        assert!(matches!(&run("scene 1").unwrap()[0], Command::SceneGo { id: 1, on: true }));
        assert!(matches!(&run("escena 5").unwrap()[0], Command::SceneGo { id: 5, on: false })); // 5 is active in ctx → toggles off
        assert!(matches!(&run("scene 2 at 60").unwrap()[0], Command::SceneLevel { id: 2, level } if (*level - 0.6).abs() < 1e-6));
        assert!(matches!(&run("scenes release").unwrap()[0], Command::ScenesRelease));
        assert!(matches!(&run("release scene").unwrap()[0], Command::ScenesRelease));
        assert!(err_of("scene 1 banana").contains("expected on, off"));
        assert!(err_of("scene 1 at 300").contains("0-100"));
    }

    #[test]
    fn chase_verbs() {
        assert!(matches!(&run("chase 1 on").unwrap()[0], Command::ChaseRun { id: 1, on: true }));
        assert!(matches!(&run("chase 1").unwrap()[0], Command::ChaseRun { id: 1, on: true }));
        assert!(matches!(&run("chase 1 off").unwrap()[0], Command::ChaseRun { id: 1, on: false }));
        assert!(matches!(&run("chase 2 bpm 128").unwrap()[0], Command::ChaseBpm { id: 2, bpm } if *bpm == 128.0));
        assert!(matches!(&run("chase 2 tap").unwrap()[0], Command::ChaseTap { id: 2 }));
        assert!(matches!(&run("chase 2 next").unwrap()[0], Command::ChaseNudge { id: 2, delta: 1 }));
        assert!(matches!(&run("chase 2 prev").unwrap()[0], Command::ChaseNudge { id: 2, delta: -1 }));
        assert!(err_of("chase 1 dance").contains("expected on, off"));
    }

    #[test]
    fn macro_and_timeline() {
        assert!(matches!(&run("macro 3").unwrap()[0], Command::MacroRun { id: 3 }));
        assert!(matches!(&run("macro stop").unwrap()[0], Command::MacroStop { id: None }));
        assert!(matches!(&run("macro stop 2").unwrap()[0], Command::MacroStop { id: Some(2) }));
        assert!(matches!(&run("timeline 1 run").unwrap()[0], Command::TimelineRun { id: 1, from: None }));
        assert!(matches!(&run("timeline 1").unwrap()[0], Command::TimelineRun { id: 1, from: None }));
        assert!(matches!(&run("timeline 1 run from 12.5").unwrap()[0], Command::TimelineRun { id: 1, from: Some(f) } if *f == 12.5));
        assert!(matches!(&run("tl 1 stop").unwrap()[0], Command::TimelineStop { id: 1 }));
        assert!(matches!(&run("timeline 2 seek 30").unwrap()[0], Command::TimelineSeek { id: 2, t } if *t == 30.0));
        assert!(err_of("timeline 1 rewind").contains("expected run, stop or seek"));
    }

    #[test]
    fn blackout_gm_clear() {
        assert!(matches!(&run("blackout on").unwrap()[0], Command::Blackout { on: true }));
        assert!(matches!(&run("blackout off").unwrap()[0], Command::Blackout { on: false }));
        assert!(matches!(&run("bo").unwrap()[0], Command::Blackout { on: true })); // toggles from off
        assert!(matches!(&run("blackout toggle").unwrap()[0], Command::Blackout { on: true }));
        assert!(matches!(&run("gm 80").unwrap()[0], Command::GrandMaster { level } if (*level - 0.8).abs() < 1e-6));
        assert!(matches!(&run("master at 50").unwrap()[0], Command::GrandMaster { level } if (*level - 0.5).abs() < 1e-6));
        assert!(matches!(&run("gm full").unwrap()[0], Command::GrandMaster { level } if *level == 1.0));
        assert!(matches!(&run("clear").unwrap()[0], Command::ProgRelease { fixtures: None, attrs: None }));
        assert!(matches!(&run("clr").unwrap()[0], Command::ProgRelease { .. }));
        assert!(matches!(&run("limpiar").unwrap()[0], Command::ProgRelease { .. }));
        assert!(err_of("gm home").contains("not valid"));
        assert!(err_of("blackout maybe").contains("expected on, off or toggle"));
    }

    #[test]
    fn clear_resets_selection() {
        let mut s = Session::default();
        run_in("fixture 1 thru 3; clear", &mut s).unwrap();
        assert!(s.selection.is_empty());
    }

    #[test]
    fn park_and_unpark() {
        assert!(matches!(&run("park 1/5 at 100").unwrap()[0], Command::DmxSet { universe: 1, channel: 5, value: Some(255) }));
        assert!(matches!(&run("park 2/10 at 50").unwrap()[0], Command::DmxSet { universe: 2, channel: 10, value: Some(128) }));
        assert!(matches!(&run("park 7 at full").unwrap()[0], Command::DmxSet { universe: 1, channel: 7, value: Some(255) }));
        assert!(matches!(&run("aparcar 1/2 en 0").unwrap()[0], Command::DmxSet { value: Some(0), .. }));
        assert!(matches!(&run("unpark").unwrap()[0], Command::Unpark { universe: None, channel: None }));
        assert!(matches!(&run("unpark 1/5").unwrap()[0], Command::Unpark { universe: Some(1), channel: Some(5) }));
        assert!(err_of("park 1/600 at 50").contains("channel 1-512"));
        assert!(err_of("park 1/5").contains("expected a value"));
    }

    #[test]
    fn undo_redo_blind() {
        assert!(matches!(&run("undo").unwrap()[0], Command::Undo));
        assert!(matches!(&run("deshacer").unwrap()[0], Command::Undo));
        assert!(matches!(&run("redo").unwrap()[0], Command::Redo));
        assert!(matches!(&run("rehacer").unwrap()[0], Command::Redo));
        assert!(matches!(&run("blind").unwrap()[0], Command::BlindSet { on: true }));
        assert!(matches!(&run("blind on").unwrap()[0], Command::BlindSet { on: true }));
        assert!(matches!(&run("ciego off").unwrap()[0], Command::BlindSet { on: false }));
    }

    #[test]
    fn locate() {
        let c = run("fixture 2 thru 3 locate").unwrap();
        assert_eq!(c.len(), 2);
        assert!(matches!(&c[0], Command::Identify { fixture: 2, on: true }));
        assert!(matches!(&run("fixture 4; locate off").unwrap()[0], Command::Identify { fixture: 4, on: false }));
        assert!(matches!(&run("fixture 4 identificar").unwrap()[0], Command::Identify { fixture: 4, on: true }));
    }

    // ── errors ──

    #[test]
    fn unknown_command_suggests() {
        let e = err_of("gobo2 xx");
        assert!(e.contains("unknown command") || e.contains("expected"), "{e}");
        let e = err_of("recrod cue 1");
        assert!(e.contains("unknown command 'recrod'"), "{e}");
        assert!(e.contains("record"), "{e}");
        let e = err_of("blakout on");
        assert!(e.contains("blackout"), "{e}");
    }

    #[test]
    fn trailing_garbage_is_an_error() {
        assert!(err_of("go cuelist 1 banana").contains("unexpected"));
        assert!(err_of("clear now").contains("unexpected"));
    }

    #[test]
    fn operator_start_is_an_error() {
        assert!(err_of("+ 3").contains("cannot start with an operator"));
        assert!(err_of("\"hola\"").contains("quoted"));
    }

    #[test]
    fn error_positions_are_one_based_columns() {
        let e = parse("go; fixture 1 thru").unwrap_err();
        let r = e.render("go; fixture 1 thru");
        assert!(r.contains("col 19"), "{r}");
        let text = "fixture 1; blakout";
        let e = parse(text).unwrap_err();
        assert!(e.render(text).contains("col 12"), "{}", e.render(text));
        assert!(e.render(text).contains("«blakout»"));
    }

    #[test]
    fn semantic_error_points_at_statement() {
        let text = "go; fixture 99 at 50";
        let e = run(text).unwrap_err();
        assert!(e.contains("fixture 99 does not exist"), "{e}");
        assert!(e.contains("col 5"), "{e}");
    }

    #[test]
    fn syntax_error_in_second_statement_runs_nothing() {
        // parse() fails as a whole, so no command is produced
        assert!(parse("go; banana banana").is_err());
    }

    #[test]
    fn too_long_text_rejected() {
        let long = "go;".repeat(3000);
        assert!(parse(&long).is_err());
    }

    // ── completion & help ──

    #[test]
    fn complete_verbs_at_start() {
        let c = complete("rec");
        assert!(c.contains(&"record".to_string()), "{c:?}");
        let c = complete("go; bla");
        assert!(c.contains(&"blackout".to_string()) || c.contains(&"blind".to_string()), "{c:?}");
        assert!(complete("").contains(&"fixture".to_string()));
    }

    #[test]
    fn complete_contextual_follow_ups() {
        let c = complete("fixture 1 at ");
        assert!(c.contains(&"full".to_string()) && c.contains(&"out".to_string()), "{c:?}");
        let c = complete("record ");
        assert!(c.contains(&"cue".to_string()) && c.contains(&"scene".to_string()), "{c:?}");
        let c = complete("record sc");
        assert_eq!(c, vec!["scene".to_string()]);
        let c = complete("fixture 1 colour ma");
        assert!(c.contains(&"magenta".to_string()), "{c:?}");
    }

    #[test]
    fn complete_is_accent_insensitive_and_limited() {
        assert!(complete("aml").is_empty() || complete("am").contains(&"ambar".to_string()) || complete("am").contains(&"amber".to_string()));
        assert!(complete("a").len() <= 16);
    }

    #[test]
    fn help_json_shape() {
        let h = help_json();
        assert_eq!(h["version"], 1);
        assert!(h["statements"].as_array().unwrap().len() >= 10);
        assert!(h["keywords"]["fixture"].as_array().unwrap().iter().any(|x| x == "aparato"));
        assert!(h["attributes"].as_array().unwrap().iter().any(|a| a["name"] == "dimmer"));
        assert!(h["colours"].as_array().unwrap().len() >= 10);
    }

    // ── engine integration ──

    use crate::engine::tests_support::*;

    #[test]
    fn engine_runs_a_multi_statement_text() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 4);
        e.handle(Command::CommandLine { text: "fixture 1 thru 4 colour red; at 50".into() }).unwrap();
        let f = frame(&mut e);
        // rgb-par has no dimmer channel: virtual dimmer scales colour
        assert_eq!(f[0], 128);
        assert_eq!(f[1], 0);
        assert_eq!(f[9], 128); // fixture 4 red
        let log = &e.auto.log.0;
        assert_eq!(log.len(), 2);
        assert!(log.iter().all(|l| l.ok));
        assert_eq!(log[0].text, "fixture 1 thru 4 colour red");
    }

    #[test]
    fn engine_logs_errors_and_stops() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 2);
        let r = e.handle(Command::CommandLine { text: "fixture 1 at 100; fixture 9 at 50; fixture 2 at 100".into() });
        let msg = r.unwrap_err();
        assert!(msg.contains("fixture 9 does not exist"), "{msg}");
        // first statement applied, third never ran
        assert!(e.programmer.get(&1).is_some());
        assert!(e.programmer.get(&2).is_none());
        let log = &e.auto.log.0;
        assert!(log[0].ok && !log[1].ok);
    }

    #[test]
    fn engine_syntax_error_runs_nothing() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        let r = e.handle(Command::CommandLine { text: "fixture 1 at 100; wibble".into() });
        assert!(r.unwrap_err().contains("unknown command"));
        assert!(e.programmer.is_empty());
        assert_eq!(e.auto.log.0.len(), 1);
        assert!(!e.auto.log.0[0].ok);
    }

    #[test]
    fn engine_record_and_go_via_text() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        e.handle(Command::CommandLine { text: "fixture 1 at 80; record cue 1 release; fixture 1 at 20; record cue 2 release".into() }).unwrap();
        assert_eq!(e.show.cuelists[0].cues.len(), 2);
        for c in &mut e.show.cuelists[0].cues {
            c.fade_in = 0.0;
            c.fade_out = 0.0;
        }
        e.handle(Command::CommandLine { text: "go; go".into() }).unwrap();
        e.update_playback(0.5);
        assert_eq!(frame(&mut e)[0], 51);
    }

    #[test]
    fn engine_cmd_log_is_capped_at_30() {
        let mut e = engine();
        for _ in 0..45 {
            let _ = e.run_cmdline_new("blackout toggle");
        }
        assert_eq!(e.auto.log.0.len(), 30);
        let rt = e.build_rt();
        assert_eq!(rt.cmd_log.len(), 30);
    }

    #[test]
    fn engine_selection_is_per_text() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 2);
        e.handle(Command::CommandLine { text: "fixture 1".into() }).unwrap();
        let r = e.handle(Command::CommandLine { text: "at 50".into() });
        assert!(r.unwrap_err().contains("no fixtures selected"));
    }

    #[test]
    fn programmer_overrides_a_releasing_cuelist() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        e.handle(Command::CommandLine {
            text: "fixture 1 at 100; record cue 1 cuelist 9 release; fixture 1 at 20; record cue 2 cuelist 9 release; go cuelist 9; go cuelist 9; release all".into(),
        })
        .unwrap();
        e.handle(Command::CommandLine { text: "fixture 1 at 100".into() }).unwrap();
        let t = e.now();
        e.update_playback(t + 0.3);
        assert_eq!(frame(&mut e)[0], 255, "programmer 100% must win over a releasing cuelist");
    }
}
