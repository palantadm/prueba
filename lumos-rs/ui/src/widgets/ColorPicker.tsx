import { useEffect, useRef, useState } from "preact/hooks";
import { hsvToRgb, rgbToHsv } from "../lib/attrs";
import { t } from "../i18n";

type RGB = [number, number, number];

/** Saturation/Value box + hue strip. Values 0..1. */
export function ColorPicker(p: { rgb: RGB; onChange: (rgb: RGB) => void; onEnd?: (rgb: RGB) => void; size?: number }) {
  const [hsv, setHsv] = useState<RGB>(() => rgbToHsv(...p.rgb));
  const dragging = useRef<"sv" | "h" | null>(null);
  const last = useRef<RGB>(p.rgb);
  const box = useRef<HTMLDivElement>(null);
  const strip = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (dragging.current) return;
    const cur = hsvToRgb(...hsv);
    if (Math.abs(cur[0] - p.rgb[0]) + Math.abs(cur[1] - p.rgb[1]) + Math.abs(cur[2] - p.rgb[2]) > 0.01) {
      const n = rgbToHsv(...p.rgb);
      setHsv([n[1] < 0.01 || n[2] < 0.01 ? hsv[0] : n[0], n[1], n[2]]);
    }
  }, [p.rgb[0], p.rgb[1], p.rgb[2]]);

  const apply = (n: RGB) => { setHsv(n); last.current = hsvToRgb(...n); p.onChange(last.current); };
  const cl = (v: number) => Math.max(0, Math.min(1, v));
  const svAt = (e: PointerEvent) => {
    const r = box.current!.getBoundingClientRect();
    apply([hsv[0], cl((e.clientX - r.left) / r.width), cl(1 - (e.clientY - r.top) / r.height)]);
  };
  const hAt = (e: PointerEvent) => {
    const r = strip.current!.getBoundingClientRect();
    apply([Math.min(0.9999, cl((e.clientX - r.left) / r.width)), hsv[1] || 1, hsv[2] || 1]);
  };
  const mk = (kind: "sv" | "h", fn: (e: PointerEvent) => void) => ({
    onPointerDown: (e: PointerEvent) => { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); dragging.current = kind; fn(e); },
    onPointerMove: (e: PointerEvent) => { if (dragging.current === kind) fn(e); },
    onPointerUp: () => { if (dragging.current === kind) { dragging.current = null; p.onEnd?.(last.current); } },
    onPointerCancel: () => { dragging.current = null; },
  });
  const hue = `hsl(${hsv[0] * 360} 100% 50%)`;
  return (
    <div class="lv-cp" style={{ width: p.size ?? 240 }}>
      <div ref={box} class="lv-cp-sv" style={{ background: `linear-gradient(to top,#000,transparent),linear-gradient(to right,#fff,${hue})` }}
        role="application" aria-label={t("Saturation and brightness")} {...mk("sv", svAt)}>
        <div class="lv-cp-dot" style={{ left: `${hsv[1] * 100}%`, top: `${(1 - hsv[2]) * 100}%` }} />
      </div>
      <div ref={strip} class="lv-cp-hue" role="application" aria-label={t("Hue")} {...mk("h", hAt)}>
        <div class="lv-cp-hdot" style={{ left: `${hsv[0] * 100}%` }} />
      </div>
    </div>
  );
}
