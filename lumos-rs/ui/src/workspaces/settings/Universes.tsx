import { useMemo, useState } from "preact/hooks";
import type { OutputCfg, OutputStatus, UniverseCfg } from "../../types";
import { rt, send, show, toast } from "../../store";
import { EmptyState, Modal, NumberField, PageHead, Segmented, Select, StatusDot, TextField, Toggle, VirtualList, confirmDialog } from "../../ui";
import { KINDS, KindFields, OutputEditor, advance, defaultKind, defaultOutput, summarize, type KindName } from "./OutputEditor";
import { TestPanel } from "./OutputTest";
import { t, tp } from "../../i18n";

const ROW = 40;

function worst(sts: OutputStatus[]): "ok" | "err" | "off" {
  if (sts.some((s) => s.state === "error")) return "err";
  if (sts.some((s) => s.state === "ok")) return "ok";
  return "off";
}

async function saveMany(list: UniverseCfg[]) {
  let failed = 0;
  for (const u of list) await send({ type: "universe_save", universe: u }, { quiet: true }).catch(() => failed++);
  if (failed) toast(t("{failed} universe(s) failed to save", { failed }), "err");
  else toast(tp(list.length, "{n} universe updated", "{n} universes updated"), "ok", 1800);
}

function UniverseDialog(props: { u: UniverseCfg; status: OutputStatus[]; onClose: () => void }) {
  const [u, setU] = useState<UniverseCfg>(structuredClone(props.u));
  const setOut = (i: number, o: OutputCfg) => setU({ ...u, outputs: u.outputs.map((x, j) => (j === i ? o : x)) });
  return (
    <Modal title={t("Universe {id}", { id: u.id })} width={640} onClose={props.onClose}
      footer={<>
        <button class="btn" onClick={props.onClose}>{t("Cancel")}</button>
        <button class="btn primary" onClick={async () => { await send({ type: "universe_save", universe: u }).then(props.onClose).catch(() => {}); }}>{t("Save")}</button>
      </>}>
      <TextField label={t("Name")} value={u.name} onChange={(name) => setU({ ...u, name })} />
      {u.outputs.length === 0 && <div class="dim">{t("No outputs — this universe is rendered but not sent anywhere.")}</div>}
      {u.outputs.map((o, i) => (
        <OutputEditor key={i} value={o} onChange={(x) => setOut(i, x)} status={props.status.find((s) => s.index === i)}
          onRemove={() => setU({ ...u, outputs: u.outputs.filter((_, j) => j !== i) })} />
      ))}
      <button class="btn" onClick={() => setU({ ...u, outputs: [...u.outputs, defaultOutput("art_net")] })}>{t("+ Add output")}</button>
    </Modal>
  );
}

function AddDialog(props: { existing: Set<number>; onClose: () => void }) {
  let next = 1;
  while (props.existing.has(next)) next++;
  const [start, setStart] = useState(next);
  const [count, setCount] = useState(1);
  const [prefix, setPrefix] = useState(t("Universe"));
  const clash = [...Array(count)].some((_, i) => props.existing.has(start + i));
  return (
    <Modal title={t("Add universes")} onClose={props.onClose}
      footer={<><button class="btn" onClick={props.onClose}>{t("Cancel")}</button>
        <button class="btn primary" disabled={clash} onClick={async () => {
          await saveMany([...Array(count)].map((_, i) => ({ id: start + i, name: `${prefix} ${start + i}`, outputs: [] })));
          props.onClose();
        }}>{t("Add {n}", { n: count })}</button></>}>
      <div class="grid2">
        <NumberField label={t("First universe id")} value={start} min={1} max={65535} onChange={setStart} />
        <NumberField label={t("How many")} value={count} min={1} max={512} onChange={setCount} />
      </div>
      <TextField label={t("Name prefix")} value={prefix} onChange={setPrefix} />
      {clash && <div style={{ color: "var(--stop)" }}>{t("Some of those ids already exist.")}</div>}
    </Modal>
  );
}

function BulkDialog(props: { ids: number[]; all: number[]; onClose: () => void }) {
  const lo = props.ids.length ? Math.min(...props.ids) : Math.min(...props.all, 1);
  const hi = props.ids.length ? Math.max(...props.ids) : Math.max(...props.all, 1);
  const [from, setFrom] = useState(lo);
  const [to, setTo] = useState(hi);
  const [kind, setKind] = useState<KindName>("art_net");
  const [base, setBase] = useState(defaultKind("art_net"));
  const [mode, setMode] = useState<"replace" | "append">("replace");
  const ids = props.all.filter((id) => id >= from && id <= to);
  const first = summarize({ enabled: true, kind: base });
  const last = summarize({ enabled: true, kind: advance(base, Math.max(0, ids.length - 1)) });
  return (
    <Modal title={t("Bulk assign outputs")} width={640} onClose={props.onClose}
      footer={<><button class="btn" onClick={props.onClose}>{t("Cancel")}</button>
        <button class="btn primary" disabled={!ids.length} onClick={async () => {
          const us = show.value?.universes ?? [];
          const list = ids.map((id, i): UniverseCfg => {
            const u = us.find((x) => x.id === id)!;
            const out: OutputCfg = { enabled: true, kind: advance(base, i) };
            return { ...u, outputs: mode === "replace" ? [out] : [...u.outputs, out] };
          });
          await saveMany(list);
          props.onClose();
        }}>{t("Apply to {n} universes", { n: ids.length })}</button></>}>
      <div class="grid2">
        <NumberField label={t("From universe")} value={from} min={1} onChange={setFrom} />
        <NumberField label={t("To universe")} value={to} min={1} onChange={setTo} />
      </div>
      <Select<KindName> label={t("Protocol")} value={kind} options={KINDS.map((x) => ({ ...x, label: t(x.label) }))} onChange={(k) => { setKind(k); setBase(defaultKind(k)); }} />
      <KindFields kind={base} onChange={setBase} />
      <Segmented value={mode} onChange={setMode} label={t("Existing outputs")} options={[{ value: "replace", label: t("Replace existing outputs") }, { value: "append", label: t("Add to existing") }]} />
      <div class="dim mono" style={{ fontSize: "var(--fs-sm)" }}>
        {ids.length ? <>U{ids[0]} → {first}<br />U{ids[ids.length - 1]} → {last}<br /><span class="faint">{t("Art-Net and sACN numbers increment per universe.")}</span></> : t("No existing universes in that range.")}
      </div>
    </Modal>
  );
}

function CopyDialog(props: { source: UniverseCfg; targets: number[]; onClose: () => void }) {
  const [inc, setInc] = useState(true);
  return (
    <Modal title={t("Copy outputs of universe {id}", { id: props.source.id })} onClose={props.onClose}
      footer={<><button class="btn" onClick={props.onClose}>{t("Cancel")}</button>
        <button class="btn primary" onClick={async () => {
          const us = show.value?.universes ?? [];
          const list = props.targets.map((id): UniverseCfg => {
            const u = us.find((x) => x.id === id)!;
            const d = id - props.source.id;
            return { ...u, outputs: props.source.outputs.map((o) => ({ ...o, kind: inc ? advance(o.kind, d) : structuredClone(o.kind) })) };
          });
          await saveMany(list);
          props.onClose();
        }}>{t("Copy to {n}", { n: props.targets.length })}</button></>}>
      <div>{t("Source outputs:")}</div>
      <ul class="mono" style={{ margin: 0 }}>{props.source.outputs.map((o, i) => <li key={i}>{summarize(o)}</li>)}</ul>
      <Toggle checked={inc} onChange={setInc} label={t("Increment Art-Net / sACN universe by the id difference")} />
    </Modal>
  );
}

export function UniversesPage() {
  const s = show.value;
  const [sel, setSel] = useState<Set<number>>(new Set());
  const [filter, setFilter] = useState("");
  const [edit, setEdit] = useState<number | null>(null);
  const [dlg, setDlg] = useState<"add" | "bulk" | "copy" | null>(null);
  const universes = s?.universes ?? [];
  const stByU = useMemo(() => {
    const m = new Map<number, OutputStatus[]>();
    for (const o of rt.value?.outputs ?? []) (m.get(o.universe) ?? m.set(o.universe, []).get(o.universe)!).push(o);
    return m;
  }, [rt.value?.outputs]);
  const sorted = useMemo(() => [...universes].sort((a, b) => a.id - b.id), [universes]);
  const f = filter.trim().toLowerCase();
  const rows = f ? sorted.filter((u) => String(u.id) === f || u.name.toLowerCase().includes(f) || u.outputs.some((o) => summarize(o).toLowerCase().includes(f))) : sorted;
  const selIds = [...sel].sort((a, b) => a - b);
  const toggle = (id: number, shift: boolean) => {
    const n = new Set(sel);
    if (shift && selIds.length) {
      const anchor = selIds[selIds.length - 1];
      const a = rows.findIndex((r) => r.id === anchor), b = rows.findIndex((r) => r.id === id);
      if (a >= 0 && b >= 0) rows.slice(Math.min(a, b), Math.max(a, b) + 1).forEach((r) => n.add(r.id));
    } else n.has(id) ? n.delete(id) : n.add(id);
    setSel(n);
  };
  const editU = edit !== null ? universes.find((u) => u.id === edit) : undefined;
  return (
    <div class="page">
      <PageHead title={t("Universes & outputs")} sub={t("{n} universes · each can drive any number of outputs", { n: universes.length })}>
        <input class="input" style={{ width: 180 }} placeholder={t("Filter…")} aria-label={t("Filter universes")} value={filter} onInput={(e) => setFilter((e.target as HTMLInputElement).value)} />
        <button class="btn tall" onClick={() => setDlg("add")}>{t("+ Add")}</button>
        <button class="btn tall" onClick={() => setDlg("bulk")} title={t("Assign a protocol to a range of universes")}>{t("Bulk assign…")}</button>
        <button class="btn tall" disabled={selIds.length < 2} onClick={() => setDlg("copy")} title={t("Copy outputs of the lowest selected universe to the others")}>{t("Copy to selected")}</button>
        <button class="btn tall danger" disabled={!selIds.length} onClick={async () => {
          if (!(await confirmDialog(t("Remove universes"), t("Remove {n} universe(s)? Fixtures patched to them stay in the show.", { n: selIds.length }), { danger: true, ok: t("Remove") }))) return;
          for (const id of selIds) await send({ type: "universe_remove", id }, { quiet: true }).catch(() => {});
          setSel(new Set());
        }}>{t("Remove")}{selIds.length ? ` (${selIds.length})` : ""}</button>
      </PageHead>
      <TestPanel />
      <div class="vrow" style={{ background: "var(--bg-2)", height: 32, fontSize: "var(--fs-xs)", color: "var(--text-dim)", textTransform: "uppercase", letterSpacing: ".06em", fontWeight: 700 }}>
        <input type="checkbox" aria-label={t("Select all")} checked={sel.size > 0 && sel.size === rows.length} onChange={(e) => setSel((e.target as HTMLInputElement).checked ? new Set(rows.map((r) => r.id)) : new Set())} />
        <span style={{ width: 48 }}>#</span><span style={{ width: 160 }}>{t("Name")}</span><span class="grow">{t("Outputs")}</span><span style={{ width: 70 }}>{t("Status")}</span><span style={{ width: 80, textAlign: "right" }}>{t("Frames")}</span><span style={{ width: 64 }} />
      </div>
      {rows.length === 0 ? <EmptyState title={t("No universes")} hint={t("Add universes, then assign Art-Net, sACN, OSC or USB outputs.")} icon="⎍"><button class="btn primary tall" onClick={() => setDlg("add")}>{t("+ Add universes")}</button></EmptyState> :
        <VirtualList class="grow" label={t("Universes")} items={rows} rowHeight={ROW} render={(u) => {
          const st = stByU.get(u.id) ?? [];
          const frames = st.reduce((n, x) => n + x.frames, 0);
          const w = worst(st);
          return (
            <div key={u.id} role="listitem" class={`vrow ${sel.has(u.id) ? "sel" : ""}`} style={{ height: ROW }} onDblClick={() => setEdit(u.id)}>
              <input type="checkbox" aria-label={t("Select universe {id}", { id: u.id })} checked={sel.has(u.id)} onClick={(e) => toggle(u.id, (e as MouseEvent).shiftKey)} />
              <span class="mono" style={{ width: 48 }}>{u.id}</span>
              <span style={{ width: 160, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{u.name}</span>
              <span class="grow dim" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {u.outputs.length === 0 ? <span class="faint">{t("— none —")}</span> : <>{u.outputs.map((o) => (o.enabled ? "" : "⏸ ") + summarize(o)).join("  ·  ")}</>}
              </span>
              <span style={{ width: 70 }} title={st.map((x) => x.message).filter(Boolean).join("\n")}><StatusDot state={w} title={{ ok: t("OK"), err: t("Error"), off: t("Off") }[w]} /> {u.outputs.length > 1 && <span class="faint">×{u.outputs.length}</span>}</span>
              <span class="mono dim" style={{ width: 80, textAlign: "right" }}>{frames || ""}</span>
              <button class="btn sm" style={{ width: 64 }} onClick={() => setEdit(u.id)}>{t("Edit")}</button>
            </div>
          );
        }} />}
      {editU && <UniverseDialog u={editU} status={stByU.get(editU.id) ?? []} onClose={() => setEdit(null)} />}
      {dlg === "add" && <AddDialog existing={new Set(universes.map((u) => u.id))} onClose={() => setDlg(null)} />}
      {dlg === "bulk" && <BulkDialog ids={selIds} all={sorted.map((u) => u.id)} onClose={() => setDlg(null)} />}
      {dlg === "copy" && selIds.length > 1 && <CopyDialog source={universes.find((u) => u.id === selIds[0])!} targets={selIds.slice(1)} onClose={() => setDlg(null)} />}
    </div>
  );
}
