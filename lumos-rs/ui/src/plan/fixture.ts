// Per-frame fixture geometry: decoded DMX look, pan/tilt pose, beam rays and where they land.
import type { BeamInfo, Fixture, FixtureProfile, GeoNode, Stage, StageObject } from "../types";
import { beamAngle, decode, decodeExtras, makePlan, newState, type FxExtras, type FxState, type Plan } from "../lib/stage-dmx";
import {
  D2R, add, apply, boxCorners, cross, eulerYXZ, mul, norm, rayObb, type M3, type Obb, type V3,
} from "./math";

export type GlyphKind = "mover" | "par" | "ledpar" | "fresnel" | "profile" | "bar" | "strobe" | "blinder" | "fog" | "other";

export interface BeamGeo {
  origin: V3;
  dir: V3;
  /** Half-angle in radians. */
  half: number;
  /** Axis hit point and distance. */
  hit: V3;
  dist: number;
  hitKind: "floor" | "object" | "open";
  /** Beam-edge ray end points (ring around the axis). */
  ends: V3[];
  /** Fraction of the ring that lands on the floor (for footprint drawing). */
  floorEnds: V3[];
}

export interface FxGeo {
  fx: Fixture;
  prof: FixtureProfile | undefined;
  plan: Plan | null;
  st: FxState;
  ex: FxExtras;
  R: M3;
  pos: V3;
  moving: boolean;
  glyph: GlyphKind;
  /** Pan / tilt angles in radians (0 when fixed). */
  P: number;
  T: number;
  pivot: V3;
  origin: V3;
  dir: V3;
  beam: BeamGeo | null;
  lit: boolean;
}

const planCache = new Map<number, { sig: string; plan: Plan | null }>();
const MAX_LEN = 30;
const OPEN_LEN = 9;
const RING = 28;

export function planOf(fx: Fixture, prof: FixtureProfile | undefined): Plan | null {
  const sig = `${fx.profile}|${fx.mode}|${prof ? prof.modes.length : -1}`;
  const c = planCache.get(fx.id);
  if (c && c.sig === sig) return c.plan;
  const plan = makePlan(fx, prof);
  planCache.set(fx.id, { sig, plan });
  return plan;
}

function findBeam(n: GeoNode): BeamInfo | null {
  if (n.beam) return n.beam;
  for (const c of n.children) { const b = findBeam(c); if (b) return b; }
  return null;
}

export function glyphKind(prof: FixtureProfile | undefined, plan: Plan | null): GlyphKind {
  if (plan?.moving) return "mover";
  if (!prof) return "other";
  const k = `${prof.id} ${prof.model}`.toLowerCase();
  const cat = prof.category;
  if (/blinder/.test(k)) return "blinder";
  if (cat === "strobe" || /strobe/.test(k)) return "strobe";
  if (cat === "bar" || /\bbar\b|strip|batten/.test(k)) return "bar";
  if (/fog|haze/.test(k) || cat === "effect") return "fog";
  if (/fresnel/.test(k)) return "fresnel";
  if (/profile|ellips|zoom spot/.test(k) || cat === "spot") return "profile";
  if (cat === "par" || cat === "wash" || /par/.test(k)) return plan?.hasRgb || /led/.test(k) ? "ledpar" : "par";
  if (cat === "dimmer") return "par";
  return "other";
}

export function objObb(o: StageObject): Obb {
  return { c: [o.pos[0], o.pos[1], o.pos[2]], R: eulerYXZ([o.rot[0], o.rot[1], o.rot[2]]), h: [o.size[0] / 2, o.size[1] / 2, o.size[2] / 2] };
}

const SOLID = new Set(["wall", "curtain", "screen", "column", "platform", "box", "tree"]);
export function obstacles(stage: Stage | null): Obb[] {
  if (!stage) return [];
  return stage.objects.filter((o) => SOLID.has(o.kind)).map(objObb);
}

/** Model-aware full beam angle in degrees. */
export function angleOf(plan: Plan, st: FxState): number {
  const info = plan.prof.model3d ? findBeam(plan.prof.model3d.root) : null;
  if (info && info.beam_angle > 0) {
    const hi = info.beam_angle_max > 0 ? info.beam_angle_max : info.beam_angle;
    return (info.beam_angle + (hi - info.beam_angle) * st.zoom) * (1 - 0.8 * st.iris);
  }
  return beamAngle(plan, st);
}

function trace(o: V3, d: V3, obs: Obb[]): { t: number; kind: "floor" | "object" | "open" } {
  let t = MAX_LEN, kind: "floor" | "object" | "open" = "open";
  if (d[1] < -1e-4 && o[1] > 0) {
    const tf = o[1] / -d[1];
    if (tf < t) { t = tf; kind = "floor"; }
  }
  for (const b of obs) {
    const tb = rayObb(o, d, b);
    if (tb !== null && tb < t) { t = tb; kind = "object"; }
  }
  // nothing hit within range: draw a short, fading open beam instead of a 30 m cone
  if (kind === "open") t = OPEN_LEN;
  return { t, kind };
}

export function computeFixture(fx: Fixture, prof: FixtureProfile | undefined, frame: Uint8Array | undefined, time: number, obs: Obb[], withBeam: boolean): FxGeo {
  const plan = planOf(fx, prof);
  const st = newState();
  let ex: FxExtras = { gobo: null, prism: false, frost: 0 };
  if (plan) { decode(plan, fx, frame, time, st); ex = decodeExtras(plan, fx, frame); }
  // Fixtures with neither a dimmer nor colour channels (strobes, blinders) are "on" only if any channel is driven.
  if (plan && !plan.hasDimmer && !plan.hasRgb) {
    let any = false;
    for (const c of plan.channels) {
      const i = fx.address - 1 + c.o;
      if (frame && i >= 0 && i < 512 && frame[i] > 0) { any = true; break; }
    }
    if (!any) st.bright = 0;
  }
  const R = eulerYXZ([fx.rot[0], fx.rot[1], fx.rot[2]]);
  const pos: V3 = [fx.pos[0], fx.pos[1], fx.pos[2]];
  const moving = !!plan?.moving;
  let P = 0, T = 0;
  let localOrigin: V3, localDir: V3, localPivot: V3;
  if (moving && plan) {
    P = (st.pan - 0.5) * plan.panRange * D2R;
    T = (st.tilt - 0.5) * plan.tiltRange * D2R;
    const sP = Math.sin(P), cP = Math.cos(P), sT = Math.sin(T), cT = Math.cos(T);
    localDir = [-sT * sP, -cT, -sT * cP];
    localOrigin = [-0.312 * sT * sP, -0.31 - 0.312 * cT, -0.312 * sT * cP];
    localPivot = [0, -0.09, 0];
  } else {
    localDir = [0, -1, 0];
    localOrigin = [0, -0.2, 0];
    localPivot = [0, -0.1, 0];
  }
  const origin = add(pos, apply(R, localOrigin));
  if (origin[1] < 0.02) origin[1] = 0.02;
  const dir = norm(apply(R, localDir));
  const pivot = add(pos, apply(R, localPivot));
  const lit = st.bright > 0.004;
  const g: FxGeo = {
    fx, prof, plan, st, ex, R, pos, moving, glyph: glyphKind(prof, plan), P, T, pivot, origin, dir, beam: null, lit,
  };
  const canBeam = withBeam && plan && g.glyph !== "fog" && g.glyph !== "other";
  if (canBeam && plan) {
    const half = Math.max(0.01, (angleOf(plan, st) * D2R) / 2);
    g.beam = beamOf(origin, dir, half, obs);
  }
  return g;
}

/** Beam geometry even for unlit fixtures (used for aim handles). */
export function beamOf(origin: V3, dir: V3, half: number, obs: Obb[]): BeamGeo {
  const ax = trace(origin, dir, obs);
  const hit = add(origin, mul(dir, ax.t));
  // orthonormal basis around the axis
  const up: V3 = Math.abs(dir[1]) < 0.95 ? [0, 1, 0] : [1, 0, 0];
  const u = norm(cross(dir, up));
  const v = cross(dir, u);
  const ends: V3[] = [];
  const floorEnds: V3[] = [];
  const cs = Math.cos(half), sn = Math.sin(half);
  for (let k = 0; k < RING; k++) {
    const a = (k / RING) * Math.PI * 2;
    const r: V3 = norm([
      dir[0] * cs + (u[0] * Math.cos(a) + v[0] * Math.sin(a)) * sn,
      dir[1] * cs + (u[1] * Math.cos(a) + v[1] * Math.sin(a)) * sn,
      dir[2] * cs + (u[2] * Math.cos(a) + v[2] * Math.sin(a)) * sn,
    ]);
    const tr = trace(origin, r, obs);
    const e = add(origin, mul(r, tr.t));
    ends.push(e);
    if (tr.kind === "floor") floorEnds.push(e);
  }
  return { origin, dir, half, hit, dist: ax.t, hitKind: ax.kind, ends, floorEnds };
}

export { boxCorners };
