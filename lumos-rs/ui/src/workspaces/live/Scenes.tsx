// Scenes pool: record looks, recall them with fades, solo, level faders and an A/B morph crossfader.
// Touch-first: every control is ≥ 44 px and nothing depends on hover.
import { useEffect, useRef, useState } from "preact/hooks";
import "../../styles/scenes.css";
import { fire, rt, selection, send, show } from "../../store";
import type { Scene } from "../../types";
import { Modal } from "../../ui";
import { Crossfader, Fader } from "../../widgets";
import { useLongPress } from "../../lib/hooks";
import { t, tp } from "../../i18n";
import { ConfirmDialog, PromptDialog } from "./dialogs";

const COLORS = ["#3cc4e8", "#35c46a", "#f5a524", "#e5484d", "#e266b8", "#b38cff", "#5b8def", "#f2d98d"];
const colorOf = (s: Scene) => s.color || COLORS[0];
const stateOf = (id: number) => rt.value?.scenes?.find((x) => x.id === id);

/** Fixtures a recording applies to: the selection, or everything (`null`). */
const scope = (): number[] | null => (selection.value.length ? [...selection.value] : null);

/** How many fixtures in scope currently hold programmer values. */
function heldCount(): number {
  const prog = rt.value?.programmer ?? {};
  const ids = selection.value.length ? selection.value : Object.keys(prog).map(Number);
  return ids.filter((id) => prog[id] && Object.keys(prog[id]!).length).length;
}

export function ScenePool() {
  const scenes = show.value?.scenes ?? [];
  const [solo, setSolo] = useState(false);
  const [view, setView] = useState<"tiles" | "faders">("tiles");
  const [naming, setNaming] = useState<"programmer" | "output" | null>(null);
  const [edit, setEdit] = useState<Scene | null>(null);
  const held = heldCount();
  const anyActive = (rt.value?.scenes ?? []).some((s) => s.active);
  const n = selection.value.length;
  const nextName = t("Scene {n}", { n: Math.max(0, ...scenes.map((s) => s.id)) + 1 });

  return (
    <>
      <div class="sc-bar">
        <button class="btn primary" disabled={!held} onClick={() => setNaming("programmer")}
          title={t("Record a scene from the programmer values")}>{t("● From programmer")}</button>
        <button class="btn primary" onClick={() => setNaming("output")}
          title={t("Record a scene from what is on stage now")}>{t("● From output")}</button>
        <span class="sp" />
        <button class={`btn ${solo ? "on" : ""}`} aria-pressed={solo} onClick={() => setSolo(!solo)}
          title={t("Tapping a scene activates it and releases all the others")}>{t("Solo")}</button>
        <button class="btn danger" disabled={!anyActive} onClick={() => send({ type: "scenes_release" })}>{t("Release scenes")}</button>
      </div>
      <div class="sc-bar" style={{ paddingTop: 0 }}>
        <span class="sc-scope">{n ? tp(n, "Records {n} selected fixture", "Records {n} selected fixtures") : t("Records all fixtures")}</span>
        <span class="sp" />
        <div role="tablist" aria-label={t("Scene view")} class="row" style={{ gap: 4 }}>
          <button role="tab" aria-selected={view === "tiles"} class={`btn ${view === "tiles" ? "sel" : ""}`} onClick={() => setView("tiles")}>{t("Tiles")}</button>
          <button role="tab" aria-selected={view === "faders"} class={`btn ${view === "faders" ? "sel" : ""}`} onClick={() => setView("faders")}>{t("Faders")}</button>
        </div>
      </div>
      <div class="lv-scroll">
        {scenes.length === 0 && <div class="dim" style={{ padding: 12 }}>{t("No scenes yet. Set a look, then record it with “From programmer” or “From output”.")}</div>}
        {view === "tiles" ? (
          <div class="sc-grid">
            {scenes.map((s) => <Tile key={s.id} s={s} solo={solo} onEdit={() => setEdit(s)} />)}
          </div>
        ) : (
          <div class="sc-faders">
            {scenes.map((s) => {
              const st = stateOf(s.id);
              return (
                <div class="sc-fader" key={s.id} style={{ "--sc": colorOf(s) } as any}>
                  <Fader value={st?.level ?? 0} height={170} width={44} color={colorOf(s)} ariaLabel={t("{name} level", { name: s.name })}
                    onChange={(v) => fire({ type: "scene_level", id: s.id, level: v })} />
                  <div class="mono">{Math.round((st?.level ?? 0) * 100)}%</div>
                  <button class={`btn ${st?.active ? "on" : ""}`} aria-pressed={st?.active ?? false}
                    onClick={() => send(solo ? { type: "scene_solo", id: s.id } : { type: "scene_go", id: s.id, on: !st?.active })}>{st?.active ? t("Off") : t("On")}</button>
                  <button class="btn ghost nm" onClick={() => setEdit(s)} aria-label={t("Scene options")}>{s.name}</button>
                </div>
              );
            })}
          </div>
        )}
        <MorphPanel scenes={scenes} />
      </div>
      {naming && <PromptDialog title={naming === "programmer" ? t("Record scene from programmer") : t("Record scene from output")}
        label={t("Name")} ok={t("Record")} initial={nextName} onClose={() => setNaming(null)}
        extra={<div class="dim">{n ? tp(n, "Records {n} selected fixture", "Records {n} selected fixtures") : t("Records all fixtures")}</div>}
        onSubmit={(name) => send({ type: "scene_record", id: null, name, from_output: naming === "output", fixtures: scope(), merge: false })} />}
      {edit && <SceneEdit s={edit} onClose={() => setEdit(null)} />}
    </>
  );
}

function Tile({ s, solo, onEdit }: { s: Scene; solo: boolean; onEdit: () => void }) {
  const st = stateOf(s.id);
  const level = st?.level ?? 0;
  const on = st?.active ?? false;
  const { wasLong, ...lp } = useLongPress(onEdit);
  return (
    <div class={`sc-tile ${on ? "on" : ""}`} style={{ "--sc": colorOf(s) } as any}>
      <span class="sc-led" />
      <button class="sc-main" {...(lp as any)} aria-pressed={on} aria-label={s.name}
        onClick={() => {
          if (wasLong()) return;
          send(solo ? { type: "scene_solo", id: s.id } : { type: "scene_go", id: s.id, on: !on });
        }}>
        <span class="sc-name">{s.name}</span>
        <span class="sc-sub">{on || level > 0 ? `${Math.round(level * 100)}%` : `↑${s.fade_in}s  ↓${s.fade_out}s`}</span>
      </button>
      <button class="sc-menu" aria-label={t("Scene options")} onClick={onEdit}>⋮</button>
      <div class="sc-bar-lvl"><i style={{ width: `${level * 100}%` }} /></div>
    </div>
  );
}

function Stepper({ label, value, onChange }: { label: string; value: number; onChange: (v: number) => void }) {
  const set = (v: number) => onChange(Math.max(0, Math.min(600, Math.round(v * 10) / 10)));
  return (
    <div class="field">
      <label>{label}</label>
      <div class="sc-step">
        <button class="btn" aria-label={t("Less")} onClick={() => set(value - (value > 10 ? 5 : 0.5))}>−</button>
        <span class="val">{value.toFixed(1)} s</span>
        <button class="btn" aria-label={t("More")} onClick={() => set(value + (value >= 10 ? 5 : 0.5))}>+</button>
      </div>
    </div>
  );
}

function SceneEdit({ s, onClose }: { s: Scene; onClose: () => void }) {
  const [name, setName] = useState(s.name);
  const [color, setColor] = useState(colorOf(s));
  const [fi, setFi] = useState(s.fade_in);
  const [fo, setFo] = useState(s.fade_out);
  const [del, setDel] = useState(false);
  const held = heldCount();
  const n = selection.value.length;
  if (del) return <ConfirmDialog title={t("Delete scene")} text={t("Delete “{name}”?", { name: s.name })} ok={t("Delete")} danger onClose={onClose}
    onYes={() => send({ type: "scene_remove", id: s.id })} />;
  const nm = name.trim() || s.name;
  const update = (fromOutput: boolean, merge: boolean) => {
    send({ type: "scene_record", id: s.id, name: nm, from_output: fromOutput, fixtures: scope(), merge });
    onClose();
  };
  const dup = () => {
    const id = Math.max(0, ...(show.value?.scenes ?? []).map((x) => x.id)) + 1;
    send({ type: "scene_save", scene: { ...s, id, name: t("{name} (copy)", { name: s.name }) } });
    onClose();
  };
  return (
    <Modal title={t("Scene “{name}”", { name: s.name })} onClose={onClose} footer={<>
      <button class="btn danger" style={{ minHeight: 48 }} onClick={() => setDel(true)}>{t("Delete")}</button><span class="sp" />
      <button class="btn" style={{ minHeight: 48 }} onClick={onClose}>{t("Cancel")}</button>
      <button class="btn primary" style={{ minHeight: 48 }} onClick={() => { send({ type: "scene_save", scene: { ...s, name: nm, color, fade_in: fi, fade_out: fo } }); onClose(); }}>{t("Save")}</button>
    </>}>
      <div class="field"><label>{t("Name")}</label>
        <input class="input" style={{ minHeight: 44 }} value={name} onInput={(e) => setName((e.target as HTMLInputElement).value)} aria-label={t("Name")} /></div>
      <div class="field"><label>{t("Color")}</label>
        <div class="sc-colors" role="radiogroup" aria-label={t("Color")}>
          {COLORS.map((c) => <button key={c} role="radio" aria-checked={c === color} class={c === color ? "sel" : ""} style={{ background: c }} aria-label={c} onClick={() => setColor(c)} />)}
        </div></div>
      <Stepper label={t("Fade in")} value={fi} onChange={setFi} />
      <Stepper label={t("Fade out")} value={fo} onChange={setFo} />
      <div class="field"><label>{n ? tp(n, "Update from {n} selected fixture", "Update from {n} selected fixtures") : t("Update from all fixtures")}</label>
        <div class="sc-actions">
          <button class="btn" disabled={!held} onClick={() => update(false, true)}>{t("Programmer · merge")}</button>
          <button class="btn" disabled={!held} onClick={() => update(false, false)}>{t("Programmer · replace")}</button>
          <button class="btn" onClick={() => update(true, true)}>{t("Output · merge")}</button>
          <button class="btn" onClick={() => update(true, false)}>{t("Output · replace")}</button>
        </div></div>
      <div class="sc-actions">
        <button class="btn" onClick={() => { send({ type: "scene_solo", id: s.id }); onClose(); }}>{t("Solo")}</button>
        <button class="btn" onClick={dup}>{t("Duplicate")}</button>
      </div>
    </Modal>
  );
}

// ── A/B morph ───────────────────────────────────────────
function MorphPanel({ scenes }: { scenes: Scene[] }) {
  const m = rt.value?.morph ?? null;
  const [a, setA] = useState(0);
  const [b, setB] = useState(0);
  const [tv, setT] = useState(0);
  const dragging = useRef(false);
  const last = useRef(0);

  // keep the A/B choice valid as scenes come and go
  useEffect(() => {
    const ids = scenes.map((s) => s.id);
    if (!ids.includes(a)) setA(ids[0] ?? 0);
    if (!ids.includes(b)) setB(ids[1] ?? ids[0] ?? 0);
  }, [scenes.map((s) => s.id).join(",")]);
  // follow the engine (another client, MIDI, VC slider…)
  useEffect(() => {
    if (m && !dragging.current) { setA(m[0]); setB(m[1]); setT(m[2]); }
  }, [m?.[0], m?.[1], m?.[2]]);

  const ok = scenes.length >= 2 && a !== b && scenes.some((s) => s.id === a) && scenes.some((s) => s.id === b);
  const push = (v: number, na = a, nb = b, force = false) => {
    setT(v);
    const now = performance.now();
    if (force || now - last.current >= 33) { last.current = now; fire({ type: "scene_morph", a: na, b: nb, t: v }); }
  };
  const pick = (which: "a" | "b", id: number) => {
    const na = which === "a" ? id : a, nb = which === "b" ? id : b;
    which === "a" ? setA(id) : setB(id);
    if (m && na !== nb) fire({ type: "scene_morph", a: na, b: nb, t: tv });
  };
  const chips = (which: "a" | "b") => scenes.map((s) => {
    const sel = (which === "a" ? a : b) === s.id;
    return <button key={s.id} class={`btn ${sel ? "sel" : ""}`} aria-pressed={sel} style={{ borderLeft: `6px solid ${colorOf(s)}` }} onClick={() => pick(which, s.id)}>{s.name}</button>;
  });
  if (scenes.length < 2) return (
    <div class="sc-morph"><h4>{t("Morph")}</h4><div class="dim">{t("Record at least two scenes to morph between them.")}</div></div>
  );
  return (
    <div class="sc-morph">
      <h4>{t("Morph A → B")}</h4>
      <div class="sc-chips"><span class="lbl">A</span>{chips("a")}</div>
      <div class="sc-chips"><span class="lbl">B</span>{chips("b")}</div>
      <div class="row" style={{ gap: 8 }}>
        <div class="grow">
          <Crossfader value={tv} disabled={!ok} ariaLabel={t("Morph position")} color={scenes.find((s) => s.id === b)?.color || undefined}
            leftLabel="A" rightLabel="B"
            onChange={(v) => { dragging.current = true; push(v); }}
            onEnd={() => { dragging.current = false; push(tv, a, b, true); }} />
        </div>
        <span class="sc-readout">{Math.round(tv * 100)}%</span>
      </div>
      <div class="sc-chips">
        <button class="btn" disabled={!ok} onClick={() => push(0, a, b, true)}>A</button>
        <button class="btn" disabled={!ok} onClick={() => push(Math.max(0, tv - 0.1), a, b, true)}>−10%</button>
        <button class="btn" disabled={!ok} onClick={() => push(Math.min(1, tv + 0.1), a, b, true)}>+10%</button>
        <button class="btn" disabled={!ok} onClick={() => push(1, a, b, true)}>B</button>
        <span class="sp" />
        <button class="btn danger" disabled={!m} onClick={() => { send({ type: "scene_morph", a, b, t: null }); }}>{t("Release morph")}</button>
      </div>
      {!ok && <div class="dim">{t("Pick two different scenes.")}</div>}
    </div>
  );
}
