import type { Executor, ExecRt } from "../types";
import { fire, rt as liveRt } from "../store";
import { VFader } from "./SlimFader";
import { useState } from "preact/hooks";
import { t } from "../i18n";

/** Accent per executor kind (matches the Cues board): cuelist green, scene cyan, chase/effect violet, speed amber, group intensity. */
export const KIND_COLOR: Record<string, string> = {
  cuelist: "var(--green)", x_fade: "var(--green)", scene: "var(--cyan)", chase: "var(--violet)", effect: "var(--violet)",
  speed: "var(--amber)", group: "var(--attr-intensity)", channel: "var(--attr-color)", master: "var(--amber)",
};

/** One executor slot: header (assign), vertical fader, GO + FLASH. `empty` renders an assignable placeholder. */
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
      <button class="cu-ex empty" onClick={p.onAssign} aria-label={t("Assign executor {n}", { n: p.slot + 1 })}>
        <span class="num mono">{p.slot + 1}</span>
        <span class="plus">{t("+ Assign")}</span>
      </button>
    );
  }
  const level = p.rt?.level ?? 0;
  const isMaster = e.kind.type === "master";
  const isScene = e.kind.type === "scene";
  const isChase = e.kind.type === "chase";
  const isSpeed = e.kind.type === "speed";
  const canGo = e.kind.type === "cuelist" || e.kind.type === "x_fade" || e.kind.type === "effect" || isScene || isChase || isSpeed;
  const toggled = isScene || isChase ? !!p.rt?.active : !!p.effectOn;
  const bpm = isSpeed ? liveRt.value?.speed?.find((m) => m.id === (e.kind as { master: number }).master)?.bpm : undefined;
  const color = KIND_COLOR[e.kind.type] ?? "var(--amber)";
  const active = !!(p.rt?.active || p.effectOn);
  const flashable = !isMaster && !isSpeed && !isChase;
  const goLabel = isSpeed ? "TAP" : e.kind.type === "effect" || isScene || isChase ? (toggled ? "OFF" : "ON") : "GO";
  return (
    <div class={`cu-ex ${active ? "active" : ""}`} style={{ "--kc": color } as any}>
      <button class="cu-ex-head" onClick={p.onAssign} title={t("Edit assignment")} aria-label={`${t("Edit assignment")}: ${p.slot + 1}`}>
        <span class="top">
          <span class="num mono">{p.slot + 1}</span><i class="kdot" aria-hidden="true" /><span class="sp" />
          <span class="lvl mono">{bpm !== undefined ? `${Math.round(bpm)}` : `${Math.round(level * 100)}%`}</span>
        </span>
        <span class="nm">{e.label || p.kindLabel}</span>
      </button>
      <div class="cu-ex-fader">
        <VFader value={level} color={color} ariaLabel={t("Executor {n} level", { n: p.slot + 1 })}
          onChange={(v) => fire({ type: "exec_level", id: e.id, level: v })} />
      </div>
      <div class="cu-ex-btns">
        {canGo && <button class={`btn go ${active && !isSpeed ? "lit" : ""}`} onClick={p.onGo} aria-label={t("Go executor {n}", { n: p.slot + 1 })}>{goLabel}</button>}
        {flashable && (
          <button class={`btn ${down || p.rt?.flash ? "on" : ""}`}
            onPointerDown={(ev) => { (ev.currentTarget as HTMLElement).setPointerCapture(ev.pointerId); setDown(true); fire({ type: "exec_flash", id: e.id, down: true }); }}
            onPointerUp={() => { setDown(false); fire({ type: "exec_flash", id: e.id, down: false }); }}
            onPointerCancel={() => { setDown(false); fire({ type: "exec_flash", id: e.id, down: false }); }}>FLASH</button>
        )}
      </div>
    </div>
  );
}
