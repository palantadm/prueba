//! Programming helpers: palette links ("preset references"), cue load/update, blind and park.
//!
//! # Palette links
//! The programmer remembers which attributes were applied from a palette (`links`: fixture → attr →
//! `(palette id, value when applied)`). A link is only honoured while the programmer still holds exactly
//! that value, so touching an attribute by hand drops its link without hooking every mutation site.
//! Recording (`ProgRecordCue`, `SceneRecord`, `CueUpdate` with `refs: true`) stores the links in
//! `Cue.refs` / `Scene.refs` next to the absolute value. Editing a palette **writes through**: the
//! stored values of every linked cue/scene entry are overwritten with the palette's current value (the
//! absolute value is therefore also the fallback if the palette is deleted), and running cuelists are
//! refreshed so the change is visible immediately. Because tracking resolves from the stored values at
//! GO time, tracked cues follow palette edits as well.

use super::history::History;
use super::*;

pub type Links = BTreeMap<u32, BTreeMap<String, (u32, f32)>>;

/// Programming state owned by this module (kept in one field of `Engine`).
#[derive(Default)]
pub(super) struct ProgState {
    pub hist: History,
    pub links: Links,
    /// Blind: the programmer is not sent to the output.
    pub blind: bool,
    /// Cuelists whose manual crossfader currently rests at the top end.
    pub xf_top: BTreeSet<u32>,
}

/// State captured before a command runs, needed to finish its bookkeeping afterwards.
pub(super) enum Pre {
    None,
    Cue { cuelist: u32, number: Option<f64>, mode: RecordMode, refs: bool, prog: PerFixture, links: Links },
    Scene { id: Option<u32>, fixtures: Option<Vec<u32>>, merge: bool, refs: bool, from_output: bool, prog: PerFixture, links: Links },
    Palette(u32),
}

pub fn palette_value(pal: &Palette, f: u32, a: &str) -> Option<f32> {
    pal.per_fixture.get(&f).and_then(|m| m.get(a)).or_else(|| pal.values.get(a)).copied()
}

fn valid_link(links: &Links, palettes: &BTreeSet<u32>, f: u32, a: &str, v: f32) -> Option<u32> {
    let (pid, val) = links.get(&f)?.get(a)?;
    ((val - v).abs() < 1e-5 && palettes.contains(pid)).then_some(*pid)
}

fn set_link(refs: &mut PerFixtureRefs, f: u32, a: &str, link: Option<u32>) {
    match link {
        Some(pid) => {
            refs.entry(f).or_default().insert(a.to_string(), pid);
        }
        None => {
            if let Some(m) = refs.get_mut(&f) {
                m.remove(a);
            }
        }
    }
}

impl Engine {
    fn palette_ids(&self) -> BTreeSet<u32> {
        self.show.palettes.iter().map(|p| p.id).collect()
    }

    // ───────────── before / after hooks used by `handle` ─────────────

    pub(super) fn pre_command(&self, cmd: &Command) -> Pre {
        match cmd {
            Command::ProgRecordCue { cuelist, number, mode, refs, .. } => Pre::Cue {
                cuelist: *cuelist,
                number: *number,
                mode: *mode,
                refs: *refs,
                prog: self.programmer.clone(),
                links: self.prog.links.clone(),
            },
            Command::SceneRecord { id, fixtures, merge, refs, from_output, .. } => Pre::Scene {
                id: *id,
                fixtures: fixtures.clone(),
                merge: *merge,
                refs: *refs,
                from_output: *from_output,
                prog: self.programmer.clone(),
                links: self.prog.links.clone(),
            },
            Command::PaletteSave { palette } if palette.id != 0 => Pre::Palette(palette.id),
            Command::PaletteRemove { id } => Pre::Palette(*id),
            Command::ProgRecordPalette { id: Some(id), .. } => Pre::Palette(*id),
            _ => Pre::None,
        }
    }

    pub(super) fn post_command(&mut self, pre: Pre) {
        match pre {
            Pre::None => {}
            Pre::Palette(id) => self.palette_changed(id),
            Pre::Cue { cuelist, number, mode, refs, prog, links } => self.link_cue(cuelist, number, mode, refs, &prog, &links),
            Pre::Scene { id, fixtures, merge, refs, from_output, prog, links } => {
                if !from_output {
                    self.link_scene(id, fixtures, merge, refs, &prog, &links);
                }
            }
        }
    }

    /// Store palette links on the cue that `ProgRecordCue` just wrote.
    fn link_cue(&mut self, cuelist: u32, number: Option<f64>, mode: RecordMode, use_refs: bool, prog: &PerFixture, links: &Links) {
        let palettes = self.palette_ids();
        let Some(cl) = self.show.cuelists.iter_mut().find(|c| c.id == cuelist) else { return };
        let idx = match number {
            Some(n) => index_of(cl, n),
            None => cl.cues.len().checked_sub(1),
        };
        let Some(idx) = idx else { return };
        let cue = &mut cl.cues[idx];
        if mode == RecordMode::Replace {
            cue.refs.clear();
        }
        for (f, m) in prog {
            for (a, v) in m {
                let link = if use_refs { valid_link(links, &palettes, *f, a, *v) } else { None };
                set_link(&mut cue.refs, *f, a, link);
            }
        }
        cue.refs.retain(|_, m| !m.is_empty());
    }

    fn link_scene(&mut self, id: Option<u32>, fixtures: Option<Vec<u32>>, merge: bool, use_refs: bool, prog: &PerFixture, links: &Links) {
        let palettes = self.palette_ids();
        let known: BTreeSet<u32> = self.show.fixtures.iter().map(|f| f.id).collect();
        let sid = id.or_else(|| self.show.scenes.iter().map(|s| s.id).max());
        let Some(sc) = sid.and_then(|sid| self.show.scenes.iter_mut().find(|s| s.id == sid)) else { return };
        if !merge {
            sc.refs.clear();
        }
        for (f, m) in prog {
            if !known.contains(f) || fixtures.as_ref().map_or(false, |w| !w.contains(f)) {
                continue;
            }
            for (a, v) in m {
                let link = if use_refs { valid_link(links, &palettes, *f, a, *v) } else { None };
                set_link(&mut sc.refs, *f, a, link);
            }
        }
        sc.refs.retain(|_, m| !m.is_empty());
    }

    // ───────────── palette edits write through ─────────────

    /// A palette was saved, re-recorded or removed: update every linked value (or drop the links).
    pub(super) fn palette_changed(&mut self, id: u32) {
        let pal = self.show.palettes.iter().find(|p| p.id == id).cloned();
        let mut changed = false;
        for cl in &mut self.show.cuelists {
            for cue in &mut cl.cues {
                match &pal {
                    Some(p) => {
                        for (f, m) in &cue.refs {
                            for (a, pid) in m {
                                if *pid != id {
                                    continue;
                                }
                                if let Some(v) = palette_value(p, *f, a) {
                                    let v = clamp01(v);
                                    let slot = cue.changes.entry(*f).or_default().entry(a.clone()).or_insert(v);
                                    if (*slot - v).abs() > 1e-6 {
                                        *slot = v;
                                        changed = true;
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        for m in cue.refs.values_mut() {
                            m.retain(|_, pid| *pid != id);
                        }
                        cue.refs.retain(|_, m| !m.is_empty());
                    }
                }
            }
        }
        for sc in &mut self.show.scenes {
            match &pal {
                Some(p) => {
                    for (f, m) in &sc.refs {
                        for (a, pid) in m {
                            if *pid != id {
                                continue;
                            }
                            if let Some(v) = palette_value(p, *f, a) {
                                let v = clamp01(v);
                                let slot = sc.values.entry(*f).or_default().entry(a.clone()).or_insert(v);
                                if (*slot - v).abs() > 1e-6 {
                                    *slot = v;
                                    changed = true;
                                }
                            }
                        }
                    }
                }
                None => {
                    for m in sc.refs.values_mut() {
                        m.retain(|_, pid| *pid != id);
                    }
                    sc.refs.retain(|_, m| !m.is_empty());
                }
            }
        }
        if pal.is_none() {
            for m in self.prog.links.values_mut() {
                m.retain(|_, (pid, _)| *pid != id);
            }
        }
        if changed {
            self.refresh_runs();
            self.touch();
        }
    }

    /// Re-resolve the target of every playing cue from the stored values (after palette edits / undo).
    pub(super) fn refresh_runs(&mut self) {
        let mut any = false;
        for (id, r) in self.runs.iter_mut() {
            if r.releasing {
                continue;
            }
            let Some(cl) = self.show.cuelists.iter().find(|c| c.id == *id) else { continue };
            let Some(idx) = r.current.and_then(|n| index_of(cl, n)) else { continue };
            for (f, m) in tracked_state(cl, idx) {
                for (a, v) in m {
                    let Some(t) = r.to.get_mut(&f).and_then(|x| x.get_mut(&a)) else { continue };
                    if (*t - v).abs() <= 1e-6 {
                        continue;
                    }
                    *t = v;
                    any = true;
                    if r.settled {
                        if let Some(x) = r.values.get_mut(&f).and_then(|x| x.get_mut(&a)) {
                            *x = v;
                        }
                    }
                }
            }
        }
        if any {
            self.dirty = true;
        }
    }

    // ───────────── cue load / update ─────────────

    /// Load the cue's resulting look into the programmer (palette links preserved).
    pub(super) fn cue_load(&mut self, cuelist: u32, number: f64) -> Result<(), String> {
        let cl = self.require_cuelist(cuelist)?;
        let idx = index_of(cl, number).ok_or_else(|| format!("Cue {number} does not exist"))?;
        let state = tracked_state(cl, idx);
        let start = if cl.tracking { (0..=idx).rev().find(|&i| cl.cues[i].block).unwrap_or(0) } else { idx };
        let mut links = Links::new();
        for c in &cl.cues[start..=idx] {
            for (f, m) in &c.changes {
                for (a, v) in m {
                    match c.refs.get(f).and_then(|r| r.get(a)) {
                        Some(pid) => {
                            links.entry(*f).or_default().insert(a.clone(), (*pid, *v));
                        }
                        None => {
                            if let Some(x) = links.get_mut(f) {
                                x.remove(a);
                            }
                        }
                    }
                }
            }
        }
        // the link's reference value is the value now in the programmer
        for (f, m) in links.iter_mut() {
            for (a, (_, v)) in m.iter_mut() {
                if let Some(cur) = state.get(f).and_then(|x| x.get(a)) {
                    *v = *cur;
                }
            }
        }
        self.programmer = state;
        self.prog.links = links;
        self.dirty = true;
        Ok(())
    }

    /// Write the programmer into an existing cue.
    pub(super) fn cue_update(&mut self, cuelist: u32, number: f64, mode: UpdateMode, use_refs: bool) -> Result<(), String> {
        if self.programmer.is_empty() {
            return Err("Programmer is empty — nothing to update".into());
        }
        let prog = self.programmer.clone();
        let links = self.prog.links.clone();
        let palettes = self.palette_ids();
        let cl = self.show.cuelists.iter_mut().find(|c| c.id == cuelist).ok_or("Cuelist does not exist")?;
        let idx = index_of(cl, number).ok_or_else(|| format!("Cue {number} does not exist"))?;
        let old_state = tracked_state(cl, idx);
        let tracking = cl.tracking;

        {
            let cue = &mut cl.cues[idx];
            if mode == UpdateMode::Replace {
                cue.changes = prog.clone();
                cue.refs.clear();
            }
            for (f, m) in &prog {
                for (a, v) in m {
                    if mode != UpdateMode::Replace {
                        cue.changes.entry(*f).or_default().insert(a.clone(), *v);
                    }
                    let link = if use_refs { valid_link(&links, &palettes, *f, a, *v) } else { None };
                    set_link(&mut cue.refs, *f, a, link);
                }
            }
            cue.refs.retain(|_, m| !m.is_empty());
        }

        match mode {
            UpdateMode::TrackForward => {
                for (f, m) in &prog {
                    for (a, v) in m {
                        let old = old_state.get(f).and_then(|x| x.get(a)).copied();
                        let link = if use_refs { valid_link(&links, &palettes, *f, a, *v) } else { None };
                        for j in idx + 1..cl.cues.len() {
                            if tracking && cl.cues[j].block {
                                break;
                            }
                            let cue = &mut cl.cues[j];
                            match cue.changes.get_mut(f).and_then(|x| x.get_mut(a)) {
                                Some(x) => match old {
                                    Some(o) if (*x - o).abs() < 1e-4 => {
                                        *x = *v;
                                        set_link(&mut cue.refs, *f, a, link);
                                    }
                                    _ => break,
                                },
                                None => continue,
                            }
                        }
                    }
                }
            }
            UpdateMode::CueOnly if tracking => {
                if let Some(next) = cl.cues.get_mut(idx + 1) {
                    if !next.block {
                        for (f, m) in &prog {
                            for a in m.keys() {
                                if next.changes.get(f).map_or(false, |x| x.contains_key(a)) {
                                    continue;
                                }
                                let old = old_state
                                    .get(f)
                                    .and_then(|x| x.get(a))
                                    .copied()
                                    .unwrap_or_else(|| if a == "dimmer" { 0.0 } else { attr_default(a) });
                                next.changes.entry(*f).or_default().insert(a.clone(), old);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        self.after_cuelist_edit(cuelist);
        Ok(())
    }

    // ───────────── park ─────────────

    pub(super) fn unpark(&mut self, universe: Option<u16>, channel: Option<u16>) {
        self.dmx_override.retain(|(u, c), _| !(universe.map_or(true, |x| x == *u) && channel.map_or(true, |x| x == *c)));
        self.dirty = true;
    }

    pub(super) fn parked_rt(&self) -> Vec<ParkedChannel> {
        self.dmx_override.iter().map(|((u, c), v)| ParkedChannel { universe: *u, channel: *c, value: *v }).collect()
    }
}
