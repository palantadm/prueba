import { useRef, useState } from "preact/hooks";

/** Wide horizontal crossfader (A ↔ B). Big touch target; drag anywhere on the track. */
export function Crossfader(p: {
  value: number;
  onChange: (v: number) => void;
  onEnd?: () => void;
  color?: string;
  leftLabel?: string;
  rightLabel?: string;
  disabled?: boolean;
  ariaLabel?: string;
}) {
  const track = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState(false);
  const v = Math.max(0, Math.min(1, p.value));
  const read = (e: PointerEvent) => {
    const r = track.current!.getBoundingClientRect();
    return Math.max(0, Math.min(1, (e.clientX - r.left) / r.width));
  };
  const down = (e: PointerEvent) => {
    if (p.disabled) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    setDrag(true);
    p.onChange(read(e));
  };
  const move = (e: PointerEvent) => { if (drag) p.onChange(read(e)); };
  const up = () => { if (drag) { setDrag(false); p.onEnd?.(); } };
  const key = (e: KeyboardEvent) => {
    const step = e.shiftKey ? 0.1 : 0.02;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") { e.preventDefault(); p.onChange(Math.min(1, v + step)); }
    if (e.key === "ArrowLeft" || e.key === "ArrowDown") { e.preventDefault(); p.onChange(Math.max(0, v - step)); }
    if (e.key === "Home") { e.preventDefault(); p.onChange(0); }
    if (e.key === "End") { e.preventDefault(); p.onChange(1); }
  };
  const color = p.color ?? "var(--accent)";
  return (
    <div class={`xfader ${drag ? "drag" : ""} ${p.disabled ? "disabled" : ""}`}
      ref={track} role="slider" tabIndex={p.disabled ? -1 : 0} aria-label={p.ariaLabel} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(v * 100)}
      onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up} onKeyDown={key}>
      <div class="xfader-fill" style={{ width: `${v * 100}%`, background: `color-mix(in srgb, ${color} 35%, transparent)` }} />
      <div class="xfader-knob" style={{ left: `${v * 100}%`, borderColor: color }} />
      <span class="xfader-end l">{p.leftLabel ?? "A"}</span>
      <span class="xfader-end r">{p.rightLabel ?? "B"}</span>
    </div>
  );
}
