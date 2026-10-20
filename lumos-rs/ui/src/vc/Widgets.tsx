// Play-mode renderers for virtual console widgets. They only send Vc* commands and render `rt.vc` state.
import { computed } from "@preact/signals";
import { useEffect, useRef, useState } from "preact/hooks";
import { fire, rt } from "../store";
import type { VcWidget } from "../types";
import { Fader } from "../widgets/Fader";
import { Knob } from "../widgets/Knob";
import { XYPad } from "../widgets/XYPad";
import { pageId } from "./editor";

export const vcState = computed(() => new Map((rt.value?.vc ?? []).map((s) => [s.id, s])));

/** Rate-limit continuous sends to ~30/s, always delivering the final value. */
function useThrottled(send: (v: number) => void, ms = 33) {
  const last = useRef(0), pend = useRef<number | null>(null), timer = useRef(0);
  return (v: number) => {
    const now = performance.now();
    if (now - last.current >= ms) { last.current = now; send(v); pend.current = null; return; }
    pend.current = v;
    if (!timer.current) timer.current = window.setTimeout(() => { timer.current = 0; if (pend.current !== null) { last.current = performance.now(); send(pend.current); pend.current = null; } }, ms);
  };
}

type Ctx = { w: VcWidget; interactive: boolean; px: { w: number; h: number } };

function ButtonW({ w, interactive }: Ctx) {
  const st = vcState.value.get(w.id);
  const [held, setHeld] = useState(false);
  const active = (st?.active ?? false) || held;
  const down = (e: PointerEvent) => {
    if (!interactive) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    setHeld(true);
    if (w.action.type === "page") pageId.value = w.action.page;
    fire({ type: "vc_press", widget: w.id, down: true });
  };
  const up = () => { if (!held) return; setHeld(false); fire({ type: "vc_press", widget: w.id, down: false }); };
  return (
    <button class={`vc-btn ${active ? "on" : ""} ${held ? "held" : ""}`} tabIndex={interactive ? 0 : -1} aria-pressed={active}
      onPointerDown={down} onPointerUp={up} onPointerCancel={up}
      onKeyDown={(e) => { if (interactive && (e.key === " " || e.key === "Enter") && !e.repeat) { e.preventDefault(); setHeld(true); fire({ type: "vc_press", widget: w.id, down: true }); } }}
      onKeyUp={(e) => { if (interactive && (e.key === " " || e.key === "Enter")) up(); }}>
      <span class="vc-btn-led" aria-hidden="true" />
      <span class="vc-btn-label">{w.label}</span>
      {w.mode === "toggle" && <span class="vc-btn-mode">TOGGLE</span>}
    </button>
  );
}

function SliderW({ w, interactive, px }: Ctx) {
  const st = vcState.value.get(w.id);
  const v = st?.value ?? 0;
  const send = useThrottled((x) => fire({ type: "vc_value", widget: w.id, value: x }));
  const horizontal = w.orientation === "horizontal";
  const trackLen = Math.max(40, (horizontal ? px.w : px.h) - 52);
  return (
    <div class={`vc-slider ${horizontal ? "h" : ""}`}>
      <div class="vc-slider-val mono">{Math.round(v * 100)}%</div>
      <Fader value={v} onChange={send} horizontal={horizontal} color={w.color} disabled={!interactive}
        height={horizontal ? 36 : trackLen} width={horizontal ? trackLen : Math.max(36, Math.min(64, px.w - 12))} ariaLabel={w.label} defaultValue={0} />
      <div class="vc-label">{w.label}</div>
    </div>
  );
}

function KnobW({ w, interactive, px }: Ctx) {
  const st = vcState.value.get(w.id);
  const send = useThrottled((x) => fire({ type: "vc_value", widget: w.id, value: x }));
  return (
    <div class="vc-knob" style={{ pointerEvents: interactive ? "auto" : "none" }}>
      <Knob value={st?.value ?? 0} onChange={send} label={w.label} color={w.color} size={Math.max(40, Math.min(px.w, px.h - 22) - 10)} format={(x) => `${Math.round(x * 100)}%`} />
    </div>
  );
}

const xyLocal = new Map<number, [number, number]>();
function XyW({ w, interactive }: Ctx) {
  const [xy, setXy] = useState<[number, number]>(xyLocal.get(w.id) ?? [0.5, 0.5]);
  const last = useRef(0);
  return (
    <div class="vc-xy" style={{ pointerEvents: interactive ? "auto" : "none" }}>
      <XYPad x={xy[0]} y={xy[1]} color={w.color} labelX={w.action.type === "xy" ? w.action.x_attr : "X"} labelY={w.action.type === "xy" ? w.action.y_attr : "Y"}
        onChange={(x, y) => {
          setXy([x, y]); xyLocal.set(w.id, [x, y]);
          const n = performance.now(); if (n - last.current < 33) return; last.current = n;
          fire({ type: "vc_xy", widget: w.id, x, y });
        }}
        onEnd={() => { const [x, y] = xyLocal.get(w.id) ?? xy; fire({ type: "vc_xy", widget: w.id, x, y }); }} />
      <div class="vc-label">{w.label}</div>
    </div>
  );
}

function CueListW({ w, interactive }: Ctx) {
  const id = w.action.type === "cuelist" ? w.action.cuelist : 0;
  const c = rt.value?.cuelists.find((x) => x.id === id);
  const act = (cmd: "go" | "back" | "pause") => interactive && id && fire({ type: cmd, cuelist: id });
  return (
    <div class="vc-cl">
      <div class="row"><b class="grow vc-cl-title">{w.label}</b><span class={`chip ${c?.state === "running" ? "ok" : ""}`}>{c?.state ?? "—"}</span></div>
      <div class="row vc-cl-cues">
        <div><div class="vc-cap">CURRENT</div><div class="vc-cl-num">{c?.current ?? "–"}</div></div>
        <div><div class="vc-cap">NEXT</div><div class="vc-cl-num dim">{c?.next ?? "–"}</div></div>
        <div class="grow" />
      </div>
      <div class="vc-cl-prog"><i style={{ width: `${Math.round((c?.progress ?? 0) * 100)}%`, background: w.color }} /></div>
      <div class="row vc-cl-btns">
        <button class="btn go grow" tabIndex={interactive ? 0 : -1} onPointerDown={() => act("go")}>GO</button>
        <button class="btn grow" tabIndex={interactive ? 0 : -1} onPointerDown={() => act("back")}>BACK</button>
        <button class="btn" tabIndex={interactive ? 0 : -1} onPointerDown={() => act("pause")}>⏸</button>
      </div>
    </div>
  );
}

function SpeedW({ w, interactive }: Ctx) {
  const st = vcState.value.get(w.id);
  const taps = useRef<number[]>([]);
  const hz = w.out_min + (st?.value ?? 0) * (w.out_max - w.out_min);
  const setHz = (h: number) => fire({ type: "vc_value", widget: w.id, value: Math.max(0, Math.min(1, (h - w.out_min) / ((w.out_max - w.out_min) || 1))) });
  const tap = () => {
    if (!interactive) return;
    const n = performance.now();
    if (taps.current.length && n - taps.current[taps.current.length - 1] > 2000) taps.current = [];
    taps.current.push(n); taps.current = taps.current.slice(-5);
    if (taps.current.length >= 2) {
      const iv = (taps.current[taps.current.length - 1] - taps.current[0]) / (taps.current.length - 1);
      setHz(1000 / iv);
    }
  };
  return (
    <div class="vc-speed">
      <div class="vc-cap">{w.label}</div>
      <div class="vc-speed-bpm mono">{Math.round(hz * 60)}<small> BPM</small></div>
      <div class="dim mono" style={{ fontSize: "var(--fs-xs)" }}>{hz.toFixed(2)} Hz</div>
      <button class="btn lg vc-tap" tabIndex={interactive ? 0 : -1} onPointerDown={tap}>TAP</button>
      <div class="row">
        <button class="btn sm grow" tabIndex={interactive ? 0 : -1} onClick={() => interactive && setHz(hz / 2)}>½×</button>
        <button class="btn sm grow" tabIndex={interactive ? 0 : -1} onClick={() => interactive && setHz(hz * 2)}>2×</button>
      </div>
    </div>
  );
}

function ClockW({ w }: Ctx) {
  const [now, setNow] = useState(new Date());
  useEffect(() => { const t = setInterval(() => setNow(new Date()), 500); return () => clearInterval(t); }, []);
  const tc = rt.value?.timecode;
  return (
    <div class="vc-clock">
      <div class="mono vc-clock-big">{tc?.running ? tc.text : now.toLocaleTimeString([], { hour12: false })}</div>
      <div class="vc-cap">{tc?.running ? "TIMECODE" : w.label}</div>
    </div>
  );
}

export function WidgetBody(p: Ctx) {
  const { w } = p;
  switch (w.kind) {
    case "button": return <ButtonW {...p} />;
    case "slider": return <SliderW {...p} />;
    case "knob": return <KnobW {...p} />;
    case "xy_pad": return <XyW {...p} />;
    case "cue_list": return <CueListW {...p} />;
    case "speed_dial": return <SpeedW {...p} />;
    case "clock": return <ClockW {...p} />;
    case "label": return <div class="vc-text">{w.label}</div>;
    case "frame": return <div class="vc-frame-title">{w.label}</div>;
  }
}
