//! Fixture morph (exchange): replace fixtures by another model keeping their ids so cues, scenes,
//! palettes and groups survive, plus the colour-emulation layer that lets one look drive fixtures with
//! different colour systems (RGB / RGBW / RGBA / CMY / colour wheel / dimmer-only).

use super::patch::PatchFixture;
use super::{clamp01, Engine};
use crate::library::mode_footprint;
use crate::model::*;
use std::collections::{BTreeSet, HashMap};

const CMY_SETS: [[&str; 3]; 2] = [["colorsub_c", "colorsub_m", "colorsub_y"], ["cyan", "magenta", "yellow"]];
const COLOUR_ATTRS: &[&str] = &["red", "green", "blue", "white", "amber", "uv", "lime", "cto", "colorwheel"];

/// Amber ≈ (1.0, 0.55, 0.0) in RGB — used both to synthesise amber from RGB and the reverse.
const AMBER_G: f32 = 0.55;

fn cmy_attrs(has: &dyn Fn(&str) -> bool) -> Option<[&'static str; 3]> {
    CMY_SETS.iter().copied().find(|s| s.iter().all(|a| has(a)))
}

fn parse_hex(s: &str) -> Option<[f32; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|v| v as f32 / 255.0);
    Some([p(0)?, p(2)?, p(4)?])
}

fn cap_colour(c: &Capability) -> Option<[f32; 3]> {
    if let Some(col) = c.color.as_deref().and_then(parse_hex) {
        return Some(col);
    }
    let l = c.label.to_lowercase();
    if l.contains("open") || l.contains("white") {
        Some([1.0, 1.0, 1.0])
    } else {
        None
    }
}

/// RGB colour of the wheel slot selected by normalised value `v` (None for rotation/gobo-like slots).
pub fn wheel_rgb(def: &ChannelDef, v: f32) -> Option<[f32; 3]> {
    let x = (clamp01(v) * 255.0).round() as i32;
    def.capabilities.iter().find(|c| x >= c.min as i32 && x <= c.max as i32).and_then(cap_colour)
}

/// Normalised wheel value of the slot whose colour is nearest to `rgb` (hue only: `rgb` is pre-normalised).
fn nearest_slot(def: &ChannelDef, rgb: [f32; 3]) -> Option<f32> {
    let mut best: Option<(f32, f32)> = None;
    for c in &def.capabilities {
        let Some(col) = cap_colour(c) else { continue };
        let d: f32 = (0..3).map(|i| (col[i] - rgb[i]).powi(2)).sum();
        if best.map_or(true, |(bd, _)| d < bd) {
            best = Some((d, (c.min as f32 + c.max as f32) / 2.0 / 255.0));
        }
    }
    best.map(|(_, v)| v)
}

/// Render-time colour emulation for one fixture: derive the attributes the fixture really has from the
/// colour attributes of the look, but only those the look does not already set explicitly.
pub fn emulate(pf: &PatchFixture, a: &mut Attrs) {
    let has = |k: &str| pf.has_attr(k);
    let has_rgb = has("red") || has("green") || has("blue");

    // A. Legacy colour attributes the fixture has no channel for → fold into RGB.
    if has_rgb {
        let mut add = [0f32; 3];
        let mut touched = false;
        if !has("white") {
            if let Some(w) = a.remove("white") {
                add[0] += w;
                add[1] += w;
                add[2] += w;
                touched = true;
            }
        }
        if !has("amber") {
            if let Some(x) = a.remove("amber") {
                add[0] += x;
                add[1] += AMBER_G * x;
                touched = true;
            }
        }
        if !has("lime") {
            if let Some(x) = a.remove("lime") {
                add[1] += x;
                add[0] += 0.5 * x;
                touched = true;
            }
        }
        if !has("uv") {
            if let Some(x) = a.remove("uv") {
                add[2] += 0.6 * x;
                touched = true;
            }
        }
        if touched {
            for (i, k) in ["red", "green", "blue"].iter().enumerate() {
                if has(k) {
                    let v = a.get(*k).copied().unwrap_or(0.0) + add[i];
                    a.insert((*k).to_string(), v.min(1.0));
                }
            }
        }
    }

    // B. RGB sources → this fixture's colour system.
    if !(a.contains_key("red") || a.contains_key("green") || a.contains_key("blue")) {
        return;
    }
    let g = |a: &Attrs, k: &str| a.get(k).copied().unwrap_or(0.0).clamp(0.0, 1.0);
    let (mut r, mut gr, mut b) = (g(a, "red"), g(a, "green"), g(a, "blue"));
    let cmy = cmy_attrs(&has);
    let colour_capable = has_rgb || cmy.is_some() || COLOUR_ATTRS.iter().any(|k| has(k));

    if has_rgb {
        // RGBA: pull amber out of red/green.
        if has("amber") && !a.contains_key("amber") {
            let am = r.min(gr / AMBER_G).clamp(0.0, 1.0);
            if am > 0.0 {
                a.insert("amber".into(), am);
                r -= am;
                gr -= AMBER_G * am;
            }
        }
        // RGBW: white = min(r,g,b), subtracted from the colour channels.
        if has("white") && !a.contains_key("white") {
            let w = r.min(gr).min(b);
            if w > 0.0 {
                a.insert("white".into(), w);
                r -= w;
                gr -= w;
                b -= w;
            }
        }
        for (k, v) in [("red", r), ("green", gr), ("blue", b)] {
            if has(k) && (a.contains_key(k) || v > 0.0) {
                a.insert(k.to_string(), v.max(0.0));
            }
        }
    } else if let Some(c) = cmy {
        for (k, v) in c.iter().zip([1.0 - r, 1.0 - gr, 1.0 - b]) {
            if !a.contains_key(*k) {
                a.insert((*k).to_string(), v);
            }
        }
    } else if let Some(def) = pf.chans.iter().find(|c| c.attr == "colorwheel") {
        if !a.contains_key("colorwheel") {
            let m = r.max(gr).max(b);
            if m > 0.0 {
                if let Some(v) = nearest_slot(def, [r / m, gr / m, b / m]) {
                    a.insert("colorwheel".into(), v);
                }
            }
            if has("dimmer") {
                let d = a.get("dimmer").copied().unwrap_or(1.0);
                a.insert("dimmer".into(), d * m);
            }
        }
    } else if !colour_capable && has("dimmer") {
        let d = a.get("dimmer").copied().unwrap_or(1.0);
        a.insert("dimmer".into(), d * r.max(gr).max(b));
    }
}

// ───────────── morph planning (pure) ─────────────

pub struct MorphPlan {
    pub maps: Vec<MorphMap>,
    pub dropped: Vec<String>,
    pub unset: Vec<String>,
    pub mapped: usize,
    pub total: usize,
}

fn map(from: &str, to: &str, kind: &str, note: &str) -> MorphMap {
    MorphMap { from: from.into(), to: to.into(), kind: kind.into(), note: note.into() }
}

pub fn plan_morph(used: &BTreeSet<String>, target: &Mode) -> MorphPlan {
    let tattrs: BTreeSet<&str> = target.channels.iter().map(|c| c.attr.as_str()).collect();
    let has = |a: &str| tattrs.contains(a);
    let has_rgb = ["red", "green", "blue"].iter().any(|a| has(a));
    let cmy = cmy_attrs(&has);
    let wheel = has("colorwheel");
    let colour_capable = has_rgb || cmy.is_some() || wheel || ["white", "amber", "uv", "lime", "cto"].iter().any(|a| has(a));
    let mut maps: Vec<MorphMap> = vec![];
    let mut dropped = vec![];
    let mut covered: BTreeSet<String> = BTreeSet::new();
    let mut mapped = 0;
    for s in used {
        if has(s) {
            maps.push(map(s, s, "direct", ""));
            covered.insert(s.clone());
            mapped += 1;
            continue;
        }
        let colours_present: Vec<&str> = ["red", "green", "blue", "white", "amber", "uv", "lime"].iter().copied().filter(|a| has(a)).collect();
        let m: Option<MorphMap> = match s.as_str() {
            "red" | "green" | "blue" => {
                let idx = ["red", "green", "blue"].iter().position(|x| x == s).unwrap_or(0);
                if let Some(c) = cmy {
                    Some(map(s, c[idx], "emulated", "RGB → CMY (cyan = 1 − red …)"))
                } else if wheel {
                    Some(map(s, "colorwheel", "approx", "Nearest colour-wheel slot"))
                } else if has("dimmer") && !colour_capable {
                    Some(map(s, "dimmer", "approx", "Brightness only: the dimmer follows the colour level"))
                } else {
                    None
                }
            }
            "white" if has_rgb => Some(map(s, "red+green+blue", "emulated", "White adds equal red, green and blue")),
            "amber" if has_rgb => Some(map(s, "red+green", "approx", "Amber mixed from red and green")),
            "lime" if has_rgb => Some(map(s, "green+red", "approx", "Lime mixed from green and some red")),
            "uv" if has_rgb => Some(map(s, "blue", "approx", "UV approximated with blue")),
            "dimmer" if !colours_present.is_empty() => {
                Some(map(s, &colours_present.join("+"), "emulated", "Virtual dimmer: scales the colour channels"))
            }
            "colorwheel" if has_rgb => Some(map(s, "red+green+blue", "emulated", "Wheel slot converted to its RGB colour")),
            "colorwheel" if cmy.is_some() => {
                Some(map(s, &cmy.unwrap().join("+"), "emulated", "Wheel slot converted to CMY"))
            }
            _ => None,
        };
        match m {
            Some(m) => {
                for t in m.to.split('+') {
                    covered.insert(t.to_string());
                }
                maps.push(m);
                mapped += 1;
            }
            None => dropped.push(s.clone()),
        }
    }
    // RGB sources also feed white/amber LEDs by extraction (see `emulate`).
    if used.iter().any(|a| matches!(a.as_str(), "red" | "green" | "blue")) && has_rgb {
        if has("white") && !used.contains("white") {
            maps.push(map("red+green+blue", "white", "emulated", "White LED takes the common part of red, green and blue"));
            covered.insert("white".into());
        }
        if has("amber") && !used.contains("amber") {
            maps.push(map("red+green", "amber", "approx", "Amber LED takes the orange part of red and green"));
            covered.insert("amber".into());
        }
    }
    let unset: Vec<String> = tattrs.iter().filter(|a| !covered.contains(**a)).map(|a| a.to_string()).collect();
    MorphPlan { maps, dropped, unset, mapped, total: used.len() }
}

// ───────────── engine side ─────────────

impl Engine {
    fn morph_target(&self, profile: &str, mode: &str) -> Result<(FixtureProfile, Mode), String> {
        let prof = self.profiles.get(profile).ok_or_else(|| format!("Unknown profile '{profile}'"))?.clone();
        let m = prof.modes.iter().find(|m| m.name == mode).cloned().ok_or_else(|| format!("Profile has no mode '{mode}'"))?;
        if mode_footprint(&m) == 0 {
            return Err("Mode has no channels".into());
        }
        Ok((prof, m))
    }

    fn morph_check_ids(&self, ids: &[u32]) -> Result<(), String> {
        if ids.is_empty() {
            return Err("Select at least one fixture".into());
        }
        for id in ids {
            if !self.show.fixtures.iter().any(|f| f.id == *id) {
                return Err(format!("Fixture {id} does not exist"));
            }
        }
        Ok(())
    }

    /// Attributes the given fixtures actually use: their mode channels plus anything stored in the show data.
    fn morph_used_attrs(&self, ids: &[u32]) -> BTreeSet<String> {
        let mut used = BTreeSet::new();
        for id in ids {
            let Some(f) = self.show.fixtures.iter().find(|f| f.id == *id) else { continue };
            if let Some(m) = self.profiles.get(&f.profile).and_then(|p| p.modes.iter().find(|m| m.name == f.mode)) {
                used.extend(m.channels.iter().map(|c| c.attr.clone()));
            }
            let mut take = |pf: &PerFixture| {
                if let Some(a) = pf.get(id) {
                    used.extend(a.keys().cloned());
                }
            };
            take(&self.programmer);
            for cl in &self.show.cuelists {
                for c in &cl.cues {
                    take(&c.changes);
                }
            }
            for s in &self.show.scenes {
                take(&s.values);
            }
            for p in &self.show.palettes {
                take(&p.per_fixture);
            }
        }
        used
    }

    pub(super) fn morph_preview(&self, ids: &[u32], profile: &str, mode: &str) -> Result<MorphPreview, String> {
        self.morph_check_ids(ids)?;
        let (_prof, target) = self.morph_target(profile, mode)?;
        let used = self.morph_used_attrs(ids);
        let plan = plan_morph(&used, &target);
        let new_fp = mode_footprint(&target);
        let mut warnings = vec![];

        if plan.dropped.iter().any(|a| a == "pan" || a == "tilt") {
            warnings.push("Pan/tilt are lost: the new fixture cannot move.".to_string());
        }
        if !plan.dropped.is_empty() {
            let rest: Vec<&String> = plan.dropped.iter().filter(|a| *a != "pan" && *a != "tilt").collect();
            if !rest.is_empty() {
                warnings.push(format!("Not available in the new fixture: {}.", rest.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
            }
        }
        // 8/16-bit resolution changes on directly mapped attributes
        let first = self.show.fixtures.iter().find(|f| f.id == ids[0]);
        if let Some(old) = first.and_then(|f| self.profiles.get(&f.profile).and_then(|p| p.modes.iter().find(|m| m.name == f.mode))) {
            let changed: Vec<&str> = plan
                .maps
                .iter()
                .filter(|m| m.kind == "direct")
                .filter_map(|m| {
                    let o = old.channels.iter().find(|c| c.attr == m.from)?;
                    let n = target.channels.iter().find(|c| c.attr == m.to)?;
                    (o.fine.is_some() != n.fine.is_some()).then_some(m.from.as_str())
                })
                .collect();
            if !changed.is_empty() {
                warnings.push(format!("Different resolution (8/16-bit) for: {}.", changed.join(", ")));
            }
        }
        if plan.maps.iter().any(|m| m.kind != "direct") {
            warnings.push("Some colour attributes are emulated; the result may differ slightly from the original look.".to_string());
        }
        // footprint overlap with neighbours
        let old_max = ids
            .iter()
            .filter_map(|id| self.show.fixtures.iter().find(|f| f.id == *id))
            .map(|f| super::patch::footprint_of(f, &self.profiles))
            .max()
            .unwrap_or(0);
        if new_fp > old_max {
            let mut others: HashMap<u16, [bool; 512]> = HashMap::new();
            for f in self.show.fixtures.iter().filter(|f| !ids.contains(&f.id)) {
                let fp = super::patch::footprint_of(f, &self.profiles) as usize;
                let occ = others.entry(f.universe).or_insert([false; 512]);
                for b in occ.iter_mut().skip(f.address.max(1) as usize - 1).take(fp) {
                    *b = true;
                }
            }
            let overlaps = ids.iter().filter_map(|id| self.show.fixtures.iter().find(|f| f.id == *id)).any(|f| {
                let a = f.address.max(1) as usize - 1;
                a + new_fp as usize > 512
                    || others.get(&f.universe).map_or(false, |o| o.iter().skip(a).take(new_fp as usize).any(|b| *b))
            });
            if overlaps {
                warnings.push(format!(
                    "Footprint grows from {old_max} to {new_fp} channels and overlaps neighbouring fixtures — use re-address (repack)."
                ));
            }
        }
        let coverage = if plan.total == 0 { 1.0 } else { plan.mapped as f32 / plan.total as f32 };
        Ok(MorphPreview { coverage, maps: plan.maps, dropped: plan.dropped, unset: plan.unset, warnings, footprint: new_fp })
    }

    pub(super) fn fixture_morph(&mut self, ids: Vec<u32>, profile: String, mode: String, repack: bool) -> Result<(), String> {
        self.morph_check_ids(&ids)?;
        let (_prof, target) = self.morph_target(&profile, &mode)?;
        let fp = mode_footprint(&target);
        if fp > 512 {
            return Err("Mode has too many channels".into());
        }
        // Old colour wheels (slot colours) for data conversion, before anything changes.
        let mut wheels: HashMap<u32, ChannelDef> = HashMap::new();
        for id in &ids {
            if let Some(f) = self.show.fixtures.iter().find(|f| f.id == *id) {
                if let Some(def) = self
                    .profiles
                    .get(&f.profile)
                    .and_then(|p| p.modes.iter().find(|m| m.name == f.mode))
                    .and_then(|m| m.channels.iter().find(|c| c.attr == "colorwheel" && !c.capabilities.is_empty()))
                {
                    wheels.insert(*id, def.clone());
                }
            }
        }
        let tattrs: BTreeSet<&str> = target.channels.iter().map(|c| c.attr.as_str()).collect();
        let new_has_rgb = ["red", "green", "blue"].iter().any(|a| tattrs.contains(a));
        let new_has_cmy = cmy_attrs(&|a| tattrs.contains(a)).is_some();

        let mut placement: HashMap<u32, (u16, u16)> = HashMap::new();
        if repack {
            let first = self.show.fixtures.iter().find(|f| f.id == ids[0]).ok_or("Fixture does not exist")?;
            let (u, start) = (first.universe, first.address.max(1));
            let mut a = start as u32;
            for id in &ids {
                if a + fp as u32 - 1 > 512 {
                    return Err(format!(
                        "The {} fixtures do not fit in universe {u} from address {start} ({fp} channels each)",
                        ids.len()
                    ));
                }
                placement.insert(*id, (u, a as u16));
                a += fp as u32;
            }
        }
        for f in self.show.fixtures.iter_mut().filter(|f| ids.contains(&f.id)) {
            f.profile = profile.clone();
            f.mode = mode.clone();
            if let Some((u, a)) = placement.get(&f.id) {
                f.universe = *u;
                f.address = *a;
            }
        }
        if new_has_rgb || new_has_cmy {
            let conv = |pf: &mut PerFixture| {
                for (id, def) in &wheels {
                    if let Some(attrs) = pf.get_mut(id) {
                        convert_wheel(attrs, def);
                    }
                }
            };
            conv(&mut self.programmer);
            for cl in &mut self.show.cuelists {
                for c in &mut cl.cues {
                    conv(&mut c.changes);
                }
            }
            for s in &mut self.show.scenes {
                conv(&mut s.values);
            }
            for p in &mut self.show.palettes {
                conv(&mut p.per_fixture);
            }
        }
        self.touch_patch();
        Ok(())
    }
}

/// Add red/green/blue derived from a stored colour-wheel slot (the wheel value itself is kept so a
/// morph back restores the original look).
fn convert_wheel(attrs: &mut Attrs, def: &ChannelDef) {
    if attrs.contains_key("red") || attrs.contains_key("green") || attrs.contains_key("blue") {
        return;
    }
    if let Some(v) = attrs.get("colorwheel").copied() {
        if let Some(c) = wheel_rgb(def, v) {
            attrs.insert("red".into(), c[0]);
            attrs.insert("green".into(), c[1]);
            attrs.insert("blue".into(), c[2]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::super::*;
    use super::*;

    fn chan(attr: &str, off: u16) -> ChannelDef {
        ChannelDef { attr: attr.into(), offset: off, fine: None, default: 0.0, invert: false, capabilities: vec![] }
    }

    fn pfx(chans: Vec<ChannelDef>) -> PatchFixture {
        let has_dimmer = chans.iter().any(|c| c.attr == "dimmer");
        PatchFixture { id: 1, universe: 1, address: 1, chans, has_dimmer, discrete: vec![] }
    }

    fn at(pairs: &[(&str, f32)]) -> Attrs {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn rgbw_extracts_white_and_respects_explicit_white() {
        let pf = pfx(vec![chan("red", 0), chan("green", 1), chan("blue", 2), chan("white", 3)]);
        let mut a = at(&[("red", 1.0), ("green", 0.5), ("blue", 0.25)]);
        emulate(&pf, &mut a);
        assert!((a["white"] - 0.25).abs() < 1e-5);
        assert!((a["red"] - 0.75).abs() < 1e-5 && (a["green"] - 0.25).abs() < 1e-5 && a["blue"].abs() < 1e-5);
        // explicit white is never touched
        let mut a = at(&[("red", 1.0), ("green", 1.0), ("blue", 1.0), ("white", 0.0)]);
        emulate(&pf, &mut a);
        assert_eq!((a["red"], a["white"]), (1.0, 0.0));
        // pure white goes entirely to the white LED
        let mut a = at(&[("red", 1.0), ("green", 1.0), ("blue", 1.0)]);
        emulate(&pf, &mut a);
        assert_eq!((a["white"], a["red"]), (1.0, 0.0));
    }

    #[test]
    fn rgba_extracts_amber() {
        let pf = pfx(vec![chan("red", 0), chan("green", 1), chan("blue", 2), chan("amber", 3)]);
        let mut a = at(&[("red", 1.0), ("green", AMBER_G), ("blue", 0.0)]);
        emulate(&pf, &mut a);
        assert!((a["amber"] - 1.0).abs() < 1e-5);
        assert!(a["red"].abs() < 1e-5 && a["green"].abs() < 1e-5);
    }

    #[test]
    fn legacy_white_amber_fold_into_rgb() {
        let pf = pfx(vec![chan("red", 0), chan("green", 1), chan("blue", 2)]);
        let mut a = at(&[("white", 0.5), ("amber", 0.4)]);
        emulate(&pf, &mut a);
        assert!(!a.contains_key("white") && !a.contains_key("amber"));
        assert!((a["red"] - 0.9).abs() < 1e-5);
        assert!((a["green"] - (0.5 + 0.4 * AMBER_G)).abs() < 1e-5);
        assert!((a["blue"] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn cmy_fixture_gets_subtractive_colour() {
        let pf = pfx(vec![chan("dimmer", 0), chan("colorsub_c", 1), chan("colorsub_m", 2), chan("colorsub_y", 3)]);
        let mut a = at(&[("dimmer", 1.0), ("red", 1.0), ("green", 0.25), ("blue", 0.0)]);
        emulate(&pf, &mut a);
        assert_eq!(a["colorsub_c"], 0.0);
        assert!((a["colorsub_m"] - 0.75).abs() < 1e-5);
        assert_eq!(a["colorsub_y"], 1.0);
        // explicit cmy wins
        let mut a = at(&[("red", 1.0), ("colorsub_c", 0.3)]);
        emulate(&pf, &mut a);
        assert_eq!(a["colorsub_c"], 0.3);
        // no rgb → untouched
        let mut a = at(&[("dimmer", 1.0)]);
        emulate(&pf, &mut a);
        assert!(!a.contains_key("colorsub_c"));
    }

    #[test]
    fn colour_wheel_fixture_picks_nearest_slot_and_dims() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 1);
        let pf = e.patch.get(1).unwrap();
        let mut a = at(&[("dimmer", 1.0), ("red", 0.5), ("green", 0.0), ("blue", 0.0)]);
        emulate(pf, &mut a);
        // slot "Red" = 10..19 → centre 14.5/255
        assert!((a["colorwheel"] - 14.5 / 255.0).abs() < 1e-5, "{a:?}");
        assert!((a["dimmer"] - 0.5).abs() < 1e-5, "hue normalised, level goes to the dimmer");
        let mut a = at(&[("dimmer", 1.0), ("red", 0.0), ("green", 0.0), ("blue", 1.0)]);
        emulate(pf, &mut a);
        assert!((a["colorwheel"] - 64.5 / 255.0).abs() < 1e-5);
        let mut a = at(&[("dimmer", 1.0), ("red", 1.0), ("green", 1.0), ("blue", 1.0)]);
        emulate(pf, &mut a);
        assert!((a["colorwheel"] - 4.5 / 255.0).abs() < 1e-5, "white/open slot");
        // explicit wheel is left alone
        let mut a = at(&[("dimmer", 1.0), ("red", 1.0), ("colorwheel", 0.9)]);
        emulate(pf, &mut a);
        assert_eq!(a["colorwheel"], 0.9);
    }

    #[test]
    fn dimmer_only_fixture_follows_colour_level() {
        let pf = pfx(vec![chan("dimmer", 0)]);
        let mut a = at(&[("red", 0.5), ("green", 0.25)]);
        emulate(&pf, &mut a);
        assert!((a["dimmer"] - 0.5).abs() < 1e-5, "dimmer defaults to 1 × luma");
        let mut a = at(&[("dimmer", 0.8), ("blue", 0.5)]);
        emulate(&pf, &mut a);
        assert!((a["dimmer"] - 0.4).abs() < 1e-5);
        let mut a = at(&[("dimmer", 0.8)]);
        emulate(&pf, &mut a);
        assert_eq!(a["dimmer"], 0.8);
    }

    #[test]
    fn native_rgb_fixture_is_unchanged() {
        let pf = pfx(vec![chan("red", 0), chan("green", 1), chan("blue", 2)]);
        let mut a = at(&[("red", 1.0), ("green", 0.2)]);
        let before = a.clone();
        emulate(&pf, &mut a);
        assert_eq!(a, before);
    }

    #[test]
    fn rgbw_fixture_dmx_white_extraction_in_render() {
        let mut e = engine();
        add(&mut e, "generic/rgbw-par", "4ch", 1);
        for a in ["red", "green", "blue"] {
            set(&mut e, 1, a, 1.0);
        }
        let fr = frame(&mut e);
        assert_eq!((fr[0], fr[1], fr[2], fr[3]), (0, 0, 0, 255));
    }

    #[test]
    fn grand_master_scales_emulated_white() {
        let mut e = engine();
        add(&mut e, "generic/rgbw-par", "4ch", 1);
        for a in ["red", "green", "blue"] {
            set(&mut e, 1, a, 1.0);
        }
        e.handle(Command::GrandMaster { level: 0.5 }).unwrap();
        assert_eq!(frame(&mut e)[3], 128);
    }

    #[test]
    fn plan_rgb_to_rgbw_and_mover_to_par() {
        let rgb: BTreeSet<String> = ["red", "green", "blue", "dimmer"].iter().map(|s| s.to_string()).collect();
        let rgbw = Mode { name: "m".into(), channels: vec![chan("red", 0), chan("green", 1), chan("blue", 2), chan("white", 3)] };
        let p = plan_morph(&rgb, &rgbw);
        assert!(p.dropped.is_empty());
        assert_eq!(p.maps.iter().filter(|m| m.kind == "direct").count(), 3);
        assert!(p.maps.iter().any(|m| m.from == "dimmer" && m.kind == "emulated"));
        assert!(p.unset.is_empty(), "white is driven by extraction: {:?}", p.unset);
        assert!(p.maps.iter().any(|m| m.to == "white" && m.kind == "emulated"));
        assert_eq!((p.mapped, p.total), (4, 4));

        let mover: BTreeSet<String> = ["pan", "tilt", "dimmer", "colorwheel", "gobo"].iter().map(|s| s.to_string()).collect();
        let rgb_mode = Mode { name: "m".into(), channels: vec![chan("red", 0), chan("green", 1), chan("blue", 2)] };
        let p = plan_morph(&mover, &rgb_mode);
        assert_eq!(p.dropped, vec!["gobo".to_string(), "pan".to_string(), "tilt".to_string()]);
        assert!(p.maps.iter().any(|m| m.from == "colorwheel" && m.to == "red+green+blue"));
        assert_eq!((p.mapped, p.total), (2, 5));
    }

    #[test]
    fn plan_rgb_to_cmy_wheel_and_dimmer() {
        let rgb: BTreeSet<String> = ["red", "green", "blue"].iter().map(|s| s.to_string()).collect();
        let cmy = Mode { name: "m".into(), channels: vec![chan("dimmer", 0), chan("colorsub_c", 1), chan("colorsub_m", 2), chan("colorsub_y", 3)] };
        let p = plan_morph(&rgb, &cmy);
        assert!(p.dropped.is_empty());
        assert!(p.maps.iter().all(|m| m.kind == "emulated"));
        assert_eq!(p.maps.iter().find(|m| m.from == "red").unwrap().to, "colorsub_c");
        let wheel = Mode { name: "m".into(), channels: vec![chan("dimmer", 0), chan("colorwheel", 1)] };
        assert!(plan_morph(&rgb, &wheel).maps.iter().all(|m| m.to == "colorwheel" && m.kind == "approx"));
        let dim = Mode { name: "m".into(), channels: vec![chan("dimmer", 0)] };
        assert!(plan_morph(&rgb, &dim).maps.iter().all(|m| m.to == "dimmer"));
        let none = Mode { name: "m".into(), channels: vec![chan("zoom", 0)] };
        assert_eq!(plan_morph(&rgb, &none).dropped.len(), 3);
    }

    #[test]
    fn fixture_morph_keeps_looks_and_ids() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 2);
        set(&mut e, 1, "red", 1.0);
        set(&mut e, 1, "green", 1.0);
        set(&mut e, 1, "blue", 1.0);
        e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: "".into(), release: true }).unwrap();
        e.handle(Command::FixtureMorph { ids: vec![1, 2], profile: "generic/rgbw-par".into(), mode: "4ch".into(), repack: false }).unwrap();
        e.rebuild_derived();
        assert_eq!(e.show.fixtures.iter().map(|f| f.id).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(e.show.fixtures[0].profile, "generic/rgbw-par");
        assert_eq!(e.show.cuelists[0].cues[0].changes[&1]["red"], 1.0, "cue data is untouched");
        e.show.cuelists[0].cues[0].fade_in = 0.0;
        e.go(1).unwrap();
        e.update_playback(0.1);
        let fr = frame(&mut e);
        assert_eq!((fr[0], fr[1], fr[2], fr[3]), (0, 0, 0, 255), "same white look, now on the white LED");
    }

    #[test]
    fn fixture_morph_repack_readdresses_consecutively() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 3);
        assert_eq!(e.show.fixtures.iter().map(|f| f.address).collect::<Vec<_>>(), vec![1, 4, 7]);
        e.handle(Command::FixtureMorph { ids: vec![1, 2, 3], profile: "generic/rgbw-par".into(), mode: "4ch".into(), repack: true }).unwrap();
        assert_eq!(e.show.fixtures.iter().map(|f| f.address).collect::<Vec<_>>(), vec![1, 5, 9]);
        // without repack the addresses stay and neighbours may overlap
        let mut e2 = engine();
        add(&mut e2, "generic/rgb-par", "3ch", 3);
        e2.handle(Command::FixtureMorph { ids: vec![1, 2, 3], profile: "generic/rgbw-par".into(), mode: "4ch".into(), repack: false }).unwrap();
        assert_eq!(e2.show.fixtures.iter().map(|f| f.address).collect::<Vec<_>>(), vec![1, 4, 7]);
        // doesn't fit
        let mut e3 = engine();
        add(&mut e3, "generic/rgb-par", "3ch", 2);
        e3.show.fixtures[0].address = 510;
        e3.rebuild_derived();
        assert!(e3.handle(Command::FixtureMorph { ids: vec![1, 2], profile: "generic/rgbw-par".into(), mode: "4ch".into(), repack: true }).is_err());
    }

    #[test]
    fn fixture_morph_errors() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        assert!(e.handle(Command::FixtureMorph { ids: vec![], profile: "generic/rgbw-par".into(), mode: "4ch".into(), repack: false }).is_err());
        assert!(e.handle(Command::FixtureMorph { ids: vec![9], profile: "generic/rgbw-par".into(), mode: "4ch".into(), repack: false }).is_err());
        assert!(e.handle(Command::FixtureMorph { ids: vec![1], profile: "nope".into(), mode: "x".into(), repack: false }).is_err());
        assert!(e.handle(Command::FixtureMorph { ids: vec![1], profile: "generic/rgbw-par".into(), mode: "nope".into(), repack: false }).is_err());
        assert_eq!(e.show.fixtures[0].profile, "generic/rgb-par", "failed morph changes nothing");
    }

    #[test]
    fn colourwheel_data_converts_to_rgb_and_morphing_back_restores() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 1);
        set(&mut e, 1, "dimmer", 1.0);
        set(&mut e, 1, "colorwheel", 64.0 / 255.0); // Blue slot (60..69)
        e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: "".into(), release: false }).unwrap();
        e.handle(Command::SceneRecord { id: None, name: "S".into(), from_output: false, fixtures: None, merge: false }).unwrap();
        e.handle(Command::FixtureMorph { ids: vec![1], profile: "generic/rgb-par-dimstrobe".into(), mode: "6ch".into(), repack: false }).unwrap();
        e.rebuild_derived();
        let cue = &e.show.cuelists[0].cues[0].changes[&1];
        assert_eq!(cue["red"], 0.0, "blue slot colour is #0030ff");
        assert!((cue["green"] - 48.0 / 255.0).abs() < 1e-4);
        assert!((cue["blue"] - 1.0).abs() < 1e-5);
        assert!(cue.contains_key("colorwheel"), "original value kept");
        assert!(e.show.scenes[0].values[&1].contains_key("blue"));
        assert!(e.programmer[&1].contains_key("blue"));
        // back to a wheel fixture: the wheel value still drives it
        e.handle(Command::FixtureMorph { ids: vec![1], profile: "generic/mh-spot".into(), mode: "16ch".into(), repack: false }).unwrap();
        e.rebuild_derived();
        assert!((e.show.cuelists[0].cues[0].changes[&1]["colorwheel"] - 64.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn preview_reports_coverage_dropped_and_warnings() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 2);
        set(&mut e, 1, "dimmer", 1.0);
        let p = e.morph_preview(&[1, 2], "generic/rgb-par", "3ch").unwrap();
        assert!(p.dropped.contains(&"pan".to_string()) && p.dropped.contains(&"gobo".to_string()));
        assert!(p.coverage < 0.5 && p.coverage > 0.0);
        assert_eq!(p.footprint, 3);
        assert!(p.warnings.iter().any(|w| w.contains("Pan/tilt")));
        // growing footprint overlaps the neighbour
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 2);
        let p = e.morph_preview(&[1], "generic/rgbw-par", "4ch").unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("overlaps")), "{:?}", p.warnings);
        assert_eq!(p.coverage, 1.0);
        let j = serde_json::to_value(&p).unwrap();
        assert!(j["maps"].is_array() && j["unset"].is_array() && j["footprint"] == 4);
        assert!(e.morph_preview(&[], "generic/rgbw-par", "4ch").is_err());
        assert!(e.morph_preview(&[1], "generic/rgbw-par", "zz").is_err());
    }

    #[test]
    fn preview_flags_resolution_change() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 1);
        let p = e.morph_preview(&[1], "generic/mh-spot", "12ch").unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("8/16-bit") && w.contains("pan")), "{:?}", p.warnings);
    }
}
