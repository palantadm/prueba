import { useEffect, useState } from "preact/hooks";
import { send, show, toast } from "../../store";
import type { Cue, Cuelist, FadeCurve, UpdateMode } from "../../types";
import { attrLabel, fixtureById, fmtNum, pct } from "../../lib/attrs";
import { CURVES, curveValue } from "../../lib/curves";
import { focusPalette } from "../../lib/state";
import { goto } from "../../App";
import { Modal, Tabs } from "../../ui";
import { t, tp } from "../../i18n";

type Tab = "content" | "timing" | "notes";
const GROUPS: { id: string; label: string }[] = [
  { id: "intensity", label: "Intensity" },
  { id: "position", label: "Position" },
  { id: "color", label: "Color" },
  { id: "beam", label: "Beam" },
  { id: "control", label: "Control" },
];
const big = { minHeight: 44 } as const;

/** Detail editor of the selected cue: contents (with palette links), per-group timing, notes. */
export function CueDetail({ list, number }: { list: Cuelist; number: number | null }) {
  const [tab, setTab] = useState<Tab>("content");
  const [upd, setUpd] = useState(false);
  const cue: Cue | undefined = number === null ? undefined : list.cues.find((c) => c.number === number);
  if (!cue) return <div class="panel panel-body dim" style={{ flex: "none" }}>{t("Select a cue to see what it stores.")}</div>;
  const save = (patch: Partial<Cue>) => send({ type: "cue_save", cuelist: list.id, cue: { ...cue, ...patch } });
  const entries = Object.entries(cue.changes).filter(([, a]) => a && Object.keys(a).length);
  const nTiming = Object.keys(cue.timing ?? {}).length + (cue.stagger ? 1 : 0) + (cue.curve !== "linear" ? 1 : 0);
  return (
    <div class="panel" style={{ flex: "none" }}>
      <div class="panel-head" style={{ flexWrap: "wrap", height: "auto", minHeight: 32 }}>
        <span>{t("Cue")} {fmtNum(cue.number)} {cue.label && `· ${cue.label}`}</span><span class="sp" />
        <button class="btn sm" style={big} onClick={() => send({ type: "cue_load", cuelist: list.id, number: cue.number }).then(() => toast(t("Cue {n} loaded into the programmer", { n: fmtNum(cue.number) }), "ok"))
          .catch(() => {})} title={t("Copy this cue's look into the programmer to edit it")}>{t("Load into programmer")}</button>
        <button class="btn sm primary" style={big} onClick={() => setUpd(true)}>{t("Update…")}</button>
      </div>
      <Tabs<Tab> value={tab} onChange={setTab} options={[
        { value: "content", label: t("Contents"), badge: entries.length },
        { value: "timing", label: t("Timing"), badge: nTiming || undefined },
        { value: "notes", label: t("Notes"), badge: cue.notes ? "•" : undefined },
      ]} />
      <div class="panel-body lv-detail" style={{ maxHeight: tab === "content" ? 180 : "none" }}>
        {tab === "content" && <Content list={list} cue={cue} entries={entries} save={save} />}
        {tab === "timing" && <Timing cue={cue} save={save} />}
        {tab === "notes" && <Notes cue={cue} save={save} />}
      </div>
      {upd && <UpdateDialog list={list} cue={cue} onClose={() => setUpd(false)} />}
    </div>
  );
}

function Content({ list, cue, entries, save }: { list: Cuelist; cue: Cue; entries: [string, Record<string, number | undefined> | undefined][]; save: (p: Partial<Cue>) => void }) {
  void list;
  const removeAttr = (fid: string, attr: string) => {
    const changes = { ...cue.changes };
    const a = { ...(changes[Number(fid)] ?? {}) };
    delete a[attr];
    if (Object.keys(a).length) changes[Number(fid)] = a; else delete changes[Number(fid)];
    const refs = { ...(cue.refs ?? {}) };
    if (refs[Number(fid)]) {
      const r = { ...refs[Number(fid)]! };
      delete r[attr];
      if (Object.keys(r).length) refs[Number(fid)] = r; else delete refs[Number(fid)];
    }
    save({ changes, refs });
  };
  const palName = (id: number) => show.value?.palettes.find((p) => p.id === id)?.name ?? `#${id}`;
  return (
    <>
      <div class="dim" style={{ marginBottom: 4 }}>{tp(entries.length, "{n} fixture stored", "{n} fixtures stored")}</div>
      {entries.length === 0 && <span class="dim">{t("Empty cue (tracks previous state).")}</span>}
      {entries.map(([fid, attrs]) => (
        <div class="lv-detail-fx" key={fid}>
          <b class="mono" style={{ minWidth: 90 }}>{fid} {fixtureById(Number(fid))?.name ?? ""}</b>
          {Object.entries(attrs!).map(([a, v]) => {
            const ref = cue.refs?.[Number(fid)]?.[a];
            return (
              <span class="chip" key={a} style={{ minHeight: 28, height: "auto" }}>
                {attrLabel(a)} <span class="mono">{pct(v ?? 0)}</span>
                {ref !== undefined && (
                  <button class="lv-palbadge" style={{ minHeight: 28 }} title={t("Linked to palette “{name}” — tap to show it", { name: palName(ref) })}
                    onClick={() => { focusPalette.value = ref; goto("live"); }}>⛓ {palName(ref)}</button>
                )}
                <button class="lv-x" style={{ minWidth: 28, minHeight: 28 }} aria-label={t("Remove {attr} from fixture {id}", { attr: attrLabel(a), id: fid })} onClick={() => removeAttr(fid, a)}>✕</button>
              </span>
            );
          })}
        </div>
      ))}
    </>
  );
}

function Num(p: { value: number | undefined; placeholder: string; label: string; onCommit: (v: number | undefined) => void }) {
  const [v, setV] = useState(p.value === undefined ? "" : fmtNum(p.value));
  useEffect(() => setV(p.value === undefined ? "" : fmtNum(p.value)), [p.value]);
  const commit = () => {
    if (v.trim() === "") return p.value !== undefined && p.onCommit(undefined);
    const n = parseFloat(v.replace(",", "."));
    if (Number.isFinite(n) && n >= 0 && n !== p.value) p.onCommit(n);
    else setV(p.value === undefined ? "" : fmtNum(p.value));
  };
  return (
    <input class="input num" style={{ ...big, width: 90 }} inputMode="decimal" value={v} placeholder={p.placeholder} aria-label={p.label}
      onInput={(e) => setV((e.target as HTMLInputElement).value)} onBlur={commit}
      onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Enter") (e.target as HTMLInputElement).blur(); if (e.key === "Escape") { setV(p.value === undefined ? "" : fmtNum(p.value)); (e.target as HTMLInputElement).blur(); } }} />
  );
}

function CurveGraph({ curve }: { curve: FadeCurve }) {
  const W = 120, H = 70;
  const pts = Array.from({ length: 41 }, (_, i) => { const x = i / 40; return `${(x * W).toFixed(1)},${(H - curveValue(curve, x) * H).toFixed(1)}`; }).join(" ");
  return (
    <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} role="img" aria-label={t("Fade curve preview")} style={{ background: "var(--bg-3)", border: "1px solid var(--line-strong)", borderRadius: 6, flex: "none" }}>
      <line x1="0" y1={H} x2={W} y2="0" stroke="var(--line-strong)" stroke-dasharray="3 3" />
      <polyline points={pts} fill="none" stroke="var(--accent)" stroke-width="2.5" />
    </svg>
  );
}

function Timing({ cue, save }: { cue: Cue; save: (p: Partial<Cue>) => void }) {
  const timing = cue.timing ?? {};
  const setGroup = (g: string, key: "delay" | "fade", v: number | undefined) => {
    const base = { delay: cue.delay, fade: cue.fade_in };
    const next = { ...(timing[g] ?? base), [key]: v ?? base[key] };
    const all = { ...timing };
    if (next.delay === base.delay && next.fade === base.fade) delete all[g]; else all[g] = next;
    save({ timing: all });
  };
  return (
    <div class="col" style={{ gap: 12 }}>
      <div class="dim">{t("Blank = use the cue's own delay / fade. Per-group values override them for that group of attributes.")}</div>
      <table class="tbl lv-timing" aria-label={t("Timing per attribute group")}>
        <thead><tr><th>{t("Group")}</th><th>{t("Delay (s)")}</th><th>{t("Fade (s)")}</th></tr></thead>
        <tbody>
          <tr class="dim"><td>{t("Cue default")}</td><td class="mono">{fmtNum(cue.delay)}</td><td class="mono">{fmtNum(cue.fade_in)} / {fmtNum(cue.fade_out)}</td></tr>
          {GROUPS.map((g) => (
            <tr key={g.id}>
              <td>{t(g.label)}</td>
              <td><Num value={timing[g.id]?.delay} placeholder={fmtNum(cue.delay)} label={t("{g} delay", { g: t(g.label) })} onCommit={(v) => setGroup(g.id, "delay", v)} /></td>
              <td><Num value={timing[g.id]?.fade} placeholder={fmtNum(cue.fade_in)} label={t("{g} fade", { g: t(g.label) })} onCommit={(v) => setGroup(g.id, "fade", v)} /></td>
            </tr>
          ))}
        </tbody>
      </table>
      <div class="row wrap" style={{ alignItems: "flex-start", gap: 16 }}>
        <div class="col" style={{ gap: 6 }}>
          <div class="label">{t("Fade curve")}</div>
          <div class="row wrap" role="radiogroup" aria-label={t("Fade curve")} style={{ maxWidth: 380 }}>
            {CURVES.map((c) => (
              <button key={c.id} role="radio" aria-checked={cue.curve === c.id} class={`btn ${cue.curve === c.id ? "sel" : ""}`} style={big} onClick={() => save({ curve: c.id })}>{t(c.label)}</button>
            ))}
          </div>
        </div>
        <CurveGraph curve={cue.curve} />
        <div class="col" style={{ gap: 6 }}>
          <div class="label">{t("Stagger (s per fixture)")}</div>
          <Num value={cue.stagger || undefined} placeholder="0" label={t("Stagger")} onCommit={(v) => save({ stagger: v ?? 0 })} />
          <div class="dim" style={{ maxWidth: 220 }}>{t("Adds this delay for each fixture in number order, so the look rolls across the rig.")}</div>
        </div>
      </div>
    </div>
  );
}

function Notes({ cue, save }: { cue: Cue; save: (p: Partial<Cue>) => void }) {
  const [v, setV] = useState(cue.notes ?? "");
  useEffect(() => setV(cue.notes ?? ""), [cue.number, cue.notes]);
  return (
    <div class="field">
      <label>{t("Cue notes")}</label>
      <textarea class="input" style={{ minHeight: 110 }} value={v} placeholder={t("Stage manager call, standby, cue description…")} aria-label={t("Cue notes")}
        onInput={(e) => setV((e.target as HTMLTextAreaElement).value)} onBlur={() => v !== (cue.notes ?? "") && save({ notes: v })}
        onKeyDown={(e) => e.stopPropagation()} />
    </div>
  );
}

const MODES: { id: UpdateMode; label: string; help: string }[] = [
  { id: "merge", label: "Merge", help: "Add the programmer's values to the cue; keep everything else." },
  { id: "replace", label: "Replace", help: "The cue becomes exactly what is in the programmer." },
  { id: "track_forward", label: "Track forward", help: "Also push the changes into the following cues until they set the attribute again." },
  { id: "cue_only", label: "This cue only", help: "Change only this cue; following cues keep their current look." },
];

function UpdateDialog({ list, cue, onClose }: { list: Cuelist; cue: Cue; onClose: () => void }) {
  const [mode, setMode] = useState<UpdateMode>("merge");
  const [refs, setRefs] = useState(true);
  const go = () => {
    send({ type: "cue_update", cuelist: list.id, number: cue.number, mode, refs })
      .then(() => { toast(t("Cue {n} updated", { n: fmtNum(cue.number) }), "ok"); onClose(); }).catch(() => {});
  };
  const cur = MODES.find((m) => m.id === mode)!;
  return (
    <Modal title={t("Update cue {n}", { n: fmtNum(cue.number) })} onClose={onClose} footer={<>
      <button class="btn lg" style={big} onClick={onClose}>{t("Cancel")}</button>
      <button class="btn lg primary" style={big} onClick={go}>{t("Update")}</button>
    </>}>
      <div class="row wrap" role="radiogroup" aria-label={t("Update mode")}>
        {MODES.map((m) => <button key={m.id} role="radio" aria-checked={mode === m.id} class={`btn ${mode === m.id ? "sel" : ""}`} style={big} onClick={() => setMode(m.id)}>{t(m.label)}</button>)}
      </div>
      <div class="dim">{t(cur.help)}</div>
      <label class="check" style={big}><input type="checkbox" checked={refs} onChange={(e) => setRefs((e.target as HTMLInputElement).checked)} />{t("Keep palette links")}</label>
      <div class="dim">{t("Updates use the values currently in the programmer.")}</div>
    </Modal>
  );
}
