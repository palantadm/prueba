//! Minimal 3DS → binary glTF (GLB) converter used for GDTF models.
//!
//! * Input: Autodesk 3DS (all objects merged into one mesh; materials/UVs ignored).
//! * Output coordinates are already in Lumos' convention (Y-up, metres): the GDTF/3DS
//!   Z-up right-handed vertices `(x, y, z)` are mapped to `(x, z, -y)` (a −90° rotation
//!   about X), so the viewer loads the GLB without any extra transform.
//! * Normals are smoothed across faces whose angle is below 50° so cylinders look round
//!   while boxes keep hard edges.
//! * Units: the mesh is scaled by a power of ten so its bounding box matches the GDTF
//!   `Model` Length/Width/Height (3DS files in the wild are in m, cm or mm).

use std::collections::HashMap;

type V3 = [f32; 3];

fn u16le(d: &[u8], p: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(p..p + 2)?.try_into().ok()?))
}
fn u32le(d: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(p..p + 4)?.try_into().ok()?))
}
fn f32le(d: &[u8], p: usize) -> Option<f32> {
    Some(f32::from_le_bytes(d.get(p..p + 4)?.try_into().ok()?))
}

#[derive(Default)]
struct Mesh {
    pos: Vec<V3>,
    faces: Vec<[u32; 3]>,
}

fn walk(d: &[u8], mut p: usize, end: usize, depth: u32, cur: &mut Mesh, out: &mut Vec<Mesh>) {
    if depth > 16 {
        return;
    }
    while p + 6 <= end {
        let (Some(id), Some(len)) = (u16le(d, p), u32le(d, p + 2)) else { return };
        let len = len as usize;
        if len < 6 {
            return;
        }
        let cend = (p + len).min(end);
        match id {
            0x4D4D | 0x3D3D => walk(d, p + 6, cend, depth + 1, cur, out),
            0x4000 => {
                // object: zero-terminated name, then sub-chunks
                let mut q = p + 6;
                while q < cend && d[q] != 0 {
                    q += 1;
                }
                walk(d, q + 1, cend, depth + 1, cur, out);
            }
            0x4100 => {
                let mut m = Mesh::default();
                walk(d, p + 6, cend, depth + 1, &mut m, out);
                if !m.pos.is_empty() && !m.faces.is_empty() {
                    out.push(m);
                }
            }
            0x4110 => {
                if let Some(n) = u16le(d, p + 6) {
                    for i in 0..n as usize {
                        let o = p + 8 + i * 12;
                        if let (Some(x), Some(y), Some(z)) = (f32le(d, o), f32le(d, o + 4), f32le(d, o + 8)) {
                            cur.pos.push([x, y, z]);
                        }
                    }
                }
            }
            0x4120 => {
                if let Some(n) = u16le(d, p + 6) {
                    for i in 0..n as usize {
                        let o = p + 8 + i * 8;
                        if let (Some(a), Some(b), Some(c)) = (u16le(d, o), u16le(d, o + 2), u16le(d, o + 4)) {
                            cur.faces.push([a as u32, b as u32, c as u32]);
                        }
                    }
                }
            }
            _ => {}
        }
        p += len;
    }
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
fn norm(a: V3) -> V3 {
    let l = len(a);
    if l < 1e-12 { [0.0, 1.0, 0.0] } else { [a[0] / l, a[1] / l, a[2] / l] }
}

/// `dims` = GDTF Model (Length, Width, Height) in metres, used only to pick the unit scale.
pub fn convert_3ds_to_glb(data: &[u8], dims: [f32; 3]) -> Result<Vec<u8>, String> {
    let mut meshes = Vec::new();
    let mut scratch = Mesh::default();
    walk(data, 0, data.len(), 0, &mut scratch, &mut meshes);
    if meshes.is_empty() {
        return Err("3DS: no mesh found".into());
    }
    // merge
    let mut pos: Vec<V3> = Vec::new();
    let mut faces: Vec<[u32; 3]> = Vec::new();
    for m in meshes {
        let base = pos.len() as u32;
        pos.extend(m.pos.iter().copied());
        for f in m.faces {
            if f.iter().all(|&i| (i as usize) < m.pos.len()) {
                faces.push([f[0] + base, f[1] + base, f[2] + base]);
            }
        }
    }
    if faces.is_empty() {
        return Err("3DS: no valid faces".into());
    }

    // unit scale from bounding box vs declared model size
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in &pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let mut ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    ext.sort_by(|a, b| b.total_cmp(a));
    let mut dm = dims;
    dm.sort_by(|a, b| b.total_cmp(a));
    let scale = if ext[0] > 1e-9 && dm[0] > 1e-6 {
        10f32.powf((dm[0] / ext[0]).log10().round().clamp(-3.0, 3.0))
    } else {
        1.0
    };

    // Z-up → Y-up
    let conv = |p: V3| -> V3 { [p[0] * scale, p[2] * scale, -p[1] * scale] };
    let pos: Vec<V3> = pos.into_iter().map(conv).collect();

    // face normals (area weighted) and smoothing groups by position
    let fnorm: Vec<V3> = faces
        .iter()
        .map(|f| cross(sub(pos[f[1] as usize], pos[f[0] as usize]), sub(pos[f[2] as usize], pos[f[0] as usize])))
        .collect();
    let key = |p: V3| ((p[0] * 1e5).round() as i64, (p[1] * 1e5).round() as i64, (p[2] * 1e5).round() as i64);
    let mut groups: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (fi, f) in faces.iter().enumerate() {
        for &i in f {
            groups.entry(key(pos[i as usize])).or_default().push(fi);
        }
    }
    let cos_t = 50f32.to_radians().cos();
    let mut vp: Vec<f32> = Vec::with_capacity(faces.len() * 9);
    let mut vn: Vec<f32> = Vec::with_capacity(faces.len() * 9);
    let mut bmin = [f32::MAX; 3];
    let mut bmax = [f32::MIN; 3];
    for (fi, f) in faces.iter().enumerate() {
        let un = norm(fnorm[fi]);
        for &i in f {
            let p = pos[i as usize];
            let mut acc = [0.0f32; 3];
            for &g in &groups[&key(p)] {
                if dot(un, norm(fnorm[g])) >= cos_t {
                    for k in 0..3 {
                        acc[k] += fnorm[g][k];
                    }
                }
            }
            let n = if len(acc) > 1e-12 { norm(acc) } else { un };
            vp.extend_from_slice(&p);
            vn.extend_from_slice(&n);
            for k in 0..3 {
                bmin[k] = bmin[k].min(p[k]);
                bmax[k] = bmax[k].max(p[k]);
            }
        }
    }

    let count = vp.len() / 3;
    let mut bin: Vec<u8> = Vec::with_capacity(vp.len() * 8);
    for v in vp.iter().chain(vn.iter()) {
        bin.extend_from_slice(&v.to_le_bytes());
    }
    let blen = vp.len() * 4;
    let json = serde_json::json!({
        "asset": {"version": "2.0", "generator": "lumos gdtf import"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1}, "material": 0, "mode": 4}]}],
        "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [0.16, 0.16, 0.17, 1.0], "metallicFactor": 0.7, "roughnessFactor": 0.45}, "doubleSided": true}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": blen, "target": 34962},
            {"buffer": 0, "byteOffset": blen, "byteLength": blen, "target": 34962}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": count, "type": "VEC3", "min": bmin, "max": bmax},
            {"bufferView": 1, "componentType": 5126, "count": count, "type": "VEC3"}
        ]
    });
    let mut jb = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
    while jb.len() % 4 != 0 {
        jb.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let total = 12 + 8 + jb.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(jb.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    out.extend_from_slice(&jb);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    out.extend_from_slice(&bin);
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn chunk(id: u16, body: &[u8]) -> Vec<u8> {
        let mut v = id.to_le_bytes().to_vec();
        v.extend_from_slice(&((body.len() + 6) as u32).to_le_bytes());
        v.extend_from_slice(body);
        v
    }

    /// 3DS of a 100×100×200 (mm) box centred on the origin, Z up.
    pub fn box_3ds_mm() -> Vec<u8> {
        let (x, y, z) = (50.0f32, 50.0f32, 100.0f32);
        let verts = [
            [-x, -y, -z], [x, -y, -z], [x, y, -z], [-x, y, -z],
            [-x, -y, z], [x, -y, z], [x, y, z], [-x, y, z],
        ];
        let tris: [[u16; 3]; 12] = [
            [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4],
            [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7],
        ];
        let mut vb = (verts.len() as u16).to_le_bytes().to_vec();
        for v in verts {
            for c in v {
                vb.extend_from_slice(&c.to_le_bytes());
            }
        }
        let mut fb = (tris.len() as u16).to_le_bytes().to_vec();
        for t in tris {
            for i in t {
                fb.extend_from_slice(&i.to_le_bytes());
            }
            fb.extend_from_slice(&0u16.to_le_bytes());
        }
        let mut mesh = chunk(0x4110, &vb);
        mesh.extend(chunk(0x4120, &fb));
        let mut obj = b"box\0".to_vec();
        obj.extend(chunk(0x4100, &mesh));
        let editor = chunk(0x4000, &obj);
        chunk(0x4D4D, &chunk(0x3D3D, &editor))
    }

    #[test]
    fn converts_box_to_yup_metres() {
        // declared size 0.1 × 0.1 × 0.2 m → mm mesh must be scaled by 1e-3
        let glb = convert_3ds_to_glb(&box_3ds_mm(), [0.1, 0.1, 0.2]).unwrap();
        assert_eq!(&glb[0..4], b"glTF");
        let jl = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        let j: serde_json::Value = serde_json::from_slice(&glb[20..20 + jl]).unwrap();
        let acc = &j["accessors"][0];
        let mx: Vec<f64> = acc["max"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let mn: Vec<f64> = acc["min"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        // Z-up height 0.2 becomes Y extent; GDTF Y (0.1) becomes Z extent
        assert!((mx[1] - 0.1).abs() < 1e-4 && (mn[1] + 0.1).abs() < 1e-4, "y {mn:?} {mx:?}");
        assert!((mx[0] - 0.05).abs() < 1e-4 && (mx[2] - 0.05).abs() < 1e-4);
        assert_eq!(acc["count"].as_u64().unwrap(), 36);
    }

    #[test]
    fn rejects_garbage() {
        assert!(convert_3ds_to_glb(b"not a 3ds", [1.0, 1.0, 1.0]).is_err());
    }
}
