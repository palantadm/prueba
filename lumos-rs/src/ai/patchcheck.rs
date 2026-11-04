//! Patch sanity for the assistant: DMX channel overlaps, out-of-range addresses, free ranges and a dry run of `fixture_add`
//! / `fixture_update` so the assistant never silently stacks fixtures on top of each other.
//!
//! Pure functions over [`AiState`]; the executor in [`super::tools`] applies them before and after patch commands.

use super::AiState;
use crate::model::*;
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub id: u32,
    pub name: String,
    pub universe: u16,
    /// First DMX channel (1-based).
    pub start: u16,
    /// Channels used (≥ 1; unknown profile/mode counts as 1).
    pub len: u16,
    /// Profile or mode could not be resolved (the footprint is a guess).
    pub unknown: bool,
}

impl Span {
    pub fn end(&self) -> u16 {
        self.start.saturating_add(self.len).saturating_sub(1)
    }
    fn label(&self) -> String {
        format!("#{} «{}» (U{} {}–{})", self.id, self.name, self.universe, self.start, self.end())
    }
}

pub fn footprint(st: &AiState, profile: &str, mode: &str) -> Option<u16> {
    st.profile_mode(profile, mode).1.map(crate::library::mode_footprint).filter(|n| *n > 0)
}

pub fn spans(st: &AiState) -> Vec<Span> {
    st.show
        .fixtures
        .iter()
        .map(|f| {
            let fp = footprint(st, &f.profile, &f.mode);
            Span { id: f.id, name: f.name.clone(), universe: f.universe, start: f.address, len: fp.unwrap_or(1), unknown: fp.is_none() }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Overlap {
    pub a: Span,
    pub b: Span,
    /// Same start and same length: deliberate clones are common, so this is reported softer.
    pub identical: bool,
}

fn touch(a: &Span, b: &Span) -> bool {
    a.universe == b.universe && a.start <= b.end() && b.start <= a.end()
}

pub fn overlaps(sp: &[Span]) -> Vec<Overlap> {
    let mut v = Vec::new();
    for (i, a) in sp.iter().enumerate() {
        for b in &sp[i + 1..] {
            if touch(a, b) {
                v.push(Overlap { a: a.clone(), b: b.clone(), identical: a.start == b.start && a.len == b.len });
            }
        }
    }
    v
}

pub fn out_of_range(sp: &[Span]) -> Vec<&Span> {
    sp.iter().filter(|s| s.start < 1 || s.end() > 512).collect()
}

/// Free `(from, to)` channel ranges (1..=512) of a universe.
pub fn free_ranges(sp: &[Span], universe: u16) -> Vec<(u16, u16)> {
    let mut used = [false; 513];
    for s in sp.iter().filter(|s| s.universe == universe) {
        for c in s.start.max(1)..=s.end().min(512) {
            used[c as usize] = true;
        }
    }
    let mut out = Vec::new();
    let mut c = 1usize;
    while c <= 512 {
        if !used[c] {
            let from = c;
            while c <= 512 && !used[c] {
                c += 1;
            }
            out.push((from as u16, (c - 1) as u16));
        } else {
            c += 1;
        }
    }
    out
}

/// First address in `universe` (or a later one) where `len` consecutive channels are free.
pub fn first_fit(sp: &[Span], universe: u16, len: u16) -> Option<(u16, u16)> {
    for u in universe..universe.saturating_add(64) {
        if let Some((a, _)) = free_ranges(sp, u).into_iter().find(|(a, b)| b - a + 1 >= len) {
            return Some((u, a));
        }
    }
    None
}

fn describe(o: &Overlap) -> String {
    format!("{} se pisa con {}{}", o.a.label(), o.b.label(), if o.identical { " (misma dirección y tamaño: clon)" } else { "" })
}

/// Result of simulating `fixture_add` with an explicit address (mirrors the engine: consecutive footprints, spill to the next universe).
pub struct AddPlan {
    pub placements: Vec<(u16, u16)>,
    pub conflicts: Vec<String>,
    pub suggestion: Option<String>,
}

pub fn plan_add(st: &AiState, profile: &str, mode: &str, count: u32, universe: u16, address: u16) -> Result<AddPlan, String> {
    if st.profiles.iter().all(|p| p.id != profile) {
        return Err(format!("Perfil «{profile}» no encontrado; búscalo con search_library."));
    }
    let fp = footprint(st, profile, mode).ok_or_else(|| format!("El perfil «{profile}» no tiene el modo «{mode}» (o no tiene canales); mira get_profile."))?;
    let mut sp = spans(st);
    let mut conflicts = Vec::new();
    let mut placements = Vec::new();
    let (mut u, mut cursor) = (universe, address);
    for i in 0..count {
        if cursor as u32 + fp as u32 - 1 > 512 {
            // engine moves to the next universe's first free slot
            let (nu, na) = first_fit(&sp, u.saturating_add(1), fp).ok_or("No queda espacio DMX")?;
            u = nu;
            cursor = na;
        }
        let new = Span { id: 0, name: format!("nuevo {}", i + 1), universe: u, start: cursor, len: fp, unknown: false };
        for s in sp.iter().filter(|s| touch(s, &new)) {
            conflicts.push(format!("el nuevo aparato {} (U{u} {}–{}) pisa a {}", i + 1, new.start, new.end(), s.label()));
        }
        placements.push((u, cursor));
        sp.push(new);
        cursor += fp;
        if cursor > 512 {
            cursor = 513;
        }
    }
    let suggestion = if conflicts.is_empty() {
        None
    } else {
        // look for a block big enough for the whole run in the requested universe, ignoring the new ones
        let base = spans(st);
        let total = (fp as u32 * count).min(512) as u16;
        Some(match first_fit(&base, universe, total) {
            Some((fu, fa)) => format!("Hay hueco para los {count} aparatos seguidos ({total} canales) en U{fu} desde la dirección {fa}. O envía `address: null` y Lumos usa los primeros huecos libres."),
            None => "No hay un bloque libre tan grande: envía `address: null` y Lumos reparte los aparatos por los huecos y universos libres.".to_string(),
        })
    };
    Ok(AddPlan { placements, conflicts, suggestion })
}

/// Would `fixture` (a proposed update) overlap anything *new*? Overlaps that already existed before are ignored.
pub fn check_update(st: &AiState, fixture: &Fixture) -> Vec<String> {
    let before = overlaps(&spans(st));
    let mut moved = st.show.fixtures.clone();
    if let Some(slot) = moved.iter_mut().find(|f| f.id == fixture.id) {
        *slot = fixture.clone();
    }
    let probe = AiStateView { st, fixtures: &moved };
    overlaps(&probe.spans())
        .into_iter()
        .filter(|o| (o.a.id == fixture.id || o.b.id == fixture.id) && !before.iter().any(|b| (b.a.id, b.b.id) == (o.a.id, o.b.id)))
        .filter(|o| !o.identical)
        .map(|o| describe(&o))
        .collect()
}

struct AiStateView<'a> {
    st: &'a AiState,
    fixtures: &'a [Fixture],
}

impl AiStateView<'_> {
    fn spans(&self) -> Vec<Span> {
        self.fixtures
            .iter()
            .map(|f| {
                let fp = footprint(self.st, &f.profile, &f.mode);
                Span { id: f.id, name: f.name.clone(), universe: f.universe, start: f.address, len: fp.unwrap_or(1), unknown: fp.is_none() }
            })
            .collect()
    }
}

/// Full report used by the `check_patch` tool and appended after patch edits.
pub fn report(st: &AiState, universe: Option<u16>) -> Value {
    let sp = spans(st);
    let all = overlaps(&sp);
    let keep = |s: &Span| universe.map_or(true, |u| s.universe == u);
    let real: Vec<String> = all.iter().filter(|o| keep(&o.a) && !o.identical).map(describe).collect();
    let clones: Vec<String> = all.iter().filter(|o| keep(&o.a) && o.identical).map(describe).collect();
    let oor: Vec<String> = out_of_range(&sp).into_iter().filter(|s| keep(s)).map(|s| format!("{} sale del rango 1–512", s.label())).collect();
    let unknown: Vec<String> = sp.iter().filter(|s| keep(s) && s.unknown).map(|s| format!("{} usa un perfil/modo que no existe: su tamaño real se desconoce", s.label())).collect();
    let mut unis: Vec<u16> = sp.iter().map(|s| s.universe).collect();
    unis.sort_unstable();
    unis.dedup();
    let per_universe: Vec<Value> = unis
        .into_iter()
        .filter(|u| universe.map_or(true, |w| w == *u))
        .map(|u| {
            let used: u32 = sp.iter().filter(|s| s.universe == u).map(|s| s.len as u32).sum();
            json!({ "universe": u, "fixtures": sp.iter().filter(|s| s.universe == u).count(), "channels_used": used, "free_ranges": free_ranges(&sp, u).into_iter().map(|(a, b)| format!("{a}-{b}")).collect::<Vec<_>>() })
        })
        .collect();
    let ok = real.is_empty() && oor.is_empty() && unknown.is_empty();
    json!({
        "ok": ok,
        "solapes": real, "clones_misma_direccion": clones, "fuera_de_rango": oor, "perfil_desconocido": unknown,
        "universos": per_universe,
        "nota": if ok { "Sin solapes de canales DMX." } else { "Hay aparatos pisándose: corrígelos con fixture_update (nueva dirección) o recoloca con address:null." },
    })
}

/// One-line verdict appended to the result of a patch-changing command.
pub fn verdict(st: &AiState) -> String {
    let r = report(st, None);
    if r["ok"] == true {
        let n = st.show.fixtures.len();
        let clones = r["clones_misma_direccion"].as_array().map_or(0, |a| a.len());
        format!("Patch verificado: {n} aparatos, sin solapes de canales{}.", if clones > 0 { format!(" ({clones} clon(es) con la misma dirección a propósito)") } else { String::new() })
    } else {
        let list = ["solapes", "fuera_de_rango", "perfil_desconocido"].iter().flat_map(|k| r[*k].as_array().cloned().unwrap_or_default()).filter_map(|v| v.as_str().map(|s| s.to_string())).take(6).collect::<Vec<_>>();
        format!("⚠ PATCH CON PROBLEMAS: {}. Corrígelo (fixture_update con otra dirección, o recoloca) antes de seguir.", list.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(id: u32, u: u16, start: u16, len: u16) -> Span {
        Span { id, name: format!("f{id}"), universe: u, start, len, unknown: false }
    }

    #[test]
    fn overlap_detection_and_clones() {
        let v = vec![sp(1, 1, 1, 7), sp(2, 1, 7, 7), sp(3, 1, 20, 7), sp(4, 1, 20, 7), sp(5, 2, 1, 7)];
        let o = overlaps(&v);
        assert_eq!(o.len(), 2);
        assert!(!o[0].identical && (o[0].a.id, o[0].b.id) == (1, 2));
        assert!(o[1].identical && (o[1].a.id, o[1].b.id) == (3, 4));
        assert!(overlaps(&[sp(1, 1, 1, 7), sp(2, 1, 8, 7), sp(3, 2, 1, 7)]).is_empty(), "adjacent / other universe is fine");
    }

    #[test]
    fn free_ranges_and_first_fit() {
        let v = vec![sp(1, 1, 1, 10), sp(2, 1, 21, 10)];
        assert_eq!(free_ranges(&v, 1), vec![(11, 20), (31, 512)]);
        assert_eq!(free_ranges(&v, 2), vec![(1, 512)]);
        assert_eq!(first_fit(&v, 1, 10), Some((1, 11)));
        assert_eq!(first_fit(&v, 1, 11), Some((1, 31)));
        let full = vec![sp(1, 1, 1, 512)];
        assert_eq!(first_fit(&full, 1, 5), Some((2, 1)));
    }

    #[test]
    fn out_of_range_flags() {
        assert_eq!(out_of_range(&[sp(1, 1, 510, 7), sp(2, 1, 1, 7)]).len(), 1);
    }
}
