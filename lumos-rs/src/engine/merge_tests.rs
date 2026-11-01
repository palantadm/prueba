//! `ShowMerge` end to end: a saved show is merged into the live one, can be previewed and undone.

use super::tests_support::*;
use super::*;

fn saved_show(e: &Engine) -> Show {
    let mut s = Show::default();
    s.name = "Anterior".into();
    s.groups.push(Group { id: 1, name: "Frente".into(), fixtures: vec![] });
    s.palettes.push(serde_json::from_value(serde_json::json!({"id": 1, "name": "Ámbar", "kind": "color", "values": {"red": 1.0, "green": 0.6}})).unwrap());
    s.cuelists.push(serde_json::from_value(serde_json::json!({"id": 1, "name": "Acto 1", "cues": [{"number": 1.0}]})).unwrap());
    e.store.save("Anterior", &s).unwrap();
    s
}

#[test]
fn merge_adds_selected_parts_renumbers_clashes_and_is_undoable() {
    let mut e = engine();
    e.show.groups.push(Group { id: 1, name: "Mío".into(), fixtures: vec![] });
    saved_show(&e);
    let before_undo = e.prog.hist.counts().0;
    e.handle(Command::ShowMerge { name: "Anterior".into(), parts: vec![MergePart::Groups, MergePart::Palettes] }).unwrap();
    assert_eq!(e.show.groups.iter().map(|g| (g.id, g.name.as_str())).collect::<Vec<_>>(), vec![(1, "Mío"), (2, "Frente")]);
    assert_eq!(e.show.palettes.len(), 1);
    assert!(e.show.cuelists.is_empty(), "not selected");
    assert_eq!(e.prog.hist.counts().0, before_undo + 1, "one undo step");
    e.handle(Command::Undo).unwrap();
    assert_eq!(e.show.groups.len(), 1);
    assert!(e.show.palettes.is_empty());
}

#[test]
fn preview_reports_without_changing_anything_and_unknown_shows_fail() {
    let mut e = engine();
    saved_show(&e);
    let r = e.merge_report("Anterior", &[MergePart::Cuelists], false).unwrap();
    assert_eq!(r.added, vec![MergeCount { part: MergePart::Cuelists, count: 1 }]);
    assert!(r.available.iter().any(|c| c.part == MergePart::Palettes && c.count == 1));
    assert!(e.show.cuelists.is_empty(), "a preview never writes");
    assert!(e.handle(Command::ShowMerge { name: "No existe".into(), parts: vec![MergePart::Groups] }).is_err());
}

#[test]
fn a_custom_profile_can_be_saved_patched_and_edited_safely() {
    let mut e = engine();
    let ch = |a: &str, o: u16| serde_json::json!({"attr": a, "offset": o, "fine": null, "default": 0.0, "invert": false, "capabilities": []});
    let prof = |modes: Vec<serde_json::Value>| -> FixtureProfile {
        serde_json::from_value(serde_json::json!({"id": "acme/par-led", "manufacturer": "Acme", "model": "Par LED", "category": "par", "modes": modes, "beam_angle": [25.0, 25.0], "pan_range": 0.0, "tilt_range": 0.0, "power_w": 60.0})).unwrap()
    };
    let m4 = serde_json::json!({"name": "4ch", "channels": [ch("dimmer", 0), ch("red", 1), ch("green", 2), ch("blue", 3)]});
    let m3 = serde_json::json!({"name": "3ch", "channels": [ch("red", 0), ch("green", 1), ch("blue", 2)]});
    e.handle(Command::ProfileSave { profile: prof(vec![m4.clone(), m3.clone()]) }).unwrap();
    assert_eq!(e.show.profiles[0].source.as_deref(), Some("custom"));
    e.handle(Command::FixtureAdd { profile: "acme/par-led".into(), mode: "4ch".into(), count: 2, universe: 1, address: None, name: Some("P".into()) }).unwrap();
    assert_eq!(e.show.fixtures[1].address, 5, "4 channels each");
    // dropping the mode in use is refused; dropping an unused one is fine; editing replaces in place
    assert!(e.handle(Command::ProfileSave { profile: prof(vec![m3.clone()]) }).unwrap_err().contains("usa el modo"));
    e.handle(Command::ProfileSave { profile: prof(vec![m4.clone()]) }).unwrap();
    assert_eq!(e.show.profiles.len(), 1);
    // the built-in library is read-only
    let mut p = prof(vec![m4]);
    p.id = "generic/rgb-par".into();
    assert!(e.handle(Command::ProfileSave { profile: p }).unwrap_err().contains("biblioteca de Lumos"));
}

#[test]
fn the_response_curve_shapes_the_output_of_a_fixture() {
    let mut e = engine();
    add(&mut e, "generic/dimmer", "1ch", 2);
    let out = |e: &mut Engine| {
        e.rebuild_derived();
        e.render(1.0);
        let f = **e.frames.get(&1).unwrap();
        (f[0], f[1])
    };
    set(&mut e, 1, "dimmer", 0.5);
    set(&mut e, 2, "dimmer", 0.5);
    assert_eq!(out(&mut e), (128, 128), "linear");
    let mut f1 = e.show.fixtures[0].clone();
    f1.curve = ResponseCurve::Square;
    e.handle(Command::FixtureUpdate { fixture: f1 }).unwrap();
    let mut f2 = e.show.fixtures[1].clone();
    f2.curve = ResponseCurve::Switched;
    e.handle(Command::FixtureUpdate { fixture: f2 }).unwrap();
    let (a, b) = out(&mut e);
    assert_eq!(a, 64, "0.5² = 0.25 → 64");
    assert_eq!(b, 255, "switched: ≥ 50 % is full");
    set(&mut e, 2, "dimmer", 0.4);
    assert_eq!(out(&mut e).1, 0, "switched: below 50 % is off");
    assert!((shape(ResponseCurve::Gamma, 0.5) - 0.2176).abs() < 1e-3 && shape(ResponseCurve::SCurve, 0.5) == 0.5);
}
