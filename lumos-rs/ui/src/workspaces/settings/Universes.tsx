import { useMemo, useState } from "preact/hooks";
import type { OutputCfg, OutputStatus, UniverseCfg } from "../../types";
import { rt, send, show, toast } from "../../store";
import { EmptyState, Modal, NumberField, PageHead, Segmented, Select, StatusDot, TextField, Toggle, VirtualList, confirmDialog } from "../../ui";
import { NodesPanel } from "./Network";
import { KINDS, KindFields, ProtoGrid, advance, defaultKind, defaultOutput, summarize, type KindName } from "./OutputEditor";
import { TestPanel } from "./OutputTest";
import { t, tp } from "../../i18n";
import { tErr } from "../../i18n/errors";

const ROW = 64;

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

function UniverseEditor(props: { u: UniverseCfg; status: OutputStatus[]; open: boolean; onClose: () => void }) {
  const [u, setU] = useState<UniverseCfg>(structuredClone(props.u));
  const [oi, setOi] = useState(0);
  const dirty = JSON.stringify(u) !== JSON.stringify(props.u);
  const n = u.outputs.length;
  const i = Math.min(oi, Math.max(0, n - 1));
  const o = u.outputs[i];
  const st = props.status.find((x) => x.index === i);
  const testing = !!rt.value?.testing;
  const setOut = (x: OutputCfg) => setU({ ...u, outputs: u.outputs.map((y, j) => (j === i ? x : y)) });
  const stateTxt = !o ? t("No output") : !o.enabled ? t("Paused") : st?.state === "ok" ? t("Sending") : st?.state === "error" ? t("Error") : t("Off");
  const dot: "ok" | "err" | "off" = o && o.enabled ? (st?.state === "ok" ? "ok" : st?.state === "error" ? "err" : "off") : "off";
  const save = () => send({ type: "universe_save", universe: u }).catch(() => {});
  return (
    <aside class={`uni-ed ${props.open ? "open" : ""}`} aria-label={t("Edit universe")}>
      <div class="hd">
        <div class="grow-t">
          <div class="cap">{t("Universe {id}", { id: u.id })} · {props.u.name}</div>
          <div class="ttl">{n ? t("Output {i} of {n}", { i: i + 1, n }) : t("No output")}</div>
        </div>
        <button class="btn" disabled={!dirty} aria-label={t("Revert changes")} title={t("Revert changes")} onClick={() => setU(structuredClone(props.u))}>↺</button>
        <button class="btn primary" disabled={!dirty} onClick={save}>{t("Save")}</button>
        <button class="btn ghost icon close" aria-label={t("Close")} onClick={props.onClose}>✕</button>
      </div>
      <div class="bd">
        <TextField label={t("Name")} class="bare" value={u.name} onChange={(name) => setU({ ...u, name })} />
        {n > 1 && (
          <div class="outtabs" role="tablist" aria-label={t("Outputs")}>
            {u.outputs.map((_, j) => <button key={j} role="tab" aria-selected={j === i} class={j === i ? "on" : ""} onClick={() => setOi(j)}>{j + 1}</button>)}
          </div>
        )}
        {!o ? <div class="dim">{t("No outputs — this universe is rendered but not sent anywhere.")}</div> : <>
          <div>
            <span class="lbl-cap">{t("Protocol")}</span>
            <ProtoGrid value={o.kind.type} onChange={(ty) => setOut({ ...o, kind: defaultKind(ty) })} />
          </div>
          <KindFields kind={o.kind} onChange={(kind) => setOut({ ...o, kind })} />
          <div class="statrow">
            <StatusDot state={dot} title={stateTxt} />
            <div class="t"><b>{stateTxt}</b><small>{(st?.message && tErr(st.message)) || summarize(o)}{st?.state === "ok" ? ` · ${t("{frames} frames", { frames: st.frames })}` : ""}</small></div>
            <Toggle checked={o.enabled} onChange={(enabled) => setOut({ ...o, enabled })} label={t("Enabled")} />
            <button class="btn" onClick={() => send({ type: "output_test", mode: testing ? { type: "off" } : { type: "chase", universe: u.id } }).catch(() => {})}>{testing ? t("Stop") : t("Test")}</button>
          </div>
        </>}
      </div>
      <div class="ft">
        <div class="two">
          <button class="btn danger" disabled={!o} onClick={() => { setU({ ...u, outputs: u.outputs.filter((_, j) => j !== i) }); setOi(Math.max(0, i - 1)); }}>{t("Remove output")}</button>
          <button class="btn" onClick={() => { setU({ ...u, outputs: [...u.outputs, defaultOutput("art_net")] }); setOi(n); }}>{t("+ Another output")}</button>
        </div>
      </div>
    </aside>
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
  const [open, setOpen] = useState(false);
  const [showTest, setShowTest] = useState(false);
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
  const editId = edit !== null && rows.some((r) => r.id === edit) ? edit : rows[0]?.id ?? null;
  const editU = editId !== null ? universes.find((u) => u.id === editId) : undefined;
  const pickRow = (id: number) => { setEdit(id); setOpen(true); };
  return (
    <div class="uni-wrap">
      <div class="uni-left">
        <section class="uni-card grow">
          <PageHead title={t("Universes & outputs")} sub={t("{n} universes · each can drive any number of outputs", { n: universes.length })}>
            <button class="btn" onClick={() => setDlg("bulk")} title={t("Assign a protocol to a range of universes")}>{t("Bulk assign…")}</button>
            <button class={`btn ${showTest || rt.value?.testing ? "sel" : ""}`} aria-pressed={showTest} onClick={() => setShowTest(!showTest)}>{t("Output test")}</button>
            <button class="btn cyan" onClick={() => setDlg("add")}>{t("+ Add")}</button>
          </PageHead>
          {(selIds.length > 0 || universes.length > 6) && (
            <div class="uni-bar">
              {universes.length > 6 && <input class="input" style={{ width: 200 }} placeholder={t("Filter…")} aria-label={t("Filter universes")} value={filter} onInput={(e) => setFilter((e.target as HTMLInputElement).value)} />}
              <span class="sp" />
              {selIds.length > 0 && <span class="dim">{selIds.length} ✓</span>}
              <button class="btn" disabled={selIds.length < 2} onClick={() => setDlg("copy")} title={t("Copy outputs of the lowest selected universe to the others")}>{t("Copy to selected")}</button>
              <button class="btn danger" disabled={!selIds.length} onClick={async () => {
                if (!(await confirmDialog(t("Remove universes"), t("Remove {n} universe(s)? Fixtures patched to them stay in the show.", { n: selIds.length }), { danger: true, ok: t("Remove") }))) return;
                for (const id of selIds) await send({ type: "universe_remove", id }, { quiet: true }).catch(() => {});
                setSel(new Set());
              }}>{t("Remove")}{selIds.length ? ` (${selIds.length})` : ""}</button>
            </div>
          )}
          {(showTest || rt.value?.testing) && <TestPanel />}
          <div class="uni-cols">
            <label class="pick" style={{ display: "grid", placeItems: "center", width: 40, height: 40 }}>
              <input type="checkbox" aria-label={t("Select all")} checked={sel.size > 0 && sel.size === rows.length} onChange={(e) => setSel((e.target as HTMLInputElement).checked ? new Set(rows.map((r) => r.id)) : new Set())} />
            </label>
            <span>{t("No.")}</span><span>{t("Name")}</span><span>{t("Outputs")}</span><span>{t("Status")}</span><span>{t("Frames")}</span>
          </div>
          {rows.length === 0 ? <EmptyState title={t("No universes")} hint={t("Add universes, then assign Art-Net, sACN, OSC or USB outputs.")} icon="⎍"><button class="btn primary tall" onClick={() => setDlg("add")}>{t("+ Add universes")}</button></EmptyState> :
            <VirtualList class="grow" label={t("Universes")} items={rows} rowHeight={ROW} render={(u) => {
              const st = stByU.get(u.id) ?? [];
              const frames = st.reduce((n, x) => n + x.frames, 0);
              const w = worst(st);
              return (
                <div key={u.id} role="listitem" tabIndex={0} class={`uni-row ${u.id === editId ? "sel" : ""}`} style={{ height: ROW }}
                  onClick={() => pickRow(u.id)} onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && e.target === e.currentTarget && (e.preventDefault(), pickRow(u.id))}>
                  <label class="pick" onClick={(e) => e.stopPropagation()}>
                    <input type="checkbox" aria-label={t("Select universe {id}", { id: u.id })} checked={sel.has(u.id)} onClick={(e) => toggle(u.id, (e as MouseEvent).shiftKey)} />
                  </label>
                  <span class="n">{u.id}</span>
                  <span class="nm">{u.name}</span>
                  <span class="outs">
                    {u.outputs.length === 0 ? <span class="ochip none">{t("No output")}</span> : <>{u.outputs.map((o, i) => <span key={i} class={`ochip ${o.kind.type} ${o.enabled ? "" : "off"} ${i > 0 ? "extra" : ""}`}>{summarize(o)}</span>)}{u.outputs.length > 1 && <span class="ochip more">+{u.outputs.length - 1}</span>}</>}
                  </span>
                  <span class="st" title={st.map((x) => x.message).filter(Boolean).join("\n")}>
                    <StatusDot state={w} title={{ ok: t("OK"), err: t("Error"), off: t("Off") }[w]} />{{ ok: t("Sending"), err: t("Error"), off: u.outputs.length ? t("Off") : t("No output") }[w]}
                  </span>
                  <span class="fr">{frames || "—"}</span>
                </div>
              );
            }} />}
        </section>
        <NodesPanel />
      </div>
      {editU ? <UniverseEditor key={editU.id} u={editU} status={stByU.get(editU.id) ?? []} open={open} onClose={() => setOpen(false)} />
        : <aside class="uni-ed"><div class="bd"><EmptyState title={t("Select a universe")} hint={t("Pick a universe in the list to edit its outputs.")} /></div></aside>}
      {dlg === "add" && <AddDialog existing={new Set(universes.map((u) => u.id))} onClose={() => setDlg(null)} />}
      {dlg === "bulk" && <BulkDialog ids={selIds} all={sorted.map((u) => u.id)} onClose={() => setDlg(null)} />}
      {dlg === "copy" && selIds.length > 1 && <CopyDialog source={universes.find((u) => u.id === selIds[0])!} targets={selIds.slice(1)} onClose={() => setDlg(null)} />}
    </div>
  );
}
