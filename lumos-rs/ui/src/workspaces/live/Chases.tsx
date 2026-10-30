// Chases pool + chase editor + speed masters. Touch-first: controls ≥ 44 px, nothing depends on hover.
import { useRef, useState } from "preact/hooks";
import "../../styles/chases.css";
import { rt, selection, send, show } from "../../store";
import type { Chase, ChaseDir, ChaseStep, SpeedMaster } from "../../types";
import { Modal } from "../../ui";
import { useLongPress } from "../../lib/hooks";
import { t, tp } from "../../i18n";
import { ConfirmDialog, PromptDialog } from "./dialogs";

const COLORS = ["#3cc4e8", "#35c46a", "#f5a524", "#e5484d", "#e266b8", "#b38cff", "#5b8def", "#f2d98d"];
const DIRS: { id: ChaseDir; label: string }[] = [
  { id: "forward", label: "Forward" }, { id: "backward", label: "Backward" },
  { id: "bounce", label: "Bounce" }, { id: "random", label: "Random" },
];
const clampBpm = (v: number) => Math.max(20, Math.min(240, Math.round(v * 10) / 10));
const chaseRt = (id: number) => rt.value?.chases?.find((c) => c.id === id);
const speedBpm = (m: SpeedMaster) => rt.value?.speed?.find((s) => s.id === m.id)?.bpm ?? m.bpm;
const nextId = (xs: { id: number }[]) => Math.max(0, ...xs.map((x) => x.id)) + 1;

/** Tap-tempo estimator (average of the last taps, reset after 2 s idle). */
export function useTapTempo(onBpm: (bpm: number) => void) {
  const taps = useRef<number[]>([]);
  return () => {
    const now = performance.now();
    const a = taps.current;
    if (a.length && now - a[a.length - 1] > 2000) a.length = 0;
    a.push(now);
    if (a.length > 5) a.shift();
    if (a.length >= 2) onBpm(clampBpm(60000 / ((a[a.length - 1] - a[0]) / (a.length - 1))));
  };
}

/** Programmer values of the selection (or of every fixture holding values when nothing is selected). */
function captureProgrammer(): ChaseStep["values"] | null {
  const prog = rt.value?.programmer ?? {};
  const sel = selection.value;
  const out: ChaseStep["values"] = {};
  for (const [k, v] of Object.entries(prog)) {
    const id = Number(k);
    if (sel.length && !sel.includes(id)) continue;
    if (v && Object.keys(v).length) out[id] = { ...v };
  }
  return Object.keys(out).length ? out : null;
}

function stepSummary(s: ChaseStep): string {
  const parts: string[] = [];
  const nf = Object.keys(s.values).length;
  if (nf) {
    const na = Object.values(s.values).reduce((a, v) => a + Object.keys(v ?? {}).length, 0);
    parts.push(t("{f} fixtures · {a} values", { f: nf, a: na }));
  }
  if (s.scene !== null) parts.push(t("Scene: {name}", { name: show.value?.scenes?.find((x) => x.id === s.scene)?.name ?? `#${s.scene}` }));
  return parts.join(" + ") || t("Empty step");
}

const newStep = (p: Partial<ChaseStep> = {}): ChaseStep => ({ values: {}, refs: {}, scene: null, hold: 1, fade: 0, ...p });

// ── pool ────────────────────────────────────────────────
export function ChasePool() {
  const chases = show.value?.chases ?? [];
  const [edit, setEdit] = useState<Chase | null>(null);
  const [naming, setNaming] = useState(false);
  const canCapture = !!captureProgrammer();
  const blank = (): Chase => ({ id: nextId(chases), name: t("Chase {n}", { n: chases.length + 1 }), steps: [], bpm: 120, speed_master: null, dir: "forward" });
  return (
    <>
      <div class="ch-bar">
        <button class="btn primary" onClick={() => setEdit(blank())}>{t("+ New chase")}</button>
        <button class="btn primary" disabled={!canCapture} onClick={() => setNaming(true)}
          title={t("Create a chase whose first step is the programmer")}>{t("● From programmer")}</button>
      </div>
      <div class="lv-scroll ch-scroll">
        {chases.length === 0 && <div class="dim" style={{ padding: 12 }}>{t("No chases yet. Create one, then add steps from the programmer or from scenes.")}</div>}
        <div class="ch-grid">
          {chases.map((c, i) => <Tile key={c.id} c={c} color={COLORS[i % COLORS.length]} onEdit={() => setEdit(c)} />)}
        </div>
        <SpeedMasters />
      </div>
      {naming && <PromptDialog title={t("New chase from programmer")} label={t("Name")} ok={t("Create")} initial={blank().name}
        onClose={() => setNaming(false)}
        onSubmit={(name) => {
          const values = captureProgrammer();
          if (values) send({ type: "chase_save", chase: { ...blank(), name, steps: [newStep({ values })] } });
        }} />}
      {edit && <ChaseEditor c={edit} exists={chases.some((x) => x.id === edit.id)} onClose={() => setEdit(null)} />}
    </>
  );
}

function Tile({ c, color, onEdit }: { c: Chase; color: string; onEdit: () => void }) {
  const st = chaseRt(c.id);
  const on = st?.running ?? false;
  const { wasLong, ...lp } = useLongPress(onEdit);
  const master = c.speed_master !== null ? show.value?.speed_masters?.find((m) => m.id === c.speed_master) : undefined;
  const bpm = on ? st?.bpm ?? c.bpm : master ? speedBpm(master) : c.bpm;
  return (
    <div class={`ch-tile ${on ? "on" : ""}`} style={{ "--ch": color } as any}>
      <span class="ch-led" />
      <button class="ch-main" {...(lp as any)} aria-pressed={on} aria-label={c.name}
        onClick={() => { if (!wasLong()) send({ type: "chase_run", id: c.id, on: !on }); }}>
        <span class="ch-name">{c.name}</span>
        <span class="ch-sub">
          {on ? t("Step {a}/{b}", { a: (st?.step ?? 0) + 1, b: c.steps.length }) : tp(c.steps.length, "{n} step", "{n} steps")}
          {" · "}{Math.round(bpm)} BPM
        </span>
      </button>
      <button class="ch-menu" aria-label={t("Chase options")} onClick={onEdit}>⋮</button>
      {on && c.steps.length > 0 && (
        <div class="ch-dots" aria-hidden="true">
          {c.steps.slice(0, 32).map((_, i) => <i key={i} class={i === st?.step ? "cur" : ""} />)}
        </div>
      )}
    </div>
  );
}

// ── editor ──────────────────────────────────────────────
function Stepper({ value, set, step, min, max, fmt }: { value: number; set: (v: number) => void; step: number; min: number; max: number; fmt?: (v: number) => string }) {
  const go = (d: number) => set(Math.max(min, Math.min(max, Math.round((value + d) * 1000) / 1000)));
  return (
    <div class="ch-step">
      <button class="btn" aria-label="−" onClick={() => go(-step)}>−</button>
      <span class="val">{fmt ? fmt(value) : value}</span>
      <button class="btn" aria-label="+" onClick={() => go(step)}>+</button>
    </div>
  );
}

function ChaseEditor({ c, exists, onClose }: { c: Chase; exists: boolean; onClose: () => void }) {
  const [d, setD] = useState<Chase>(c);
  const [del, setDel] = useState(false);
  const [pickScene, setPickScene] = useState(false);
  const up = (patch: Partial<Chase>) => setD({ ...d, ...patch });
  const setStep = (i: number, patch: Partial<ChaseStep>) => up({ steps: d.steps.map((s, k) => (k === i ? { ...s, ...patch } : s)) });
  const move = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    if (j < 0 || j >= d.steps.length) return;
    const a = d.steps.slice();
    [a[i], a[j]] = [a[j], a[i]];
    up({ steps: a });
  };
  const tap = useTapTempo((bpm) => up({ bpm, speed_master: null }));
  const st = exists ? chaseRt(c.id) : undefined;
  const masters = show.value?.speed_masters ?? [];
  const scenes = show.value?.scenes ?? [];
  const manualBpm = d.speed_master === null;
  const fromProgrammer = () => {
    const values = captureProgrammer();
    if (values) up({ steps: [...d.steps, newStep({ values })] });
  };
  if (del) return <ConfirmDialog title={t("Delete chase")} text={t("Delete “{name}”?", { name: c.name })} ok={t("Delete")} danger onClose={onClose}
    onYes={() => send({ type: "chase_remove", id: c.id })} />;
  return (
    <Modal title={exists ? t("Chase “{name}”", { name: c.name }) : t("New chase")} onClose={onClose} width={640} footer={<>
      {exists && <button class="btn danger" onClick={() => setDel(true)}>{t("Delete")}</button>}
      <span class="sp" />
      <button class="btn" onClick={onClose}>{t("Cancel")}</button>
      <button class="btn primary" disabled={!d.steps.length} onClick={() => { send({ type: "chase_save", chase: d }); onClose(); }}>{t("Save")}</button>
    </>}>
      {exists && (
        <div class="ch-live">
          <button class={`btn ${st?.running ? "on" : "go"}`} onClick={() => send({ type: "chase_run", id: c.id, on: !st?.running })}>{st?.running ? t("Stop") : t("Run")}</button>
          <button class="btn" aria-label={t("Previous step")} onClick={() => send({ type: "chase_nudge", id: c.id, delta: -1 })}>◀</button>
          <button class="btn" aria-label={t("Next step")} onClick={() => send({ type: "chase_nudge", id: c.id, delta: 1 })}>▶</button>
          <span class="mono grow">{st?.running ? t("Step {a}/{b}", { a: (st.step ?? 0) + 1, b: d.steps.length }) : t("Stopped")}</span>
          <span class="dim" style={{ fontSize: 11 }}>{t("Save to apply the edits below")}</span>
        </div>
      )}
      <div class="field"><label>{t("Name")}</label><input class="input" style={{ minHeight: 44 }} value={d.name} onInput={(e) => up({ name: (e.target as HTMLInputElement).value })} /></div>
      <div class="field"><label>{t("Direction")}</label>
        <div class="ch-seg" role="tablist" aria-label={t("Direction")}>
          {DIRS.map((x) => <button key={x.id} role="tab" aria-selected={d.dir === x.id} class={`btn ${d.dir === x.id ? "sel" : ""}`} onClick={() => up({ dir: x.id })}>{t(x.label)}</button>)}
        </div>
      </div>
      <div class="field"><label>{t("Tempo")}</label>
        <div class="ch-tempo">
          <Stepper value={manualBpm ? d.bpm : masters.find((m) => m.id === d.speed_master) ? speedBpm(masters.find((m) => m.id === d.speed_master)!) : d.bpm}
            set={(v) => manualBpm && up({ bpm: clampBpm(v) })} step={1} min={20} max={240} fmt={(v) => `${Math.round(v * 10) / 10} BPM`} />
          <button class="btn primary ch-tap" disabled={!manualBpm} onPointerDown={tap}>TAP</button>
          <select class="input" style={{ minHeight: 44 }} aria-label={t("Speed master")} value={d.speed_master ?? ""}
            onChange={(e) => { const v = (e.target as HTMLSelectElement).value; up({ speed_master: v === "" ? null : Number(v) }); }}>
            <option value="">{t("Manual BPM")}</option>
            {masters.map((m) => <option key={m.id} value={m.id}>{m.name} · {Math.round(speedBpm(m))} BPM</option>)}
          </select>
        </div>
      </div>
      <div class="field"><label>{tp(d.steps.length, "{n} step", "{n} steps")}</label>
        <div class="ch-steps">
          {d.steps.length === 0 && <div class="dim">{t("No steps. Add the programmer values or a scene as a step.")}</div>}
          {d.steps.map((s, i) => (
            <div key={i} class={`ch-steprow ${st?.running && st.step === i ? "cur" : ""}`}>
              <span class="ch-idx mono">{i + 1}</span>
              <span class="ch-sum grow">{stepSummary(s)}</span>
              <div class="ch-ctl"><span class="ch-k">{t("Beats")}</span>
                <Stepper value={s.hold} set={(v) => setStep(i, { hold: v })} step={0.25} min={0.25} max={64} /></div>
              <div class="ch-ctl"><span class="ch-k">{t("Fade")} {Math.round(s.fade * 100)}%</span>
                <input class="ch-range" type="range" min="0" max="100" value={Math.round(s.fade * 100)} aria-label={t("Fade")}
                  onInput={(e) => setStep(i, { fade: Number((e.target as HTMLInputElement).value) / 100 })} /></div>
              <div class="ch-acts">
                <button class="btn" aria-label={t("Move up")} disabled={i === 0} onClick={() => move(i, -1)}>▲</button>
                <button class="btn" aria-label={t("Move down")} disabled={i === d.steps.length - 1} onClick={() => move(i, 1)}>▼</button>
                <button class="btn" aria-label={t("Duplicate step")} onClick={() => { const a = d.steps.slice(); a.splice(i + 1, 0, JSON.parse(JSON.stringify(s))); up({ steps: a }); }}>⧉</button>
                <button class="btn danger" aria-label={t("Delete step")} onClick={() => up({ steps: d.steps.filter((_, k) => k !== i) })}>✕</button>
              </div>
            </div>
          ))}
        </div>
        <div class="ch-add">
          <button class="btn" disabled={!captureProgrammer()} onClick={fromProgrammer}>{t("+ Step from programmer")}</button>
          <button class="btn" disabled={!scenes.length} onClick={() => setPickScene(true)}>{t("+ Step from scene")}</button>
          <button class="btn" onClick={() => up({ steps: [...d.steps, newStep()] })}>{t("+ Empty step")}</button>
        </div>
      </div>
      {pickScene && (
        <Modal title={t("Pick a scene")} onClose={() => setPickScene(false)}>
          <div class="ch-pick">
            {scenes.map((s) => (
              <button key={s.id} class="btn" style={{ minHeight: 48, justifyContent: "flex-start" }}
                onClick={() => { up({ steps: [...d.steps, newStep({ scene: s.id })] }); setPickScene(false); }}>{s.name}</button>
            ))}
          </div>
        </Modal>
      )}
    </Modal>
  );
}

// ── speed masters ───────────────────────────────────────
export function SpeedMasters() {
  const masters = show.value?.speed_masters ?? [];
  const [edit, setEdit] = useState<SpeedMaster | null>(null);
  const add = () => send({ type: "speed_save", master: { id: nextId(masters), name: t("Master {n}", { n: masters.length + 1 }), bpm: 120 } });
  return (
    <div class="ch-speed">
      <div class="ch-speed-h">
        <h4>{t("Speed masters")}</h4>
        <span class="sp" />
        <button class="btn" onClick={add}>{t("+ Master")}</button>
      </div>
      {masters.length === 0 && <div class="dim">{t("A speed master sets one BPM for several chases and effects. Add one and tap the tempo.")}</div>}
      {masters.map((m) => <MasterRow key={m.id} m={m} onEdit={() => setEdit(m)} />)}
      {edit && <MasterEdit m={edit} onClose={() => setEdit(null)} />}
    </div>
  );
}

function MasterRow({ m, onEdit }: { m: SpeedMaster; onEdit: () => void }) {
  const bpm = speedBpm(m);
  const set = (v: number) => send({ type: "speed_set", id: m.id, bpm: clampBpm(v) });
  return (
    <div class="ch-master">
      <button class="ch-mname" onClick={onEdit} aria-label={t("Edit {name}", { name: m.name })}>{m.name}</button>
      <button class="ch-bpm mono" onClick={onEdit} aria-label={t("BPM")}>{Math.round(bpm * 10) / 10}<small>BPM</small></button>
      <div class="ch-mbtns">
        <button class="btn" onClick={() => set(bpm / 2)} aria-label={t("Half speed")}>½×</button>
        <button class="btn" onClick={() => set(bpm - 1)} aria-label="−1">−</button>
        <button class="btn" onClick={() => set(bpm + 1)} aria-label="+1">+</button>
        <button class="btn" onClick={() => set(bpm * 2)} aria-label={t("Double speed")}>2×</button>
        <button class="btn primary ch-tap" onPointerDown={() => send({ type: "speed_tap", id: m.id })}>TAP</button>
      </div>
    </div>
  );
}

function MasterEdit({ m, onClose }: { m: SpeedMaster; onClose: () => void }) {
  const [name, setName] = useState(m.name);
  const [bpm, setBpm] = useState(String(Math.round(speedBpm(m) * 10) / 10));
  const [del, setDel] = useState(false);
  if (del) return <ConfirmDialog title={t("Delete speed master")} text={t("Delete “{name}”?", { name: m.name })} ok={t("Delete")} danger onClose={onClose}
    onYes={() => send({ type: "speed_remove", id: m.id })} />;
  const v = parseFloat(bpm);
  return (
    <Modal title={t("Speed master")} onClose={onClose} footer={<>
      <button class="btn danger" onClick={() => setDel(true)}>{t("Delete")}</button><span class="sp" />
      <button class="btn" onClick={onClose}>{t("Cancel")}</button>
      <button class="btn primary" disabled={!Number.isFinite(v)} onClick={() => { send({ type: "speed_save", master: { ...m, name: name.trim() || m.name, bpm: clampBpm(v) } }); onClose(); }}>{t("Save")}</button>
    </>}>
      <div class="field"><label>{t("Name")}</label><input class="input" style={{ minHeight: 44 }} value={name} onInput={(e) => setName((e.target as HTMLInputElement).value)} /></div>
      <div class="field"><label>BPM (20–240)</label><input class="input num" style={{ minHeight: 44 }} type="number" min="20" max="240" step="0.5" value={bpm} onInput={(e) => setBpm((e.target as HTMLInputElement).value)} /></div>
    </Modal>
  );
}
