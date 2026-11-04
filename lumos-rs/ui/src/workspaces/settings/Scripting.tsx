import { useEffect, useRef, useState } from "preact/hooks";
import type { LuaScript } from "../../types";
import { fire, rt, send, show, toast } from "../../store";
import { EmptyState, NumberField, TextField, Toggle } from "../../ui";
import { SettingsPage } from "./Common";
import { logOpen } from "../../lib/log";
import { openHelp } from "../../help/state";
import { t } from "../../i18n";

/* Lua scripts of the show: editor, run, periodic / on-start options and a live output panel. */

const SAMPLE = `-- Ejemplo: sube un grupo según la hora
local nivel = lumos.date().hour >= 20 and 30 or 80
lumos.cmd("group 1 at " .. nivel)
print("nivel", nivel)
`;

export function ScriptingPage() {
  const scripts = show.value?.lua ?? [];
  const [sel, setSel] = useState<number | null>(scripts[0]?.id ?? null);
  const cur = scripts.find((s) => s.id === sel) ?? null;
  const [draft, setDraft] = useState<LuaScript | null>(cur);
  const [dirty, setDirty] = useState(false);
  const out = useRef<HTMLDivElement>(null);
  const log = rt.value?.lua?.log ?? [];

  useEffect(() => { if (!dirty) setDraft(cur); }, [cur?.id, JSON.stringify(cur), dirty]);
  useEffect(() => { if (out.current) out.current.scrollTop = out.current.scrollHeight; }, [log.length]);

  const create = () => {
    const s: LuaScript = { id: 0, name: t("New Lua script"), code: SAMPLE, on_start: false, every_s: 0, enabled: false };
    void send({ type: "lua_save", script: s }).then(() => {
      // the engine gives it an id: select the newest
      setTimeout(() => { const l = show.value?.lua ?? []; if (l.length) { setSel(l[l.length - 1].id); setDirty(false); } }, 250);
    }).catch(() => {});
  };
  const save = async () => { if (!draft) return; try { await send({ type: "lua_save", script: draft }); setDirty(false); toast(t("Script saved"), "ok", 1200); } catch { /* toasted */ } };
  const patch = (p: Partial<LuaScript>) => { if (draft) { setDraft({ ...draft, ...p }); setDirty(true); } };
  const run = async () => { if (!draft) return; if (dirty) await save(); fire({ type: "lua_run", id: draft.id }); };
  const mine = log.filter((l) => !draft || l.script === draft.name || l.script === "(prueba)");

  return (
    <SettingsPage title={t("Scripts (Lua)")} sub={t("Automate with Lua: read the console state, send commands, schedule things and react to the clock.")}>
      <div class="panel">
        <div class="panel-head row"><span class="grow">{t("Lua scripts")}</span>
          <button class="btn" onClick={() => openHelp("topic:scripting")}>{t("Reference")}</button>
          <button class="btn primary" onClick={create}>{t("+ New script")}</button></div>
        <div class="panel-body col" style={{ gap: 6 }}>
          {scripts.length === 0 && <EmptyState title={t("No scripts yet")} hint={t("A script can run from a console button, a cue mark, when the show opens, or on a timer.")} />}
          <div class="row wrap" style={{ gap: 6 }}>
            {scripts.map((s) => <button key={s.id} class={`btn ${s.id === sel ? "sel" : ""}`} onClick={() => { if (dirty && !confirm(t("Discard the unsaved changes?"))) return; setDirty(false); setSel(s.id); }}>{s.name}{s.enabled && s.every_s > 0 ? " ⏱" : ""}</button>)}
          </div>
        </div>
      </div>
      {draft && (
        <div class="panel">
          <div class="panel-head row"><span class="grow">{t("Editor")}</span>
            {dirty && <span class="chip warn">{t("Unsaved changes")}</span>}
            <button class="btn" disabled={!dirty} onClick={() => void save()}>{t("Save")}</button>
            <button class="btn primary" onClick={() => void run()}>{t("Run")}</button>
            <button class="btn danger" onClick={() => { void send({ type: "lua_remove", id: draft.id }).then(() => { setSel(null); setDraft(null); setDirty(false); }).catch(() => {}); }}>{t("Delete")}</button>
          </div>
          <div class="panel-body col" style={{ gap: 10 }}>
            <TextField label={t("Name")} value={draft.name} onChange={(name) => patch({ name })} />
            <textarea class="input mono" spellcheck={false} rows={16} style={{ width: "100%", minHeight: 260, resize: "vertical", tabSize: 2 }} aria-label={t("Lua code")} value={draft.code}
              onInput={(e) => patch({ code: (e.target as HTMLTextAreaElement).value })}
              onKeyDown={(e) => {
                const el = e.target as HTMLTextAreaElement;
                if (e.key === "Tab") { e.preventDefault(); const s = el.selectionStart; patch({ code: el.value.slice(0, s) + "  " + el.value.slice(el.selectionEnd) }); requestAnimationFrame(() => { el.selectionStart = el.selectionEnd = s + 2; }); }
                if ((e.ctrlKey || e.metaKey) && e.key === "Enter") { e.preventDefault(); void run(); }
              }} />
            <div class="faint" style={{ fontSize: 11 }}>{t("Ctrl+Enter runs it. Up to 250 ms per run; no files or network except through lumos.*")}</div>
            <div class="row wrap" style={{ gap: 12, alignItems: "flex-end" }}>
              <Toggle checked={draft.on_start} onChange={(on_start) => patch({ on_start })} label={t("Run when the show opens")} />
              <NumberField label={t("Repeat every (s, 0 = never)")} value={draft.every_s} min={0} max={86400} float step={0.5} onChange={(every_s) => patch({ every_s })} />
              <Toggle checked={draft.enabled} onChange={(enabled) => patch({ enabled })} label={t("Active")} />
            </div>
          </div>
        </div>
      )}
      <div class="panel">
        <div class="panel-head row"><span class="grow">{t("Output")} <span class="faint">· {t("timers: {n}", { n: rt.value?.lua?.timers ?? 0 })}</span></span>
          <button class="btn" disabled={!(rt.value?.lua?.timers)} onClick={() => fire({ type: "lua_stop" })}>{t("Stop timers")}</button>
          <button class="btn" onClick={() => { logOpen.value = true; }}>{t("Open the live log")}</button></div>
        <div class="panel-body mono" ref={out} style={{ maxHeight: 220, overflow: "auto", fontSize: 12 }}>
          {mine.length === 0 && <span class="faint">{t("What the script prints and its errors appear here.")}</span>}
          {mine.map((l, i) => <div key={i} style={{ color: l.ok ? undefined : "var(--stop)" }}><span class="faint">{l.t.toFixed(1)}s</span> {l.ok ? "" : "⚠ "}{l.text}</div>)}
        </div>
      </div>
    </SettingsPage>
  );
}
