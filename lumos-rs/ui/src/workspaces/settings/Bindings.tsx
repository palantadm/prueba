import { useEffect, useState } from "preact/hooks";
import type { Action, Binding, MidiKind, Source } from "../../types";
import { onLearned, send, show } from "../../store";
import { EmptyState, Modal, NumberField, PageHead, Segmented, Select, TextField, Toggle, confirmDialog } from "../../ui";
import { ATTRS } from "../../lib/shell-attrs";
import { midiPorts } from "../../midi";

export function sourceText(s: Source): string {
  if (s.type === "osc") return `OSC ${s.address}`;
  const k = { note: "Note", cc: "CC", program: "Program" }[s.kind];
  return `MIDI ${k} ${s.number}${s.channel ? ` · ch${s.channel}` : " · any ch"}${s.port ? ` · ${s.port}` : ""}`;
}
export function actionText(a: Action): string {
  const sh = show.value;
  switch (a.type) {
    case "exec_level": return `Executor ${a.exec} level`;
    case "exec_go": return `Executor ${a.exec} GO`;
    case "exec_back": return `Executor ${a.exec} BACK`;
    case "exec_flash": return `Executor ${a.exec} FLASH`;
    case "go": return `GO ${sh?.cuelists.find((c) => c.id === a.cuelist)?.name ?? `cuelist ${a.cuelist}`}`;
    case "back": return `BACK ${sh?.cuelists.find((c) => c.id === a.cuelist)?.name ?? `cuelist ${a.cuelist}`}`;
    case "grand_master": return "Grand master";
    case "blackout_toggle": return "Blackout toggle";
    case "dmx": return `DMX U${a.universe}/${a.channel}`;
    case "fixture_attr": return `Fixture ${a.fixture} ${a.attr}`;
    case "group_attr": return `Group ${a.group} ${a.attr}`;
    case "effect_toggle": return `Effect ${a.effect} toggle`;
    case "palette_apply": return `Palette ${a.palette}${a.group ? ` → group ${a.group}` : ""}`;
    case "prog_clear": return "Clear programmer";
  }
}

type AT = Action["type"];
const ACTIONS: { value: AT; label: string; cont: boolean }[] = [
  { value: "exec_level", label: "Executor — fader level", cont: true },
  { value: "exec_go", label: "Executor — GO", cont: false },
  { value: "exec_back", label: "Executor — BACK", cont: false },
  { value: "exec_flash", label: "Executor — FLASH", cont: false },
  { value: "go", label: "Cuelist — GO", cont: false },
  { value: "back", label: "Cuelist — BACK", cont: false },
  { value: "grand_master", label: "Grand master", cont: true },
  { value: "blackout_toggle", label: "Blackout toggle", cont: false },
  { value: "dmx", label: "DMX channel (direct)", cont: true },
  { value: "fixture_attr", label: "Fixture attribute", cont: true },
  { value: "group_attr", label: "Group attribute", cont: true },
  { value: "effect_toggle", label: "Effect toggle", cont: false },
  { value: "palette_apply", label: "Apply palette", cont: false },
  { value: "prog_clear", label: "Clear programmer", cont: false },
];

function defaultAction(t: AT): Action {
  const sh = show.value;
  const ex = sh?.executors[0]?.id ?? 1, cl = sh?.cuelists[0]?.id ?? 1;
  switch (t) {
    case "exec_level": case "exec_go": case "exec_back": case "exec_flash": return { type: t, exec: ex };
    case "go": case "back": return { type: t, cuelist: cl };
    case "grand_master": case "blackout_toggle": case "prog_clear": return { type: t };
    case "dmx": return { type: "dmx", universe: sh?.universes[0]?.id ?? 1, channel: 1 };
    case "fixture_attr": return { type: t, fixture: sh?.fixtures[0]?.id ?? 1, attr: "dimmer" };
    case "group_attr": return { type: t, group: sh?.groups[0]?.id ?? 1, attr: "dimmer" };
    case "effect_toggle": return { type: t, effect: sh?.effects[0]?.id ?? 1 };
    case "palette_apply": return { type: t, palette: sh?.palettes[0]?.id ?? 1, group: null };
  }
}

function pickOpts<T extends { id: number }>(items: T[] | undefined, name: (x: T) => string) {
  const o = (items ?? []).map((x) => ({ value: x.id, label: `${x.id} · ${name(x)}` }));
  return o.length ? o : [{ value: 0, label: "(none defined yet)", disabled: true }];
}

function ActionFields(props: { a: Action; onChange: (a: Action) => void }) {
  const a = props.a, sh = show.value;
  const attrOpts = ATTRS.map((x) => ({ value: x.name, label: x.label }));
  switch (a.type) {
    case "exec_level": case "exec_go": case "exec_back": case "exec_flash":
      return <Select label="Executor" value={a.exec} options={pickOpts(sh?.executors, (e) => e.label || `page ${e.page} slot ${e.slot + 1}`)} onChange={(exec) => props.onChange({ ...a, exec })} />;
    case "go": case "back":
      return <Select label="Cuelist" value={a.cuelist} options={pickOpts(sh?.cuelists, (c) => c.name)} onChange={(cuelist) => props.onChange({ ...a, cuelist })} />;
    case "dmx":
      return <div class="grid2"><Select label="Universe" value={a.universe} options={(sh?.universes ?? []).map((u) => ({ value: u.id, label: `${u.id} · ${u.name}` }))} onChange={(universe) => props.onChange({ ...a, universe })} />
        <NumberField label="Channel" value={a.channel} min={1} max={512} onChange={(channel) => props.onChange({ ...a, channel })} /></div>;
    case "fixture_attr":
      return <div class="grid2"><Select label="Fixture" value={a.fixture} options={pickOpts(sh?.fixtures, (f) => f.name)} onChange={(fixture) => props.onChange({ ...a, fixture })} />
        <Select label="Attribute" value={a.attr} options={attrOpts} onChange={(attr) => props.onChange({ ...a, attr })} /></div>;
    case "group_attr":
      return <div class="grid2"><Select label="Group" value={a.group} options={pickOpts(sh?.groups, (g) => g.name)} onChange={(group) => props.onChange({ ...a, group })} />
        <Select label="Attribute" value={a.attr} options={attrOpts} onChange={(attr) => props.onChange({ ...a, attr })} /></div>;
    case "effect_toggle":
      return <Select label="Effect" value={a.effect} options={pickOpts(sh?.effects, (e) => e.name)} onChange={(effect) => props.onChange({ ...a, effect })} />;
    case "palette_apply":
      return <div class="grid2"><Select label="Palette" value={a.palette} options={pickOpts(sh?.palettes, (p) => p.name)} onChange={(palette) => props.onChange({ ...a, palette })} />
        <Select label="Only group" value={a.group ?? 0} options={[{ value: 0, label: "Current selection / all" }, ...pickOpts(sh?.groups, (g) => g.name)]} onChange={(g) => props.onChange({ ...a, group: g || null })} /></div>;
    default: return null;
  }
}

function SourceFields(props: { s: Source; onChange: (s: Source) => void }) {
  const s = props.s;
  const [learning, setLearning] = useState(false);
  useEffect(() => {
    if (!learning) return;
    const off = onLearned((src) => { props.onChange(src); setLearning(false); });
    void send({ type: "learn_start", binding: null }, { quiet: true });
    return () => { off(); void send({ type: "learn_stop" }, { quiet: true }); };
  }, [learning]);
  return (
    <div class="col" style={{ gap: 12 }}>
      <div class="row">
        <Segmented value={s.type} onChange={(t) => props.onChange(t === "osc" ? { type: "osc", address: "/lumos/fader/1" } : { type: "midi", port: null, kind: "cc", channel: null, number: 1 })}
          options={[{ value: "midi", label: "MIDI" }, { value: "osc", label: "OSC" }]} />
        <span class="sp" />
        <button class={`btn tall ${learning ? "on" : ""}`} onClick={() => setLearning(!learning)}>{learning ? "Listening… move a control (click to cancel)" : "● Learn"}</button>
      </div>
      {s.type === "midi" ? (
        <div class="grid2">
          <Select<MidiKind> label="Message" value={s.kind} options={[{ value: "cc", label: "Control change (CC)" }, { value: "note", label: "Note" }, { value: "program", label: "Program change" }]} onChange={(kind) => props.onChange({ ...s, kind })} />
          <NumberField label="Number (0–127)" value={s.number} min={0} max={127} onChange={(number) => props.onChange({ ...s, number })} />
          <NumberField label="Channel (0 = any)" value={s.channel ?? 0} min={0} max={16} onChange={(c) => props.onChange({ ...s, channel: c || null })} />
          <div class="field"><label>Port</label>
            <input class="input" list="midi-port-list" value={s.port ?? ""} placeholder="any port" onInput={(e) => props.onChange({ ...s, port: (e.target as HTMLInputElement).value || null })} />
            <datalist id="midi-port-list">{midiPorts.value.map((p) => <option key={p.id} value={p.name} />)}</datalist></div>
        </div>
      ) : (
        <TextField label="OSC address" mono value={s.address} placeholder="/lumos/fader/1" onChange={(address) => props.onChange({ ...s, address })} />
      )}
    </div>
  );
}

function BindingDialog(props: { b: Binding; onClose: () => void }) {
  const [b, setB] = useState<Binding>(structuredClone(props.b));
  const cont = ACTIONS.find((x) => x.value === b.action.type)?.cont ?? false;
  const invalid = b.source.type === "osc" && !b.source.address.startsWith("/");
  return (
    <Modal title={props.b.id ? `Binding ${b.id}` : "New binding"} width={620} onClose={props.onClose}
      footer={<><button class="btn" onClick={props.onClose}>Cancel</button>
        <button class="btn primary" disabled={invalid} onClick={() => send({ type: "binding_save", binding: b }).then(props.onClose).catch(() => {})}>Save</button></>}>
      <TextField label="Label" value={b.label} onChange={(label) => setB({ ...b, label })} />
      <div class="label">Source</div>
      <SourceFields s={b.source} onChange={(source) => setB({ ...b, source })} />
      {invalid && <div style={{ color: "var(--stop)" }}>OSC addresses must start with “/”.</div>}
      <div class="label">Action</div>
      <Select<AT> label="Do" value={b.action.type} options={ACTIONS} onChange={(t) => setB({ ...b, action: defaultAction(t) })} />
      <ActionFields a={b.action} onChange={(action) => setB({ ...b, action })} />
      {cont && <div class="grid2">
        <NumberField float label="Output at input min" value={b.out_min} min={0} max={1} step={0.05} onChange={(out_min) => setB({ ...b, out_min })} />
        <NumberField float label="Output at input max" value={b.out_max} min={0} max={1} step={0.05} onChange={(out_max) => setB({ ...b, out_max })} />
      </div>}
      <Toggle checked={b.enabled} onChange={(enabled) => setB({ ...b, enabled })} label="Enabled" />
    </Modal>
  );
}

const TEMPLATES: { label: string; make: () => Pick<Binding, "label" | "source" | "action"> }[] = [
  { label: "Fader → executor level", make: () => ({ label: "Fader → executor", source: { type: "midi", port: null, kind: "cc", channel: null, number: 1 }, action: defaultAction("exec_level") }) },
  { label: "Button → GO cuelist", make: () => ({ label: "Button → GO", source: { type: "midi", port: null, kind: "note", channel: null, number: 36 }, action: defaultAction("go") }) },
  { label: "Fader → grand master", make: () => ({ label: "Grand master", source: { type: "midi", port: null, kind: "cc", channel: null, number: 7 }, action: { type: "grand_master" } }) },
  { label: "Button → blackout", make: () => ({ label: "Blackout", source: { type: "midi", port: null, kind: "note", channel: null, number: 40 }, action: { type: "blackout_toggle" } }) },
  { label: "OSC fader → executor", make: () => ({ label: "OSC fader", source: { type: "osc", address: "/lumos/exec/1/level" }, action: defaultAction("exec_level") }) },
];

export function BindingsPage() {
  const list = show.value?.bindings ?? [];
  const [edit, setEdit] = useState<Binding | null>(null);
  const nextId = () => list.reduce((m, b) => Math.max(m, b.id), 0) + 1;
  const fresh = (t?: (typeof TEMPLATES)[number]): Binding => ({ id: nextId(), enabled: true, out_min: 0, out_max: 1, ...(t ? t.make() : { label: "", source: { type: "midi", port: null, kind: "cc", channel: null, number: 1 }, action: defaultAction("exec_level") }) });
  return (
    <div class="page">
      <PageHead title="Controls" sub="Map MIDI and OSC inputs to executors, cuelists, masters and more.">
        <button class="btn primary tall" onClick={() => setEdit(fresh())}>+ New binding</button>
      </PageHead>
      <div class="page-body col" style={{ gap: 16 }}>
        <div class="row wrap"><span class="label">Quick start</span>{TEMPLATES.map((t) => <button key={t.label} class="btn" onClick={() => setEdit(fresh(t))}>{t.label}</button>)}</div>
        {list.length === 0 ? <EmptyState title="No bindings yet" hint="Use Learn: open a binding, press Learn, then move the control on your surface." icon="🎛" /> : (
          <table class="tbl"><thead><tr><th style={{ width: 60 }}>On</th><th>Label</th><th>Source</th><th>Action</th><th style={{ width: 130 }} /></tr></thead>
            <tbody>{list.map((b) => (
              <tr key={b.id}>
                <td><Toggle checked={b.enabled} onChange={(enabled) => send({ type: "binding_save", binding: { ...b, enabled } })} /></td>
                <td>{b.label || <span class="faint">—</span>}</td>
                <td class="mono">{sourceText(b.source)}</td>
                <td>{actionText(b.action)}</td>
                <td><div class="row"><button class="btn sm" onClick={() => setEdit(b)}>Edit</button>
                  <button class="btn sm danger" onClick={async () => (await confirmDialog("Delete binding", `Delete “${b.label || b.id}”?`, { danger: true, ok: "Delete" })) && send({ type: "binding_remove", id: b.id })}>Delete</button></div></td>
              </tr>))}</tbody></table>
        )}
      </div>
      {edit && <BindingDialog b={edit} onClose={() => setEdit(null)} />}
    </div>
  );
}
