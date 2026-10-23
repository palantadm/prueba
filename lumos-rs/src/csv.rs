//! Patch CSV export/import. Columns: `id,name,profile,mode,universe,address,x,y,z`.
//! Import is an upsert by fixture id and atomic: any invalid line aborts the whole import with per-line errors.

use crate::library::mode_footprint;
use crate::model::*;
use std::collections::{BTreeSet, HashMap};

pub const HEADER: &str = "id,name,profile,mode,universe,address,x,y,z";

fn esc(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn num(v: f32) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn export_patch(show: &Show) -> String {
    let mut out = String::from(HEADER);
    out.push_str("\r\n");
    let mut fx: Vec<&Fixture> = show.fixtures.iter().collect();
    fx.sort_by_key(|f| f.id);
    for f in fx {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\r\n",
            f.id,
            esc(&f.name),
            esc(&f.profile),
            esc(&f.mode),
            f.universe,
            f.address,
            num(f.pos[0]),
            num(f.pos[1]),
            num(f.pos[2])
        ));
    }
    out
}

/// RFC-4180-style parser (quotes, `""` escapes, CRLF/LF, BOM). The delimiter is `,` or `;` (autodetected from
/// the first line). Returns `(1-based line where the record starts, fields)`; blank lines are skipped.
pub fn parse_csv(text: &str) -> Vec<(usize, Vec<String>)> {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().next().unwrap_or("");
    let delim = if first.matches(';').count() > first.matches(',').count() { ';' } else { ',' };
    let mut rows = Vec::new();
    let mut field = String::new();
    let mut row: Vec<String> = Vec::new();
    let mut in_q = false;
    let mut line = 1usize;
    let mut start_line = 1usize;
    let mut chars = text.chars().peekable();
    let mut had_data = false;
    while let Some(c) = chars.next() {
        if in_q {
            match c {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_q = false;
                    }
                }
                '\n' => {
                    line += 1;
                    field.push('\n');
                }
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => {
                in_q = true;
                had_data = true;
            }
            c if c == delim => {
                row.push(std::mem::take(&mut field));
                had_data = true;
            }
            '\r' => {}
            '\n' => {
                if had_data || !field.is_empty() {
                    row.push(std::mem::take(&mut field));
                    if row.iter().any(|f| !f.trim().is_empty()) {
                        rows.push((start_line, std::mem::take(&mut row)));
                    } else {
                        row.clear();
                    }
                }
                had_data = false;
                line += 1;
                start_line = line;
            }
            _ => {
                field.push(c);
                had_data = true;
            }
        }
    }
    if had_data || !field.is_empty() {
        row.push(field);
        if row.iter().any(|f| !f.trim().is_empty()) {
            rows.push((start_line, row));
        }
    }
    rows
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineError {
    pub line: usize,
    pub error: String,
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub created: usize,
    pub updated: usize,
    pub removed: usize,
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// Match by profile id, then "manufacturer model", then a unique model name.
pub fn resolve_profile<'a>(profs: &'a HashMap<String, FixtureProfile>, key: &str) -> Option<&'a FixtureProfile> {
    let k = key.trim();
    if let Some(p) = profs.get(k) {
        return Some(p);
    }
    let lower = k.to_lowercase();
    if let Some(p) = profs.values().find(|p| p.id.to_lowercase() == lower) {
        return Some(p);
    }
    let nk = norm(k);
    if nk.is_empty() {
        return None;
    }
    if let Some(p) = profs.values().find(|p| norm(&format!("{}{}", p.manufacturer, p.model)) == nk) {
        return Some(p);
    }
    let by_model: Vec<&FixtureProfile> = profs.values().filter(|p| norm(&p.model) == nk).collect();
    if by_model.len() == 1 {
        return Some(by_model[0]);
    }
    None
}

pub fn resolve_mode<'a>(p: &'a FixtureProfile, mode: &str) -> Result<&'a Mode, String> {
    let m = mode.trim();
    if m.is_empty() {
        return p.modes.first().ok_or_else(|| format!("profile '{}' has no modes", p.id));
    }
    p.modes
        .iter()
        .find(|x| x.name == m)
        .or_else(|| p.modes.iter().find(|x| x.name.to_lowercase() == m.to_lowercase()))
        .ok_or_else(|| format!("mode '{m}' not found in profile '{}'", p.id))
}

fn column_of(h: &str) -> Option<&'static str> {
    Some(match norm(h).as_str() {
        "id" | "fixtureid" | "numero" | "number" => "id",
        "name" | "nombre" => "name",
        "profile" | "perfil" | "fixture" | "aparato" | "type" | "tipo" => "profile",
        "mode" | "modo" => "mode",
        "universe" | "universo" => "universe",
        "address" | "addr" | "direccion" | "dmx" => "address",
        "x" => "x",
        "y" => "y",
        "z" => "z",
        _ => return None,
    })
}

/// Apply a patch CSV to `show` (upsert by id). `replace`: also remove fixtures that are not in the file.
/// On any error the show is left untouched and all line errors are returned.
pub fn import_patch(
    show: &mut Show,
    profs: &HashMap<String, FixtureProfile>,
    text: &str,
    replace: bool,
) -> Result<Outcome, Vec<LineError>> {
    let rows = parse_csv(text);
    let err = |line: usize, e: String| LineError { line, error: e };
    let Some((hline, header)) = rows.first() else {
        return Err(vec![err(1, "empty file".into())]);
    };
    let mut idx: HashMap<&'static str, usize> = HashMap::new();
    for (i, h) in header.iter().enumerate() {
        if let Some(c) = column_of(h) {
            idx.entry(c).or_insert(i);
        }
    }
    let mut errors = Vec::new();
    for need in ["id", "profile", "universe", "address"] {
        if !idx.contains_key(need) {
            errors.push(err(*hline, format!("missing column '{need}'")));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    struct Row {
        line: usize,
        f: Fixture,
        has_pos: [bool; 3],
    }
    let mut parsed: Vec<Row> = Vec::new();
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    for (line, r) in rows.iter().skip(1) {
        let get = |c: &str| idx.get(c).and_then(|i| r.get(*i)).map(|s| s.trim().to_string()).unwrap_or_default();
        let mut bad = |e: String| errors.push(err(*line, e));
        let id: u32 = match get("id").parse() {
            Ok(v) if v > 0 => v,
            _ => {
                bad(format!("invalid id '{}'", get("id")));
                continue;
            }
        };
        if !seen.insert(id) {
            bad(format!("duplicate id {id}"));
            continue;
        }
        let pkey = get("profile");
        let Some(prof) = resolve_profile(profs, &pkey) else {
            bad(format!("unknown profile '{pkey}'"));
            continue;
        };
        let mode = match resolve_mode(prof, &get("mode")) {
            Ok(m) => m,
            Err(e) => {
                bad(e);
                continue;
            }
        };
        let universe: u16 = match get("universe").parse() {
            Ok(v) if v >= 1 => v,
            _ => {
                bad(format!("invalid universe '{}'", get("universe")));
                continue;
            }
        };
        let address: u16 = match get("address").parse() {
            Ok(v) if (1..=512).contains(&v) => v,
            _ => {
                bad(format!("address must be 1-512 (got '{}')", get("address")));
                continue;
            }
        };
        let fp = mode_footprint(mode).max(1);
        if address as u32 + fp as u32 - 1 > 512 {
            bad(format!("address {address} + {fp} channels exceeds 512"));
            continue;
        }
        let mut pos = [0.0f32; 3];
        let mut has_pos = [false; 3];
        let mut pos_ok = true;
        for (k, c) in ["x", "y", "z"].iter().enumerate() {
            let s = get(c);
            if s.is_empty() {
                continue;
            }
            match s.replace(',', ".").parse::<f32>() {
                Ok(v) if v.is_finite() => {
                    pos[k] = v;
                    has_pos[k] = true;
                }
                _ => {
                    bad(format!("invalid {c} '{s}'"));
                    pos_ok = false;
                }
            }
        }
        if !pos_ok {
            continue;
        }
        let name = get("name");
        parsed.push(Row {
            line: *line,
            f: Fixture {
                id,
                name: if name.is_empty() { format!("Fixture {id}") } else { name },
                profile: prof.id.clone(),
                mode: mode.name.clone(),
                universe,
                address,
                pos,
                rot: [0.0; 3],
            },
            has_pos,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let _ = parsed.iter().map(|r| r.line).count();

    let mut out = Outcome::default();
    for r in parsed {
        match show.fixtures.iter_mut().find(|f| f.id == r.f.id) {
            Some(ex) => {
                ex.name = r.f.name;
                ex.profile = r.f.profile;
                ex.mode = r.f.mode;
                ex.universe = r.f.universe;
                ex.address = r.f.address;
                for k in 0..3 {
                    if r.has_pos[k] {
                        ex.pos[k] = r.f.pos[k];
                    }
                }
                out.updated += 1;
            }
            None => {
                show.fixtures.push(r.f.clone());
                out.created += 1;
            }
        }
        if !show.universes.iter().any(|u| u.id == r.f.universe) {
            show.universes.push(UniverseCfg { id: r.f.universe, name: format!("Universe {}", r.f.universe), outputs: vec![] });
            show.universes.sort_by_key(|u| u.id);
        }
    }
    if replace {
        let before = show.fixtures.len();
        show.fixtures.retain(|f| seen.contains(&f.id));
        out.removed = before - show.fixtures.len();
        for g in &mut show.groups {
            g.fixtures.retain(|i| seen.contains(i));
        }
    }
    show.fixtures.sort_by_key(|f| f.id);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::builtin;

    fn profs() -> HashMap<String, FixtureProfile> {
        builtin().into_iter().map(|p| (p.id.clone(), p)).collect()
    }

    fn show_with(n: u32) -> Show {
        let mut s = Show::default();
        let lib = profs();
        let par = &lib["generic/rgb-par"];
        for i in 0..n {
            s.fixtures.push(Fixture {
                id: i + 1,
                name: if i == 1 { "Par, \"izq\"".into() } else { format!("Par {}", i + 1) },
                profile: par.id.clone(),
                mode: par.modes[0].name.clone(),
                universe: 1,
                address: 1 + i as u16 * 3,
                pos: [i as f32 * 1.25, 5.0, -0.5],
                rot: [0.0; 3],
            });
        }
        s
    }

    #[test]
    fn parser_handles_quotes_bom_crlf_and_semicolons() {
        let r = parse_csv("\u{feff}a,b,c\r\n1,\"x, y\",\"he said \"\"hi\"\"\"\r\n\r\n2,\"multi\nline\",z\r\n");
        assert_eq!(r.len(), 3);
        assert_eq!(r[1].1, vec!["1", "x, y", "he said \"hi\""]);
        assert_eq!(r[2].0, 4);
        assert_eq!(r[2].1[1], "multi\nline");
        let s = parse_csv("a;b\n1;2\n");
        assert_eq!(s[1].1, vec!["1", "2"]);
    }

    #[test]
    fn round_trip_export_import() {
        let src = show_with(5);
        let csv = export_patch(&src);
        assert!(csv.starts_with(HEADER));
        assert!(csv.contains("\"Par, \"\"izq\"\"\""));
        let mut dst = Show::default();
        let out = import_patch(&mut dst, &profs(), &csv, false).unwrap();
        assert_eq!(out.created, 5);
        assert_eq!(dst.fixtures.len(), 5);
        for (a, b) in src.fixtures.iter().zip(dst.fixtures.iter()) {
            assert_eq!((a.id, &a.name, &a.profile, &a.mode, a.universe, a.address), (b.id, &b.name, &b.profile, &b.mode, b.universe, b.address));
            assert!((a.pos[0] - b.pos[0]).abs() < 1e-3 && (a.pos[2] - b.pos[2]).abs() < 1e-3);
        }
        assert_eq!(export_patch(&dst), csv, "export → import → export is stable");
    }

    #[test]
    fn upsert_and_replace() {
        let mut s = show_with(3);
        let csv = "id,name,profile,mode,universe,address\n2,Renombrado,generic/rgb-par,3ch,1,100\n9,Nuevo,Generic RGB Par,,2,1\n";
        let out = import_patch(&mut s, &profs(), csv, false).unwrap();
        assert_eq!((out.created, out.updated, out.removed), (1, 1, 0));
        assert_eq!(s.fixtures.len(), 4);
        let f2 = s.fixtures.iter().find(|f| f.id == 2).unwrap();
        assert_eq!((f2.name.as_str(), f2.address), ("Renombrado", 100));
        assert!((f2.pos[0] - 1.25).abs() < 1e-6, "position kept when the column is absent");
        assert!(s.universes.iter().any(|u| u.id == 2), "universe created");
        let out = import_patch(&mut s, &profs(), csv, true).unwrap();
        assert_eq!(out.removed, 2);
        assert_eq!(s.fixtures.len(), 2);
    }

    #[test]
    fn atomic_with_line_errors() {
        let mut s = show_with(2);
        let before = serde_json::to_string(&s).unwrap();
        let csv = "id,name,profile,mode,universe,address\n1,A,generic/rgb-par,3ch,1,10\n3,B,nope/none,,1,5\n4,C,generic/rgb-par,9ch,1,5\n5,D,generic/rgb-par,3ch,1,511\n1,E,generic/rgb-par,3ch,1,1\nx,F,generic/rgb-par,3ch,1,1\n";
        let errs = import_patch(&mut s, &profs(), csv, false).unwrap_err();
        let lines: Vec<usize> = errs.iter().map(|e| e.line).collect();
        assert_eq!(lines, vec![3, 4, 5, 6, 7], "{errs:?}");
        assert!(errs[0].error.contains("unknown profile"));
        assert!(errs[1].error.contains("mode"));
        assert!(errs[2].error.contains("exceeds 512"));
        assert!(errs[3].error.contains("duplicate"));
        assert_eq!(serde_json::to_string(&s).unwrap(), before, "show untouched");
        assert!(import_patch(&mut s, &profs(), "foo,bar\n1,2\n", false).unwrap_err()[0].error.contains("missing column"));
    }

    #[test]
    fn profile_resolution() {
        let p = profs();
        assert_eq!(resolve_profile(&p, "generic/rgb-par").unwrap().id, "generic/rgb-par");
        assert_eq!(resolve_profile(&p, "GENERIC/RGB-PAR").unwrap().id, "generic/rgb-par");
        assert_eq!(resolve_profile(&p, "generic rgb par").unwrap().id, "generic/rgb-par");
        assert!(resolve_profile(&p, "").is_none());
    }
}
