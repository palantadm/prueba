import { useRef, useState } from "preact/hooks";

export type FaderProps = {
  value: number;
  onChange: (v: number) => void;
  onEnd?: () => void;
  height?: number | string;
  width?: number | string;
  color?: string;
  held?: boolean;
  horizontal?: boolean;
  defaultValue?: number;
  disabled?: boolean;
  label?: string;
  ariaLabel?: string;
};

/** Pointer-driven fader (mouse + touch). Shift = fine (relative, ×0.1). Double-click resets. */
export function Fader(p: FaderProps) {
  const track = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const start = useRef({ pos: 0, v: 0, fine: false });
  const v = drag ?? p.value;
  const color = p.held ? "var(--live)" : p.color ?? "var(--accent)";

  const read = (e: PointerEvent): number => {
    const r = track.current!.getBoundingClientRect();
    const len = p.horizontal ? r.width : r.height;
    const pos = p.horizontal ? e.clientX - r.left : r.bottom - e.clientY;
    if (start.current.fine) {
      const d = (pos - start.current.pos) / len;
      return clamp(start.current.v + d * 0.1);
    }
    return clamp(pos / len);
  };
  const down = (e: PointerEvent) => {
    if (p.disabled) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const r = track.current!.getBoundingClientRect();
    const pos = p.horizontal ? e.clientX - r.left : r.bottom - e.clientY;
    start.current = { pos, v, fine: e.shiftKey };
    const nv = e.shiftKey ? v : read(e);
    setDrag(nv);
    if (!e.shiftKey) p.onChange(nv);
  };
  const move = (e: PointerEvent) => {
    if (drag === null) return;
    const nv = read(e);
    setDrag(nv);
    p.onChange(nv);
  };
  const up = () => {
    if (drag === null) return;
    setDrag(null);
    p.onEnd?.();
  };
  const key = (e: KeyboardEvent) => {
    const step = e.shiftKey ? 0.1 : 0.01;
    if (e.key === "ArrowUp" || e.key === "ArrowRight") { p.onChange(clamp(v + step)); e.preventDefault(); }
    if (e.key === "ArrowDown" || e.key === "ArrowLeft") { p.onChange(clamp(v - step)); e.preventDefault(); }
  };
  const size = p.horizontal ? { width: p.width ?? 200, height: p.height ?? 40 } : { height: p.height ?? 200, width: p.width ?? 44 };
  return (
    <div
      ref={track}
      class={`lv-fader ${p.horizontal ? "h" : ""} ${p.held ? "held" : ""} ${p.disabled ? "disabled" : ""}`}
      style={{ ...size, "--fc": color } as any}
      role="slider"
      tabIndex={p.disabled ? -1 : 0}
      aria-label={p.ariaLabel ?? p.label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(v * 100)}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
      onKeyDown={key}
      onDblClick={() => { p.onChange(p.defaultValue ?? 0); p.onEnd?.(); }}
    >
      <div class="lv-fader-fill" style={p.horizontal ? { width: `${v * 100}%` } : { height: `${v * 100}%` }} />
      <div class="lv-fader-thumb" style={p.horizontal ? { left: `${v * 100}%` } : { bottom: `${v * 100}%` }} />
    </div>
  );
}
const clamp = (v: number) => Math.max(0, Math.min(1, v));
