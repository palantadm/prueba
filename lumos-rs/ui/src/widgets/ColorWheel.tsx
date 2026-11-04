import { useRef, useState } from "preact/hooks";
import { hsvToRgb, rgbToHsv } from "../lib/attrs";
import { t } from "../i18n";

type RGB = [number, number, number];

/** Hue (angle) / saturation (radius) wheel. Brightness stays that of the current colour (min 1 when black). */
export function ColorWheel(p: { rgb: RGB; onChange: (rgb: RGB) => void; onEnd?: (rgb: RGB) => void; size?: number; held?: boolean }) {
  const size = p.size ?? 292;
  const ref = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<[number, number] | null>(null); // hue, sat while dragging
  const last = useRef<RGB>(p.rgb);
  const [h0, s0, v0] = rgbToHsv(...p.rgb);
  const hue = drag?.[0] ?? h0, sat = drag?.[1] ?? s0;
  const val = v0 < 0.02 ? 1 : v0;
  const at = (e: PointerEvent): [number, number] => {
    const r = ref.current!.getBoundingClientRect();
    const dx = e.clientX - (r.left + r.width / 2), dy = e.clientY - (r.top + r.height / 2);
    const rad = r.width / 2;
    const h = ((Math.atan2(dy, dx) / (Math.PI * 2)) + 1.25) % 1; // 0 at top, clockwise
    return [Math.min(0.9999, h), Math.max(0, Math.min(1, Math.hypot(dx, dy) / rad))];
  };
  const apply = (e: PointerEvent) => {
    const [h, s] = at(e);
    setDrag([h, s]);
    last.current = hsvToRgb(h, s, val);
    p.onChange(last.current);
  };
  const down = (e: PointerEvent) => { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); apply(e); };
  const move = (e: PointerEvent) => { if (drag) apply(e); };
  const up = () => { if (drag) { setDrag(null); p.onEnd?.(last.current); } };
  const a = (hue - 0.25) * Math.PI * 2, rr = sat * (size / 2);
  const x = size / 2 + Math.cos(a) * rr, y = size / 2 + Math.sin(a) * rr;
  const cur = hsvToRgb(hue, sat, val);
  return (
    <div ref={ref} class={`cw ${p.held ? "held" : ""}`} style={{ width: size, height: size }} role="application" aria-label={t("Hue and saturation wheel")}
      onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up}>
      <div class="cw-sat" />
      <div class="cw-dot" style={{ left: x, top: y, background: `rgb(${cur[0] * 255 | 0},${cur[1] * 255 | 0},${cur[2] * 255 | 0})` }} />
    </div>
  );
}
