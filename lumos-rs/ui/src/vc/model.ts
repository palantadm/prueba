// Virtual console helpers: defaults, descriptions, validation. Pure functions, no UI.
import type { FeedbackTarget, MidiKind, Show, Source, VcAction, VcInput, VcKind, VcMode, VcOp, VcPage, VcWidget } from "../types";
import { ATTR_META } from "../lib/attrs";
import { t } from "../i18n";
import { es } from "../i18n/es";

/** Localised attribute name (falls back to the raw id). */
export const attrLabel = (k: string): string => t(ATTR_META[k]?.label ?? k);
/** Localised widget-kind name. */
export const kindLabel = (k: VcKind): string => t(KINDS.find((x) => x.kind === k)?.label ?? k);

export const KINDS: { kind: VcKind; label: string; icon: string; w: number; h: number }[] = [
  { kind: "button", label: "Button", icon: "▣", w: 2, h: 2 },
  { kind: "slider", label: "Slider", icon: "⇕", w: 2, h: 6 },
  { kind: "knob", label: "Knob", icon: "◉", w: 2, h: 2 },
  { kind: "xy_pad", label: "XY pad", icon: "✛", w: 4, h: 4 },
  { kind: "cue_list", label: "Cue list", icon: "▶", w: 4, h: 3 },
  { kind: "speed_dial", label: "Speed dial", icon: "⏱", w: 3, h: 3 },
  { kind: "label", label: "Label", icon: "T", w: 3, h: 1 },
  { kind: "frame", label: "Frame", icon: "▢", w: 6, h: 6 },
  { kind: "clock", label: "Clock", icon: "◷", w: 3, h: 1 },
];
export const kindInfo = (k: VcKind) => KINDS.find((x) => x.kind === k)!;
export const isContinuous = (k: VcKind) => k === "slider" || k === "knob" || k === "speed_dial";
export const hasAction = (k: VcKind) => !(k === "label" || k === "frame" || k === "clock");

export const PALETTE_COLORS = ["#3cc4e8", "#35c46a", "#f5a524", "#e5484d", "#e266b8", "#b38cff", "#5b8def", "#f2d98d", "#d9dde3"];

export const OPS: { value: VcOp; label: string }[] = [
  { value: "go", label: "GO" }, { value: "back", label: "BACK" }, { value: "flash", label: "Flash" },
  { value: "toggle", label: "Toggle" }, { value: "release", label: "Release" }, { value: "level", label: "Level (fader)" },
];

export function newWidget(kind: VcKind, id: number, x = 0, y = 0): VcWidget {
  const k = kindInfo(kind);
  const cont = isContinuous(kind);
  return {
    id, kind, label: kind === "label" ? t("Label") : kind === "frame" ? t("Group") : kind === "button" ? t("Button") : cont ? (kind === "speed_dial" ? t("Speed") : t("Level")) : t(k.label),
    x, y, w: k.w, h: k.h, color: kind === "frame" || kind === "label" ? "#8a93a0" : "#3cc4e8",
    action: kind === "xy_pad" ? { type: "xy", group: 0, x_attr: "pan", y_attr: "tilt" } : { type: "none" },
    mode: kind === "button" ? "trigger" : "momentary",
    orientation: kind === "slider" && k.h < k.w ? "horizontal" : "vertical",
    inputs: [], feedback: [], out_min: 0, out_max: kind === "speed_dial" ? 4 : 1,
  };
}

export const newPage = (id: number, name: string, cols = 16, rows = 10): VcPage => ({ id, name, cols, rows, widgets: [] });

export function nextWidgetId(pages: VcPage[]): number {
  let m = 0;
  for (const p of pages) for (const w of p.widgets) m = Math.max(m, w.id);
  return m + 1;
}
export const nextPageId = (pages: VcPage[]) => pages.reduce((m, p) => Math.max(m, p.id), 0) + 1;

/** First free spot (row-major) for a w×h widget; falls back below the lowest widget. */
export function findFree(page: VcPage, w: number, h: number): { x: number; y: number } {
  const occ = page.widgets.filter((o) => o.kind !== "frame");
  const hit = (x: number, y: number) => occ.some((o) => x < o.x + o.w && x + w > o.x && y < o.y + o.h && y + h > o.y);
  for (let y = 0; y + h <= page.rows; y++) for (let x = 0; x + w <= page.cols; x++) if (!hit(x, y)) return { x, y };
  return { x: 0, y: occ.reduce((m, o) => Math.max(m, o.y + o.h), 0) };
}

// ── descriptions ────────────────────────────────────────────────────────────
const MIDI_KIND: Record<MidiKind, string> = { note: "Note", cc: "CC", program: "Program" };
export function describeSource(s: Source): string {
  if (s.type === "osc") return `OSC ${s.address}`;
  return t("MIDI {kind} #{n} · ch {ch}", { kind: t(MIDI_KIND[s.kind]), n: s.number, ch: s.channel ?? t("any") }) + (s.port ? ` · ${s.port}` : "");
}
export function describeFeedback(f: FeedbackTarget): string {
  if (f.type === "osc") return `OSC ${f.host}:${f.port} ${f.address}`;
  return t("MIDI {kind} #{n} · ch {ch}", { kind: t(MIDI_KIND[f.kind]), n: f.number, ch: f.channel })
    + (f.scale ? ` · ${t("scaled")}` : ` · ${t("on {on} / off {off}", { on: f.on, off: f.off })}`) + (f.port ? ` · ${f.port}` : "");
}

const opLabel = (o: VcOp): string => t(OPS.find((x) => x.value === o)?.label ?? o);
export function describeAction(a: VcAction, show: Show | null): string {
  const nm = (list: { id: number; name: string }[] | undefined, id: number) => list?.find((x) => x.id === id)?.name ?? `#${id}`;
  switch (a.type) {
    case "none": return "—";
    case "exec": {
      const e = show?.executors.find((x) => x.id === a.exec);
      return t("Executor {name} · {op}", { name: e ? e.label || `${e.page + 1}.${e.slot + 1}` : `#${a.exec}`, op: opLabel(a.op) });
    }
    case "cuelist": return t("Cuelist {name} · {op}", { name: nm(show?.cuelists, a.cuelist), op: opLabel(a.op) });
    case "grand_master": return t("Grand master");
    case "blackout": return t("Blackout");
    case "dmx": return `DMX ${a.universe}/${a.channel}`;
    case "fixture_attr": return a.fixtures.length === 1
      ? t("{attr} of {n} fixture", { attr: attrLabel(a.attr), n: 1 })
      : t("{attr} of {n} fixtures", { attr: attrLabel(a.attr), n: a.fixtures.length });
    case "group_attr": return t("{attr} of {name}", { attr: attrLabel(a.attr), name: nm(show?.groups, a.group) });
    case "effect_toggle": return t("Effect {name}", { name: nm(show?.effects, a.effect) });
    case "effect_rate": return t("Rate of {name}", { name: nm(show?.effects, a.effect) });
    case "palette_apply": return t("Palette {name}", { name: nm(show?.palettes, a.palette) });
    case "scene": return t("Scene {name} · {op}", { name: nm(show?.scenes, a.scene), op: opLabel(a.op) });
    case "scene_morph": return t("Morph {a} → {b}", { a: nm(show?.scenes, a.a), b: nm(show?.scenes, a.b) });
    case "prog_clear": return t("Clear programmer");
    case "page": return t("Page {name}", { name: show?.vconsole.find((p) => p.id === a.page)?.name ?? `#${a.page}` });
    case "xy": return `${nm(show?.groups, a.group)} ${a.x_attr}/${a.y_attr}`;
  }
}

/** Action types selectable for a widget kind. */
export const ACTION_TYPES: { type: VcAction["type"]; label: string }[] = [
  { type: "none", label: "None" },
  { type: "exec", label: "Executor" },
  { type: "cuelist", label: "Cuelist" },
  { type: "grand_master", label: "Grand master" },
  { type: "blackout", label: "Blackout" },
  { type: "group_attr", label: "Group attribute" },
  { type: "fixture_attr", label: "Fixture attribute" },
  { type: "dmx", label: "DMX channel" },
  { type: "effect_toggle", label: "Effect on/off" },
  { type: "effect_rate", label: "Effect rate" },
  { type: "palette_apply", label: "Apply palette" },
  { type: "scene", label: "Scene" },
  { type: "scene_morph", label: "Scene morph (A→B)" },
  { type: "prog_clear", label: "Clear programmer" },
  { type: "page", label: "Go to page" },
  { type: "xy", label: "XY (group)" },
];
export function actionTypesFor(k: VcKind): VcAction["type"][] {
  switch (k) {
    case "button": return ["none", "exec", "cuelist", "scene", "blackout", "grand_master", "effect_toggle", "palette_apply", "prog_clear", "page", "dmx"];
    case "slider": case "knob": return ["none", "exec", "cuelist", "scene", "scene_morph", "grand_master", "group_attr", "fixture_attr", "dmx", "effect_rate"];
    case "xy_pad": return ["xy", "none"];
    case "cue_list": return ["cuelist", "none"];
    case "speed_dial": return ["effect_rate", "none"];
    default: return ["none"];
  }
}
export function defaultAction(type: VcAction["type"], show: Show | null, kind: VcKind): VcAction {
  const op: VcOp = isContinuous(kind) ? "level" : "go";
  switch (type) {
    case "exec": return { type, exec: show?.executors[0]?.id ?? 0, op };
    case "cuelist": return { type, cuelist: show?.cuelists[0]?.id ?? 0, op: kind === "cue_list" ? "go" : op };
    case "dmx": return { type, universe: 1, channel: 1 };
    case "fixture_attr": return { type, fixtures: [], attr: "dimmer" };
    case "group_attr": return { type, group: show?.groups[0]?.id ?? 0, attr: "dimmer" };
    case "effect_toggle": return { type, effect: show?.effects[0]?.id ?? 0 };
    case "effect_rate": return { type, effect: show?.effects[0]?.id ?? 0 };
    case "palette_apply": return { type, palette: show?.palettes[0]?.id ?? 0, group: null };
    case "scene": return { type, scene: show?.scenes?.[0]?.id ?? 0, op: isContinuous(kind) ? "level" : "toggle" };
    case "scene_morph": return { type, a: show?.scenes?.[0]?.id ?? 0, b: show?.scenes?.[1]?.id ?? show?.scenes?.[0]?.id ?? 0 };
    case "page": return { type, page: show?.vconsole[0]?.id ?? 0 };
    case "xy": return { type, group: show?.groups[0]?.id ?? 0, x_attr: "pan", y_attr: "tilt" };
    default: return { type } as VcAction;
  }
}

export const MODE_LABEL: Record<VcMode, string> = { momentary: "Momentary (while held)", toggle: "Toggle", trigger: "Trigger (once)" };

export function defaultInput(source: Source): VcInput { return { source, invert: false, in_min: 0, in_max: 1 }; }

export function clone<T>(x: T): T { return structuredClone(x); }

const DEFAULT_LABEL_KEYS = ["Button", "Level", "Speed", "Slider", "Knob", "XY pad", "Cue list", "Speed dial", "Clock"];
const DEFAULT_LABELS = new Set(["", ...DEFAULT_LABEL_KEYS, ...DEFAULT_LABEL_KEYS.map((k) => es[k]).filter(Boolean)]);
/** Short human label for an action, used to name widgets that still carry a default label. */
export function autoLabel(a: VcAction, show: Show | null): string | null {
  const nm = (list: { id: number; name: string }[] | undefined, id: number) => list?.find((x) => x.id === id)?.name;
  const op = (o: VcOp) => ({ go: t("GO"), back: t("BACK"), flash: t("FLASH"), toggle: "", release: t("RELEASE"), level: "" }[o]);
  switch (a.type) {
    case "exec": { const e = show?.executors.find((x) => x.id === a.exec); const n = e ? e.label || t("Exec {n}", { n: e.slot + 1 }) : null; return n ? `${op(a.op)} ${n}`.trim() : null; }
    case "cuelist": { const n = nm(show?.cuelists, a.cuelist); return n ? `${op(a.op)} ${n}`.trim() : null; }
    case "grand_master": return t("Grand master");
    case "blackout": return t("BLACKOUT");
    case "group_attr": { const g = nm(show?.groups, a.group); return g ? `${g} ${attrLabel(a.attr)}` : null; }
    case "fixture_attr": return attrLabel(a.attr);
    case "effect_toggle": case "effect_rate": return nm(show?.effects, a.effect) ?? null;
    case "palette_apply": return nm(show?.palettes, a.palette) ?? null;
    case "scene": { const n = nm(show?.scenes, a.scene); return n ? `${op(a.op)} ${n}`.trim() : null; }
    case "scene_morph": { const x = nm(show?.scenes, a.a), y = nm(show?.scenes, a.b); return x && y ? `${x} → ${y}` : null; }
    case "prog_clear": return t("CLEAR");
    case "page": return show?.vconsole.find((p) => p.id === a.page)?.name ?? null;
    case "xy": return nm(show?.groups, a.group) ?? null;
    case "dmx": return `DMX ${a.universe}/${a.channel}`;
    default: return null;
  }
}
export const isDefaultLabel = (l: string) => DEFAULT_LABELS.has(l);
