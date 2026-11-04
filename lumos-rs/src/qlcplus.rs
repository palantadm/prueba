//! QLC+ workspace (`.qxw`) import and export: the patch, scenes and chasers.
//!
//! QLC+ numbers universes and DMX addresses from 0 and stores a scene as `channel,value` pairs per fixture.
//! Lumos scenes hold attribute levels, so values go through the fixture's mode (channel ↔ attribute, 16-bit
//! pairs, inverted channels). Cuelists leave as chasers of full-state scenes (tracking is resolved first).

use crate::csv::{resolve_mode, resolve_profile};
use crate::engine::patch::write_fixture;
use crate::model::*;
use roxmltree::{Document, Node};
use std::collections::{BTreeMap, HashMap};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Full per-cue state of a cuelist (tracking resolved), `(number, label, fade_in, hold_s, state)`.
fn resolved_cues(cl: &Cuelist) -> Vec<(f64, String, f32, f32, PerFixture)> {
    let mut state = PerFixture::new();
    let mut out = Vec::new();
    for c in &cl.cues {
        if c.block || !cl.tracking {
            state = c.changes.clone();
        } else {
            for (f, attrs) in &c.changes {
                state.entry(*f).or_default().extend(attrs.iter().map(|(k, v)| (k.clone(), *v)));
            }
        }
        out.push((c.number, c.label.clone(), c.fade_in, c.follow.unwrap_or(0.0), state.clone()));
    }
    out
}

fn num_text(n: f64) -> String {
    if (n - n.round()).abs() < 1e-9 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// `<FixtureVal>` lines for a look: every channel of every known fixture in it.
fn fixture_vals(values: &PerFixture, show: &Show, profs: &HashMap<String, FixtureProfile>, qid: &HashMap<u32, usize>) -> String {
    let mut s = String::new();
    for (fid, attrs) in values {
        let (Some(q), Some(f)) = (qid.get(fid), show.fixtures.iter().find(|f| f.id == *fid)) else { continue };
        let Some(mode) = profs.get(&f.profile).and_then(|p| p.modes.iter().find(|m| m.name == f.mode)) else { continue };
        let fp = crate::library::mode_footprint(mode) as usize;
        let mut frame = [0u8; 512];
        write_fixture(&mut frame, 1, &mode.channels, attrs);
        let pairs: Vec<String> = (0..fp.min(512)).map(|i| format!("{i},{}", frame[i])).collect();
        s.push_str(&format!("   <FixtureVal ID=\"{q}\">{}</FixtureVal>\n", pairs.join(",")));
    }
    s
}

pub fn export(show: &Show, profs: &HashMap<String, FixtureProfile>) -> String {
    let mut o = String::new();
    o.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE Workspace>\n");
    o.push_str("<Workspace xmlns=\"http://www.qlcplus.org/Workspace\" CurrentWindow=\"FixtureManager\">\n");
    o.push_str(&format!(" <Creator>\n  <Name>Lumos</Name>\n  <Version>1</Version>\n  <Author>{}</Author>\n </Creator>\n <Engine>\n", esc(&show.name)));
    // universes: QLC+ counts from 0
    o.push_str("  <InputOutputMap>\n");
    for (i, u) in show.universes.iter().enumerate() {
        o.push_str(&format!("   <Universe Name=\"{}\" ID=\"{i}\"/>\n", esc(&u.name)));
    }
    o.push_str("  </InputOutputMap>\n");
    let uni_idx: HashMap<u16, usize> = show.universes.iter().enumerate().map(|(i, u)| (u.id, i)).collect();
    let mut qid: HashMap<u32, usize> = HashMap::new();
    for f in &show.fixtures {
        let Some(p) = profs.get(&f.profile) else { continue };
        let Some(m) = p.modes.iter().find(|m| m.name == f.mode) else { continue };
        let q = qid.len();
        qid.insert(f.id, q);
        o.push_str(&format!(
            "  <Fixture>\n   <Manufacturer>{}</Manufacturer>\n   <Model>{}</Model>\n   <Mode>{}</Mode>\n   <ID>{q}</ID>\n   <Name>{}</Name>\n   <Universe>{}</Universe>\n   <Address>{}</Address>\n   <Channels>{}</Channels>\n  </Fixture>\n",
            esc(&p.manufacturer),
            esc(&p.model),
            esc(&m.name),
            esc(&f.name),
            uni_idx.get(&f.universe).copied().unwrap_or(f.universe.saturating_sub(1) as usize),
            f.address.saturating_sub(1),
            crate::library::mode_footprint(m)
        ));
    }
    let mut fid = 0usize;
    let mut scene_ids: HashMap<u32, usize> = HashMap::new();
    for sc in &show.scenes {
        scene_ids.insert(sc.id, fid);
        o.push_str(&format!("  <Function ID=\"{fid}\" Type=\"Scene\" Name=\"{}\" Path=\"\">\n", esc(&sc.name)));
        o.push_str(&format!("   <Speed FadeIn=\"{}\" FadeOut=\"{}\" Duration=\"0\"/>\n", (sc.fade_in * 1000.0).round() as i64, (sc.fade_out * 1000.0).round() as i64));
        o.push_str(&fixture_vals(&sc.values, show, profs, &qid));
        o.push_str("  </Function>\n");
        fid += 1;
    }
    let chaser = |o: &mut String, fid: &mut usize, name: &str, dir: &str, order: &str, steps: &[(usize, i64, i64)]| {
        o.push_str(&format!("  <Function ID=\"{fid}\" Type=\"Chaser\" Name=\"{}\" Path=\"\">\n", esc(name)));
        o.push_str("   <Speed FadeIn=\"0\" FadeOut=\"0\" Duration=\"0\"/>\n");
        o.push_str(&format!("   <Direction>{dir}</Direction>\n   <RunOrder>{order}</RunOrder>\n   <SpeedModes FadeIn=\"PerStep\" FadeOut=\"PerStep\" Duration=\"PerStep\"/>\n"));
        for (n, (sid, fade, hold)) in steps.iter().enumerate() {
            o.push_str(&format!("   <Step Number=\"{n}\" FadeIn=\"{fade}\" Hold=\"{hold}\" FadeOut=\"0\">{sid}</Step>\n"));
        }
        o.push_str("  </Function>\n");
        *fid += 1;
    };
    // chases: a step is a scene or an inline look (exported as its own scene)
    for ch in &show.chases {
        let mut steps = Vec::new();
        for (i, st) in ch.steps.iter().enumerate() {
            let sid = match (st.scene, st.values.is_empty()) {
                (Some(s), true) if scene_ids.contains_key(&s) => scene_ids[&s],
                _ => {
                    let id = fid;
                    o.push_str(&format!("  <Function ID=\"{id}\" Type=\"Scene\" Name=\"{} {}\" Path=\"{}\">\n   <Speed FadeIn=\"0\" FadeOut=\"0\" Duration=\"0\"/>\n", esc(&ch.name), i + 1, esc(&ch.name)));
                    o.push_str(&fixture_vals(&st.values, show, profs, &qid));
                    o.push_str("  </Function>\n");
                    fid += 1;
                    id
                }
            };
            let bpm = if ch.bpm > 0.0 { ch.bpm } else { 120.0 };
            let hold = (st.hold.max(0.01) * 60000.0 / bpm).round() as i64;
            steps.push((sid, (hold as f32 * st.fade.clamp(0.0, 1.0)).round() as i64, hold));
        }
        let (dir, order) = match ch.dir {
            ChaseDir::Forward => ("Forward", "Loop"),
            ChaseDir::Backward => ("Backward", "Loop"),
            ChaseDir::Bounce => ("Forward", "PingPong"),
            ChaseDir::Random => ("Forward", "Random"),
        };
        chaser(&mut o, &mut fid, &ch.name, dir, order, &steps);
    }
    // cuelists: one scene per cue (full state) and a chaser over them
    for cl in &show.cuelists {
        let mut steps = Vec::new();
        for (num, label, fade_in, hold_s, state) in resolved_cues(cl) {
            let id = fid;
            let name = if label.is_empty() { format!("{} {}", cl.name, num_text(num)) } else { format!("{} {} {}", cl.name, num_text(num), label) };
            o.push_str(&format!("  <Function ID=\"{id}\" Type=\"Scene\" Name=\"{}\" Path=\"{}\">\n   <Speed FadeIn=\"0\" FadeOut=\"0\" Duration=\"0\"/>\n", esc(&name), esc(&cl.name)));
            o.push_str(&fixture_vals(&state, show, profs, &qid));
            o.push_str("  </Function>\n");
            fid += 1;
            steps.push((id, (fade_in * 1000.0).round() as i64, (hold_s * 1000.0).round() as i64));
        }
        chaser(&mut o, &mut fid, &cl.name, "Forward", if cl.looped { "Loop" } else { "SingleShot" }, &steps);
    }
    o.push_str(" </Engine>\n <VirtualConsole/>\n <SimpleDesk/>\n</Workspace>\n");
    o
}

// ───────────────────────────── import ─────────────────────────────

#[derive(Default, Debug)]
pub struct Report {
    pub fixtures: usize,
    pub scenes: usize,
    pub chasers: usize,
    /// `(fixture name, "manufacturer model")` that no profile of the library matches.
    pub unmatched: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

fn is(n: &Node, tag: &str) -> bool {
    n.is_element() && n.tag_name().name() == tag
}

fn child_text(n: Node, tag: &str) -> String {
    n.children().find(|c| is(c, tag)).and_then(|c| c.text()).map(|t| t.trim().to_string()).unwrap_or_default()
}

fn ms_attr(n: Option<Node>, k: &str) -> f32 {
    n.and_then(|n| n.attribute(k)).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0).max(0.0) / 1000.0
}

/// `channel,value,channel,value…` → attribute levels of the fixture's mode.
fn attrs_from_pairs(mode: &Mode, text: &str) -> Attrs {
    let nums: Vec<i64> = text.split(',').filter_map(|t| t.trim().parse().ok()).collect();
    let mut ch: BTreeMap<u16, u8> = BTreeMap::new();
    for p in nums.chunks(2) {
        if let [c, v] = p {
            if (0..512).contains(c) {
                ch.insert(*c as u16, (*v).clamp(0, 255) as u8);
            }
        }
    }
    let mut a = Attrs::new();
    for def in &mode.channels {
        let Some(&c) = ch.get(&def.offset) else { continue };
        let mut v = match def.fine.and_then(|f| ch.get(&f)) {
            Some(&f) => (c as f32 * 256.0 + f as f32) / 65535.0,
            None => c as f32 / 255.0,
        };
        if def.invert {
            v = 1.0 - v;
        }
        a.insert(def.attr.clone(), v);
    }
    a
}

/// Merge a QLC+ workspace into the show. `replace`: drop the current fixtures, scenes and chases first.
pub fn import(xml: &str, show: &mut Show, profs: &HashMap<String, FixtureProfile>, replace: bool) -> Result<Report, String> {
    parse_workspace(xml).and_then(|doc| import_doc(&doc, show, profs, replace))
}

/// QLC+ files start with `<!DOCTYPE Workspace>`; roxmltree refuses DTDs unless allowed (its entity limits still apply).
fn parse_workspace(xml: &str) -> Result<Document<'_>, String> {
    Document::parse_with_options(xml, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }).map_err(|e| format!("No es un workspace de QLC+ válido: {e}"))
}

fn import_doc(doc: &Document, show: &mut Show, profs: &HashMap<String, FixtureProfile>, replace: bool) -> Result<Report, String> {
    let root = doc.root_element();
    if !is(&root, "Workspace") {
        return Err("No es un workspace de QLC+ (.qxw): falta el elemento <Workspace>".into());
    }
    let engine = root.children().find(|c| is(c, "Engine")).ok_or("El workspace no tiene <Engine>")?;
    let mut rep = Report::default();
    if replace {
        show.fixtures.clear();
        for g in &mut show.groups {
            g.fixtures.clear();
        }
        show.scenes.clear();
        show.chases.clear();
        for e in &mut show.executors {
            if matches!(e.kind, ExecKind::Scene { .. } | ExecKind::Chase { .. }) {
                e.kind = ExecKind::Empty;
            }
        }
    }
    // QLC+ universe names by index
    let mut uni_names: HashMap<u32, String> = HashMap::new();
    for io in engine.children().filter(|c| is(c, "InputOutputMap")) {
        for u in io.children().filter(|c| is(c, "Universe")) {
            if let Some(id) = u.attribute("ID").and_then(|v| v.parse::<u32>().ok()) {
                uni_names.insert(id, u.attribute("Name").unwrap_or("").to_string());
            }
        }
    }
    let base = show.fixtures.iter().map(|f| f.id).max().unwrap_or(0);
    // QLC+ fixture id → (Lumos id, mode)
    let mut map: HashMap<u32, (u32, Mode)> = HashMap::new();
    for fx in engine.children().filter(|c| is(c, "Fixture")) {
        let (manu, model, mode_name, name) = (child_text(fx, "Manufacturer"), child_text(fx, "Model"), child_text(fx, "Mode"), child_text(fx, "Name"));
        let Ok(qid) = child_text(fx, "ID").parse::<u32>() else { continue };
        let label = if name.is_empty() { format!("{manu} {model}") } else { name.clone() };
        let key = format!("{manu} {model}");
        let Some(prof) = resolve_profile(profs, &key).or_else(|| resolve_profile(profs, &model)) else {
            rep.unmatched.push((label, key.trim().to_string()));
            continue;
        };
        let mode = match resolve_mode(prof, &mode_name) {
            Ok(m) => m,
            Err(_) => {
                let Some(m) = prof.modes.first() else { continue };
                rep.warnings.push(format!("{label}: el modo «{mode_name}» no existe en {}; se usa «{}»", prof.id, m.name));
                m
            }
        };
        let universe = child_text(fx, "Universe").parse::<u32>().unwrap_or(0);
        let address = child_text(fx, "Address").parse::<u32>().unwrap_or(0) + 1;
        if address > 512 {
            rep.warnings.push(format!("{label}: dirección {address} fuera de rango; omitido"));
            continue;
        }
        let id = base + 1 + qid;
        let uid = (universe + 1) as u16;
        if !show.universes.iter().any(|u| u.id == uid) {
            let name = uni_names.get(&universe).filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| format!("Universe {uid}"));
            show.universes.push(UniverseCfg { id: uid, name, outputs: vec![] });
            show.universes.sort_by_key(|u| u.id);
        }
        show.fixtures.push(Fixture { id, name: if name.is_empty() { format!("Fixture {id}") } else { name }, profile: prof.id.clone(), mode: mode.name.clone(), universe: uid, address: address as u16, pos: [0.0; 3], rot: [0.0; 3], curve: Default::default(), orient: Default::default() });
        map.insert(qid, (id, mode.clone()));
        rep.fixtures += 1;
    }
    // scenes first (chasers refer to them)
    let mut scene_of: HashMap<u32, u32> = HashMap::new();
    let mut next_scene = show.scenes.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    for f in engine.children().filter(|c| is(c, "Function") && c.attribute("Type") == Some("Scene")) {
        let Some(fid) = f.attribute("ID").and_then(|v| v.parse::<u32>().ok()) else { continue };
        let mut values = PerFixture::new();
        for v in f.children().filter(|c| is(c, "FixtureVal")) {
            let Some(q) = v.attribute("ID").and_then(|x| x.parse::<u32>().ok()) else { continue };
            let Some((lid, mode)) = map.get(&q) else { continue };
            let a = attrs_from_pairs(mode, v.text().unwrap_or(""));
            if !a.is_empty() {
                values.insert(*lid, a);
            }
        }
        let speed = f.children().find(|c| is(c, "Speed"));
        let name = f.attribute("Name").filter(|n| !n.is_empty()).unwrap_or("Scene").to_string();
        show.scenes.push(Scene { id: next_scene, name, values, fade_in: ms_attr(speed, "FadeIn"), fade_out: ms_attr(speed, "FadeOut"), color: String::new(), refs: Default::default(), kind: SceneKind::Look });
        scene_of.insert(fid, next_scene);
        next_scene += 1;
        rep.scenes += 1;
    }
    let mut next_chase = show.chases.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    for f in engine.children().filter(|c| is(c, "Function") && c.attribute("Type") == Some("Chaser")) {
        let name = f.attribute("Name").filter(|n| !n.is_empty()).unwrap_or("Chaser").to_string();
        let mut steps = Vec::new();
        for st in f.children().filter(|c| is(c, "Step")) {
            let Some(sid) = st.text().and_then(|t| t.trim().parse::<u32>().ok()).and_then(|q| scene_of.get(&q).copied()) else {
                rep.warnings.push(format!("{name}: un paso apunta a una función que no es una escena; omitido"));
                continue;
            };
            let hold_ms = st.attribute("Hold").and_then(|v| v.parse::<f32>().ok()).unwrap_or(500.0).max(10.0);
            let fade_ms = st.attribute("FadeIn").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0).max(0.0);
            // 120 BPM = 500 ms per beat
            steps.push(ChaseStep { values: Default::default(), refs: Default::default(), scene: Some(sid), hold: hold_ms / 500.0, fade: (fade_ms / hold_ms).clamp(0.0, 1.0) });
        }
        if steps.is_empty() {
            continue;
        }
        let dir = match (child_text(f, "RunOrder").as_str(), child_text(f, "Direction").as_str()) {
            ("PingPong", _) => ChaseDir::Bounce,
            ("Random", _) => ChaseDir::Random,
            (_, "Backward") => ChaseDir::Backward,
            _ => ChaseDir::Forward,
        };
        show.chases.push(Chase { id: next_chase, name, steps, bpm: 120.0, speed_master: None, dir });
        next_chase += 1;
        rep.chasers += 1;
    }
    show.fixtures.sort_by_key(|f| f.id);
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::builtin;

    fn profs() -> HashMap<String, FixtureProfile> {
        builtin().into_iter().map(|p| (p.id.clone(), p)).collect()
    }

    fn rig() -> Show {
        let mut s = Show::default();
        s.name = "Función & «ñ»".into();
        let p = profs();
        let par = &p["generic/rgb-par"];
        let mh = &p["generic/mh-spot"];
        s.fixtures.push(Fixture { id: 1, name: "Par 1".into(), profile: par.id.clone(), mode: par.modes[0].name.clone(), universe: 1, address: 1, pos: [0.0; 3], rot: [0.0; 3], curve: Default::default(), orient: Default::default() });
        s.fixtures.push(Fixture { id: 2, name: "Mover".into(), profile: mh.id.clone(), mode: mh.modes[0].name.clone(), universe: 2, address: 10, pos: [0.0; 3], rot: [0.0; 3], curve: Default::default(), orient: Default::default() });
        let mut a = Attrs::new();
        a.insert("red".into(), 1.0);
        a.insert("green".into(), 0.5);
        let mut b = Attrs::new();
        b.insert("pan".into(), 0.25);
        b.insert("dimmer".into(), 1.0);
        let mut values = PerFixture::new();
        values.insert(1, a);
        values.insert(2, b);
        s.scenes.push(Scene { id: 7, name: "Rojo <fuerte>".into(), values: values.clone(), fade_in: 1.5, fade_out: 2.0, color: String::new(), refs: Default::default(), kind: SceneKind::Look });
        s.chases.push(Chase { id: 3, name: "Ronda".into(), steps: vec![ChaseStep { values: Default::default(), refs: Default::default(), scene: Some(7), hold: 2.0, fade: 0.5 }, ChaseStep { values, refs: Default::default(), scene: None, hold: 1.0, fade: 0.0 }], bpm: 120.0, speed_master: None, dir: ChaseDir::Bounce });
        s
    }

    #[test]
    fn export_is_well_formed_and_uses_zero_based_numbers() {
        let s = rig();
        let xml = export(&s, &profs());
        let doc = parse_workspace(&xml).expect("well-formed XML");
        let fixtures: Vec<_> = doc.descendants().filter(|n| is(n, "Fixture")).collect();
        assert_eq!(fixtures.len(), 2);
        assert_eq!(child_text(fixtures[0], "Universe"), "0");
        assert_eq!(child_text(fixtures[0], "Address"), "0", "DMX 1 is address 0 in QLC+");
        assert_eq!(child_text(fixtures[1], "Universe"), "1");
        assert_eq!(child_text(fixtures[1], "Address"), "9");
        assert!(xml.contains("Name=\"Rojo &lt;fuerte&gt;\"") && xml.contains("Type=\"Chaser\""));
        assert!(xml.contains("<RunOrder>PingPong</RunOrder>"));
        // a scene stores 1.5 s fade in, as milliseconds
        assert!(xml.contains("FadeIn=\"1500\" FadeOut=\"2000\""));
    }

    #[test]
    fn round_trip_keeps_patch_scene_values_and_chase_timing() {
        let s = rig();
        let xml = export(&s, &profs());
        let mut t = Show::default();
        t.universes.clear();
        let rep = import(&xml, &mut t, &profs(), true).unwrap();
        assert_eq!((rep.fixtures, rep.chasers), (2, 1), "{rep:?}");
        assert!(rep.unmatched.is_empty(), "{:?}", rep.unmatched);
        assert_eq!(t.fixtures.len(), 2);
        assert_eq!((t.fixtures[0].universe, t.fixtures[0].address, t.fixtures[1].universe, t.fixtures[1].address), (1, 1, 2, 10));
        assert_eq!(t.fixtures[1].profile, "generic/mh-spot");
        let sc = t.scenes.iter().find(|x| x.name == "Rojo <fuerte>").unwrap();
        assert!((sc.fade_in - 1.5).abs() < 1e-3 && (sc.fade_out - 2.0).abs() < 1e-3);
        let (f1, f2) = (t.fixtures[0].id, t.fixtures[1].id);
        assert!((sc.values[&f1]["red"] - 1.0).abs() < 0.005 && (sc.values[&f1]["green"] - 0.5).abs() < 0.005);
        assert!((sc.values[&f2]["pan"] - 0.25).abs() < 0.001, "16-bit pan survives: {}", sc.values[&f2]["pan"]);
        assert!((sc.values[&f2]["dimmer"] - 1.0).abs() < 0.005);
        let ch = &t.chases[0];
        assert_eq!(ch.dir, ChaseDir::Bounce);
        assert_eq!(ch.steps.len(), 2);
        assert!((ch.steps[0].hold - 2.0).abs() < 1e-3 && (ch.steps[0].fade - 0.5).abs() < 1e-3, "{:?}", ch.steps[0]);
        assert!(ch.steps.iter().all(|s| s.scene.is_some()), "inline steps became scenes");
    }

    #[test]
    fn cuelists_leave_as_chasers_of_full_states() {
        let mut s = rig();
        let mut c1 = Cue { number: 1.0, label: "Uno".into(), fade_in: 2.0, fade_out: 2.0, delay: 0.0, follow: None, timecode: None, changes: PerFixture::new(), block: false, refs: Default::default(), timing: Default::default(), curve: Default::default(), stagger: 0.0, notes: String::new() };
        let mut a = Attrs::new();
        a.insert("red".into(), 1.0);
        c1.changes.insert(1, a);
        let mut c2 = c1.clone();
        c2.number = 2.0;
        c2.label = String::new();
        c2.changes = PerFixture::new();
        let mut b = Attrs::new();
        b.insert("blue".into(), 1.0);
        c2.changes.insert(1, b);
        s.cuelists.push(Cuelist { id: 1, name: "Acto 1".into(), cues: vec![c1, c2], looped: false, tracking: true, priority: 0, auto_mark: false });
        let xml = export(&s, &profs());
        let mut t = Show::default();
        let rep = import(&xml, &mut t, &profs(), true).unwrap();
        assert_eq!(rep.chasers, 2, "the chase and the cuelist");
        let cue2 = t.scenes.iter().find(|x| x.name == "Acto 1 2").expect("cue 2 scene");
        let f1 = t.fixtures[0].id;
        assert!(cue2.values[&f1]["red"] > 0.99 && cue2.values[&f1]["blue"] > 0.99, "tracked: red from cue 1 stays in cue 2");
    }

    #[test]
    fn unknown_fixtures_bad_files_and_merge_are_reported() {
        let xml = r#"<?xml version="1.0"?><Workspace xmlns="http://www.qlcplus.org/Workspace"><Engine>
            <Fixture><Manufacturer>Nadie</Manufacturer><Model>Inventado 9000</Model><Mode>5ch</Mode><ID>0</ID><Name>X</Name><Universe>0</Universe><Address>0</Address><Channels>5</Channels></Fixture>
            <Fixture><Manufacturer>Generic</Manufacturer><Model>RGB Par</Model><Mode>3ch</Mode><ID>1</ID><Name>P</Name><Universe>0</Universe><Address>600</Address><Channels>3</Channels></Fixture>
            </Engine></Workspace>"#;
        let mut t = rig();
        let before = t.fixtures.len();
        let rep = import(xml, &mut t, &profs(), false).unwrap();
        assert_eq!(rep.unmatched.len(), 1);
        assert_eq!(rep.unmatched[0].1, "Nadie Inventado 9000");
        assert!(rep.warnings.iter().any(|w| w.contains("fuera de rango")));
        assert_eq!(t.fixtures.len(), before, "nothing usable → nothing added; existing kept");
        assert!(import("<html/>", &mut t, &profs(), false).is_err());
        // an entity bomb is refused, not expanded
        let bomb = "<!DOCTYPE Workspace [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\"><!ENTITY c \"&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;\"><!ENTITY d \"&c;&c;&c;&c;&c;&c;&c;&c;&c;&c;\"><!ENTITY e \"&d;&d;&d;&d;&d;&d;&d;&d;&d;&d;\"><!ENTITY f \"&e;&e;&e;&e;&e;&e;&e;&e;&e;&e;\"><!ENTITY g \"&f;&f;&f;&f;&f;&f;&f;&f;&f;&f;\"><!ENTITY h \"&g;&g;&g;&g;&g;&g;&g;&g;&g;&g;\"><!ENTITY i \"&h;&h;&h;&h;&h;&h;&h;&h;&h;&h;\"><!ENTITY j \"&i;&i;&i;&i;&i;&i;&i;&i;&i;&i;\">]><Workspace><Engine><Fixture><Name>&j;</Name></Fixture></Engine></Workspace>";
        assert!(import(bomb, &mut t, &profs(), false).is_err());
        assert!(import("no xml", &mut t, &profs(), false).is_err());
    }
}
