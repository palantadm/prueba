// Small 3D math + view projection helpers for the 2D plan/elevation visualiser.
// World axes (same as the 3D view): x = stage right, y = up, z = downstage (towards the audience).

export const D2R = Math.PI / 180;
export type V3 = [number, number, number];
export type M3 = [number, number, number, number, number, number, number, number, number]; // row-major

export const add = (a: V3, b: V3): V3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const sub = (a: V3, b: V3): V3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const mul = (a: V3, k: number): V3 => [a[0] * k, a[1] * k, a[2] * k];
export const dot = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const len = (a: V3) => Math.hypot(a[0], a[1], a[2]);
export const norm = (a: V3): V3 => { const l = len(a) || 1; return [a[0] / l, a[1] / l, a[2] / l]; };
export const cross = (a: V3, b: V3): V3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];

export function mmul(a: M3, b: M3): M3 {
  const r = new Array(9) as M3;
  for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) {
    r[i * 3 + j] = a[i * 3] * b[j] + a[i * 3 + 1] * b[3 + j] + a[i * 3 + 2] * b[6 + j];
  }
  return r;
}
export const transpose = (m: M3): M3 => [m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]];
export const apply = (m: M3, v: V3): V3 => [
  m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
  m[3] * v[0] + m[4] * v[1] + m[5] * v[2],
  m[6] * v[0] + m[7] * v[1] + m[8] * v[2],
];
export const Rx = (a: number): M3 => { const c = Math.cos(a), s = Math.sin(a); return [1, 0, 0, 0, c, -s, 0, s, c]; };
export const Ry = (a: number): M3 => { const c = Math.cos(a), s = Math.sin(a); return [c, 0, s, 0, 1, 0, -s, 0, c]; };
export const Rz = (a: number): M3 => { const c = Math.cos(a), s = Math.sin(a); return [c, -s, 0, s, c, 0, 0, 0, 1]; };

/** three.js Euler order "YXZ": R = Ry · Rx · Rz (degrees in, as stored in Fixture.rot / StageObject.rot). */
export function eulerYXZ(rot: V3): M3 {
  return mmul(mmul(Ry(rot[1] * D2R), Rx(rot[0] * D2R)), Rz(rot[2] * D2R));
}

// ── views ───────────────────────────────────────────────
export type ViewKind = "plan" | "front" | "side";

/** World → view plane (u right, v down on screen, metres). */
export function proj(view: ViewKind, p: V3): [number, number] {
  switch (view) {
    case "plan": return [p[0], p[2]]; // audience at the bottom
    case "front": return [p[0], -p[1]]; // seen from the audience
    case "side": return [p[2], -p[1]]; // audience to the right
  }
}

export interface Cam { cx: number; cy: number; scale: number; w: number; h: number }

export const toScreen = (c: Cam, u: number, v: number): [number, number] =>
  [(u - c.cx) * c.scale + c.w / 2, (v - c.cy) * c.scale + c.h / 2];
export const fromScreen = (c: Cam, sx: number, sy: number): [number, number] =>
  [(sx - c.w / 2) / c.scale + c.cx, (sy - c.h / 2) / c.scale + c.cy];

export function toScreenP(c: Cam, view: ViewKind, p: V3): [number, number] {
  const [u, v] = proj(view, p);
  return toScreen(c, u, v);
}

/** Which world axes a drag in `view` moves. */
export function dragAxes(view: ViewKind): [0 | 1 | 2, 0 | 1 | 2, 1 | -1] {
  switch (view) {
    case "plan": return [0, 2, 1];
    case "front": return [0, 1, -1];
    case "side": return [2, 1, -1];
  }
}

/** Set the two world coordinates visible in `view` from view-plane (u, v). */
export function setFromView(view: ViewKind, base: V3, u: number, v: number): V3 {
  const p: V3 = [base[0], base[1], base[2]];
  const [a, b, s] = dragAxes(view);
  p[a] = u;
  p[b] = v * s;
  return p;
}

// ── convex hull (monotone chain) ────────────────────────
export function hull(pts: [number, number][]): [number, number][] {
  const p = pts.slice().sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  if (p.length < 3) return p;
  const cr = (o: number[], a: number[], b: number[]) => (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  const lo: [number, number][] = [];
  for (const q of p) { while (lo.length >= 2 && cr(lo[lo.length - 2], lo[lo.length - 1], q) <= 0) lo.pop(); lo.push(q); }
  const up: [number, number][] = [];
  for (let i = p.length - 1; i >= 0; i--) { const q = p[i]; while (up.length >= 2 && cr(up[up.length - 2], up[up.length - 1], q) <= 0) up.pop(); up.push(q); }
  up.pop(); lo.pop();
  return lo.concat(up);
}

// ── oriented boxes ──────────────────────────────────────
export interface Obb { c: V3; R: M3; h: V3 } // centre, rotation (local→world), half extents

export function boxCorners(b: Obb): V3[] {
  const out: V3[] = [];
  for (const sx of [-1, 1]) for (const sy of [-1, 1]) for (const sz of [-1, 1]) {
    out.push(add(b.c, apply(b.R, [sx * b.h[0], sy * b.h[1], sz * b.h[2]])));
  }
  return out;
}

/** Ray vs oriented box (slab method). Returns entry distance or null. */
export function rayObb(o: V3, d: V3, b: Obb): number | null {
  const Rt = transpose(b.R);
  const lo = apply(Rt, sub(o, b.c));
  const ld = apply(Rt, d);
  let tmin = 0, tmax = Infinity;
  for (let i = 0; i < 3; i++) {
    if (Math.abs(ld[i]) < 1e-9) { if (Math.abs(lo[i]) > b.h[i]) return null; continue; }
    let t1 = (-b.h[i] - lo[i]) / ld[i], t2 = (b.h[i] - lo[i]) / ld[i];
    if (t1 > t2) { const t = t1; t1 = t2; t2 = t; }
    tmin = Math.max(tmin, t1); tmax = Math.min(tmax, t2);
    if (tmin > tmax) return null;
  }
  return tmin > 1e-4 ? tmin : null;
}

export const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/** "Nice" step ≥ min for grid/scale-bar (1, 2, 5 × 10^n). */
export function niceStep(min: number): number {
  const p = Math.pow(10, Math.floor(Math.log10(min)));
  for (const m of [1, 2, 5, 10]) if (m * p >= min) return m * p;
  return 10 * p;
}
