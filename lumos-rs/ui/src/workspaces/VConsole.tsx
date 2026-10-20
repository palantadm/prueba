import "../styles/vc.css";
import { useEffect, useRef } from "preact/hooks";
import { connected, rt, show, toast } from "../store";
import { initMidi, midiActivity, midiOutLog, midiOutEnabled, midiState } from "../midi";
import { EmptyState, Segmented, Tabs, promptDialog, confirmDialog } from "../ui";
import type { VcPage } from "../types";
import { Canvas } from "../vc/Canvas";
import { MultiProps, PageProps, WidgetProps } from "../vc/Props";
import { KINDS, nextPageId, nextWidgetId } from "../vc/model";
import { autoBuildFromExecutors } from "../vc/presets";
import {
  addPage, addWidget, canRedo, canUndo, clipboard, copySelected, deleteSelected, draft, duplicateSelected, edit, editMode, ensureFirstPage,
  moveWidgets, page, pageId, pasteClipboard, redo, replacePages, saving, sel, selected, undo,
} from "../vc/editor";

function useEditorKeys() {
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (!editMode.value) return;
      const t = e.target as HTMLElement | null;
      if (t?.matches?.("input,textarea,select,[contenteditable]")) return;
      const mod = e.ctrlKey || e.metaKey, k = e.key.toLowerCase();
      if (mod && k === "z") { e.preventDefault(); e.shiftKey ? redo() : undo(); }
      else if (mod && k === "y") { e.preventDefault(); redo(); }
      else if (mod && k === "c") { e.preventDefault(); copySelected(); }
      else if (mod && k === "v") { e.preventDefault(); pasteClipboard(); }
      else if (mod && k === "d") { e.preventDefault(); duplicateSelected(); }
      else if (mod && k === "a") { e.preventDefault(); sel.value = page.value?.widgets.filter((w) => w.kind !== "frame").map((w) => w.id) ?? []; }
      else if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); deleteSelected(); }
      else if (e.key === "Escape") sel.value = [];
      else if (e.key.startsWith("Arrow") && sel.value.length) {
        e.preventDefault();
        moveWidgets(sel.value, e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : 0, e.key === "ArrowUp" ? -1 : e.key === "ArrowDown" ? 1 : 0);
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);
}

function exportLayout() {
  const blob = new Blob([JSON.stringify({ lumos_vconsole: 1, pages: draft.value }, null, 2)], { type: "application/json" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob); a.download = `${show.value?.name || "show"}-console.json`; a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}
async function importLayout(file: File) {
  try {
    const j = JSON.parse(await file.text());
    const pages: VcPage[] = Array.isArray(j) ? j : j.pages;
    if (!Array.isArray(pages) || !pages.every((p) => p && Array.isArray(p.widgets) && typeof p.cols === "number")) throw new Error("not a console layout");
    if (draft.value.some((p) => p.widgets.length) && !(await confirmDialog("Import layout", "Replace the current console layout?", { danger: true, ok: "Replace" }))) return;
    replacePages(pages); toast(`Imported ${pages.length} page(s)`, "ok");
  } catch (e) { toast(`Import failed: ${(e as Error).message}`, "err"); }
}
function autoBuild() {
  const s = show.value; if (!s) return;
  const { page: pg } = autoBuildFromExecutors(s, nextWidgetId(draft.value), nextPageId(draft.value));
  edit((ps) => { ps.push(pg); });
  pageId.value = pg.id; sel.value = [];
}

function Activity() {
  const a = rt.value?.inputs.recent ?? [];
  const row = (cls: string, items: string[], empty: string) => (
    <div class={`vc-act-col ${cls}`}>{items.length ? items.slice(0, 3).map((t, i) => <div key={i} class="mono vc-act-line">{t}</div>) : <div class="faint vc-act-line">{empty}</div>}</div>
  );
  return (
    <div class="vc-activity" aria-label="Input and feedback activity">
      <span class="vc-cap">IN</span>{row("in", midiActivity.value.map((x) => x.text), "no MIDI input")}
      <span class="vc-cap">ENGINE</span>{row("eng", a, "idle")}
      <span class="vc-cap">OUT</span>{row("out", midiOutLog.value.map((x) => x.text), "no feedback sent")}
    </div>
  );
}

export function VConsole() {
  useEditorKeys();
  const imp = useRef<HTMLInputElement>(null);
  const pg = page.value;
  const pages = draft.value;
  const editing = editMode.value;
  const s = selected.value;

  useEffect(() => { if (editing) ensureFirstPage(); }, [editing]);
  useEffect(() => { if (!pageId.value && pages[0]) pageId.value = pages[0].id; }, [pages.length]);

  const midiHint = midiState.value === "idle" ? <button class="btn sm" onClick={() => initMidi()}>Enable MIDI</button>
    : midiState.value === "ready" && midiOutEnabled.value.length === 0 ? <span class="chip warn" title="Settings → MIDI → Output ports">No MIDI output enabled for feedback</span> : null;

  return (
    <div class="vc">
      <div class="vc-bar">
        <Segmented value={editing ? "edit" : "play"} options={[{ value: "play", label: "▶ Play" }, { value: "edit", label: "✎ Edit" }]} onChange={(v) => { editMode.value = v === "edit"; sel.value = []; }} />
        <div class="vc-tabs">
          <Tabs value={String(pg?.id ?? 0)} options={pages.map((p) => ({ value: String(p.id), label: p.name }))} onChange={(v) => { pageId.value = Number(v); sel.value = []; }} />
        </div>
        {editing && <button class="btn sm" onClick={async () => { const n = await promptDialog("New page", "Name", `Page ${pages.length + 1}`); if (n) addPage(n); }}>＋ Page</button>}
        <span class="sp" />
        {midiHint}
        {editing && <>
          <button class="btn sm" disabled={!canUndo()} onClick={undo} title="Undo (Ctrl+Z)">↶</button>
          <button class="btn sm" disabled={!canRedo()} onClick={redo} title="Redo (Ctrl+Shift+Z)">↷</button>
          <button class="btn sm" disabled={!s.length} onClick={copySelected} title="Copy (Ctrl+C)">Copy</button>
          <button class="btn sm" disabled={!clipboard.value.length} onClick={() => pasteClipboard()} title="Paste (Ctrl+V)">Paste</button>
          <button class="btn sm" onClick={autoBuild} title="New page mirroring executor page 1">Auto-build</button>
          <button class="btn sm" onClick={exportLayout}>Export</button>
          <button class="btn sm" onClick={() => imp.current?.click()}>Import</button>
          <input ref={imp} type="file" accept="application/json,.json" hidden onChange={(e) => { const f = (e.target as HTMLInputElement).files?.[0]; if (f) void importLayout(f); (e.target as HTMLInputElement).value = ""; }} />
          <span class={`chip ${saving.value ? "warn" : "ok"}`}>{!connected.value ? "offline" : saving.value ? "saving…" : "saved"}</span>
        </>}
      </div>

      <div class="vc-body">
        {editing && (
          <aside class="vc-palette" aria-label="Widget palette">
            <div class="vc-cap">ADD WIDGET</div>
            {KINDS.map((k) => (
              <button key={k.kind} class="vc-pal" onClick={() => { ensureFirstPage(); addWidget(k.kind); }}>
                <span class="vc-pal-i" aria-hidden="true">{k.icon}</span><span>{k.label}</span>
              </button>
            ))}
            <div class="faint vc-hint" style={{ marginTop: 8 }}>Drag to move, corner handle to resize. Shift-click or drag a box to multi-select. Delete, Ctrl+C/V/D/Z work.</div>
          </aside>
        )}
        <div class="vc-main">
          {pg ? <Canvas /> : (
            <EmptyState icon="🎛" title="Virtual console" hint="Build a control surface with buttons, faders, knobs and XY pads, map your MIDI/OSC controller to them and get LED / motor-fader feedback.">
              <div class="row"><button class="btn primary" onClick={() => { editMode.value = true; ensureFirstPage(); }}>Create console</button><button class="btn" onClick={autoBuild} disabled={!show.value}>Auto-build from executors</button></div>
            </EmptyState>
          )}
        </div>
        {editing && (
          <aside class="vc-props" aria-label="Properties">
            {s.length === 1 ? <WidgetProps w={s[0]} /> : s.length > 1 ? <MultiProps /> : <PageProps />}
          </aside>
        )}
      </div>
      {(editing || midiActivity.value.length > 0) && <Activity />}
    </div>
  );
}
