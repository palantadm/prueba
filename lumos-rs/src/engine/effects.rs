//! Parametric effects evaluated on top of the merged state.

use super::patch::{Patch, COLOR_ATTRS};
use crate::model::*;

fn frac(x: f64) -> f64 {
    x - x.floor()
}

/// Waveform in −1..1 for `phase` (cycles).
pub fn shape_value(shape: Shape, phase: f64, seed: u32) -> f32 {
    let f = frac(phase);
    (match shape {
        Shape::Sine => (phase * std::f64::consts::TAU).sin(),
        Shape::Triangle => {
            if f < 0.25 {
                4.0 * f
            } else if f < 0.75 {
                2.0 - 4.0 * f
            } else {
                4.0 * f - 4.0
            }
        }
        Shape::Saw => 2.0 * f - 1.0,
        Shape::Square => {
            if f < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Shape::Random => {
            let mut h = (phase.floor() as i64 as u64) ^ ((seed as u64) << 32) ^ 0x9E37_79B9_7F4A_7C15;
            h ^= h >> 33;
            h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
            h ^= h >> 33;
            h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
            h ^= h >> 33;
            (h % 20001) as f64 / 10000.0 - 1.0
        }
    }) as f32
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let h = (h - h.floor()) * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// Apply every running effect to `state`. `depth_mul(effect id)` is the executor fader.
pub fn apply(
    state: &mut PerFixture,
    effects: &[Effect],
    running: &dyn Fn(u32) -> bool,
    depth_mul: &dyn Fn(u32) -> f32,
    patch: &Patch,
    t: f64,
) {
    for e in effects {
        if !running(e.id) || e.fixtures.is_empty() {
            continue;
        }
        let depth = (e.depth * depth_mul(e.id)).clamp(0.0, 1.0);
        let n = e.fixtures.len() as f64;
        for (i, &fid) in e.fixtures.iter().enumerate() {
            let Some(pf) = patch.get(fid) else { continue };
            let phase = e.rate as f64 * t + (i as f64 / n) * (e.phase_spread as f64 / 360.0);
            let x = shape_value(e.shape, phase, fid.wrapping_add(e.id.wrapping_mul(7919)));
            let attrs = state.entry(fid).or_default();
            if e.attr == "hue" {
                if !(pf.has_attr("red") && pf.has_attr("green") && pf.has_attr("blue")) {
                    continue;
                }
                let (r, g, b) = hsv_to_rgb(frac(phase) as f32, 1.0, 1.0);
                for (name, target) in [("red", r), ("green", g), ("blue", b)] {
                    let base = attrs.get(name).copied().unwrap_or(0.0);
                    attrs.insert(name.into(), base + (target - base) * depth);
                }
            } else if e.attr == "dimmer" {
                let factor = 1.0 - depth * (0.5 - 0.5 * x);
                if pf.has_dimmer {
                    let base = attrs.get("dimmer").copied().unwrap_or(0.0);
                    attrs.insert("dimmer".into(), (base * factor).clamp(0.0, 1.0));
                } else {
                    for a in COLOR_ATTRS {
                        if pf.has_attr(a) {
                            if let Some(v) = attrs.get_mut(*a) {
                                *v = (*v * factor).clamp(0.0, 1.0);
                            }
                        }
                    }
                }
            } else {
                let base = attrs.get(&e.attr).copied().unwrap_or_else(|| attr_default(&e.attr));
                attrs.insert(e.attr.clone(), (base + x * depth * 0.5).clamp(0.0, 1.0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_in_range() {
        for s in [Shape::Sine, Shape::Triangle, Shape::Saw, Shape::Square, Shape::Random] {
            for i in 0..200 {
                let v = shape_value(s, i as f64 * 0.037, 3);
                assert!((-1.0..=1.0).contains(&v), "{s:?} {v}");
            }
        }
        assert!((shape_value(Shape::Sine, 0.25, 0) - 1.0).abs() < 1e-6);
        assert_eq!(shape_value(Shape::Triangle, 0.25, 0), 1.0);
        assert_eq!(shape_value(Shape::Triangle, 0.75, 0), -1.0);
        assert_eq!(shape_value(Shape::Square, 0.1, 0), 1.0);
        assert_eq!(shape_value(Shape::Square, 0.6, 0), -1.0);
    }

    #[test]
    fn hsv() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), (1.0, 0.0, 0.0));
        let (r, g, b) = hsv_to_rgb(1.0 / 3.0, 1.0, 1.0);
        assert!(r < 0.01 && (g - 1.0).abs() < 0.01 && b < 0.01);
    }
}
