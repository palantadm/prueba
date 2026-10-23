import type { Executor, ExecRt } from "../types";
import { fire } from "../store";
import { Fader } from "./Fader";
import { useState } from "preact/hooks";
import { t } from "../i18n";

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
        <button class="btn ghost sm" onClick={p.onAssign} aria-label={t("Assign executor {n}", { n: p.slot + 1 })}>{t("+ Assign")}</button>
      </div>
    );
  }
  const level = p.rt?.level ?? 0;
  const isMaster = e.kind.type === "master";
  const isScene = e.kind.type === "scene";
  const canGo = e.kind.type === "cuelist" || e.kind.type === "xfade" || e.kind.type === "effect" || isScene;
  const toggled = isScene ? !!p.rt?.active : !!p.effectOn;
  return (
    <div class={`lv-ex ${p.rt?.active || p.effectOn ? "active" : ""}`}>
      <div class="row" style={{ gap: 4 }}>
        <span class="lv-ex-num mono">{p.slot + 1}</span>
        <button class="lv-ex-label" title={t("Edit assignment")} onClick={p.onAssign}>{e.label || p.kindLabel}</button>
      </div>
      <div class="lv-ex-kind dim">{p.kindLabel}</div>
      <Fader value={level} height={120} width={40} color="var(--accent)"
        ariaLabel={t("Executor {n} level", { n: p.slot + 1 })}
        onChange={(v) => fire({ type: "exec_level", id: e.id, level: v })} />
      <div class="mono lv-ex-lvl">{Math.round(level * 100)}%</div>
      {canGo && <button class={`btn go sm lv-ex-btn`} onClick={p.onGo} aria-label={t("Go executor {n}", { n: p.slot + 1 })}>{e.kind.type === "effect" || isScene ? (toggled ? "OFF" : "ON") : "GO"}</button>}
      {!isMaster && (
        <button class={`btn sm lv-ex-btn ${down || p.rt?.flash ? "on" : ""}`}
          onPointerDown={(ev) => { (ev.currentTarget as HTMLElement).setPointerCapture(ev.pointerId); setDown(true); fire({ type: "exec_flash", id: e.id, down: true }); }}
          onPointerUp={() => { setDown(false); fire({ type: "exec_flash", id: e.id, down: false }); }}
          onPointerCancel={() => { setDown(false); fire({ type: "exec_flash", id: e.id, down: false }); }}>FLASH</button>
      )}
    </div>
  );
}
