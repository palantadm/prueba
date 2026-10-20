import type { Executor, ExecRt } from "../types";
import { fire } from "../store";
import { Fader } from "./Fader";
import { useState } from "preact/hooks";

/** One executor slot: fader, GO, FLASH, label. `empty` renders an assignable placeholder. */
export function ExecutorStrip(p: {
  slot: number;
  exec?: Executor;
  rt?: ExecRt;
  kindLabel?: string;
  effectOn?: boolean;
  onAssign: () => void;
  onGo?: () => void;
}) {
  const [down, setDown] = useState(false);
  const e = p.exec;
  if (!e || e.kind.type === "empty") {
    return (
      <div class="lv-ex empty">
        <div class="lv-ex-num mono">{p.slot + 1}</div>
        <button class="btn ghost sm" onClick={p.onAssign} aria-label={`Assign executor ${p.slot + 1}`}>+ Assign</button>
      </div>
    );
  }
  const level = p.rt?.level ?? 0;
  const isMaster = e.kind.type === "master";
  const canGo = e.kind.type === "cuelist" || e.kind.type === "effect";
  return (
    <div class={`lv-ex ${p.rt?.active || p.effectOn ? "active" : ""}`}>
      <div class="row" style={{ gap: 4 }}>
        <span class="lv-ex-num mono">{p.slot + 1}</span>
        <button class="lv-ex-label" title="Edit assignment" onClick={p.onAssign}>{e.label || p.kindLabel}</button>
      </div>
      <div class="lv-ex-kind dim">{p.kindLabel}</div>
      <Fader value={level} height={120} width={40} color="var(--accent)"
        ariaLabel={`Executor ${p.slot + 1} level`}
        onChange={(v) => fire({ type: "exec_level", id: e.id, level: v })} />
      <div class="mono lv-ex-lvl">{Math.round(level * 100)}%</div>
      {canGo && <button class={`btn go sm lv-ex-btn`} onClick={p.onGo} aria-label={`Go executor ${p.slot + 1}`}>{e.kind.type === "effect" ? (p.effectOn ? "OFF" : "ON") : "GO"}</button>}
      {!isMaster && (
        <button class={`btn sm lv-ex-btn ${down || p.rt?.flash ? "on" : ""}`}
          onPointerDown={(ev) => { (ev.currentTarget as HTMLElement).setPointerCapture(ev.pointerId); setDown(true); fire({ type: "exec_flash", id: e.id, down: true }); }}
          onPointerUp={() => { setDown(false); fire({ type: "exec_flash", id: e.id, down: false }); }}
          onPointerCancel={() => { setDown(false); fire({ type: "exec_flash", id: e.id, down: false }); }}>FLASH</button>
      )}
    </div>
  );
}
