//! Pixel mapping: treat a row / grid of fixtures (LED bars, pixel tubes, a wall of pars) as the pixels of a small
//! screen and paint patterns across it — gradients, rainbows, waves, chases, noise, strobes. Each running map writes
//! red / green / blue (and dimmer) of its fixtures as a playback layer: above cuelists and scenes, below the programmer.

use super::*;

fn fract(x: f32) -> f32 {
    x - x.floor()
}
fn tri(x: f32) -> f32 {
    1.0 - (2.0 * fract(x) - 1.0).abs()
}
fn lerp3(a: [f32; 3], b: [f32; 3], m: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * m, a[1] + (b[1] - a[1]) * m, a[2] + (b[2] - a[2]) * m]
}
fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = fract(h) * 6.0;
    let (i, f) = (h.floor() as i32, h - h.floor());
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}
/// Deterministic lattice noise in 0..1.
fn lattice(ix: i32, iy: i32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(374_761_393) ^ (iy as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xFFFF) as f32 / 65535.0
}
fn value_noise(x: f32, y: f32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (ix, iy) = (x0 as i32, y0 as i32);
    let a = lattice(ix, iy) + (lattice(ix + 1, iy) - lattice(ix, iy)) * s(fx);
    let b = lattice(ix, iy + 1) + (lattice(ix + 1, iy + 1) - lattice(ix, iy + 1)) * s(fx);
    a + (b - a) * s(fy)
}

/// Colour (0..1 RGB, before `level`) of the pixel at `(x, y)` ∈ [0,1]² at time `t` seconds.
pub fn pixel_color(m: &PixelMap, x: f32, y: f32, t: f32) -> [f32; 3] {
    let a = m.angle.to_radians();
    let u = (x - 0.5) * a.cos() + (y - 0.5) * a.sin() + 0.5; // position along the travel direction
    let phase = u * m.size - t * m.speed;
    match m.kind {
        PixelKind::Solid => m.color_a,
        PixelKind::Gradient => lerp3(m.color_a, m.color_b, tri(phase)),
        PixelKind::Rainbow => hsv(phase, 1.0, 1.0),
        PixelKind::Wave => lerp3(m.color_b, m.color_a, 0.5 + 0.5 * (std::f32::consts::TAU * phase).sin()),
        PixelKind::Chase => {
            // a bar of `width` of every cycle, soft edges
            let f = fract(phase);
            let w = m.width.clamp(0.02, 1.0);
            let edge = (m.softness.clamp(0.0, 1.0) * w * 0.5).max(1e-3);
            let d = (f - w * 0.5).abs().min((f + 1.0 - w * 0.5).abs()).min((f - 1.0 - w * 0.5).abs());
            let k = ((w * 0.5 - d) / edge).clamp(0.0, 1.0);
            lerp3(m.color_b, m.color_a, k)
        }
        PixelKind::Noise => lerp3(m.color_b, m.color_a, value_noise(x * m.size * 3.0 - t * m.speed, y * m.size * 3.0 + t * m.speed * 0.37)),
        PixelKind::Strobe => lerp3(m.color_b, m.color_a, if fract(t * m.speed) < 0.5 { 1.0 } else { 0.0 }),
    }
}

/// `(x, y)` of pixel `i` of `n` on a grid `cols` wide (rows of a zig-zag wiring run backwards).
pub fn pixel_xy(i: usize, n: usize, cols: usize, zigzag: bool) -> (f32, f32) {
    let cols = cols.max(1);
    let rows = ((n + cols - 1) / cols).max(1);
    let (mut col, row) = (i % cols, i / cols);
    if zigzag && row % 2 == 1 {
        col = cols - 1 - col;
    }
    ((col as f32 + 0.5) / cols as f32, if rows == 1 { 0.5 } else { (row as f32 + 0.5) / rows as f32 })
}

impl Engine {
    pub(super) fn apply_pixelmaps(&self, merged: &mut PerFixture, now: f64) {
        if self.pixels_on.is_empty() {
            return;
        }
        for m in self.show.pixelmaps.iter().filter(|m| self.pixels_on.contains(&m.id)) {
            let n = m.fixtures.len();
            let level = m.level.clamp(0.0, 1.0);
            for (i, id) in m.fixtures.iter().enumerate() {
                let (x, y) = pixel_xy(i, n, m.width_px.max(1) as usize, m.zigzag);
                let c = pixel_color(m, x, y, now as f32);
                let e = merged.entry(*id).or_default();
                e.insert("red".into(), (c[0] * level).clamp(0.0, 1.0));
                e.insert("green".into(), (c[1] * level).clamp(0.0, 1.0));
                e.insert("blue".into(), (c[2] * level).clamp(0.0, 1.0));
                if m.drive_dimmer {
                    e.insert("dimmer".into(), 1.0);
                }
            }
        }
    }

    pub(super) fn pixel_save(&mut self, mut m: PixelMap) -> Result<(), String> {
        if m.fixtures.is_empty() {
            return Err("El mapa de píxeles necesita al menos un aparato".into());
        }
        if m.fixtures.len() > 2048 {
            return Err("Demasiados aparatos (máximo 2048)".into());
        }
        if let Some(bad) = m.fixtures.iter().find(|f| !self.show.fixtures.iter().any(|x| x.id == **f)) {
            return Err(format!("El aparato {bad} no existe"));
        }
        m.width_px = m.width_px.clamp(1, 512);
        m.level = m.level.clamp(0.0, 1.0);
        m.size = m.size.clamp(0.05, 64.0);
        if !m.speed.is_finite() || !m.angle.is_finite() {
            return Err("Velocidad o ángulo no válidos".into());
        }
        m.speed = m.speed.clamp(-50.0, 50.0);
        m.name = m.name.chars().take(60).collect();
        if m.id == 0 {
            m.id = next_id(self.show.pixelmaps.iter().map(|p| p.id));
        }
        match self.show.pixelmaps.iter_mut().find(|p| p.id == m.id) {
            Some(p) => *p = m,
            None => self.show.pixelmaps.push(m),
        }
        self.touch();
        Ok(())
    }

    pub(super) fn pixel_remove(&mut self, id: u32) {
        self.show.pixelmaps.retain(|p| p.id != id);
        self.pixels_on.remove(&id);
        self.touch();
    }

    pub(super) fn pixel_run(&mut self, id: u32, on: bool) -> Result<(), String> {
        if !self.show.pixelmaps.iter().any(|p| p.id == id) {
            return Err(format!("El mapa de píxeles {id} no existe"));
        }
        if on {
            self.pixels_on.insert(id);
        } else {
            self.pixels_on.remove(&id);
        }
        self.dirty = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(kind: PixelKind) -> PixelMap {
        PixelMap { id: 1, name: "t".into(), fixtures: vec![], width_px: 8, zigzag: false, kind, color_a: [1.0, 0.0, 0.0], color_b: [0.0, 0.0, 1.0], speed: 0.0, size: 1.0, angle: 0.0, width: 0.25, softness: 0.0, level: 1.0, drive_dimmer: true }
    }

    #[test]
    fn grid_coordinates_and_zigzag_wiring() {
        assert_eq!(pixel_xy(0, 8, 8, false), (0.0625, 0.5));
        assert_eq!(pixel_xy(7, 8, 8, false).0, 0.9375);
        // 2 rows of 4: the second row is wired backwards
        assert_eq!(pixel_xy(4, 8, 4, true).0, 0.875, "pixel 4 is at the right end of row 2");
        assert_eq!(pixel_xy(4, 8, 4, false).0, 0.125);
        assert_eq!(pixel_xy(0, 8, 4, false).1, 0.25);
        assert_eq!(pixel_xy(7, 8, 4, false).1, 0.75);
    }

    #[test]
    fn solid_gradient_and_direction() {
        assert_eq!(pixel_color(&map(PixelKind::Solid), 0.3, 0.5, 9.0), [1.0, 0.0, 0.0]);
        let g = map(PixelKind::Gradient);
        let (l, c, r) = (pixel_color(&g, 0.0, 0.5, 0.0), pixel_color(&g, 0.5, 0.5, 0.0), pixel_color(&g, 1.0, 0.5, 0.0));
        assert!(l[0] > 0.9 && c[2] > 0.9 && r[0] > 0.9, "A → B → A across one cycle: {l:?} {c:?} {r:?}");
        // travelling: at speed 1 the pattern moves one full cycle per second
        let mut m = map(PixelKind::Gradient);
        m.speed = 1.0;
        let a = pixel_color(&m, 0.2, 0.5, 0.0);
        let b = pixel_color(&m, 0.2, 0.5, 1.0);
        assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3), "period of one second");
        let c = pixel_color(&m, 0.2, 0.5, 0.25);
        assert!(a.iter().zip(c).any(|(x, y)| (x - y).abs() > 0.1));
        // angle 90°: varies along y, constant along x
        let mut v = map(PixelKind::Gradient);
        v.angle = 90.0;
        let (p, q) = (pixel_color(&v, 0.1, 0.5, 0.0), pixel_color(&v, 0.9, 0.5, 0.0));
        assert!(p.iter().zip(q).all(|(x, y)| (x - y).abs() < 1e-4), "{p:?} {q:?}");
        assert_ne!(pixel_color(&v, 0.5, 0.1, 0.0), pixel_color(&v, 0.5, 0.5, 0.0));
    }

    #[test]
    fn rainbow_covers_the_hue_circle_and_chase_is_a_moving_bar() {
        let mut r = map(PixelKind::Rainbow);
        r.size = 1.0;
        let c0 = pixel_color(&r, 0.0, 0.5, 0.0);
        let c1 = pixel_color(&r, 1.0 / 3.0, 0.5, 0.0);
        let c2 = pixel_color(&r, 2.0 / 3.0, 0.5, 0.0);
        assert!(c0[0] > 0.9 && c1[1] > 0.9 && c2[2] > 0.9, "red → green → blue at thirds of the cycle (starting at x = 0.5 − …): {c0:?} {c1:?} {c2:?}");
        let mut ch = map(PixelKind::Chase);
        ch.speed = 1.0;
        let lit = |t: f32| -> Vec<usize> { (0..8).filter(|i| pixel_color(&ch, pixel_xy(*i, 8, 8, false).0, 0.5, t)[0] > 0.9).collect() };
        let (a, b) = (lit(0.0), lit(0.25));
        assert!(!a.is_empty() && a.len() <= 3, "a short bar: {a:?}");
        assert_ne!(a, b, "and it moves");
    }

    #[test]
    fn noise_is_smooth_bounded_and_deterministic() {
        let n = map(PixelKind::Noise);
        for i in 0..50 {
            let (x, t) = (i as f32 / 50.0, i as f32 * 0.07);
            let c = pixel_color(&n, x, 0.5, t);
            assert!((0.0..=1.0).contains(&c[0]) && (0.0..=1.0).contains(&c[2]));
            assert_eq!(c, pixel_color(&n, x, 0.5, t));
        }
        let (a, b) = (pixel_color(&n, 0.500, 0.5, 0.0), pixel_color(&n, 0.502, 0.5, 0.0));
        assert!((a[0] - b[0]).abs() < 0.05, "smooth");
    }
}

#[cfg(test)]
mod engine_tests {
    use super::super::tests_support::*;
    use super::*;

    fn base() -> PixelMap {
        PixelMap { id: 0, name: "Arcoíris".into(), fixtures: vec![1, 2, 3, 4], width_px: 4, zigzag: false, kind: PixelKind::Rainbow, color_a: [1.0, 0.0, 0.0], color_b: [0.0, 0.0, 1.0], speed: 0.0, size: 1.0, angle: 0.0, width: 0.25, softness: 0.0, level: 1.0, drive_dimmer: true }
    }

    #[test]
    fn a_running_map_paints_its_fixtures_and_stops_cleanly() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 4);
        e.handle(Command::PixelSave { map: base() }).unwrap();
        let id = e.show.pixelmaps[0].id;
        assert_eq!(id, 1);
        let off = e.compute_look(0.0);
        assert!(off.get(&1).map_or(true, |a| !a.contains_key("red")), "not running: untouched");
        e.handle(Command::PixelRun { id, on: true }).unwrap();
        assert_eq!(e.build_rt().pixels_running, vec![1]);
        let look = e.compute_look(0.0);
        let rgb = |f: u32| { let a = &look[&f]; (a["red"], a["green"], a["blue"]) };
        assert_ne!(rgb(1), rgb(3), "each pixel has its own colour");
        assert!(look.values().all(|a| a["dimmer"] == 1.0), "drive_dimmer");
        // the programmer still wins
        set(&mut e, 1, "red", 0.0);
        assert_eq!(e.compute_look(0.0)[&1]["red"], 0.0);
        e.handle(Command::PixelRun { id, on: false }).unwrap();
        assert!(e.compute_look(0.0).get(&2).map_or(true, |a| !a.contains_key("blue")));
    }

    #[test]
    fn validation_and_console() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 2);
        let mut m = base();
        m.fixtures = vec![];
        assert!(e.handle(Command::PixelSave { map: m }).unwrap_err().contains("al menos un aparato"));
        let mut m = base();
        m.fixtures = vec![1, 99];
        assert!(e.handle(Command::PixelSave { map: m }).unwrap_err().contains("99"));
        let mut m = base();
        m.fixtures = vec![1, 2];
        m.speed = f32::NAN;
        assert!(e.handle(Command::PixelSave { map: m }).is_err());
        assert!(e.handle(Command::PixelRun { id: 5, on: true }).is_err());
        let mut m = base();
        m.fixtures = vec![1, 2];
        m.size = 0.0;
        m.width_px = 0;
        e.handle(Command::PixelSave { map: m }).unwrap();
        assert!(e.show.pixelmaps[0].size >= 0.05 && e.show.pixelmaps[0].width_px == 1, "clamped");
        e.handle(Command::CommandLine { text: "pixel 1 on".into() }).unwrap();
        assert!(e.pixels_on.contains(&1));
        e.handle(Command::CommandLine { text: "pixel 1 off".into() }).unwrap();
        assert!(e.pixels_on.is_empty());
        e.handle(Command::PixelRemove { id: 1 }).unwrap();
        assert!(e.show.pixelmaps.is_empty());
    }
}
