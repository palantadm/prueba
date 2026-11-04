// Parked DMX channels (server-held overrides). Parking = dmx_set with a value; release via unpark.
import { signal } from "@preact/signals";
import { fire, rt } from "../store";
import type { ParkedChannel } from "../types";

/** Optimistic local copy of overrides so faders respond instantly (key "universe:channel"). */
export const overrides = signal<Record<string, number>>({});
export const okey = (u: number, c: number) => `${u}:${c}`;

export const parkedList = (): ParkedChannel[] => rt.value?.parked ?? [];
export const parkedSet = (universe: number): Set<number> =>
  new Set(parkedList().filter((p) => p.universe === universe).map((p) => p.channel));
export const parkedValue = (universe: number, channel: number): number | undefined =>
  parkedList().find((p) => p.universe === universe && p.channel === channel)?.value;

/** Park a channel at a value (also used by the Desk faders). `null` releases. */
export function setOverride(universe: number, channel: number, value: number | null) {
  const o = { ...overrides.value };
  if (value === null) delete o[okey(universe, channel)];
  else o[okey(universe, channel)] = Math.max(0, Math.min(255, Math.round(value)));
  overrides.value = o;
  fire({ type: "dmx_set", universe, channel, value: value === null ? null : o[okey(universe, channel)] });
}
export const parkChannel = (universe: number, channel: number, value: number) => setOverride(universe, channel, value);
export function unparkChannel(universe: number, channel: number) {
  setOverride(universe, channel, null);
  fire({ type: "unpark", universe, channel });
}
export function unparkAll() {
  const parked = parkedList();
  overrides.value = {};
  for (const p of parked) fire({ type: "dmx_set", universe: p.universe, channel: p.channel, value: null });
  fire({ type: "unpark", universe: null, channel: null });
}
