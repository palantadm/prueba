// UI state + debounced edit layer for the 2D workspace (independent of the 3D module's state).
import { signal } from "@preact/signals";
import { send, show } from "../store";
import type { Fixture, Stage, StageObject } from "../types";
import type { ViewKind } from "./math";

function persisted<T>(key: string, init: T) {
  let v = init;
  try {
    const raw = localStorage.getItem("plan." + key);
    if (raw !== null) v = JSON.parse(raw) as T;
  } catch { /* storage unavailable */ }
  const s = signal<T>(v);
  s.subscribe((x) => { try { localStorage.setItem("plan." + key, JSON.stringify(x)); } catch { /* ignore */ } });
  return s;
}

export type Tool = "select" | "hand" | "aim";
export type LabelMode = "id" | "name" | "addr" | "none";
export type AimMode = "stack" | "fan";

export const view = persisted<ViewKind>("view", "plan");
export const tool = signal<Tool>("select");
export const editLayout = persisted("edit", true);
export const aimMode = persisted<AimMode>("aimmode", "stack");
export const labelMode = persisted<LabelMode>("label", "id");
export const layers = persisted("layers", { beams: true, footprints: true, grid: true, objects: true, background: true, handles: true, smoke: true });
export type Layers = typeof layers.value;
if (layers.value.smoke === undefined) layers.value = { ...layers.value, smoke: true };
export const selObj = signal<number | null>(null);
export const hud = signal({ x: 0, y: 0, z: 0, scale: 40, fps: 0 });
export const menu = signal<{ x: number; y: number; items: MenuItem[] } | null>(null);
export interface MenuItem { label: string; run?: () => void; sep?: boolean; disabled?: boolean }

// ── floor-plan background (per show, kept client-side) ──
export interface Bg { url: string; cx: number; cz: number; width: number; rot: number; opacity: number; aspect: number }
export const bgStore = persisted<Record<string, Bg>>("bg", {});
export const bgName = () => show.value?.name ?? "";
export const currentBg = (): Bg | null => bgStore.value[bgName()] ?? null;
export function setBg(patch: Partial<Bg> | null) {
  const name = bgName();
  const all = { ...bgStore.value };
  if (patch === null) delete all[name];
  else {
    const cur = all[name] ?? { url: "", cx: 0, cz: 0, width: 10, rot: 0, opacity: 0.5, aspect: 1 };
    all[name] = { ...cur, ...patch };
  }
  bgStore.value = all;
}

// ── drafts (optimistic edits shown immediately, committed debounced) ──
let draftStage: Stage | null = null;
const draftFx = new Map<number, Fixture>();
let stageTimer: ReturnType<typeof setTimeout> | undefined;
let stageRelease: ReturnType<typeof setTimeout> | undefined;
const fxTimers = new Map<number, ReturnType<typeof setTimeout>>();
const fxRelease = new Map<number, ReturnType<typeof setTimeout>>();

export const effStage = (): Stage | null => draftStage ?? show.value?.stage ?? null;
export function effFixtures(): Fixture[] {
  const base = show.value?.fixtures ?? [];
  return draftFx.size === 0 ? base : base.map((f) => draftFx.get(f.id) ?? f);
}
const clone = <T,>(x: T): T => JSON.parse(JSON.stringify(x));

export function editFixture(id: number, mut: (f: Fixture) => void, delay = 250) {
  const cur = effFixtures().find((f) => f.id === id);
  if (!cur) return;
  const next = clone(cur);
  mut(next);
  draftFx.set(id, next);
  clearTimeout(fxTimers.get(id));
  clearTimeout(fxRelease.get(id));
  fxTimers.set(id, setTimeout(() => {
    const f = draftFx.get(id);
    if (!f) return;
    send({ type: "fixture_update", fixture: f }).catch(() => undefined).finally(() => {
      fxRelease.set(id, setTimeout(() => draftFx.delete(id), 1500));
    });
  }, delay));
}

export function editObject(id: number, mut: (o: StageObject) => void, delay = 250) {
  const cur = effStage();
  if (!cur) return;
  const next = clone(cur);
  const o = next.objects.find((x) => x.id === id);
  if (!o) return;
  mut(o);
  draftStage = next;
  clearTimeout(stageTimer);
  clearTimeout(stageRelease);
  stageTimer = setTimeout(() => {
    const s = draftStage;
    if (!s) return;
    send({ type: "stage_save", stage: s }).catch(() => undefined).finally(() => {
      stageRelease = setTimeout(() => { draftStage = null; }, 1500);
    });
  }, delay);
}
