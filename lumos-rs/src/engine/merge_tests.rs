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
