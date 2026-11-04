import { useState } from "preact/hooks";
import { send, show, toast, midiPorts as nativeMidi } from "../store";
import { Select, Segmented, TextField, NumberField } from "../ui";
import { t } from "../i18n";
import type { MidiKind, Script, ScriptAction } from "../types";
import { SCRIPT_TEST_ID, actionSummary } from "./util";

/** Fire one action in isolation: a throw-away script (never shown) holds a single mark with it. */
export async function testAction(a: ScriptAction) {
  const tmp: Script = {
    id: SCRIPT_TEST_ID, name: "__test", pdf: null,
    blocks: [{ id: 1, kind: "direction", speaker: "", text: "test", page: 0, marks: [{ id: 1, label: "test", kind: "other", at_word: 0, actions: [a], link: null, standby_blocks: 0, note: "" }] }],
  };
  try {
    await send({ type: "script_save", script: tmp });
    await send({ type: "script_fire", id: SCRIPT_TEST_ID, block: 1, mark: 1 });
    toast(t("Action sent: {what}", { what: actionSummary(a) }), "ok", 2200);
  } catch { /* toasted */ }
  setTimeout(() => void send({ type: "script_remove", id: SCRIPT_TEST_ID }, { quiet: true }).catch(() => {}), 1500);
}

type Preset = { id: string; group: string; label: string; make: () => ScriptAction };
const osc = (label: string, host: string, port: number, address: string, args: string[] = []): Preset["make"] => () => ({ type: "osc", host, port, address, args });
export const PRESETS: Preset[] = [
  { id: "go", group: "Lumos", label: "Lumos: GO cuelist", make: () => ({ type: "console", text: `go cuelist ${show.value?.cuelists[0]?.id ?? 1}` }) },
  { id: "scene", group: "Lumos", label: "Lumos: scene on", make: () => ({ type: "console", text: `scene ${show.value?.scenes?.[0]?.id ?? 1} on` }) },
  { id: "macro", group: "Lumos", label: "Lumos: run macro", make: () => ({ type: "console", text: `macro ${show.value?.macros?.[0]?.id ?? 1}` }) },
  { id: "bo", group: "Lumos", label: "Lumos: blackout", make: () => ({ type: "console", text: "blackout on" }) },
  { id: "lua", group: "Lumos", label: "Lumos: Lua script", make: () => ({ type: "lua", id: show.value?.lua?.[0]?.id ?? 0 }) },
  { id: "console", group: "Lumos", label: "Lumos: custom command", make: () => ({ type: "console", text: "" }) },
  { id: "qlab", group: "QLab", label: "QLab: start cue", make: osc("QLab", "127.0.0.1", 53000, "/cue/1/start") },
  { id: "qlab-go", group: "QLab", label: "QLab: GO", make: osc("QLab", "127.0.0.1", 53000, "/go") },
  { id: "qlab-stop", group: "QLab", label: "QLab: stop all", make: osc("QLab", "127.0.0.1", 53000, "/panic") },
  { id: "resolume", group: "Video", label: "Resolume: trigger clip", make: osc("Resolume", "127.0.0.1", 7000, "/composition/layers/1/clips/1/connect", ["1"]) },
  { id: "pp", group: "Video", label: "ProPresenter: next", make: () => ({ type: "http", method: "GET", url: "http://127.0.0.1:1025/v1/trigger/next", body: "" }) },
  { id: "vlc", group: "Video", label: "VLC: play", make: () => ({ type: "http", method: "GET", url: "http://127.0.0.1:8080/requests/status.xml?command=pl_play", body: "" }) },
  { id: "osc", group: "Other", label: "OSC: custom", make: osc("OSC", "127.0.0.1", 8000, "/cue/1", []) },
  { id: "midi-note", group: "Other", label: "MIDI: note", make: () => ({ type: "midi", port: null, kind: "note", channel: 1, number: 60, value: 127 }) },
  { id: "midi-cc", group: "Other", label: "MIDI: CC", make: () => ({ type: "midi", port: null, kind: "cc", channel: 1, number: 1, value: 127 }) },
  { id: "midi-pc", group: "Other", label: "MIDI: program change", make: () => ({ type: "midi", port: null, kind: "program", channel: 1, number: 1, value: 0 }) },
  { id: "http", group: "Other", label: "HTTP / webhook", make: () => ({ type: "http", method: "POST", url: "http://127.0.0.1:4455/", body: "" }) },
];
const GROUPS = ["Lumos", "QLab", "Video", "Other"];
const groupLabel = (g: string) => ({ Lumos: "Lumos", QLab: "QLab", Video: t("Video / media"), Other: t("Other") })[g] ?? g;

export function PresetPicker(props: { onPick: (a: ScriptAction) => void; onClose: () => void }) {
  return (
    <div class="sc-presets">
      {GROUPS.map((g) => (
        <div key={g}>
          <div class="label">{groupLabel(g)}</div>
          <div class="sc-preset-grid">
            {PRESETS.filter((p) => p.group === g).map((p) => (
              <button key={p.id} class="btn lg" onClick={() => { props.onPick(p.make()); props.onClose(); }}>{t(p.label)}</button>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function Stepper(props: { value: number; min: number; max: number; onChange: (n: number) => void; label?: string }) {
  return (
    <div class="stepper" role="group" aria-label={props.label}>
      <button aria-label="−" disabled={props.value <= props.min} onClick={() => props.onChange(Math.max(props.min, props.value - 1))}>−</button>
      <span class="val mono">{props.value}</span>
      <button aria-label="+" disabled={props.value >= props.max} onClick={() => props.onChange(Math.min(props.max, props.value + 1))}>+</button>
    </div>
  );
}
export { Stepper };

export function ActionEditor(props: { a: ScriptAction; onChange: (a: ScriptAction) => void; onRemove: () => void }) {
  const { a } = props;
  const [open, setOpen] = useState(true);
  const outs = nativeMidi.value?.outputs ?? [];
  const title = { console: t("Lumos command"), lua: t("Lua script"), osc: "OSC", midi: "MIDI", http: "HTTP" }[a.type];
  return (
    <div class="sc-action">
      <div class="sc-action-head">
        <button class="sc-action-title" onClick={() => setOpen(!open)} aria-expanded={open}>
          <span class="chip">{title}</span>
          <span class="sc-action-sum">{actionSummary(a)}</span>
        </button>
        <button class="btn sm" onClick={() => void testAction(a)}>{t("Test")}</button>
        <button class="btn sm danger" aria-label={t("Remove action")} onClick={props.onRemove}>✕</button>
      </div>
      {open && (
        <div class="sc-action-body">
          {a.type === "console" && (
            <TextField label={t("Command line")} mono value={a.text} placeholder="go cuelist 1; scene 3 on" onChange={(text) => props.onChange({ ...a, text })} />
          )}
          {a.type === "lua" && (
            <Select<number> label={t("Lua script")} value={a.id} options={[{ value: 0, label: t("— choose a script —") }, ...(show.value?.lua ?? []).map((x) => ({ value: x.id, label: x.name }))]} onChange={(id) => props.onChange({ ...a, id })} />
          )}
          {a.type === "osc" && (
            <>
              <div class="grid2">
                <TextField label={t("Host")} mono value={a.host} onChange={(host) => props.onChange({ ...a, host })} />
                <NumberField label={t("Port")} value={a.port} min={1} max={65535} onChange={(port) => props.onChange({ ...a, port })} />
              </div>
              <TextField label={t("OSC address")} mono value={a.address} placeholder="/cue/1/start" onChange={(address) => props.onChange({ ...a, address })} />
              <TextField label={t("Arguments (space separated)")} mono value={a.args.join(" ")} placeholder="1 0.5 true text"
                onChange={(v) => props.onChange({ ...a, args: v.split(/\s+/).filter(Boolean) })} />
            </>
          )}
          {a.type === "midi" && (
            <>
              <Segmented<MidiKind> value={a.kind} onChange={(kind) => props.onChange({ ...a, kind })}
                options={[{ value: "note", label: t("Note") }, { value: "cc", label: "CC" }, { value: "program", label: t("Program") }]} />
              <div class="grid2">
                <Select<number> label={t("Channel")} value={a.channel} options={Array.from({ length: 16 }, (_, i) => ({ value: i + 1, label: String(i + 1) }))} onChange={(channel) => props.onChange({ ...a, channel })} />
                <NumberField label={t("Number (0–127)")} value={a.number} min={0} max={127} onChange={(number) => props.onChange({ ...a, number })} />
                <NumberField label={t("Value (0–127)")} value={a.value} min={0} max={127} onChange={(value) => props.onChange({ ...a, value })} />
                <Select<string> label={t("Output port")} value={a.port ?? ""} options={[{ value: "", label: t("All enabled outputs") }, ...outs.map((o) => ({ value: o.name, label: o.name }))]}
                  onChange={(p) => props.onChange({ ...a, port: p || null })} />
              </div>
            </>
          )}
          {a.type === "http" && (
            <>
              <div class="grid2">
                <Select<string> label={t("Method")} value={a.method} options={["GET", "POST", "PUT"].map((m) => ({ value: m, label: m }))} onChange={(method) => props.onChange({ ...a, method })} />
                <TextField label="URL" mono value={a.url} placeholder="http://127.0.0.1:8080/…" onChange={(url) => props.onChange({ ...a, url })} />
              </div>
              {a.method !== "GET" && (
                <div class="field"><label>{t("Body")}</label>
                  <textarea class="input mono" rows={3} value={a.body} onInput={(e) => props.onChange({ ...a, body: (e.target as HTMLTextAreaElement).value })} /></div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
