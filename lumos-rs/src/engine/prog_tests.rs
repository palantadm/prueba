//! Tests for the programming features: palette links, undo/redo, blind, cue load/update, park,
//! per-attribute timing/curves/stagger, auto-mark and the manual crossfader.

use super::tests_support::*;
use super::*;

fn cue_with(number: f64, vals: &[(u32, &str, f32)]) -> Cue {
    let mut changes = PerFixture::new();
    for (f, a, v) in vals {
        changes.entry(*f).or_default().insert((*a).to_string(), *v);
    }
    Cue {
        number,
        label: String::new(),
        fade_in: 2.0,
        fade_out: 2.0,
        delay: 0.0,
        follow: None,
        timecode: None,
        changes,
        block: false,
        refs: Default::default(),
        timing: Default::default(),
        curve: Default::default(),
        stagger: 0.0,
        notes: String::new(),
    }
}

fn save_cue(e: &mut Engine, cl: u32, c: Cue) {
    if !e.show.cuelists.iter().any(|x| x.id == cl) {
        e.handle(Command::CuelistSave {
            cuelist: Cuelist { id: cl, name: "t".into(), cues: vec![], looped: false, tracking: true, priority: 0, auto_mark: false },
        })
        .unwrap();
    }
    e.handle(Command::CueSave { cuelist: cl, cue: c }).unwrap();
}

fn rec(e: &mut Engine, refs: bool, release: bool) {
    e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: String::new(), release, refs }).unwrap();
}

fn instant(e: &mut Engine) {
    for cl in e.show.cuelists.iter_mut() {
        for c in cl.cues.iter_mut() {
            c.fade_in = 0.0;
            c.fade_out = 0.0;
        }
    }
}

fn red_palette(e: &mut Engine, fixtures: &[u32], r: f32) -> u32 {
    for f in fixtures {
        set(e, *f, "red", r);
    }
    e.handle(Command::ProgRecordPalette { id: None, fixtures: fixtures.to_vec(), kind: PaletteKind::Color, name: "Red".into() }).unwrap();
    e.handle(Command::ProgRelease { fixtures: None, attrs: None }).unwrap();
    e.show.palettes.last().unwrap().id
}

fn run_vals(e: &Engine, cl: u32, f: u32, a: &str) -> f32 {
    e.runs[&cl].values[&f][a]
}

// ───────────── palette links ─────────────

#[test]
fn recorded_cue_links_palette_and_follows_edits() {
    let mut e = engine();
    add(&mut e, "generic/rgb-par", "3ch", 2);
    let pid = red_palette(&mut e, &[1, 2], 1.0);
    e.handle(Command::ProgApplyPalette { fixtures: vec![1, 2], palette: pid }).unwrap();
    rec(&mut e, true, true);
    {
        let cue = &e.show.cuelists[0].cues[0];
        assert_eq!(cue.refs[&1]["red"], pid);
        assert_eq!(cue.refs[&2]["red"], pid);
        assert_eq!(cue.changes[&1]["red"], 1.0);
    }
    instant(&mut e);
    e.handle(Command::Go { cuelist: 1 }).unwrap();
    e.update_playback(0.1);
    assert_eq!(frame(&mut e)[0], 255);

    // edit the palette: the cue's stored value follows and the playing cue updates immediately
    let mut pal = e.show.palettes[0].clone();
    pal.values.insert("red".into(), 0.5);
    e.handle(Command::PaletteSave { palette: pal.clone() }).unwrap();
    assert!((e.show.cuelists[0].cues[0].changes[&1]["red"] - 0.5).abs() < 1e-6);
    assert_eq!(frame(&mut e)[0], 128);
    pal.values.insert("red".into(), 0.25);
    e.handle(Command::PaletteSave { palette: pal }).unwrap();
    assert_eq!(frame(&mut e)[0], 64);
}

#[test]
fn palette_edit_reaches_tracked_cues() {
    let mut e = engine();
    add(&mut e, "generic/rgb-par", "3ch", 1);
    let pid = red_palette(&mut e, &[1], 1.0);
    e.handle(Command::ProgApplyPalette { fixtures: vec![1], palette: pid }).unwrap();
    rec(&mut e, true, true);
    // cue 2 only changes green: red is tracked from cue 1
    set(&mut e, 1, "green", 1.0);
    rec(&mut e, true, true);
    instant(&mut e);
    let mut pal = e.show.palettes[0].clone();
    pal.values.insert("red".into(), 0.2);
    e.handle(Command::PaletteSave { palette: pal }).unwrap();
    e.handle(Command::GoTo { cuelist: 1, number: 2.0 }).unwrap();
    e.update_playback(0.1);
    let f = frame(&mut e);
    assert_eq!((f[0], f[1]), (51, 255));
}

#[test]
fn refs_false_and_manual_touch_store_plain_values() {
    let mut e = engine();
    add(&mut e, "generic/rgb-par", "3ch", 2);
    let pid = red_palette(&mut e, &[1, 2], 1.0);
    e.handle(Command::ProgApplyPalette { fixtures: vec![1, 2], palette: pid }).unwrap();
    rec(&mut e, false, false);
    assert!(e.show.cuelists[0].cues[0].refs.is_empty());
    // touching fixture 1 by hand drops only its link
    set(&mut e, 1, "red", 0.7);
    rec(&mut e, true, true);
    let c = &e.show.cuelists[0].cues[1];
    assert!(c.refs.get(&1).map_or(true, |m| !m.contains_key("red")));
    assert_eq!(c.refs[&2]["red"], pid);
    assert!((c.changes[&1]["red"] - 0.7).abs() < 1e-6);
}

#[test]
fn removing_a_palette_keeps_values_and_drops_links() {
    let mut e = engine();
    add(&mut e, "generic/rgb-par", "3ch", 1);
    let pid = red_palette(&mut e, &[1], 0.6);
    e.handle(Command::ProgApplyPalette { fixtures: vec![1], palette: pid }).unwrap();
    rec(&mut e, true, true);
    assert!(!e.show.cuelists[0].cues[0].refs.is_empty());
    e.handle(Command::PaletteRemove { id: pid }).unwrap();
    let c = &e.show.cuelists[0].cues[0];
    assert!(c.refs.is_empty());
    assert!((c.changes[&1]["red"] - 0.6).abs() < 1e-6);
}

#[test]
fn scene_record_links_palette() {
    let mut e = engine();
    add(&mut e, "generic/rgb-par", "3ch", 1);
    let pid = red_palette(&mut e, &[1], 1.0);
    e.handle(Command::ProgApplyPalette { fixtures: vec![1], palette: pid }).unwrap();
    e.handle(Command::SceneRecord { id: None, name: "S".into(), from_output: false, fixtures: None, merge: false, refs: true }).unwrap();
    assert_eq!(e.show.scenes[0].refs[&1]["red"], pid);
    let mut pal = e.show.palettes[0].clone();
    pal.values.insert("red".into(), 0.3);
    e.handle(Command::PaletteSave { palette: pal }).unwrap();
    assert!((e.show.scenes[0].values[&1]["red"] - 0.3).abs() < 1e-6);
}

#[test]
fn cue_load_restores_links_and_edits_can_be_updated_back() {
    let mut e = engine();
    add(&mut e, "generic/rgb-par", "3ch", 1);
    let pid = red_palette(&mut e, &[1], 1.0);
    e.handle(Command::ProgApplyPalette { fixtures: vec![1], palette: pid }).unwrap();
    rec(&mut e, true, true);
    assert!(e.programmer.is_empty());
    e.handle(Command::CueLoad { cuelist: 1, number: 1.0 }).unwrap();
    assert_eq!(e.programmer[&1]["red"], 1.0);
    assert_eq!(e.prog.links[&1]["red"].0, pid);
    // update keeps the link
    e.handle(Command::CueUpdate { cuelist: 1, number: 1.0, mode: UpdateMode::Merge, refs: true }).unwrap();
    assert_eq!(e.show.cuelists[0].cues[0].refs[&1]["red"], pid);
    assert!(e.handle(Command::CueLoad { cuelist: 1, number: 9.0 }).is_err());
}

#[test]
fn numeric_key_json_round_trips_with_refs_and_timing() {
    let j = r#"{"t":"cmd","id":"7","cmd":{"type":"cue_save","cuelist":1,"cue":{
        "number":2.5,"label":"x","fade_in":1.0,"fade_out":1.0,"delay":0.0,"follow":null,"timecode":null,
        "changes":{"1":{"red":0.5}},"block":false,"refs":{"1":{"red":3}},
        "timing":{"color":{"delay":0.5,"fade":3.0}},"curve":"s_curve","stagger":0.1,"notes":"hi"}}}"#;
    let m: ClientMsg = serde_json::from_str(j).unwrap();
    let Command::CueSave { cue, .. } = m.cmd else { panic!() };
    assert_eq!(cue.refs[&1]["red"], 3);
    assert_eq!(cue.timing["color"].fade, 3.0);
    assert_eq!(cue.curve, FadeCurve::SCurve);
    assert_eq!(cue.stagger, 0.1);
    let back = serde_json::to_string(&cue).unwrap();
    let again: Cue = serde_json::from_str(&back).unwrap();
    assert_eq!(again.refs, cue.refs);
    // scenes and chase-style maps too
    let s = r#"{"id":4,"name":"S","values":{"2":{"dimmer":1.0}},"fade_in":1.0,"fade_out":1.0,"color":"","refs":{"2":{"dimmer":9}},"kind":"inhibitive"}"#;
    let sc: Scene = serde_json::from_str(s).unwrap();
    assert_eq!(sc.refs[&2]["dimmer"], 9);
    assert_eq!(sc.kind, SceneKind::Inhibitive);
    // old shows without the new fields still load
    let old = r#"{"number":1.0,"changes":{"1":{"dimmer":1.0}}}"#;
    let c: Cue = serde_json::from_str(old).unwrap();
    assert!(c.refs.is_empty() && c.timing.is_empty() && c.curve == FadeCurve::Linear);
}

// ───────────── undo / redo ─────────────

#[test]
fn undo_redo_fixture_add_and_counts() {
    let mut e = engine();
    e.handle(Command::FixtureAdd { profile: "generic/dimmer".into(), mode: "1ch".into(), count: 2, universe: 1, address: None, name: None }).unwrap();
    assert_eq!(e.show.fixtures.len(), 2);
    let rt = e.build_rt();
    assert_eq!((rt.undo, rt.redo), (1, 0));
    e.handle(Command::Undo).unwrap();
    assert!(e.show.fixtures.is_empty());
    let rt = e.build_rt();
    assert_eq!((rt.undo, rt.redo), (0, 1));
    e.handle(Command::Redo).unwrap();
    assert_eq!(e.show.fixtures.len(), 2);
    assert!(e.handle(Command::Redo).is_err());
    e.handle(Command::Undo).unwrap();
    assert!(e.handle(Command::Undo).is_err());
}

#[test]
fn runtime_commands_do_not_create_undo_steps() {
    let mut e = engine();
    add(&mut e, "generic/dimmer", "1ch", 1);
    let base = e.build_rt().undo;
    set(&mut e, 1, "dimmer", 0.5);
    e.handle(Command::GrandMaster { level: 0.5 }).unwrap();
    e.handle(Command::Blackout { on: true }).unwrap();
    e.handle(Command::DmxSet { universe: 1, channel: 3, value: Some(9) }).unwrap();
    e.handle(Command::BlindSet { on: true }).unwrap();
    assert_eq!(e.build_rt().undo, base);
    // failing commands neither
    assert!(e.handle(Command::GoTo { cuelist: 1, number: 1.0 }).is_err());
    assert_eq!(e.build_rt().undo, base);
}

#[test]
fn rapid_saves_of_the_same_object_coalesce() {
    let mut e = engine();
    let pal = |v: f32| Palette { id: 5, name: "P".into(), kind: PaletteKind::Color, values: [("red".to_string(), v)].into(), per_fixture: Default::default() };
    e.handle(Command::PaletteSave { palette: pal(0.1) }).unwrap();
    e.handle(Command::PaletteSave { palette: pal(0.2) }).unwrap();
    e.handle(Command::PaletteSave { palette: pal(0.3) }).unwrap();
    assert_eq!(e.build_rt().undo, 1);
    // a different object is its own step
    let mut p6 = pal(0.9);
    p6.id = 6;
    e.handle(Command::PaletteSave { palette: p6 }).unwrap();
    assert_eq!(e.build_rt().undo, 2);
    e.handle(Command::Undo).unwrap();
    assert_eq!(e.show.palettes.len(), 1);
    assert!((e.show.palettes[0].values["red"] - 0.3).abs() < 1e-6);
    e.handle(Command::Undo).unwrap();
    assert!(e.show.palettes.is_empty());
}

#[test]
fn new_edit_clears_redo_and_show_load_clears_history() {
    let mut e = engine();
    e.handle(Command::GroupSave { group: Group { id: 1, name: "a".into(), fixtures: vec![] } }).unwrap();
    e.handle(Command::GroupRemove { id: 1 }).unwrap();
    e.handle(Command::Undo).unwrap();
    assert_eq!(e.build_rt().redo, 1);
    e.handle(Command::GroupSave { group: Group { id: 2, name: "b".into(), fixtures: vec![] } }).unwrap();
    assert_eq!(e.build_rt().redo, 0);
    e.handle(Command::ShowNew { name: "fresh".into() }).unwrap();
    let rt = e.build_rt();
    assert_eq!((rt.undo, rt.redo), (0, 0));
}

#[test]
fn undo_drops_playback_pointing_at_removed_cues() {
    let mut e = engine();
    add(&mut e, "generic/dimmer", "1ch", 1);
    set(&mut e, 1, "dimmer", 0.4);
    rec(&mut e, true, true);
    set(&mut e, 1, "dimmer", 0.8);
    rec(&mut e, true, true);
    instant(&mut e);
    e.handle(Command::GoTo { cuelist: 1, number: 2.0 }).unwrap();
    assert_eq!(e.runs[&1].current, Some(2.0));
    e.handle(Command::Undo).unwrap(); // removes the second recorded cue
    assert_eq!(e.show.cuelists[0].cues.len(), 1);
    assert!(e.runs.get(&1).is_none());
    e.handle(Command::Redo).unwrap();
    assert_eq!(e.show.cuelists[0].cues.len(), 2);
}

// ───────────── blind ─────────────

#[test]
fn blind_hides_the_programmer_but_keeps_editing() {
    let mut e = engine();
    add(&mut e, "generic/rgb-par", "3ch", 1);
    set(&mut e, 1, "red", 1.0);
    assert_eq!(frame(&mut e)[0], 255);
    e.handle(Command::BlindSet { on: true }).unwrap();
    assert!(e.build_rt().blind);
    assert_eq!(frame(&mut e)[0], 0);
    set(&mut e, 1, "green", 1.0);
    assert_eq!(frame(&mut e)[1], 0);
    // recording while blind works
    rec(&mut e, true, false);
    assert_eq!(e.show.cuelists[0].cues[0].changes[&1]["green"], 1.0);
    // a snapshot of the output excludes the blind programmer
    assert!(e.handle(Command::SceneRecord { id: None, name: "x".into(), from_output: true, fixtures: None, merge: false, refs: true }).is_err());
    e.handle(Command::BlindSet { on: false }).unwrap();
    assert_eq!(frame(&mut e)[0], 255);
}

// ───────────── cue update modes ─────────────

fn dimmer_cues(e: &mut Engine, vals: &[f32]) {
    for (i, v) in vals.iter().enumerate() {
        save_cue(e, 1, cue_with(i as f64 + 1.0, &[(1, "dimmer", *v)]));
    }
}

fn cue_dimmer(e: &Engine, n: usize) -> Option<f32> {
    e.show.cuelists[0].cues[n].changes.get(&1).and_then(|m| m.get("dimmer")).copied()
}

#[test]
fn update_merge_touches_only_the_cue() {
    let mut e = engine();
    add(&mut e, "generic/dimmer", "1ch", 1);
    dimmer_cues(&mut e, &[0.2, 0.2, 0.9]);
    set(&mut e, 1, "dimmer", 0.5);
    e.handle(Command::CueUpdate { cuelist: 1, number: 1.0, mode: UpdateMode::Merge, refs: true }).unwrap();
    assert_eq!(cue_dimmer(&e, 0), Some(0.5));
    assert_eq!(cue_dimmer(&e, 1), Some(0.2));
    assert_eq!(cue_dimmer(&e, 2), Some(0.9));
}

#[test]
fn update_track_forward_follows_restated_values_until_a_move() {
    let mut e = engine();
    add(&mut e, "generic/dimmer", "1ch", 1);
    dimmer_cues(&mut e, &[0.2, 0.2, 0.2, 0.9, 0.2]);
    set(&mut e, 1, "dimmer", 0.5);
    e.handle(Command::CueUpdate { cuelist: 1, number: 1.0, mode: UpdateMode::TrackForward, refs: true }).unwrap();
    assert_eq!(
        (0..5).map(|i| cue_dimmer(&e, i)).collect::<Vec<_>>(),
        vec![Some(0.5), Some(0.5), Some(0.5), Some(0.9), Some(0.2)]
    );
}

#[test]
fn update_cue_only_keeps_the_following_look() {
    let mut e = engine();
    add(&mut e, "generic/dimmer", "1ch", 2);
    save_cue(&mut e, 1, cue_with(1.0, &[(1, "dimmer", 0.2)]));
    save_cue(&mut e, 1, cue_with(2.0, &[(2, "dimmer", 1.0)])); // does not restate fixture 1
    set(&mut e, 1, "dimmer", 0.7);
    e.handle(Command::CueUpdate { cuelist: 1, number: 1.0, mode: UpdateMode::CueOnly, refs: true }).unwrap();
    assert_eq!(cue_dimmer(&e, 0), Some(0.7));
    // cue 2 now states the old value so its resulting look is unchanged
    assert_eq!(cue_dimmer(&e, 1), Some(0.2));
    let st = tracked_state(&e.show.cuelists[0], 1);
    assert_eq!(st[&1]["dimmer"], 0.2);
}

#[test]
fn update_replace_swaps_the_contents() {
    let mut e = engine();
    add(&mut e, "generic/dimmer", "1ch", 2);
    save_cue(&mut e, 1, cue_with(1.0, &[(1, "dimmer", 0.2), (2, "dimmer", 0.3)]));
    set(&mut e, 1, "dimmer", 0.9);
    e.handle(Command::CueUpdate { cuelist: 1, number: 1.0, mode: UpdateMode::Replace, refs: false }).unwrap();
    let c = &e.show.cuelists[0].cues[0];
    assert_eq!(c.changes.len(), 1);
    assert_eq!(c.changes[&1]["dimmer"], 0.9);
    assert!(e.handle(Command::CueUpdate { cuelist: 1, number: 7.0, mode: UpdateMode::Merge, refs: true }).is_err());
    e.handle(Command::ProgRelease { fixtures: None, attrs: None }).unwrap();
    assert!(e.handle(Command::CueUpdate { cuelist: 1, number: 1.0, mode: UpdateMode::Merge, refs: true }).is_err());
}

#[test]
fn cue_load_puts_the_tracked_look_in_the_programmer() {
    let mut e = engine();
    add(&mut e, "generic/rgb-par", "3ch", 1);
    save_cue(&mut e, 1, cue_with(1.0, &[(1, "red", 0.3)]));
    save_cue(&mut e, 1, cue_with(2.0, &[(1, "green", 0.6)]));
    e.handle(Command::CueLoad { cuelist: 1, number: 2.0 }).unwrap();
    assert_eq!(e.programmer[&1]["red"], 0.3);
    assert_eq!(e.programmer[&1]["green"], 0.6);
}

// ───────────── park ─────────────

#[test]
fn park_and_unpark() {
    let mut e = engine();
    add(&mut e, "generic/dimmer", "1ch", 1);
    e.handle(Command::DmxSet { universe: 1, channel: 10, value: Some(77) }).unwrap();
    e.handle(Command::DmxSet { universe: 1, channel: 11, value: Some(88) }).unwrap();
    let rt = e.build_rt();
    assert_eq!(rt.parked.len(), 2);
    assert_eq!((rt.parked[0].universe, rt.parked[0].channel, rt.parked[0].value), (1, 10, 77));
    assert_eq!(frame(&mut e)[9], 77);
    e.handle(Command::Unpark { universe: Some(1), channel: Some(10) }).unwrap();
    assert_eq!(e.build_rt().parked.len(), 1);
    assert_eq!(frame(&mut e)[9], 0);
    e.handle(Command::Unpark { universe: None, channel: None }).unwrap();
    assert!(e.build_rt().parked.is_empty());
}

// ───────────── cue timing, curves, stagger ─────────────

fn two_cue_rgb(e: &mut Engine, n: u32) {
    add(e, "generic/rgb-par", "4ch", n);
    for f in 1..=n {
        save_cue(e, 1, cue_with(1.0, &[(f, "dimmer", 0.0), (f, "red", 0.0)]));
        save_cue(e, 1, cue_with(2.0, &[(f, "dimmer", 1.0), (f, "red", 1.0)]));
    }
    for c in e.show.cuelists[0].cues.iter_mut() {
        c.fade_in = 4.0;
        c.fade_out = 4.0;
    }
}

fn settle_first_cue_then_go_second(e: &mut Engine) -> f64 {
    e.handle(Command::GoTo { cuelist: 1, number: 1.0 }).unwrap();
    let s = e.runs[&1].start;
    e.update_playback(s + 10.0);
    e.handle(Command::Go { cuelist: 1 }).unwrap();
    e.runs[&1].start
}

#[test]
fn group_timing_overrides_cue_fade() {
    let mut e = engine();
    two_cue_rgb(&mut e, 1);
    e.show.cuelists[0].cues[1].timing.insert("color".into(), AttrTiming { delay: 1.0, fade: 2.0 });
    let start = settle_first_cue_then_go_second(&mut e);
    e.update_playback(start + 2.0);
    assert!((run_vals(&e, 1, 1, "dimmer") - 0.5).abs() < 1e-4); // cue fade 4 s
    assert!((run_vals(&e, 1, 1, "red") - 0.5).abs() < 1e-4); // delay 1 + fade 2 → (2-1)/2
    e.update_playback(start + 3.0);
    assert_eq!(run_vals(&e, 1, 1, "red"), 1.0);
    assert!((run_vals(&e, 1, 1, "dimmer") - 0.75).abs() < 1e-4);
    // overall progress is the earliest start to the latest end (0..4 s)
    assert!((e.runs[&1].progress - 0.75).abs() < 1e-4);
    e.update_playback(start + 4.1);
    assert!(e.runs[&1].settled);
}

#[test]
fn curves_shape_the_fade() {
    let mut e = engine();
    two_cue_rgb(&mut e, 1);
    e.show.cuelists[0].cues[1].curve = FadeCurve::Square;
    let start = settle_first_cue_then_go_second(&mut e);
    e.update_playback(start + 2.0);
    assert!((run_vals(&e, 1, 1, "dimmer") - 0.25).abs() < 1e-4);
    assert!((curve_apply(FadeCurve::SCurve, 0.5) - 0.5).abs() < 1e-6);
    assert!((curve_apply(FadeCurve::InvSquare, 0.5) - 0.75).abs() < 1e-6);
    assert!((curve_apply(FadeCurve::Sine, 0.5) - 0.5).abs() < 1e-6);
    assert_eq!(curve_apply(FadeCurve::Sine, 1.0), 1.0);
    assert_eq!(curve_apply(FadeCurve::Linear, 0.3), 0.3);
}

#[test]
fn stagger_delays_each_fixture_in_order() {
    let mut e = engine();
    two_cue_rgb(&mut e, 3);
    e.show.cuelists[0].cues[1].fade_in = 2.0;
    e.show.cuelists[0].cues[1].fade_out = 2.0;
    e.show.cuelists[0].cues[1].stagger = 1.0;
    let start = settle_first_cue_then_go_second(&mut e);
    e.update_playback(start + 1.0);
    assert!((run_vals(&e, 1, 1, "dimmer") - 0.5).abs() < 1e-4);
    assert!(run_vals(&e, 1, 2, "dimmer").abs() < 1e-4); // starts at 1 s
    assert!(run_vals(&e, 1, 3, "dimmer").abs() < 1e-4); // starts at 2 s
    e.update_playback(start + 3.0);
    assert_eq!(run_vals(&e, 1, 1, "dimmer"), 1.0);
    assert_eq!(run_vals(&e, 1, 2, "dimmer"), 1.0);
    assert!((run_vals(&e, 1, 3, "dimmer") - 0.5).abs() < 1e-4);
    // span: 0 … 2 s stagger + 2 s fade
    assert!((e.runs[&1].progress - 0.75).abs() < 1e-4);
}

#[test]
fn untimed_cues_keep_legacy_progress() {
    let mut e = engine();
    two_cue_rgb(&mut e, 1);
    e.show.cuelists[0].cues[1].delay = 1.0;
    let start = settle_first_cue_then_go_second(&mut e);
    e.update_playback(start + 1.0);
    assert!(run_vals(&e, 1, 1, "dimmer").abs() < 1e-6);
    assert!(e.runs[&1].progress.abs() < 1e-6);
    e.update_playback(start + 3.0); // 2 s into a 4 s fade after the 1 s delay
    assert!((e.runs[&1].progress - 0.5).abs() < 1e-4);
}

// ───────────── auto-mark ─────────────

fn mover_pair(e: &mut Engine, auto_mark: bool) {
    add(e, "generic/mh-spot", "16ch", 2);
    save_cue(e, 1, cue_with(1.0, &[(1, "dimmer", 1.0), (1, "pan", 0.1), (2, "dimmer", 0.0), (2, "pan", 0.2)]));
    save_cue(e, 1, cue_with(2.0, &[(1, "dimmer", 1.0), (1, "pan", 0.9), (2, "dimmer", 1.0), (2, "pan", 0.8)]));
    e.show.cuelists[0].auto_mark = auto_mark;
    for c in e.show.cuelists[0].cues.iter_mut() {
        c.fade_in = 0.0;
        c.fade_out = 0.0;
    }
}

#[test]
fn auto_mark_preposition_dark_fixtures_only() {
    let mut e = engine();
    mover_pair(&mut e, true);
    e.handle(Command::GoTo { cuelist: 1, number: 1.0 }).unwrap();
    let s = e.runs[&1].start;
    e.update_playback(s + 5.0); // marks fade in ≥ 1 s
    // dark mover 2 is already at the next cue's position, lit mover 1 is not touched
    assert!((run_vals(&e, 1, 2, "pan") - 0.8).abs() < 1e-4);
    assert_eq!(run_vals(&e, 1, 2, "dimmer"), 0.0);
    assert!((run_vals(&e, 1, 1, "pan") - 0.1).abs() < 1e-4);
    // GO: nothing left to move for mover 2, its light arrives
    e.handle(Command::Go { cuelist: 1 }).unwrap();
    let s = e.runs[&1].start;
    e.update_playback(s + 0.01);
    assert!((run_vals(&e, 1, 2, "pan") - 0.8).abs() < 1e-4);
    assert_eq!(run_vals(&e, 1, 2, "dimmer"), 1.0);
}

#[test]
fn without_auto_mark_dark_fixtures_stay_put() {
    let mut e = engine();
    mover_pair(&mut e, false);
    e.handle(Command::GoTo { cuelist: 1, number: 1.0 }).unwrap();
    let s = e.runs[&1].start;
    e.update_playback(s + 5.0);
    assert!((run_vals(&e, 1, 2, "pan") - 0.2).abs() < 1e-4);
}

#[test]
fn auto_mark_marks_take_time_not_a_snap() {
    let mut e = engine();
    mover_pair(&mut e, true);
    e.handle(Command::GoTo { cuelist: 1, number: 1.0 }).unwrap();
    let s = e.runs[&1].start;
    // marks use at least a 1 s fade; at t = 0 the mover is still at its own cue position (from = default 0.5)
    e.update_playback(s);
    let v = run_vals(&e, 1, 2, "pan");
    assert!((v - 0.8).abs() > 0.05, "{v}");
    e.update_playback(s + 0.5);
    let half = run_vals(&e, 1, 2, "pan");
    e.update_playback(s + 1.0);
    let full = run_vals(&e, 1, 2, "pan");
    assert!(half < full);
    assert!((full - 0.8).abs() < 1e-4);
}

// ───────────── manual crossfade ─────────────

fn three_dimmer_cues(e: &mut Engine) {
    add(e, "generic/dimmer", "1ch", 1);
    dimmer_cues(e, &[0.2, 1.0, 0.0]);
    for c in e.show.cuelists[0].cues.iter_mut() {
        c.fade_in = 5.0;
        c.fade_out = 5.0;
    }
    e.handle(Command::GoTo { cuelist: 1, number: 1.0 }).unwrap();
    let s = e.runs[&1].start;
    e.update_playback(s + 10.0);
}

fn manual(e: &mut Engine, q: f32) {
    e.handle(Command::CuelistManual { cuelist: 1, progress: Some(q) }).unwrap();
    let s = e.runs[&1].start;
    e.update_playback(s + 0.001);
}

#[test]
fn manual_fader_drives_the_fade_and_completes() {
    let mut e = engine();
    three_dimmer_cues(&mut e);
    manual(&mut e, 0.5);
    assert_eq!(e.runs[&1].current, Some(2.0));
    assert!((run_vals(&e, 1, 1, "dimmer") - 0.6).abs() < 1e-4);
    assert!(e.build_rt().cuelists[0].manual);
    assert!((e.build_rt().cuelists[0].progress - 0.5).abs() < 1e-4);
    manual(&mut e, 1.0);
    let s = e.runs[&1].start;
    e.update_playback(s + 0.002);
    assert_eq!(run_vals(&e, 1, 1, "dimmer"), 1.0);
    assert!(!e.build_rt().cuelists[0].manual);
    assert!(e.runs[&1].settled);
    // the fader rests at the top: moving down runs the next cue the other way round
    manual(&mut e, 0.5);
    assert_eq!(e.runs[&1].current, Some(3.0));
    assert!((run_vals(&e, 1, 1, "dimmer") - 0.5).abs() < 1e-4);
    manual(&mut e, 0.0);
    let s = e.runs[&1].start;
    e.update_playback(s + 0.003);
    assert_eq!(run_vals(&e, 1, 1, "dimmer"), 0.0);
    assert_eq!(e.runs[&1].current, Some(3.0));
}

#[test]
fn manual_fader_back_to_origin_cancels() {
    let mut e = engine();
    three_dimmer_cues(&mut e);
    manual(&mut e, 0.4);
    assert_eq!(e.runs[&1].current, Some(2.0));
    manual(&mut e, 0.0);
    assert_eq!(e.runs[&1].current, Some(1.0));
    assert!(!e.build_rt().cuelists[0].manual);
    let s = e.runs[&1].start;
    e.update_playback(s + 0.01);
    assert!((run_vals(&e, 1, 1, "dimmer") - 0.2).abs() < 1e-4);
    // can start again from the same end
    manual(&mut e, 0.5);
    assert_eq!(e.runs[&1].current, Some(2.0));
}

#[test]
fn manual_none_hands_back_to_the_clock_and_go_finishes() {
    let mut e = engine();
    three_dimmer_cues(&mut e);
    manual(&mut e, 0.5);
    e.handle(Command::CuelistManual { cuelist: 1, progress: None }).unwrap();
    assert!(!e.runs[&1].manual.is_some());
    let s = e.runs[&1].start;
    e.update_playback(s + 100.0);
    assert_eq!(run_vals(&e, 1, 1, "dimmer"), 1.0);

    let mut e = engine();
    three_dimmer_cues(&mut e);
    manual(&mut e, 0.5);
    e.handle(Command::Go { cuelist: 1 }).unwrap(); // finishes cue 2, then runs cue 3
    assert_eq!(e.runs[&1].current, Some(3.0));
}

#[test]
fn xfade_executor_wiring() {
    let mut e = engine();
    three_dimmer_cues(&mut e);
    e.handle(Command::ExecSave { executor: Executor { id: 1, page: 0, slot: 0, label: "XF".into(), kind: ExecKind::XFade { cuelist: 1 } } }).unwrap();
    e.handle(Command::ExecLevel { id: 1, level: 0.5 }).unwrap();
    assert_eq!(e.runs[&1].current, Some(2.0));
    let rt = e.build_rt();
    assert!(rt.executors[0].active);
    assert!(rt.cuelists[0].manual);
    e.handle(Command::ExecLevel { id: 1, level: 1.0 }).unwrap();
    let s = e.runs[&1].start;
    e.update_playback(s + 0.01);
    assert_eq!(run_vals(&e, 1, 1, "dimmer"), 1.0);
    assert!(!e.build_rt().executors[0].active);
    e.handle(Command::ExecGo { id: 1 }).unwrap();
    assert_eq!(e.runs[&1].current, Some(3.0));
}
