//! PDF reports: patch list, channel hookup, cue sheets, power report, palette/scene pools.
//! Spanish by default (`lang=en` for English). A4, repeated table headers, page numbers.

use crate::library::{builtin, mode_footprint};
use crate::model::*;
use crate::pdf::{fit, text_width, Doc, PAGE_H, PAGE_W};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    pub fn parse(s: Option<&str>) -> Lang {
        match s.map(|x| x.trim().to_lowercase()) {
            Some(x) if x == "en" || x.starts_with("en-") => Lang::En,
            _ => Lang::Es,
        }
    }
    fn t<'a>(self, es: &'a str, en: &'a str) -> &'a str {
        match self {
            Lang::Es => es,
            Lang::En => en,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Patch,
    Channels,
    Cues,
    Power,
    Scenes,
}

pub fn kind_from(name: &str) -> Option<Kind> {
    match name.trim_end_matches(".pdf") {
        "patch" => Some(Kind::Patch),
        "channels" => Some(Kind::Channels),
        "cues" => Some(Kind::Cues),
        "power" => Some(Kind::Power),
        "scenes" => Some(Kind::Scenes),
        _ => None,
    }
}

/// `YYYY-MM-DD` (UTC) from a unix timestamp.
pub fn date_string(unix_secs: u64) -> String {
    let z = (unix_secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    date_string(secs)
}

// ───────────────────────────── layout ─────────────────────────────

const MARGIN: f32 = 40.0;
const TOP: f32 = PAGE_H - 62.0;
const BOTTOM: f32 = 48.0;
const FONT: f32 = 8.0;
const ROW_H: f32 = 13.0;

struct Col {
    title: String,
    weight: f32,
    right: bool,
}

fn col(title: &str, weight: f32) -> Col {
    Col { title: title.to_string(), weight, right: false }
}
fn rcol(title: &str, weight: f32) -> Col {
    Col { title: title.to_string(), weight, right: true }
}

struct Rep<'a> {
    doc: Doc,
    y: f32,
    page: usize,
    title: String,
    show: &'a str,
    date: &'a str,
    lang: Lang,
}

impl<'a> Rep<'a> {
    fn new(title: &str, show: &'a str, date: &'a str, lang: Lang) -> Self {
        let mut r = Rep { doc: Doc::new(&format!("{title} - {show}")), y: TOP, page: 0, title: title.to_string(), show, date, lang };
        r.new_page();
        r
    }

    fn new_page(&mut self) {
        self.page = self.doc.add_page();
        let p = &mut self.doc.pages[self.page];
        p.text(MARGIN, PAGE_H - 40.0, 15.0, true, &self.title);
        let right = format!("{} · {}", self.show, self.date);
        p.text_right(PAGE_W - MARGIN, PAGE_H - 38.0, 9.0, false, &fit(&right, 300.0, 9.0, false));
        p.line(MARGIN, PAGE_H - 48.0, PAGE_W - MARGIN, PAGE_H - 48.0, 0.8, 0.4);
        self.y = TOP;
    }

    fn ensure(&mut self, h: f32) {
        if self.y - h < BOTTOM {
            self.new_page();
        }
    }

    fn heading(&mut self, text: &str) {
        self.ensure(40.0);
        self.y -= 8.0;
        self.doc.pages[self.page].text(MARGIN, self.y - 11.0, 11.0, true, text);
        self.y -= 20.0;
    }

    fn note(&mut self, text: &str) {
        self.ensure(16.0);
        self.doc.pages[self.page].text(MARGIN, self.y - 9.0, 8.5, false, &fit(text, PAGE_W - 2.0 * MARGIN, 8.5, false));
        self.y -= 14.0;
    }

    fn table(&mut self, cols: &[Col], rows: &[Vec<String>]) {
        let total_w = PAGE_W - 2.0 * MARGIN;
        let wsum: f32 = cols.iter().map(|c| c.weight).sum();
        let widths: Vec<f32> = cols.iter().map(|c| total_w * c.weight / wsum).collect();
        self.table_header(cols, &widths);
        if rows.is_empty() {
            self.ensure(ROW_H);
            let msg = self.lang.t("(sin datos)", "(no data)");
            self.doc.pages[self.page].text(MARGIN + 4.0, self.y - 9.5, FONT, false, msg);
            self.y -= ROW_H;
            return;
        }
        for (i, row) in rows.iter().enumerate() {
            if self.y - ROW_H < BOTTOM {
                self.new_page();
                self.table_header(cols, &widths);
            }
            let p = &mut self.doc.pages[self.page];
            if i % 2 == 1 {
                p.rect_fill(MARGIN, self.y - ROW_H, total_w, ROW_H, (0.955, 0.955, 0.955));
            }
            let mut x = MARGIN;
            for (ci, (c, cell)) in cols.iter().zip(row.iter()).enumerate() {
                let w = widths[ci];
                let s = fit(cell, w - 6.0, FONT, false);
                if c.right {
                    p.text_right(x + w - 3.0, self.y - 9.5, FONT, false, &s);
                } else {
                    p.text(x + 3.0, self.y - 9.5, FONT, false, &s);
                }
                x += w;
            }
            self.y -= ROW_H;
        }
        self.y -= 6.0;
    }

    fn table_header(&mut self, cols: &[Col], widths: &[f32]) {
        self.ensure(ROW_H * 3.0);
        let total_w = PAGE_W - 2.0 * MARGIN;
        let p = &mut self.doc.pages[self.page];
        p.rect_fill(MARGIN, self.y - 15.0, total_w, 15.0, (0.86, 0.87, 0.89));
        let mut x = MARGIN;
        for (i, c) in cols.iter().enumerate() {
            let s = fit(&c.title, widths[i] - 6.0, FONT, true);
            if c.right {
                p.text_right(x + widths[i] - 3.0, self.y - 10.5, FONT, true, &s);
            } else {
                p.text(x + 3.0, self.y - 10.5, FONT, true, &s);
            }
            x += widths[i];
        }
        self.y -= 15.0;
    }

    fn finish(mut self) -> Vec<u8> {
        let total = self.doc.pages.len();
        let lang = self.lang;
        for (i, p) in self.doc.pages.iter_mut().enumerate() {
            p.line(MARGIN, 38.0, PAGE_W - MARGIN, 38.0, 0.5, 0.6);
            p.text(MARGIN, 26.0, 8.0, false, "Lumos");
            let s = format!("{} {} {} {}", lang.t("Página", "Page"), i + 1, lang.t("de", "of"), total);
            p.text_right(PAGE_W - MARGIN, 26.0, 8.0, false, &s);
        }
        self.doc.finish()
    }
}

// ───────────────────────────── data helpers ─────────────────────────────

fn profile_map(show: &Show) -> HashMap<String, FixtureProfile> {
    let mut m: HashMap<String, FixtureProfile> = builtin().into_iter().map(|p| (p.id.clone(), p)).collect();
    for p in &show.profiles {
        m.insert(p.id.clone(), p.clone());
    }
    m
}

fn profile_label(p: Option<&FixtureProfile>, id: &str) -> String {
    match p {
        Some(p) if !p.manufacturer.is_empty() => format!("{} {}", p.manufacturer, p.model),
        Some(p) => p.model.clone(),
        None => id.to_string(),
    }
}

fn footprint(f: &Fixture, profs: &HashMap<String, FixtureProfile>) -> Option<u16> {
    let m = profs.get(&f.profile)?.modes.iter().find(|m| m.name == f.mode)?;
    Some(mode_footprint(m))
}

fn num(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn sorted_fixtures(show: &Show) -> Vec<&Fixture> {
    let mut v: Vec<&Fixture> = show.fixtures.iter().collect();
    v.sort_by_key(|f| f.id);
    v
}

fn cue_num(n: f64) -> String {
    if (n - n.round()).abs() < 1e-9 {
        format!("{}", n.round() as i64)
    } else {
        let s = format!("{n:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn group_label(lang: Lang, g: &str) -> &'static str {
    match (lang, g) {
        (Lang::Es, "intensity") => "intensidad",
        (Lang::Es, "position") => "posición",
        (Lang::Es, "color") => "color",
        (Lang::Es, "beam") => "haz",
        (Lang::Es, _) => "control",
        (Lang::En, "intensity") => "intensity",
        (Lang::En, "position") => "position",
        (Lang::En, "color") => "colour",
        (Lang::En, "beam") => "beam",
        (Lang::En, _) => "control",
    }
}

fn attr_group(attr: &str) -> &'static str {
    ATTRS.iter().find(|(n, _, _)| *n == attr).map(|(_, g, _)| *g).unwrap_or("control")
}

/// "3 aparatos · intensidad, color" for a `PerFixture` map.
fn values_summary(lang: Lang, vals: &PerFixture, refs: &PerFixtureRefs) -> String {
    let mut groups: BTreeSet<&'static str> = BTreeSet::new();
    for attrs in vals.values() {
        for a in attrs.keys() {
            groups.insert(attr_group(a));
        }
    }
    let n = vals.len();
    let g: Vec<&str> = ["intensity", "position", "color", "beam", "control"]
        .iter()
        .filter(|k| groups.contains(*k))
        .map(|g| group_label(lang, g))
        .collect();
    let r: usize = refs.values().map(|m| m.len()).sum();
    let mut s = format!("{} {}", n, if n == 1 { lang.t("aparato", "fixture") } else { lang.t("aparatos", "fixtures") });
    if !g.is_empty() {
        s.push_str(" · ");
        s.push_str(&g.join(", "));
    }
    if r > 0 {
        s.push_str(&format!(" · {} {}", r, lang.t("paletas ref.", "palette refs")));
    }
    s
}

// ───────────────────────────── reports ─────────────────────────────

pub fn generate(kind: Kind, show: &Show, lang: Lang, date: &str) -> Vec<u8> {
    match kind {
        Kind::Patch => patch(show, lang, date),
        Kind::Channels => channels(show, lang, date),
        Kind::Cues => cues(show, lang, date),
        Kind::Power => power(show, lang, date),
        Kind::Scenes => scenes(show, lang, date),
    }
}

fn patch(show: &Show, lang: Lang, date: &str) -> Vec<u8> {
    let profs = profile_map(show);
    let mut r = Rep::new(lang.t("Lista de parcheo", "Patch list"), &show.name, date, lang);
    r.note(&format!(
        "{} {} · {} {}",
        show.fixtures.len(),
        lang.t("aparatos", "fixtures"),
        show.universes.len(),
        lang.t("universos", "universes")
    ));
    let cols = [
        rcol("ID", 0.6),
        col(lang.t("Nombre", "Name"), 2.2),
        col(lang.t("Perfil", "Profile"), 3.2),
        col(lang.t("Modo", "Mode"), 2.2),
        rcol(lang.t("Univ.", "Univ."), 0.8),
        rcol(lang.t("Dir.", "Addr."), 0.8),
        rcol(lang.t("Canales", "Chans"), 0.9),
        rcol("X", 0.8),
        rcol("Y", 0.8),
        rcol("Z", 0.8),
    ];
    let rows: Vec<Vec<String>> = sorted_fixtures(show)
        .iter()
        .map(|f| {
            let prof = profs.get(&f.profile);
            vec![
                f.id.to_string(),
                f.name.clone(),
                profile_label(prof, &f.profile),
                f.mode.clone(),
                f.universe.to_string(),
                f.address.to_string(),
                footprint(f, &profs).map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
                num(f.pos[0]),
                num(f.pos[1]),
                num(f.pos[2]),
            ]
        })
        .collect();
    r.table(&cols, &rows);
    r.note(lang.t("Posiciones en metros (x: derecha de escena, y: altura, z: hacia el público).", "Positions in metres (x: stage right, y: height, z: downstage)."));
    r.finish()
}

struct Span {
    id: u32,
    name: String,
    label: String,
    start: u16,
    end: u16,
    known: bool,
}

fn channels(show: &Show, lang: Lang, date: &str) -> Vec<u8> {
    let profs = profile_map(show);
    let mut r = Rep::new(lang.t("Mapa de canales DMX", "DMX channel hookup"), &show.name, date, lang);

    let mut by_uni: BTreeMap<u16, Vec<Span>> = BTreeMap::new();
    for u in &show.universes {
        by_uni.entry(u.id).or_default();
    }
    for f in sorted_fixtures(show) {
        let fp = footprint(f, &profs);
        let prof = profs.get(&f.profile);
        by_uni.entry(f.universe).or_default().push(Span {
            id: f.id,
            name: f.name.clone(),
            label: format!("{} / {}", profile_label(prof, &f.profile), f.mode),
            start: f.address,
            end: f.address + fp.unwrap_or(1).max(1) - 1,
            known: fp.is_some(),
        });
    }
    let unames: HashMap<u16, String> = show.universes.iter().map(|u| (u.id, u.name.clone())).collect();

    for (u, spans) in by_uni.iter_mut() {
        spans.sort_by_key(|s| (s.start, s.id));
        let used: usize = {
            let mut occ = [false; 513];
            for s in spans.iter() {
                for c in s.start..=s.end.min(512) {
                    occ[c as usize] = true;
                }
            }
            occ.iter().filter(|b| **b).count()
        };
        let uname = unames.get(u).cloned().unwrap_or_default();
        r.heading(&format!(
            "{} {}{}",
            lang.t("Universo", "Universe"),
            u,
            if uname.is_empty() { String::new() } else { format!(" — {uname}") }
        ));
        r.note(&format!(
            "{}: {} / 512 ({}%) · {} {}",
            lang.t("Canales usados", "Channels used"),
            used,
            used * 100 / 512,
            spans.len(),
            lang.t("aparatos", "fixtures")
        ));
        let cols = [
            col(lang.t("Rango", "Range"), 1.3),
            rcol("ID", 0.6),
            col(lang.t("Nombre", "Name"), 2.2),
            col(lang.t("Perfil / modo", "Profile / mode"), 4.0),
            col(lang.t("Aviso", "Note"), 1.9),
        ];
        let rows: Vec<Vec<String>> = spans
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let overlaps: Vec<String> = spans
                    .iter()
                    .enumerate()
                    .filter(|(j, o)| *j != i && o.start <= s.end && s.start <= o.end)
                    .map(|(_, o)| o.id.to_string())
                    .collect();
                let mut note = String::new();
                if !overlaps.is_empty() {
                    note = format!("{} {}", lang.t("Solapa con", "Overlaps"), overlaps.join(","));
                } else if s.end > 512 {
                    note = lang.t("Excede 512", "Exceeds 512").to_string();
                } else if !s.known {
                    note = lang.t("Modo desconocido", "Unknown mode").to_string();
                }
                vec![format!("{}–{}", s.start, s.end), s.id.to_string(), s.name.clone(), s.label.clone(), note]
            })
            .collect();
        r.table(&cols, &rows);
    }
    r.finish()
}

fn cues(show: &Show, lang: Lang, date: &str) -> Vec<u8> {
    let mut r = Rep::new(lang.t("Hoja de cues", "Cue sheet"), &show.name, date, lang);
    if show.cuelists.is_empty() {
        r.note(lang.t("No hay listas de cues.", "There are no cuelists."));
    }
    for cl in &show.cuelists {
        r.heading(&format!(
            "{} {} — {}{}{}",
            lang.t("Lista", "Cuelist"),
            cl.id,
            cl.name,
            if cl.looped { lang.t(" · en bucle", " · looped") } else { "" },
            if cl.tracking { lang.t(" · tracking", " · tracking") } else { lang.t(" · solo cue", " · cue only") }
        ));
        let cols = [
            rcol("Cue", 0.7),
            col(lang.t("Etiqueta", "Label"), 1.8),
            rcol(lang.t("Entrada", "Fade in"), 0.8),
            rcol(lang.t("Salida", "Fade out"), 0.8),
            rcol(lang.t("Retardo", "Delay"), 0.8),
            rcol(lang.t("Seguir", "Follow"), 0.7),
            col("Timecode", 1.7),
            col(lang.t("Contenido", "Content"), 4.2),
            col(lang.t("Notas", "Notes"), 1.6),
        ];
        let rows: Vec<Vec<String>> = cl
            .cues
            .iter()
            .map(|c| {
                vec![
                    cue_num(c.number),
                    c.label.clone(),
                    num(c.fade_in),
                    num(c.fade_out),
                    num(c.delay),
                    c.follow.map(num).unwrap_or_default(),
                    c.timecode.clone().unwrap_or_default(),
                    format!("{}{}", values_summary(lang, &c.changes, &c.refs), if c.block { lang.t(" · bloque", " · block") } else { "" }),
                    c.notes.clone(),
                ]
            })
            .collect();
        r.table(&cols, &rows);
    }
    r.finish()
}

fn power(show: &Show, lang: Lang, date: &str) -> Vec<u8> {
    let profs = profile_map(show);
    let mut r = Rep::new(lang.t("Informe de consumo", "Power report"), &show.name, date, lang);
    const VOLTS: f32 = 230.0;

    let mut per_uni: BTreeMap<u16, (usize, f32, usize)> = BTreeMap::new(); // fixtures, watts, without data
    let mut per_prof: BTreeMap<String, (usize, f32)> = BTreeMap::new();
    for f in &show.fixtures {
        let prof = profs.get(&f.profile);
        let w = prof.map(|p| p.power_w).unwrap_or(0.0);
        let e = per_uni.entry(f.universe).or_default();
        e.0 += 1;
        e.1 += w;
        if w <= 0.0 {
            e.2 += 1;
        }
        let pe = per_prof.entry(profile_label(prof, &f.profile)).or_default();
        pe.0 += 1;
        pe.1 = w;
    }
    let total_w: f32 = per_uni.values().map(|v| v.1).sum();
    let missing: usize = per_uni.values().map(|v| v.2).sum();

    r.note(&format!(
        "{}: {:.0} W · {:.1} A @ {:.0} V",
        lang.t("Potencia total", "Total power"),
        total_w,
        total_w / VOLTS,
        VOLTS
    ));
    if missing > 0 {
        r.note(&format!(
            "{} {} {}",
            missing,
            lang.t("aparatos sin dato de potencia (no incluidos).", "fixtures without power data (not included)."),
            ""
        ));
    }

    r.heading(lang.t("Por universo", "By universe"));
    let cols = [
        col(lang.t("Universo", "Universe"), 2.0),
        rcol(lang.t("Aparatos", "Fixtures"), 1.0),
        rcol(lang.t("Sin dato", "No data"), 1.0),
        rcol(lang.t("Potencia (W)", "Power (W)"), 1.3),
        rcol("A @ 230 V", 1.2),
    ];
    let unames: HashMap<u16, String> = show.universes.iter().map(|u| (u.id, u.name.clone())).collect();
    let mut rows: Vec<Vec<String>> = per_uni
        .iter()
        .map(|(u, (n, w, m))| {
            vec![
                format!("{} {}", u, unames.get(u).map(|s| format!("({s})")).unwrap_or_default()),
                n.to_string(),
                m.to_string(),
                format!("{w:.0}"),
                format!("{:.1}", w / VOLTS),
            ]
        })
        .collect();
    rows.push(vec![
        lang.t("TOTAL", "TOTAL").to_string(),
        per_uni.values().map(|v| v.0).sum::<usize>().to_string(),
        missing.to_string(),
        format!("{total_w:.0}"),
        format!("{:.1}", total_w / VOLTS),
    ]);
    r.table(&cols, &rows);

    r.heading(lang.t("Por modelo", "By model"));
    let cols = [
        col(lang.t("Modelo", "Model"), 4.0),
        rcol(lang.t("Cantidad", "Qty"), 1.0),
        rcol(lang.t("W unidad", "W each"), 1.0),
        rcol(lang.t("W total", "W total"), 1.0),
    ];
    let rows: Vec<Vec<String>> = per_prof
        .iter()
        .map(|(name, (n, w))| vec![name.clone(), n.to_string(), format!("{w:.0}"), format!("{:.0}", *n as f32 * w)])
        .collect();
    r.table(&cols, &rows);
    r.note(lang.t(
        "Potencia nominal de cada perfil; consumo real depende del uso y de la fuente.",
        "Nominal power per profile; real consumption depends on use and driver efficiency.",
    ));
    r.finish()
}

fn scenes(show: &Show, lang: Lang, date: &str) -> Vec<u8> {
    let mut r = Rep::new(lang.t("Paletas, escenas y grupos", "Palettes, scenes and groups"), &show.name, date, lang);

    r.heading(lang.t("Grupos", "Groups"));
    let rows: Vec<Vec<String>> = show
        .groups
        .iter()
        .map(|g| {
            vec![
                g.id.to_string(),
                g.name.clone(),
                g.fixtures.len().to_string(),
                g.fixtures.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", "),
            ]
        })
        .collect();
    r.table(
        &[rcol("ID", 0.6), col(lang.t("Nombre", "Name"), 2.5), rcol(lang.t("Aparatos", "Fixtures"), 1.0), col(lang.t("Miembros", "Members"), 6.0)],
        &rows,
    );

    r.heading(lang.t("Paletas", "Palettes"));
    let rows: Vec<Vec<String>> = show
        .palettes
        .iter()
        .map(|p| {
            let kind = match p.kind {
                PaletteKind::Intensity => lang.t("Intensidad", "Intensity"),
                PaletteKind::Position => lang.t("Posición", "Position"),
                PaletteKind::Color => "Color",
                PaletteKind::Beam => lang.t("Haz", "Beam"),
                PaletteKind::All => lang.t("Todo", "All"),
            };
            let mut attrs: BTreeSet<&str> = p.values.keys().map(|s| s.as_str()).collect();
            for a in p.per_fixture.values() {
                attrs.extend(a.keys().map(|s| s.as_str()));
            }
            vec![
                p.id.to_string(),
                p.name.clone(),
                kind.to_string(),
                p.per_fixture.len().to_string(),
                attrs.into_iter().collect::<Vec<_>>().join(", "),
            ]
        })
        .collect();
    r.table(
        &[rcol("ID", 0.6), col(lang.t("Nombre", "Name"), 2.5), col(lang.t("Tipo", "Kind"), 1.4), rcol(lang.t("Por aparato", "Per fixture"), 1.2), col(lang.t("Atributos", "Attributes"), 5.0)],
        &rows,
    );

    r.heading(lang.t("Escenas", "Scenes"));
    let rows: Vec<Vec<String>> = show
        .scenes
        .iter()
        .map(|s| {
            let kind = match s.kind {
                SceneKind::Look => lang.t("Look", "Look"),
                SceneKind::Intensity => lang.t("Intensidad", "Intensity"),
                SceneKind::Inhibitive => lang.t("Inhibitivo", "Inhibitive"),
            };
            vec![
                s.id.to_string(),
                s.name.clone(),
                kind.to_string(),
                num(s.fade_in),
                num(s.fade_out),
                values_summary(lang, &s.values, &s.refs),
            ]
        })
        .collect();
    r.table(
        &[rcol("ID", 0.6), col(lang.t("Nombre", "Name"), 2.5), col(lang.t("Tipo", "Kind"), 1.3), rcol(lang.t("Entrada", "Fade in"), 0.9), rcol(lang.t("Salida", "Fade out"), 0.9), col(lang.t("Contenido", "Content"), 4.5)],
        &rows,
    );
    let _ = text_width; // keep the import used in all cfgs
    r.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::pdf_string;

    fn has(pdf: &[u8], text: &str) -> bool {
        // prefix match on the literal string `(text…`: notes are single longer strings
        let mut needle = pdf_string(text);
        needle.pop(); // closing ')'
        pdf.windows(needle.len()).any(|w| w == needle.as_slice())
    }

    fn pages(pdf: &[u8]) -> usize {
        String::from_utf8_lossy(pdf).matches("/Type /Page ").count()
    }

    fn demo_show(n: usize) -> Show {
        let lib = builtin();
        let par = lib.iter().find(|p| p.id == "generic/rgb-par").unwrap();
        let mh = lib.iter().find(|p| p.id == "generic/mh-spot").unwrap();
        let mut s = Show::default();
        s.name = "Función ñandú".into();
        for i in 0..n {
            let (p, addr) = if i % 5 == 4 { (mh, 100 + (i as u16 / 5) * 16) } else { (par, 1 + (i as u16 % 100) * 3) };
            s.fixtures.push(Fixture {
                id: i as u32 + 1,
                name: format!("Foco {}", i + 1),
                profile: p.id.clone(),
                mode: p.modes[0].name.clone(),
                universe: 1 + (i as u16 / 60),
                address: addr,
                pos: [i as f32 * 0.5, 5.0, 1.0],
                rot: [0.0; 3],
            });
        }
        s
    }

    #[test]
    fn dates() {
        assert_eq!(date_string(0), "1970-01-01");
        assert_eq!(date_string(1_792_000_000), "2026-10-14");
        assert_eq!(date_string(951_782_400), "2000-02-29");
        assert_eq!(Lang::parse(Some("en")), Lang::En);
        assert_eq!(Lang::parse(None), Lang::Es);
        assert_eq!(kind_from("power.pdf"), Some(Kind::Power));
        assert_eq!(kind_from("nope.pdf"), None);
    }

    #[test]
    fn patch_pdf_spanish_and_english() {
        let show = demo_show(12);
        let es = generate(Kind::Patch, &show, Lang::Es, "2026-10-22");
        assert!(es.starts_with(b"%PDF-1.4") && es.ends_with(b"%%EOF\n"));
        assert_eq!(pages(&es), 1);
        assert!(has(&es, "Lista de parcheo"));
        assert!(has(&es, "Función ñandú · 2026-10-22"));
        assert!(has(&es, "Foco 7"));
        assert!(has(&es, "Página 1 de 1"));
        let en = generate(Kind::Patch, &show, Lang::En, "2026-10-22");
        assert!(has(&en, "Patch list"));
        assert!(has(&en, "Page 1 of 1"));
    }

    #[test]
    fn long_tables_paginate_with_repeated_header() {
        let show = demo_show(200);
        let pdf = generate(Kind::Patch, &show, Lang::Es, "2026-10-22");
        let n = pages(&pdf);
        assert!(n >= 4, "pages {n}");
        assert!(has(&pdf, &format!("Página {n} de {n}")));
        // header cell repeated on every page
        let s = pdf.windows(b"(Nombre)".len()).filter(|w| *w == b"(Nombre)").count();
        assert_eq!(s, n);
        assert!(has(&pdf, "Foco 200"));
    }

    #[test]
    fn channels_flags_overlaps_and_usage() {
        let mut show = demo_show(3);
        show.fixtures[1].address = show.fixtures[0].address + 1; // overlap
        let pdf = generate(Kind::Channels, &show, Lang::Es, "2026-10-22");
        assert!(has(&pdf, "Mapa de canales DMX"));
        assert!(has(&pdf, "Solapa con 2"));
        assert!(has(&pdf, "Canales usados"));
        assert!(has(&pdf, "Universo 1"));
    }

    #[test]
    fn cues_power_scenes() {
        let mut show = demo_show(6);
        let mut ch = PerFixture::new();
        ch.entry(1).or_default().insert("dimmer".into(), 1.0);
        ch.entry(2).or_default().insert("red".into(), 1.0);
        show.cuelists.push(Cuelist {
            id: 1,
            name: "Principal".into(),
            cues: vec![Cue {
                number: 2.5,
                label: "Apertura".into(),
                fade_in: 3.0,
                fade_out: 1.5,
                delay: 0.0,
                follow: Some(4.0),
                timecode: Some("00:01:00:00".into()),
                changes: ch,
                block: false,
                refs: Default::default(),
                timing: Default::default(),
                curve: Default::default(),
                stagger: 0.0,
                notes: "Entrada actriz".into(),
            }],
            looped: false,
            tracking: true,
            priority: 0,
            auto_mark: false,
        });
        let c = generate(Kind::Cues, &show, Lang::Es, "d");
        assert!(has(&c, "Hoja de cues"));
        assert!(has(&c, "Apertura"));
        assert!(has(&c, "2.5") || has(&c, "2,5"));
        assert!(has(&c, "00:01:00:00"));
        assert!(has(&c, "Entrada actriz"));
        assert!(has(&c, "2 aparatos · intensidad, color"));

        let p = generate(Kind::Power, &show, Lang::En, "d");
        assert!(has(&p, "Power report"));
        assert!(has(&p, "TOTAL"));

        let sc = generate(Kind::Scenes, &show, Lang::Es, "d");
        assert!(has(&sc, "Paletas, escenas y grupos"));
        assert!(has(&sc, "(sin datos)"));
    }

    #[test]
    fn empty_show_still_valid() {
        for k in [Kind::Patch, Kind::Channels, Kind::Cues, Kind::Power, Kind::Scenes] {
            let pdf = generate(k, &Show::default(), Lang::Es, "d");
            assert!(pdf.starts_with(b"%PDF") && pages(&pdf) >= 1, "{k:?}");
        }
    }
}
