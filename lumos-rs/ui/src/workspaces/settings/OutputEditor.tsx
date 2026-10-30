import { useEffect, useState } from "preact/hooks";
import type { OutputCfg, OutputKind, OscMode, SerialDriver } from "../../types";
import { Field, NumberField, Select, TextField, Toggle } from "../../ui";
import { fetchPorts, type PortInfo } from "../../lib/api";
import { t } from "../../i18n";
import { tErr } from "../../i18n/errors";

export type KindName = OutputKind["type"];
export const KIND_LABEL: Record<KindName, string> = { art_net: "Art-Net", sacn: "sACN (E1.31)", osc: "OSC", serial: "USB / Serial DMX", usb: "USB (raw)" };
export const KINDS = (Object.keys(KIND_LABEL) as KindName[]).map((value) => ({ value, label: KIND_LABEL[value] }));

export function defaultKind(ty: KindName): OutputKind {
  switch (ty) {
    case "art_net": return { type: "art_net", host: "", port: 6454, net: 0, subnet: 0, universe: 0 };
    case "sacn": return { type: "sacn", universe: 1, priority: 100, unicast: null };
    case "osc": return { type: "osc", host: "127.0.0.1", port: 8000, pattern: "/dmx/{universe}/{channel}", mode: "per_channel", only_nonzero: true };
    case "serial": return { type: "serial", driver: "enttec_pro", port: "" };
    case "usb": return { type: "usb", driver: "udmx", serial: null };
  }
}
export const defaultOutput = (ty: KindName = "art_net"): OutputCfg => ({ enabled: true, kind: defaultKind(ty) });

export function summarize(o: OutputCfg): string {
  const k = o.kind;
  switch (k.type) {
    case "art_net": return `Art-Net ${k.host || t("broadcast")} · ${k.net}:${k.subnet}:${k.universe}`;
    case "sacn": return `sACN u${k.universe}${k.unicast ? ` → ${k.unicast}` : ` ${t("multicast")}`} · p${k.priority}`;
    case "osc": return `OSC ${k.host}:${k.port} ${k.mode}`;
    case "serial": return `${k.driver === "enttec_pro" ? "Enttec Pro" : "Open DMX"} ${k.port || t("(no port)")}`;
    case "usb": return `USB ${k.driver}${k.serial ? ` #${k.serial}` : ""}`;
  }
}

/** Advance an output's addressing by `n` universes (Art-Net port-address / sACN universe). */
export function advance(k: OutputKind, n: number): OutputKind {
  if (k.type === "art_net") {
    const pa = ((k.net << 8) | (k.subnet << 4) | k.universe) + n;
    return { ...k, net: (pa >> 8) & 0x7f, subnet: (pa >> 4) & 0xf, universe: pa & 0xf };
  }
  if (k.type === "sacn") return { ...k, universe: Math.min(63999, k.universe + n) };
  return k;
}

let portCache: PortInfo[] | null = null;

export function KindFields(props: { kind: OutputKind; onChange: (k: OutputKind) => void }) {
  const k = props.kind;
  const [ports, setPorts] = useState<PortInfo[]>(portCache ?? []);
  const [portErr, setPortErr] = useState("");
  useEffect(() => {
    if (k.type !== "serial" || portCache) return;
    fetchPorts().then((p) => { portCache = p; setPorts(p); }).catch((e) => setPortErr(tErr(String(e.message ?? e))));
  }, [k.type]);
  const set = (p: Partial<OutputKind>) => props.onChange({ ...k, ...p } as OutputKind);
  switch (k.type) {
    case "art_net":
      return (
        <div class="col" style={{ gap: 14 }}>
          <div>
            <label class="lbl-cap" for="artnet-host">{t("Host / IP (empty = broadcast)")}</label>
            <input id="artnet-host" class="bigfield" inputMode="decimal" value={k.host} placeholder={t("2.255.255.255 or 192.168.1.50")} aria-label={t("Host (empty = broadcast)")}
              onInput={(e) => set({ host: (e.target as HTMLInputElement).value })} />
          </div>
          <div class="numstep3">
            <NumberField stepper label={t("Net")} value={k.net} min={0} max={127} onChange={(net) => set({ net })} />
            <NumberField stepper label={t("Sub-net")} value={k.subnet} min={0} max={15} onChange={(subnet) => set({ subnet })} />
            <NumberField stepper label={t("Art-Net universe")} value={k.universe} min={0} max={15} onChange={(universe) => set({ universe })} />
          </div>
          <NumberField label={t("UDP port")} value={k.port} min={1} max={65535} onChange={(port) => set({ port })} />
        </div>
      );
    case "sacn":
      return (
        <div class="col" style={{ gap: 14 }}>
          <TextField label={t("Unicast IP (empty = multicast)")} value={k.unicast ?? ""} mono placeholder={t("239.255.x.x multicast")} onChange={(v) => set({ unicast: v.trim() ? v : null })} />
          <div class="numstep3" style={{ gridTemplateColumns: "repeat(2, minmax(0, 1fr))" }}>
            <NumberField stepper label={t("sACN universe (1–63999)")} value={k.universe} min={1} max={63999} onChange={(universe) => set({ universe })} />
            <NumberField stepper label={t("Priority (0–200)")} value={k.priority} min={0} max={200} step={10} onChange={(priority) => set({ priority })} />
          </div>
        </div>
      );
    case "osc":
      return (
        <div class="grid2">
          <TextField label={t("Host")} value={k.host} mono onChange={(host) => set({ host })} />
          <NumberField label={t("Port")} value={k.port} min={1} max={65535} onChange={(port) => set({ port })} />
          <TextField label={t("Address pattern")} value={k.pattern} mono hint={t("{universe} and {channel} are substituted")} onChange={(pattern) => set({ pattern })} />
          <Select<OscMode> label={t("Mode")} value={k.mode} onChange={(mode) => set({ mode })}
            options={[{ value: "per_channel", label: t("One message per channel") }, { value: "blob", label: t("One blob per universe") }, { value: "both", label: t("Both") }]} />
          <Field label={t("Filter")}><Toggle checked={k.only_nonzero} label={t("Only non-zero channels")} onChange={(only_nonzero) => set({ only_nonzero })} /></Field>
        </div>
      );
    case "serial":
      return (
        <div class="grid2">
          <Select<SerialDriver> label={t("Driver")} value={k.driver} onChange={(driver) => set({ driver })}
            options={[{ value: "enttec_pro", label: t("Enttec DMX USB Pro / DMXking ultraDMX") }, { value: "enttec_open", label: t("Enttec Open DMX / FTDI raw") }]} />
          <Field label={t("Serial port")} hint={portErr || (ports.length ? t("Detected ports are suggested; you can type any path.") : t("No ports detected — type the path (e.g. /dev/ttyUSB0, COM3)."))}>
            <input class="input mono" list="serial-ports" value={k.port} placeholder="/dev/ttyUSB0" onInput={(e) => set({ port: (e.target as HTMLInputElement).value })} />
            <datalist id="serial-ports">{ports.map((p) => <option key={p.name} value={p.name}>{p.desc}</option>)}</datalist>
          </Field>
        </div>
      );
    case "usb":
      return (
        <div class="grid2">
          <TextField label={t("Driver")} value={k.driver} mono hint={t("e.g. udmx")} onChange={(driver) => set({ driver })} />
          <TextField label={t("Device serial (optional)")} value={k.serial ?? ""} mono onChange={(v) => set({ serial: v.trim() ? v : null })} />
        </div>
      );
  }
}

/** 2-column protocol picker (56 px buttons). */
export function ProtoGrid(props: { value: KindName; onChange: (k: KindName) => void }) {
  return (
    <div class="proto-grid" role="radiogroup" aria-label={t("Protocol")}>
      {KINDS.map((x) => (
        <button key={x.value} role="radio" aria-checked={props.value === x.value} class={props.value === x.value ? "on" : ""} onClick={() => props.value !== x.value && props.onChange(x.value)}>{t(x.label)}</button>
      ))}
    </div>
  );
}

export function OutputEditor(props: { value: OutputCfg; onChange: (o: OutputCfg) => void; onRemove?: () => void; status?: { state: string; message: string; frames: number } }) {
  const o = props.value;
  return (
    <div class="panel">
      <div class="panel-head">
        <Toggle checked={o.enabled} onChange={(enabled) => props.onChange({ ...o, enabled })} label={t("Enabled")} />
        <span class="sp" />
        {props.status && <span class={`chip ${props.status.state === "ok" ? "ok" : props.status.state === "error" ? "err" : ""}`} title={tErr(props.status.message)}>{props.status.state === "ok" ? t("{frames} frames", { frames: props.status.frames }) : tErr(props.status.message) || ({ ok: t("OK"), error: t("Error"), off: t("Off") } as Record<string, string>)[props.status.state] || props.status.state}</span>}
        {props.onRemove && <button class="btn danger" onClick={props.onRemove} aria-label={t("Remove output")}>{t("Remove")}</button>}
      </div>
      <div class="panel-body col" style={{ gap: 12 }}>
        <ProtoGrid value={o.kind.type} onChange={(ty) => props.onChange({ ...o, kind: defaultKind(ty) })} />
        <KindFields kind={o.kind} onChange={(kind) => props.onChange({ ...o, kind })} />
      </div>
    </div>
  );
}
