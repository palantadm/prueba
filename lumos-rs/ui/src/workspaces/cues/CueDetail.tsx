import { Fragment } from "preact";
import { useEffect, useState } from "preact/hooks";
import { send, show, toast } from "../../store";
import type { Cue, Cuelist, FadeCurve, UpdateMode } from "../../types";
import { attrLabel, fixtureById, fmtNum, pct } from "../../lib/attrs";
import { CURVES, curveValue } from "../../lib/curves";
import { focusPalette } from "../../lib/state";
import { goto } from "../../App";
import { Modal } from "../../ui";
import { Cell, TC_RE } from "./CueTable";
import { SwitchRow } from "./SwitchRow";
import { t, tp } from "../../i18n";

type Tab = "content" | "timing" | "notes";
const GROUPS: { id: string; label: string }[] = [
  { id: "intensity", label: "Intensity" },
  { id: "position", label: "Position" },
  { id: "color", label: "Color" },
  { id: "beam", label: "Beam" },
  { id: "control", label: "Control" },
];
const big = { minHeight: 48 } as const;

/** Detail editor of the selected cue: timing, contents (with palette links), notes. */
export function CueDetail({ list, number, onDeleted }: { list: Cuelist; number: number | null; onDeleted: () => void }) {
  const [tab, setTab] = useState<Tab>("timing");
  const [upd, setUpd] = useState(false);
  const cue: Cue | undefined = number === null ? undefined : list.cues.find((c) => c.number === number);
  if (!cue) return <div class="cu-detail"><div class="cu-empty">{t("Select a cue to see what it stores.")}</div></div>;
  const save = (patch: Partial<Cue>) => send({ type: "cue_save", cuelist: list.id, cue: { ...cue, ...patch } });
  const entries = Object.entries(cue.changes).filter(([, a]) => a && Object.keys(a).length);
  const nTiming = Object.keys(cue.timing ?? {}).length + (cue.stagger ? 1 : 0) + (cue.curve !== "linear" ? 1 : 0);
  const tabs: { value: Tab; label: string; badge?: string | number }[] = [
    { value: "timing", label: t("Timing"), badge: nTiming || undefined },
    { value: "content", label: t("Contents"), badge: entries.length || undefined },
    { value: "notes", label: t("Notes"), badge: cue.notes ? "•" : undefined },
  ];
  return (
    <div class="cu-detail">
      <div class="cu-dh">
        <div class="ttl">
          <span class="label">{t("Cue")}</span>
          <b class="mono">{fmtNum(cue.number)}</b>
          <span class="lbl">{cue.label}</span>
        </div>
        <button class="btn icon" onClick={() => send({ type: "go_to", cuelist: list.id, number: cue.number })} title={t("Double-click: GO TO this cue")} aria-label={t("Go to cue {n}", { n: fmtNum(cue.number) })}>▶</button>
        <button class="btn icon danger" aria-label={t("Delete cue {n}", { n: fmtNum(cue.number) })}
          onClick={() => { send({ type: "cue_remove", cuelist: list.id, number: cue.number }); onDeleted(); }}>✕</button>
      </div>
      <div class="cu-seg" role="tablist">
        {tabs.map((o) => (
          <button key={o.value} role="tab" aria-selected={tab === o.value} class={tab === o.value ? "on" : ""} onClick={() => setTab(o.value)}>
            {o.label}{o.badge !== undefined && <span class="bd">{o.badge}</span>}
          </button>
        ))}
      </div>
      <div class="cu-db">
        {tab === "timing" && <Timing list={list} cue={cue} save={save} />}
        {tab === "content" && <Content list={list} cue={cue} entries={entries} save={save} />}
        {tab === "notes" && <Notes cue={cue} save={save} />}
      </div>
      <div class="cu-df">
        <button class="btn" onClick={() => send({ type: "cue_load", cuelist: list.id, number: cue.number }).then(() => toast(t("Cue {n} loaded into the programmer", { n: fmtNum(cue.number) }), "ok"))
          .catch(() => {})} title={t("Copy this cue's look into the programmer to edit it")}>{t("Load into programmer")}</button>
        <button class="btn primary" onClick={() => setUpd(true)}>{t("Update…")}</button>
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
      <div class="dim cu-cnt">{tp(entries.length, "{n} fixture stored", "{n} fixtures stored")}</div>
      {entries.length === 0 && <span class="dim">{t("Empty cue (tracks previous state).")}</span>}
      {entries.map(([fid, attrs]) => (
        <div class="cu-fx" key={fid}>
          <b class="mono fx">{fid} {fixtureById(Number(fid))?.name ?? ""}</b>
          {Object.entries(attrs!).map(([a, v]) => {
            const ref = cue.refs?.[Number(fid)]?.[a];
            return (
              <span class="chip cu-attr" key={a}>
                {attrLabel(a)} <span class="mono">{pct(v ?? 0)}</span>
                {ref !== undefined && (
                  <button class="lv-palbadge cu-pal" title={t("Linked to palette “{name}” — tap to show it", { name: palName(ref) })}
                    onClick={() => { focusPalette.value = ref; goto("live"); }}>⛓ {palName(ref)}</button>
                )}
                <button class="lv-x cu-x" aria-label={t("Remove {attr} from fixture {id}", { attr: attrLabel(a), id: fid })} onClick={() => removeAttr(fid, a)}>✕</button>
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
    <input class="input num cu-val" inputMode="decimal" value={v} placeholder={p.placeholder} aria-label={p.label}
      onInput={(e) => setV((e.target as HTMLInputElement).value)} onBlur={commit}
      onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Enter") (e.target as HTMLInputElement).blur(); if (e.key === "Escape") { setV(p.value === undefined ? "" : fmtNum(p.value)); (e.target as HTMLInputElement).blur(); } }} />
  );
}

function CurveGraph({ curve }: { curve: FadeCurve }) {
  const W = 120, H = 70;
  const pts = Array.from({ length: 41 }, (_, i) => { const x = i / 40; return `${(x * W).toFixed(1)},${(H - curveValue(curve, x) * H).toFixed(1)}`; }).join(" ");
  return (
    <svg class="cu-curve" width={W} height={H} viewBox={`0 0 ${W} ${H}`} role="img" aria-label={t("Fade curve preview")}>
      <line x1="0" y1={H} x2={W} y2="0" stroke="var(--line-strong)" stroke-dasharray="3 3" />
      <polyline points={pts} fill="none" stroke="var(--amber)" stroke-width="2.5" />
    </svg>
  );
}

function TcField({ cue, save }: { cue: Cue; save: (p: Partial<Cue>) => void }) {
  return (
    <div class="cu-tc">
      <span class="label">{t("Timecode")}</span>
      <Cell label={t("Timecode")} placeholder="HH:MM:SS:FF" value={cue.timecode ?? ""}
        onCommit={(v) => { if (v.trim() === "") save({ timecode: null }); else if (TC_RE.test(v.trim())) save({ timecode: v.trim() }); else toast(t("Timecode format HH:MM:SS:FF"), "err"); }} />
    </div>
  );
}

function Timing({ list, cue, save }: { list: Cuelist; cue: Cue; save: (p: Partial<Cue>) => void }) {
  const timing = cue.timing ?? {};
  const setGroup = (g: string, key: "delay" | "fade", v: number | undefined) => {
    const base = { delay: cue.delay, fade: cue.fade_in };
    const next = { ...(timing[g] ?? base), [key]: v ?? base[key] };
    const all = { ...timing };
    if (next.delay === base.delay && next.fade === base.fade) delete all[g]; else all[g] = next;
    save({ timing: all });
  };
  return (
    <div class="cu-timing">
      <div class="cu-defs mono" aria-label={t("Cue default")}>
        <div><span class="label">{t("Fade in")}</span><b>{fmtNum(cue.fade_in)}</b></div>
        <div><span class="label">{t("Fade out")}</span><b>{fmtNum(cue.fade_out)}</b></div>
        <div><span class="label">{t("Delay")}</span><b>{fmtNum(cue.delay)}</b></div>
        <div><span class="label">{t("Follow")}</span><b>{cue.follow == null ? "—" : fmtNum(cue.follow)}</b></div>
      </div>
      <div class="dim cu-hint">{t("Blank = use the cue's own delay / fade. Per-group values override them for that group of attributes.")}</div>
      <div class="cu-grid" role="group" aria-label={t("Timing per attribute group")}>
        <span class="label">{t("Group")}</span><span class="label">{t("Delay (s)")}</span><span class="label">{t("Fade (s)")}</span>
        {GROUPS.map((g) => (
          <Fragment key={g.id}>
            <span class="g">{t(g.label)}</span>
            <Num value={timing[g.id]?.delay} placeholder={fmtNum(cue.delay)} label={t("{g} delay", { g: t(g.label) })} onCommit={(v) => setGroup(g.id, "delay", v)} />
            <Num value={timing[g.id]?.fade} placeholder={fmtNum(cue.fade_in)} label={t("{g} fade", { g: t(g.label) })} onCommit={(v) => setGroup(g.id, "fade", v)} />
          </Fragment>
        ))}
      </div>
      <div class="label">{t("Fade curve")}</div>
      <div class="cu-curves" role="radiogroup" aria-label={t("Fade curve")}>
        {CURVES.map((c) => (
          <button key={c.id} role="radio" aria-checked={cue.curve === c.id} class={`pill ${cue.curve === c.id ? "on" : ""}`} onClick={() => save({ curve: c.id })}>{t(c.label)}</button>
        ))}
      </div>
      <div class="cu-curve-row">
        <CurveGraph curve={cue.curve} />
        <div class="col" style={{ gap: 6, flex: 1, minWidth: 0 }}>
          <div class="label">{t("Stagger (s per fixture)")}</div>
          <Num value={cue.stagger || undefined} placeholder="0" label={t("Stagger")} onCommit={(v) => save({ stagger: v ?? 0 })} />
          <div class="dim cu-hint">{t("Adds this delay for each fixture in number order, so the look rolls across the rig.")}</div>
        </div>
      </div>
      <TcField cue={cue} save={save} />
      <SwitchRow on={cue.block} onChange={(v) => save({ block: v })} label={t("Block")} />
      <SwitchRow on={list.auto_mark} onChange={(v) => send({ type: "cuelist_save", cuelist: { ...list, auto_mark: v } })} label={t("Auto-mark")}
        title={t("Pre-position moving heads that are dark in this cue and lit in the next one")} />
    </div>
  );
}

function Notes({ cue, save }: { cue: Cue; save: (p: Partial<Cue>) => void }) {
  const [v, setV] = useState(cue.notes ?? "");
  useEffect(() => setV(cue.notes ?? ""), [cue.number, cue.notes]);
  return (
    <div class="field">
      <label>{t("Cue notes")}</label>
      <textarea class="input cu-notes" value={v} placeholder={t("Stage manager call, standby, cue description…")} aria-label={t("Cue notes")}
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
