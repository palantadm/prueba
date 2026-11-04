// Attribute metadata + profile/fixture helpers shared by Live / Desk / Cues.
import type { ChannelDef, Fixture, FixtureProfile, Mode } from "../types";
import { dmx, library, show } from "../store";
import { t, lang } from "../i18n";
import { capLabelsEs } from "../i18n/es/live";

export type AttrGroup = "intensity" | "position" | "color" | "beam" | "control";

export const ATTR_META: Record<string, { group: AttrGroup; label: string }> = {
  dimmer: { group: "intensity", label: "Dimmer" },
  pan: { group: "position", label: "Pan" },
  tilt: { group: "position", label: "Tilt" },
  red: { group: "color", label: "Red" },
  green: { group: "color", label: "Green" },
  blue: { group: "color", label: "Blue" },
  white: { group: "color", label: "White" },
  amber: { group: "color", label: "Amber" },
  uv: { group: "color", label: "UV" },
  lime: { group: "color", label: "Lime" },
  cto: { group: "color", label: "CTO" },
  colorwheel: { group: "color", label: "Color wheel" },
  gobo: { group: "beam", label: "Gobo" },
  gobo_rot: { group: "beam", label: "Gobo rot" },
  prism: { group: "beam", label: "Prism" },
  prism_rot: { group: "beam", label: "Prism rot" },
  frost: { group: "beam", label: "Frost" },
  zoom: { group: "beam", label: "Zoom" },
  focus: { group: "beam", label: "Focus" },
  iris: { group: "beam", label: "Iris" },
  shutter: { group: "beam", label: "Shutter/Strobe" },
  speed: { group: "control", label: "Speed" },
  effect: { group: "control", label: "Effect" },
  control: { group: "control", label: "Control" },
};
export const ALL_ATTRS = Object.keys(ATTR_META);

export const GROUP_COLOR: Record<AttrGroup, string> = {
  intensity: "var(--attr-intensity)",
  position: "var(--attr-position)",
  color: "var(--attr-color)",
  beam: "var(--attr-beam)",
  control: "var(--attr-control)",
};
export const attrColor = (a: string) => GROUP_COLOR[ATTR_META[a]?.group ?? "control"];
export const attrLabel = (a: string) => (ATTR_META[a] ? t(ATTR_META[a].label) : a);
/** Slot label from a fixture profile (data); translated only for known built-in labels. */
export const capLabel = (l: string) => (lang.value === "es" ? capLabelsEs[l] ?? l : l);
export const attrHome = (a: string) => (a === "pan" || a === "tilt" ? 0.5 : 0);

// ── lookups (cached by identity of the source arrays) ─────────────
let profCache: { src: FixtureProfile[]; map: Map<string, FixtureProfile> } | null = null;
export function profileById(id: string): FixtureProfile | undefined {
  const src = library.value;
  if (!profCache || profCache.src !== src) profCache = { src, map: new Map(src.map((p) => [p.id, p])) };
  return profCache.map.get(id);
}
let fxCache: { src: Fixture[]; map: Map<number, Fixture> } | null = null;
export function fixtureById(id: number): Fixture | undefined {
  const src = show.value?.fixtures ?? [];
  if (!fxCache || fxCache.src !== src) fxCache = { src, map: new Map(src.map((f) => [f.id, f])) };
  return fxCache.map.get(id);
}
export function modeOf(fx: Fixture): Mode | undefined {
  return profileById(fx.profile)?.modes.find((m) => m.name === fx.mode);
}
export function channelOf(fx: Fixture, attr: string): ChannelDef | undefined {
  return modeOf(fx)?.channels.find((c) => c.attr === attr);
}
export function hasAttr(fx: Fixture, attr: string) {
  return !!channelOf(fx, attr);
}
export function fixtureAttrs(fx: Fixture): string[] {
  return modeOf(fx)?.channels.map((c) => c.attr) ?? [];
}
export function unionAttrs(fxs: Fixture[]): Set<string> {
  const s = new Set<string>();
  for (const f of fxs) for (const a of fixtureAttrs(f)) s.add(a);
  return s;
}
export function footprint(fx: Fixture): number {
  const m = modeOf(fx);
  if (!m) return 1;
  return m.channels.reduce((n, c) => Math.max(n, c.offset + 1, (c.fine ?? -1) + 1), 1);
}

// ── live value decoding from DMX frames ───────────────────────────
export function readChannel(fx: Fixture, ch: ChannelDef): number | undefined {
  const f = dmx.get(fx.universe);
  if (!f) return undefined;
  const a = fx.address - 1 + ch.offset;
  if (a < 0 || a > 511) return undefined;
  let v = f[a] / 255;
  if (ch.fine != null) {
    const b = fx.address - 1 + ch.fine;
    if (b >= 0 && b < 512) v = (f[a] * 256 + f[b]) / 65535;
  }
  return ch.invert ? 1 - v : v;
}
export function readAttr(fx: Fixture, attr: string): number | undefined {
  const c = channelOf(fx, attr);
  return c ? readChannel(fx, c) : undefined;
}
/** 0..1 visible output level: dimmer, else brightest colour channel. */
export function fixtureLevel(fx: Fixture): number {
  const d = readAttr(fx, "dimmer");
  if (d !== undefined) return d;
  let m = 0;
  for (const a of ["red", "green", "blue", "white", "amber"]) m = Math.max(m, readAttr(fx, a) ?? 0);
  return m;
}
export function fixtureRgb(fx: Fixture): [number, number, number] {
  return [readAttr(fx, "red") ?? 0, readAttr(fx, "green") ?? 0, readAttr(fx, "blue") ?? 0];
}

/** Which channel (if any) of a patched fixture covers universe/channel (1-based). */
export function channelOwner(universe: number, channel: number): { fx: Fixture; attr: string; fine: boolean } | undefined {
  for (const fx of show.value?.fixtures ?? []) {
    if (fx.universe !== universe) continue;
    const off = channel - fx.address;
    if (off < 0) continue;
    const m = modeOf(fx);
    if (!m) continue;
    for (const c of m.channels) {
      if (c.offset === off) return { fx, attr: c.attr, fine: false };
      if (c.fine === off) return { fx, attr: c.attr, fine: true };
    }
  }
  return undefined;
}

export const pct = (v: number) => `${Math.round(v * 100)}%`;
export const fmtNum = (n: number) => (Number.isInteger(n) ? String(n) : String(+n.toFixed(3)));

export function hexToRgb(h: string): [number, number, number] | null {
  const m = /^#?([0-9a-f]{6})$/i.exec(h.trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}
export function rgbToHex(r: number, g: number, b: number): string {
  const c = (x: number) => Math.round(Math.max(0, Math.min(1, x)) * 255).toString(16).padStart(2, "0");
  return `#${c(r)}${c(g)}${c(b)}`;
}
export function hsvToRgb(h: number, s: number, v: number): [number, number, number] {
  const i = Math.floor(h * 6) % 6, f = h * 6 - Math.floor(h * 6);
  const p = v * (1 - s), q = v * (1 - f * s), t = v * (1 - (1 - f) * s);
  return [[v, q, p, p, t, v][i], [t, v, v, q, p, p][i], [p, p, t, v, v, q][i]] as [number, number, number];
}
export function rgbToHsv(r: number, g: number, b: number): [number, number, number] {
  const mx = Math.max(r, g, b), mn = Math.min(r, g, b), d = mx - mn;
  let h = 0;
  if (d > 0) {
    if (mx === r) h = ((g - b) / d + 6) % 6;
    else if (mx === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h /= 6;
  }
  return [h, mx === 0 ? 0 : d / mx, mx];
}
