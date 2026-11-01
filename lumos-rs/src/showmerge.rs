//! Merge parts of another show into the current one: patch, groups, palettes, scenes, cuelists, chases, effects,
//! macros, timelines, scripts. Pure over `Show` so it is easy to test; the engine wraps it in an undoable command.
//!
//! Rules: pool items keep their id when it is free and are renumbered when it is not, with every structural
//! reference (palette links in cues/scenes/steps, scenes in chase steps, speed masters, cuelist links of script
//! marks, fixture ids everywhere) rewritten. References to something that was *not* merged are dropped (cue values
//! stay). Console text inside macros / timelines / script actions is **not** rewritten: it is reported as a warning.

use crate::model::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

type IdMap = HashMap<u32, u32>;

fn free_id(used: &BTreeSet<u32>, want: u32) -> u32 {
    if want != 0 && !used.contains(&want) {
        want
    } else {
        used.iter().next_back().copied().unwrap_or(0) + 1
    }
}

/// Assign target ids for `items` (their current ids), reserving them in `used`.
fn assign(used: &mut BTreeSet<u32>, items: impl Iterator<Item = u32>) -> IdMap {
    let mut m = IdMap::new();
    for old in items {
        let new = free_id(used, old);
        used.insert(new);
        m.insert(old, new);
    }
    m
}

fn remap_pf(pf: &PerFixture, fm: &HashMap<u32, u32>, dropped: &mut u32) -> PerFixture {
    let mut out = PerFixture::new();
    for (k, v) in pf {
        match fm.get(k) {
            Some(n) => {
                out.insert(*n, v.clone());
            }
            None => *dropped += 1,
        }
    }
    out
}

fn remap_refs(r: &PerFixtureRefs, fm: &HashMap<u32, u32>, pal: Option<&IdMap>) -> PerFixtureRefs {
    let mut out = PerFixtureRefs::new();
    let Some(pal) = pal else { return out }; // palettes not merged: the stored absolute values remain
    for (k, v) in r {
        let Some(n) = fm.get(k) else { continue };
        let m: BTreeMap<String, u32> = v.iter().filter_map(|(a, p)| pal.get(p).map(|np| (a.clone(), *np))).collect();
        if !m.is_empty() {
            out.insert(*n, m);
        }
    }
    out
}

/// Occupied channels of a universe, to place merged fixtures where they fit.
struct Occ(HashMap<u16, [bool; 512]>);

impl Occ {
    fn mark(&mut self, u: u16, addr: u16, fp: u16) {
        let o = self.0.entry(u).or_insert([false; 512]);
        for b in o.iter_mut().skip(addr.max(1) as usize - 1).take(fp as usize) {
            *b = true;
        }
    }
    fn free(&self, u: u16, addr: u16, fp: u16) -> bool {
        let a = addr.max(1) as usize - 1;
        a + fp as usize <= 512 && self.0.get(&u).map_or(true, |o| o[a..a + fp as usize].iter().all(|b| !b))
    }
    fn first_free(&self, mut u: u16, fp: u16) -> (u16, u16) {
        for _ in 0..64 {
            for a in 1..=(512 - fp.min(512) + 1) {
                if self.free(u, a, fp) {
                    return (u, a);
                }
            }
            u += 1;
        }
        (u, 1)
    }
}

/// `fp(profile id, mode)` = channels of that fixture type (1 when unknown).
pub fn merge(target: &mut Show, src: &Show, parts: &[MergePart], fp: &dyn Fn(&str, &str) -> u16) -> MergeReport {
    let has = |p: MergePart| parts.contains(&p);
    let mut rep = MergeReport::default();
    let count = |rep: &mut MergeReport, p: MergePart, n: usize| {
        if n > 0 {
            rep.added.push(MergeCount { part: p, count: n as u32 });
        }
    };

    // ── fixtures (patch) ──
    let mut fm: HashMap<u32, u32> = HashMap::new();
    if has(MergePart::Fixtures) {
        let mut used: BTreeSet<u32> = target.fixtures.iter().map(|f| f.id).collect();
        let mut occ = Occ(HashMap::new());
        for f in &target.fixtures {
            occ.mark(f.universe, f.address, fp(&f.profile, &f.mode));
        }
        let mut moved = 0;
        for f in &src.fixtures {
            let id = free_id(&used, f.id);
            used.insert(id);
            let n = fp(&f.profile, &f.mode).max(1);
            let (mut u, mut a) = (f.universe.max(1), f.address.max(1));
            if !occ.free(u, a, n) {
                (u, a) = occ.first_free(u, n);
                moved += 1;
            }
            occ.mark(u, a, n);
            let mut nf = f.clone();
            nf.id = id;
            nf.universe = u;
            nf.address = a;
            fm.insert(f.id, id);
            if !target.universes.iter().any(|x| x.id == u) {
                target.universes.push(UniverseCfg { id: u, name: format!("Universe {u}"), outputs: vec![] });
                target.universes.sort_by_key(|x| x.id);
            }
            target.fixtures.push(nf);
            // bring the profile along when the target does not know it
            if !target.profiles.iter().any(|p| p.id == f.profile) {
                if let Some(p) = src.profiles.iter().find(|p| p.id == f.profile) {
                    target.profiles.push(p.clone());
                }
            }
        }
        count(&mut rep, MergePart::Fixtures, src.fixtures.len());
        if moved > 0 {
            rep.warnings.push(format!("{moved} aparato(s) estaban en direcciones ocupadas y se han recolocado en huecos libres: revisa el patch."));
        }
    } else {
        // same id and same profile in both shows = the same fixture
        for f in &src.fixtures {
            if target.fixtures.iter().any(|t| t.id == f.id && t.profile == f.profile) {
                fm.insert(f.id, f.id);
            }
        }
    }
    let mut dropped = 0u32;

    // ── palettes first (cues/scenes/steps link to them) ──
    let mut pal_map: Option<IdMap> = None;
    if has(MergePart::Palettes) {
        let mut used: BTreeSet<u32> = target.palettes.iter().map(|p| p.id).collect();
        let m = assign(&mut used, src.palettes.iter().map(|p| p.id));
        for p in &src.palettes {
            let mut np = p.clone();
            np.id = m[&p.id];
            np.per_fixture = remap_pf(&p.per_fixture, &fm, &mut dropped);
            target.palettes.push(np);
        }
        count(&mut rep, MergePart::Palettes, src.palettes.len());
        pal_map = Some(m);
    }

    // ── speed masters ride along with the chases / effects that use them ──
    let mut sp_map = IdMap::new();
    if has(MergePart::Chases) || has(MergePart::Effects) {
        let mut needed: BTreeSet<u32> = BTreeSet::new();
        if has(MergePart::Chases) {
            needed.extend(src.chases.iter().filter_map(|c| c.speed_master));
        }
        if has(MergePart::Effects) {
            needed.extend(src.effects.iter().filter_map(|e| e.speed_master));
        }
        let mut used: BTreeSet<u32> = target.speed_masters.iter().map(|s| s.id).collect();
        for s in src.speed_masters.iter().filter(|s| needed.contains(&s.id)) {
            let id = free_id(&used, s.id);
            used.insert(id);
            sp_map.insert(s.id, id);
            let mut ns = s.clone();
            ns.id = id;
            target.speed_masters.push(ns);
        }
    }

    // ── groups ──
    if has(MergePart::Groups) {
        let mut used: BTreeSet<u32> = target.groups.iter().map(|g| g.id).collect();
        let m = assign(&mut used, src.groups.iter().map(|g| g.id));
        for g in &src.groups {
            let mut ng = g.clone();
            ng.id = m[&g.id];
            ng.fixtures = g.fixtures.iter().filter_map(|f| fm.get(f).copied()).collect();
            if ng.fixtures.len() != g.fixtures.len() {
                dropped += (g.fixtures.len() - ng.fixtures.len()) as u32;
            }
            target.groups.push(ng);
        }
        count(&mut rep, MergePart::Groups, src.groups.len());
    }

    // ── scenes ──
    let mut scene_map: Option<IdMap> = None;
    if has(MergePart::Scenes) {
        let mut used: BTreeSet<u32> = target.scenes.iter().map(|s| s.id).collect();
        let m = assign(&mut used, src.scenes.iter().map(|s| s.id));
        for s in &src.scenes {
            let mut ns = s.clone();
            ns.id = m[&s.id];
            ns.values = remap_pf(&s.values, &fm, &mut dropped);
            ns.refs = remap_refs(&s.refs, &fm, pal_map.as_ref());
            target.scenes.push(ns);
        }
        count(&mut rep, MergePart::Scenes, src.scenes.len());
        scene_map = Some(m);
    }

    // ── cuelists ──
    let mut cl_map: Option<IdMap> = None;
    if has(MergePart::Cuelists) {
        let mut used: BTreeSet<u32> = target.cuelists.iter().map(|c| c.id).collect();
        let m = assign(&mut used, src.cuelists.iter().map(|c| c.id));
        for c in &src.cuelists {
            let mut nc = c.clone();
            nc.id = m[&c.id];
            for q in &mut nc.cues {
                q.changes = remap_pf(&q.changes, &fm, &mut dropped);
                q.refs = remap_refs(&q.refs, &fm, pal_map.as_ref());
            }
            target.cuelists.push(nc);
        }
        count(&mut rep, MergePart::Cuelists, src.cuelists.len());
        cl_map = Some(m);
    }

    // ── chases ──
    if has(MergePart::Chases) {
        let mut used: BTreeSet<u32> = target.chases.iter().map(|c| c.id).collect();
        let m = assign(&mut used, src.chases.iter().map(|c| c.id));
        let mut cleared_scene = 0;
        for c in &src.chases {
            let mut nc = c.clone();
            nc.id = m[&c.id];
            nc.speed_master = c.speed_master.and_then(|s| sp_map.get(&s).copied());
            for st in &mut nc.steps {
                st.values = remap_pf(&st.values, &fm, &mut dropped);
                st.refs = remap_refs(&st.refs, &fm, pal_map.as_ref());
                st.scene = match (st.scene, &scene_map) {
                    (Some(s), Some(sm)) => sm.get(&s).copied(),
                    (Some(_), None) => {
                        cleared_scene += 1;
                        None
                    }
                    (None, _) => None,
                };
            }
            target.chases.push(nc);
        }
        count(&mut rep, MergePart::Chases, src.chases.len());
        if cleared_scene > 0 {
            rep.warnings.push(format!("{cleared_scene} paso(s) de chase usaban una escena que no se ha fusionado: se quitó la escena (se conservan sus valores)."));
        }
    }

    // ── effects ──
    if has(MergePart::Effects) {
        let mut used: BTreeSet<u32> = target.effects.iter().map(|e| e.id).collect();
        let m = assign(&mut used, src.effects.iter().map(|e| e.id));
        for e in &src.effects {
            let mut ne = e.clone();
            ne.id = m[&e.id];
            ne.fixtures = e.fixtures.iter().filter_map(|f| fm.get(f).copied()).collect();
            ne.speed_master = e.speed_master.and_then(|s| sp_map.get(&s).copied());
            target.effects.push(ne);
        }
        count(&mut rep, MergePart::Effects, src.effects.len());
    }

    // ── macros / timelines (console text by number is not rewritten) ──
    if has(MergePart::Macros) {
        let mut used: BTreeSet<u32> = target.macros.iter().map(|m| m.id).collect();
        let m = assign(&mut used, src.macros.iter().map(|m| m.id));
        for x in &src.macros {
            let mut nx = x.clone();
            nx.id = m[&x.id];
            target.macros.push(nx);
        }
        count(&mut rep, MergePart::Macros, src.macros.len());
    }
    if has(MergePart::Timelines) {
        let mut used: BTreeSet<u32> = target.timelines.iter().map(|m| m.id).collect();
        let m = assign(&mut used, src.timelines.iter().map(|m| m.id));
        for x in &src.timelines {
            let mut nx = x.clone();
            nx.id = m[&x.id];
            target.timelines.push(nx);
        }
        count(&mut rep, MergePart::Timelines, src.timelines.len());
    }

    // ── scripts ──
    if has(MergePart::Scripts) {
        let mut used: BTreeSet<u32> = target.scripts.iter().map(|m| m.id).collect();
        let m = assign(&mut used, src.scripts.iter().map(|m| m.id));
        let mut unlinked = 0;
        for x in &src.scripts {
            let mut nx = x.clone();
            nx.id = m[&x.id];
            for b in &mut nx.blocks {
                for mk in &mut b.marks {
                    if let Some(l) = mk.link.as_mut() {
                        match cl_map.as_ref().and_then(|cm| cm.get(&l.cuelist)) {
                            Some(n) => l.cuelist = *n,
                            None => {
                                mk.link = None;
                                unlinked += 1;
                            }
                        }
                    }
                }
            }
            target.scripts.push(nx);
        }
        count(&mut rep, MergePart::Scripts, src.scripts.len());
        if unlinked > 0 {
            rep.warnings.push(format!("{unlinked} marca(s) del guion enlazaban un cue de una cuelist que no se ha fusionado: se quitó el enlace."));
        }
    }

    if has(MergePart::Locate) {
        let mut n = 0;
        for (k, v) in &src.locate {
            if !target.locate.contains_key(k) {
                target.locate.insert(k.clone(), v.clone());
                n += 1;
            }
        }
        count(&mut rep, MergePart::Locate, n);
    }

    if dropped > 0 {
        rep.warnings.push(format!("{dropped} valor(es) de aparatos que no existen en este show se han descartado (fusiona también el patch, o usa los mismos números de aparato)."));
    }
    let console_text = (has(MergePart::Macros) && !src.macros.is_empty()) || (has(MergePart::Timelines) && !src.timelines.is_empty()) || (has(MergePart::Scripts) && src.scripts.iter().any(|s| s.blocks.iter().any(|b| b.marks.iter().any(|m| !m.actions.is_empty()))));
    if console_text && (cl_map.as_ref().map_or(false, |m| m.iter().any(|(a, b)| a != b)) || scene_map.as_ref().map_or(false, |m| m.iter().any(|(a, b)| a != b))) {
        rep.warnings.push("Hubo ids renumerados: las macros, timelines y acciones del guion que llaman a cues, escenas o grupos por número (p. ej. «go cuelist 2») no se reescriben: revísalas.".into());
    }
    rep
}

/// What the other show offers: counts per part.
pub fn available(src: &Show) -> Vec<MergeCount> {
    let c = |p, n: usize| MergeCount { part: p, count: n as u32 };
    vec![
        c(MergePart::Fixtures, src.fixtures.len()),
        c(MergePart::Groups, src.groups.len()),
        c(MergePart::Palettes, src.palettes.len()),
        c(MergePart::Scenes, src.scenes.len()),
        c(MergePart::Cuelists, src.cuelists.len()),
        c(MergePart::Chases, src.chases.len()),
        c(MergePart::Effects, src.effects.len()),
        c(MergePart::Macros, src.macros.len()),
        c(MergePart::Timelines, src.timelines.len()),
        c(MergePart::Scripts, src.scripts.len()),
        c(MergePart::Locate, src.locate.len()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn show(v: serde_json::Value) -> Show {
        let mut base = serde_json::to_value(Show::default()).unwrap();
        for (k, x) in v.as_object().unwrap() {
            base[k] = x.clone();
        }
        serde_json::from_value(base).expect("show json")
    }
    fn fx(id: u32, u: u16, a: u16) -> serde_json::Value {
        json!({"id": id, "name": format!("F{id}"), "profile": "p", "mode": "m", "universe": u, "address": a})
    }
    const FP: &dyn Fn(&str, &str) -> u16 = &|_, _| 4;

    fn target() -> Show {
        show(json!({
            "fixtures": [fx(1, 1, 1), fx(2, 1, 5)],
            "groups": [{"id": 1, "name": "T", "fixtures": [1, 2]}],
            "palettes": [{"id": 1, "name": "PT", "kind": "color", "values": {"red": 1.0}}],
            "scenes": [{"id": 1, "name": "ST", "values": {"1": {"dimmer": 1.0}}}],
            "cuelists": [{"id": 1, "name": "CT", "cues": []}],
        }))
    }
    fn other() -> Show {
        show(json!({
            "fixtures": [fx(1, 1, 1), fx(7, 1, 9)],
            "groups": [{"id": 1, "name": "S", "fixtures": [1, 7]}],
            "palettes": [{"id": 1, "name": "PS", "kind": "color", "values": {"blue": 1.0}, "per_fixture": {"7": {"blue": 0.5}}}],
            "scenes": [{"id": 1, "name": "SS", "values": {"1": {"dimmer": 0.5}, "7": {"dimmer": 0.2}}, "refs": {"7": {"blue": 1}}}],
            "cuelists": [{"id": 1, "name": "CS", "cues": [{"number": 1, "changes": {"7": {"blue": 1.0}, "99": {"dimmer": 1.0}}, "refs": {"7": {"blue": 1}}}]}],
            "chases": [{"id": 1, "name": "Ch", "speed_master": 3, "steps": [{"values": {"7": {"dimmer": 1.0}}, "scene": 1, "hold": 1.0, "fade": 0.0}]}],
            "speed_masters": [{"id": 3, "name": "Tempo", "bpm": 100.0}],
            "scripts": [{"id": 1, "name": "Obra", "blocks": [{"id": 1, "kind": "speech", "text": "hola", "marks": [{"id": 1, "kind": "light", "at_word": 0, "link": {"cuelist": 1, "number": 1.0}, "actions": []}]}]}],
        }))
    }

    #[test]
    fn patch_is_relocated_and_every_reference_follows_the_new_ids() {
        let (mut t, s) = (target(), other());
        let parts = [MergePart::Fixtures, MergePart::Groups, MergePart::Palettes, MergePart::Scenes, MergePart::Cuelists, MergePart::Chases, MergePart::Scripts];
        let rep = merge(&mut t, &s, &parts, FP);
        // fixtures: id 1 clashes (→ 3), id 7 is free; 1@1 clashes with target 1@1 → moved; 7@9 free
        let ids: Vec<(u32, u16, u16)> = t.fixtures.iter().map(|f| (f.id, f.universe, f.address)).collect();
        assert_eq!(ids[..2], [(1, 1, 1), (2, 1, 5)]);
        assert_eq!(ids[2].0, 3, "merged fixture 1 renumbered: {ids:?}");
        assert_eq!((ids[2].1, ids[2].2), (1, 9), "moved to the first free slot after the existing 1-8");
        assert_eq!(ids[3], (7, 1, 13), "7@9 was free in its own show but the moved fixture took 9-12");
        for (i, a) in ids.iter().enumerate() {
            for b in ids.iter().skip(i + 1) {
                assert!(a.1 != b.1 || a.2 + 4 <= b.2 || b.2 + 4 <= a.2, "overlap {a:?} {b:?}");
            }
        }
        assert!(rep.warnings.iter().any(|w| w.contains("recolocado")));
        // groups: id 1 clashes → 2; members follow fixture ids
        let g = t.groups.iter().find(|g| g.name == "S").unwrap();
        assert_eq!(g.id, 2);
        assert_eq!(g.fixtures, vec![3, 7]);
        // palette → 2, per_fixture follows
        let p = t.palettes.iter().find(|p| p.name == "PS").unwrap();
        assert_eq!(p.id, 2);
        assert!(p.per_fixture.contains_key(&7));
        // scene refs point at the renumbered palette
        let sc = t.scenes.iter().find(|s| s.name == "SS").unwrap();
        assert_eq!(sc.id, 2);
        assert_eq!(sc.values.keys().copied().collect::<Vec<_>>(), vec![3, 7]);
        assert_eq!(sc.refs[&7]["blue"], 2);
        // cuelist: changes remapped; fixture 99 does not exist anywhere → dropped, refs follow palette
        let cl = t.cuelists.iter().find(|c| c.name == "CS").unwrap();
        assert_eq!(cl.id, 2);
        assert!(cl.cues[0].changes.contains_key(&7) && !cl.cues[0].changes.contains_key(&99));
        assert_eq!(cl.cues[0].refs[&7]["blue"], 2);
        // chase: scene and speed master carried over with new ids
        let ch = &t.chases[0];
        assert_eq!(ch.steps[0].scene, Some(2));
        let sp = t.speed_masters.iter().find(|s| s.name == "Tempo").unwrap();
        assert_eq!(ch.speed_master, Some(sp.id));
        // script mark link follows the renumbered cuelist
        assert_eq!(t.scripts[0].blocks[0].marks[0].link.as_ref().unwrap().cuelist, 2);
        assert!(rep.warnings.iter().any(|w| w.contains("descartado")), "{:?}", rep.warnings);
        // counts
        let n = |p| rep.added.iter().find(|c| c.part == p).map_or(0, |c| c.count);
        assert_eq!((n(MergePart::Fixtures), n(MergePart::Groups), n(MergePart::Cuelists)), (2, 1, 1));
    }

    #[test]
    fn without_the_patch_only_matching_fixtures_survive_and_unmerged_references_are_cleared() {
        let (mut t, s) = (target(), other());
        let rep = merge(&mut t, &s, &[MergePart::Scenes, MergePart::Cuelists, MergePart::Chases], FP);
        assert_eq!(t.fixtures.len(), 2, "patch untouched");
        // fixture 1 exists with the same profile → kept; 7 does not → dropped
        let sc = t.scenes.iter().find(|s| s.name == "SS").unwrap();
        assert_eq!(sc.values.keys().copied().collect::<Vec<_>>(), vec![1]);
        assert!(sc.refs.is_empty(), "palettes not merged → links cleared, values stay");
        let ch = &t.chases[0];
        assert!(ch.steps[0].scene.is_some(), "scenes were merged");
        assert!(ch.steps[0].values.is_empty());
        let mut t2 = target();
        let rep2 = merge(&mut t2, &s, &[MergePart::Chases], FP);
        assert_eq!(t2.chases[0].steps[0].scene, None);
        assert!(rep2.warnings.iter().any(|w| w.contains("escena")), "{:?}", rep2.warnings);
        assert!(rep.warnings.iter().any(|w| w.contains("no existen")));
    }

    #[test]
    fn nothing_selected_changes_nothing_and_merging_twice_adds_twice() {
        let mut t = target();
        let before = serde_json::to_string(&t).unwrap();
        let rep = merge(&mut t, &other(), &[], FP);
        assert_eq!(serde_json::to_string(&t).unwrap(), before);
        assert!(rep.added.is_empty());
        let mut t = target();
        merge(&mut t, &other(), &[MergePart::Palettes], FP);
        merge(&mut t, &other(), &[MergePart::Palettes], FP);
        assert_eq!(t.palettes.len(), 3);
        let ids: BTreeSet<u32> = t.palettes.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), 3, "ids stay unique");
        assert!(available(&other()).iter().any(|c| c.part == MergePart::Cuelists && c.count == 1));
    }

    #[test]
    fn a_full_universe_pushes_merged_fixtures_to_the_next_one() {
        let mut t = show(json!({"fixtures": [{"id": 1, "name": "A", "profile": "p", "mode": "m", "universe": 1, "address": 1}]}));
        let s = show(json!({"fixtures": [fx(5, 1, 1)]}));
        let big: &dyn Fn(&str, &str) -> u16 = &|_, _| 512;
        merge(&mut t, &s, &[MergePart::Fixtures], big);
        assert_eq!((t.fixtures[1].universe, t.fixtures[1].address), (2, 1));
        assert!(t.universes.iter().any(|u| u.id == 2), "the new universe exists");
    }
}
