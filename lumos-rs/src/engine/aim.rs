//! Aim moving heads at a point in the room (same kinematics as the 3D / 2D visualisers):
//! beam direction = Ry(pan)·Rx(tilt)·(0,−1,0) in the fixture's own frame, fixture frame = Ry(yaw)·Rx(pitch)·Rz(roll).

use super::*;

type V3 = [f32; 3];
type M3 = [[f32; 3]; 3];

fn rx(a: f32) -> M3 {
    let (s, c) = a.sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}
fn ry(a: f32) -> M3 {
    let (s, c) = a.sin_cos();
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}
fn rz(a: f32) -> M3 {
    let (s, c) = a.sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}
fn mmul(a: M3, b: M3) -> M3 {
    let mut o = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}
fn transpose(m: M3) -> M3 {
    [[m[0][0], m[1][0], m[2][0]], [m[0][1], m[1][1], m[2][1]], [m[0][2], m[1][2], m[2][2]]]
}
fn apply(m: M3, v: V3) -> V3 {
    [m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2], m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2], m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2]]
}

/// Fixture frame from its rotation in degrees (pitch about x, yaw about y, roll about z).
pub fn euler_yxz(rot: V3) -> M3 {
    let d = std::f32::consts::PI / 180.0;
    mmul(mmul(ry(rot[1] * d), rx(rot[0] * d)), rz(rot[2] * d))
}

/// Pan/tilt (0..1) so the beam points along `d` (unit vector in the fixture's frame; down = −Y).
pub fn aim_pan_tilt(pan_range: f32, tilt_range: f32, d: V3) -> (f32, f32) {
    let t0 = (-d[1]).clamp(-1.0, 1.0).acos();
    let s = (d[0] * d[0] + d[2] * d[2]).sqrt();
    let mut p = if s > 1e-6 { (-d[0]).atan2(-d[2]) } else { 0.0 };
    let mut tl = t0;
    let deg = 180.0 / std::f32::consts::PI;
    let (pan_half, tilt_half) = (pan_range / 2.0, tilt_range / 2.0);
    let fits = |p: f32, t: f32| (p * deg).abs() <= pan_half + 0.01 && (t * deg).abs() <= tilt_half + 0.01;
    if !fits(p, tl) {
        let p2 = if p > 0.0 { p - std::f32::consts::PI } else { p + std::f32::consts::PI };
        if fits(p2, -tl) {
            p = p2;
            tl = -tl;
        }
    }
    ((0.5 + p * deg / pan_range).clamp(0.0, 1.0), (0.5 + tl * deg / tilt_range).clamp(0.0, 1.0))
}

/// Pan/tilt for a fixture at `pos` / `rot` to hit `target` (world metres, y up).
pub fn aim_at(pos: V3, rot: V3, pan_range: f32, tilt_range: f32, target: V3) -> (f32, f32) {
    let r = euler_yxz(rot);
    let pivot_local = [0.0, -0.09, 0.0];
    let rp = apply(r, pivot_local);
    let pivot = [pos[0] + rp[0], pos[1] + rp[1], pos[2] + rp[2]];
    let v = apply(transpose(r), [target[0] - pivot[0], target[1] - pivot[1], target[2] - pivot[2]]);
    let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    aim_pan_tilt(pan_range, tilt_range, [v[0] / m, v[1] / m, v[2] / m])
}

impl Engine {
    /// A moving head's pan/tilt ranges, or `None` when the fixture cannot pan/tilt.
    pub(super) fn mover_ranges(&self, f: &Fixture) -> Option<(f32, f32)> {
        let p = self.profiles.get(&f.profile)?;
        let m = p.modes.iter().find(|m| m.name == f.mode)?;
        let has = |a: &str| m.channels.iter().any(|c| c.attr == a);
        (has("pan") && has("tilt")).then(|| (if p.pan_range > 0.0 { p.pan_range } else { 540.0 }, if p.tilt_range > 0.0 { p.tilt_range } else { 270.0 }))
    }

    /// World position a target refers to.
    pub(super) fn aim_target(&self, t: &AimTarget) -> Result<V3, String> {
        match t {
            AimTarget::Point { x, y, z } => {
                if ![x, y, z].iter().all(|v| v.is_finite()) {
                    return Err("Coordenadas no válidas".into());
                }
                Ok([*x, *y, *z])
            }
            AimTarget::Fixture { id } => self.show.fixtures.iter().find(|f| f.id == *id).map(|f| f.pos).ok_or_else(|| format!("El aparato {id} no existe")),
            AimTarget::Object { id } => {
                let o = self.show.stage.objects.iter().find(|o| o.id == *id).ok_or_else(|| format!("El objeto {id} del escenario no existe"))?;
                // aim at the middle of the object's top surface (a person on a platform, a prop…)
                Ok([o.pos[0], o.pos[1] + o.size[1] / 2.0, o.pos[2]])
            }
        }
    }

    pub(super) fn aim(&mut self, fixtures: Vec<u32>, target: AimTarget, height: f32) -> Result<(), String> {
        let mut p = self.aim_target(&target)?;
        if matches!(target, AimTarget::Point { .. }) {
            p[1] += 0.0;
        }
        p[1] += if height.is_finite() { height.clamp(-5.0, 20.0) } else { 0.0 };
        let mut done = 0;
        for id in fixtures {
            let Some(f) = self.show.fixtures.iter().find(|f| f.id == id).cloned() else { continue };
            let Some((pr, tr)) = self.mover_ranges(&f) else { continue };
            let (pan, tilt) = aim_at(f.pos, f.rot, pr, tr, p);
            let mut a = Attrs::new();
            a.insert("pan".into(), (pan * 1e4).round() / 1e4);
            a.insert("tilt".into(), (tilt * 1e4).round() / 1e4);
            self.handle(Command::ProgSet { fixtures: vec![id], attrs: a })?;
            done += 1;
        }
        if done == 0 {
            return Err("Ningún aparato seleccionado puede apuntar (hacen falta canales de pan y tilt)".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: f32 = std::f32::consts::PI / 180.0;

    /// Forward kinematics, exactly as the visualisers draw a beam: pan/tilt → world direction.
    fn beam_dir(rot: V3, pan_range: f32, tilt_range: f32, pan: f32, tilt: f32) -> V3 {
        let (p, t) = ((pan - 0.5) * pan_range * D, (tilt - 0.5) * tilt_range * D);
        let local = [-t.sin() * p.sin(), -t.cos(), -t.sin() * p.cos()];
        apply(euler_yxz(rot), local)
    }

    fn hits_floor(origin: V3, d: V3) -> V3 {
        let k = origin[1] / -d[1];
        [origin[0] + d[0] * k, 0.0, origin[2] + d[2] * k]
    }

    #[test]
    fn a_hanging_head_lands_on_the_target_for_any_floor_point() {
        let pos = [1.0, 6.0, 0.5];
        for target in [[0.0, 0.0, 0.0], [3.0, 0.0, 2.0], [-4.0, 0.0, -1.0], [1.0, 0.0, 5.0], [1.0, 0.0, 0.5]] {
            let (pan, tilt) = aim_at(pos, [0.0, 0.0, 0.0], 540.0, 270.0, target);
            let d = beam_dir([0.0, 0.0, 0.0], 540.0, 270.0, pan, tilt);
            let origin = [pos[0], pos[1] - 0.09, pos[2]];
            let hit = hits_floor(origin, d);
            assert!(((hit[0] - target[0]).powi(2) + (hit[2] - target[2]).powi(2)).sqrt() < 0.02, "target {target:?} → hit {hit:?} (pan {pan:.3} tilt {tilt:.3})");
        }
    }

    #[test]
    fn rotated_and_tilted_fixtures_still_hit_the_target() {
        // a head mounted on a side boom: yawed 90°, pitched 20°
        let pos = [4.0, 5.0, -2.0];
        for rot in [[0.0, 90.0, 0.0], [20.0, 0.0, 0.0], [10.0, 135.0, 5.0]] {
            let target = [0.5, 0.0, 1.0];
            let (pan, tilt) = aim_at(pos, rot, 540.0, 270.0, target);
            let d = beam_dir(rot, 540.0, 270.0, pan, tilt);
            let r = euler_yxz(rot);
            let rp = apply(r, [0.0, -0.09, 0.0]);
            let origin = [pos[0] + rp[0], pos[1] + rp[1], pos[2] + rp[2]];
            let hit = hits_floor(origin, d);
            assert!(((hit[0] - target[0]).powi(2) + (hit[2] - target[2]).powi(2)).sqrt() < 0.02, "rot {rot:?} → {hit:?}");
        }
    }

    #[test]
    fn straight_down_is_the_centre_and_limits_clamp() {
        let (p, t) = aim_pan_tilt(540.0, 270.0, [0.0, -1.0, 0.0]);
        assert!((p - 0.5).abs() < 1e-4 && (t - 0.5).abs() < 1e-4, "{p} {t}");
        // a pan range of only 180° cannot reach behind: the solver flips pan/tilt when that gives an in-range solution
        let (p, t) = aim_pan_tilt(180.0, 270.0, [0.0, -0.3, 0.9539]);
        assert!((0.0..=1.0).contains(&p) && (0.0..=1.0).contains(&t));
        // a degenerate request never produces NaN
        let (p, t) = aim_pan_tilt(540.0, 270.0, [0.0, 0.0, 0.0]);
        assert!(p.is_finite() && t.is_finite());
    }
}
