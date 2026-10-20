import "../styles/live.css";
import { useEffect, useState } from "preact/hooks";
import { rt, send, show, toast } from "../store";
import type { Cue, Cuelist } from "../types";
import { attrLabel, fixtureById, fmtNum, pct } from "../lib/attrs";
import { activeCuelist } from "../lib/state";
import { useKey } from "../lib/hooks";
import { CueTable, InsertCue } from "./cues/CueTable";
import { Executors } from "./cues/Executors";
import { ConfirmDialog, PromptDialog } from "./live/dialogs";

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
    send({ type: "cuelist_save", cuelist: { id, name, cues: [], looped: false, tracking: true, priority: 0 } }).then(() => (activeCuelist.value = id));
  };
  const renumber = () => {
    if (!cur) return;
    const cues = [...cur.cues].sort((a, b) => a.number - b.number).map((c, i) => ({ ...c, number: i + 1 }));
    send({ type: "cuelist_save", cuelist: { ...cur, cues } }).then(() => toast("Cues renumbered 1…n", "ok"));
  };

  return (
    <div class="lv-cues">
      <div class="panel lv-col">
        <div class="panel-head"><span>Cuelists</span><span class="sp" /><button class="btn sm primary" onClick={() => setDlg("new")} aria-label="New cuelist">+ New</button></div>
        <div class="lv-scroll" role="listbox" aria-label="Cuelists">
          {lists.length === 0 && <div class="dim" style={{ padding: 12 }}>No cuelists yet. Create one, then record cues from the programmer.</div>}
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
          <div class="panel panel-body dim">Create a cuelist to begin.</div>
        ) : (
          <>
            <div class="panel">
              <div class="panel-head">
                <span>{cur.name}</span><span class="sp" />
                <label class="check"><input type="checkbox" checked={cur.looped} onChange={(e) => save({ looped: (e.target as HTMLInputElement).checked })} />Loop</label>
                <label class="check"><input type="checkbox" checked={cur.tracking} onChange={(e) => save({ tracking: (e.target as HTMLInputElement).checked })} />Tracking</label>
                <label class="check">Priority <input class="input num" type="number" min={0} max={255} style={{ width: 60 }} value={cur.priority}
                  onChange={(e) => save({ priority: Math.max(0, Math.min(255, Number((e.target as HTMLInputElement).value) || 0)) })} /></label>
                <button class="btn sm" onClick={() => setDlg("rename")}>Rename</button>
                <button class="btn sm danger" onClick={() => setDlg("delete")}>Delete</button>
              </div>
              <Transport list={cur} />
            </div>

            <div class="row">
              <button class="btn" onClick={() => setDlg("insert")}>+ Insert cue</button>
              <button class="btn" onClick={renumber} disabled={!cur.cues.length}>Renumber</button>
              <span class="dim">Double-click a cue to GO TO it · Space = GO</span>
            </div>

            <CueTable list={cur} rt={c_rt} selected={selCue} onSelect={setSelCue} />
            <Detail list={cur} number={selCue} />
          </>
        )}
        <Executors />
      </div>

      {dlg === "new" && <PromptDialog title="New cuelist" label="Name" initial={`Cuelist ${Math.max(0, ...lists.map((l) => l.id)) + 1}`} ok="Create" onClose={() => setDlg(null)} onSubmit={newList} />}
      {dlg === "rename" && cur && <PromptDialog title="Rename cuelist" label="Name" initial={cur.name} onClose={() => setDlg(null)} onSubmit={(name) => save({ name })} />}
      {dlg === "delete" && cur && <ConfirmDialog title="Delete cuelist" text={`Delete “${cur.name}” and its ${cur.cues.length} cue(s)?`} ok="Delete" danger onClose={() => setDlg(null)}
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
      <button class="btn go lv-go" onClick={() => send({ type: "go", cuelist: list.id })} aria-label="Go">GO</button>
      <button class="btn lg" onClick={() => send({ type: "back", cuelist: list.id })} aria-label="Back">BACK</button>
      <button class={`btn lg ${state === "paused" ? "on" : ""}`} onClick={() => send({ type: "pause", cuelist: list.id })} aria-label="Pause">PAUSE</button>
      <button class="btn lg danger" onClick={() => send({ type: "release_cuelist", cuelist: list.id })} aria-label="Release cuelist">RELEASE</button>
      <div class="col" style={{ gap: 2 }}><span class="label">Current</span><span class="lv-big" style={{ color: "var(--go)" }}>{r?.current != null ? fmtNum(r.current) : "—"}</span></div>
      <div class="col" style={{ gap: 2 }}><span class="label">Next</span><span class="lv-big" style={{ color: "var(--live)" }}>{r?.next != null ? fmtNum(r.next) : "—"}</span></div>
      <div class="col grow" style={{ gap: 4, minWidth: 180 }}>
        <div class="row"><span class="chip">{state}</span><span class="mono dim">{cue?.label}</span><span class="sp" />
          <span class="mono" aria-label="Fade countdown">{state === "running" && prog < 1 ? `${remain.toFixed(1)} s` : ""}</span></div>
        <div class="lv-prog" role="progressbar" aria-valuenow={Math.round(prog * 100)} aria-valuemin={0} aria-valuemax={100}><i style={{ width: `${prog * 100}%` }} /></div>
      </div>
    </div>
  );
}

function Detail({ list, number }: { list: Cuelist; number: number | null }) {
  const cue: Cue | undefined = number === null ? undefined : list.cues.find((c) => c.number === number);
  if (!cue) return <div class="panel panel-body dim" style={{ flex: "none" }}>Select a cue to see what it stores.</div>;
  const entries = Object.entries(cue.changes).filter(([, a]) => a && Object.keys(a).length);
  const removeAttr = (fid: string, attr: string) => {
    const changes = { ...cue.changes };
    const a = { ...(changes[Number(fid)] ?? {}) };
    delete a[attr];
    if (Object.keys(a).length) changes[Number(fid)] = a; else delete changes[Number(fid)];
    send({ type: "cue_save", cuelist: list.id, cue: { ...cue, changes } });
  };
  return (
    <div class="panel" style={{ flex: "none" }}>
      <div class="panel-head">Cue {fmtNum(cue.number)} {cue.label && `· ${cue.label}`}<span class="sp" /><span>{entries.length} fixture{entries.length === 1 ? "" : "s"} stored</span></div>
      <div class="panel-body lv-detail">
        {entries.length === 0 && <span class="dim">Empty cue (tracks previous state).</span>}
        {entries.map(([fid, attrs]) => (
          <div class="lv-detail-fx" key={fid}>
            <b class="mono" style={{ minWidth: 90 }}>{fid} {fixtureById(Number(fid))?.name ?? ""}</b>
            {Object.entries(attrs!).map(([a, v]) => (
              <span class="chip" key={a}>{attrLabel(a)} <span class="mono">{pct(v ?? 0)}</span>
                <button class="lv-x" aria-label={`Remove ${attrLabel(a)} from fixture ${fid}`} onClick={() => removeAttr(fid, a)}>✕</button></span>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
