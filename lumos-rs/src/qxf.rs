//! QLC+ fixture definition (`.qxf`) importer. No 3D data — profiles import with `model3d: None`.

use crate::gdtf::slug;
use crate::model::*;
use roxmltree::{Document, Node};
use std::collections::{HashMap, HashSet};

fn kids<'a, 'b>(n: Node<'a, 'b>, tag: &'static str) -> impl Iterator<Item = Node<'a, 'b>> {
    n.children().filter(move |c| c.is_element() && c.has_tag_name(tag))
}

fn text(n: Node, tag: &str) -> String {
    n.children()
        .find(|c| c.is_element() && c.has_tag_name(tag))
        .and_then(|c| c.text())
        .map(|t| t.trim().to_string())
        .unwrap_or_default()
}

fn fnum(n: Node, k: &str) -> f32 {
    n.attribute(k).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0)
}

struct Def {
    name: String,
    attr: String,
    fine: bool,
    caps: Vec<Capability>,
}

fn color_attr(c: &str) -> String {
    match c.to_lowercase().as_str() {
        "red" => "red",
        "green" => "green",
        "blue" => "blue",
        "white" => "white",
        "amber" => "amber",
        "uv" => "uv",
        "lime" => "lime",
        other => return other.to_string(),
    }
    .to_string()
}

fn map_channel(ch: Node) -> (String, bool) {
    let group_el = ch.children().find(|c| c.is_element() && c.has_tag_name("Group"));
    let group = group_el.and_then(|g| g.text()).unwrap_or("").trim().to_string();
    let fine = group_el.and_then(|g| g.attribute("Byte")) == Some("1");
    let colour = text(ch, "Colour");
    let name = ch.attribute("Name").unwrap_or("").to_lowercase();
    let preset = ch.attribute("Preset").unwrap_or("").to_lowercase();
    let has = |s: &str| name.contains(s) || preset.contains(s);
    let attr = match group.as_str() {
        "Intensity" => {
            if !colour.is_empty() {
                color_attr(&colour)
            } else if has("strobe") || has("shutter") {
                "shutter".into()
            } else {
                "dimmer".into()
            }
        }
        "Pan" => "pan".into(),
        "Tilt" => "tilt".into(),
        "Colour" => {
            if !colour.is_empty() {
                color_attr(&colour)
            } else {
                "colorwheel".into()
            }
        }
        "Gobo" => {
            if has("rotat") || has("index") || has("spin") {
                "gobo_rot".into()
            } else {
                "gobo".into()
            }
        }
        "Shutter" => "shutter".into(),
        "Speed" => "speed".into(),
        "Prism" => {
            if has("rotat") { "prism_rot".into() } else { "prism".into() }
        }
        "Beam" => {
            if has("zoom") {
                "zoom".into()
            } else if has("focus") {
                "focus".into()
            } else if has("iris") {
                "iris".into()
            } else if has("frost") {
                "frost".into()
            } else {
                name.replace(' ', "_")
            }
        }
        "Effect" => "effect".into(),
        "Maintenance" => "control".into(),
        "Nothing" | "" => String::new(),
        other => other.to_lowercase(),
    };
    (attr, fine)
}

fn discrete(attr: &str) -> bool {
    matches!(attr, "colorwheel" | "gobo" | "gobo2" | "prism" | "shutter" | "effect" | "control")
}

fn category(t: &str) -> &'static str {
    match t {
        "Moving Head" | "Scanner" => "spot",
        "Color Changer" => "par",
        "Dimmer" => "dimmer",
        "Strobe" => "strobe",
        "LED Bar (Beams)" | "LED Bar (Pixels)" => "bar",
        "Smoke" | "Hazer" | "Effect" | "Flower" | "Laser" => "effect",
        _ => "other",
    }
}

pub fn import_qxf(xml: &str) -> Result<FixtureProfile, String> {
    let doc = Document::parse(xml.trim_start_matches('\u{feff}')).map_err(|e| format!("QXF: {e}"))?;
    let root = doc
        .descendants()
        .find(|n| n.is_element() && n.has_tag_name("FixtureDefinition"))
        .ok_or("QXF: no FixtureDefinition")?;
    let manufacturer = text(root, "Manufacturer");
    let model = text(root, "Model");
    if model.is_empty() {
        return Err("QXF: missing <Model>".into());
    }
    let ftype = text(root, "Type");

    let mut defs: HashMap<String, Def> = HashMap::new();
    for ch in kids(root, "Channel") {
        let name = ch.attribute("Name").unwrap_or("").to_string();
        let (attr, fine) = map_channel(ch);
        let mut caps = Vec::new();
        if discrete(&attr) {
            for c in kids(ch, "Capability") {
                let label = c.text().unwrap_or("").trim().to_string();
                let color = ["Color", "Res1"].iter().filter_map(|k| c.attribute(*k)).find(|v| v.starts_with('#')).map(|s| s.to_string());
                let (min, max) = (fnum(c, "Min") as u8, fnum(c, "Max") as u8);
                if max >= min {
                    caps.push(Capability { min, max, label, color, image: None, wheel: None, slot: None });
                }
            }
        }
        defs.insert(name.clone(), Def { name, attr, fine, caps });
    }

    let mut modes = Vec::new();
    let (mut beam, mut pan, mut tilt, mut power) = ([25.0f32, 25.0], 0.0f32, 0.0f32, 0.0f32);
    for (mi, mode) in kids(root, "Mode").enumerate() {
        let mut order: Vec<(u16, &str)> = kids(mode, "Channel")
            .filter_map(|c| Some((c.attribute("Number")?.parse::<u16>().ok()?, c.text()?.trim())))
            .collect();
        order.sort_by_key(|o| o.0);
        let mut chans: Vec<ChannelDef> = Vec::new();
        let mut used: HashSet<String> = HashSet::new();
        let mut fines: Vec<(String, u16)> = Vec::new();
        for (num, cname) in &order {
            let Some(d) = defs.get(*cname) else { continue };
            if d.attr.is_empty() {
                continue;
            }
            if d.fine {
                fines.push((d.attr.clone(), *num));
                continue;
            }
            let mut attr = d.attr.clone();
            if attr == "gobo" && used.contains("gobo") {
                attr = "gobo2".into();
            }
            if used.contains(&attr) {
                attr = d.name.to_lowercase().replace(' ', "_");
            }
            let base = attr.clone();
            let mut k = 2;
            while used.contains(&attr) {
                attr = format!("{base}{k}");
                k += 1;
            }
            used.insert(attr.clone());
            let default = if attr == "pan" || attr == "tilt" { 0.5 } else { 0.0 };
            chans.push(ChannelDef { attr, offset: *num, fine: None, default, invert: false, capabilities: d.caps.clone() });
        }
        for (a, off) in fines {
            if let Some(c) = chans.iter_mut().find(|c| c.attr == a) {
                c.fine = Some(off);
            }
        }
        if let Some(ph) = kids(mode, "Physical").next() {
            if mi == 0 {
                if let Some(l) = kids(ph, "Lens").next() {
                    let (a, b) = (fnum(l, "DegreesMin"), fnum(l, "DegreesMax"));
                    if a > 0.0 {
                        beam = [a.min(b.max(a)), b.max(a)];
                    }
                }
                if let Some(f) = kids(ph, "Focus").next() {
                    pan = fnum(f, "PanMax");
                    tilt = fnum(f, "TiltMax");
                }
                if let Some(t) = kids(ph, "Technical").next() {
                    power = fnum(t, "PowerConsumption");
                }
            }
        }
        if !chans.is_empty() {
            modes.push(Mode { name: mode.attribute("Name").unwrap_or("Mode").to_string(), channels: chans });
        }
    }
    if modes.is_empty() {
        return Err("QXF: no mode with channels".into());
    }
    Ok(FixtureProfile {
        id: format!("qxf/{}-{}", slug(&manufacturer), slug(&model)),
        manufacturer: if manufacturer.is_empty() { "Imported".into() } else { manufacturer },
        model,
        category: category(&ftype).into(),
        modes,
        beam_angle: beam,
        pan_range: pan,
        tilt_range: tilt,
        power_w: power,
        source: Some("qxf".into()),
        model3d: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const QXF: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<FixtureDefinition xmlns="http://www.qlcplus.org/FixtureDefinition">
 <Creator><Name>Q Light Controller Plus</Name></Creator>
 <Manufacturer>Acme</Manufacturer>
 <Model>Spot 300</Model>
 <Type>Moving Head</Type>
 <Channel Name="Pan"><Group Byte="0">Pan</Group><Capability Min="0" Max="255">Pan</Capability></Channel>
 <Channel Name="Pan Fine"><Group Byte="1">Pan</Group><Capability Min="0" Max="255">Pan fine</Capability></Channel>
 <Channel Name="Tilt"><Group Byte="0">Tilt</Group><Capability Min="0" Max="255">Tilt</Capability></Channel>
 <Channel Name="Dimmer"><Group Byte="0">Intensity</Group><Capability Min="0" Max="255">Dimmer</Capability></Channel>
 <Channel Name="Red"><Group Byte="0">Intensity</Group><Colour>Red</Colour><Capability Min="0" Max="255">Red</Capability></Channel>
 <Channel Name="Colors"><Group Byte="0">Colour</Group>
   <Capability Min="0" Max="9" Color="#ffffff">Open</Capability>
   <Capability Min="10" Max="19" Color="#ff0000">Red</Capability></Channel>
 <Channel Name="Gobos"><Group Byte="0">Gobo</Group>
   <Capability Min="0" Max="9">Open</Capability><Capability Min="10" Max="19">Dots</Capability></Channel>
 <Channel Name="Zoom"><Group Byte="0">Beam</Group><Capability Min="0" Max="255">Zoom</Capability></Channel>
 <Mode Name="9 channel">
  <Physical>
   <Lens Name="Other" DegreesMin="10" DegreesMax="35"/>
   <Focus Type="Head" PanMax="540" TiltMax="270"/>
   <Technical PowerConsumption="300" DmxConnector="3-pin"/>
  </Physical>
  <Channel Number="0">Pan</Channel><Channel Number="1">Pan Fine</Channel><Channel Number="2">Tilt</Channel>
  <Channel Number="3">Dimmer</Channel><Channel Number="4">Red</Channel><Channel Number="5">Colors</Channel>
  <Channel Number="6">Gobos</Channel><Channel Number="7">Zoom</Channel>
 </Mode>
</FixtureDefinition>"##;

    #[test]
    fn imports_qlcplus_head() {
        let p = import_qxf(QXF).unwrap();
        assert_eq!(p.id, "qxf/acme-spot-300");
        assert_eq!(p.category, "spot");
        assert_eq!(p.source.as_deref(), Some("qxf"));
        assert_eq!((p.pan_range, p.tilt_range, p.power_w), (540.0, 270.0, 300.0));
        assert_eq!(p.beam_angle, [10.0, 35.0]);
        let c = &p.modes[0].channels;
        let a: Vec<_> = c.iter().map(|c| (c.attr.as_str(), c.offset, c.fine)).collect();
        assert_eq!(
            a,
            vec![("pan", 0, Some(1)), ("tilt", 2, None), ("dimmer", 3, None), ("red", 4, None), ("colorwheel", 5, None), ("gobo", 6, None), ("zoom", 7, None)]
        );
        assert_eq!(c[4].capabilities.len(), 2);
        assert_eq!(c[4].capabilities[1].color.as_deref(), Some("#ff0000"));
        assert!(c[3].capabilities.is_empty() && c[0].capabilities.is_empty());
    }

    #[test]
    fn rejects_bad_xml() {
        assert!(import_qxf("<x/>").is_err());
    }
}
