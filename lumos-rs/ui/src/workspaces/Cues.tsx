import "../styles/live.css";
import "../styles/cues.css";
import { useEffect, useState } from "preact/hooks";
import { fire, rt, send, show, toast } from "../store";
import { HFader } from "../widgets/SlimFader";
import { Modal } from "../ui";
import type { Cue, Cuelist } from "../types";
import { attrLabel, fixtureById, fmtNum, pct } from "../lib/attrs";
import { activeCuelist } from "../lib/state";
import { useKey } from "../lib/hooks";
import { CueTable, InsertCue } from "./cues/CueTable";
import { CueDetail } from "./cues/CueDetail";
import { Executors } from "./cues/Executors";
import { Macros } from "./cues/Macros";
import { SwitchRow } from "./cues/SwitchRow";
import { ConfirmDialog, PromptDialog } from "./live/dialogs";
import { t, tp } from "../i18n";

export function Cues() {
  const lists = show.value?.cuelists ?? [];
  const cur = lists.find((l) => l.id === activeCuelist.value) ?? lists[0];
  const [selCue, setSelCue] = useState<number | null>(null);
  const [dlg, setDlg] = useState<"new" | "rename" | "delete" | "insert" | "opts" | null>(null);

  useEffect(() => { if (cur && activeCuelist.value !== cur.id) activeCuelist.value = cur.id; }, [cur?.id]);
  useEffect(() => setSelCue(null), [cur?.id]);
  useKey((e) => {
    if (e.key === " " && cur) { e.preventDefault(); send({ type: "go", cuelist: cur.id }); }
  }, [cur?.id]);

  const c_rt = cur ? rt.value?.cuelists.find((c) => c.id === cur.id) : undefined;
  const save = (patch: Partial<Cuelist>) => cur && send({ type: "cuelist_save", cuelist: { ...cur, ...patch } });

  const newList = (name: string) => {
    const id = Math.max(0, ...lists.map((l) => l.id)) + 1;
    send({ type: "cuelist_save", cuelist: { id, name, cues: [], looped: false, tracking: true, priority: 0, auto_mark: false } }).then(() => (activeCuelist.value = id));
  };
  const renumber = () => {
    if (!cur) return;
    const cues = [...cur.cues].sort((a, b) => a.number - b.number).map((c, i) => ({ ...c, number: i + 1 }));
    send({ type: "cuelist_save", cuelist: { ...cur, cues } }).then(() => toast(t("Cues renumbered 1…n"), "ok"));
  };

  return (
    <div class="cu-root">
      <aside class="panel cu-left">
        <div class="cu-sec">
          <span class="label">{t("Cuelists")}</span><span class="sp" />
          <button class="btn sm" onClick={() => setDlg("new")} aria-label={t("New cuelist")}>{t("+ New")}</button>
        </div>
        <div class="cu-lists" role="listbox" aria-label={t("Cuelists")}>
          {lists.length === 0 && <div class="dim" style={{ padding: 4 }}>{t("No cuelists yet. Create one, then record cues from the programmer.")}</div>}
          {lists.map((l) => {
            const r = rt.value?.cuelists.find((c) => c.id === l.id);
            const st = r?.state ?? "idle";
            return (
              <button key={l.id} role="option" aria-selected={l.id === cur?.id} class={`cu-list ${l.id === cur?.id ? "sel" : ""} ${st === "running" ? "run" : ""} ${st === "paused" ? "pause" : ""}`}
                onClick={() => (activeCuelist.value = l.id)}>
                <span class="row"><span class={`dot ${st === "running" ? "ok" : st === "paused" ? "warn" : ""}`} /><span class="nm">{l.name}</span></span>
                <span class="sub mono">{l.cues.length} · {t(st)}{r?.current != null ? ` · ${fmtNum(r.current)}` : ""}</span>
              </button>
            );
          })}
        </div>
        <Macros />
      </aside>

      <section class="cu-center">
        {!cur ? (
          <div class="panel panel-body dim">{t("Create a cuelist to begin.")}</div>
        ) : (
          <>
            <Transport list={cur} />
            <div class="cu-bar">
              <b class="cu-listname">{cur.name}</b>
              {cur.looped && <span class="chip">{t("Loop")}</span>}
              {cur.auto_mark && <span class="chip">{t("Auto-mark")}</span>}
              {cur.tracking && <span class="chip">{t("Tracking")}</span>}
              <span class="sp" />
              <button class="btn" onClick={() => setDlg("opts")}>{t("Options")}</button>
              <button class="btn" onClick={() => setDlg("insert")}>{t("+ Insert cue")}</button>
              <button class="btn" onClick={renumber} disabled={!cur.cues.length}>{t("Renumber")}</button>
            </div>
            <CueTable list={cur} rt={c_rt} selected={selCue} onSelect={setSelCue} />
          </>
        )}
      </section>

      <aside class="panel cu-right">
        {cur ? <CueDetail list={cur} number={selCue} onDeleted={() => setSelCue(null)} /> : <div class="dim" style={{ padding: 16 }}>{t("Create a cuelist to begin.")}</div>}
      </aside>

      <Executors />

      {dlg === "new" && <PromptDialog title={t("New cuelist")} label={t("Name")} initial={t("Cuelist {n}", { n: Math.max(0, ...lists.map((l) => l.id)) + 1 })} ok={t("Create")} onClose={() => setDlg(null)} onSubmit={newList} />}
      {dlg === "rename" && cur && <PromptDialog title={t("Rename cuelist")} label={t("Name")} initial={cur.name} onClose={() => setDlg(null)} onSubmit={(name) => save({ name })} />}
      {dlg === "delete" && cur && <ConfirmDialog title={t("Delete cuelist")} text={t("Delete “{name}” and its {n} cue(s)?", { name: cur.name, n: cur.cues.length })} ok={t("Delete")} danger onClose={() => setDlg(null)}
        onYes={() => { send({ type: "cuelist_remove", id: cur.id }); activeCuelist.value = null; }} />}
      {dlg === "insert" && cur && <InsertCue list={cur} after={selCue} onClose={() => setDlg(null)} onDone={setSelCue} />}
      {dlg === "opts" && cur && <ListOptions list={cur} save={save} onRename={() => setDlg("rename")} onDelete={() => setDlg("delete")} onClose={() => setDlg(null)} />}
    </div>
  );
}

/** Cuelist-level settings in one touch dialog (switches + priority stepper). */
function ListOptions(p: { list: Cuelist; save: (patch: Partial<Cuelist>) => void; onRename: () => void; onDelete: () => void; onClose: () => void }) {
  const l = p.list;
  const pr = (d: number) => p.save({ priority: Math.max(0, Math.min(255, l.priority + d)) });
  return (
    <Modal title={l.name} onClose={p.onClose} footer={<>
      <button class="btn lg" onClick={p.onRename}>{t("Rename")}</button>
      <button class="btn lg danger" onClick={p.onDelete}>{t("Delete")}</button>
      <span class="sp" />
      <button class="btn lg primary" onClick={p.onClose}>{t("Done")}</button>
    </>}>
      <SwitchRow on={l.looped} onChange={(v) => p.save({ looped: v })} label={t("Loop")} />
      <SwitchRow on={l.auto_mark} onChange={(v) => p.save({ auto_mark: v })} label={t("Auto-mark")} title={t("Pre-position moving heads that are dark in this cue and lit in the next one")} />
      <SwitchRow on={l.tracking} onChange={(v) => p.save({ tracking: v })} label={t("Tracking")} />
      <div class="row" style={{ gap: 12 }}>
        <span style={{ fontWeight: 600 }}>{t("Priority")}</span><span class="sp" />
        <div class="stepper">
          <button aria-label="−" onClick={() => pr(-1)}>−</button>
          <span class="val">{l.priority}</span>
          <button aria-label="+" onClick={() => pr(1)}>+</button>
        </div>
      </div>
    </Modal>
  );
}

function Transport({ list }: { list: Cuelist }) {
  const r = rt.value?.cuelists.find((c) => c.id === list.id);
  const state = r?.state ?? "idle";
  const cue = r?.current != null ? list.cues.find((c) => c.number === r.current) : undefined;
  const prog = r?.progress ?? 0;
  const remain = cue && state === "running" ? Math.max(0, (1 - prog) * cue.fade_in) : 0;
  return (
    <div class="panel cu-play">
      <div class="cu-pbtns">
        <button class="btn go cu-go" onClick={() => send({ type: "go", cuelist: list.id })} aria-label={t("Go")}>GO</button>
        <button class="btn cu-pb" onClick={() => send({ type: "back", cuelist: list.id })} aria-label={t("Back")}>{t("BACK")}</button>
        <button class={`btn cu-pb ${state === "paused" ? "on" : ""}`} onClick={() => send({ type: "pause", cuelist: list.id })} aria-label={t("Pause")}>{t("PAUSE")}</button>
        <button class="btn cu-pb danger" onClick={() => send({ type: "release_cuelist", cuelist: list.id })} aria-label={t("Release cuelist")}>{t("RELEASE")}</button>
      </div>
      <div class="cu-sep" />
      <div class="cu-info">
        <div class="cu-now" title={cue?.label}>
          <div><span class="label">{t("Current")}</span><span class="cu-num cur">{r?.current != null ? fmtNum(r.current) : "—"}</span></div>
          <div><span class="label">{t("Next")}</span><span class="cu-num">{r?.next != null ? fmtNum(r.next) : "—"}</span></div>
          <span class="sp" />
          <span class="mono dim cu-remain" aria-label={t("Fade countdown")}>{state === "running" && prog < 1 ? `${remain.toFixed(1)} s` : ""}</span>
          <span class={`chip cu-state ${state === "running" ? "ok" : state === "paused" ? "warn" : ""}`}>{t(state)}</span>
        </div>
        <ManualFade list={list} />
      </div>
      <div class="cu-prog" role="progressbar" aria-valuenow={Math.round(prog * 100)} aria-valuemin={0} aria-valuemax={100}><i style={{ width: `${prog * 100}%` }} /></div>
    </div>
  );
}

/** Manual crossfader: drive the running cue's fade by hand (theatre-style A/B fade). */
function ManualFade({ list }: { list: Cuelist }) {
  const r = rt.value?.cuelists.find((c) => c.id === list.id);
  const [drag, setDrag] = useState<number | null>(null);
  const manual = !!r?.manual;
  const shown = drag ?? (manual ? r?.progress ?? 0 : 0);
  return (
    <div class="cu-mf" title={t("Manual fade")}>
      <HFader value={shown} color={manual ? "var(--amber)" : "var(--green)"} ariaLabel={t("Manual crossfade")}
        onChange={(v) => { setDrag(v); fire({ type: "cuelist_manual", cuelist: list.id, progress: v }); }} onEnd={() => setDrag(null)} />
      <button class={`btn ${manual ? "on" : ""}`} disabled={!manual} onClick={() => fire({ type: "cuelist_manual", cuelist: list.id, progress: null })}>{t("Auto")}</button>
    </div>
  );
}
