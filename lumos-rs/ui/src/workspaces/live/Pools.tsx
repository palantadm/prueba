import { useState } from "preact/hooks";
import { rt, selection, send, show } from "../../store";
import type { Effect, Palette, PaletteKind, Shape } from "../../types";
import { ALL_ATTRS, attrLabel, fixtureById, rgbToHex } from "../../lib/attrs";
import { useClock, useLongPress } from "../../lib/hooks";
import { Modal } from "../../ui";
import { ConfirmDialog, PromptDialog } from "./dialogs";

const KINDS: { id: PaletteKind; label: string }[] = [
  { id: "intensity", label: "Intensity" }, { id: "position", label: "Position" },
  { id: "color", label: "Color" }, { id: "beam", label: "Beam" }, { id: "all", label: "All" },
];

export function Pools() {
  const [tab, setTab] = useState<"palettes" | "effects">("palettes");
  return (
    <div class="panel lv-col pools">
      <div class="tabs" role="tablist">
        <button role="tab" aria-selected={tab === "palettes"} class={`tab ${tab === "palettes" ? "active" : ""}`} onClick={() => setTab("palettes")}>Palettes</button>
        <button role="tab" aria-selected={tab === "effects"} class={`tab ${tab === "effects" ? "active" : ""}`} onClick={() => setTab("effects")}>Effects</button>
      </div>
      {tab === "palettes" ? <PalettePool /> : <EffectPool />}
    </div>
  );
}

// ── palettes ────────────────────────────────────────────
function swatch(p: Palette): string | null {
  const v = { ...p.values };
  for (const pf of Object.values(p.per_fixture)) if (pf) { Object.assign(v, pf); break; }
  if (v.red !== undefined || v.green !== undefined || v.blue !== undefined) return rgbToHex(v.red ?? 0, v.green ?? 0, v.blue ?? 0);
  return null;
}
function summary(p: Palette): string {
  const v = p.values;
  if (v.pan !== undefined || v.tilt !== undefined) return `P ${Math.round((v.pan ?? 0) * 100)} T ${Math.round((v.tilt ?? 0) * 100)}`;
  if (v.dimmer !== undefined) return `${Math.round(v.dimmer * 100)}%`;
  const n = Object.keys(v).length + Object.keys(p.per_fixture).length;
  return `${n} value${n === 1 ? "" : "s"}`;
}

function PaletteCell({ p, onEdit }: { p: Palette; onEdit: () => void }) {
  const lp = useLongPress(onEdit);
  const sw = swatch(p);
  return (
    <button class="lv-pal-cell" {...lp as any} title={`${p.name} — right-click / long-press to edit`}
      onClick={() => { if (!lp.wasLong() && selection.value.length) send({ type: "prog_apply_palette", fixtures: selection.value, palette: p.id }); }}>
      {sw && <span class="lv-pal-sw" style={{ background: sw }} />}
      <span>{p.name}</span><span class="dim">{summary(p)}</span>
    </button>
  );
}

function PalettePool() {
  const [kind, setKind] = useState<PaletteKind>("color");
  const [naming, setNaming] = useState(false);
  const [edit, setEdit] = useState<Palette | null>(null);
  const pals = (show.value?.palettes ?? []).filter((p) => p.kind === kind || kind === "all");
  const held = selection.value.some((id) => rt.value?.programmer[id] && Object.keys(rt.value.programmer[id]!).length);
  return (
    <>
      <div class="lv-selbar" role="tablist" aria-label="Palette kind">
        {KINDS.map((k) => <button key={k.id} role="tab" class={`btn sm ${kind === k.id ? "sel" : ""}`} onClick={() => setKind(k.id)}>{k.label}</button>)}
        <span class="sp" />
        <button class="btn sm primary" disabled={!held} onClick={() => setNaming(true)} title="Record palette from programmer values of the selection">● Record</button>
      </div>
      <div class="lv-scroll">
        <div class="lv-pal">
          {pals.length === 0 && <div class="dim" style={{ gridColumn: "1/-1" }}>No {kind} palettes. Set values, select fixtures, then Record.</div>}
          {pals.map((p) => <PaletteCell key={p.id} p={p} onEdit={() => setEdit(p)} />)}
        </div>
      </div>
      {naming && <PromptDialog title={`Record ${kind} palette`} label="Name" ok="Record" initial={`${KINDS.find((k) => k.id === kind)!.label} ${pals.length + 1}`}
        onClose={() => setNaming(false)}
        onSubmit={(name) => send({ type: "prog_record_palette", id: null, fixtures: selection.value, kind, name })} />}
      {edit && <PaletteEdit p={edit} onClose={() => setEdit(null)} />}
    </>
  );
}

function PaletteEdit({ p, onClose }: { p: Palette; onClose: () => void }) {
  const [name, setName] = useState(p.name);
  const [del, setDel] = useState(false);
  if (del) return <ConfirmDialog title="Delete palette" text={`Delete “${p.name}”?`} ok="Delete" danger onClose={onClose} onYes={() => send({ type: "palette_remove", id: p.id })} />;
  return (
    <Modal title={`Palette “${p.name}”`} onClose={onClose} footer={<>
      <button class="btn danger" onClick={() => setDel(true)}>Delete</button><span class="sp" />
      <button class="btn" onClick={onClose}>Cancel</button>
      <button class="btn primary" onClick={() => { send({ type: "palette_save", palette: { ...p, name: name.trim() || p.name } }); onClose(); }}>Save</button>
    </>}>
      <div class="field"><label>Name</label><input class="input" value={name} onInput={(e) => setName((e.target as HTMLInputElement).value)} /></div>
      <button class="btn" disabled={!selection.value.length}
        onClick={() => { send({ type: "prog_record_palette", id: p.id, fixtures: selection.value, kind: p.kind, name: name.trim() || p.name }); onClose(); }}>
        Update from programmer ({selection.value.length} selected)</button>
      <div class="dim mono">{Object.entries(p.values).map(([k, v]) => `${k}=${Math.round((v ?? 0) * 100)}%`).join("  ") || "per-fixture values only"}</div>
    </Modal>
  );
}

// ── effects ─────────────────────────────────────────────
const SHAPES: Shape[] = ["sine", "triangle", "saw", "square", "random"];
function shapeAt(shape: Shape, t: number): number {
  const f = t - Math.floor(t);
  switch (shape) {
    case "sine": return Math.sin(f * Math.PI * 2);
    case "triangle": return f < 0.5 ? f * 4 - 1 : 3 - f * 4;
    case "saw": return f * 2 - 1;
    case "square": return f < 0.5 ? 1 : -1;
    default: return Math.sin(Math.floor(t * 8) * 12.9898) % 1;
  }
}
function Wave({ e, on }: { e: Effect; on: boolean }) {
  const t = useClock(on);
  const pts: string[] = [];
  const W = 90, H = 28;
  for (let x = 0; x <= W; x += 2) {
    const ph = x / W + (on ? t * e.rate : 0);
    pts.push(`${x},${(H / 2 - shapeAt(e.shape, ph) * e.depth * (H / 2 - 2)).toFixed(1)}`);
  }
  return (
    <svg class="lv-wave" viewBox={`0 0 ${W} ${H}`} aria-hidden="true">
      <polyline points={pts.join(" ")} fill="none" stroke={on ? "var(--attr-effect)" : "var(--text-faint)"} stroke-width="1.5" />
    </svg>
  );
}

function EffectPool() {
  const effects = show.value?.effects ?? [];
  const running = new Set(rt.value?.effects_running ?? []);
  const [edit, setEdit] = useState<Effect | null>(null);
  const blank = (): Effect => ({
    id: Math.max(0, ...effects.map((e) => e.id)) + 1, name: `Effect ${effects.length + 1}`,
    attr: "dimmer", shape: "sine", rate: 0.5, depth: 0.5, phase_spread: 0, fixtures: [...selection.value],
  });
  return (
    <>
      <div class="lv-selbar">
        <button class="btn sm primary" onClick={() => setEdit(blank())}>+ New effect</button>
        <span class="dim" style={{ alignSelf: "center" }}>uses current selection ({selection.value.length})</span>
      </div>
      <div class="lv-scroll">
        {effects.length === 0 && <div class="dim" style={{ padding: 12 }}>No effects yet.</div>}
        {effects.map((e) => {
          const on = running.has(e.id);
          return (
            <div class="lv-fxrow" key={e.id}>
              <button class={`btn ${on ? "on" : ""}`} aria-pressed={on} aria-label={`Toggle ${e.name}`}
                onClick={() => send({ type: "effect_toggle", id: e.id, on: !on })}>{on ? "ON" : "OFF"}</button>
              <div class="grow" style={{ cursor: "pointer" }} onClick={() => setEdit(e)}>
                <div style={{ fontWeight: 700 }}>{e.name}</div>
                <div class="dim" style={{ fontSize: 11 }}>{e.attr === "hue" ? "Hue" : attrLabel(e.attr)} · {e.shape} · {e.rate} Hz · {e.fixtures.length} fx</div>
              </div>
              <Wave e={e} on={on} />
            </div>
          );
        })}
      </div>
      {edit && <EffectEditor e={edit} exists={effects.some((x) => x.id === edit.id)} onClose={() => setEdit(null)} />}
    </>
  );
}

function EffectEditor({ e, exists, onClose }: { e: Effect; exists: boolean; onClose: () => void }) {
  const [d, setD] = useState<Effect>(e);
  const up = (patch: Partial<Effect>) => setD({ ...d, ...patch });
  const num = (v: string, f: number) => (Number.isFinite(parseFloat(v)) ? parseFloat(v) : f);
  return (
    <Modal title={exists ? `Effect “${e.name}”` : "New effect"} onClose={onClose} footer={<>
      {exists && <button class="btn danger" onClick={() => { send({ type: "effect_remove", id: d.id }); onClose(); }}>Delete</button>}
      <span class="sp" />
      <button class="btn" onClick={onClose}>Cancel</button>
      <button class="btn primary" disabled={!d.fixtures.length} onClick={() => { send({ type: "effect_save", effect: d }); onClose(); }}>Save</button>
    </>}>
      <div class="field"><label>Name</label><input class="input" value={d.name} onInput={(ev) => up({ name: (ev.target as HTMLInputElement).value })} /></div>
      <div class="row wrap">
        <div class="field grow"><label>Attribute</label>
          <select class="input" value={d.attr} onChange={(ev) => up({ attr: (ev.target as HTMLSelectElement).value })}>
            <option value="hue">Hue (rainbow)</option>
            {ALL_ATTRS.map((a) => <option key={a} value={a}>{attrLabel(a)}</option>)}
          </select></div>
        <div class="field grow"><label>Shape</label>
          <select class="input" value={d.shape} onChange={(ev) => up({ shape: (ev.target as HTMLSelectElement).value as Shape })}>
            {SHAPES.map((s) => <option key={s} value={s}>{s}</option>)}
          </select></div>
      </div>
      <div class="row wrap">
        <div class="field grow"><label>Rate (Hz)</label><input class="input num" type="number" step="0.05" min="0" value={d.rate} onInput={(ev) => up({ rate: num((ev.target as HTMLInputElement).value, d.rate) })} /></div>
        <div class="field grow"><label>Depth %</label><input class="input num" type="number" min="0" max="100" value={Math.round(d.depth * 100)} onInput={(ev) => up({ depth: num((ev.target as HTMLInputElement).value, 0) / 100 })} /></div>
        <div class="field grow"><label>Phase spread °</label><input class="input num" type="number" min="0" max="360" value={d.phase_spread} onInput={(ev) => up({ phase_spread: num((ev.target as HTMLInputElement).value, 0) })} /></div>
      </div>
      <div class="row">
        <Wave e={d} on={true} />
        <span class="dim grow">Fixtures: {d.fixtures.length ? d.fixtures.join(", ") : "none"}</span>
        <button class="btn" disabled={!selection.value.length} onClick={() => up({ fixtures: selection.value.filter((i) => fixtureById(i)) })}>Use selection ({selection.value.length})</button>
      </div>
    </Modal>
  );
}
