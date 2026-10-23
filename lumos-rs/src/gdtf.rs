//! GDTF (DIN SPEC 15800) importer: `.gdtf` ZIP → [`FixtureProfile`] with real geometry.
//!
//! # Coordinate convention
//! GDTF is right-handed **Z-up**, metres, fixtures drawn *hanging* (base plate at the origin,
//! device hangs below, pan axis = Z, tilt axis = X, beam emits into −Z). Lumos/three.js is
//! **Y-up**. Every point/vector is mapped with `C: (x, y, z) → (x, z, −y)` (a −90° rotation
//! about X, determinant +1) and each node matrix `M` becomes `C·M·C⁻¹`, then is written
//! column-major (`Matrix4.fromArray`). Consequences in Lumos space: the device hangs below its
//! origin (child offsets have negative `y`), the beam shines along local **−Y**, pan rotates
//! about **+Y**, tilt about **+X**. `.3ds` meshes are converted to `.glb` with the same mapping
//! (see `glb.rs`), so `GeoNode.model` always points to a Y-up GLB — except GDTF-supplied
//! `.glb` files which are passed through untouched.
//!
//! `GeoNode.size` = `[Length (x), Height (y), Width (z)]` in Lumos axes.

use crate::model::*;
use roxmltree::{Document, Node};
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

pub struct Imported {
    pub profile: FixtureProfile,
    /// `(path relative to the assets dir, bytes)` to be written under `<data>/library/<asset_dir>/`.
    pub files: Vec<(String, Vec<u8>)>,
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Original `.gdtf` bytes kept next to the extracted assets (`<data>/library/<asset_dir>/source.gdtf`).
pub const SOURCE_FILE: &str = "source.gdtf";

/// Directory name (inside `<data>/library`) for a profile id.
pub fn asset_dir(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

// ───────────── ZIP ─────────────

type Pack = HashMap<String, (String, Vec<u8>)>; // lower-case path → (original path, bytes)

const MAX_ENTRY: u64 = 64 * 1024 * 1024;

fn load_pack(bytes: &[u8]) -> Result<(String, Pack), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("Not a valid GDTF (zip): {e}"))?;
    let mut xml = None;
    let mut pack: Pack = HashMap::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let Some(path) = f.enclosed_name() else { continue };
        let name = path.to_string_lossy().replace('\\', "/");
        let lower = name.to_lowercase();
        let wanted = lower == "description.xml"
            || ((lower.starts_with("models/gltf/") || lower.starts_with("models/3ds/") || lower.starts_with("wheels/"))
                && !lower.contains("_high"));
        if !wanted {
            continue;
        }
        if f.size() > MAX_ENTRY {
            continue;
        }
        total += f.size();
        if total > 256 * 1024 * 1024 {
            return Err("GDTF too large".into());
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.take(MAX_ENTRY).read_to_end(&mut buf).map_err(|e| e.to_string())?;
        if lower == "description.xml" {
            let s = String::from_utf8_lossy(&buf).into_owned();
            xml = Some(s.trim_start_matches('\u{feff}').to_string());
        } else {
            pack.insert(lower, (name, buf));
        }
    }
    Ok((xml.ok_or("GDTF has no description.xml")?, pack))
}

// ───────────── small parsing helpers ─────────────

fn fa(n: &Node, k: &str, d: f32) -> f32 {
    n.attribute(k).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(d)
}

fn child<'a, 'b>(n: Node<'a, 'b>, tag: &str) -> Option<Node<'a, 'b>> {
    n.children().find(|c| c.is_element() && c.has_tag_name(tag))
}

fn children<'a, 'b>(n: Node<'a, 'b>, tag: &'static str) -> impl Iterator<Item = Node<'a, 'b>> {
    n.children().filter(move |c| c.is_element() && c.has_tag_name(tag))
}

/// `"value/bytes"` → (value, bytes). Plain integers have `bytes = 1`.
fn dmx_val(s: &str) -> (f64, u32) {
    let mut it = s.trim().split('/');
    let v = it.next().and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0);
    let n = it.next().and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(1).clamp(1, 4);
    (v, n)
}

fn max_of(n: u32) -> f64 {
    256f64.powi(n as i32) - 1.0
}

/// CIE xyY (`"x,y,Y"`, Y in %) → `#rrggbb`, normalised so the brightest channel is 1
/// (a swatch of the filter's hue, not its transmission).
pub fn xyy_to_hex(s: &str) -> Option<String> {
    let v: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if v.len() < 2 || v[1].abs() < 1e-6 {
        return None;
    }
    let (x, y) = (v[0], v[1]);
    let yy = if v.len() > 2 { (v[2] / 100.0).max(1e-4) } else { 1.0 };
    let (xx, zz) = (x * yy / y, (1.0 - x - y) * yy / y);
    let r = 3.2406 * xx - 1.5372 * yy - 0.4986 * zz;
    let g = -0.9689 * xx + 1.8758 * yy + 0.0415 * zz;
    let b = 0.0557 * xx - 0.2040 * yy + 1.0570 * zz;
    let (r, g, b) = (r.max(0.0), g.max(0.0), b.max(0.0));
    let m = r.max(g).max(b);
    if m <= 0.0 {
        return None;
    }
    let enc = |c: f32| {
        let c = (c / m).clamp(0.0, 1.0);
        let c = if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
        (c.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    Some(format!("#{:02x}{:02x}{:02x}", enc(r), enc(g), enc(b)))
}

type Mat = [[f32; 4]; 4];

/// `"{a,b,c,d}{e,f,g,h}{i,j,k,l}{0,0,0,1}"` (row-major, translation in the last column).
fn parse_matrix(s: &str) -> Mat {
    let mut m: Mat = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]];
    let rows: Vec<Vec<f32>> = s
        .split('}')
        .map(|r| r.trim_start_matches(|c| c == '{' || c == ' ').split(',').filter_map(|v| v.trim().parse().ok()).collect::<Vec<f32>>())
        .filter(|r: &Vec<f32>| r.len() == 4)
        .collect();
    if rows.len() >= 3 {
        for (i, r) in rows.iter().take(4).enumerate() {
            m[i].copy_from_slice(&r[..4]);
        }
    }
    m
}

const C: Mat = [[1., 0., 0., 0.], [0., 0., 1., 0.], [0., -1., 0., 0.], [0., 0., 0., 1.]];

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut o = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            o[i][j] = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}

fn transpose(a: &Mat) -> Mat {
    let mut o = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            o[i][j] = a[j][i];
        }
    }
    o
}

/// GDTF row-major Z-up matrix → Lumos column-major Y-up.
pub fn to_lumos_matrix(m: &Mat) -> [f32; 16] {
    let c = mul(&mul(&C, m), &transpose(&C));
    let mut o = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            o[col * 4 + row] = c[row][col];
        }
    }
    o
}

// ───────────── attribute vocabulary ─────────────

/// GDTF attribute name → Lumos attribute name.
pub fn map_attr(a: &str) -> String {
    match a {
        "Dimmer" => return "dimmer".into(),
        "Pan" => return "pan".into(),
        "Tilt" => return "tilt".into(),
        "ColorAdd_R" | "ColorRGB_Red" => return "red".into(),
        "ColorAdd_G" | "ColorRGB_Green" => return "green".into(),
        "ColorAdd_B" | "ColorRGB_Blue" => return "blue".into(),
        "ColorAdd_W" => return "white".into(),
        "ColorAdd_RY" => return "amber".into(),
        "ColorAdd_GY" => return "lime".into(),
        "ColorAdd_UV" | "UV" => return "uv".into(),
        "CTO" => return "cto".into(),
        "Zoom" => return "zoom".into(),
        "Iris" => return "iris".into(),
        "StrobeFrequency" | "StrobeModeShutter" => return "shutter".into(),
        "ColorWheel" => return "colorwheel".into(),
        "PositionMSpeed" => return "speed".into(),
        _ => {}
    }
    // Name + optional index + optional suffix, e.g. Gobo2Pos, Prism1, Shutter1Strobe.
    let split = a.find(|c: char| c.is_ascii_digit()).unwrap_or(a.len());
    let (base, rest) = a.split_at(split);
    let dend = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    let (digits, suffix) = rest.split_at(dend);
    let n: u32 = digits.parse().unwrap_or(1);
    let first = n == 1;
    let one = |s: &str| s.to_string();
    match (base, suffix) {
        ("Gobo", "") => if first { one("gobo") } else { format!("gobo{n}") },
        ("Gobo", "Pos") => if first { one("gobo_rot") } else { format!("gobo{n}_rot") },
        ("Color", "") if first => one("colorwheel"),
        ("Prism", "") if first => one("prism"),
        ("Prism", "Pos") if first => one("prism_rot"),
        ("Frost", "") if first => one("frost"),
        ("Focus", "") if first => one("focus"),
        ("Shutter", "") | ("Shutter", "Strobe") if first => one("shutter"),
        ("Control", "") if first => one("control"),
        ("Effects", "") if first => one("effect"),
        _ => a.to_lowercase(),
    }
}

#[derive(PartialEq)]
enum Caps {
    Always,
    Never,
    IfSets,
}

fn caps_mode(attr: &str) -> Caps {
    match attr {
        "dimmer" | "pan" | "tilt" | "red" | "green" | "blue" | "white" | "amber" | "uv" | "lime" | "cto" | "zoom"
        | "focus" | "iris" | "frost" | "speed" => Caps::Never,
        a if a.ends_with("_rot") => Caps::Never,
        "colorwheel" | "prism" | "shutter" | "effect" | "control" | "macro" => Caps::Always,
        a if a.starts_with("gobo") => Caps::Always,
        _ => Caps::IfSets,
    }
}

// ───────────── geometry ─────────────

struct ModelInfo {
    file: String,
    dims: [f32; 3], // length, width, height (GDTF)
    primitive: String,
}

const GEO_TAGS: &[&str] = &[
    "Geometry", "Axis", "Beam", "GeometryReference", "FilterBeam", "FilterColor", "FilterGobo", "FilterShaper",
    "Inventory", "Structure", "Support", "Magnet", "WiringObject", "Display", "Laser", "MediaServerLayer",
    "MediaServerCamera", "MediaServerMaster",
];

fn is_geo(n: &Node) -> bool {
    n.is_element() && GEO_TAGS.contains(&n.tag_name().name())
}

struct GeoCtx<'a, 'b> {
    top: HashMap<String, Node<'a, 'b>>,
    models: HashMap<String, ModelInfo>,
    pack: &'a Pack,
    files: Vec<(String, Vec<u8>)>,
    done_models: HashMap<String, Option<String>>, // model file stem → relative path
}

impl<'a, 'b> GeoCtx<'a, 'b> {
    /// Locate, convert and register the model file for `model_name`; returns its relative path.
    fn resolve_model(&mut self, model_name: &str) -> Option<String> {
        let info = self.models.get(model_name)?;
        if info.file.is_empty() {
            return None;
        }
        let stem = info.file.clone();
        let dims = info.dims;
        if let Some(r) = self.done_models.get(&stem) {
            return r.clone();
        }
        let l = stem.to_lowercase();
        let result = if let Some((orig, bytes)) = self.pack.get(&format!("models/gltf/{l}.glb")) {
            let rel = format!("models/{}", orig.rsplit('/').next().unwrap_or("model.glb"));
            self.files.push((rel.clone(), bytes.clone()));
            Some(rel)
        } else if let Some((orig, bytes)) = self.pack.get(&format!("models/3ds/{l}.3ds")) {
            let fname = orig.rsplit('/').next().unwrap_or("model.3ds").to_string();
            self.files.push((format!("models/3ds/{fname}"), bytes.clone()));
            match crate::glb::convert_3ds_to_glb(bytes, dims) {
                Ok(glb) => {
                    let rel = format!("models/{}.glb", fname.trim_end_matches(".3ds").trim_end_matches(".3DS"));
                    self.files.push((rel.clone(), glb));
                    Some(rel)
                }
                Err(e) => {
                    tracing::warn!("GDTF model {fname}: {e}");
                    None
                }
            }
        } else {
            None
        };
        self.done_models.insert(stem, result.clone());
        result
    }

    fn node(&mut self, el: Node<'a, 'b>, depth: u32) -> Option<GeoNode> {
        if depth > 24 || !is_geo(&el) {
            return None;
        }
        let tag = el.tag_name().name();
        let name = el.attribute("Name").unwrap_or("").to_string();
        let mut matrix = to_lumos_matrix(&parse_matrix(el.attribute("Position").unwrap_or("")));
        // GeometryReference: instantiate the referenced geometry under the reference's placement.
        let (src, reference) = if tag == "GeometryReference" {
            match el.attribute("Geometry").and_then(|g| self.top.get(g)).copied() {
                Some(t) => (t, true),
                None => return None,
            }
        } else {
            (el, false)
        };
        let model_name = src.attribute("Model").unwrap_or("").to_string();
        let model = self.resolve_model(&model_name);
        let (mut size, mut primitive) = ([0.0; 3], None);
        if let Some(mi) = self.models.get(&model_name) {
            size = [mi.dims[0], mi.dims[2], mi.dims[1]];
            if model.is_none() && mi.primitive != "Undefined" && !mi.primitive.is_empty() {
                primitive = Some(mi.primitive.clone());
            }
        }
        if model.is_some() {
            primitive = None;
        }
        if reference && src.attribute("Position").is_some() && el.attribute("Position").is_none() {
            matrix = to_lumos_matrix(&parse_matrix(src.attribute("Position").unwrap_or("")));
        }
        let beam = if src.tag_name().name() == "Beam" { Some(beam_info(&src)) } else { None };
        let mut kids = Vec::new();
        for c in src.children().filter(is_geo) {
            if let Some(g) = self.node(c, depth + 1) {
                kids.push(g);
            }
        }
        Some(GeoNode { name, axis: None, model, primitive, size, matrix, beam, children: kids })
    }
}

fn beam_info(n: &Node) -> BeamInfo {
    let beam_angle = fa(n, "BeamAngle", 25.0);
    BeamInfo {
        beam_angle,
        field_angle: fa(n, "FieldAngle", beam_angle),
        beam_angle_max: beam_angle,
        field_angle_max: fa(n, "FieldAngle", beam_angle),
        radius: fa(n, "BeamRadius", 0.05),
        beam_type: match n.attribute("BeamType").unwrap_or("Wash") {
            "Spot" => BeamType::Spot,
            "None" => BeamType::None,
            "Rectangle" => BeamType::Rectangle,
            "PC" => BeamType::Pc,
            "Fresnel" => BeamType::Fresnel,
            "Glow" => BeamType::Glow,
            _ => BeamType::Wash,
        },
        lamp: n.attribute("LampType").unwrap_or("").to_string(),
        power_w: fa(n, "PowerConsumption", 0.0),
        luminous_flux: fa(n, "LuminousFlux", 0.0),
        color_temp: fa(n, "ColorTemperature", 6000.0),
    }
}

fn find_mut<'t>(n: &'t mut GeoNode, name: &str) -> Option<&'t mut GeoNode> {
    if n.name == name {
        return Some(n);
    }
    n.children.iter_mut().find_map(|c| find_mut(c, name))
}

fn walk_beams(n: &mut GeoNode, f: &mut dyn FnMut(&mut BeamInfo)) {
    if let Some(b) = n.beam.as_mut() {
        f(b);
    }
    for c in n.children.iter_mut() {
        walk_beams(c, f);
    }
}

// ───────────── DMX modes ─────────────

#[derive(Default)]
struct ModeStats {
    pan: f32,
    tilt: f32,
    zoom: Option<(f32, f32)>,
    axes: Vec<(String, &'static str)>, // (geometry name, "pan"|"tilt")
    has_color_wheel: bool,
    has_gobo: bool,
    has_pantilt: bool,
    colors: usize,
}

fn parse_offsets(s: &str) -> Vec<u16> {
    let t = s.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("none") {
        return vec![];
    }
    t.split(',').filter_map(|p| p.trim().parse::<u16>().ok()).filter(|v| *v >= 1).collect()
}

fn build_mode(mode: Node, wheels: &[Wheel], assets: &str) -> (Mode, ModeStats) {
    let mut channels: Vec<ChannelDef> = Vec::new();
    let mut st = ModeStats::default();
    let mut used: HashSet<String> = HashSet::new();
    let Some(dchs) = child(mode, "DMXChannels") else {
        return (Mode { name: mode.attribute("Name").unwrap_or("Mode").to_string(), channels }, st);
    };
    for dc in children(dchs, "DMXChannel") {
        match dc.attribute("DMXBreak") {
            None | Some("1") | Some("") => {}
            _ => continue,
        }
        let offs = parse_offsets(dc.attribute("Offset").unwrap_or(""));
        if offs.is_empty() {
            continue; // virtual channel
        }
        let geometry = dc.attribute("Geometry").unwrap_or("").to_string();
        let lcs: Vec<Node> = children(dc, "LogicalChannel").collect();
        let Some(first_lc) = lcs.first() else { continue };
        let gdtf_attr = first_lc.attribute("Attribute").unwrap_or("NoFeature").to_string();
        let mut attr = map_attr(&gdtf_attr);
        if used.contains(&attr) {
            attr = gdtf_attr.to_lowercase();
        }
        if used.contains(&attr) {
            attr = format!("{}_{}", attr, geometry.to_lowercase());
        }
        let mut k = 2;
        let base = attr.clone();
        while used.contains(&attr) {
            attr = format!("{base}{k}");
            k += 1;
        }
        used.insert(attr.clone());

        let funcs: Vec<Node> = lcs.iter().flat_map(|lc| children(*lc, "ChannelFunction")).collect();
        let bytes = offs.len().min(2) as u32;
        // default from InitialFunction ("Geo_Attr.LogicalAttr.FunctionName") else first function
        let init = dc.attribute("InitialFunction").unwrap_or("");
        let init_name = init.rsplit('.').next().unwrap_or("");
        let init_fn = funcs.iter().find(|f| !init_name.is_empty() && f.attribute("Name") == Some(init_name)).or(funcs.first());
        let default = init_fn
            .and_then(|f| f.attribute("Default"))
            .map(|d| {
                let (v, n) = dmx_val(d);
                (v / max_of(if d.contains('/') { n } else { bytes })) as f32
            })
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);

        // physical data
        if attr == "pan" || attr == "tilt" {
            if let Some(f) = funcs.first() {
                let range = (fa(f, "PhysicalTo", 0.0) - fa(f, "PhysicalFrom", 0.0)).abs();
                if attr == "pan" {
                    st.pan = range;
                } else {
                    st.tilt = range;
                }
                st.has_pantilt = true;
                st.axes.push((geometry.clone(), if attr == "pan" { "pan" } else { "tilt" }));
            }
        }
        if attr == "zoom" {
            if let Some(f) = funcs.first() {
                st.zoom = Some((fa(f, "PhysicalFrom", 0.0), fa(f, "PhysicalTo", 0.0)));
            }
        }
        if attr == "colorwheel" {
            st.has_color_wheel = true;
        }
        if attr.starts_with("gobo") && !attr.ends_with("_rot") {
            st.has_gobo = true;
        }
        if matches!(attr.as_str(), "red" | "green" | "blue" | "white" | "amber" | "uv" | "lime") {
            st.colors += 1;
        }

        // capabilities
        let mode_caps = caps_mode(&attr);
        let mut caps: Vec<Capability> = Vec::new();
        if mode_caps != Caps::Never {
            let to8 = |frac: f64| (frac * 255.0).round().clamp(0.0, 255.0) as u8;
            for (fi, f) in funcs.iter().enumerate() {
                if f.attribute("ModeMaster").map_or(false, |m| !m.is_empty()) {
                    continue;
                }
                let (fv, fnb) = dmx_val(f.attribute("DMXFrom").unwrap_or("0"));
                let fmax = max_of(fnb);
                let next_from = funcs.get(fi + 1).map(|nf| {
                    let (v, n) = dmx_val(nf.attribute("DMXFrom").unwrap_or("0"));
                    v / max_of(n)
                });
                let f_lo = fv / fmax;
                let f_hi = next_from.map(|nf| (nf - 1.0 / fmax).max(f_lo)).unwrap_or(1.0);
                let wheel = f.attribute("Wheel").filter(|w| !w.is_empty()).and_then(|w| wheels.iter().find(|x| x.name == w));
                let sets: Vec<Node> = children(*f, "ChannelSet").collect();
                let fname = f.attribute("Name").unwrap_or("").to_string();
                let mk = |lo: f64, hi: f64, label: String, slot_idx: i64| -> Capability {
                    let (mut color, mut image, mut wname, mut slot) = (None, None, None, None);
                    if let (Some(w), true) = (wheel, slot_idx >= 1) {
                        if let Some(s) = w.slots.get(slot_idx as usize - 1) {
                            color = s.color.clone();
                            image = s.image.as_ref().map(|i| format!("{assets}{i}"));
                            wname = Some(w.name.clone());
                            slot = Some(slot_idx as u32);
                        }
                    }
                    Capability { min: to8(lo), max: to8(hi).max(to8(lo)), label, color, image, wheel: wname, slot }
                };
                if sets.is_empty() {
                    if !fname.is_empty() {
                        caps.push(mk(f_lo, f_hi, fname.clone(), 0));
                    }
                } else {
                    for (si, s) in sets.iter().enumerate() {
                        let (sv, sn) = dmx_val(s.attribute("DMXFrom").unwrap_or("0"));
                        let lo = sv / max_of(sn);
                        let hi = sets
                            .get(si + 1)
                            .map(|ns| {
                                let (v, n) = dmx_val(ns.attribute("DMXFrom").unwrap_or("0"));
                                (v / max_of(n) - 1.0 / max_of(n)).max(lo)
                            })
                            .unwrap_or(f_hi);
                        let nm = s.attribute("Name").unwrap_or("").trim();
                        let label = if nm.is_empty() { fname.clone() } else { nm.to_string() };
                        let idx = s.attribute("WheelSlotIndex").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
                        caps.push(mk(lo, hi, label, idx));
                    }
                }
            }
            if mode_caps == Caps::IfSets {
                let mut names: HashSet<&str> = HashSet::new();
                for c in &caps {
                    names.insert(c.label.as_str());
                }
                if names.len() < 2 {
                    caps.clear();
                }
            }
            caps.retain(|c| c.max >= c.min);
        }

        let coarse = offs[0] - 1;
        let fine = if offs.len() >= 2 { Some(offs[1] - 1) } else { None };
        channels.push(ChannelDef { attr, offset: coarse, fine, default, invert: false, capabilities: caps });
    }
    channels.sort_by_key(|c| c.offset);
    (Mode { name: mode.attribute("Name").unwrap_or("Mode").to_string(), channels }, st)
}

fn parse_wheels(ft: Node, pack: &Pack, files: &mut Vec<(String, Vec<u8>)>) -> Vec<Wheel> {
    let mut out = Vec::new();
    let Some(ws) = child(ft, "Wheels") else { return out };
    for w in children(ws, "Wheel") {
        let mut slots = Vec::new();
        for s in children(w, "Slot") {
            let mut image = None;
            if let Some(mf) = s.attribute("MediaFileName").filter(|m| !m.is_empty()) {
                for ext in ["png", "jpg", "jpeg"] {
                    if let Some((orig, bytes)) = pack.get(&format!("wheels/{}.{}", mf.to_lowercase(), ext)) {
                        let rel = format!("wheels/{}", orig.rsplit('/').next().unwrap_or(mf));
                        if !files.iter().any(|(p, _)| *p == rel) {
                            files.push((rel.clone(), bytes.clone()));
                        }
                        image = Some(rel);
                        break;
                    }
                }
            }
            slots.push(WheelSlot {
                name: s.attribute("Name").unwrap_or("").trim().to_string(),
                color: s.attribute("Color").and_then(xyy_to_hex),
                image,
            });
        }
        out.push(Wheel { name: w.attribute("Name").unwrap_or("").to_string(), slots });
    }
    out
}

pub fn import_gdtf(bytes: &[u8]) -> Result<Imported, String> {
    let (xml, pack) = load_pack(bytes)?;
    let doc = Document::parse(&xml).map_err(|e| format!("description.xml: {e}"))?;
    let ft = doc
        .descendants()
        .find(|n| n.is_element() && n.has_tag_name("FixtureType"))
        .ok_or("description.xml has no FixtureType")?;
    let name = ft.attribute("Name").or(ft.attribute("LongName")).unwrap_or("Fixture").to_string();
    let manufacturer = ft.attribute("Manufacturer").unwrap_or("Unknown").to_string();
    let id = format!("gdtf/{}-{}", slug(&manufacturer), slug(&name));
    let assets = format!("/library/{}/", asset_dir(&id));

    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let wheels = parse_wheels(ft, &pack, &mut files);

    // models
    let mut models = HashMap::new();
    if let Some(ms) = child(ft, "Models") {
        for m in children(ms, "Model") {
            models.insert(
                m.attribute("Name").unwrap_or("").to_string(),
                ModelInfo {
                    file: m.attribute("File").unwrap_or("").to_string(),
                    dims: [fa(&m, "Length", 0.0), fa(&m, "Width", 0.0), fa(&m, "Height", 0.0)],
                    primitive: m.attribute("PrimitiveType").unwrap_or("Undefined").to_string(),
                },
            );
        }
    }

    // modes
    let mut modes = Vec::new();
    let mut stats: Vec<ModeStats> = Vec::new();
    let mut root_name = None;
    if let Some(dm) = child(ft, "DMXModes") {
        for m in children(dm, "DMXMode") {
            if root_name.is_none() {
                root_name = m.attribute("Geometry").map(|s| s.to_string());
            }
            let (mode, st) = build_mode(m, &wheels, &assets);
            if !mode.channels.is_empty() {
                modes.push(mode);
                stats.push(st);
            }
        }
    }
    if modes.is_empty() {
        return Err("GDTF has no DMX mode with channels".into());
    }

    // geometry
    let geos_el = child(ft, "Geometries").ok_or("GDTF has no Geometries")?;
    let mut top = HashMap::new();
    let mut first_top = None;
    for g in geos_el.children().filter(is_geo) {
        if first_top.is_none() {
            first_top = g.attribute("Name").map(|s| s.to_string());
        }
        top.insert(g.attribute("Name").unwrap_or("").to_string(), g);
    }
    let root_el = root_name
        .as_deref()
        .and_then(|n| top.get(n).copied())
        .or_else(|| first_top.as_deref().and_then(|n| top.get(n).copied()))
        .ok_or("GDTF has no root geometry")?;
    let mut ctx = GeoCtx { top, models, pack: &pack, files, done_models: HashMap::new() };
    let mut root = ctx.node(root_el, 0).ok_or("Could not build geometry tree")?;
    let mut files = std::mem::take(&mut ctx.files);

    let st = &stats[0];
    for (geo, axis) in &st.axes {
        if let Some(n) = find_mut(&mut root, geo) {
            if n.axis.is_none() {
                n.axis = Some((*axis).to_string());
            }
        }
    }
    // zoom → angle range
    let mut beam_angle = [25.0f32, 25.0];
    let mut power = 0.0f32;
    let mut first = true;
    walk_beams(&mut root, &mut |b| {
        if let Some((a, z)) = st.zoom {
            let (lo, hi) = (a.min(z), a.max(z));
            if lo > 0.0 && hi > 0.0 {
                let ratio = if b.beam_angle > 0.0 { b.field_angle / b.beam_angle } else { 1.0 };
                b.beam_angle = lo;
                b.beam_angle_max = hi;
                b.field_angle = lo * ratio;
                b.field_angle_max = hi * ratio;
            }
        }
        if first {
            beam_angle = [b.beam_angle, b.beam_angle_max];
            first = false;
        }
        power += b.power_w;
    });
    let beam_type = {
        let mut t = BeamType::Wash;
        let mut seen = false;
        walk_beams(&mut root, &mut |b| {
            if !seen {
                t = b.beam_type;
                seen = true;
            }
        });
        t
    };

    let category = if st.has_pantilt {
        if beam_type == BeamType::Wash && !st.has_gobo {
            "wash"
        } else if !st.has_gobo && !st.has_color_wheel && beam_angle[0] < 8.0 {
            "beam"
        } else {
            "spot"
        }
    } else if st.colors > 0 {
        if modes.iter().any(|m| m.channels.len() > 12) { "bar" } else { "par" }
    } else if modes[0].channels.iter().any(|c| c.attr == "dimmer") {
        "dimmer"
    } else {
        "other"
    };

    // keep the original archive so MVR export can embed it
    files.push((SOURCE_FILE.to_string(), bytes.to_vec()));
    // de-duplicate / sort outputs
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.dedup_by(|a, b| a.0 == b.0);

    let has_model = {
        fn any(n: &GeoNode) -> bool {
            n.model.is_some() || n.primitive.is_some() || n.children.iter().any(any)
        }
        any(&root)
    };
    let profile = FixtureProfile {
        id,
        manufacturer,
        model: name,
        category: category.into(),
        modes,
        beam_angle,
        pan_range: st.pan,
        tilt_range: st.tilt,
        power_w: power,
        source: Some("gdtf".into()),
        model3d: if has_model || !wheels.is_empty() { Some(Model3d { assets, root, wheels }) } else { None },
    };
    Ok(Imported { profile, files })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<GDTF DataVersion="1.1">
 <FixtureType Name="Mini Head" Manufacturer="Test Co">
  <Wheels>
   <Wheel Name="Colors">
    <Slot Name="Open" Color="0.3127,0.3290,100"/>
    <Slot Name="Red" Color="0.64,0.33,21"/>
   </Wheel>
   <Wheel Name="Gobos">
    <Slot Name="Open"/>
    <Slot Name="Dots" MediaFileName="dots"/>
   </Wheel>
  </Wheels>
  <Models>
   <Model Name="Base" Length="0.2" Width="0.1" Height="0.1" PrimitiveType="Cube" File="base"/>
   <Model Name="Yoke" Length="0.2" Width="0.05" Height="0.3" PrimitiveType="Undefined" File=""/>
   <Model Name="Head" Length="0.15" Width="0.15" Height="0.25" PrimitiveType="Cylinder" File=""/>
  </Models>
  <Geometries>
   <Geometry Name="Base" Model="Base" Position="{1,0,0,0}{0,1,0,0}{0,0,1,0}{0,0,0,1}">
    <Axis Name="Yoke" Model="Yoke" Position="{1,0,0,0}{0,1,0,0}{0,0,1,-0.1}{0,0,0,1}">
     <Axis Name="Head" Model="Head" Position="{1,0,0,0}{0,1,0,0}{0,0,1,-0.2}{0,0,0,1}">
      <Beam Name="Beam" BeamAngle="10" FieldAngle="20" BeamRadius="0.07" BeamType="Spot" LampType="LED" PowerConsumption="300" LuminousFlux="9000" ColorTemperature="6500" Position="{1,0,0,0}{0,1,0,0}{0,0,1,-0.1}{0,0,0,1}"/>
     </Axis>
    </Axis>
   </Geometry>
  </Geometries>
  <DMXModes>
   <DMXMode Name="Basic" Geometry="Base">
    <DMXChannels>
     <DMXChannel DMXBreak="1" Geometry="Yoke" Offset="1,2" InitialFunction="Yoke_Pan.Pan.Pan">
      <LogicalChannel Attribute="Pan"><ChannelFunction Name="Pan" Attribute="Pan" DMXFrom="0/2" Default="32768/2" PhysicalFrom="-270" PhysicalTo="270">
        <ChannelSet DMXFrom="0/2" Name=""/><ChannelSet DMXFrom="32768/2" Name="Center"/></ChannelFunction></LogicalChannel>
     </DMXChannel>
     <DMXChannel DMXBreak="1" Geometry="Head" Offset="3" InitialFunction="Head_Tilt.Tilt.Tilt">
      <LogicalChannel Attribute="Tilt"><ChannelFunction Name="Tilt" Attribute="Tilt" DMXFrom="0/1" Default="128/1" PhysicalFrom="-135" PhysicalTo="135"/></LogicalChannel>
     </DMXChannel>
     <DMXChannel DMXBreak="1" Geometry="Beam" Offset="4">
      <LogicalChannel Attribute="Dimmer"><ChannelFunction Name="Dimmer" Attribute="Dimmer" DMXFrom="0/1" Default="0/1" PhysicalFrom="0" PhysicalTo="1"/></LogicalChannel>
     </DMXChannel>
     <DMXChannel DMXBreak="1" Geometry="Beam" Offset="5">
      <LogicalChannel Attribute="Color1"><ChannelFunction Name="Color1" Attribute="Color1" Wheel="Colors" DMXFrom="0/1">
        <ChannelSet DMXFrom="0/1" Name="Open" WheelSlotIndex="1"/><ChannelSet DMXFrom="128/1" Name="Red" WheelSlotIndex="2"/></ChannelFunction></LogicalChannel>
     </DMXChannel>
     <DMXChannel DMXBreak="1" Geometry="Beam" Offset="6">
      <LogicalChannel Attribute="Gobo1"><ChannelFunction Name="Gobo1" Attribute="Gobo1" Wheel="Gobos" DMXFrom="0/1">
        <ChannelSet DMXFrom="0/1" Name="Open" WheelSlotIndex="1"/><ChannelSet DMXFrom="100/1" Name="Dots" WheelSlotIndex="2"/></ChannelFunction></LogicalChannel>
     </DMXChannel>
     <DMXChannel DMXBreak="1" Geometry="Beam" Offset="7">
      <LogicalChannel Attribute="Zoom"><ChannelFunction Name="Zoom" Attribute="Zoom" DMXFrom="0/1" PhysicalFrom="8" PhysicalTo="40"/></LogicalChannel>
     </DMXChannel>
     <DMXChannel DMXBreak="2" Geometry="Beam" Offset="9"><LogicalChannel Attribute="Dimmer"><ChannelFunction Name="x" DMXFrom="0/1"/></LogicalChannel></DMXChannel>
     <DMXChannel DMXBreak="1" Geometry="Beam" Offset=""><LogicalChannel Attribute="Dimmer"><ChannelFunction Name="virt" DMXFrom="0/1"/></LogicalChannel></DMXChannel>
    </DMXChannels>
   </DMXMode>
  </DMXModes>
 </FixtureType>
</GDTF>"#;

    pub(crate) fn build_zip(extra: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            z.start_file("description.xml", o).unwrap();
            z.write_all(XML.as_bytes()).unwrap();
            for (n, b) in extra {
                z.start_file(*n, o).unwrap();
                z.write_all(b).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn imports_synthetic_head() {
        let gdtf = build_zip(&[
            ("models/3ds/base.3ds", crate::glb::tests::box_3ds_mm()),
            ("wheels/dots.png", vec![0x89, b'P', b'N', b'G']),
        ]);
        let imp = import_gdtf(&gdtf).unwrap();
        let p = &imp.profile;
        assert_eq!(p.id, "gdtf/test-co-mini-head");
        assert_eq!(p.source.as_deref(), Some("gdtf"));
        let ch = &p.modes[0].channels;
        let attrs: Vec<_> = ch.iter().map(|c| (c.attr.as_str(), c.offset, c.fine)).collect();
        assert_eq!(
            attrs,
            vec![
                ("pan", 0, Some(1)),
                ("tilt", 2, None),
                ("dimmer", 3, None),
                ("colorwheel", 4, None),
                ("gobo", 5, None),
                ("zoom", 6, None)
            ]
        );
        assert!((ch[0].default - 0.5).abs() < 1e-3, "pan default {}", ch[0].default);
        assert!(ch[0].capabilities.is_empty(), "pan must stay continuous");
        assert!(ch[3].capabilities.len() == 2 && ch[3].capabilities[1].color.is_some());
        assert_eq!(ch[3].capabilities[1].min, 128);
        let g = &ch[4].capabilities[1];
        assert_eq!((g.wheel.as_deref(), g.slot, g.min), (Some("Gobos"), Some(2), 100));
        assert_eq!(g.image.as_deref(), Some("/library/gdtf_test-co-mini-head/wheels/dots.png"));
        assert_eq!((p.pan_range, p.tilt_range), (540.0, 270.0));
        assert_eq!(p.beam_angle, [8.0, 40.0]);
        assert_eq!(p.category, "spot");
        let m = p.model3d.as_ref().unwrap();
        assert_eq!(m.assets, "/library/gdtf_test-co-mini-head/");
        assert_eq!(m.root.model.as_deref(), Some("models/base.glb"));
        let yoke = &m.root.children[0];
        let head = &yoke.children[0];
        assert_eq!((yoke.axis.as_deref(), head.axis.as_deref()), (Some("pan"), Some("tilt")));
        assert_eq!(head.primitive.as_deref(), Some("Cylinder"));
        let beam = &head.children[0];
        let b = beam.beam.as_ref().unwrap();
        assert_eq!((b.beam_angle, b.beam_angle_max, b.beam_type), (8.0, 40.0, BeamType::Spot));
        assert!((b.field_angle - 16.0).abs() < 1e-3);
        let names: Vec<_> = imp.files.iter().map(|f| f.0.as_str()).collect();
        assert!(names.contains(&"models/3ds/base.3ds") && names.contains(&"models/base.glb") && names.contains(&"wheels/dots.png"));
    }

    #[test]
    fn matrix_conversion_is_y_up() {
        // GDTF offset (0,0,-0.1) (down) → Lumos (0,-0.1,0): translation = last column of column-major.
        let m = to_lumos_matrix(&parse_matrix("{1,0,0,0}{0,1,0,0}{0,0,1,-0.1}{0,0,0,1}"));
        assert!((m[12]).abs() < 1e-6 && (m[13] + 0.1).abs() < 1e-6 && m[14].abs() < 1e-6, "{m:?}");
        // GDTF +Y offset (away from viewer) → Lumos −Z
        let m = to_lumos_matrix(&parse_matrix("{1,0,0,0}{0,1,0,0.5}{0,0,1,0}{0,0,0,1}"));
        assert!((m[14] + 0.5).abs() < 1e-6, "{m:?}");
        // rotation about GDTF Z (pan) becomes a rotation about Lumos Y
        let rz = to_lumos_matrix(&parse_matrix("{0,-1,0,0}{1,0,0,0}{0,0,1,0}{0,0,0,1}")); // +90° about Z
        // column-major: m[0..3] is image of +X → GDTF +X→+Y becomes Lumos +X→ -Z ; Y axis unchanged
        assert!((rz[1]).abs() < 1e-6 && (rz[5] - 1.0).abs() < 1e-6, "{rz:?}");
        assert!((rz[2] + 1.0).abs() < 1e-6, "{rz:?}");
    }

    #[test]
    fn xyy_swatches() {
        assert_eq!(xyy_to_hex("0.3127,0.3290,100").unwrap(), "#ffffff");
        let red = xyy_to_hex("0.64,0.33,21").unwrap();
        assert!(red.starts_with("#ff"), "{red}");
        assert!(xyy_to_hex("0,0,0").is_none());
    }

    #[test]
    fn attribute_mapping() {
        for (g, o) in [
            ("Dimmer", "dimmer"), ("ColorAdd_R", "red"), ("ColorRGB_Blue", "blue"), ("ColorAdd_RY", "amber"),
            ("Color1", "colorwheel"), ("Color2", "color2"), ("Gobo1", "gobo"), ("Gobo2", "gobo2"),
            ("Gobo1Pos", "gobo_rot"), ("Gobo2Pos", "gobo2_rot"), ("Prism1", "prism"), ("Prism1Pos", "prism_rot"),
            ("Frost1", "frost"), ("Focus1", "focus"), ("Shutter1", "shutter"), ("Shutter1Strobe", "shutter"),
            ("StrobeFrequency", "shutter"), ("Control1", "control"), ("Gobo1WheelSpin", "gobo1wheelspin"),
        ] {
            assert_eq!(map_attr(g), o, "{g}");
        }
    }

    #[test]
    fn rejects_non_gdtf() {
        assert!(import_gdtf(b"nope").is_err());
    }

    #[test]
    fn real_megapointe_if_present() {
        let Ok(bytes) = std::fs::read("/tmp/gdtf-samples/MegaPointe.gdtf") else { return };
        let imp = import_gdtf(&bytes).unwrap();
        let p = &imp.profile;
        assert_eq!(p.manufacturer, "Robe Lighting");
        assert_eq!(p.modes.len(), 2);
        assert!(p.modes[0].channels.len() > 30, "{}", p.modes[0].channels.len());
        assert!(p.modes[0].channels.iter().any(|c| c.attr == "pan" && c.fine.is_some()));
        assert!(p.modes[0].channels.iter().any(|c| c.attr == "colorwheel" && c.capabilities.len() > 5));
        assert!(p.modes[0].channels.iter().any(|c| c.attr == "gobo" && !c.capabilities.is_empty()));
        assert_eq!(p.pan_range, 540.0);
        let m = p.model3d.as_ref().unwrap();
        // hanging device: yoke below base, head below yoke, beam below head
        let base = &m.root;
        let yoke = base.children.iter().find(|c| c.axis.as_deref() == Some("pan")).expect("pan axis");
        let head = yoke.children.iter().find(|c| c.axis.as_deref() == Some("tilt")).expect("tilt axis");
        let beam = head.children.iter().find(|c| c.beam.is_some()).expect("beam");
        let y = |n: &GeoNode| n.matrix[13];
        assert!(y(yoke) < 0.0 && y(head) < 0.0 && y(beam) < 0.0, "{} {} {}", y(yoke), y(head), y(beam));
        assert!((y(yoke) + 0.0948).abs() < 1e-4 && (y(head) + 0.28).abs() < 1e-4 && (y(beam) + 0.257).abs() < 1e-4);
        assert!(base.model.as_deref().map_or(false, |f| f.ends_with(".glb")));
        let names: Vec<_> = imp.files.iter().map(|f| f.0.clone()).collect();
        assert!(names.iter().any(|n| n.ends_with(".3ds")) && names.iter().any(|n| n.ends_with(".glb")), "{names:?}");
        let b = beam.beam.as_ref().unwrap();
        assert_eq!(b.beam_type, BeamType::Spot);
        assert!(b.beam_angle > 0.0 && b.beam_angle_max >= b.beam_angle);
    }
}
