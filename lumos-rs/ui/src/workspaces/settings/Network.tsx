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
    <section class="uni-card node-card">
      <div class="hd">
        <b>{t("Art-Net nodes")}</b>
        {list.length > 0 && <span class="cnt">{t("{n} detected", { n: list.length })}</span>}
        {list.length > 0 && <span class={`chip ${unused ? "warn" : "ok"}`}>{unused ? tp(unused, "{n} unused", "{n} unused") : t("all in use")}</span>}
        <span class="sp" />
        <button class="btn" onClick={() => { fire({ type: "nodes_scan" }); toast(t("Scanning the network…"), "info", 1200); }}>{t("Scan")}</button>
      </div>
      <div class="list">
        {list.length === 0 && <div class="node-empty">{t("No Art-Net nodes found yet. Nodes answer an ArtPoll within a few seconds; make sure they are on the same network and subnet.")}</div>}
        {list.map((n) => {
          const inUse = used.has(n.ip);
          return (
            <div key={n.ip} class="node-row">
              <span class="nm">{n.name || "—"}{n.long_name && n.long_name !== n.name && <small>{n.long_name}</small>}</span>
              <span class="ip">{n.ip}</span>
              <span class="un">{n.universes.length ? n.universes.map(pa).join("  ") : "—"} · {n.last_seen_ms < 1000 ? "<1 s" : `${Math.round(n.last_seen_ms / 1000)} s`}</span>
              <button class={`btn ${inUse ? "" : "free"}`} onClick={() => setPick(n)} title={t("Assign a universe output to this node")}>{inUse ? t("In use") : t("Assign to a universe")}</button>
            </div>
          );
        })}
      </div>
      {pick && <AssignDialog node={pick} onClose={() => setPick(null)} />}
    </section>
  );
}
