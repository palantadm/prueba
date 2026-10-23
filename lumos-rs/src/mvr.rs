//! MVR (My Virtual Rig, DIN SPEC 15801): zip with `GeneralSceneDescription.xml` + GDTF files.
//!
//! Coordinates: MVR/GDTF are Z-up, right-handed, millimetres; Lumos is Y-up, metres (x = stage right,
//! y = up, z = downstage). With `C` mapping our (x,y,z) → GDTF (x,−z,y): `M_gdtf = C·M·Cᵀ`, `t_gdtf = C·t·1000`.
//! Fixture rotation is Euler "YXZ" in degrees (the 3D scene's order): `R = Ry·Rx·Rz`.
//! An MVR `Matrix` is `{u}{v}{w}{o}`: u, v, w are the basis vectors (columns of R), o the origin.

use crate::csv::{resolve_mode, resolve_profile};
use crate::gdtf;
use crate::model::*;
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;

type R3 = [[f64; 3]; 3];
const C: R3 = [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]];

fn mul(a: &R3, b: &R3) -> R3 {
    let mut o = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}

fn tr(a: &R3) -> R3 {
    let mut o = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = a[j][i];
        }
    }
    o
}

fn mv(a: &R3, v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| (0..3).map(|k| a[i][k] * v[k]).sum())
}

/// three.js Euler "YXZ" → rotation matrix (`Ry·Rx·Rz`), degrees.
pub fn euler_to_matrix(deg: [f32; 3]) -> R3 {
    let [x, y, z] = deg.map(|d| (d as f64).to_radians());
    let rx = [[1.0, 0.0, 0.0], [0.0, x.cos(), -x.sin()], [0.0, x.sin(), x.cos()]];
    let ry = [[y.cos(), 0.0, y.sin()], [0.0, 1.0, 0.0], [-y.sin(), 0.0, y.cos()]];
    let rz = [[z.cos(), -z.sin(), 0.0], [z.sin(), z.cos(), 0.0], [0.0, 0.0, 1.0]];
    mul(&ry, &mul(&rx, &rz))
}

/// Inverse of [`euler_to_matrix`] (three.js `Euler.setFromRotationMatrix`, order YXZ), degrees.
pub fn matrix_to_euler(m: &R3) -> [f32; 3] {
    let x = (-m[1][2].clamp(-1.0, 1.0)).asin();
    let (y, z) = if m[1][2].abs() < 0.999_999_9 {
        (m[0][2].atan2(m[2][2]), m[1][0].atan2(m[1][1]))
    } else {
        ((-m[2][0]).atan2(m[0][0]), 0.0)
    };
    [x, y, z].map(|r| {
        let d = r.to_degrees();
        // tidy -0 / 1e-14 noise
        ((d * 1e4).round() / 1e4) as f32
    })
}

/// A rigid transform in the GDTF frame: rotation `r` and translation `t` (mm).
#[derive(Clone, Copy)]
struct Xf {
    r: R3,
    t: [f64; 3],
}

impl Xf {
    const ID: Xf = Xf { r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: [0.0; 3] };
    fn then(&self, child: &Xf) -> Xf {
        let rt = mv(&self.r, child.t);
        Xf { r: mul(&self.r, &child.r), t: [rt[0] + self.t[0], rt[1] + self.t[1], rt[2] + self.t[2]] }
    }
}

/// Our pose (metres, YXZ degrees) → GDTF-frame transform (mm).
fn to_gdtf(pos: [f32; 3], rot: [f32; 3]) -> Xf {
    let r = euler_to_matrix(rot);
    let rg = mul(&C, &mul(&r, &tr(&C)));
    let t = mv(&C, [pos[0] as f64, pos[1] as f64, pos[2] as f64]);
    Xf { r: rg, t: [t[0] * 1000.0, t[1] * 1000.0, t[2] * 1000.0] }
}

/// GDTF-frame transform (mm) → our pose.
fn from_gdtf(x: &Xf) -> ([f32; 3], [f32; 3]) {
    let r = mul(&tr(&C), &mul(&x.r, &C));
    let t = mv(&tr(&C), [x.t[0] / 1000.0, x.t[1] / 1000.0, x.t[2] / 1000.0]);
    ([t[0] as f32, t[1] as f32, t[2] as f32].map(|v| (v * 1e4).round() / 1e4), matrix_to_euler(&r))
}

fn matrix_string(x: &Xf) -> String {
    let col = |j: usize| format!("{{{:.6},{:.6},{:.6}}}", x.r[0][j], x.r[1][j], x.r[2][j]);
    format!("{}{}{}{{{:.6},{:.6},{:.6}}}", col(0), col(1), col(2), x.t[0], x.t[1], x.t[2])
}

fn parse_matrix(s: &str) -> Option<Xf> {
    let groups: Vec<Vec<f64>> = s
        .split('}')
        .map(|g| g.trim().trim_start_matches('{'))
        .filter(|g| !g.is_empty())
        .map(|g| g.split(',').filter_map(|v| v.trim().parse::<f64>().ok()).collect::<Vec<f64>>())
        .collect();
    if groups.len() != 4 || groups.iter().any(|g| g.len() != 3) {
        return None;
    }
    let mut r = [[0.0; 3]; 3];
    for j in 0..3 {
        for i in 0..3 {
            r[i][j] = groups[j][i];
        }
    }
    Some(Xf { r, t: [groups[3][0], groups[3][1], groups[3][2]] })
}

fn xml_esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn uuid(kind: u32, id: u32) -> String {
    format!("{:08x}-0000-4000-8000-{:012x}", 0x4c75_6d00 + kind, id)
}

fn file_safe(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect();
    t.trim().to_string()
}

/// `Manufacturer@Model.gdtf`
pub fn spec_name(p: &FixtureProfile) -> String {
    format!("{}@{}.gdtf", file_safe(&p.manufacturer), file_safe(&p.model))
}

// ───────────────────────────── export ─────────────────────────────

pub fn export(show: &Show, profs: &HashMap<String, FixtureProfile>, lib_dir: &Path) -> Result<Vec<u8>, String> {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n");
    xml.push_str("<GeneralSceneDescription verMajor=\"1\" verMinor=\"6\">\n");
    xml.push_str("  <UserData>\n    <Data provider=\"Lumos\" ver=\"0.3\"/>\n  </UserData>\n  <Scene>\n    <Layers>\n");
    xml.push_str(&format!("      <Layer name=\"{}\" uuid=\"{}\">\n        <ChildList>\n", xml_esc(&show.name), uuid(1, 0)));

    let mut gdtf_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut fixtures: Vec<&Fixture> = show.fixtures.iter().collect();
    fixtures.sort_by_key(|f| f.id);
    for f in fixtures {
        let prof = profs.get(&f.profile);
        let spec = match prof {
            Some(p) => spec_name(p),
            None => format!("Unknown@{}.gdtf", file_safe(&f.profile)),
        };
        if let Some(p) = prof {
            if p.id.starts_with("gdtf/") && !gdtf_files.iter().any(|(n, _)| *n == spec) {
                let src = lib_dir.join(gdtf::asset_dir(&p.id)).join(gdtf::SOURCE_FILE);
                if let Ok(bytes) = std::fs::read(&src) {
                    gdtf_files.push((spec.clone(), bytes));
                }
            }
        }
        let xf = to_gdtf(f.pos, f.rot);
        let abs = (f.universe as u32 - 1) * 512 + f.address as u32;
        xml.push_str(&format!(
            "          <Fixture name=\"{}\" uuid=\"{}\">\n            <Matrix>{}</Matrix>\n            <GDTFSpec>{}</GDTFSpec>\n            <GDTFMode>{}</GDTFMode>\n            <Addresses>\n              <Address break=\"0\">{}</Address>\n            </Addresses>\n            <FixtureID>{}</FixtureID>\n            <FixtureIDNumeric>{}</FixtureIDNumeric>\n            <UnitNumber>0</UnitNumber>\n            <CustomId>0</CustomId>\n            <CustomIdType>0</CustomIdType>\n          </Fixture>\n",
            xml_esc(&f.name),
            uuid(2, f.id),
            matrix_string(&xf),
            xml_esc(&spec),
            xml_esc(&f.mode),
            abs,
            f.id,
            f.id,
        ));
    }
    xml.push_str("        </ChildList>\n      </Layer>\n    </Layers>\n  </Scene>\n</GeneralSceneDescription>\n");

    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        z.start_file("GeneralSceneDescription.xml", deflated).map_err(|e| e.to_string())?;
        z.write_all(xml.as_bytes()).map_err(|e| e.to_string())?;
        for (name, bytes) in &gdtf_files {
            z.start_file(name.as_str(), stored).map_err(|e| e.to_string())?;
            z.write_all(bytes).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

// ───────────────────────────── import ─────────────────────────────

#[derive(Debug, Clone)]
pub struct MvrFixture {
    pub name: String,
    /// GDTFSpec text (file name).
    pub spec: String,
    pub mode: String,
    pub id: Option<u32>,
    /// `None` when the fixture has no DMX address.
    pub address: Option<(u16, u16)>,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
}

pub struct Parsed {
    pub fixtures: Vec<MvrFixture>,
    /// `.gdtf` files found in the archive: (file name, bytes).
    pub gdtf: Vec<(String, Vec<u8>)>,
}

const MAX_XML: u64 = 64 * 1024 * 1024;
const MAX_GDTF_TOTAL: u64 = 256 * 1024 * 1024;

fn child<'a, 'b>(n: Node<'a, 'b>, tag: &str) -> Option<Node<'a, 'b>> {
    n.children().find(|c| c.is_element() && c.has_tag_name(tag))
}

fn text(n: Node, tag: &str) -> Option<String> {
    child(n, tag).and_then(|c| c.text()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn parse_address(s: &str) -> Option<(u16, u16)> {
    let s = s.trim();
    if let Some((u, a)) = s.split_once('.') {
        let (u, a): (u16, u16) = (u.trim().parse().ok()?, a.trim().parse().ok()?);
        return ((1..=512).contains(&a) && u >= 1).then_some((u, a));
    }
    let abs: u32 = s.parse().ok()?;
    if abs == 0 || abs > 65_535 * 512 {
        return None;
    }
    Some((((abs - 1) / 512 + 1) as u16, ((abs - 1) % 512 + 1) as u16))
}

fn walk(list: Node, parent: &Xf, out: &mut Vec<MvrFixture>) {
    for n in list.children().filter(|c| c.is_element()) {
        let own = child(n, "Matrix").and_then(|m| m.text()).and_then(parse_matrix).unwrap_or(Xf::ID);
        let xf = parent.then(&own);
        match n.tag_name().name() {
            "Fixture" => {
                let address = child(n, "Addresses").and_then(|a| {
                    let adds: Vec<Node> = a.children().filter(|c| c.is_element() && c.has_tag_name("Address")).collect();
                    let pick = adds.iter().find(|x| x.attribute("break").unwrap_or("0") == "0").or(adds.first())?;
                    parse_address(pick.text()?)
                });
                let id = text(n, "FixtureIDNumeric").or_else(|| text(n, "FixtureID")).and_then(|s| s.parse::<u32>().ok()).filter(|v| *v > 0);
                let (pos, rot) = from_gdtf(&xf);
                out.push(MvrFixture {
                    name: n.attribute("name").unwrap_or("").to_string(),
                    spec: text(n, "GDTFSpec").unwrap_or_default(),
                    mode: text(n, "GDTFMode").unwrap_or_default(),
                    id,
                    address,
                    pos,
                    rot,
                });
            }
            "GroupObject" => {
                if let Some(cl) = child(n, "ChildList") {
                    walk(cl, &xf, out);
                }
            }
            _ => {}
        }
    }
}

pub fn parse(bytes: &[u8]) -> Result<Parsed, String> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("not a valid MVR (zip) file: {e}"))?;
    let mut xml: Option<String> = None;
    let mut gdtf_files = Vec::new();
    let mut gdtf_total = 0u64;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().to_string();
        let base = name.rsplit('/').next().unwrap_or(&name).to_string();
        if name == "GeneralSceneDescription.xml" {
            if f.size() > MAX_XML {
                return Err("GeneralSceneDescription.xml is too large".into());
            }
            let mut s = String::new();
            f.read_to_string(&mut s).map_err(|e| format!("GeneralSceneDescription.xml: {e}"))?;
            xml = Some(s);
        } else if base.to_lowercase().ends_with(".gdtf") && !name.contains('/') {
            gdtf_total += f.size();
            if gdtf_total > MAX_GDTF_TOTAL {
                return Err("embedded GDTF files are too large".into());
            }
            let mut b = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut b).map_err(|e| e.to_string())?;
            gdtf_files.push((base, b));
        }
    }
    let xml = xml.ok_or("GeneralSceneDescription.xml missing in archive")?;
    let doc = Document::parse(&xml).map_err(|e| format!("GeneralSceneDescription.xml: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "GeneralSceneDescription" {
        return Err("not an MVR scene description".into());
    }
    let mut fixtures = Vec::new();
    for layer in root.descendants().filter(|n| n.is_element() && n.has_tag_name("Layer")) {
        let lx = child(layer, "Matrix").and_then(|m| m.text()).and_then(parse_matrix).unwrap_or(Xf::ID);
        if let Some(cl) = child(layer, "ChildList") {
            walk(cl, &lx, &mut fixtures);
        }
    }
    Ok(Parsed { fixtures, gdtf: gdtf_files })
}

fn norm_spec(s: &str) -> String {
    let b = s.rsplit(['/', '\\']).next().unwrap_or(s);
    let b = b.strip_suffix(".gdtf").or_else(|| b.strip_suffix(".GDTF")).unwrap_or(b);
    b.to_lowercase()
}

/// `Manufacturer@Model[@Revision]` → (manufacturer, model).
fn split_spec(spec: &str) -> Option<(String, String)> {
    let b = spec.rsplit(['/', '\\']).next().unwrap_or(spec);
    let b = b.strip_suffix(".gdtf").unwrap_or(b);
    let mut it = b.split('@');
    let m = it.next()?.trim().to_string();
    let n = it.next()?.trim().to_string();
    (!m.is_empty() && !n.is_empty()).then_some((m, n))
}

#[derive(Debug, Default)]
pub struct Applied {
    pub created: usize,
    pub updated: usize,
    pub unmatched: Vec<(String, String)>,
    pub skipped: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

/// Merge parsed MVR fixtures into the show. `spec_ids`: embedded-GDTF spec name (normalised) → profile id.
pub fn apply(
    show: &mut Show,
    profs: &HashMap<String, FixtureProfile>,
    fixtures: &[MvrFixture],
    spec_ids: &HashMap<String, String>,
    replace: bool,
) -> Applied {
    let mut out = Applied::default();
    let mut used: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut next = show.fixtures.iter().map(|f| f.id).max().unwrap_or(0) + 1;
    let mut keep: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for mf in fixtures {
        let label = if mf.name.is_empty() { mf.spec.clone() } else { mf.name.clone() };
        let Some((universe, address)) = mf.address else {
            out.skipped.push((label, "no DMX address".into()));
            continue;
        };
        let prof = spec_ids
            .get(&norm_spec(&mf.spec))
            .and_then(|id| profs.get(id))
            .or_else(|| {
                split_spec(&mf.spec).and_then(|(m, n)| resolve_profile(profs, &format!("{m} {n}")).or_else(|| resolve_profile(profs, &n)))
            })
            .or_else(|| resolve_profile(profs, mf.spec.trim_end_matches(".gdtf")));
        let Some(prof) = prof else {
            out.unmatched.push((label, mf.spec.clone()));
            continue;
        };
        let mode = match resolve_mode(prof, &mf.mode) {
            Ok(m) => m,
            Err(_) => {
                out.warnings.push(format!("{label}: mode '{}' not found in {} — using '{}'", mf.mode, prof.id, prof.modes.first().map(|m| m.name.as_str()).unwrap_or("?")));
                match prof.modes.first() {
                    Some(m) => m,
                    None => {
                        out.unmatched.push((label, mf.spec.clone()));
                        continue;
                    }
                }
            }
        };
        let id = match mf.id {
            Some(i) if !used.contains(&i) => i,
            _ => {
                while used.contains(&next) || show.fixtures.iter().any(|f| f.id == next) {
                    next += 1;
                }
                next
            }
        };
        used.insert(id);
        keep.insert(id);
        let name = if mf.name.is_empty() { format!("Fixture {id}") } else { mf.name.clone() };
        match show.fixtures.iter_mut().find(|f| f.id == id) {
            Some(ex) => {
                ex.name = name;
                ex.profile = prof.id.clone();
                ex.mode = mode.name.clone();
                ex.universe = universe;
                ex.address = address;
                ex.pos = mf.pos;
                ex.rot = mf.rot;
                out.updated += 1;
            }
            None => {
                show.fixtures.push(Fixture { id, name, profile: prof.id.clone(), mode: mode.name.clone(), universe, address, pos: mf.pos, rot: mf.rot });
                out.created += 1;
            }
        }
        if !show.universes.iter().any(|u| u.id == universe) {
            show.universes.push(UniverseCfg { id: universe, name: format!("Universe {universe}"), outputs: vec![] });
            show.universes.sort_by_key(|u| u.id);
        }
    }
    if replace {
        show.fixtures.retain(|f| keep.contains(&f.id));
        for g in &mut show.groups {
            g.fixtures.retain(|i| keep.contains(i));
        }
    }
    show.fixtures.sort_by_key(|f| f.id);
    out
}

/// Spec names (normalised) of the embedded GDTF files → raw bytes, restricted to those the fixtures reference.
pub fn referenced_gdtf<'a>(parsed: &'a Parsed) -> Vec<(String, &'a [u8])> {
    let wanted: std::collections::BTreeSet<String> = parsed.fixtures.iter().map(|f| norm_spec(&f.spec)).collect();
    parsed.gdtf.iter().filter(|(n, _)| wanted.contains(&norm_spec(n))).map(|(n, b)| (norm_spec(n), b.as_slice())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::builtin;

    fn profs() -> HashMap<String, FixtureProfile> {
        builtin().into_iter().map(|p| (p.id.clone(), p)).collect()
    }

    fn close(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= tol)
    }

    #[test]
    fn euler_matrix_round_trip() {
        for rot in [[0.0, 0.0, 0.0], [-80.0, 0.0, 0.0], [10.0, 35.0, -20.0], [-45.0, 170.0, 5.0], [89.0, -120.0, 30.0]] {
            let back = matrix_to_euler(&euler_to_matrix(rot));
            // compare rotations (Euler triples are not unique): same matrix
            let a = euler_to_matrix(rot);
            let b = euler_to_matrix(back);
            for i in 0..3 {
                for j in 0..3 {
                    assert!((a[i][j] - b[i][j]).abs() < 1e-6, "{rot:?} → {back:?}");
                }
            }
        }
        assert!(close(matrix_to_euler(&euler_to_matrix([-80.0, 0.0, 0.0])), [-80.0, 0.0, 0.0], 1e-3));
    }

    #[test]
    fn coordinate_conversion_and_matrix_text() {
        // 2 m stage right, 5 m up, 1 m downstage → GDTF mm: (2000, −1000, 5000)
        let xf = to_gdtf([2.0, 5.0, 1.0], [0.0; 3]);
        assert!(close([xf.t[0] as f32, xf.t[1] as f32, xf.t[2] as f32], [2000.0, -1000.0, 5000.0], 1e-3));
        let s = matrix_string(&xf);
        assert!(s.starts_with("{1.000000,0.000000,0.000000}{0.000000,1.000000,0.000000}{0.000000,0.000000,1.000000}{2000.000000,-1000.000000,5000.000000}"), "{s}");
        let (p, r) = from_gdtf(&parse_matrix(&s).unwrap());
        assert!(close(p, [2.0, 5.0, 1.0], 1e-4) && close(r, [0.0; 3], 1e-3));
        // spec example matrix parses (rotation about Z)
        let m = parse_matrix("{0.158127,-0.987419,0.000000}{0.987419,0.158127,0.000000}{0.000000,0.000000,1.000000}{6020.939200,2838.588955,4978.134459}").unwrap();
        assert!((m.r[0][0] - 0.158127).abs() < 1e-9 && (m.r[1][0] + 0.987419).abs() < 1e-9 && (m.t[2] - 4978.134459).abs() < 1e-9);
        assert!(parse_matrix("{1,0,0}{0,1,0}").is_none());
        // a fixture pitched toward the stage keeps the same orientation after the round trip
        let xf = to_gdtf([0.0, 5.0, 0.0], [-80.0, 20.0, 0.0]);
        let (_, r) = from_gdtf(&xf);
        let (a, b) = (euler_to_matrix([-80.0, 20.0, 0.0]), euler_to_matrix(r));
        for i in 0..3 {
            for j in 0..3 {
                assert!((a[i][j] - b[i][j]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn addresses() {
        assert_eq!(parse_address("1"), Some((1, 1)));
        assert_eq!(parse_address("512"), Some((1, 512)));
        assert_eq!(parse_address("513"), Some((2, 1)));
        assert_eq!(parse_address("3.45"), Some((3, 45)));
        assert_eq!(parse_address("0"), None);
        assert_eq!(parse_address("1.513"), None);
        assert_eq!(parse_address("x"), None);
    }

    fn rig() -> Show {
        let lib = profs();
        let par = &lib["generic/rgb-par"];
        let mh = &lib["generic/mh-spot"];
        let mut s = Show::default();
        s.name = "Rig & <test>".into();
        s.fixtures.push(Fixture { id: 7, name: "Par \"A\"".into(), profile: par.id.clone(), mode: par.modes[0].name.clone(), universe: 1, address: 1, pos: [-2.5, 5.0, 1.5], rot: [-30.0, 10.0, 0.0] });
        s.fixtures.push(Fixture { id: 8, name: "Mover".into(), profile: mh.id.clone(), mode: mh.modes[0].name.clone(), universe: 3, address: 130, pos: [3.0, 4.2, -1.0], rot: [0.0; 3] });
        s
    }

    #[test]
    fn export_import_round_trip_builtin_profiles() {
        let show = rig();
        let bytes = export(&show, &profs(), Path::new("/nonexistent")).unwrap();
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.fixtures.len(), 2);
        assert!(parsed.gdtf.is_empty());
        let mut dst = Show::default();
        let rep = apply(&mut dst, &profs(), &parsed.fixtures, &HashMap::new(), false);
        assert_eq!((rep.created, rep.updated, rep.unmatched.len()), (2, 0, 0), "{rep:?}");
        for (a, b) in show.fixtures.iter().zip(dst.fixtures.iter()) {
            assert_eq!((a.id, &a.name, &a.profile, &a.mode, a.universe, a.address), (b.id, &b.name, &b.profile, &b.mode, b.universe, b.address));
            assert!(close(a.pos, b.pos, 1e-3), "{:?} {:?}", a.pos, b.pos);
            let (ma, mb) = (euler_to_matrix(a.rot), euler_to_matrix(b.rot));
            for i in 0..3 {
                for j in 0..3 {
                    assert!((ma[i][j] - mb[i][j]).abs() < 1e-5);
                }
            }
        }
        assert!(dst.universes.iter().any(|u| u.id == 3));
    }

    #[test]
    fn synthetic_mvr_with_groups_and_unmatched() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<GeneralSceneDescription verMajor="1" verMinor="5"><Scene><Layers><Layer name="L" uuid="x"><Matrix>{1,0,0}{0,1,0}{0,0,1}{0,0,1000}</Matrix><ChildList>
 <GroupObject name="G" uuid="g"><Matrix>{1,0,0}{0,1,0}{0,0,1}{1000,0,0}</Matrix><ChildList>
  <Fixture name="Grouped" uuid="a"><Matrix>{1,0,0}{0,1,0}{0,0,1}{500,0,2000}</Matrix><GDTFSpec>Generic@RGB Par.gdtf</GDTFSpec><GDTFMode>3ch</GDTFMode><Addresses><Address break="0">514</Address></Addresses><FixtureID>12</FixtureID></Fixture>
 </ChildList></GroupObject>
 <Fixture name="Mystery" uuid="b"><Matrix>{1,0,0}{0,1,0}{0,0,1}{0,0,0}</Matrix><GDTFSpec>Nobody@Unknown Thing.gdtf</GDTFSpec><GDTFMode>x</GDTFMode><Addresses><Address break="0">1</Address></Addresses></Fixture>
 <Fixture name="NoAddr" uuid="c"><GDTFSpec>Generic@RGB Par.gdtf</GDTFSpec><GDTFMode>3ch</GDTFMode></Fixture>
</ChildList></Layer></Layers></Scene></GeneralSceneDescription>"#;
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file("GeneralSceneDescription.xml", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(xml.as_bytes()).unwrap();
            z.finish().unwrap();
        }
        let parsed = parse(&buf.into_inner()).unwrap();
        assert_eq!(parsed.fixtures.len(), 3);
        let g = &parsed.fixtures[0];
        // layer (0,0,1 m up) + group (1 m x) + fixture (0.5 m x, 2 m up in GDTF Z) → ours: x 1.5, y 3, z 0
        assert!(close(g.pos, [1.5, 3.0, 0.0], 1e-4), "{:?}", g.pos);
        assert_eq!(g.address, Some((2, 2)));
        assert_eq!(g.id, Some(12));
        let mut s = Show::default();
        let rep = apply(&mut s, &profs(), &parsed.fixtures, &HashMap::new(), false);
        assert_eq!((rep.created, rep.unmatched.len(), rep.skipped.len()), (1, 1, 1), "{rep:?}");
        assert_eq!(rep.unmatched[0].0, "Mystery");
        assert_eq!(s.fixtures[0].id, 12);
        assert!(parse(b"not a zip").is_err());
    }

    #[test]
    fn round_trip_with_embedded_gdtf() {
        use crate::gdtf::tests::build_zip;
        let gdtf_bytes = build_zip(&[("models/3ds/base.3ds", crate::glb::tests::box_3ds_mm())]);
        let imp = gdtf::import_gdtf(&gdtf_bytes).unwrap();
        // library dir as the REST import would leave it
        let dir = std::env::temp_dir().join(format!("lumos-mvr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let adir = dir.join(gdtf::asset_dir(&imp.profile.id));
        std::fs::create_dir_all(&adir).unwrap();
        std::fs::write(adir.join(gdtf::SOURCE_FILE), gdtf::tests::build_zip(&[("models/3ds/base.3ds", crate::glb::tests::box_3ds_mm())])).unwrap();

        let mut library = profs();
        library.insert(imp.profile.id.clone(), imp.profile.clone());
        let mut show = Show::default();
        show.fixtures.push(Fixture {
            id: 1,
            name: "Head".into(),
            profile: imp.profile.id.clone(),
            mode: imp.profile.modes[0].name.clone(),
            universe: 2,
            address: 33,
            pos: [1.0, 6.0, 2.0],
            rot: [-60.0, 0.0, 0.0],
        });
        let bytes = export(&show, &library, &dir).unwrap();
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.gdtf.len(), 1, "GDTF embedded in the archive");
        let refs = referenced_gdtf(&parsed);
        assert_eq!(refs.len(), 1);
        // fresh install: profile unknown until the embedded GDTF is imported
        let mut spec_ids = HashMap::new();
        let mut fresh = profs();
        for (spec, b) in refs {
            let i = gdtf::import_gdtf(b).unwrap();
            spec_ids.insert(spec, i.profile.id.clone());
            fresh.insert(i.profile.id.clone(), i.profile);
        }
        let mut dst = Show::default();
        let rep = apply(&mut dst, &fresh, &parsed.fixtures, &spec_ids, false);
        assert_eq!((rep.created, rep.unmatched.len()), (1, 0), "{rep:?}");
        assert_eq!(dst.fixtures[0].profile, imp.profile.id);
        assert_eq!((dst.fixtures[0].universe, dst.fixtures[0].address), (2, 33));
        assert!(close(dst.fixtures[0].pos, [1.0, 6.0, 2.0], 1e-3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real fixture from the GDTF share: only runs when the sample file is present.
    #[test]
    fn megapointe_sample_round_trip() {
        let Ok(bytes) = std::fs::read("/tmp/gdtf-samples/MegaPointe.gdtf") else { return };
        let imp = gdtf::import_gdtf(&bytes).unwrap();
        let dir = std::env::temp_dir().join(format!("lumos-mvr-mp-{}", std::process::id()));
        let adir = dir.join(gdtf::asset_dir(&imp.profile.id));
        std::fs::create_dir_all(&adir).unwrap();
        std::fs::write(adir.join(gdtf::SOURCE_FILE), &bytes).unwrap();
        let mut library = profs();
        library.insert(imp.profile.id.clone(), imp.profile.clone());
        let mut show = Show::default();
        for i in 0..3u32 {
            show.fixtures.push(Fixture {
                id: i + 1,
                name: format!("MP {}", i + 1),
                profile: imp.profile.id.clone(),
                mode: imp.profile.modes[0].name.clone(),
                universe: 1,
                address: 1 + i as u16 * 40,
                pos: [i as f32, 5.0, 0.5],
                rot: [0.0; 3],
            });
        }
        let out = export(&show, &library, &dir).unwrap();
        let parsed = parse(&out).unwrap();
        assert_eq!(parsed.gdtf.len(), 1);
        assert_eq!(parsed.fixtures.len(), 3);
        let i2 = gdtf::import_gdtf(&parsed.gdtf[0].1).unwrap();
        assert_eq!(i2.profile.id, imp.profile.id);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
