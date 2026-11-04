// Decode a fixture's DMX footprint into a visual state for the 3D previsualiser.
import type { Capability, Fixture, FixtureProfile } from "../types";

interface Ch { attr: string; o: number; f: number; inv: boolean; caps: Capability[] }
export interface Plan {
  prof: FixtureProfile;
  channels: Ch[];
  moving: boolean;
  hasDimmer: boolean;
  hasRgb: boolean;
  panRange: number;
  tiltRange: number;
}

export interface FxState {
  /** 0..1 final brightness (dimmer × colour intensity × shutter). */
  bright: number;
  r: number; g: number; b: number; // colour, max component = 1
  pan: number; tilt: number; // 0..1
  zoom: number; iris: number;
  strobeHz: number;
  closed: boolean;
  /** selected gobo capability (null = open / none) */
  goboCap: Capability | null;
  /** gobo_rot channel: 0..1 raw value and whether it behaves as a speed (continuous spin) or a position */
  goboRot: number;
  goboRotSpeed: boolean;
  frost: number; focus: number;
  prism: boolean;
  /** colour-wheel slot colour when a wheel is in the path (rgb 0..1) */
  wheel: [number, number, number] | null;
}

export function newState(): FxState {
  return {
    bright: 0, r: 1, g: 1, b: 1, pan: 0.5, tilt: 0.5, zoom: 0.5, iris: 0, strobeHz: 0, closed: false,
    goboCap: null, goboRot: 0, goboRotSpeed: false, frost: 0, focus: 0.5, prism: false, wheel: null,
  };
}

export const isOpenCap = (c: Capability | null | undefined) => !c || /open|none|empty|blank|white|off|hole|flat field/i.test(c.label);

export function makePlan(fx: Fixture, prof: FixtureProfile | undefined): Plan | null {
  if (!prof) return null;
  const mode = prof.modes.find((m) => m.name === fx.mode) ?? prof.modes[0];
  if (!mode) return null;
  const channels = mode.channels.map((c) => ({
    attr: c.attr, o: c.offset, f: c.fine ?? -1, inv: c.invert, caps: c.capabilities,
  }));
  const has = (a: string) => channels.some((c) => c.attr === a);
  const hasPT = has("pan") || has("tilt");
  const moving = hasPT && (prof.category === "spot" || prof.category === "beam" || prof.category === "wash" || prof.category === "effect" || prof.pan_range > 0 || prof.tilt_range > 0);
  const plan: Plan = {
    prof, channels, moving,
    hasDimmer: has("dimmer"),
    hasRgb: has("red") || has("green") || has("blue"),
    panRange: prof.pan_range > 0 ? prof.pan_range : 540,
    tiltRange: prof.tilt_range > 0 ? prof.tilt_range : 270,
  };
  return plan;
}

const hexCache = new Map<string, [number, number, number]>();
function hex(c: string): [number, number, number] {
  let v = hexCache.get(c);
  if (!v) {
    const n = parseInt(c.replace("#", "").padEnd(6, "0").slice(0, 6), 16);
    v = [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
    hexCache.set(c, v);
  }
  return v;
}

function capAt(caps: Capability[], v8: number): Capability | undefined {
  for (const c of caps) if (v8 >= c.min && v8 <= c.max) return c;
  return undefined;
}

export function decode(plan: Plan, fx: Fixture, frame: Uint8Array | undefined, t: number, out: FxState) {
  let dimmer = 1, r = 0, g = 0, b = 0, w = 0, am = 0, uv = 0, lime = 0, cto = 0;
  let pan = 0.5, tilt = 0.5, zoom = 0.5, iris = 0;
  let wheel: [number, number, number] | null = null;
  let strobeHz = 0, closed = false, goboDim = 1;
  let hasShutter = false;
  let goboCap: Capability | null = null, goboRot = 0, goboRotSpeed = false, frost = 0, focus = 0.5, prism = false;
  let cy = 0, mg = 0, ye = 0, hasCmy = false;
  const base = fx.address - 1;
  for (const c of plan.channels) {
    const i = base + c.o;
    let v8 = 0, v = 0;
    if (frame && i >= 0 && i < 512) {
      v8 = frame[i];
      if (c.f >= 0 && base + c.f < 512) v = (v8 * 256 + frame[base + c.f]) / 65535;
      else v = v8 / 255;
    }
    if (c.inv) v = 1 - v;
    switch (c.attr) {
      case "dimmer": dimmer = v; break;
      case "red": r = v; break;
      case "green": g = v; break;
      case "blue": b = v; break;
      case "white": w = v; break;
      case "amber": am = v; break;
      case "uv": uv = v; break;
      case "lime": lime = v; break;
      case "cto": cto = v; break;
      case "pan": pan = v; break;
      case "tilt": tilt = v; break;
      case "zoom": zoom = v; break;
      case "iris": iris = v; break;
      case "colorwheel": {
        const cap = capAt(c.caps, v8);
        if (cap?.color) wheel = hex(cap.color);
        break;
      }
      case "gobo":
      case "gobo2": {
        const cap = capAt(c.caps, v8);
        if (v8 > 0 && cap && !isOpenCap(cap)) { goboDim = 0.75; goboCap = cap; }
        break;
      }
      case "colorsub_c": cy = v; hasCmy = true; break;
      case "colorsub_m": mg = v; hasCmy = true; break;
      case "colorsub_y": ye = v; hasCmy = true; break;
      case "gobo_rot":
      case "gobo2_rot": {
        goboRot = v;
        const cap = capAt(c.caps, v8);
        goboRotSpeed = !!cap && /rot|spin|cw|ccw|slow|fast|speed/i.test(cap.label);
        break;
      }
      case "frost": frost = v; break;
      case "focus": focus = v; break;
      case "prism": { const cap = capAt(c.caps, v8); prism = cap ? !isOpenCap(cap) && !/raw/i.test(cap.label) : v8 > 4; break; }
      case "shutter": {
        hasShutter = true;
        const cap = capAt(c.caps, v8);
        if (cap) {
          const lbl = cap.label.toLowerCase();
          if (/clos|black/.test(lbl)) closed = true;
          else if (/strobe|puls|flash/.test(lbl)) {
            const span = Math.max(1, cap.max - cap.min);
            strobeHz = 1 + ((v8 - cap.min) / span) * 19;
          }
        } else if (v8 > 25) {
          strobeHz = 1 + ((v8 - 25) / 230) * 19;
        }
        break;
      }
    }
  }
  void hasShutter;
  // colour mix
  let cr = r + w, cg = g + w, cb = b + w;
  if (am) { cr += am; cg += am * 0.62; }
  if (uv) { cb += uv * 0.55; cr += uv * 0.18; }
  if (lime) { cr += lime * 0.55; cg += lime; }
  if (wheel && !plan.hasRgb) { cr = wheel[0]; cg = wheel[1]; cb = wheel[2]; if (!plan.hasDimmer) { /* wheel fixtures are always on */ } }
  const colourless = !plan.hasRgb && w === 0 && am === 0 && uv === 0 && lime === 0;
  if (colourless) { cr = wheel ? wheel[0] : 1; cg = wheel ? wheel[1] : 1; cb = wheel ? wheel[2] : 1; }
  if (cto > 0) { cr *= 1; cg *= 1 - cto * 0.25; cb *= 1 - cto * 0.6; }
  if (hasCmy) { cr *= 1 - cy; cg *= 1 - mg; cb *= 1 - ye; }
  const m = Math.max(cr, cg, cb);
  const inten = Math.min(1, m);
  if (m > 1e-5) { out.r = cr / m; out.g = cg / m; out.b = cb / m; } else { out.r = out.g = out.b = 1; }
  let bright = (plan.hasDimmer ? dimmer : 1) * inten * goboDim;
  if (closed) bright = 0;
  if (strobeHz > 0 && Math.floor(t * strobeHz * 2) % 2 === 1) bright = 0;
  out.bright = bright;
  out.pan = pan; out.tilt = tilt; out.zoom = zoom; out.iris = iris;
  out.strobeHz = strobeHz; out.closed = closed;
  out.goboCap = goboCap; out.goboRot = goboRot; out.goboRotSpeed = goboRotSpeed;
  out.frost = frost; out.focus = focus; out.prism = prism; out.wheel = wheel;
}

/** Beam full-angle in degrees. */
export function beamAngle(plan: Plan, st: FxState): number {
  const [lo, hi] = plan.prof.beam_angle;
  return (lo + (hi - lo) * st.zoom) * (1 - 0.8 * st.iris);
}

/** Pan/tilt (0..1) so that a fixture's beam points along `dirLocal` (unit vector in the fixture's own frame; down = -Y). */
export function aimPanTilt(plan: Plan, dx: number, dy: number, dz: number): { pan: number; tilt: number } {
  // beam dir = Ry(pan)·Rx(tilt)·(0,-1,0) = (-sinT·sinP, -cosT, -sinT·cosP)
  const T = Math.acos(Math.max(-1, Math.min(1, -dy)));
  const s = Math.hypot(dx, dz);
  let P = s > 1e-6 ? Math.atan2(-dx, -dz) : 0;
  let tl = T;
  const deg = 180 / Math.PI;
  const panHalf = plan.panRange / 2, tiltHalf = plan.tiltRange / 2;
  const fits = (p: number, t: number) => Math.abs(p * deg) <= panHalf + 0.01 && Math.abs(t * deg) <= tiltHalf + 0.01;
  if (!fits(P, tl)) {
    const P2 = P > 0 ? P - Math.PI : P + Math.PI;
    if (fits(P2, -tl)) { P = P2; tl = -tl; }
  }
  const clamp = (v: number) => Math.max(0, Math.min(1, v));
  return { pan: clamp(0.5 + (P * deg) / plan.panRange), tilt: clamp(0.5 + (tl * deg) / plan.tiltRange) };
}

// ── additive: extra look data for the 2D visualiser (gobo slot, prism, frost) ──
export interface FxExtras {
  /** Selected gobo slot (null = open / none). `image` is a gobo image URL when the profile provides one. */
  gobo: { label: string; index: number; image: string | null } | null;
  prism: boolean;
  frost: number;
}

export function decodeExtras(plan: Plan, fx: Fixture, frame: Uint8Array | undefined): FxExtras {
  const out: FxExtras = { gobo: null, prism: false, frost: 0 };
  if (!frame) return out;
  const base = fx.address - 1;
  for (const c of plan.channels) {
    const i = base + c.o;
    if (i < 0 || i >= 512) continue;
    const v8 = frame[i];
    if (c.attr === "gobo") {
      const cap = capAt(c.caps, v8);
      if (cap && v8 > 4 && !/open|none|empty/i.test(cap.label)) {
        out.gobo = { label: cap.label, index: c.caps.indexOf(cap), image: cap.image ?? null };
      }
    } else if (c.attr === "prism") {
      out.prism = v8 > 8;
    } else if (c.attr === "frost") {
      out.frost = v8 / 255;
    }
  }
  return out;
}
