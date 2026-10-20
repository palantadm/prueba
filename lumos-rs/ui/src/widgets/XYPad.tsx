import { useRef, useState } from "preact/hooks";

/** 2-D pad. x: left→right, y: bottom→top (0..1). Shift = fine relative movement. */
export function XYPad(p: {
  x: number; y: number; onChange: (x: number, y: number) => void; onEnd?: () => void;
  held?: boolean; color?: string; labelX?: string; labelY?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<[number, number] | null>(null);
  const st = useRef({ cx: 0, cy: 0, x: 0, y: 0, fine: false });
  const x = drag?.[0] ?? p.x, y = drag?.[1] ?? p.y;
  const color = p.held ? "var(--live)" : p.color ?? "var(--attr-position)";
  const cl = (v: number) => Math.max(0, Math.min(1, v));
  const calc = (e: PointerEvent): [number, number] => {
    const r = ref.current!.getBoundingClientRect();
    if (st.current.fine) {
      return [cl(st.current.x + ((e.clientX - st.current.cx) / r.width) * 0.1), cl(st.current.y - ((e.clientY - st.current.cy) / r.height) * 0.1)];
    }
    return [cl((e.clientX - r.left) / r.width), cl(1 - (e.clientY - r.top) / r.height)];
  };
  const down = (e: PointerEvent) => {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    st.current = { cx: e.clientX, cy: e.clientY, x, y, fine: e.shiftKey };
    const v: [number, number] = e.shiftKey ? [x, y] : calc(e);
    setDrag(v);
    if (!e.shiftKey) p.onChange(v[0], v[1]);
  };
  const move = (e: PointerEvent) => {
    if (!drag) return;
    const v = calc(e);
    setDrag(v);
    p.onChange(v[0], v[1]);
  };
  const up = () => { if (drag) { setDrag(null); p.onEnd?.(); } };
  const key = (e: KeyboardEvent) => {
    const s = e.shiftKey ? 0.1 : 0.01;
    let nx = x, ny = y;
    if (e.key === "ArrowLeft") nx -= s; else if (e.key === "ArrowRight") nx += s;
    else if (e.key === "ArrowUp") ny += s; else if (e.key === "ArrowDown") ny -= s; else return;
    e.preventDefault();
    p.onChange(cl(nx), cl(ny));
  };
  return (
    <div ref={ref} class={`lv-xy ${p.held ? "held" : ""}`} style={{ "--fc": color } as any} tabIndex={0}
      role="application" aria-label={`${p.labelX ?? "Pan"} / ${p.labelY ?? "Tilt"} pad`}
      onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up} onKeyDown={key}>
      <div class="lv-xy-h" style={{ bottom: `${y * 100}%` }} />
      <div class="lv-xy-v" style={{ left: `${x * 100}%` }} />
      <div class="lv-xy-dot" style={{ left: `${x * 100}%`, bottom: `${y * 100}%` }} />
    </div>
  );
}
