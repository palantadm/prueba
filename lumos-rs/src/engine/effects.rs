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
        // Circle / Eight are 2-D (see `shape_xy`); driving a single attribute they behave like a sine.
        Shape::Sine | Shape::Circle | Shape::Eight => (phase * std::f64::consts::TAU).sin(),
        // Falling ramp: starts at +1, drops to -1, then snaps back.
        Shape::Ramp => 1.0 - 2.0 * f,
        // Short flash: +1 for the first 15 % of the cycle, -1 otherwise.
        Shape::Pulse => {
            if f < 0.15 {
                1.0
            } else {
                -1.0
            }
        }
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

/// Cycles per second: its speed master's BPM when it has one, else its own BPM (>0), else `rate` (Hz).
/// `master_bpm` is the already-resolved BPM of `e.speed_master` (`None` if unset or unknown).
pub fn cycles_per_sec(e: &Effect, master_bpm: Option<f32>) -> f64 {
    if let Some(b) = master_bpm {
        return b as f64 / 60.0;
    }
    if e.bpm > 0.0 {
        return e.bpm as f64 / 60.0;
    }
    e.rate as f64
}

/// Unit offset (-1..1 on both axes) of a 2-D shape at `phase` cycles, before rotation.
/// Circle starts at (1, 0) and runs counter-clockwise; Eight is the figure-eight x = sin t, y = sin 2t
/// (crosses the centre at t = 0 and pi). 1-D shapes return `(shape_value, 0)`.
pub fn shape_xy(shape: Shape, phase: f64) -> (f32, f32) {
    let th = phase * std::f64::consts::TAU;
    match shape {
        Shape::Circle => (th.cos() as f32, th.sin() as f32),
        Shape::Eight => (th.sin() as f32, (2.0 * th).sin() as f32),
        s => (shape_value(s, phase, 0), 0.0),
    }
}

pub fn rotate(x: f32, y: f32, deg: f32) -> (f32, f32) {
    let (s, c) = (deg.to_radians().sin(), deg.to_radians().cos());
    (x * c - y * s, x * s + y * c)
}

fn is_2d(e: &Effect) -> bool {
    matches!(e.shape, Shape::Circle | Shape::Eight) && e.attr2.is_some() && e.attr != "hue"
}

/// Apply every running effect to `state`.
/// * `depth_mul(effect id)` is the executor fader.
/// * `phase_of(effect id)` is the effect's accumulated phase in cycles (see `Engine::effect_phases`).
///
/// Amplitude: `depth` 1 swings +-0.5 of the attribute range around its current value (1-D shapes) or
/// draws a circle of radius 0.5 (2-D shapes). `phase_spread` offsets the fixtures evenly.
pub fn apply(
    state: &mut PerFixture,
    effects: &[Effect],
    running: &dyn Fn(u32) -> bool,
    depth_mul: &dyn Fn(u32) -> f32,
    phase_of: &dyn Fn(u32) -> f64,
    patch: &Patch,
) {
    for e in effects {
        if !running(e.id) || e.fixtures.is_empty() {
            continue;
        }
        let depth = (e.depth * depth_mul(e.id)).clamp(0.0, 1.0);
        let n = e.fixtures.len() as f64;
        let base_phase = phase_of(e.id);
        for (i, &fid) in e.fixtures.iter().enumerate() {
            let Some(pf) = patch.get(fid) else { continue };
            let phase = base_phase + (i as f64 / n) * (e.phase_spread as f64 / 360.0);
            let attrs = state.entry(fid).or_default();
            if is_2d(e) {
                let y_attr = e.attr2.as_deref().unwrap_or_default();
                if !(pf.has_attr(&e.attr) && pf.has_attr(y_attr)) {
                    continue;
                }
                let (x, y) = shape_xy(e.shape, phase);
                let (x, y) = rotate(x, y, e.rotation);
                for (name, d) in [(e.attr.as_str(), x), (y_attr, y)] {
                    let base = attrs.get(name).copied().unwrap_or_else(|| attr_default(name));
                    attrs.insert(name.into(), (base + d * depth * 0.5).clamp(0.0, 1.0));
                }
                continue;
            }
            let x = shape_value(e.shape, phase, fid.wrapping_add(e.id.wrapping_mul(7919)));
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
        for s in [Shape::Sine, Shape::Triangle, Shape::Saw, Shape::Square, Shape::Random, Shape::Circle, Shape::Eight, Shape::Ramp, Shape::Pulse] {
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

    #[test]
    fn ramp_and_pulse() {
        assert_eq!(shape_value(Shape::Ramp, 0.0, 0), 1.0);
        assert!((shape_value(Shape::Ramp, 0.5, 0)).abs() < 1e-6);
        assert!(shape_value(Shape::Ramp, 0.999, 0) < -0.99);
        assert_eq!(shape_value(Shape::Pulse, 0.05, 0), 1.0);
        assert_eq!(shape_value(Shape::Pulse, 0.2, 0), -1.0);
    }

    #[test]
    fn circle_has_constant_radius_and_rotation_keeps_it() {
        for i in 0..100 {
            let (x, y) = shape_xy(Shape::Circle, i as f64 * 0.0137);
            assert!(((x * x + y * y).sqrt() - 1.0).abs() < 1e-5);
            let (rx, ry) = rotate(x, y, 37.0);
            assert!(((rx * rx + ry * ry).sqrt() - 1.0).abs() < 1e-5);
        }
        let (x, y) = shape_xy(Shape::Circle, 0.0);
        assert!((x - 1.0).abs() < 1e-6 && y.abs() < 1e-6);
        let (x, y) = shape_xy(Shape::Circle, 0.25);
        assert!(x.abs() < 1e-5 && (y - 1.0).abs() < 1e-5);
        // 90 degrees maps (1, 0) onto (0, 1)
        let (x, y) = rotate(1.0, 0.0, 90.0);
        assert!(x.abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn figure_eight_crosses_at_the_centre_twice_per_cycle() {
        let (x, y) = shape_xy(Shape::Eight, 0.0);
        assert!(x.abs() < 1e-6 && y.abs() < 1e-6);
        let (x, y) = shape_xy(Shape::Eight, 0.5);
        assert!(x.abs() < 1e-5 && y.abs() < 1e-5);
        // lobes: x extreme at a quarter cycle with y = 0, y extreme at an eighth with x = sin(45deg)
        let (x, y) = shape_xy(Shape::Eight, 0.25);
        assert!((x - 1.0).abs() < 1e-5 && y.abs() < 1e-5);
        let (x, y) = shape_xy(Shape::Eight, 0.125);
        assert!((x - 0.7071).abs() < 1e-3 && (y - 1.0).abs() < 1e-5);
        // the two lobes mirror each other left/right: x flips sign half a cycle later, y repeats
        let (x1, y1) = shape_xy(Shape::Eight, 0.125);
        let (x2, y2) = shape_xy(Shape::Eight, 0.625);
        assert!((x1 + x2).abs() < 1e-5 && (y1 - y2).abs() < 1e-5);
    }

    #[test]
    fn cycles_per_sec_priority() {
        let mut e = Effect { id: 1, name: String::new(), attr: "dimmer".into(), shape: Shape::Sine, rate: 2.0, depth: 1.0, phase_spread: 0.0, fixtures: vec![], attr2: None, speed_master: None, bpm: 0.0, rotation: 0.0 };
        assert_eq!(cycles_per_sec(&e, None), 2.0);
        e.bpm = 90.0;
        assert_eq!(cycles_per_sec(&e, None), 1.5);
        assert_eq!(cycles_per_sec(&e, Some(120.0)), 2.0); // speed master beats bpm and rate
    }
}

#[cfg(test)]
mod engine_tests {
    use super::super::tests_support::*;
    use super::super::*;

    fn fx(shape: Shape) -> Effect {
        Effect {
            id: 0, name: "fx".into(), attr: "pan".into(), shape, rate: 1.0, depth: 0.4, phase_spread: 0.0, fixtures: vec![1],
            attr2: Some("tilt".into()), speed_master: None, bpm: 0.0, rotation: 0.0,
        }
    }

    fn start(e: &mut Engine, fx: Effect) {
        e.handle(Command::EffectSave { effect: fx }).unwrap();
        let id = e.show.effects.last().unwrap().id;
        e.handle(Command::EffectToggle { id, on: true }).unwrap();
    }

    fn pan_tilt(e: &mut Engine, now: f64) -> (f32, f32) {
        let m = e.compute_look(now);
        (m[&1]["pan"], m[&1]["tilt"])
    }

    #[test]
    fn circle_on_a_mover_keeps_its_radius() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 1);
        start(&mut e, fx(Shape::Circle));
        for i in 0..40 {
            let (p, t) = pan_tilt(&mut e, 0.1 + i as f64 * 0.037);
            let r = ((p - 0.5).powi(2) + (t - 0.5).powi(2)).sqrt();
            assert!((r - 0.2).abs() < 1e-4, "radius {r} at step {i}"); // depth 0.4 → radius 0.2
        }
    }

    #[test]
    fn rotation_turns_the_start_point() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 1);
        let mut f = fx(Shape::Circle);
        f.rotation = 90.0;
        start(&mut e, f);
        // first call initialises the phase clock at rate·now; sample exactly at phase 0 (now = 0)
        let (p, t) = pan_tilt(&mut e, 0.0);
        assert!((p - 0.5).abs() < 1e-4 && (t - 0.7).abs() < 1e-4, "{p} {t}"); // (1,0) rotated 90° → straight up
    }

    #[test]
    fn figure_eight_crosses_the_centre() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 1);
        start(&mut e, fx(Shape::Eight));
        let (p, t) = pan_tilt(&mut e, 0.0);
        assert!((p - 0.5).abs() < 1e-4 && (t - 0.5).abs() < 1e-4);
        let (p, t) = pan_tilt(&mut e, 0.5); // half a cycle later: centre again, other lobe next
        assert!((p - 0.5).abs() < 1e-3 && (t - 0.5).abs() < 1e-3, "{p} {t}");
        let (p, t) = pan_tilt(&mut e, 0.625); // 0.125 cycle after the crossing: x = -sin45°·r, y = r
        assert!((p - (0.5 - 0.2 * 0.7071)).abs() < 2e-3 && (t - 0.7).abs() < 2e-3, "{p} {t}");
    }

    #[test]
    fn speed_master_drives_the_rate_without_phase_jumps() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 1);
        e.handle(Command::SpeedSave { master: SpeedMaster { id: 0, name: "M".into(), bpm: 120.0 } }).unwrap();
        let mut f = fx(Shape::Saw);
        f.attr = "tilt".into();
        f.attr2 = None;
        f.depth = 1.0;
        f.rate = 5.0; // ignored: the master wins
        f.speed_master = Some(1);
        start(&mut e, f);
        let m = e.compute_look(0.1); // clock starts at phase cps·now = 2 · 0.1 = 0.2
        assert!((m[&1]["tilt"] - (0.5 + (2.0 * 0.2 - 1.0) * 0.5)).abs() < 1e-3);
        let m = e.compute_look(0.35); // 120 BPM = 2 Hz → phase 0.7
        assert!((m[&1]["tilt"] - (0.5 + (2.0 * 0.7 - 1.0) * 0.5)).abs() < 1e-3);
        e.handle(Command::SpeedSet { id: 1, bpm: 60.0 }).unwrap(); // halve the tempo
        let m = e.compute_look(0.6); // continues from 0.7 at 1 Hz → 0.95 (an absolute rate·t clock would give 0.6)
        assert!((m[&1]["tilt"] - (0.5 + (2.0 * 0.95 - 1.0) * 0.5)).abs() < 1e-3, "{}", m[&1]["tilt"]);
    }

    #[test]
    fn own_bpm_beats_rate() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 1);
        let mut f = fx(Shape::Saw);
        f.attr = "tilt".into();
        f.attr2 = None;
        f.depth = 1.0;
        f.rate = 9.0;
        f.bpm = 60.0; // 1 Hz
        start(&mut e, f);
        let _ = e.compute_look(0.0);
        let m = e.compute_look(0.25);
        assert!((m[&1]["tilt"] - (0.5 + (2.0 * 0.25 - 1.0) * 0.5)).abs() < 1e-3);
    }

    #[test]
    fn two_dimensional_effect_needs_both_attributes() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1); // no pan/tilt
        start(&mut e, fx(Shape::Circle));
        let m = e.compute_look(0.2);
        assert!(m.get(&1).map_or(true, |a| !a.contains_key("pan") && !a.contains_key("tilt")));
    }

    #[test]
    fn ramp_and_pulse_on_dimmer_through_the_engine() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        set(&mut e, 1, "dimmer", 1.0);
        let mut f = fx(Shape::Pulse);
        f.attr = "dimmer".into();
        f.attr2 = None;
        f.depth = 1.0;
        start(&mut e, f);
        let _ = e.compute_look(0.0); // phase 0 → pulse high → full
        e.render(0.05);
        assert_eq!(e.frames[&1][0], 255);
        e.render(0.5); // pulse low → factor 1 − depth = 0
        assert_eq!(e.frames[&1][0], 0);
    }
}

