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
