import { useEffect, useState } from "preact/hooks";
import type { Action, Binding, MidiKind, Source } from "../../types";
import { onLearned, send, show } from "../../store";
import { EmptyState, Modal, NumberField, PageHead, Segmented, Select, TextField, Toggle, confirmDialog } from "../../ui";
import { ATTRS } from "../../lib/shell-attrs";
import { midiPorts } from "../../midi";
import { t } from "../../i18n";

export function sourceText(s: Source): string {
  if (s.type === "osc") return `OSC ${s.address}`;
  const k = { note: t("Note"), cc: "CC", program: t("Program") }[s.kind];
  const ch = s.channel ? t("· ch{n}", { n: s.channel }) : t("· any ch");
  return `MIDI ${k} ${s.number} ${ch}${s.port ? ` · ${s.port}` : ""}`;
}
export function actionText(a: Action): string {
  const sh = show.value;
  const cl = (id: number) => sh?.cuelists.find((c) => c.id === id)?.name ?? t("cuelist {id}", { id });
  switch (a.type) {
    case "exec_level": return t("Executor {exec} level", { exec: a.exec });
    case "exec_go": return t("Executor {exec} GO", { exec: a.exec });
    case "exec_back": return t("Executor {exec} BACK", { exec: a.exec });
    case "exec_flash": return t("Executor {exec} FLASH", { exec: a.exec });
    case "go": return t("GO {name}", { name: cl(a.cuelist) });
    case "back": return t("BACK {name}", { name: cl(a.cuelist) });
    case "grand_master": return t("Grand master");
    case "blackout_toggle": return t("Blackout toggle");
    case "dmx": return `DMX U${a.universe}/${a.channel}`;
    case "fixture_attr": return t("Fixture {fixture} {attr}", { fixture: a.fixture, attr: a.attr });
    case "group_attr": return t("Group {group} {attr}", { group: a.group, attr: a.attr });
    case "effect_toggle": return t("Effect {effect} toggle", { effect: a.effect });
    case "palette_apply": return a.group ? t("Palette {palette} → group {group}", { palette: a.palette, group: a.group }) : t("Palette {palette}", { palette: a.palette });
    case "prog_clear": return t("Clear programmer");
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

function defaultAction(ty: AT): Action {
  const sh = show.value;
  const ex = sh?.executors[0]?.id ?? 1, cl = sh?.cuelists[0]?.id ?? 1;
  switch (ty) {
    case "exec_level": case "exec_go": case "exec_back": case "exec_flash": return { type: ty, exec: ex };
    case "go": case "back": return { type: ty, cuelist: cl };
    case "grand_master": case "blackout_toggle": case "prog_clear": return { type: ty };
    case "dmx": return { type: "dmx", universe: sh?.universes[0]?.id ?? 1, channel: 1 };
    case "fixture_attr": return { type: ty, fixture: sh?.fixtures[0]?.id ?? 1, attr: "dimmer" };
    case "group_attr": return { type: ty, group: sh?.groups[0]?.id ?? 1, attr: "dimmer" };
    case "effect_toggle": return { type: ty, effect: sh?.effects[0]?.id ?? 1 };
    case "palette_apply": return { type: ty, palette: sh?.palettes[0]?.id ?? 1, group: null };
  }
}

function pickOpts<T extends { id: number }>(items: T[] | undefined, name: (x: T) => string) {
  const o = (items ?? []).map((x) => ({ value: x.id, label: `${x.id} · ${name(x)}` }));
  return o.length ? o : [{ value: 0, label: t("(none defined yet)"), disabled: true }];
}

function ActionFields(props: { a: Action; onChange: (a: Action) => void }) {
  const a = props.a, sh = show.value;
  const attrOpts = ATTRS.map((x) => ({ value: x.name, label: x.label }));
  switch (a.type) {
    case "exec_level": case "exec_go": case "exec_back": case "exec_flash":
      return <Select label={t("Executor")} value={a.exec} options={pickOpts(sh?.executors, (e) => e.label || t("page {page} slot {slot}", { page: e.page, slot: e.slot + 1 }))} onChange={(exec) => props.onChange({ ...a, exec })} />;
    case "go": case "back":
      return <Select label={t("Cuelist")} value={a.cuelist} options={pickOpts(sh?.cuelists, (c) => c.name)} onChange={(cuelist) => props.onChange({ ...a, cuelist })} />;
    case "dmx":
      return <div class="grid2"><Select label={t("Universe")} value={a.universe} options={(sh?.universes ?? []).map((u) => ({ value: u.id, label: `${u.id} · ${u.name}` }))} onChange={(universe) => props.onChange({ ...a, universe })} />
        <NumberField label={t("Channel")} value={a.channel} min={1} max={512} onChange={(channel) => props.onChange({ ...a, channel })} /></div>;
    case "fixture_attr":
      return <div class="grid2"><Select label={t("Fixture")} value={a.fixture} options={pickOpts(sh?.fixtures, (f) => f.name)} onChange={(fixture) => props.onChange({ ...a, fixture })} />
        <Select label={t("Attribute")} value={a.attr} options={attrOpts} onChange={(attr) => props.onChange({ ...a, attr })} /></div>;
    case "group_attr":
      return <div class="grid2"><Select label={t("Group")} value={a.group} options={pickOpts(sh?.groups, (g) => g.name)} onChange={(group) => props.onChange({ ...a, group })} />
        <Select label={t("Attribute")} value={a.attr} options={attrOpts} onChange={(attr) => props.onChange({ ...a, attr })} /></div>;
    case "effect_toggle":
      return <Select label={t("Effect")} value={a.effect} options={pickOpts(sh?.effects, (e) => e.name)} onChange={(effect) => props.onChange({ ...a, effect })} />;
    case "palette_apply":
      return <div class="grid2"><Select label={t("Palette")} value={a.palette} options={pickOpts(sh?.palettes, (p) => p.name)} onChange={(palette) => props.onChange({ ...a, palette })} />
        <Select label={t("Only group")} value={a.group ?? 0} options={[{ value: 0, label: t("Current selection / all") }, ...pickOpts(sh?.groups, (g) => g.name)]} onChange={(g) => props.onChange({ ...a, group: g || null })} /></div>;
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
        <Segmented value={s.type} onChange={(ty) => props.onChange(ty === "osc" ? { type: "osc", address: "/lumos/fader/1" } : { type: "midi", port: null, kind: "cc", channel: null, number: 1 })}
          options={[{ value: "midi", label: "MIDI" }, { value: "osc", label: "OSC" }]} />
        <span class="sp" />
        <button class={`btn tall ${learning ? "on" : ""}`} onClick={() => setLearning(!learning)}>{learning ? t("Listening… move a control (click to cancel)") : t("● Learn")}</button>
      </div>
      {s.type === "midi" ? (
        <div class="grid2">
          <Select<MidiKind> label={t("Message")} value={s.kind} options={[{ value: "cc", label: t("Control change (CC)") }, { value: "note", label: t("Note") }, { value: "program", label: t("Program change") }]} onChange={(kind) => props.onChange({ ...s, kind })} />
          <NumberField label={t("Number (0–127)")} value={s.number} min={0} max={127} onChange={(number) => props.onChange({ ...s, number })} />
          <NumberField label={t("Channel (0 = any)")} value={s.channel ?? 0} min={0} max={16} onChange={(c) => props.onChange({ ...s, channel: c || null })} />
          <div class="field"><label>{t("Port")}</label>
            <input class="input" list="midi-port-list" value={s.port ?? ""} placeholder={t("any port")} onInput={(e) => props.onChange({ ...s, port: (e.target as HTMLInputElement).value || null })} />
            <datalist id="midi-port-list">{midiPorts.value.map((p) => <option key={p.id} value={p.name} />)}</datalist></div>
        </div>
      ) : (
        <TextField label={t("OSC address")} mono value={s.address} placeholder="/lumos/fader/1" onChange={(address) => props.onChange({ ...s, address })} />
      )}
    </div>
  );
}

function BindingDialog(props: { b: Binding; onClose: () => void }) {
  const [b, setB] = useState<Binding>(structuredClone(props.b));
  const cont = ACTIONS.find((x) => x.value === b.action.type)?.cont ?? false;
  const invalid = b.source.type === "osc" && !b.source.address.startsWith("/");
  return (
    <Modal title={props.b.id ? t("Binding {id}", { id: b.id }) : t("New binding")} width={620} onClose={props.onClose}
      footer={<><button class="btn" onClick={props.onClose}>{t("Cancel")}</button>
        <button class="btn primary" disabled={invalid} onClick={() => send({ type: "binding_save", binding: b }).then(props.onClose).catch(() => {})}>{t("Save")}</button></>}>
      <TextField label={t("Label")} value={b.label} onChange={(label) => setB({ ...b, label })} />
      <div class="label">{t("Source")}</div>
      <SourceFields s={b.source} onChange={(source) => setB({ ...b, source })} />
      {invalid && <div style={{ color: "var(--stop)" }}>{t("OSC addresses must start with “/”.")}</div>}
      <div class="label">{t("Action")}</div>
      <Select<AT> label={t("Do")} value={b.action.type} options={ACTIONS.map((x) => ({ ...x, label: t(x.label) }))} onChange={(ty) => setB({ ...b, action: defaultAction(ty) })} />
      <ActionFields a={b.action} onChange={(action) => setB({ ...b, action })} />
      {cont && <div class="grid2">
        <NumberField float label={t("Output at input min")} value={b.out_min} min={0} max={1} step={0.05} onChange={(out_min) => setB({ ...b, out_min })} />
        <NumberField float label={t("Output at input max")} value={b.out_max} min={0} max={1} step={0.05} onChange={(out_max) => setB({ ...b, out_max })} />
      </div>}
      <Toggle checked={b.enabled} onChange={(enabled) => setB({ ...b, enabled })} label={t("Enabled")} />
    </Modal>
  );
}

const TEMPLATES: { label: string; make: () => Pick<Binding, "label" | "source" | "action"> }[] = [
  { label: "Fader → executor level", make: () => ({ label: t("Fader → executor"), source: { type: "midi", port: null, kind: "cc", channel: null, number: 1 }, action: defaultAction("exec_level") }) },
  { label: "Button → GO cuelist", make: () => ({ label: t("Button → GO"), source: { type: "midi", port: null, kind: "note", channel: null, number: 36 }, action: defaultAction("go") }) },
  { label: "Fader → grand master", make: () => ({ label: t("Grand master"), source: { type: "midi", port: null, kind: "cc", channel: null, number: 7 }, action: { type: "grand_master" } }) },
  { label: "Button → blackout", make: () => ({ label: t("Blackout"), source: { type: "midi", port: null, kind: "note", channel: null, number: 40 }, action: { type: "blackout_toggle" } }) },
  { label: "OSC fader → executor", make: () => ({ label: t("OSC fader"), source: { type: "osc", address: "/lumos/exec/1/level" }, action: defaultAction("exec_level") }) },
];

export function BindingsPage() {
  const list = show.value?.bindings ?? [];
  const [edit, setEdit] = useState<Binding | null>(null);
  const nextId = () => list.reduce((m, b) => Math.max(m, b.id), 0) + 1;
  const fresh = (t?: (typeof TEMPLATES)[number]): Binding => ({ id: nextId(), enabled: true, out_min: 0, out_max: 1, ...(t ? t.make() : { label: "", source: { type: "midi", port: null, kind: "cc", channel: null, number: 1 }, action: defaultAction("exec_level") }) });
  return (
    <div class="page">
      <PageHead title={t("Controls")} sub={t("Map MIDI and OSC inputs to executors, cuelists, masters and more.")}>
        <button class="btn primary tall" onClick={() => setEdit(fresh())}>{t("+ New binding")}</button>
      </PageHead>
      <div class="page-body col" style={{ gap: 16 }}>
        <div class="row wrap"><span class="label">{t("Quick start")}</span>{TEMPLATES.map((tpl) => <button key={tpl.label} class="btn" onClick={() => setEdit(fresh(tpl))}>{t(tpl.label)}</button>)}</div>
        {list.length === 0 ? <EmptyState title={t("No bindings yet")} hint={t("Use Learn: open a binding, press Learn, then move the control on your surface.")} icon="🎛" /> : (
          <table class="tbl"><thead><tr><th style={{ width: 60 }}>{t("On")}</th><th>{t("Label")}</th><th>{t("Source")}</th><th>{t("Action")}</th><th style={{ width: 130 }} /></tr></thead>
            <tbody>{list.map((b) => (
              <tr key={b.id}>
                <td><Toggle checked={b.enabled} onChange={(enabled) => send({ type: "binding_save", binding: { ...b, enabled } })} /></td>
                <td>{b.label || <span class="faint">—</span>}</td>
                <td class="mono">{sourceText(b.source)}</td>
                <td>{actionText(b.action)}</td>
                <td><div class="row"><button class="btn sm" onClick={() => setEdit(b)}>{t("Edit")}</button>
                  <button class="btn sm danger" onClick={async () => (await confirmDialog(t("Delete binding"), t("Delete “{name}”?", { name: b.label || b.id }), { danger: true, ok: t("Delete") })) && send({ type: "binding_remove", id: b.id })}>{t("Delete")}</button></div></td>
              </tr>))}</tbody></table>
        )}
      </div>
      {edit && <BindingDialog b={edit} onClose={() => setEdit(null)} />}
    </div>
  );
}
