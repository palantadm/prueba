import { useRef, useState } from "preact/hooks";

type P = {
  value: number;
  onChange: (v: number) => void;
  onEnd?: () => void;
  color?: string;
  ariaLabel: string;
  disabled?: boolean;
  class?: string;
};

const clamp = (v: number) => Math.max(0, Math.min(1, v));

/** Touch/pointer fader built on the `.fader-h` / `.fader-v` design-system tracks (cap ≥ 16 px). */
function Slim(p: P & { horizontal: boolean }) {
  const track = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const v = drag ?? p.value;
  const read = (e: PointerEvent) => {
    const r = track.current!.getBoundingClientRect();
    return clamp(p.horizontal ? (e.clientX - r.left) / r.width : (r.bottom - e.clientY) / r.height);
  };
  const down = (e: PointerEvent) => {
    if (p.disabled) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const nv = read(e);
    setDrag(nv);
    p.onChange(nv);
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
    const step = e.shiftKey ? 0.1 : 0.02;
    if (e.key === "ArrowUp" || e.key === "ArrowRight") { p.onChange(clamp(v + step)); e.preventDefault(); }
    if (e.key === "ArrowDown" || e.key === "ArrowLeft") { p.onChange(clamp(v - step)); e.preventDefault(); }
  };
  const pc = `${(v * 100).toFixed(1)}%`;
  const fill = p.color ?? "var(--amber)";
  return (
    <div ref={track} class={`${p.horizontal ? "fader-h cu-hf" : "fader-v cu-vf"} ${p.disabled ? "disabled" : ""} ${p.class ?? ""}`}
      role="slider" tabIndex={p.disabled ? -1 : 0} aria-label={p.ariaLabel} aria-orientation={p.horizontal ? "horizontal" : "vertical"}
      aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(v * 100)}
      onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up} onKeyDown={key}>
      {p.horizontal
        ? <><div class="fill" style={{ width: pc, background: `linear-gradient(90deg, color-mix(in srgb, ${fill} 25%, transparent), ${fill})` }} /><div class="cap" style={{ left: pc }} /></>
        : <><div class="fill" style={{ height: pc, background: fill }} /><div class="cap" style={{ bottom: pc }} /></>}
    </div>
  );
}

export const VFader = (p: P) => <Slim {...p} horizontal={false} />;
export const HFader = (p: P) => <Slim {...p} horizontal />;
