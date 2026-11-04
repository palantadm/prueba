import type { Fixture, FixtureProfile, Mode } from "../types";
import { library, show } from "../store";

export function profileById(id: string): FixtureProfile | undefined {
  return show.value?.profiles.find((p) => p.id === id) ?? library.value.find((p) => p.id === id);
}
export function modeOf(f: Fixture): Mode | undefined {
  return profileById(f.profile)?.modes.find((m) => m.name === f.mode);
}
export function footprint(m: Mode | undefined): number {
  if (!m) return 1;
  return m.channels.reduce((n, c) => Math.max(n, c.offset + 1, (c.fine ?? -1) + 1), 0) || 1;
}
export function hueFor(key: string): number {
  let h = 0;
  for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) >>> 0;
  return h % 360;
}

/** Fixture ids that overlap another fixture in the same universe (or run past channel 512). */
export function collisions(fixtures: Fixture[]): Set<number> {
  const bad = new Set<number>();
  const byU = new Map<number, { id: number; a: number; b: number }[]>();
  for (const f of fixtures) {
    const n = footprint(modeOf(f));
    const r = { id: f.id, a: f.address, b: f.address + n - 1 };
    if (r.b > 512 || r.a < 1) bad.add(f.id);
    (byU.get(f.universe) ?? byU.set(f.universe, []).get(f.universe)!).push(r);
  }
  for (const list of byU.values()) {
    list.sort((x, y) => x.a - y.a);
    let maxEnd = -1, maxId = -1;
    for (const r of list) {
      if (r.a <= maxEnd) { bad.add(r.id); bad.add(maxId); }
      if (r.b > maxEnd) { maxEnd = r.b; maxId = r.id; }
    }
  }
  return bad;
}
