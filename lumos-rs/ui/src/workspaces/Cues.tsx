import "../styles/live.css";
import { useEffect, useState } from "preact/hooks";
import { fire, rt, send, show, toast } from "../store";
import { Fader } from "../widgets";
import type { Cue, Cuelist } from "../types";
import { attrLabel, fixtureById, fmtNum, pct } from "../lib/attrs";
import { activeCuelist } from "../lib/state";
import { useKey } from "../lib/hooks";
import { CueTable, InsertCue } from "./cues/CueTable";
import { CueDetail } from "./cues/CueDetail";
import { Executors } from "./cues/Executors";
import { Macros } from "./cues/Macros";
import { ConfirmDialog, PromptDialog } from "./live/dialogs";
import { t, tp } from "../i18n";

export function Cues() {
  const lists = show.value?.cuelists ?? [];
  const cur = lists.find((l) => l.id === activeCuelist.value) ?? lists[0];
  const [selCue, setSelCue] = useState<number | null>(null);
  const [dlg, setDlg] = useState<"new" | "rename" | "delete" | "insert" | null>(null);

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
    <div class="lv-cues has-macros">
      <div class="panel lv-col">
        <div class="panel-head"><span>{t("Cuelists")}</span><span class="sp" /><button class="btn sm primary" onClick={() => setDlg("new")} aria-label={t("New cuelist")}>{t("+ New")}</button></div>
        <div class="lv-scroll" role="listbox" aria-label={t("Cuelists")}>
          {lists.length === 0 && <div class="dim" style={{ padding: 12 }}>{t("No cuelists yet. Create one, then record cues from the programmer.")}</div>}
          {lists.map((l) => {
            const r = rt.value?.cuelists.find((c) => c.id === l.id);
            return (
              <div key={l.id} role="option" aria-selected={l.id === cur?.id} class={`lv-cl ${l.id === cur?.id ? "sel" : ""}`} onClick={() => (activeCuelist.value = l.id)}>
                <span class={`dot ${r?.state === "running" ? "ok" : r?.state === "paused" ? "warn" : ""}`} />
                <span class="grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{l.name}</span>
                <span class="faint mono">{l.cues.length}</span>
              </div>
            );
          })}
        </div>
      </div>

      <div class="lv-cues-main" style={{ overflow: "auto" }}>
        {!cur ? (
          <div class="panel panel-body dim">{t("Create a cuelist to begin.")}</div>
        ) : (
          <>
            <div class="panel">
              <div class="panel-head">
                <span>{cur.name}</span><span class="sp" />
                <label class="check"><input type="checkbox" checked={cur.looped} onChange={(e) => save({ looped: (e.target as HTMLInputElement).checked })} />{t("Loop")}</label>
                <label class="check" title={t("Pre-position moving heads that are dark in this cue and lit in the next one")}><input type="checkbox" checked={cur.auto_mark} onChange={(e) => save({ auto_mark: (e.target as HTMLInputElement).checked })} />{t("Auto-mark")}</label>
                <label class="check"><input type="checkbox" checked={cur.tracking} onChange={(e) => save({ tracking: (e.target as HTMLInputElement).checked })} />{t("Tracking")}</label>
                <label class="check">{t("Priority")} <input class="input num" type="number" min={0} max={255} style={{ width: 60 }} value={cur.priority}
                  onChange={(e) => save({ priority: Math.max(0, Math.min(255, Number((e.target as HTMLInputElement).value) || 0)) })} /></label>
                <button class="btn sm" onClick={() => setDlg("rename")}>{t("Rename")}</button>
                <button class="btn sm danger" onClick={() => setDlg("delete")}>{t("Delete")}</button>
              </div>
              <Transport list={cur} />
            </div>

            <div class="row">
              <button class="btn" onClick={() => setDlg("insert")}>{t("+ Insert cue")}</button>
              <button class="btn" onClick={renumber} disabled={!cur.cues.length}>{t("Renumber")}</button>
              <span class="dim">{t("Double-click a cue to GO TO it · Space = GO")}</span>
            </div>

            <CueTable list={cur} rt={c_rt} selected={selCue} onSelect={setSelCue} />
            <CueDetail list={cur} number={selCue} />
          </>
        )}
        <Executors />
      </div>
      <Macros />

      {dlg === "new" && <PromptDialog title={t("New cuelist")} label={t("Name")} initial={t("Cuelist {n}", { n: Math.max(0, ...lists.map((l) => l.id)) + 1 })} ok={t("Create")} onClose={() => setDlg(null)} onSubmit={newList} />}
      {dlg === "rename" && cur && <PromptDialog title={t("Rename cuelist")} label={t("Name")} initial={cur.name} onClose={() => setDlg(null)} onSubmit={(name) => save({ name })} />}
      {dlg === "delete" && cur && <ConfirmDialog title={t("Delete cuelist")} text={t("Delete “{name}” and its {n} cue(s)?", { name: cur.name, n: cur.cues.length })} ok={t("Delete")} danger onClose={() => setDlg(null)}
        onYes={() => { send({ type: "cuelist_remove", id: cur.id }); activeCuelist.value = null; }} />}
      {dlg === "insert" && cur && <InsertCue list={cur} after={selCue} onClose={() => setDlg(null)} onDone={setSelCue} />}
    </div>
  );
}

function Transport({ list }: { list: Cuelist }) {
  const r = rt.value?.cuelists.find((c) => c.id === list.id);
  const state = r?.state ?? "idle";
  const cue = r?.current != null ? list.cues.find((c) => c.number === r.current) : undefined;
  const prog = r?.progress ?? 0;
  const remain = cue && state === "running" ? Math.max(0, (1 - prog) * cue.fade_in) : 0;
  return (
    <div class="lv-transport">
      <button class="btn go lv-go" onClick={() => send({ type: "go", cuelist: list.id })} aria-label={t("Go")}>GO</button>
      <button class="btn lg" onClick={() => send({ type: "back", cuelist: list.id })} aria-label={t("Back")}>BACK</button>
      <button class={`btn lg ${state === "paused" ? "on" : ""}`} onClick={() => send({ type: "pause", cuelist: list.id })} aria-label={t("Pause")}>{t("PAUSE")}</button>
      <button class="btn lg danger" onClick={() => send({ type: "release_cuelist", cuelist: list.id })} aria-label={t("Release cuelist")}>{t("RELEASE")}</button>
      <div class="col" style={{ gap: 2 }}><span class="label">{t("Current")}</span><span class="lv-big" style={{ color: "var(--go)" }}>{r?.current != null ? fmtNum(r.current) : "—"}</span></div>
      <div class="col" style={{ gap: 2 }}><span class="label">{t("Next")}</span><span class="lv-big" style={{ color: "var(--live)" }}>{r?.next != null ? fmtNum(r.next) : "—"}</span></div>
      <ManualFade list={list} />
      <div class="col grow" style={{ gap: 4, minWidth: 180 }}>
        <div class="row"><span class="chip">{t(state)}</span><span class="mono dim">{cue?.label}</span><span class="sp" />
          <span class="mono" aria-label={t("Fade countdown")}>{state === "running" && prog < 1 ? `${remain.toFixed(1)} s` : ""}</span></div>
        <div class="lv-prog" role="progressbar" aria-valuenow={Math.round(prog * 100)} aria-valuemin={0} aria-valuemax={100}><i style={{ width: `${prog * 100}%` }} /></div>
      </div>
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
    <div class="col" style={{ gap: 4, minWidth: 220, flex: "1 1 220px" }}>
      <div class="row">
        <span class="label">{t("Manual fade")}</span><span class="sp" />
        <span class={`chip ${manual ? "warn" : ""}`}>{manual ? t("manual") : t("timed")}</span>
        <button class="btn sm" style={{ minHeight: 44 }} disabled={!manual} onClick={() => fire({ type: "cuelist_manual", cuelist: list.id, progress: null })}>{t("Auto")}</button>
      </div>
      <Fader horizontal width="100%" height={44} value={shown} color="var(--live)" held={manual} ariaLabel={t("Manual crossfade")}
        onChange={(v) => { setDrag(v); fire({ type: "cuelist_manual", cuelist: list.id, progress: v }); }} onEnd={() => setDrag(null)} />
    </div>
  );
}
