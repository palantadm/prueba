// Programmer helpers: capability-aware prog_set with write coalescing.
import type { Fixture } from "../types";
type Attrs = Record<string, number>;
import { fire, rt, selection } from "../store";
import { fixtureById, hasAttr, readAttr } from "./attrs";

const pending = new Map<string, { fixtures: number[]; attrs: Attrs }>();
let timer = 0;
function flush() {
  timer = 0;
  for (const p of pending.values()) fire({ type: "prog_set", fixtures: p.fixtures, attrs: p.attrs });
  pending.clear();
}

/** Set attributes on fixtures; each fixture only receives attributes its mode supports. Coalesced to ~33 Hz. */
export function setAttrs(ids: number[], attrs: Record<string, number>) {
  const groups = new Map<string, { fixtures: number[]; attrs: Attrs }>();
  for (const id of ids) {
    const fx = fixtureById(id);
    if (!fx) continue;
    const sup: Attrs = {};
    for (const [a, v] of Object.entries(attrs)) if (hasAttr(fx, a)) sup[a] = Math.max(0, Math.min(1, v));
    const ks = Object.keys(sup).sort();
    if (!ks.length) continue;
    const gk = ks.join(",");
    const g = groups.get(gk) ?? { fixtures: [], attrs: sup };
    g.fixtures.push(id);
    groups.set(gk, g);
  }
  for (const [gk, g] of groups) {
    pending.set(gk + "|" + g.fixtures.join(","), g);
  }
  if (!timer) timer = window.setTimeout(flush, 30);
}

/** Set per-fixture attrs (different value per fixture, e.g. fan). */
export function setPerFixture(values: Map<number, Record<string, number>>) {
  for (const [id, a] of values) setAttrs([id], a);
}

export function releaseAttrs(ids: number[] | null, attrs: string[] | null) {
  fire({ type: "prog_release", fixtures: ids, attrs });
}

export const heldBy = (id: number) => rt.value?.programmer[id];

/** Value to display for an attribute over a selection. */
export function attrState(sel: Fixture[], attr: string): { value: number; held: boolean; has: boolean } {
  let has = false;
  for (const fx of sel) {
    if (!hasAttr(fx, attr)) continue;
    has = true;
    const h = rt.value?.programmer[fx.id]?.[attr];
    if (h !== undefined) return { value: h, held: true, has };
  }
  for (const fx of sel) {
    if (!hasAttr(fx, attr)) continue;
    const v = readAttr(fx, attr);
    if (v !== undefined) return { value: v, held: false, has };
  }
  return { value: attr === "pan" || attr === "tilt" ? 0.5 : 0, held: false, has };
}

export function selectedFixtures(): Fixture[] {
  return selection.value.map((i) => fixtureById(i)).filter((f): f is Fixture => !!f);
}
