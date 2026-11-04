// Editor state for the virtual console: local draft of Show.vconsole with undo/redo and debounced save.
import { signal, computed, effect } from "@preact/signals";
import { send, show } from "../store";
import { t } from "../i18n";
import type { VcPage, VcWidget } from "../types";
import { clone, findFree, newPage, newWidget, nextPageId, nextWidgetId } from "./model";
import type { VcKind } from "../types";

export const editMode = signal(false);
export const draft = signal<VcPage[]>([]);
export const pageId = signal<number>(0);
export const sel = signal<number[]>([]);
export const clipboard = signal<VcWidget[]>([]);
export const saving = signal(false);

const undoStack: string[] = [];
const redoStack: string[] = [];
let dirty = false;
let lastSent = "";
let timer = 0;

/** Adopt the server copy unless local edits are still in flight. */
effect(() => {
  const server = show.value?.vconsole;
  if (!server) return;
  const j = JSON.stringify(server);
  if (dirty) { if (j === lastSent) { dirty = false; saving.value = false; } return; }
  if (JSON.stringify(draft.peek()) !== j) draft.value = clone(server);
  if (!server.some((p) => p.id === pageId.peek())) pageId.value = server[0]?.id ?? 0;
});

export const page = computed(() => draft.value.find((p) => p.id === pageId.value) ?? draft.value[0] ?? null);
export const selected = computed(() => {
  const p = page.value;
  return p ? p.widgets.filter((w) => sel.value.includes(w.id)) : [];
});

function schedule() {
  dirty = true; saving.value = true;
  clearTimeout(timer);
  timer = window.setTimeout(async () => {
    lastSent = JSON.stringify(draft.value);
    try { await send({ type: "vc_save", pages: draft.value }); }
    catch { dirty = false; saving.value = false; }
  }, 350);
}

/** Apply `f` to a clone of the pages; records undo. */
export function edit(f: (pages: VcPage[]) => void, opts: { coalesce?: boolean } = {}) {
  const before = JSON.stringify(draft.value);
  const next = clone(draft.value);
  f(next);
  if (JSON.stringify(next) === before) return;
  if (!opts.coalesce || undoStack.length === 0) { undoStack.push(before); if (undoStack.length > 100) undoStack.shift(); }
  redoStack.length = 0;
  draft.value = next;
  schedule();
}
export const canUndo = () => undoStack.length > 0;
export const canRedo = () => redoStack.length > 0;
export function undo() {
  const prev = undoStack.pop(); if (prev === undefined) return;
  redoStack.push(JSON.stringify(draft.value)); draft.value = JSON.parse(prev); schedule();
}
export function redo() {
  const nx = redoStack.pop(); if (nx === undefined) return;
  undoStack.push(JSON.stringify(draft.value)); draft.value = JSON.parse(nx); schedule();
}

export function ensureFirstPage() {
  if (draft.value.length === 0) {
    edit((ps) => { ps.push(newPage(1, t("Page {n}", { n: 1 }))); });
    pageId.value = 1;
  }
}

// ── page ops ────────────────────────────────────────────────────────────────
export function addPage(name: string) {
  const id = nextPageId(draft.value);
  edit((ps) => { ps.push(newPage(id, name)); });
  pageId.value = id; sel.value = [];
}
export function renamePage(id: number, name: string) { edit((ps) => { const p = ps.find((x) => x.id === id); if (p) p.name = name; }); }
export function deletePage(id: number) {
  edit((ps) => { const i = ps.findIndex((x) => x.id === id); if (i >= 0) ps.splice(i, 1); });
  pageId.value = draft.value[0]?.id ?? 0; sel.value = [];
}
export function resizePage(id: number, cols: number, rows: number) { edit((ps) => { const p = ps.find((x) => x.id === id); if (p) { p.cols = cols; p.rows = rows; } }, { coalesce: true }); }

// ── widget ops ──────────────────────────────────────────────────────────────
export function addWidget(kind: VcKind, at?: { x: number; y: number }): number | null {
  const p = page.value; if (!p) return null;
  const id = nextWidgetId(draft.value);
  const base = newWidget(kind, id);
  const pos = at ?? findFree(p, base.w, base.h);
  edit((ps) => { const pg = ps.find((x) => x.id === p.id)!; pg.widgets.push({ ...base, x: pos.x, y: pos.y }); });
  sel.value = [id];
  return id;
}
export function patchWidget(id: number, f: (w: VcWidget) => void, coalesce = true) {
  edit((ps) => { for (const p of ps) { const w = p.widgets.find((x) => x.id === id); if (w) f(w); } }, { coalesce });
}
export function patchSelected(f: (w: VcWidget) => void, coalesce = true) {
  const ids = sel.value;
  edit((ps) => { for (const p of ps) for (const w of p.widgets) if (ids.includes(w.id)) f(w); }, { coalesce });
}
export function moveWidgets(ids: number[], dx: number, dy: number) {
  const p = page.value; if (!p) return;
  edit((ps) => {
    const pg = ps.find((x) => x.id === p.id)!;
    for (const w of pg.widgets) if (ids.includes(w.id)) { w.x = Math.max(0, Math.min(pg.cols - w.w, w.x + dx)); w.y = Math.max(0, Math.min(pg.rows - w.h, w.y + dy)); }
  }, { coalesce: true });
}
export function deleteSelected() {
  const ids = sel.value; if (!ids.length) return;
  edit((ps) => { for (const p of ps) p.widgets = p.widgets.filter((w) => !ids.includes(w.id)); });
  sel.value = [];
}
export function copySelected() { clipboard.value = clone(selected.value); }
export function pasteClipboard(offset = 1) {
  const p = page.value; if (!p || !clipboard.value.length) return;
  let id = nextWidgetId(draft.value);
  const added: number[] = [];
  edit((ps) => {
    const pg = ps.find((x) => x.id === p.id)!;
    for (const w of clipboard.value) {
      const c = clone(w); c.id = id++; c.x = Math.min(pg.cols - c.w, c.x + offset); c.y = Math.min(pg.rows - c.h, c.y + offset);
      c.inputs = []; // a physical control maps to one widget; copies start unmapped
      pg.widgets.push(c); added.push(c.id);
    }
  });
  sel.value = added;
  clipboard.value = clipboard.value.map((w) => ({ ...w, x: w.x + offset, y: w.y + offset }));
}
export const duplicateSelected = () => { copySelected(); pasteClipboard(); };

export type Align = "left" | "right" | "top" | "bottom" | "hcenter" | "vcenter" | "hdist" | "vdist";
export function alignSelected(a: Align) {
  const ws = selected.value; if (ws.length < 2) return;
  const minX = Math.min(...ws.map((w) => w.x)), maxX = Math.max(...ws.map((w) => w.x + w.w));
  const minY = Math.min(...ws.map((w) => w.y)), maxY = Math.max(...ws.map((w) => w.y + w.h));
  const ids = ws.map((w) => w.id);
  edit((ps) => {
    for (const p of ps) {
      const sw = p.widgets.filter((w) => ids.includes(w.id));
      for (const w of sw) {
        if (a === "left") w.x = minX; else if (a === "right") w.x = maxX - w.w;
        else if (a === "top") w.y = minY; else if (a === "bottom") w.y = maxY - w.h;
        else if (a === "hcenter") w.x = Math.round((minX + maxX - w.w) / 2); else if (a === "vcenter") w.y = Math.round((minY + maxY - w.h) / 2);
      }
      if ((a === "hdist" || a === "vdist") && sw.length > 2) {
        const h = a === "hdist"; sw.sort((p1, p2) => (h ? p1.x - p2.x : p1.y - p2.y));
        const total = h ? maxX - minX : maxY - minY, used = sw.reduce((s, w) => s + (h ? w.w : w.h), 0);
        const gap = Math.max(0, Math.round((total - used) / (sw.length - 1)));
        let pos = h ? minX : minY;
        for (const w of sw) { if (h) w.x = pos; else w.y = pos; pos += (h ? w.w : w.h) + gap; }
      }
    }
  });
}

export function replacePages(pages: VcPage[]) { edit((ps) => { ps.length = 0; ps.push(...pages); }); pageId.value = pages[0]?.id ?? 0; sel.value = []; }
