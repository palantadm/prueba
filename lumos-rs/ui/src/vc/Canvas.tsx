// Grid canvas: renders the current page, and in edit mode handles select / move / resize / rubber-band.
import { useEffect, useRef, useState } from "preact/hooks";
import type { VcWidget } from "../types";
import { draft, edit, editMode, page, patchWidget, sel } from "./editor";
import { WidgetBody } from "./Widgets";
import { kindLabel } from "./model";
import { t } from "../i18n";

type Geo = { x: number; y: number; w: number; h: number };
type Gesture =
  | { kind: "move"; sx: number; sy: number; dx: number; dy: number; ids: number[]; base: Map<number, Geo> }
  | { kind: "resize"; sx: number; sy: number; dw: number; dh: number; id: number; base: Geo }
  | { kind: "band"; sx: number; sy: number; cx: number; cy: number; add: boolean; prev: number[] };

export function Canvas() {
  const pg = page.value;
  const editing = editMode.value;
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 900, h: 600 });
  const [g, setG] = useState<Gesture | null>(null);
  const gref = useRef<Gesture | null>(null);

  useEffect(() => {
    const el = wrap.current; if (!el) return;
    const ro = new ResizeObserver(() => setBox({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el); setBox({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  if (!pg) return null;
  const cell = Math.max(30, Math.min(96, Math.floor(Math.min((box.w - 24) / pg.cols, (box.h - 24) / pg.rows))));
  const setGesture = (n: Gesture | null) => { gref.current = n; setG(n); };

  const geoOf = (w: VcWidget): Geo => {
    if (g?.kind === "move" && g.ids.includes(w.id)) return { x: w.x + g.dx, y: w.y + g.dy, w: w.w, h: w.h };
    if (g?.kind === "resize" && g.id === w.id) return { x: w.x, y: w.y, w: Math.max(1, g.base.w + g.dw), h: Math.max(1, g.base.h + g.dh) };
    return { x: w.x, y: w.y, w: w.w, h: w.h };
  };

  const track = (onMove: (e: PointerEvent) => void, onUp: (e: PointerEvent) => void) => {
    const mv = (e: PointerEvent) => onMove(e);
    const up = (e: PointerEvent) => { window.removeEventListener("pointermove", mv); window.removeEventListener("pointerup", up); window.removeEventListener("pointercancel", up); onUp(e); };
    window.addEventListener("pointermove", mv); window.addEventListener("pointerup", up); window.addEventListener("pointercancel", up);
  };

  const startMove = (e: PointerEvent, w: VcWidget) => {
    if (!editing || e.button !== 0) return;
    e.stopPropagation();
    let ids = sel.value;
    if (e.shiftKey || e.ctrlKey || e.metaKey) {
      ids = ids.includes(w.id) ? ids.filter((x) => x !== w.id) : [...ids, w.id];
      sel.value = ids; if (!ids.includes(w.id)) return;
    } else if (!ids.includes(w.id)) { ids = [w.id]; sel.value = ids; }
    // moving a frame carries the widgets it contains
    const all = new Set(ids);
    for (const f of pg.widgets) if (f.kind === "frame" && all.has(f.id))
      for (const o of pg.widgets) if (o.id !== f.id && o.x >= f.x && o.y >= f.y && o.x + o.w <= f.x + f.w && o.y + o.h <= f.y + f.h) all.add(o.id);
    const moving = [...all];
    const base = new Map(pg.widgets.filter((x) => all.has(x.id)).map((x) => [x.id, { x: x.x, y: x.y, w: x.w, h: x.h }]));
    const sx = e.clientX, sy = e.clientY;
    setGesture({ kind: "move", sx, sy, dx: 0, dy: 0, ids: moving, base });
    track((ev) => {
      let dx = Math.round((ev.clientX - sx) / cell), dy = Math.round((ev.clientY - sy) / cell);
      for (const b of base.values()) { dx = Math.max(-b.x, Math.min(pg.cols - b.w - b.x, dx)); dy = Math.max(-b.y, Math.min(pg.rows - b.h - b.y, dy)); }
      const cur = gref.current; if (cur?.kind === "move" && (cur.dx !== dx || cur.dy !== dy)) setGesture({ ...cur, dx, dy });
    }, () => {
      const cur = gref.current; setGesture(null);
      if (cur?.kind === "move" && (cur.dx || cur.dy)) {
        edit((ps) => { const p = ps.find((x) => x.id === pg.id)!; for (const o of p.widgets) if (cur.ids.includes(o.id)) { o.x += cur.dx; o.y += cur.dy; } });
      }
    });
  };

  const startResize = (e: PointerEvent, w: VcWidget) => {
    e.stopPropagation(); e.preventDefault();
    const base = { x: w.x, y: w.y, w: w.w, h: w.h }, sx = e.clientX, sy = e.clientY;
    setGesture({ kind: "resize", sx, sy, dw: 0, dh: 0, id: w.id, base });
    track((ev) => {
      const dw = Math.max(1 - base.w, Math.min(pg.cols - base.x - base.w, Math.round((ev.clientX - sx) / cell)));
      const dh = Math.max(1 - base.h, Math.min(pg.rows - base.y - base.h, Math.round((ev.clientY - sy) / cell)));
      const cur = gref.current; if (cur?.kind === "resize" && (cur.dw !== dw || cur.dh !== dh)) setGesture({ ...cur, dw, dh });
    }, () => {
      const cur = gref.current; setGesture(null);
      if (cur?.kind === "resize" && (cur.dw || cur.dh)) patchWidget(cur.id, (o) => { o.w = Math.max(1, cur.base.w + cur.dw); o.h = Math.max(1, cur.base.h + cur.dh); }, false);
    });
  };

  const startBand = (e: PointerEvent) => {
    if (!editing || e.target !== canvas.current || e.button !== 0) return;
    const r = canvas.current!.getBoundingClientRect();
    const add = e.shiftKey || e.ctrlKey || e.metaKey, prev = add ? sel.value : [];
    if (!add) sel.value = [];
    const sx = e.clientX - r.left, sy = e.clientY - r.top;
    setGesture({ kind: "band", sx, sy, cx: sx, cy: sy, add, prev });
    track((ev) => {
      const cx = Math.max(0, Math.min(pg.cols * cell, ev.clientX - r.left)), cy = Math.max(0, Math.min(pg.rows * cell, ev.clientY - r.top));
      setGesture({ kind: "band", sx, sy, cx, cy, add, prev });
      const x0 = Math.min(sx, cx), x1 = Math.max(sx, cx), y0 = Math.min(sy, cy), y1 = Math.max(sy, cy);
      if (x1 - x0 + y1 - y0 < 6) return;
      const hit = pg.widgets.filter((w) => w.kind !== "frame" && w.x * cell < x1 && (w.x + w.w) * cell > x0 && w.y * cell < y1 && (w.y + w.h) * cell > y0).map((w) => w.id);
      sel.value = [...new Set([...prev, ...hit])];
    }, () => setGesture(null));
  };

  const bandRect = g?.kind === "band" ? { x: Math.min(g.sx, g.cx), y: Math.min(g.sy, g.cy), w: Math.abs(g.cx - g.sx), h: Math.abs(g.cy - g.sy) } : null;
  const single = editing && sel.value.length === 1 ? sel.value[0] : null;
  const ordered = [...draft.value.find((p) => p.id === pg.id)!.widgets].sort((a, b) => (a.kind === "frame" ? 0 : 1) - (b.kind === "frame" ? 0 : 1));

  return (
    <div class="vc-stage" ref={wrap}>
      <div ref={canvas} class={`vc-canvas ${editing ? "editing" : ""}`}
        style={{ width: pg.cols * cell, height: pg.rows * cell, "--cell": `${cell}px` } as never} onPointerDown={startBand}>
        {ordered.map((w) => {
          const gm = geoOf(w);
          const selected = editing && sel.value.includes(w.id);
          return (
            <div key={w.id} class={`vc-w k-${w.kind} ${selected ? "sel" : ""} ${g && ((g.kind === "move" && g.ids.includes(w.id)) || (g.kind === "resize" && g.id === w.id)) ? "dragging" : ""}`}
              style={{ left: gm.x * cell, top: gm.y * cell, width: gm.w * cell, height: gm.h * cell, "--wc": w.color } as never}
              title={editing ? `${kindLabel(w.kind)}: ${w.label}` : undefined}
              onPointerDown={(e) => startMove(e, w)}>
              <div class="vc-w-in"><WidgetBody w={w} interactive={!editing} px={{ w: gm.w * cell, h: gm.h * cell }} /></div>
              {editing && w.kind !== "frame" && <div class="vc-shield" />}
              {editing && w.kind === "frame" && <div class="vc-frame-grab" />}
              {single === w.id && <div class="vc-handle" onPointerDown={(e) => startResize(e, w)} aria-label={t("Resize")} />}
            </div>
          );
        })}
        {bandRect && <div class="vc-band" style={{ left: bandRect.x, top: bandRect.y, width: bandRect.w, height: bandRect.h }} />}
        {editing && pg.widgets.length === 0 && <div class="vc-empty">{t("Add widgets from the palette on the left, or use “Auto-build from executors”.")}</div>}
      </div>
    </div>
  );
}
