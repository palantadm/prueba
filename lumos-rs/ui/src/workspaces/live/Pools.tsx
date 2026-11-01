import { useEffect, useState } from "preact/hooks";
import { rt, selection, send, show } from "../../store";
import type { Effect, Palette, PaletteKind, Shape } from "../../types";
import { ALL_ATTRS, ATTR_META, attrLabel, fixtureById, rgbToHex } from "../../lib/attrs";
import { useClock, useLongPress } from "../../lib/hooks";
import { Modal } from "../../ui";
import { ConfirmDialog, PromptDialog } from "./dialogs";
import { ScenePool } from "./Scenes";
import { ChasePool } from "./Chases";
import { PixelPool } from "./PixelPool";
import { t, tp } from "../../i18n";
import { focusPalette } from "../../lib/state";

const KINDS: { id: PaletteKind; label: string }[] = [
  { id: "intensity", label: "Intensity" }, { id: "position", label: "Position" },
  { id: "color", label: "Color" }, { id: "beam", label: "Beam" }, { id: "all", label: "All" },
];

export function Pools() {
  const [tab, setTab] = useState<"palettes" | "effects" | "scenes" | "chases" | "pixels">("palettes");
  useEffect(() => { if (focusPalette.value !== null) setTab("palettes"); }, [focusPalette.value]);
  return (
    <div class="panel lv-col pools">
      <div class="lv-pooltabs" role="tablist">
        <button role="tab" aria-selected={tab === "palettes"} class={`seg-btn ${tab === "palettes" ? "active" : ""}`} onClick={() => setTab("palettes")}>{t("Palettes")}</button>
        <button role="tab" aria-selected={tab === "effects"} class={`seg-btn ${tab === "effects" ? "active" : ""}`} onClick={() => setTab("effects")}>{t("Effects")}</button>
        <button role="tab" aria-selected={tab === "scenes"} class={`seg-btn ${tab === "scenes" ? "active" : ""}`} onClick={() => setTab("scenes")}>{t("Scenes")}</button>
        <button role="tab" aria-selected={tab === "chases"} class={`seg-btn ${tab === "chases" ? "active" : ""}`} onClick={() => setTab("chases")}>{t("Chases")}</button>
        <button role="tab" aria-selected={tab === "pixels"} class={`seg-btn ${tab === "pixels" ? "active" : ""}`} onClick={() => setTab("pixels")}>{t("Pixels")}</button>
      </div>
      {tab === "palettes" ? <PalettePool /> : tab === "effects" ? <EffectPool /> : tab === "scenes" ? <ScenePool /> : tab === "chases" ? <ChasePool /> : <PixelPool />}
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
  return tp(n, "{n} value", "{n} values");
}

function PaletteCell({ p, onEdit, flash }: { p: Palette; onEdit: () => void; flash?: boolean }) {
  const lp = useLongPress(onEdit);
  const sw = swatch(p);
  return (
    <button class={`lv-pal-cell ${flash ? "flash" : ""}`} {...lp as any} title={t("{name} — right-click / long-press to edit", { name: p.name })}
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
  // Reveal a palette linked from the cue editor: switch to its kind, flash it, then clear.
  useEffect(() => {
    const id = focusPalette.value;
    if (id === null) return;
    const pal = show.value?.palettes.find((x) => x.id === id);
    if (pal) setKind(pal.kind);
    const tm = setTimeout(() => { if (focusPalette.value === id) focusPalette.value = null; }, 2600);
    return () => clearTimeout(tm);
  }, [focusPalette.value]);
  const held = selection.value.some((id) => rt.value?.programmer[id] && Object.keys(rt.value.programmer[id]!).length);
  return (
    <>
      <div class="lv-selbar" role="tablist" aria-label={t("Palette kind")}>
        {KINDS.map((k) => <button key={k.id} role="tab" class={`btn pill ${kind === k.id ? "sel" : ""}`} onClick={() => setKind(k.id)}>{t(k.label)}</button>)}
        <span class="sp" />
        <button class="btn primary" disabled={!held} onClick={() => setNaming(true)} title={t("Record palette from programmer values of the selection")}>{t("● Record")}</button>
      </div>
      <div class="lv-scroll">
        <div class="lv-pal">
          {pals.length === 0 && <div class="dim" style={{ gridColumn: "1/-1" }}>{t("No {kind} palettes. Set values, select fixtures, then Record.", { kind: t(KINDS.find((k) => k.id === kind)!.label).toLowerCase() })}</div>}
          {pals.map((p) => <PaletteCell key={p.id} p={p} flash={focusPalette.value === p.id} onEdit={() => setEdit(p)} />)}
        </div>
      </div>
      {naming && <PromptDialog title={t("Record {kind} palette", { kind: t(KINDS.find((k) => k.id === kind)!.label).toLowerCase() })} label={t("Name")} ok={t("Record")} initial={`${t(KINDS.find((k) => k.id === kind)!.label)} ${pals.length + 1}`}
        onClose={() => setNaming(false)}
        onSubmit={(name) => send({ type: "prog_record_palette", id: null, fixtures: selection.value, kind, name })} />}
      {edit && <PaletteEdit p={edit} onClose={() => setEdit(null)} />}
    </>
  );
}

function PaletteEdit({ p, onClose }: { p: Palette; onClose: () => void }) {
  const [name, setName] = useState(p.name);
  const [del, setDel] = useState(false);
  if (del) return <ConfirmDialog title={t("Delete palette")} text={t("Delete “{name}”?", { name: p.name })} ok={t("Delete")} danger onClose={onClose} onYes={() => send({ type: "palette_remove", id: p.id })} />;
  return (
    <Modal title={t("Palette “{name}”", { name: p.name })} onClose={onClose} footer={<>
      <button class="btn danger" onClick={() => setDel(true)}>{t("Delete")}</button><span class="sp" />
      <button class="btn" onClick={onClose}>{t("Cancel")}</button>
      <button class="btn primary" onClick={() => { send({ type: "palette_save", palette: { ...p, name: name.trim() || p.name } }); onClose(); }}>{t("Save")}</button>
    </>}>
      <div class="field"><label>{t("Name")}</label><input class="input" value={name} onInput={(e) => setName((e.target as HTMLInputElement).value)} /></div>
      <button class="btn" disabled={!selection.value.length}
        onClick={() => { send({ type: "prog_record_palette", id: p.id, fixtures: selection.value, kind: p.kind, name: name.trim() || p.name }); onClose(); }}>
        {t("Update from programmer ({n} selected)", { n: selection.value.length })}</button>
      <div class="dim mono">{Object.entries(p.values).map(([k, v]) => `${k}=${Math.round((v ?? 0) * 100)}%`).join("  ") || t("per-fixture values only")}</div>
    </Modal>
  );
}

// ── effects ─────────────────────────────────────────────
const SHAPES: Shape[] = ["sine", "triangle", "saw", "square", "random", "ramp", "pulse", "circle", "eight"];
const is2D = (s: Shape) => s === "circle" || s === "eight";
/** Mirrors the engine's waveforms (src/engine/effects.rs) for the previews. */
function shapeAt(shape: Shape, t: number): number {
  const f = t - Math.floor(t);
  switch (shape) {
    case "sine": case "circle": case "eight": return Math.sin(f * Math.PI * 2);
    case "triangle": return f < 0.5 ? f * 4 - 1 : 3 - f * 4;
    case "saw": return f * 2 - 1;
    case "ramp": return 1 - 2 * f;
    case "pulse": return f < 0.15 ? 1 : -1;
    case "square": return f < 0.5 ? 1 : -1;
    default: return Math.sin(Math.floor(t * 8) * 12.9898) % 1;
  }
}
function xyAt(shape: Shape, ph: number, rotDeg: number): [number, number] {
  const th = ph * Math.PI * 2;
  const [x, y] = shape === "circle" ? [Math.cos(th), Math.sin(th)] : [Math.sin(th), Math.sin(2 * th)];
  const r = (rotDeg * Math.PI) / 180;
  return [x * Math.cos(r) - y * Math.sin(r), x * Math.sin(r) + y * Math.cos(r)];
}
/** Cycles per second the effect actually runs at (speed master › BPM › rate). */
function effectHz(e: Effect): number {
  if (e.speed_master !== null) {
    const m = show.value?.speed_masters?.find((x) => x.id === e.speed_master);
    if (m) return (rt.value?.speed?.find((x) => x.id === m.id)?.bpm ?? m.bpm) / 60;
  }
  return e.bpm > 0 ? e.bpm / 60 : e.rate;
}
function rateText(e: Effect): string {
  if (e.speed_master !== null) return t("master {name}", { name: show.value?.speed_masters?.find((x) => x.id === e.speed_master)?.name ?? `#${e.speed_master}` });
  return e.bpm > 0 ? `${e.bpm} BPM` : `${e.rate} Hz`;
}

function Wave({ e, on, large }: { e: Effect; on: boolean; large?: boolean }) {
  const clk = useClock(on);
  const ph0 = on ? clk * effectHz(e) : 0;
  if (is2D(e.shape)) {
    const S = large ? 120 : 28, R = S / 2 - 3, k = Math.min(1, Math.max(0.1, e.depth));
    const pts: string[] = [];
    for (let i = 0; i <= 64; i++) { const [x, y] = xyAt(e.shape, i / 64, e.rotation); pts.push(`${(S / 2 + x * R * k).toFixed(1)},${(S / 2 - y * R * k).toFixed(1)}`); }
    const [dx, dy] = xyAt(e.shape, ph0, e.rotation);
    return (
      <svg class={large ? "fx-fig" : "lv-wave"} width={S} height={S} viewBox={`0 0 ${S} ${S}`} aria-hidden="true">
        <polyline points={pts.join(" ")} fill="none" stroke={on || large ? "var(--attr-effect)" : "var(--text-faint)"} stroke-width="1.5" />
        {(on || large) && <circle cx={S / 2 + dx * R * k} cy={S / 2 - dy * R * k} r={large ? 4 : 2.5} fill="var(--live)" />}
      </svg>
    );
  }
  const pts: string[] = [];
  const W = large ? 240 : 90, H = large ? 70 : 28;
  for (let x = 0; x <= W; x += 2) {
    const ph = x / W + ph0;
    pts.push(`${x},${(H / 2 - shapeAt(e.shape, ph) * e.depth * (H / 2 - 2)).toFixed(1)}`);
  }
  return (
    <svg class={large ? "fx-fig" : "lv-wave"} width={large ? W : undefined} height={large ? H : undefined} viewBox={`0 0 ${W} ${H}`} aria-hidden="true">
      <polyline points={pts.join(" ")} fill="none" stroke={on || large ? "var(--attr-effect)" : "var(--text-faint)"} stroke-width="1.5" />
    </svg>
  );
}

function EffectPool() {
  const effects = show.value?.effects ?? [];
  const running = new Set(rt.value?.effects_running ?? []);
  const [edit, setEdit] = useState<Effect | null>(null);
  const blank = (): Effect => ({
    id: Math.max(0, ...effects.map((e) => e.id)) + 1, name: t("Effect {n}", { n: effects.length + 1 }),
    attr: "dimmer", shape: "sine", rate: 0.5, depth: 0.5, phase_spread: 0, fixtures: [...selection.value],
    attr2: null, speed_master: null, bpm: 0, rotation: 0,
  });
  return (
    <>
      <div class="lv-selbar">
        <button class="btn primary" style={{ minHeight: 44 }} onClick={() => setEdit(blank())}>{t("+ New effect")}</button>
        <span class="dim" style={{ alignSelf: "center" }}>{t("uses current selection ({n})", { n: selection.value.length })}</span>
      </div>
      <div class="lv-scroll">
        {effects.length === 0 && <div class="dim" style={{ padding: 12 }}>{t("No effects yet.")}</div>}
        {effects.map((e) => {
          const on = running.has(e.id);
          return (
            <div class="lv-fxrow" key={e.id}>
              <button class={`btn ${on ? "on" : ""}`} style={{ minHeight: 44, minWidth: 56 }} aria-pressed={on} aria-label={t("Toggle {name}", { name: e.name })}
                onClick={() => send({ type: "effect_toggle", id: e.id, on: !on })}>{on ? "ON" : "OFF"}</button>
              <div class="grow" style={{ cursor: "pointer", minHeight: 44 }} onClick={() => setEdit(e)}>
                <div style={{ fontWeight: 700 }}>{e.name}</div>
                <div class="dim" style={{ fontSize: 11 }}>
                  {e.attr === "hue" ? t("Hue") : attrLabel(e.attr)}{e.attr2 && is2D(e.shape) ? ` / ${attrLabel(e.attr2)}` : ""} · {t(e.shape)} · {rateText(e)} · {t("{n} fx", { n: e.fixtures.length })}
                </div>
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
  const masters = show.value?.speed_masters ?? [];
  const mode: "hz" | "bpm" | "master" = d.speed_master !== null ? "master" : d.bpm > 0 ? "bpm" : "hz";
  const setMode = (m: "hz" | "bpm" | "master") =>
    m === "hz" ? up({ speed_master: null, bpm: 0 })
      : m === "bpm" ? up({ speed_master: null, bpm: d.bpm > 0 ? d.bpm : 120 })
        : masters.length ? up({ speed_master: masters[0].id, bpm: 0 }) : undefined;
  const pickShape = (s: Shape) => {
    if (is2D(s)) {
      const both = ATTR_META[d.attr]?.group === "position" && d.attr2 && ATTR_META[d.attr2]?.group === "position";
      up({ shape: s, ...(both ? {} : { attr: "pan", attr2: "tilt" }) });
    } else up({ shape: s, attr2: null });
  };
  const two = is2D(d.shape);
  const attrSel = (v: string, set: (a: string) => void, label: string, hue = false) => (
    <div class="field grow"><label>{label}</label>
      <select class="input" style={{ minHeight: 44 }} value={v} onChange={(ev) => set((ev.target as HTMLSelectElement).value)}>
        {hue && <option value="hue">{t("Hue (rainbow)")}</option>}
        {ALL_ATTRS.map((a) => <option key={a} value={a}>{attrLabel(a)}</option>)}
      </select></div>
  );
  return (
    <Modal title={exists ? t("Effect “{name}”", { name: e.name }) : t("New effect")} onClose={onClose} width={620} footer={<>
      {exists && <button class="btn danger" onClick={() => { send({ type: "effect_remove", id: d.id }); onClose(); }}>{t("Delete")}</button>}
      <span class="sp" />
      <button class="btn" onClick={onClose}>{t("Cancel")}</button>
      <button class="btn primary" disabled={!d.fixtures.length} onClick={() => { send({ type: "effect_save", effect: d }); onClose(); }}>{t("Save")}</button>
    </>}>
      <div class="field"><label>{t("Name")}</label><input class="input" style={{ minHeight: 44 }} value={d.name} onInput={(ev) => up({ name: (ev.target as HTMLInputElement).value })} /></div>
      <div class="field"><label>{t("Shape")}</label>
        <div class="fx-modes" role="tablist" aria-label={t("Shape")}>
          {SHAPES.map((s) => <button key={s} role="tab" aria-selected={d.shape === s} class={`btn ${d.shape === s ? "sel" : ""}`} onClick={() => pickShape(s)}>{t(s)}</button>)}
        </div>
      </div>
      <div class="fx-preview">
        <Wave e={d} on={true} large />
        <div class="fx-fig-lbl">{two ? t("Path of the two attributes (x / y)") : t("Waveform")}</div>
      </div>
      <div class="row wrap">
        {two ? <>
          {attrSel(d.attr, (a) => up({ attr: a }), t("X axis"))}
          {attrSel(d.attr2 ?? "tilt", (a) => up({ attr2: a }), t("Y axis"))}
        </> : attrSel(d.attr, (a) => up({ attr: a }), t("Attribute"), true)}
      </div>
      <div class="field"><label>{t("Speed")}</label>
        <div class="fx-modes" role="tablist" aria-label={t("Speed source")}>
          <button role="tab" aria-selected={mode === "hz"} class={`btn ${mode === "hz" ? "sel" : ""}`} onClick={() => setMode("hz")}>Hz</button>
          <button role="tab" aria-selected={mode === "bpm"} class={`btn ${mode === "bpm" ? "sel" : ""}`} onClick={() => setMode("bpm")}>BPM</button>
          <button role="tab" aria-selected={mode === "master"} class={`btn ${mode === "master" ? "sel" : ""}`} disabled={!masters.length} onClick={() => setMode("master")}
            title={masters.length ? "" : t("Create a speed master in the Chases tab first")}>{t("Speed master")}</button>
        </div>
        {mode === "hz" && <input class="input num" style={{ minHeight: 44 }} type="number" step="0.05" min="0" aria-label={t("Rate (Hz)")} value={d.rate} onInput={(ev) => up({ rate: num((ev.target as HTMLInputElement).value, d.rate) })} />}
        {mode === "bpm" && <input class="input num" style={{ minHeight: 44 }} type="number" step="1" min="1" max="400" aria-label="BPM" value={d.bpm} onInput={(ev) => up({ bpm: Math.max(1, num((ev.target as HTMLInputElement).value, d.bpm)) })} />}
        {mode === "master" && (
          <select class="input" style={{ minHeight: 44 }} aria-label={t("Speed master")} value={d.speed_master ?? ""} onChange={(ev) => up({ speed_master: Number((ev.target as HTMLSelectElement).value) })}>
            {masters.map((m) => <option key={m.id} value={m.id}>{m.name} · {Math.round(rt.value?.speed?.find((x) => x.id === m.id)?.bpm ?? m.bpm)} BPM</option>)}
          </select>
        )}
      </div>
      <div class="row wrap">
        <div class="field grow"><label>{two ? t("Size %") : t("Depth %")}</label><input class="input num" style={{ minHeight: 44 }} type="number" min="0" max="100" value={Math.round(d.depth * 100)} onInput={(ev) => up({ depth: num((ev.target as HTMLInputElement).value, 0) / 100 })} /></div>
        <div class="field grow"><label>{t("Phase spread °")}</label><input class="input num" style={{ minHeight: 44 }} type="number" min="0" max="360" value={d.phase_spread} onInput={(ev) => up({ phase_spread: num((ev.target as HTMLInputElement).value, 0) })} /></div>
        {two && <div class="field grow"><label>{t("Rotation °")}</label><input class="input num" style={{ minHeight: 44 }} type="number" min="-360" max="360" value={d.rotation} onInput={(ev) => up({ rotation: num((ev.target as HTMLInputElement).value, 0) })} /></div>}
      </div>
      <div class="row">
        <span class="dim grow">{t("Fixtures:")} {d.fixtures.length ? d.fixtures.join(", ") : t("none")}</span>
        <button class="btn" style={{ minHeight: 44 }} disabled={!selection.value.length} onClick={() => up({ fixtures: selection.value.filter((i) => fixtureById(i)) })}>{t("Use selection ({n})", { n: selection.value.length })}</button>
      </div>
    </Modal>
  );
}
