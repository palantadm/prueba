// UI-only shared state for Live / Cues.
import { signal } from "@preact/signals";

/** Cuelist targeted by transport, Space=GO and "record cue". */
export const activeCuelist = signal<number | null>(null);
export const lsGet = <T,>(k: string, d: T): T => {
  try { const v = localStorage.getItem(k); return v ? (JSON.parse(v) as T) : d; } catch { return d; }
};
export const lsSet = (k: string, v: unknown) => { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* ignore */ } };
export const colorHistory = signal<string[]>(lsGet("lumos.colorHistory", []));
export function pushColor(hex: string) {
  const l = [hex, ...colorHistory.value.filter((c) => c !== hex)].slice(0, 12);
  colorHistory.value = l;
  lsSet("lumos.colorHistory", l);
}

/** Live attribute area: faders/knobs or the fixture sheet. */
export const attrView = signal<"faders" | "sheet">(lsGet("lumos.attrView", "faders"));
export const setAttrView = (v: "faders" | "sheet") => { attrView.value = v; lsSet("lumos.attrView", v); };
/** Palette to reveal in the Live pools (set from the cue editor's palette badges). */
export const focusPalette = signal<number | null>(null);
