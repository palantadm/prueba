// Properties panel for the selected widget(s) / page.
import { useEffect, useRef, useState } from "preact/hooks";
import { onLearned, selection, send, show, toast } from "../store";
import { midiOutputs, midiPorts, sendMidi } from "../midi";
import type { FeedbackTarget, Source, VcAction, VcInput, VcKind, VcMode, VcOp, VcWidget } from "../types";
import { ALL_ATTRS, ATTR_META } from "../lib/attrs";
import { NumberField, Segmented, Select, TextField, Toggle, confirmDialog } from "../ui";
import { Picker, type PickOpt } from "./Picker";
import { ACTION_TYPES, KINDS, autoLabel, isDefaultLabel, MODE_LABEL, OPS, PALETTE_COLORS, actionTypesFor, defaultAction, defaultInput, describeFeedback, describeSource, hasAction, isContinuous } from "./model";
import { FB_PRESETS, type FbPreset } from "./presets";
import { alignSelected, deletePage, patchWidget, patchSelected, renamePage, resizePage, selected, page, deleteSelected, duplicateSelected, type Align } from "./editor";

// ── option lists ────────────────────────────────────────────────────────────
function useLists() {
  const s = show.value;
  const execs: PickOpt[] = (s?.executors ?? []).filter((e) => e.kind.type !== "empty").map((e) => ({ value: e.id, label: e.label || `Exec ${e.slot + 1}`, sub: `page ${e.page + 1} · slot ${e.slot + 1} · ${e.kind.type}` }));
  const cls: PickOpt[] = (s?.cuelists ?? []).map((c) => ({ value: c.id, label: c.name, sub: `${c.cues.length} cues` }));
  const groups: PickOpt[] = (s?.groups ?? []).map((g) => ({ value: g.id, label: g.name, sub: `${g.fixtures.length} fixtures` }));
  const effects: PickOpt[] = (s?.effects ?? []).map((e) => ({ value: e.id, label: e.name, sub: e.attr }));
  const palettes: PickOpt[] = (s?.palettes ?? []).map((p) => ({ value: p.id, label: p.name, sub: p.kind }));
  const pages: PickOpt[] = (s?.vconsole ?? []).map((p) => ({ value: p.id, label: p.name }));
  return { execs, cls, groups, effects, palettes, pages };
}

const Sec = (p: { title: string; children: preact.ComponentChildren; right?: preact.ComponentChildren }) => (
  <section class="vc-sec"><div class="vc-sec-h"><span>{p.title}</span><span class="sp" />{p.right}</div><div class="vc-sec-b">{p.children}</div></section>
);
const Row = (p: { label: string; children: preact.ComponentChildren }) => <div class="vc-row"><label>{p.label}</label><div class="grow">{p.children}</div></div>;

// ── learn ───────────────────────────────────────────────────────────────────
function useLearn(onSource: (s: Source) => void) {
  const [on, setOn] = useState(false);
  const off = useRef<null | (() => void)>(null);
  const cancel = () => { off.current?.(); off.current = null; if (on) void send({ type: "learn_stop" }, { quiet: true }).catch(() => {}); setOn(false); };
  useEffect(() => () => { if (off.current) { off.current(); void send({ type: "learn_stop" }, { quiet: true }).catch(() => {}); } }, []);
  const start = () => {
    if (on) return cancel();
    setOn(true);
    off.current = onLearned((s) => { off.current?.(); off.current = null; setOn(false); onSource(s); });
    void send({ type: "learn_start", binding: null }, { quiet: true }).catch(() => { setOn(false); toast("Learn mode unavailable", "err"); });
  };
  return { on, start };
}

// ── action ──────────────────────────────────────────────────────────────────
function ActionEditor({ w }: { w: VcWidget }) {
  const L = useLists();
  const a = w.action;
  const set = (na: VcAction) => patchWidget(w.id, (x) => {
    const prevAuto = autoLabel(x.action, show.value);
    x.action = na;
    const nl = autoLabel(na, show.value);
    if (nl && (isDefaultLabel(x.label) || x.label === prevAuto)) x.label = nl;
  }, false);
  const types = ACTION_TYPES.filter((t) => actionTypesFor(w.kind).includes(t.type));
  const attrOpts = ALL_ATTRS.map((k) => ({ value: k, label: ATTR_META[k].label }));
  const opOpts = (w.kind === "button" ? OPS.filter((o) => o.value !== "level") : OPS.filter((o) => o.value === "level")).map((o) => ({ value: o.value, label: o.label }));
  return (
    <>
      <Row label="Controls">
        <Select class="bare" value={a.type} options={types.map((t) => ({ value: t.type, label: t.label }))} onChange={(t) => set(defaultAction(t, show.value, w.kind))} />
      </Row>
      {(a.type === "exec") && <>
        <Row label="Executor"><Picker value={a.exec} options={L.execs} onChange={(v) => set({ ...a, exec: v })} /></Row>
        {w.kind !== "cue_list" && <Row label="Operation"><Select class="bare" value={a.op} options={opOpts} onChange={(op) => set({ ...a, op: op as VcOp })} /></Row>}
      </>}
      {(a.type === "cuelist") && <>
        <Row label="Cuelist"><Picker value={a.cuelist} options={L.cls} onChange={(v) => set({ ...a, cuelist: v })} /></Row>
        {w.kind !== "cue_list" && <Row label="Operation"><Select class="bare" value={a.op} options={opOpts} onChange={(op) => set({ ...a, op: op as VcOp })} /></Row>}
      </>}
      {a.type === "dmx" && <>
        <Row label="Universe"><NumberField class="bare" value={a.universe} min={1} max={9999} onChange={(v) => set({ ...a, universe: v })} /></Row>
        <Row label="Channel"><NumberField class="bare" value={a.channel} min={1} max={512} onChange={(v) => set({ ...a, channel: v })} /></Row>
      </>}
      {a.type === "group_attr" && <>
        <Row label="Group"><Picker value={a.group} options={L.groups} onChange={(v) => set({ ...a, group: v })} /></Row>
        <Row label="Attribute"><Select class="bare" value={a.attr} options={attrOpts} onChange={(v) => set({ ...a, attr: v })} /></Row>
      </>}
      {a.type === "fixture_attr" && <>
        <Row label="Fixtures">
          <div class="row">
            <TextField class="bare mono" value={a.fixtures.join(",")} placeholder="1,2,3" onChange={() => {}}
              onCommit={(v) => set({ ...a, fixtures: v.split(/[\s,;]+/).map(Number).filter((n) => Number.isInteger(n) && n > 0) })} />
            <button class="btn sm" title="Use the fixtures currently selected in Live" disabled={!selection.value.length} onClick={() => set({ ...a, fixtures: [...selection.value] })}>Selection ({selection.value.length})</button>
          </div>
        </Row>
        <Row label="Attribute"><Select class="bare" value={a.attr} options={attrOpts} onChange={(v) => set({ ...a, attr: v })} /></Row>
      </>}
      {(a.type === "effect_toggle" || a.type === "effect_rate") && <Row label="Effect"><Picker value={a.effect} options={L.effects} onChange={(v) => set({ ...a, effect: v })} /></Row>}
      {a.type === "palette_apply" && <>
        <Row label="Palette"><Picker value={a.palette} options={L.palettes} onChange={(v) => set({ ...a, palette: v })} /></Row>
        <Row label="Group"><Picker value={a.group ?? 0} options={[{ value: 0, label: "Current selection" }, ...L.groups]} onChange={(v) => set({ ...a, group: v || null })} /></Row>
      </>}
      {a.type === "page" && <Row label="Page"><Picker value={a.page} options={L.pages} onChange={(v) => set({ ...a, page: v })} /></Row>}
      {a.type === "xy" && <>
        <Row label="Group"><Picker value={a.group} options={L.groups} onChange={(v) => set({ ...a, group: v })} /></Row>
        <Row label="X attribute"><Select class="bare" value={a.x_attr} options={attrOpts} onChange={(v) => set({ ...a, x_attr: v })} /></Row>
        <Row label="Y attribute"><Select class="bare" value={a.y_attr} options={attrOpts} onChange={(v) => set({ ...a, y_attr: v })} /></Row>
      </>}
      {w.kind === "button" && a.type !== "none" && a.type !== "page" && (
        <Row label="Mode"><Select class="bare" value={w.mode} options={(Object.keys(MODE_LABEL) as VcMode[]).map((m) => ({ value: m, label: MODE_LABEL[m] }))} onChange={(m) => patchWidget(w.id, (x) => { x.mode = m; }, false)} /></Row>
      )}
      {(isContinuous(w.kind)) && <>
        <Row label="Range">
          <div class="row">
            <NumberField class="bare" float step={0.1} value={w.out_min} onChange={(v) => patchWidget(w.id, (x) => { x.out_min = v; })} />
            <span class="faint">→</span>
            <NumberField class="bare" float step={0.1} value={w.out_max} onChange={(v) => patchWidget(w.id, (x) => { x.out_max = v; })} />
          </div>
        </Row>
        {w.kind === "slider" && <Row label="Orientation"><Segmented value={w.orientation || "vertical"} options={[{ value: "vertical", label: "Vertical" }, { value: "horizontal", label: "Horizontal" }]} onChange={(o) => patchWidget(w.id, (x) => { x.orientation = o; }, false)} /></Row>}
      </>}
    </>
  );
}

// ── inputs ──────────────────────────────────────────────────────────────────
function SourceEditor({ s, onChange }: { s: Source; onChange: (s: Source) => void }) {
  const ports = midiPorts.value.map((p) => ({ value: p.name, label: p.name }));
  return (
    <div class="vc-src">
      <Select class="bare" value={s.type} options={[{ value: "midi", label: "MIDI" }, { value: "osc", label: "OSC" }]}
        onChange={(t) => onChange(t === "osc" ? { type: "osc", address: "/lumos/fader1" } : { type: "midi", port: null, kind: "cc", channel: null, number: 0 })} />
      {s.type === "osc" ? (
        <TextField class="bare mono" value={s.address} onChange={(v) => onChange({ ...s, address: v })} placeholder="/address" />
      ) : (
        <div class="vc-grid2">
          <Select class="bare" value={s.kind} options={[{ value: "note", label: "Note" }, { value: "cc", label: "CC" }, { value: "program", label: "Program" }]} onChange={(k) => onChange({ ...s, kind: k })} />
          <NumberField class="bare" value={s.number} min={0} max={127} onChange={(v) => onChange({ ...s, number: v })} />
          <Select class="bare" value={s.channel ?? 0} options={[{ value: 0, label: "Any channel" }, ...Array.from({ length: 16 }, (_, i) => ({ value: i + 1, label: `Ch ${i + 1}` }))]} onChange={(c) => onChange({ ...s, channel: c || null })} />
          <Select class="bare" value={s.port ?? ""} options={[{ value: "", label: "Any port" }, ...(s.port && !ports.some((p) => p.value === s.port) ? [{ value: s.port, label: s.port }] : []), ...ports]} onChange={(p) => onChange({ ...s, port: p || null })} />
        </div>
      )}
    </div>
  );
}

function InputsEditor({ w }: { w: VcWidget }) {
  const learn = useLearn((s) => patchWidget(w.id, (x) => { x.inputs.push(defaultInput(s)); }, false));
  const upd = (i: number, f: (x: VcInput) => void) => patchWidget(w.id, (x) => f(x.inputs[i]));
  return (
    <>
      {w.inputs.length === 0 && <div class="faint vc-hint">No hardware mapped. Press Learn, then move a fader or press a pad (MIDI) or send an OSC message.</div>}
      {w.inputs.map((inp, i) => (
        <div class="vc-card" key={i}>
          <div class="row"><b class="grow vc-card-t">{describeSource(inp.source)}</b>
            <button class="btn sm ghost" aria-label="Remove input" onClick={() => patchWidget(w.id, (x) => { x.inputs.splice(i, 1); }, false)}>✕</button></div>
          <SourceEditor s={inp.source} onChange={(s) => upd(i, (x) => { x.source = s; })} />
          <div class="row wrap">
            <Toggle checked={inp.invert} onChange={(v) => upd(i, (x) => { x.invert = v; })} label="Invert" />
            <span class="sp" />
            <span class="faint">range</span>
            <NumberField class="bare" width={52} value={Math.round(inp.in_min * 100)} min={0} max={100} onChange={(v) => upd(i, (x) => { x.in_min = v / 100; })} />
            <span class="faint">–</span>
            <NumberField class="bare" width={52} value={Math.round(inp.in_max * 100)} min={0} max={100} onChange={(v) => upd(i, (x) => { x.in_max = v / 100; })} />
            <span class="faint">%</span>
          </div>
        </div>
      ))}
      <div class="row">
        <button class={`btn ${learn.on ? "on" : "primary"} grow`} onClick={learn.start}>{learn.on ? "Listening… move a control (click to cancel)" : "● Learn"}</button>
        <button class="btn" onClick={() => patchWidget(w.id, (x) => { x.inputs.push(defaultInput({ type: "midi", port: null, kind: x.kind === "button" ? "note" : "cc", channel: null, number: 0 })); }, false)}>＋ Manual</button>
      </div>
    </>
  );
}

// ── feedback ────────────────────────────────────────────────────────────────
function fbBytes(f: Extract<FeedbackTarget, { type: "midi" }>, value: number): number[] {
  const ch = Math.max(0, Math.min(15, f.channel - 1));
  return [(f.kind === "cc" ? 0xb0 : 0x90) | ch, f.number & 127, value & 127];
}

function FeedbackEditor({ w }: { w: VcWidget }) {
  const [pid, setPid] = useState(FB_PRESETS[0].id);
  const preset = FB_PRESETS.find((p) => p.id === pid) ?? FB_PRESETS[0];
  const [ci, setCi] = useState(0);
  const color = preset.colors?.[ci];
  const outs = midiOutputs.value.map((p) => ({ value: p.name, label: p.name }));
  const upd = (i: number, f: (x: FeedbackTarget) => void) => patchWidget(w.id, (x) => f(x.feedback[i]));
  const build = (p: FbPreset, number: number, channel: number): FeedbackTarget => {
    const t = p.make({ number, channel, on: p.colors?.[ci]?.on, off: p.colors?.[ci]?.off });
    if (p.id === "cc_fader" || (isContinuous(w.kind) && t.type === "midi")) return t.type === "midi" ? { ...t, kind: "cc", scale: true } : t;
    return t;
  };
  const first = w.inputs.find((i) => i.source.type === "midi");
  const addPreset = () => {
    const src = first?.source;
    const n = src && src.type === "midi" ? src.number : 0, ch = src && src.type === "midi" ? src.channel ?? 1 : 1;
    patchWidget(w.id, (x) => { x.feedback.push(build(preset, n, ch)); }, false);
  };
  const learn = useLearn((s) => {
    if (s.type === "midi") patchWidget(w.id, (x) => { x.feedback.push(build(preset, s.number, s.channel ?? 1)); }, false);
    else patchWidget(w.id, (x) => { x.feedback.push({ type: "osc", host: "192.168.1.50", port: 9000, address: s.address, on: 1, off: 0 }); }, false);
  });
  const test = (f: FeedbackTarget) => {
    if (f.type === "osc") { toast("OSC feedback: press the widget — the engine sends it", "info"); return; }
    const n = sendMidi(f.port, fbBytes(f, f.scale ? 127 : f.on), true);
    if (!n) { toast("No MIDI output available — enable one in Settings → MIDI", "err"); return; }
    setTimeout(() => sendMidi(f.port, fbBytes(f, f.scale ? 0 : f.off), true), 450);
  };
  return (
    <>
      {w.feedback.length === 0 && <div class="faint vc-hint">No feedback. Add one so LEDs / motor faders / apps follow this widget's state.</div>}
      {w.feedback.map((f, i) => (
        <div class="vc-card" key={i}>
          <div class="row"><b class="grow vc-card-t">{describeFeedback(f)}</b>
            <button class="btn sm" onClick={() => test(f)}>Test</button>
            <button class="btn sm ghost" aria-label="Remove feedback" onClick={() => patchWidget(w.id, (x) => { x.feedback.splice(i, 1); }, false)}>✕</button></div>
          {f.type === "midi" ? (
            <div class="vc-grid2">
              <Select class="bare" value={f.kind === "cc" ? "cc" : "note"} options={[{ value: "note", label: "Note" }, { value: "cc", label: "CC" }]} onChange={(k) => upd(i, (x) => { if (x.type === "midi") x.kind = k; })} />
              <NumberField class="bare" value={f.number} min={0} max={127} onChange={(v) => upd(i, (x) => { if (x.type === "midi") x.number = v; })} />
              <Select class="bare" value={f.channel} options={Array.from({ length: 16 }, (_, k) => ({ value: k + 1, label: `Ch ${k + 1}` }))} onChange={(c) => upd(i, (x) => { if (x.type === "midi") x.channel = c; })} />
              <Select class="bare" value={f.port ?? ""} options={[{ value: "", label: "All enabled outputs" }, ...(f.port && !outs.some((p) => p.value === f.port) ? [{ value: f.port, label: f.port }] : []), ...outs]} onChange={(p) => upd(i, (x) => { if (x.type === "midi") x.port = p || null; })} />
              <label class="vc-mini">On <NumberField class="bare" value={f.on} min={0} max={127} disabled={f.scale} onChange={(v) => upd(i, (x) => { if (x.type === "midi") x.on = v; })} /></label>
              <label class="vc-mini">Off <NumberField class="bare" value={f.off} min={0} max={127} disabled={f.scale} onChange={(v) => upd(i, (x) => { if (x.type === "midi") x.off = v; })} /></label>
              <div style={{ gridColumn: "1 / -1" }}><Toggle checked={f.scale} onChange={(v) => upd(i, (x) => { if (x.type === "midi") x.scale = v; })} label="Send scaled 0–127 value (faders / knobs)" /></div>
            </div>
          ) : (
            <div class="vc-grid2">
              <TextField class="bare mono" value={f.host} onChange={(v) => upd(i, (x) => { if (x.type === "osc") x.host = v; })} placeholder="host" />
              <NumberField class="bare" value={f.port} min={1} max={65535} onChange={(v) => upd(i, (x) => { if (x.type === "osc") x.port = v; })} />
              <div style={{ gridColumn: "1 / -1" }}><TextField class="bare mono" value={f.address} onChange={(v) => upd(i, (x) => { if (x.type === "osc") x.address = v; })} placeholder="/address" /></div>
              <label class="vc-mini">On <NumberField class="bare" float step={0.1} value={f.on} onChange={(v) => upd(i, (x) => { if (x.type === "osc") x.on = v; })} /></label>
              <label class="vc-mini">Off <NumberField class="bare" float step={0.1} value={f.off} onChange={(v) => upd(i, (x) => { if (x.type === "osc") x.off = v; })} /></label>
            </div>
          )}
        </div>
      ))}
      <div class="vc-card vc-add">
        <div class="vc-cap">ADD FEEDBACK</div>
        <Select class="bare" value={pid} options={FB_PRESETS.map((p) => ({ value: p.id, label: p.label }))} onChange={(v) => { setPid(v); setCi(0); }} />
        <div class="faint vc-hint">{preset.hint}</div>
        {preset.colors && <div class="row wrap">{preset.colors.map((c, i) => <button key={c.label} class={`btn sm ${i === ci ? "sel" : ""}`} onClick={() => setCi(i)}>{c.label} <span class="mono faint">{c.on}</span></button>)}</div>}
        <div class="row">
          <button class="btn grow" onClick={addPreset} title={first ? "Uses the first MIDI input's note/CC" : "Adds with number 0"}>＋ Add{first ? " (same note/CC as input)" : ""}</button>
          <button class={`btn ${learn.on ? "on" : ""}`} onClick={learn.start} title="Learn the controller note/CC to light">{learn.on ? "Listening…" : "● Learn"}</button>
        </div>
        {color && <span class="faint vc-hint">Colour {color.label}: velocity {color.on}</span>}
      </div>
    </>
  );
}

// ── panels ──────────────────────────────────────────────────────────────────
export function WidgetProps({ w }: { w: VcWidget }) {
  return (
    <>
      <Sec title={`${KINDS.find((k) => k.kind === w.kind)?.label ?? w.kind}`} right={<>
        <button class="btn sm" title="Duplicate (Ctrl+D)" onClick={duplicateSelected}>⧉</button>
        <button class="btn sm danger" title="Delete" onClick={deleteSelected}>🗑</button></>}>
        <Row label="Label"><TextField class="bare" value={w.label} onChange={(v) => patchWidget(w.id, (x) => { x.label = v; })} /></Row>
        <Row label="Colour">
          <div class="row wrap" style={{ gap: 4 }}>
            {PALETTE_COLORS.map((c) => <button key={c} class={`vc-sw ${w.color === c ? "on" : ""}`} style={{ background: c }} aria-label={c} onClick={() => patchWidget(w.id, (x) => { x.color = c; }, false)} />)}
            <input type="color" class="vc-color" value={w.color} onInput={(e) => patchWidget(w.id, (x) => { x.color = (e.target as HTMLInputElement).value; })} />
          </div>
        </Row>
        <Row label="Kind"><Select class="bare" value={w.kind} options={KINDS.map((k) => ({ value: k.kind, label: k.label }))}
          onChange={(k) => patchWidget(w.id, (x) => { x.kind = k as VcKind; if (!actionTypesFor(x.kind).includes(x.action.type)) x.action = defaultAction(actionTypesFor(x.kind)[0], show.value, x.kind); }, false)} /></Row>
        <Row label="Position"><span class="mono dim">{w.x},{w.y} · {w.w}×{w.h}</span></Row>
      </Sec>
      {hasAction(w.kind) && <Sec title="Action"><ActionEditor w={w} /></Sec>}
      {hasAction(w.kind) && <Sec title="Inputs · MIDI / OSC"><InputsEditor w={w} /></Sec>}
      {hasAction(w.kind) && <Sec title="Feedback · LEDs / faders / OSC"><FeedbackEditor w={w} /></Sec>}
    </>
  );
}

export function MultiProps() {
  const n = selected.value.length;
  const al = (a: Align, label: string, t: string) => <button class="btn sm" title={t} onClick={() => alignSelected(a)}>{label}</button>;
  return (
    <Sec title={`${n} widgets selected`}>
      <Row label="Align"><div class="row wrap" style={{ gap: 4 }}>{al("left", "⇤", "Left")}{al("hcenter", "↔", "Centre horizontally")}{al("right", "⇥", "Right")}{al("top", "⤒", "Top")}{al("vcenter", "↕", "Centre vertically")}{al("bottom", "⤓", "Bottom")}</div></Row>
      <Row label="Distribute"><div class="row" style={{ gap: 4 }}>{al("hdist", "⇔ Horizontal", "Even gaps")}{al("vdist", "⇕ Vertical", "Even gaps")}</div></Row>
      <Row label="Colour"><div class="row wrap" style={{ gap: 4 }}>{PALETTE_COLORS.map((c) => <button key={c} class="vc-sw" style={{ background: c }} aria-label={c} onClick={() => patchSelected((x) => { x.color = c; }, false)} />)}</div></Row>
      <div class="row"><button class="btn grow" onClick={duplicateSelected}>Duplicate</button><button class="btn danger grow" onClick={deleteSelected}>Delete</button></div>
    </Sec>
  );
}

export function PageProps() {
  const p = page.value; if (!p) return null;
  return (
    <Sec title="Page">
      <Row label="Name"><TextField class="bare" value={p.name} onChange={(v) => renamePage(p.id, v)} /></Row>
      <Row label="Grid">
        <div class="row"><NumberField class="bare" value={p.cols} min={4} max={48} onChange={(v) => resizePage(p.id, v, p.rows)} /><span class="faint">×</span><NumberField class="bare" value={p.rows} min={2} max={32} onChange={(v) => resizePage(p.id, p.cols, v)} /></div>
      </Row>
      <button class="btn danger" onClick={async () => { if (await confirmDialog("Delete page", `Delete “${p.name}” and its ${p.widgets.length} widgets?`, { danger: true, ok: "Delete" })) deletePage(p.id); }}>Delete page</button>
    </Sec>
  );
}
