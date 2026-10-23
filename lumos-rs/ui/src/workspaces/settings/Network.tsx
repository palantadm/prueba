import { useEffect, useMemo, useState } from "preact/hooks";
import type { ArtNode, OutputCfg } from "../../types";
import { fire, nodes, send, show, toast } from "../../store";
import { Modal, Segmented, Select } from "../../ui";
import { OutputEditor } from "./OutputEditor";
import { t, tp } from "../../i18n";

const pa = (v: number) => `${(v >> 8) & 0x7f}:${(v >> 4) & 0xf}:${v & 0xf}`;

function usedIps(): Set<string> {
  const s = new Set<string>();
  for (const u of show.value?.universes ?? []) for (const o of u.outputs) if (o.kind.type === "art_net" && o.kind.host) s.add(o.kind.host);
  return s;
}

function AssignDialog(props: { node: ArtNode; onClose: () => void }) {
  const us = show.value?.universes ?? [];
  const n = props.node;
  const [uid, setUid] = useState(us[0]?.id ?? 1);
  const [portAddr, setPortAddr] = useState(n.universes[0] ?? 0);
  const [mode, setMode] = useState<"append" | "replace">("append");
  const out: OutputCfg = { enabled: true, kind: { type: "art_net", host: n.ip, port: 6454, net: (portAddr >> 8) & 0x7f, subnet: (portAddr >> 4) & 0xf, universe: portAddr & 0xf } };
  const [edited, setEdited] = useState<OutputCfg | null>(null);
  useEffect(() => setEdited(null), [portAddr]);
  const cur = edited ?? out;
  return (
    <Modal title={t("Send universe to node")} width={640} onClose={props.onClose}
      footer={<><button class="btn tall" style={{ minHeight: 44 }} onClick={props.onClose}>{t("Cancel")}</button>
        <button class="btn tall primary" style={{ minHeight: 44 }} disabled={!us.length} onClick={async () => {
          const u = us.find((x) => x.id === uid);
          if (!u) return;
          await send({ type: "universe_save", universe: { ...u, outputs: mode === "replace" ? [cur] : [...u.outputs, cur] } })
            .then(() => { toast(t("Output assigned"), "ok", 1500); props.onClose(); }).catch(() => {});
        }}>{t("Assign output")}</button></>}>
      <div class="dim">{n.name}{n.long_name && n.long_name !== n.name ? ` — ${n.long_name}` : ""} · <span class="mono">{n.ip}</span></div>
      {us.length === 0 ? <div class="dim">{t("Create a universe first (Universes & outputs).")}</div> : <>
        <Select<number> label={t("Lumos universe")} value={uid} options={us.map((u) => ({ value: u.id, label: `${u.id} · ${u.name}` }))} onChange={setUid} />
        {n.universes.length > 0
          ? <Select<number> label={t("Node port-address (net:sub:universe)")} value={portAddr} options={n.universes.map((v) => ({ value: v, label: pa(v) }))} onChange={setPortAddr} />
          : <div class="faint">{t("The node did not report its port-addresses; adjust them below.")}</div>}
        <Segmented value={mode} onChange={setMode} label={t("Existing outputs")} options={[{ value: "append", label: t("Add to existing") }, { value: "replace", label: t("Replace existing outputs") }]} />
        <OutputEditor value={cur} onChange={setEdited} />
      </>}
    </Modal>
  );
}

export function NodesPanel() {
  const [pick, setPick] = useState<ArtNode | null>(null);
  useEffect(() => {
    fire({ type: "nodes_scan" });
    const h = setInterval(() => fire({ type: "nodes_scan" }), 5000);
    return () => clearInterval(h);
  }, []);
  const used = usedIps();
  const list = useMemo(() => [...nodes.value].sort((a, b) => a.ip.localeCompare(b.ip, undefined, { numeric: true })), [nodes.value]);
  const unused = list.filter((x) => !used.has(x.ip)).length;
  return (
    <div class="panel">
      <div class="panel-head row" style={{ gap: 8 }}>
        <span class="grow">{t("Art-Net nodes")}</span>
        {list.length > 0 && <span class={`chip ${unused ? "warn" : "ok"}`}>{unused ? tp(unused, "{n} unused", "{n} unused") : t("all in use")}</span>}
        <button class="btn tall" style={{ minHeight: 44 }} onClick={() => { fire({ type: "nodes_scan" }); toast(t("Scanning the network…"), "info", 1200); }}>{t("Scan")}</button>
      </div>
      <div class="panel-body col" style={{ gap: 6 }}>
        {list.length === 0 && <div class="dim">{t("No Art-Net nodes found yet. Nodes answer an ArtPoll within a few seconds; make sure they are on the same network and subnet.")}</div>}
        {list.map((n) => {
          const inUse = used.has(n.ip);
          return (
            <button key={n.ip} class="btn" onClick={() => setPick(n)} title={t("Assign a universe output to this node")}
              style={{ minHeight: 52, justifyContent: "flex-start", textAlign: "left", gap: 12, flexWrap: "wrap", borderColor: inUse ? undefined : "var(--live)", background: inUse ? undefined : "var(--live-dim)" }}>
              <span class="mono" style={{ width: 120 }}>{n.ip}</span>
              <span class="grow" style={{ minWidth: 140, overflow: "hidden", textOverflow: "ellipsis" }}><b>{n.name || "—"}</b>{n.long_name && n.long_name !== n.name && <span class="dim"> {n.long_name}</span>}</span>
              <span class="mono dim">{n.universes.length ? n.universes.map(pa).join("  ") : "—"}</span>
              <span class="faint" style={{ width: 70, textAlign: "right" }}>{n.last_seen_ms < 1000 ? "<1 s" : `${Math.round(n.last_seen_ms / 1000)} s`}</span>
              <span class={`chip ${inUse ? "ok" : "warn"}`}>{inUse ? t("in use") : t("unused")}</span>
            </button>
          );
        })}
      </div>
      {pick && <AssignDialog node={pick} onClose={() => setPick(null)} />}
    </div>
  );
}
