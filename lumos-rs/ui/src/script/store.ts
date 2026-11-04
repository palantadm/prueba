import { signal } from "@preact/signals";
import { send, show } from "../store";
import { lsGet, lsSet } from "../lib/state";
import { shareSignal } from "../lib/windows";
import type { CueMark, Script } from "../types";
import { SCRIPT_TEST_ID, normalizeScript } from "./util";
import type { Suggestion } from "./api";

export const activeId = signal<number | null>(lsGet("script.active", null));
shareSignal("script", activeId);
export const setActive = (id: number | null) => { activeId.value = id; lsSet("script.active", id); };

export const editMode = signal(false);
export const fontSize = signal<number>(lsGet("script.font", 30));
export const setFontSize = (n: number) => { fontSize.value = Math.max(20, Math.min(48, Math.round(n))); lsSet("script.font", fontSize.value); };
export const pdfOpen = signal<boolean>(lsGet("script.pdf", false));
export const setPdfOpen = (b: boolean) => { pdfOpen.value = b; lsSet("script.pdf", b); };

export type Sel = { block: number; word: number } | null;
/** word selected in edit mode (block id + word index) */
export const sel = signal<Sel>(null);
/** mark being moved: next word tap relocates it */
export const moving = signal<{ block: number; mark: number } | null>(null);
export const suggestions = signal<Suggestion[]>([]);

// ── optimistic draft ───────────────────────────────────────────────────────
const draft = signal<Script | null>(null);
let ver = 0;
let timer = 0;

export function currentScript(): Script | undefined {
  const id = activeId.value;
  if (draft.value && draft.value.id === id) return draft.value;
  return show.value?.scripts.find((s) => s.id === id);
}
export function visibleScripts(): Script[] {
  return (show.value?.scripts ?? []).filter((s) => s.id < SCRIPT_TEST_ID);
}

async function flush(my: number) {
  const s = draft.peek();
  if (!s) return;
  try { await send({ type: "script_save", script: s }); } catch { /* toasted */ }
  setTimeout(() => { if (ver === my) draft.value = null; }, 700);
}
/** Replace the active script locally right away and persist (debounced). */
export function saveScript(s: Script, immediate = false) {
  const n = normalizeScript(s);
  draft.value = n;
  const my = ++ver;
  clearTimeout(timer);
  if (immediate) void flush(my);
  else timer = window.setTimeout(() => void flush(my), 220);
}
/** Save and wait for the engine's verdict; throws its validation message (the draft is rolled back). */
export async function saveScriptStrict(s: Script): Promise<void> {
  const n = normalizeScript(s);
  clearTimeout(timer);
  ++ver;
  draft.value = n;
  try { await send({ type: "script_save", script: n }, { quiet: true }); }
  catch (e) { draft.value = null; throw e; }
  const my = ver;
  setTimeout(() => { if (ver === my) draft.value = null; }, 700);
}
export function editScript(f: (s: Script) => Script, immediate = false) {
  const cur = currentScript();
  if (cur) saveScript(f(structuredClone(cur)), immediate);
}
export function updateMark(blockId: number, markId: number, f: (m: CueMark) => CueMark) {
  editScript((s) => ({ ...s, blocks: s.blocks.map((b) => (b.id === blockId ? { ...b, marks: b.marks.map((m) => (m.id === markId ? f(m) : m)) } : b)) }));
}
