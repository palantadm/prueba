import { useRef, useState } from "preact/hooks";
import { t } from "../i18n";

export type KnobProps = {
  value: number;
  onChange: (v: number) => void;
  onEnd?: () => void;
  label: string;
  color?: string;
  held?: boolean;
  defaultValue?: number;
  format?: (v: number) => string;
  onRelease?: () => void;
  size?: number;
};

const A0 = 135, SWEEP = 270;
function arc(cx: number, cy: number, r: number, from: number, to: number) {
  const p = (a: number) => [cx + r * Math.cos((a * Math.PI) / 180), cy + r * Math.sin((a * Math.PI) / 180)];
  const [x0, y0] = p(from), [x1, y1] = p(to);
  return `M${x0.toFixed(2)} ${y0.toFixed(2)} A${r} ${r} 0 ${to - from > 180 ? 1 : 0} 1 ${x1.toFixed(2)} ${y1.toFixed(2)}`;
}

/** Rotary encoder: drag vertically (shift = fine), wheel ±1 %, double-click resets. */
export function Knob(p: KnobProps) {
  const [drag, setDrag] = useState<number | null>(null);
  const st = useRef({ y: 0, v: 0 });
  const v = drag ?? p.value;
  const size = p.size ?? 56, c = size / 2, r = c - 6;
  const color = p.held ? "var(--live)" : p.color ?? "var(--accent)";
  const clamp = (x: number) => Math.max(0, Math.min(1, x));
  const down = (e: PointerEvent) => {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    st.current = { y: e.clientY, v };
    setDrag(v);
  };
  const move = (e: PointerEvent) => {
    if (drag === null) return;
    const nv = clamp(st.current.v + ((st.current.y - e.clientY) / (e.shiftKey ? 1200 : 220)));
    setDrag(nv);
    p.onChange(nv);
  };
  const up = () => { if (drag !== null) { setDrag(null); p.onEnd?.(); } };
  const wheel = (e: WheelEvent) => {
    e.preventDefault();
    p.onChange(clamp(v + (e.deltaY < 0 ? 1 : -1) * (e.shiftKey ? 0.1 : 0.01)));
  };
  const key = (e: KeyboardEvent) => {
    const s = e.shiftKey ? 0.1 : 0.01;
    if (e.key === "ArrowUp" || e.key === "ArrowRight") { p.onChange(clamp(v + s)); e.preventDefault(); }
    if (e.key === "ArrowDown" || e.key === "ArrowLeft") { p.onChange(clamp(v - s)); e.preventDefault(); }
  };
  const ang = A0 + SWEEP * v;
  return (
    <div class={`lv-knob ${p.held ? "held" : ""}`} style={{ width: size + 16 }}>
      <svg
        width={size} height={size} viewBox={`0 0 ${size} ${size}`}
        role="slider" tabIndex={0} aria-label={p.label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(v * 100)}
        onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up}
        onWheel={wheel} onKeyDown={key}
        onDblClick={() => { p.onChange(p.defaultValue ?? 0); p.onEnd?.(); }}
      >
        <path d={arc(c, c, r, A0, A0 + SWEEP)} stroke="var(--bg-4)" stroke-width="5" fill="none" stroke-linecap="round" />
        {v > 0.002 && <path d={arc(c, c, r, A0, ang)} stroke={color} stroke-width="5" fill="none" stroke-linecap="round" />}
        <line x1={c + (r - 12) * Math.cos((ang * Math.PI) / 180)} y1={c + (r - 12) * Math.sin((ang * Math.PI) / 180)}
          x2={c + (r - 3) * Math.cos((ang * Math.PI) / 180)} y2={c + (r - 3) * Math.sin((ang * Math.PI) / 180)}
          stroke="var(--text)" stroke-width="2.5" stroke-linecap="round" />
      </svg>
      <div class="lv-knob-val mono">{p.format ? p.format(v) : `${Math.round(v * 100)}`}</div>
      <div class="lv-knob-label">
        <span>{p.label}</span>
        {p.held && p.onRelease && <button class="lv-x" title={t("Release")} aria-label={t("Release {label}", { label: p.label })} onClick={p.onRelease}>✕</button>}
      </div>
    </div>
  );
}
