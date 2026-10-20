//! Patch: fixtures → DMX channels through profile modes.

use crate::library::mode_footprint;
use crate::model::*;
use std::collections::HashMap;

pub const COLOR_ATTRS: &[&str] = &["red", "green", "blue", "white", "amber", "uv", "lime"];

pub struct PatchFixture {
    pub id: u32,
    pub universe: u16,
    /// 1-based start address.
    pub address: u16,
    pub chans: Vec<ChannelDef>,
    pub has_dimmer: bool,
    /// Attributes backed by a channel with named capabilities (wheels, shutter…): never cross-fade.
    pub discrete: Vec<String>,
}

impl PatchFixture {
    pub fn has_attr(&self, attr: &str) -> bool {
        self.chans.iter().any(|c| c.attr == attr)
    }
    pub fn is_discrete(&self, attr: &str) -> bool {
        self.discrete.iter().any(|a| a == attr)
    }
}

#[derive(Default)]
pub struct Patch {
    pub fixtures: Vec<PatchFixture>,
    pub by_id: HashMap<u32, usize>,
}

impl Patch {
    pub fn build(show: &Show, profiles: &HashMap<String, FixtureProfile>) -> Patch {
        let mut p = Patch::default();
        for f in &show.fixtures {
            let Some(prof) = profiles.get(&f.profile) else { continue };
            let Some(mode) = prof.modes.iter().find(|m| m.name == f.mode) else { continue };
            let chans = mode.channels.clone();
            let has_dimmer = chans.iter().any(|c| c.attr == "dimmer");
            let discrete = chans
                .iter()
                .filter(|c| !c.capabilities.is_empty())
                .map(|c| c.attr.clone())
                .collect();
            p.by_id.insert(f.id, p.fixtures.len());
            p.fixtures.push(PatchFixture {
                id: f.id,
                universe: f.universe,
                address: f.address,
                chans,
                has_dimmer,
                discrete,
            });
        }
        p
    }

    pub fn get(&self, id: u32) -> Option<&PatchFixture> {
        self.by_id.get(&id).map(|&i| &self.fixtures[i])
    }
}

/// Convert a normalised value to the DMX bytes of one channel definition.
pub fn channel_bytes(def: &ChannelDef, value: f32) -> (u8, Option<u8>) {
    let mut v = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
    if def.invert {
        v = 1.0 - v;
    }
    if def.fine.is_some() {
        let n = (v * 65535.0).round() as u32;
        ((n >> 8) as u8, Some((n & 0xff) as u8))
    } else {
        ((v * 255.0).round() as u8, None)
    }
}

/// Write one fixture into a universe frame. Overlapping fixtures merge HTP per channel.
pub fn write_fixture(frame: &mut [u8; 512], address: u16, chans: &[ChannelDef], values: &Attrs) {
    if address == 0 {
        return;
    }
    let base = address as usize - 1;
    for def in chans {
        let v = values.get(&def.attr).copied().unwrap_or(def.default);
        let (coarse, fine) = channel_bytes(def, v);
        let i = base + def.offset as usize;
        if i < 512 {
            frame[i] = frame[i].max(coarse);
        }
        if let (Some(f), Some(fo)) = (fine, def.fine) {
            let j = base + fo as usize;
            if j < 512 {
                frame[j] = frame[j].max(f);
            }
        }
    }
}

pub fn footprint_of(f: &Fixture, profiles: &HashMap<String, FixtureProfile>) -> u16 {
    profiles
        .get(&f.profile)
        .and_then(|p| p.modes.iter().find(|m| m.name == f.mode))
        .map(mode_footprint)
        .unwrap_or(1)
        .max(1)
}

/// Find the first free run of `fp` channels at/after (`universe`, `from`), spilling into following
/// universes. `used` maps universe → occupancy bitmap and is updated.
pub fn allocate(
    used: &mut HashMap<u16, [bool; 512]>,
    mut universe: u16,
    from: u16,
    fp: u16,
) -> Option<(u16, u16)> {
    let fp = fp as usize;
    if fp == 0 || fp > 512 {
        return None;
    }
    let mut start = (from.max(1) as usize) - 1;
    for _ in 0..256 {
        let occ = used.entry(universe).or_insert([false; 512]);
        let mut a = start;
        while a + fp <= 512 {
            if occ[a..a + fp].iter().all(|b| !b) {
                for b in &mut occ[a..a + fp] {
                    *b = true;
                }
                return Some((universe, a as u16 + 1));
            }
            a += 1;
        }
        universe = universe.checked_add(1)?;
        start = 0;
    }
    None
}

pub fn occupancy(show: &Show, profiles: &HashMap<String, FixtureProfile>) -> HashMap<u16, [bool; 512]> {
    let mut used: HashMap<u16, [bool; 512]> = HashMap::new();
    for f in &show.fixtures {
        let fp = footprint_of(f, profiles) as usize;
        let occ = used.entry(f.universe).or_insert([false; 512]);
        let a = (f.address.max(1) as usize) - 1;
        for b in occ.iter_mut().skip(a).take(fp) {
            *b = true;
        }
    }
    used
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(attr: &str, off: u16, fine: Option<u16>, invert: bool, default: f32) -> ChannelDef {
        ChannelDef { attr: attr.into(), offset: off, fine, default, invert, capabilities: vec![] }
    }

    #[test]
    fn eight_bit() {
        assert_eq!(channel_bytes(&def("dimmer", 0, None, false, 0.0), 1.0).0, 255);
        assert_eq!(channel_bytes(&def("dimmer", 0, None, false, 0.0), 0.5).0, 128);
        assert_eq!(channel_bytes(&def("dimmer", 0, None, false, 0.0), 2.0).0, 255);
        assert_eq!(channel_bytes(&def("dimmer", 0, None, false, 0.0), f32::NAN).0, 0);
    }

    #[test]
    fn sixteen_bit() {
        let d = def("pan", 0, Some(1), false, 0.5);
        assert_eq!(channel_bytes(&d, 1.0), (255, Some(255)));
        assert_eq!(channel_bytes(&d, 0.0), (0, Some(0)));
        let (c, f) = channel_bytes(&d, 0.5);
        assert_eq!(((c as u32) << 8) | f.unwrap() as u32, 32768);
    }

    #[test]
    fn inverted() {
        let d = def("dimmer", 0, None, true, 0.0);
        assert_eq!(channel_bytes(&d, 1.0).0, 0);
        assert_eq!(channel_bytes(&d, 0.0).0, 255);
    }

    #[test]
    fn write_defaults_and_htp() {
        let chans = vec![def("dimmer", 0, None, false, 0.0), def("pan", 1, Some(2), false, 0.5)];
        let mut frame = [0u8; 512];
        let mut v = Attrs::new();
        v.insert("dimmer".into(), 1.0);
        write_fixture(&mut frame, 10, &chans, &v);
        assert_eq!(frame[9], 255);
        assert_eq!(frame[10], 128); // pan default 0.5 → 0x8000
        assert_eq!(frame[11], 0);
        // overlapping fixture with lower value does not lower it
        let mut v2 = Attrs::new();
        v2.insert("dimmer".into(), 0.2);
        write_fixture(&mut frame, 10, &chans, &v2);
        assert_eq!(frame[9], 255);
        // out-of-range address is clipped, not a panic
        write_fixture(&mut frame, 512, &chans, &v);
        assert_eq!(frame[511], 255);
    }

    #[test]
    fn allocation_spills_over() {
        let mut used = HashMap::new();
        assert_eq!(allocate(&mut used, 1, 1, 200), Some((1, 1)));
        assert_eq!(allocate(&mut used, 1, 1, 200), Some((1, 201)));
        assert_eq!(allocate(&mut used, 1, 1, 200), Some((2, 1)));
        assert_eq!(allocate(&mut used, 1, 1, 0), None);
    }
}
