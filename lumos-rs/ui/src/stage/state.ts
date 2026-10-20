// UI state + debounced edit/commit layer for the 3D workspace.
import { signal } from "@preact/signals";
import { send, show } from "../store";
import type { Fixture, Stage, StageObject } from "../types";

function persisted<T>(key: string, init: T) {
  let v = init;
  try {
    const raw = localStorage.getItem("stage." + key);
    if (raw !== null) v = JSON.parse(raw) as T;
  } catch { /* storage unavailable */ }
  const s = signal<T>(v);
  s.subscribe((x) => { try { localStorage.setItem("stage." + key, JSON.stringify(x)); } catch { /* ignore */ } });
  return s;
}

export type GizmoMode = "translate" | "rotate" | "scale";
export type CamPreset = "audience" | "top" | "side" | "iso" | "free";
export type QualityPref = "auto" | "low" | "med" | "high";
export type VenueMode = "blackbox" | "theatre" | "cyc" | "open";

export const editMode = persisted("edit", true);
export const gizmoMode = signal<GizmoMode>("translate");
export const snapOn = persisted("snap", true);
export const aimTool = signal(false);
export const showBeams = persisted("beams", true);
export const showPools = persisted("pools", true);
export const showGrid = persisted("grid", true);
export const showLabels = persisted("labels", false);
export const workLight = persisted("work", false);
export const haze = persisted("haze", 0.35);
export const camPreset = persisted<CamPreset>("preset", "iso");
export const quality = persisted<QualityPref>("quality", "auto");
export const bloomOn = persisted("bloom", true);
export const venueMode = persisted<VenueMode>("venue", "theatre");
export const showAudience = persisted("audience", false);
export const showMannequin = persisted("mannequin", false);
export const selObject = signal<number | null>(null);
export const hiddenObjs = persisted<number[]>("hidden", []);
export const stats = signal({ fps: 0, calls: 0, lights: 0, q: "med" as string, warn: "" });
export const draftVersion = signal(0);

// ── drafts ──────────────────────────────────────────────
let draftStage: Stage | null = null;
const draftFx = new Map<number, Fixture>();
let stageTimer: ReturnType<typeof setTimeout> | undefined;
const fxTimers = new Map<number, ReturnType<typeof setTimeout>>();
let stageRelease: ReturnType<typeof setTimeout> | undefined;
const fxRelease = new Map<number, ReturnType<typeof setTimeout>>();

export function effStage(): Stage | null {
  return draftStage ?? show.value?.stage ?? null;
}

export function effFixtures(): Fixture[] {
  const base = show.value?.fixtures ?? [];
  if (draftFx.size === 0) return base;
  return base.map((f) => draftFx.get(f.id) ?? f);
}

const clone = <T,>(x: T): T => JSON.parse(JSON.stringify(x));

export function editStage(mut: (s: Stage) => void, delay = 300) {
  const cur = effStage();
  if (!cur) return;
  const next = clone(cur);
  mut(next);
  draftStage = next;
  draftVersion.value++;
  clearTimeout(stageTimer);
  clearTimeout(stageRelease);
  stageTimer = setTimeout(() => {
    const s = draftStage;
    if (!s) return;
    send({ type: "stage_save", stage: s }).catch(() => undefined).finally(() => {
      stageRelease = setTimeout(() => { draftStage = null; draftVersion.value++; }, 1500);
    });
  }, delay);
}

export function editObject(id: number, mut: (o: StageObject) => void, delay = 300) {
  editStage((s) => { const o = s.objects.find((x) => x.id === id); if (o) mut(o); }, delay);
}

export function editFixture(id: number, mut: (f: Fixture) => void, delay = 300) {
  const cur = effFixtures().find((f) => f.id === id);
  if (!cur) return;
  const next = clone(cur);
  mut(next);
  draftFx.set(id, next);
  draftVersion.value++;
  clearTimeout(fxTimers.get(id));
  clearTimeout(fxRelease.get(id));
  fxTimers.set(id, setTimeout(() => {
    const f = draftFx.get(id);
    if (!f) return;
    send({ type: "fixture_update", fixture: f }).catch(() => undefined).finally(() => {
      fxRelease.set(id, setTimeout(() => { draftFx.delete(id); draftVersion.value++; }, 1500));
    });
  }, delay));
}

export function nextObjectId(): number {
  const objs = effStage()?.objects ?? [];
  return objs.reduce((m, o) => Math.max(m, o.id), 0) + 1;
}

export function toggleHidden(id: number) {
  const s = new Set(hiddenObjs.value);
  s.has(id) ? s.delete(id) : s.add(id);
  hiddenObjs.value = [...s];
}

// ── object templates ────────────────────────────────────
export type Template = { label: string; make: (stage: Stage) => Omit<StageObject, "id"> };

const base = (kind: StageObject["kind"], name: string, pos: [number, number, number], size: [number, number, number], color: string, extra: Partial<StageObject> = {}): Omit<StageObject, "id"> => ({
  kind, name, pos, rot: [0, 0, 0], size, color, media: null, screen_type: null, brightness: 1, ...extra,
});

export const TEMPLATES: Template[] = [
  { label: "Truss line above stage", make: (s) => base("truss", "Truss", [0, s.size[2] - 0.3, 0], [s.size[0] * 0.9, 0.3, 0.3], "#9aa0a8") },
  { label: "Truss 3 m", make: (s) => base("truss", "Truss", [0, Math.min(3, s.size[2]), 0], [3, 0.3, 0.3], "#9aa0a8") },
  { label: "Platform", make: () => base("platform", "Platform", [0, 0.2, 0], [2, 0.4, 2], "#2a2d33") },
  { label: "Wall", make: (s) => base("wall", "Wall", [0, 1.5, -s.size[1] / 2 + 0.2], [6, 3, 0.1], "#1d2026") },
  { label: "Column", make: () => base("column", "Column", [2, 1.5, 0], [0.4, 3, 0.4], "#3a3d44") },
  { label: "Curtain", make: (s) => base("curtain", "Curtain", [0, 2, -s.size[1] / 2 + 0.1], [6, 4, 0.2], "#5a0d14") },
  { label: "Tree", make: () => base("tree", "Tree", [3, 1.25, -1], [1.6, 2.5, 1.6], "#2f6b34") },
  { label: "Box", make: () => base("box", "Box", [0, 0.5, 0], [1, 1, 1], "#6b7280") },
  { label: "LED screen 16:9", make: (s) => base("screen", "LED screen", [0, 2.2, -s.size[1] / 2 + 0.3], [4, 2.25, 0.1], "#111111", { screen_type: "led", media: { kind: "none", src: "", looped: true } }) },
  { label: "Projection screen 16:9", make: (s) => base("screen", "Projection", [0, 2.4, -s.size[1] / 2 + 0.3], [5, 2.8, 0.05], "#e8e8e8", { screen_type: "projection", media: { kind: "none", src: "", looped: true } }) },
];
