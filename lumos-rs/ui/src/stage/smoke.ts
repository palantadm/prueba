// Fog / haze / low-fog simulation. Pure TypeScript — no dependency on three.js or the DOM scene, so the 3D
// visualiser, the 2D plan and tests can all share one instance (`getSmoke()`).
//
// Model: three smoke species (fog, haze, low fog) on a coarse 3D density grid, plus a small pool of
// Lagrangian "puff" particles that carry the machine's jet. Particles give the plume its direction,
// hot-rise/cool-sink buoyancy and turbulent meander, and deposit their mass into the grid; the grid then
// advects (semi-Lagrangian, block-constant ambient air currents + density-driven buoyancy), diffuses and
// dissipates. Walls, floor and ceiling are closed (smoke pools under the ceiling) unless a vent is enabled.
import type { Fixture, FixtureProfile } from "../types";

export type SmokeKind = "fog" | "haze" | "low";
export type Currents = "off" | "low" | "med" | "high";
export type Hvac = "none" | "down" | "up";

export interface SmokeCfg {
  currents: Currents;
  /** compass-style degrees: 0 = towards the audience (+Z), 90 = towards stage right (+X) */
  windDir: number;
  hvac: Hvac;
  /** multiplies every decay rate */
  dissipation: number;
  vent: boolean;
}

export const DEFAULT_CFG: SmokeCfg = { currents: "low", windDir: 0, hvac: "none", dissipation: 1, vent: false };

export interface GridSize { nx: number; ny: number; nz: number }
export const GRIDS: Record<"low" | "med" | "high", GridSize> = {
  low: { nx: 32, ny: 16, nz: 24 },
  med: { nx: 48, ny: 24, nz: 36 },
  high: { nx: 64, ny: 32, nz: 48 },
};

interface KindParams {
  /** mass units emitted per second at full output (density × m³ / s) */
  rate: number;
  /** initial jet speed, m/s */
  jet: number;
  /** half-angle of the exhaust cone, rad */
  spread: number;
  /** buoyant acceleration when freshly exhausted / after cooling (m/s², + = up) */
  buoy0: number;
  buoyInf: number;
  /** cooling time constant, s */
  cool: number;
  /** relaxation rate towards the surrounding air velocity (entrainment + drag), 1/s */
  drag: number;
  /** time constant for a puff to hand its mass to the grid, s */
  depTau: number;
  /** half-life of the grid density, s */
  halfLife: number;
  /** horizontal / vertical diffusivity, m²/s */
  dH: number;
  dV: number;
  /** drift speed of dense smoke in the grid (+ up), m/s */
  gridBuoy: number;
  /** relative puff emission rate */
  puffs: number;
}

export const KIND: Record<SmokeKind, KindParams> = {
  // heated glycol: dense and hot — rises in a jet, then cools, spreads and sinks slightly; thins out in 1–3 min
  fog: { rate: 5.0, jet: 3.0, spread: 0.3, buoy0: 1.2, buoyInf: -0.22, cool: 5, drag: 0.9, depTau: 2.4, halfLife: 80, dH: 0.04, dV: 0.03, gridBuoy: -0.012, puffs: 1.5 },
  // oil / glycol haze: very fine, neutrally buoyant, builds an even layer that lingers for 10–20 min
  haze: { rate: 0.9, jet: 1.1, spread: 0.38, buoy0: 0.12, buoyInf: 0.004, cool: 3, drag: 1.3, depTau: 1.6, halfLife: 540, dH: 0.11, dV: 0.08, gridBuoy: 0.004, puffs: 0.9 },
  // dry-ice / chilled low fog: heavy, hugs the floor and flows like a liquid
  low: { rate: 4.0, jet: 1.3, spread: 0.5, buoy0: -0.5, buoyInf: -0.9, cool: 1, drag: 1.0, depTau: 1.8, halfLife: 55, dH: 0.12, dV: 0.004, gridBuoy: -0.12, puffs: 1.2 },
};
const SPECIES: SmokeKind[] = ["fog", "haze", "low"];

/** Nozzle position / direction in the fixture's local frame (hanging convention: origin at the top of the body). */
export const NOZZLE_LOCAL = { pos: [0, -0.13, 0.2] as [number, number, number], dir: normalize3([0, 0.3, 1]) };

function normalize3(v: [number, number, number]): [number, number, number] {
  const l = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / l, v[1] / l, v[2] / l];
}

/** Which kind of smoke machine a profile is, or null when it is not one. */
export function machineKind(prof: FixtureProfile | undefined): SmokeKind | null {
  if (!prof) return null;
  const txt = `${prof.id} ${prof.manufacturer} ${prof.model} ${prof.category}`.toLowerCase();
  if (/low[\s_-]?(fog|lying|smoke)|dry[\s_-]?ice|cryo|\bco2\b|\bice\b/.test(txt)) return "low";
  if (/haze|hazer/.test(txt)) return "haze";
  if (/\bfog|smoke|vapou?r|fogger/.test(txt)) return "fog";
  return null;
}

/** 0..1 output level of a machine from its DMX frame (`control` 0–9 = off, 10–255 = output; `speed` scales it). */
export function machineOutput(prof: FixtureProfile, fx: Fixture, frame: Uint8Array | undefined): number {
  if (!frame) return 0;
  const mode = prof.modes.find((m) => m.name === fx.mode) ?? prof.modes[0];
  if (!mode) return 0;
  const base = fx.address - 1;
  const val = (attr: string): number | null => {
    const c = mode.channels.find((ch) => ch.attr === attr);
    if (!c) return null;
    const i = base + c.offset;
    return i >= 0 && i < frame.length ? frame[i] : 0;
  };
  let out = 0;
  const ctl = val("control");
  if (ctl !== null) out = ctl >= 10 ? 0.1 + 0.9 * ((ctl - 10) / 245) : 0;
  else {
    const dim = val("dimmer") ?? val("effect");
    if (dim !== null) out = dim >= 10 ? dim / 255 : 0;
    else {
      const first = mode.channels[0];
      const v = first ? frame[base + first.offset] ?? 0 : 0;
      out = v >= 10 ? 0.1 + 0.9 * ((v - 10) / 245) : 0;
    }
  }
  const sp = val("speed");
  if (sp !== null && out > 0) out *= 0.25 + 0.75 * (sp / 255);
  return Math.max(0, Math.min(1, out));
}

// ── deterministic helpers ────────────────────────────────
function mulberry32(a: number) {
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function hash3(x: number, y: number, z: number): number {
  let h = Math.imul(x | 0, 374761393) ^ Math.imul(y | 0, 668265263) ^ Math.imul(z | 0, 1274126177);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

function vnoise(x: number, y: number, z: number): number {
  const xi = Math.floor(x), yi = Math.floor(y), zi = Math.floor(z);
  let fx = x - xi, fy = y - yi, fz = z - zi;
  fx = fx * fx * (3 - 2 * fx); fy = fy * fy * (3 - 2 * fy); fz = fz * fz * (3 - 2 * fz);
  const a = hash3(xi, yi, zi), b = hash3(xi + 1, yi, zi), c = hash3(xi, yi + 1, zi), d = hash3(xi + 1, yi + 1, zi);
  const e = hash3(xi, yi, zi + 1), f = hash3(xi + 1, yi, zi + 1), g = hash3(xi, yi + 1, zi + 1), h = hash3(xi + 1, yi + 1, zi + 1);
  const x0 = a + (b - a) * fx, x1 = c + (d - c) * fx, x2 = e + (f - e) * fx, x3 = g + (h - g) * fx;
  const y0 = x0 + (x1 - x0) * fy, y1 = x2 + (x3 - x2) * fy;
  return y0 + (y1 - y0) * fz;
}

/** Same swirl with the amplitude normalisation used by the room air field. */
function curl3s(X: number, Y: number, Z: number, e: number): [number, number, number] {
  const c = curl3(X, Y, Z, e);
  const k = 2 * e;
  return [c[0] * k, c[1] * k, c[2] * k];
}

/** Curl of a vector value-noise potential (divergence-free swirl), ~±0.35 magnitude. */
function curl3(X: number, Y: number, Z: number, e: number): [number, number, number] {
  const dPzdy = vnoise(X + 11, Y + e, Z + 3) - vnoise(X + 11, Y - e, Z + 3);
  const dPydz = vnoise(X + 5, Y + 7, Z + e) - vnoise(X + 5, Y + 7, Z - e);
  const dPxdz = vnoise(X, Y, Z + e + 20) - vnoise(X, Y, Z - e + 20);
  const dPzdx = vnoise(X + e + 11, Y, Z + 3) - vnoise(X - e + 11, Y, Z + 3);
  const dPydx = vnoise(X + e + 5, Y + 7, Z) - vnoise(X - e + 5, Y + 7, Z);
  const dPxdy = vnoise(X, Y + e, Z + 20) - vnoise(X, Y - e, Z + 20);
  const k = 1 / (2 * e);
  return [(dPzdy - dPydz) * k, (dPxdz - dPzdx) * k * 0.6, (dPydx - dPxdy) * k];
}

const AMP: Record<Currents, number> = { off: 0, low: 0.05, med: 0.12, high: 0.26 };
/** Eddy (turbulent mixing) diffusivity of the room air, m²/s — stirs fog/haze into an even layer over minutes. */
const EDDY: Record<Currents, number> = { off: 0.01, low: 0.08, med: 0.2, high: 0.5 };
const MAX_PART = 7000;
const PART_RATE = 46;
/** puff lifetime: they stay visible as fine billows for a long time before melting into the room-scale grid */
const LIFE = 26;
const B = 4; // ambient-air block size in cells

interface Machine {
  id: number;
  kind: SmokeKind;
  pos: [number, number, number];
  dir: [number, number, number];
  target: number;
  out: number;
  onTime: number;
  acc: number;
}

export interface SmokeStats {
  mass: number;
  /** mass still carried by particles (not yet in the grid) */
  inFlight: number;
  emitted: number[];
  massFog: number;
  massHaze: number;
  fineMass: number;
  massLow: number;
  maxDensity: number;
  meanDensity: number;
  /** mass-weighted mean height of the low-fog species, m */
  lowMeanY: number;
  particles: number;
  machines: number;
  grid: [number, number, number];
  stepMs: number;
  time: number;
}

export class SmokeSim {
  nx = 48; ny = 24; nz = 36;
  min: [number, number, number] = [-8.5, 0, -4.6];
  size: [number, number, number] = [17, 7.5, 15];
  cfg: SmokeCfg = { ...DEFAULT_CFG };
  /** bumps whenever the density field changed (for texture uploads) */
  version = 0;
  time = 0;
  stepMs = 0;

  private f: Float32Array[] = [];
  private tmp: Float32Array = new Float32Array(0);
  private mass = [0, 0, 0];
  /** total mass ever emitted per species (conservation checks) */
  emitted = [0, 0, 0];
  private amb = new Float32Array(0);
  private bmean = new Float32Array(0);
  private bvel = new Float32Array(0);
  private tcp = new Float64Array(96);
  private tinv = new Float64Array(96);
  private tdp = new Float64Array(96);
  private starts = new Map<string, number[]>();

  // fine "puff" field: the particles splatted as gaussians at ~0.1–0.2 m resolution (billows, wisps)
  fine = new Float32Array(0);
  fnx = 0; fny = 0; fnz = 0;
  fmin: [number, number, number] = [-7, 0, -4.5];
  fsize: [number, number, number] = [14, 6, 11.5];
  private fineVoxel = 0;
  fineVersion = 0;
  private machines = new Map<number, Machine>();
  private rnd = mulberry32(1234);
  private purgeLeft = 0;
  /** tests: stop real-time stepping so `step()` is the only thing advancing the simulation */
  frozen = false;
  private lastUpdate = 0;
  private simAcc = 0;

  // particles (structure of arrays)
  private px = new Float32Array(MAX_PART); private py = new Float32Array(MAX_PART); private pz = new Float32Array(MAX_PART);
  private vx = new Float32Array(MAX_PART); private vy = new Float32Array(MAX_PART); private vz = new Float32Array(MAX_PART);
  private pm = new Float32Array(MAX_PART); private pa = new Float32Array(MAX_PART); private ps = new Uint8Array(MAX_PART);
  private np = 0;

  constructor(g: GridSize = GRIDS.med) {
    this.alloc(g.nx, g.ny, g.nz);
  }

  // ── geometry ───────────────────────────────────────────
  get cellVol() { return (this.size[0] / this.nx) * (this.size[1] / this.ny) * (this.size[2] / this.nz); }

  private alloc(nx: number, ny: number, nz: number) {
    this.nx = nx; this.ny = ny; this.nz = nz;
    const n = nx * ny * nz;
    this.f = [new Float32Array(n), new Float32Array(n), new Float32Array(n)];
    this.tmp = new Float32Array(n);
    const nb = Math.ceil(nx / B) * Math.ceil(ny / B) * Math.ceil(nz / B);
    this.amb = new Float32Array(nb * 3);
    this.bmean = new Float32Array(nb);
    this.bvel = new Float32Array(nb * 3);
    this.mass = [0, 0, 0];
    this.version++;
  }

  /** Change the grid resolution, resampling the existing density (so quality changes keep the smoke). */
  setGrid(g: GridSize) {
    if (g.nx === this.nx && g.ny === this.ny && g.nz === this.nz) return;
    const old = this.f, onx = this.nx, ony = this.ny, onz = this.nz;
    this.alloc(g.nx, g.ny, g.nz);
    for (let s = 0; s < 3; s++) {
      let sum = 0;
      for (let z = 0; z < this.nz; z++) for (let y = 0; y < this.ny; y++) for (let x = 0; x < this.nx; x++) {
        const v = trilinear(old[s], onx, ony, onz, ((x + 0.5) / this.nx) * onx - 0.5, ((y + 0.5) / this.ny) * ony - 0.5, ((z + 0.5) / this.nz) * onz - 0.5);
        this.f[s][x + this.nx * (y + this.ny * z)] = v;
        sum += v;
      }
      this.mass[s] = sum * this.cellVol;
    }
  }

  /** Room domain derived from the stage size (matches the 3D venue shell). Resets the field when it changes. */
  setStageSize(stage: [number, number, number]) {
    const [W, D, H] = stage;
    const min: [number, number, number] = [-(W / 2 + 2.5), 0, -D / 2 - 0.6];
    const size: [number, number, number] = [W + 5, H + 1.5, D + 0.6 + 7];
    if (min.every((v, i) => Math.abs(v - this.min[i]) < 1e-6) && size.every((v, i) => Math.abs(v - this.size[i]) < 1e-6)) return;
    this.min = min; this.size = size;
    this.fmin = [-(W / 2 + 1), 0, -D / 2 - 0.5];
    this.fsize = [W + 2, H, D + 3.5];
    this.fineVoxel = -1; // force re-allocation on the next setFine()
    this.reset();
  }

  setCfg(c: Partial<SmokeCfg>) { this.cfg = { ...this.cfg, ...c }; }

  /** Enable the fine puff field with the given voxel size in metres (0 = off). */
  setFine(voxel: number) {
    if (voxel === this.fineVoxel) return;
    this.fineVoxel = voxel;
    if (voxel <= 0) { this.fine = new Float32Array(0); this.fnx = this.fny = this.fnz = 0; this.fineVersion++; return; }
    this.fnx = Math.min(160, Math.max(8, Math.ceil(this.fsize[0] / voxel)));
    this.fny = Math.min(100, Math.max(8, Math.ceil(this.fsize[1] / voxel)));
    this.fnz = Math.min(160, Math.max(8, Math.ceil(this.fsize[2] / voxel)));
    this.fine = new Float32Array(this.fnx * this.fny * this.fnz);
    this.rebuildFine();
  }
  get fineOn() { return this.fine.length > 0; }

  private rebuildFine() {
    if (!this.fine.length) return;
    const { fnx, fny, fnz } = this;
    const f = this.fine;
    f.fill(0);
    const dx = this.fsize[0] / fnx, dy = this.fsize[1] / fny, dz = this.fsize[2] / fnz;
    const vol = dx * dy * dz;
    const wx = new Float32Array(9), wy = new Float32Array(9), wz = new Float32Array(9);
    for (let i = 0; i < this.np; i++) {
      const m = this.pm[i];
      if (m < 1e-6) continue;
      const age = this.pa[i];
      // puffs expand as they age (entrainment): ~0.12 m at the nozzle → 0.45 m
      const sg = Math.min(0.45, 0.12 + 0.11 * Math.pow(age, 0.8));
      const gx = (this.px[i] - this.fmin[0]) / dx - 0.5, gy = (this.py[i] - this.fmin[1]) / dy - 0.5, gz = (this.pz[i] - this.fmin[2]) / dz - 0.5;
      const rx = Math.min(4, Math.ceil((2 * sg) / dx)), ry = Math.min(4, Math.ceil((2 * sg) / dy)), rz = Math.min(4, Math.ceil((2 * sg) / dz));
      const cx = Math.round(gx), cy = Math.round(gy), cz = Math.round(gz);
      let sx = 0, sy = 0, sz = 0;
      for (let k = -rx; k <= rx; k++) { const d = (cx + k - gx) * dx; const w = Math.exp(-(d * d) / (2 * sg * sg)); wx[k + rx] = w; sx += w; }
      for (let k = -ry; k <= ry; k++) { const d = (cy + k - gy) * dy; const w = Math.exp(-(d * d) / (2 * sg * sg)); wy[k + ry] = w; sy += w; }
      for (let k = -rz; k <= rz; k++) { const d = (cz + k - gz) * dz; const w = Math.exp(-(d * d) / (2 * sg * sg)); wz[k + rz] = w; sz += w; }
      const q = m / (vol * sx * sy * sz);
      for (let a = -rz; a <= rz; a++) {
        const z = cz + a; if (z < 0 || z >= fnz) continue;
        for (let b = -ry; b <= ry; b++) {
          const y = cy + b; if (y < 0 || y >= fny) continue;
          const wzy = q * wz[a + rz] * wy[b + ry];
          const o = fnx * (y + fny * z);
          for (let c = -rx; c <= rx; c++) {
            const x = cx + c; if (x < 0 || x >= fnx) continue;
            f[o + x] += wzy * wx[c + rx];
          }
        }
      }
    }
    this.fineVersion++;
  }

  /** Fine puff density at a world position (0 outside the fine volume). */
  fineAt(x: number, y: number, z: number): number {
    if (!this.fine.length) return 0;
    const gx = ((x - this.fmin[0]) / this.fsize[0]) * this.fnx - 0.5, gy = ((y - this.fmin[1]) / this.fsize[1]) * this.fny - 0.5, gz = ((z - this.fmin[2]) / this.fsize[2]) * this.fnz - 0.5;
    if (gx < -1 || gy < -1 || gz < -1 || gx > this.fnx || gy > this.fny || gz > this.fnz) return 0;
    return trilinear(this.fine, this.fnx, this.fny, this.fnz, gx, gy, gz);
  }

  fillFineTexture(out: Uint8Array, dmax = 3) {
    const n = this.fine.length, k = 1 / dmax;
    for (let i = 0; i < n; i++) {
      const d = this.fine[i] * k;
      out[i] = d <= 0 ? 0 : d >= 1 ? 255 : (Math.sqrt(d) * 255 + 0.5) | 0;
    }
  }

  reset() {
    for (const f of this.f) f.fill(0);
    this.mass = [0, 0, 0];
    this.np = 0;
    this.fine.fill(0);
    this.fineVersion++;
    this.purgeLeft = 0;
    for (const m of this.machines.values()) { m.out = 0; m.onTime = 0; m.acc = 0; }
    this.version++;
  }

  /** Rapid ventilation: strong extra decay for a few seconds of sim time. */
  purge() { this.purgeLeft = 10; }

  // ── machines ───────────────────────────────────────────
  /** Register / update machines from the show and the current DMX frames. */
  sync(fixtures: Fixture[], profiles: Map<string, FixtureProfile>, frames: Map<number, Uint8Array>) {
    const seen = new Set<number>();
    for (const fx of fixtures) {
      const prof = profiles.get(fx.profile);
      const kind = machineKind(prof);
      if (!kind || !prof) continue;
      seen.add(fx.id);
      let m = this.machines.get(fx.id);
      if (!m) { m = { id: fx.id, kind, pos: [0, 0, 0], dir: [0, 0, 1], target: 0, out: 0, onTime: 0, acc: 0 }; this.machines.set(fx.id, m); }
      m.kind = kind;
      const [p, d] = nozzleWorld(fx);
      m.pos = p; m.dir = d;
      m.target = machineOutput(prof, fx, frames.get(fx.universe));
    }
    for (const id of [...this.machines.keys()]) if (!seen.has(id)) this.machines.delete(id);
  }

  get machineCount() { return this.machines.size; }
  /** current smoothed output (0..1) of a machine fixture */
  outputOf(id: number): number { return this.machines.get(id)?.out ?? 0; }
  hasActivity(): boolean {
    if (this.np > 0 || this.mass[0] + this.mass[1] + this.mass[2] > 1e-5) return true;
    for (const m of this.machines.values()) if (m.target > 0 || m.out > 0.01) return true;
    return false;
  }

  /**
   * Advance by real time `nowSec` using the latest synced machines. Decoupled from render fps: at most a few
   * fixed-size steps per call; if the caller was away for a long time the smoke is fast-forwarded instead.
   */
  update(nowSec: number, simHz = 12) {
    if (this.frozen) { this.lastUpdate = nowSec; return; }
    if (this.lastUpdate === 0) { this.lastUpdate = nowSec; return; }
    let dt = nowSec - this.lastUpdate;
    this.lastUpdate = nowSec;
    if (dt <= 0) return;
    if (dt > 1.5) { this.fastForward(dt); return; }
    this.simAcc += dt;
    // keep the CPU cost bounded: slow the sim clock down when a step is expensive
    const h = 1 / (this.stepMs > 9 ? Math.max(5, simHz * 0.6) : simHz);
    let n = Math.min(3, Math.floor(this.simAcc / h));
    if (n < 1) return;
    const step = Math.min(0.3, this.simAcc / n);
    this.simAcc = 0;
    for (let i = 0; i < n; i++) this.step(step);
  }

  /**
   * Catch up after the view was unmounted. Machines keep their last output. The first ~20 s are simulated in
   * coarse steps; anything beyond that is applied analytically (decay) plus a few smoothing passes that stand
   * in for room-scale mixing. Cost is bounded (~100–200 ms) however long the user was away.
   */
  fastForward(seconds: number) {
    const sim = Math.min(seconds, 20);
    const n = Math.max(1, Math.ceil(sim / 0.7));
    const dt = sim / n;
    for (let i = 0; i < n; i++) this.step(dt);
    const rest = seconds - sim;
    if (rest > 0) {
      this.decay(rest);
      this.relax(Math.min(10, Math.ceil(rest / 8)));
    }
  }

  /** Convex 3-point smoothing along each axis (unconditionally stable, mass-conserving away from walls). */
  relax(passes: number) {
    const { nx, ny, nz } = this;
    for (let s = 0; s < 3; s++) {
      if (this.mass[s] < 1e-7) continue;
      for (let p = 0; p < passes; p++) {
        const a = this.f[s], b = this.tmp;
        let sum = 0;
        for (let z = 0; z < nz; z++) {
          const zm = z > 0 ? -nx * ny : 0, zp = z < nz - 1 ? nx * ny : 0;
          for (let y = 0; y < ny; y++) {
            const ym = y > 0 ? -nx : 0, yp = y < ny - 1 ? nx : 0;
            const o = nx * (y + ny * z);
            for (let x = 0; x < nx; x++) {
              const i = o + x, c = a[i];
              const xm = x > 0 ? -1 : 0, xp = x < nx - 1 ? 1 : 0;
              const v = c * 0.4 + 0.1 * (a[i + xm] + a[i + xp] + a[i + ym] + a[i + yp] + a[i + zm] + a[i + zp]);
              b[i] = v;
              sum += v;
            }
          }
        }
        this.f[s] = b; this.tmp = a;
        this.mass[s] = sum * this.cellVol;
      }
    }
    this.version++;
  }

  private decay(sec: number) {
    for (let s = 0; s < 3; s++) {
      const k = Math.exp((-Math.LN2 / KIND[SPECIES[s]].halfLife) * this.cfg.dissipation * sec);
      const f = this.f[s];
      for (let i = 0; i < f.length; i++) f[i] *= k;
      this.mass[s] *= k;
    }
    this.version++;
  }

  // ── stepping ───────────────────────────────────────────
  step(dt: number, n = 1) {
    for (let k = 0; k < n; k++) this.step1(dt);
  }

  private step1(dt: number) {
    const t0 = typeof performance !== "undefined" ? performance.now() : 0;
    const ns = Math.max(1, Math.ceil(dt / 0.1));
    const h = dt / ns;
    for (let i = 0; i < ns; i++) { this.machinesStep(h); this.particlesStep(h); }
    this.gridStep(dt);
    this.rebuildFine();
    if (this.purgeLeft > 0) this.purgeLeft = Math.max(0, this.purgeLeft - dt);
    this.time += dt;
    this.version++;
    if (t0) this.stepMs = performance.now() - t0;
  }

  private machinesStep(h: number) {
    for (const m of this.machines.values()) {
      const kp = KIND[m.kind];
      const tau = m.target > m.out ? 1.2 : 0.7;
      m.out += (m.target - m.out) * (1 - Math.exp(-h / tau));
      if (m.target > 0.01) m.onTime += h; else if (m.out < 0.02) m.onTime = 0;
      if (m.out < 0.015) continue;
      // pressure burst when the pump kicks in
      const burst = 1 + 0.7 * Math.exp(-m.onTime / 1.4);
      m.acc += PART_RATE * kp.puffs * h * Math.min(1.2, m.out + 0.15);
      const sp = SPECIES.indexOf(m.kind);
      const mq = (kp.rate * m.out * burst * h) / Math.max(1e-6, PART_RATE * kp.puffs * h * Math.min(1.2, m.out + 0.15));
      while (m.acc >= 1 && this.np < MAX_PART) {
        m.acc -= 1;
        const i = this.np++;
        // random direction inside the exhaust cone around the nozzle direction
        const a = this.rnd() * Math.PI * 2, r = Math.sqrt(this.rnd()) * kp.spread;
        const [dx, dy, dz] = coneDir(m.dir, a, r);
        const spd = kp.jet * (0.7 + 0.6 * this.rnd()) * (0.45 + 0.55 * m.out);
        this.px[i] = m.pos[0] + dx * 0.04; this.py[i] = m.pos[1] + dy * 0.04; this.pz[i] = m.pos[2] + dz * 0.04;
        this.vx[i] = dx * spd; this.vy[i] = dy * spd; this.vz[i] = dz * spd;
        this.pm[i] = mq; this.pa[i] = 0; this.ps[i] = sp;
        this.emitted[sp] += mq;
      }
      if (m.acc > 3) m.acc = 3;
    }
  }

  private ambientAt(x: number, y: number, z: number, out: [number, number, number]) {
    const bx = Math.min(Math.ceil(this.nx / B) - 1, Math.max(0, Math.floor(((x - this.min[0]) / this.size[0]) * (this.nx / B))));
    const by = Math.min(Math.ceil(this.ny / B) - 1, Math.max(0, Math.floor(((y - this.min[1]) / this.size[1]) * (this.ny / B))));
    const bz = Math.min(Math.ceil(this.nz / B) - 1, Math.max(0, Math.floor(((z - this.min[2]) / this.size[2]) * (this.nz / B))));
    const bi = (bx + Math.ceil(this.nx / B) * (by + Math.ceil(this.ny / B) * bz)) * 3;
    out[0] = this.amb[bi]; out[1] = this.amb[bi + 1]; out[2] = this.amb[bi + 2];
  }

  private particlesStep(h: number) {
    const amb: [number, number, number] = [0, 0, 0];
    const [x0, y0, z0] = this.min;
    const x1 = x0 + this.size[0], y1 = y0 + this.size[1], z1 = z0 + this.size[2];
    let i = 0;
    while (i < this.np) {
      const kp = KIND[SPECIES[this.ps[i]]];
      const age = (this.pa[i] += h);
      this.ambientAt(this.px[i], this.py[i], this.pz[i], amb);
      // buoyancy: hot exhaust rises, then cools to its asymptotic (possibly negative) buoyancy
      const buoy = kp.buoyInf + (kp.buoy0 - kp.buoyInf) * Math.exp(-age / kp.cool);
      this.vy[i] += buoy * h;
      // entrainment / drag towards the surrounding air
      const k = Math.exp(-kp.drag * h);
      this.vx[i] = amb[0] + (this.vx[i] - amb[0]) * k;
      this.vy[i] = amb[1] + (this.vy[i] - amb[1]) * k;
      this.vz[i] = amb[2] + (this.vz[i] - amb[2]) * k;
      // billowing: curl-noise eddies (~1 m) swirl the puffs once they leave the nozzle
      if (age > 0.25) {
        const ek = Math.min(1, (age - 0.25) / 1.5) * 1.5 * h;
        const c = curl3(this.px[i] * 0.95, this.py[i] * 0.95, this.pz[i] * 0.95 + this.time * 0.3, 0.3);
        this.vx[i] += c[0] * ek; this.vy[i] += c[1] * ek * 0.7; this.vz[i] += c[2] * ek;
      }
      // turbulent meander grows with age
      const sig = 0.3 * Math.min(1, age / 2.5) * Math.sqrt(h);
      this.vx[i] += (this.rnd() - 0.5) * 3.4 * sig; this.vy[i] += (this.rnd() - 0.5) * 2.4 * sig; this.vz[i] += (this.rnd() - 0.5) * 3.4 * sig;
      let x = this.px[i] + this.vx[i] * h, y = this.py[i] + this.vy[i] * h, z = this.pz[i] + this.vz[i] * h;
      // closed room: bounce off walls, slide along floor / ceiling
      if (x < x0 + 0.02) { x = x0 + 0.02; this.vx[i] = Math.abs(this.vx[i]) * 0.25; } else if (x > x1 - 0.02) { x = x1 - 0.02; this.vx[i] = -Math.abs(this.vx[i]) * 0.25; }
      if (z < z0 + 0.02) { z = z0 + 0.02; this.vz[i] = Math.abs(this.vz[i]) * 0.25; } else if (z > z1 - 0.02) { z = z1 - 0.02; this.vz[i] = -Math.abs(this.vz[i]) * 0.25; }
      if (y < y0 + 0.03) { y = y0 + 0.03; this.vy[i] = 0; this.vx[i] *= 0.99; } else if (y > y1 - 0.03) { y = y1 - 0.03; this.vy[i] = 0; }
      this.px[i] = x; this.py[i] = y; this.pz[i] = z;
      // hand mass to the grid
      // young puffs keep their mass (and their fine structure); older ones melt into the room grid
      const dep = 1 - Math.exp(-h / (kp.depTau * (1 + 4 * Math.exp(-age / 6))));
      const m = this.pm[i] * dep;
      this.splat(this.ps[i], x, y, z, m);
      this.pm[i] -= m;
      if (age > LIFE || this.pm[i] < 1e-5) {
        if (this.pm[i] > 0) this.splat(this.ps[i], x, y, z, this.pm[i]);
        this.np--;
        const j = this.np; // swap-remove
        this.px[i] = this.px[j]; this.py[i] = this.py[j]; this.pz[i] = this.pz[j];
        this.vx[i] = this.vx[j]; this.vy[i] = this.vy[j]; this.vz[i] = this.vz[j];
        this.pm[i] = this.pm[j]; this.pa[i] = this.pa[j]; this.ps[i] = this.ps[j];
        continue;
      }
      i++;
    }
  }

  private splat(s: number, x: number, y: number, z: number, m: number) {
    const gx = ((x - this.min[0]) / this.size[0]) * this.nx - 0.5;
    const gy = ((y - this.min[1]) / this.size[1]) * this.ny - 0.5;
    const gz = ((z - this.min[2]) / this.size[2]) * this.nz - 0.5;
    const x0 = Math.floor(gx), y0 = Math.floor(gy), z0 = Math.floor(gz);
    const fx = gx - x0, fy = gy - y0, fz = gz - z0;
    const f = this.f[s];
    const inv = m / this.cellVol;
    const nx = this.nx, ny = this.ny, nz = this.nz;
    for (let dz = 0; dz < 2; dz++) {
      const zz = Math.min(nz - 1, Math.max(0, z0 + dz));
      const wz = dz ? fz : 1 - fz;
      for (let dy = 0; dy < 2; dy++) {
        const yy = Math.min(ny - 1, Math.max(0, y0 + dy));
        const wy = dy ? fy : 1 - fy;
        for (let dx = 0; dx < 2; dx++) {
          const xx = Math.min(nx - 1, Math.max(0, x0 + dx));
          f[xx + nx * (yy + ny * zz)] += inv * wz * wy * (dx ? fx : 1 - fx);
        }
      }
    }
    this.mass[s] += m;
  }

  // ── grid: ambient air, advection, diffusion + decay ────
  private updateAmbient() {
    const bnx = Math.ceil(this.nx / B), bny = Math.ceil(this.ny / B), bnz = Math.ceil(this.nz / B);
    const amp = AMP[this.cfg.currents];
    const th = (this.cfg.windDir * Math.PI) / 180;
    const wmag = amp * 0.9;
    const wx = Math.sin(th) * wmag, wz = Math.cos(th) * wmag;
    const hv = this.cfg.hvac === "down" ? -0.06 : this.cfg.hvac === "up" ? 0.06 : 0;
    const t = this.time * 0.07;
    for (let bz = 0; bz < bnz; bz++) for (let by = 0; by < bny; by++) for (let bx = 0; bx < bnx; bx++) {
      const i = (bx + bnx * (by + bny * bz)) * 3;
      let vx = wx, vy = hv, vz = wz;
      if (amp > 0) {
        const fx = (bx + 0.5) / bnx, fy = (by + 0.5) / bny, fz = (bz + 0.5) / bnz;
        // small eddies (~3 m) plus room-scale circulation (~9 m) that slowly wanders and stirs the whole volume
        const a = curl3s(fx * this.size[0] / 3.2, fy * this.size[1] / 2.2, fz * this.size[2] / 3.2 + t, 0.35);
        const b2 = curl3s(fx * this.size[0] / 9 + 40, fy * this.size[1] / 5 + 40, fz * this.size[2] / 9 + t * 1.6 + 40, 0.35);
        const k = (amp / 0.7) * 1.6;
        vx += (a[0] * 0.55 + b2[0] * 0.9) * k; vy += (a[1] * 0.55 + b2[1] * 0.9) * k; vz += (a[2] * 0.55 + b2[2] * 0.9) * k;
      }
      this.amb[i] = vx; this.amb[i + 1] = vy; this.amb[i + 2] = vz;
    }
  }

  private gridStep(dt: number) {
    const n = Math.max(1, Math.ceil(dt / 0.35));
    for (let i = 0; i < n; i++) this.gridChunk(dt / n);
  }

  private gridChunk(dt: number) {
    this.updateAmbient();
    const total = this.mass[0] + this.mass[1] + this.mass[2];
    if (total < 1e-7) return;
    for (let s = 0; s < 3; s++) {
      if (this.mass[s] < 1e-7) { this.f[s].fill(0); this.mass[s] = 0; continue; }
      this.advect(s, dt);
      this.diffuseDecay(s, dt);
    }
  }

  /** Conservative donor-cell advection by block-constant air velocity (+ density-driven buoyancy). Walls are closed. */
  private advect(s: number, dt: number) {
    const { nx, ny, nz } = this;
    const bnx = Math.ceil(nx / B), bny = Math.ceil(ny / B), bnz = Math.ceil(nz / B);
    const nb = bnx * bny * bnz;
    const dx = this.size[0] / nx, dy = this.size[1] / ny, dz = this.size[2] / nz;
    const gb = KIND[SPECIES[s]].gridBuoy;
    let a = this.f[s], b = this.tmp;
    // block mean densities → buoyancy saturation
    const bm = this.bmean;
    bm.fill(0);
    for (let z = 0; z < nz; z++) {
      const bz = z >> 2;
      for (let y = 0; y < ny; y++) {
        const bb = bnx * ((y >> 2) + bny * bz);
        const row = nx * (y + ny * z);
        for (let x = 0; x < nx; x++) bm[bb + (x >> 2)] += a[row + x];
      }
    }
    const inv = 1 / (B * B * B);
    const bv = this.bvel;
    let cfl = 0;
    for (let i = 0; i < nb; i++) {
      const m = bm[i] * inv;
      const sat = m / (m + 0.12);
      const vx = this.amb[i * 3], vy = this.amb[i * 3 + 1] + gb * sat, vz = this.amb[i * 3 + 2];
      bv[i * 3] = vx; bv[i * 3 + 1] = vy; bv[i * 3 + 2] = vz;
      const c = (Math.abs(vx) / dx + Math.abs(vy) / dy + Math.abs(vz) / dz) * dt;
      if (c > cfl) cfl = c;
    }
    const nsub = Math.max(1, Math.ceil(cfl / 0.8));
    const h = dt / nsub;
    const kx = h / dx, ky = h / dy, kz = h / dz;
    const sy = nx, sz = nx * ny;
    for (let it = 0; it < nsub; it++) {
      b.set(a);
      for (let z = 0; z < nz; z++) {
        const bz = z >> 2;
        for (let y = 0; y < ny; y++) {
          const bbase = bnx * ((y >> 2) + bny * bz);
          const o = nx * (y + ny * z);
          for (let x = 0; x < nx; x++) {
            const i = o + x;
            const bi = bbase + (x >> 2);
            const c = a[i];
            if (x > 0) {
              const u = (x & 3) === 0 ? 0.5 * (bv[(bi - 1) * 3] + bv[bi * 3]) : bv[bi * 3];
              const F = u * kx * (u > 0 ? a[i - 1] : c);
              b[i] += F; b[i - 1] -= F;
            }
            if (y > 0) {
              const w = (y & 3) === 0 ? 0.5 * (bv[(bi - bnx) * 3 + 1] + bv[bi * 3 + 1]) : bv[bi * 3 + 1];
              const F = w * ky * (w > 0 ? a[i - sy] : c);
              b[i] += F; b[i - sy] -= F;
            }
            if (z > 0) {
              const w = (z & 3) === 0 ? 0.5 * (bv[(bi - bnx * bny) * 3 + 2] + bv[bi * 3 + 2]) : bv[bi * 3 + 2];
              const F = w * kz * (w > 0 ? a[i - sz] : c);
              b[i] += F; b[i - sz] -= F;
            }
          }
        }
      }
      const t = a; a = b; b = t;
    }
    this.f[s] = a;
    this.tmp = b;
  }

  /** Implicit (backward-Euler) diffusion along one axis — unconditionally stable, positive, mass-conserving. */
  private implicitAxis(f: Float32Array, n: number, stride: number, starts: number[], k: number) {
    if (k < 1e-6) return;
    const cp = this.tcp, inv = this.tinv, dp = this.tdp;
    // forward elimination coefficients for the constant tridiagonal system (-k, 1+2k, -k), Neumann ends
    let m = 1 + k;
    inv[0] = 1 / m; cp[0] = -k * inv[0];
    for (let i = 1; i < n; i++) {
      m = (i === n - 1 ? 1 + k : 1 + 2 * k) + k * cp[i - 1];
      inv[i] = 1 / m; cp[i] = -k * inv[i];
    }
    for (let l = 0; l < starts.length; l++) {
      let p = starts[l];
      dp[0] = f[p] * inv[0];
      for (let i = 1; i < n; i++) { p += stride; dp[i] = (f[p] + k * dp[i - 1]) * inv[i]; }
      f[p] = dp[n - 1];
      for (let i = n - 2; i >= 0; i--) { p -= stride; dp[i] -= cp[i] * f[p + stride]; f[p] = dp[i]; }
    }
  }

  private lineStarts(axis: 0 | 1 | 2): number[] {
    const { nx, ny, nz } = this;
    const key = `${axis}|${nx}|${ny}|${nz}`;
    let l = this.starts.get(key);
    if (l) return l;
    l = [];
    if (axis === 0) { for (let z = 0; z < nz; z++) for (let y = 0; y < ny; y++) l.push(nx * (y + ny * z)); }
    else if (axis === 1) { for (let z = 0; z < nz; z++) for (let x = 0; x < nx; x++) l.push(x + nx * ny * z); }
    else { for (let y = 0; y < ny; y++) for (let x = 0; x < nx; x++) l.push(x + nx * y); }
    this.starts.set(key, l);
    return l;
  }

  private diffuseDecay(s: number, dt: number) {
    const { nx, ny, nz } = this;
    const kp = KIND[SPECIES[s]];
    const dx = this.size[0] / nx, dy = this.size[1] / ny, dz = this.size[2] / nz;
    // stratified low fog is barely stirred by room air; fine haze follows it; fog in between
    const eddy = EDDY[this.cfg.currents] * (s === 2 ? 0.08 : s === 1 ? 2.2 : 1);
    const dH = kp.dH + eddy, dV = kp.dV + eddy * 0.9;
    const f = this.f[s];
    this.implicitAxis(f, nx, 1, this.lineStarts(0), (dH * dt) / (dx * dx));
    this.implicitAxis(f, nz, nx * ny, this.lineStarts(2), (dH * dt) / (dz * dz));
    this.implicitAxis(f, ny, nx, this.lineStarts(1), (dV * dt) / (dy * dy));
    const lam = (Math.LN2 / kp.halfLife) * this.cfg.dissipation + (this.purgeLeft > 0 ? 0.35 : 0);
    const dec = Math.exp(-lam * dt);
    const ventDec = Math.exp(-(lam + (this.cfg.vent ? 0.8 * this.cfg.dissipation : 0)) * dt);
    const ventOn = this.cfg.vent;
    let sum = 0;
    for (let z = 0; z < nz; z++) {
      for (let y = 0; y < ny; y++) {
        const o = nx * (y + ny * z);
        const k = ventOn && y >= ny - 2 && z < 3 ? ventDec : dec;
        for (let x = 0; x < nx; x++) {
          let v = f[o + x] * k;
          if (v < 1e-8) v = 0;
          f[o + x] = v;
          sum += v;
        }
      }
    }
    this.mass[s] = sum * this.cellVol;
  }

  // ── queries ────────────────────────────────────────────
  /** Combined density (all species) at a world position. */
  sample(x: number, y: number, z: number): number {
    const gx = ((x - this.min[0]) / this.size[0]) * this.nx - 0.5;
    const gy = ((y - this.min[1]) / this.size[1]) * this.ny - 0.5;
    const gz = ((z - this.min[2]) / this.size[2]) * this.nz - 0.5;
    let d = 0;
    for (let s = 0; s < 3; s++) d += trilinear(this.f[s], this.nx, this.ny, this.nz, gx, gy, gz);
    return d + this.fineAt(x, y, z);
  }

  /** Pack total density as sqrt-encoded bytes for a 3D texture (x fastest, then y, then z). */
  fillTexture(out: Uint8Array, dmax = 1.5) {
    const n = this.nx * this.ny * this.nz;
    const a = this.f[0], b = this.f[1], c = this.f[2];
    const k = 1 / dmax;
    for (let i = 0; i < n; i++) {
      const d = (a[i] + b[i] + c[i]) * k;
      out[i] = d <= 0 ? 0 : d >= 1 ? 255 : (Math.sqrt(d) * 255 + 0.5) | 0;
    }
  }

  /**
   * Optical-depth projection (density integrated along an axis, metres) for the 2D plan / elevations.
   * `plan` → grid over (x, z); `front` → (x, y); `side` → (z, y). Returns data[v * w + u].
   */
  projection(view: "plan" | "front" | "side"): { w: number; h: number; data: Float32Array; u0: number; u1: number; v0: number; v1: number } {
    const { nx, ny, nz } = this;
    const a = this.f[0], b = this.f[1], c = this.f[2];
    const dx = this.size[0] / nx, dy = this.size[1] / ny, dz = this.size[2] / nz;
    if (view === "plan") {
      const data = new Float32Array(nx * nz);
      for (let z = 0; z < nz; z++) for (let y = 0; y < ny; y++) {
        const o = nx * (y + ny * z);
        for (let x = 0; x < nx; x++) data[x + nx * z] += (a[o + x] + b[o + x] + c[o + x]) * dy;
      }
      this.addFineProjection(data, "plan");
      return { w: nx, h: nz, data, u0: this.min[0], u1: this.min[0] + this.size[0], v0: this.min[2], v1: this.min[2] + this.size[2] };
    }
    if (view === "front") {
      const data = new Float32Array(nx * ny);
      for (let z = 0; z < nz; z++) for (let y = 0; y < ny; y++) {
        const o = nx * (y + ny * z);
        for (let x = 0; x < nx; x++) data[x + nx * y] += (a[o + x] + b[o + x] + c[o + x]) * dz;
      }
      this.addFineProjection(data, "front");
      return { w: nx, h: ny, data, u0: this.min[0], u1: this.min[0] + this.size[0], v0: this.min[1], v1: this.min[1] + this.size[1] };
    }
    const data = new Float32Array(nz * ny);
    for (let z = 0; z < nz; z++) for (let y = 0; y < ny; y++) {
      const o = nx * (y + ny * z);
      let s = 0;
      for (let x = 0; x < nx; x++) s += a[o + x] + b[o + x] + c[o + x];
      data[z + nz * y] = s * dx;
    }
    this.addFineProjection(data, "side");
    return { w: nz, h: ny, data, u0: this.min[2], u1: this.min[2] + this.size[2], v0: this.min[1], v1: this.min[1] + this.size[1] };
  }

  /** Add the fine puff field to a projection image (sampled at the coarse cell centres). */
  private addFineProjection(data: Float32Array, view: "plan" | "front" | "side") {
    if (!this.fine.length) return;
    const { nx, ny, nz } = this;
    const sx = (i: number) => this.min[0] + ((i + 0.5) / nx) * this.size[0];
    const sy = (i: number) => this.min[1] + ((i + 0.5) / ny) * this.size[1];
    const sz = (i: number) => this.min[2] + ((i + 0.5) / nz) * this.size[2];
    const steps = 24;
    if (view === "plan") {
      const dy = this.fsize[1] / steps;
      for (let z = 0; z < nz; z++) for (let x = 0; x < nx; x++) {
        let t = 0;
        for (let k = 0; k < steps; k++) t += this.fineAt(sx(x), this.fmin[1] + (k + 0.5) * dy, sz(z));
        data[x + nx * z] += t * dy;
      }
    } else if (view === "front") {
      const dz = this.fsize[2] / steps;
      for (let y = 0; y < ny; y++) for (let x = 0; x < nx; x++) {
        let t = 0;
        for (let k = 0; k < steps; k++) t += this.fineAt(sx(x), sy(y), this.fmin[2] + (k + 0.5) * dz);
        data[x + nx * y] += t * dz;
      }
    } else {
      const dx = this.fsize[0] / steps;
      for (let y = 0; y < ny; y++) for (let z = 0; z < nz; z++) {
        let t = 0;
        for (let k = 0; k < steps; k++) t += this.fineAt(this.fmin[0] + (k + 0.5) * dx, sy(y), sz(z));
        data[z + nz * y] += t * dx;
      }
    }
  }

  stats(): SmokeStats {
    let mx = 0;
    const n = this.nx * this.ny * this.nz;
    for (let i = 0; i < n; i++) { const d = this.f[0][i] + this.f[1][i] + this.f[2][i]; if (d > mx) mx = d; }
    let ly = 0, lm = 0;
    const low = this.f[2];
    for (let y = 0; y < this.ny; y++) {
      let row = 0;
      for (let z = 0; z < this.nz; z++) { const o = this.nx * (y + this.ny * z); for (let x = 0; x < this.nx; x++) row += low[o + x]; }
      ly += row * (this.min[1] + ((y + 0.5) / this.ny) * this.size[1]); lm += row;
    }
    const total = this.mass[0] + this.mass[1] + this.mass[2];
    return {
      mass: total, fineMass: this.fineMass(), inFlight: this.flight(), emitted: this.emitted.slice(), massFog: this.mass[0], massHaze: this.mass[1], massLow: this.mass[2], maxDensity: mx,
      meanDensity: total / (this.size[0] * this.size[1] * this.size[2]), lowMeanY: lm > 0 ? ly / lm : 0,
      particles: this.np, machines: this.machines.size, grid: [this.nx, this.ny, this.nz], stepMs: this.stepMs, time: this.time,
    };
  }

  private fineMass(): number { let m = 0; for (let i = 0; i < this.fine.length; i++) m += this.fine[i]; return m * ((this.fsize[0] / Math.max(1, this.fnx)) * (this.fsize[1] / Math.max(1, this.fny)) * (this.fsize[2] / Math.max(1, this.fnz))); }

  private flight(): number { let m = 0; for (let i = 0; i < this.np; i++) m += this.pm[i]; return m; }

  get meanDensity(): number { return (this.mass[0] + this.mass[1] + this.mass[2]) / (this.size[0] * this.size[1] * this.size[2]); }

  /** World position of a registered machine's nozzle (tests / debug). */
  nozzleOf(id: number): [number, number, number] | null { return this.machines.get(id)?.pos ?? null; }
}

// ── math helpers ─────────────────────────────────────────
function trilinear(f: Float32Array, nx: number, ny: number, nz: number, gx: number, gy: number, gz: number): number {
  const x0 = Math.floor(gx), y0 = Math.floor(gy), z0 = Math.floor(gz);
  const fx = gx - x0, fy = gy - y0, fz = gz - z0;
  const cx = (v: number) => (v < 0 ? 0 : v > nx - 1 ? nx - 1 : v);
  const cy = (v: number) => (v < 0 ? 0 : v > ny - 1 ? ny - 1 : v);
  const cz = (v: number) => (v < 0 ? 0 : v > nz - 1 ? nz - 1 : v);
  const xa = cx(x0), xb = cx(x0 + 1), ya = cy(y0), yb = cy(y0 + 1), za = cz(z0), zb = cz(z0 + 1);
  const at = (x: number, y: number, z: number) => f[x + nx * (y + ny * z)];
  const c00 = at(xa, ya, za) * (1 - fx) + at(xb, ya, za) * fx, c10 = at(xa, yb, za) * (1 - fx) + at(xb, yb, za) * fx;
  const c01 = at(xa, ya, zb) * (1 - fx) + at(xb, ya, zb) * fx, c11 = at(xa, yb, zb) * (1 - fx) + at(xb, yb, zb) * fx;
  return (c00 * (1 - fy) + c10 * fy) * (1 - fz) + (c01 * (1 - fy) + c11 * fy) * fz;
}

function coneDir(axis: [number, number, number], a: number, r: number): [number, number, number] {
  // orthonormal basis around axis
  const up: [number, number, number] = Math.abs(axis[1]) < 0.95 ? [0, 1, 0] : [1, 0, 0];
  let bx = axis[1] * up[2] - axis[2] * up[1], by = axis[2] * up[0] - axis[0] * up[2], bz = axis[0] * up[1] - axis[1] * up[0];
  const bl = Math.hypot(bx, by, bz) || 1; bx /= bl; by /= bl; bz /= bl;
  const cx = axis[1] * bz - axis[2] * by, cy = axis[2] * bx - axis[0] * bz, cz = axis[0] * by - axis[1] * bx;
  const s = Math.sin(r), c = Math.cos(r), ca = Math.cos(a) * s, sa = Math.sin(a) * s;
  return [axis[0] * c + bx * ca + cx * sa, axis[1] * c + by * ca + cy * sa, axis[2] * c + bz * ca + cz * sa];
}

/** Nozzle world position + direction of a machine fixture (Euler order YXZ like the 3D scene). */
export function nozzleWorld(fx: Fixture): [[number, number, number], [number, number, number]] {
  const D = Math.PI / 180;
  const rx = fx.rot[0] * D, ry = fx.rot[1] * D, rz = fx.rot[2] * D;
  const rot = (v: [number, number, number]): [number, number, number] => {
    // Rz, then Rx, then Ry
    let [x, y, z] = v;
    let c = Math.cos(rz), s = Math.sin(rz);
    [x, y] = [x * c - y * s, x * s + y * c];
    c = Math.cos(rx); s = Math.sin(rx);
    [y, z] = [y * c - z * s, y * s + z * c];
    c = Math.cos(ry); s = Math.sin(ry);
    [x, z] = [x * c + z * s, -x * s + z * c];
    return [x, y, z];
  };
  const p = rot(NOZZLE_LOCAL.pos), d = rot(NOZZLE_LOCAL.dir);
  return [[fx.pos[0] + p[0], fx.pos[1] + p[1], fx.pos[2] + p[2]], d];
}

// ── singleton + debug hook ───────────────────────────────
let inst: SmokeSim | null = null;
export function getSmoke(): SmokeSim {
  if (!inst) {
    inst = new SmokeSim(GRIDS.med);
    if (typeof window !== "undefined") {
      (window as unknown as { __smoke?: unknown }).__smoke = {
        step: (dt: number, n = 1) => inst!.step(dt, n),
        reset: () => inst!.reset(),
        stats: () => inst!.stats(),
        sample: (x: number, y: number, z: number) => inst!.sample(x, y, z),
        nozzle: (id: number) => inst!.nozzleOf(id),
        setGrid: (q: "low" | "med" | "high") => inst!.setGrid(GRIDS[q]),
        freeze: (on: boolean) => { inst!.frozen = on; },
        purge: () => inst!.purge(),
        sim: inst,
      };
    }
  }
  return inst;
}
