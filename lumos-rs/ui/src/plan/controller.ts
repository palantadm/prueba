// Canvas controller for the 2D workspace: render loop, camera, picking, dragging, aiming, export.
import { fire, library, selection, show } from "../store";
import { t, tp } from "../i18n";

import { aimPanTilt } from "../lib/stage-dmx";
import { dmx } from "../store";
import type { FixtureProfile } from "../types";
import { drawFrame, fitCam, loadImage, pointInPoly, type DrawOut } from "./draw";
import { planOf } from "./fixture";
import { readTheme, type Theme } from "./glyphs";
import {
  apply, clamp, dragAxes, fromScreen, setFromView, sub, toScreen, transpose, type Cam, type V3, type ViewKind,
} from "./math";
import {
  aimMode, currentBg, editFixture, editLayout, editObject, effFixtures, effStage, hud, labelMode, layers, menu, selObj, setBg, tool, view,
  type MenuItem,
} from "./state";

type Drag =
  | { kind: "pan"; sx: number; sy: number; cx: number; cy: number }
  | { kind: "rubber"; x0: number; y0: number; add: boolean; base: number[] }
  | { kind: "move"; ids: number[]; start: Map<number, V3>; u0: number; v0: number; moved: boolean }
  | { kind: "obj"; id: number; start: V3; u0: number; v0: number; moved: boolean }
  | { kind: "aim"; lead: number; ids: number[]; t0: Map<number, V3>; base: V3 }
  | { kind: "calib"; };

export class PlanController {
  private ctx: CanvasRenderingContext2D;
  private th: Theme;
  private dpr = 1;
  private cams = new Map<ViewKind, Cam>();
  private cam: Cam = { cx: 0, cy: 0, scale: 40, w: 800, h: 600 };
  private last: DrawOut = { fx: [], obj: [], handles: [], geos: new Map() };
  private raf = 0;
  private drag: Drag | null = null;
  private rubber: { x0: number; y0: number; x1: number; y1: number } | null = null;
  private hover: number | null = null;
  private ptrs = new Map<number, { x: number; y: number }>();
  private pinch: { d: number; scale: number; cx: number; cy: number; mx: number; my: number } | null = null;
  private space = false;
  private pending = new Map<number, { pan: number; tilt: number }>();
  private lastFlush = 0;
  private lastHud = 0;
  private frames = 0;
  private fpsT = performance.now();
  private profCache: { src: FixtureProfile[]; map: Map<string, FixtureProfile> } | null = null;
  private longPress = 0;
  private pendingObj: number | null = null;
  private downAt = { x: 0, y: 0, t: 0 };
  private ro: ResizeObserver;
  geoOf(id: number) { return this.last.geos.get(id) ?? null; }
  /** Set by the toolbar to run the two-point background calibration. */
  calibrate: { pts: [number, number][]; done: (p: [number, number][]) => void } | null = null;

  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext("2d")!;
    this.th = readTheme();
    this.ro = new ResizeObserver(() => this.resize());
    this.ro.observe(canvas.parentElement ?? canvas);
    canvas.addEventListener("pointerdown", this.onDown);
    canvas.addEventListener("pointermove", this.onMove);
    canvas.addEventListener("pointerup", this.onUp);
    canvas.addEventListener("pointercancel", this.onUp);
    canvas.addEventListener("wheel", this.onWheel, { passive: false });
    canvas.addEventListener("contextmenu", this.onContext);
    window.addEventListener("keydown", this.onKey);
    window.addEventListener("keyup", this.onKeyUp);
    this.resize();
    this.loop = this.loop.bind(this);
    this.raf = requestAnimationFrame(this.loop);
  }

  dispose() {
    cancelAnimationFrame(this.raf);
    this.ro.disconnect();
    const c = this.canvas;
    c.removeEventListener("pointerdown", this.onDown);
    c.removeEventListener("pointermove", this.onMove);
    c.removeEventListener("pointerup", this.onUp);
    c.removeEventListener("pointercancel", this.onUp);
    c.removeEventListener("wheel", this.onWheel);
    c.removeEventListener("contextmenu", this.onContext);
    window.removeEventListener("keydown", this.onKey);
    window.removeEventListener("keyup", this.onKeyUp);
  }

  // ── camera ────────────────────────────────────────────
  private resize() {
    const p = this.canvas.parentElement;
    if (!p) return;
    const w = Math.max(50, p.clientWidth), h = Math.max(50, p.clientHeight);
    this.dpr = Math.min(2.5, window.devicePixelRatio || 1);
    this.canvas.width = Math.round(w * this.dpr);
    this.canvas.height = Math.round(h * this.dpr);
    this.canvas.style.width = `${w}px`;
    this.canvas.style.height = `${h}px`;
    this.cam.w = w; this.cam.h = h;
    for (const c of this.cams.values()) { c.w = w; c.h = h; }
  }

  fit() {
    const f = fitCam(view.value, effStage(), effFixtures(), this.cam.w, this.cam.h);
    this.cam = f;
    this.cams.set(view.value, f);
  }

  zoomBy(k: number) {
    const c = this.cam;
    c.scale = clamp(c.scale * k, 3, 600);
  }

  private camFor(v: ViewKind): Cam {
    let c = this.cams.get(v);
    if (!c) {
      c = fitCam(v, effStage(), effFixtures(), this.cam.w, this.cam.h);
      this.cams.set(v, c);
    }
    c.w = this.cam.w; c.h = this.cam.h;
    return c;
  }

  // ── loop ──────────────────────────────────────────────
  private profiles(): Map<string, FixtureProfile> {
    const lib = library.value;
    if (!this.profCache || this.profCache.src !== lib) this.profCache = { src: lib, map: new Map(lib.map((p) => [p.id, p])) };
    return this.profCache.map;
  }

  private loop(now: number) {
    this.raf = requestAnimationFrame(this.loop);
    if (document.hidden) return;
    this.cam = this.camFor(view.value);
    const ctx = this.ctx;
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    const bg = currentBg();
    const out = drawFrame({
      ctx, cam: this.cam, view: view.value, th: this.th, stage: effStage(), fixtures: effFixtures(), profiles: this.profiles(),
      frames: dmx, selection: new Set(selection.value), selObj: selObj.value, layers: layers.value, labelMode: labelMode.value,
      time: now / 1000, bg, bgImg: bg ? loadImage(bg.url) : null, rubber: this.rubber, hover: this.hover,
    });
    this.last = out;
    if (this.calibrate) this.drawCalib();
    this.flushAim(now);
    this.frames++;
    if (now - this.fpsT > 1000) {
      hud.value = { ...hud.value, fps: Math.round((this.frames * 1000) / (now - this.fpsT)), scale: this.cam.scale };
      this.frames = 0; this.fpsT = now;
    }
  }

  private drawCalib() {
    const c = this.calibrate;
    if (!c) return;
    const ctx = this.ctx;
    ctx.save();
    ctx.strokeStyle = this.th.live; ctx.fillStyle = this.th.live; ctx.lineWidth = 2;
    const pts = c.pts.map(([u, v]) => toScreen(this.cam, u, v));
    for (const [x, y] of pts) { ctx.beginPath(); ctx.arc(x, y, 6, 0, 7); ctx.stroke(); ctx.beginPath(); ctx.arc(x, y, 2, 0, 7); ctx.fill(); }
    if (pts.length === 2) { ctx.beginPath(); ctx.moveTo(pts[0][0], pts[0][1]); ctx.lineTo(pts[1][0], pts[1][1]); ctx.stroke(); }
    ctx.restore();
  }

  // ── aim ───────────────────────────────────────────────
  private queueAim(id: number, target: V3) {
    const g = this.last.geos.get(id);
    if (!g?.plan?.moving) return;
    const Rt = transpose(g.R);
    const v = apply(Rt, sub(target, g.pivot));
    const m = Math.hypot(v[0], v[1], v[2]) || 1;
    const { pan, tilt } = aimPanTilt(g.plan, v[0] / m, v[1] / m, v[2] / m);
    this.pending.set(id, { pan: Math.round(pan * 1e4) / 1e4, tilt: Math.round(tilt * 1e4) / 1e4 });
  }

  private flushAim(now: number, force = false) {
    if (!this.pending.size) return;
    if (!force && now - this.lastFlush < 33) return;
    this.lastFlush = now;
    for (const [id, v] of this.pending) fire({ type: "prog_set", fixtures: [id], attrs: { pan: v.pan, tilt: v.tilt } });
    this.pending.clear();
  }

  /** Aim all selected movers at `point` (stack) — used by the aim tool and the context menu. */
  aimSelectionAt(point: V3) {
    for (const id of selection.value) this.queueAim(id, point);
    this.flushAim(performance.now(), true);
  }

  homeSelection() {
    const ids = selection.value.filter((id) => this.last.geos.get(id)?.moving);
    for (const id of ids) fire({ type: "prog_set", fixtures: [id], attrs: { pan: 0.5, tilt: 0.5 } });
  }

  private basePoint(): V3 {
    // reference for the coordinate that the current view cannot show (depth)
    for (const id of selection.value) {
      const g = this.last.geos.get(id);
      if (g?.moving && g.beam) return g.beam.hit;
    }
    return [0, 0, 0];
  }

  // ── picking ───────────────────────────────────────────
  private pos(e: PointerEvent | WheelEvent | MouseEvent): [number, number] {
    const r = this.canvas.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  }

  private hitFixture(x: number, y: number): number | null {
    let best: number | null = null, bd = 1e9;
    for (const h of this.last.fx) {
      const d = Math.hypot(h.x - x, h.y - y);
      if (d <= Math.max(h.r, 18) && d < bd) { bd = d; best = h.id; }
    }
    return best;
  }
  private hitHandle(x: number, y: number) {
    for (const h of this.last.handles) if (Math.hypot(h.x - x, h.y - y) <= 18) return h;
    return null;
  }
  private hitObject(x: number, y: number): number | null {
    for (let i = this.last.obj.length - 1; i >= 0; i--) if (pointInPoly(x, y, this.last.obj[i].poly)) return this.last.obj[i].id;
    return null;
  }

  private world(x: number, y: number): [number, number] { return fromScreen(this.cam, x, y); }

  // ── pointer events ────────────────────────────────────
  private onDown = (e: PointerEvent) => {
    this.canvas.setPointerCapture(e.pointerId);
    const [x, y] = this.pos(e);
    this.ptrs.set(e.pointerId, { x, y });
    menu.value = null;
    if (this.ptrs.size === 2) { this.startPinch(); this.drag = null; this.rubber = null; return; }
    this.downAt = { x, y, t: performance.now() };

    if (this.calibrate) {
      const [u, v] = this.world(x, y);
      this.calibrate.pts.push([u, v]);
      if (this.calibrate.pts.length >= 2) { const c = this.calibrate; this.calibrate = null; c.done(c.pts); }
      return;
    }
    if (e.button === 1 || tool.value === "hand" || this.space) {
      this.drag = { kind: "pan", sx: x, sy: y, cx: this.cam.cx, cy: this.cam.cy };
      return;
    }
    if (e.button === 2) return;
    const additive = e.shiftKey || e.ctrlKey || e.metaKey;

    if (tool.value === "aim") return; // handled on pointerup (click)

    const h = this.hitHandle(x, y);
    if (h) {
      const ids = selection.value.filter((id) => this.last.geos.get(id)?.moving);
      const t0 = new Map<number, V3>();
      for (const id of ids) { const g = this.last.geos.get(id); if (g?.beam) t0.set(id, g.beam.hit); }
      this.drag = { kind: "aim", lead: h.id, ids, t0, base: h.target };
      return;
    }
    const fid = this.hitFixture(x, y);
    if (fid !== null) {
      selObj.value = null;
      const cur = selection.value;
      if (additive) selection.value = cur.includes(fid) ? cur.filter((i) => i !== fid) : [...cur, fid];
      else if (!cur.includes(fid)) selection.value = [fid];
      if (editLayout.value && selection.value.includes(fid)) {
        const ids = selection.value.slice();
        const start = new Map<number, V3>();
        for (const id of ids) { const f = effFixtures().find((q) => q.id === id); if (f) start.set(id, [f.pos[0], f.pos[1], f.pos[2]]); }
        const [u0, v0] = this.world(x, y);
        this.drag = { kind: "move", ids, start, u0, v0, moved: false };
      }
      this.armLongPress(e, x, y);
      return;
    }
    const oid = this.hitObject(x, y);
    // Objects move only once selected (click first); dragging an unselected object rubber-bands fixtures instead.
    if (oid !== null && oid === selObj.value) {
      const o = effStage()?.objects.find((q) => q.id === oid);
      if (o && editLayout.value) {
        const [u0, v0] = this.world(x, y);
        this.drag = { kind: "obj", id: oid, start: [o.pos[0], o.pos[1], o.pos[2]], u0, v0, moved: false };
        return;
      }
    }
    this.pendingObj = oid;
    if (oid === null) selObj.value = null;
    this.drag = { kind: "rubber", x0: x, y0: y, add: additive, base: additive ? selection.value.slice() : [] };
    this.rubber = { x0: x, y0: y, x1: x, y1: y };
    this.armLongPress(e, x, y);
  };

  private armLongPress(e: PointerEvent, x: number, y: number) {
    clearTimeout(this.longPress);
    if (e.pointerType === "mouse") return;
    this.longPress = window.setTimeout(() => {
      if (Math.hypot(this.downAt.x - x, this.downAt.y - y) < 6) { this.drag = null; this.rubber = null; this.openMenu(x, y); }
    }, 550);
  }

  private startPinch() {
    const [a, b] = [...this.ptrs.values()];
    const d = Math.hypot(a.x - b.x, a.y - b.y);
    const mx = (a.x + b.x) / 2, my = (a.y + b.y) / 2;
    this.pinch = { d, scale: this.cam.scale, cx: this.cam.cx, cy: this.cam.cy, mx, my };
  }

  private onMove = (e: PointerEvent) => {
    const [x, y] = this.pos(e);
    if (this.ptrs.has(e.pointerId)) this.ptrs.set(e.pointerId, { x, y });
    const now = performance.now();
    if (now - this.lastHud > 80) {
      this.lastHud = now;
      const [u, v] = this.world(x, y);
      const p = setFromView(view.value, [0, 0, 0], u, v);
      hud.value = { ...hud.value, x: p[0], y: view.value === "plan" ? 0 : p[1], z: p[2] };
    }
    if (this.pinch && this.ptrs.size >= 2) {
      const [a, b] = [...this.ptrs.values()];
      const d = Math.hypot(a.x - b.x, a.y - b.y) || 1;
      const mx = (a.x + b.x) / 2, my = (a.y + b.y) / 2;
      const p = this.pinch;
      this.cam.scale = clamp((p.scale * d) / p.d, 3, 600);
      this.cam.cx = p.cx - (mx - p.mx) / this.cam.scale;
      this.cam.cy = p.cy - (my - p.my) / this.cam.scale;
      return;
    }
    const dr = this.drag;
    if (!dr) { this.hover = this.hitFixture(x, y); this.canvas.style.cursor = this.hover !== null || this.hitHandle(x, y) ? "pointer" : tool.value === "hand" ? "grab" : tool.value === "aim" ? "crosshair" : ""; return; }
    clearTimeout(this.longPress);
    const [u, v] = this.world(x, y);
    switch (dr.kind) {
      case "pan":
        this.cam.cx = dr.cx - (x - dr.sx) / this.cam.scale;
        this.cam.cy = dr.cy - (y - dr.sy) / this.cam.scale;
        break;
      case "rubber": {
        this.rubber = { x0: dr.x0, y0: dr.y0, x1: x, y1: y };
        const [xa, xb] = [Math.min(dr.x0, x), Math.max(dr.x0, x)], [ya, yb] = [Math.min(dr.y0, y), Math.max(dr.y0, y)];
        const inside = this.last.fx.filter((h) => h.x >= xa && h.x <= xb && h.y >= ya && h.y <= yb).map((h) => h.id);
        selection.value = dr.add ? [...new Set([...dr.base, ...inside])] : inside;
        break;
      }
      case "move": {
        if (!dr.moved && Math.hypot(x - this.downAt.x, y - this.downAt.y) < 4) break;
        dr.moved = true;
        const snap = e.altKey ? 0 : 0.05;
        const [a, b, s] = dragAxes(view.value);
        const du = u - dr.u0, dv = (v - dr.v0) * s;
        for (const id of dr.ids) {
          const st = dr.start.get(id);
          if (!st) continue;
          const np: V3 = [st[0], st[1], st[2]];
          np[a] = snap ? Math.round((st[a] + du) / snap) * snap : st[a] + du;
          np[b] = snap ? Math.round((st[b] + dv) / snap) * snap : st[b] + dv;
          if (np[1] < 0) np[1] = 0;
          editFixture(id, (f) => { f.pos = [round3(np[0]), round3(np[1]), round3(np[2])]; }, 200);
        }
        break;
      }
      case "obj": {
        if (!dr.moved && Math.hypot(x - this.downAt.x, y - this.downAt.y) < 4) break;
        dr.moved = true;
        const snap = e.altKey ? 0 : 0.05;
        const [a, b, s] = dragAxes(view.value);
        const np: V3 = [dr.start[0], dr.start[1], dr.start[2]];
        np[a] = dr.start[a] + (u - dr.u0); np[b] = dr.start[b] + (v - dr.v0) * s;
        if (snap) { np[a] = Math.round(np[a] / snap) * snap; np[b] = Math.round(np[b] / snap) * snap; }
        editObject(dr.id, (o) => { o.pos = [round3(np[0]), round3(np[1]), round3(np[2])]; }, 200);
        break;
      }
      case "aim": {
        const T = setFromView(view.value, dr.base, u, v);
        if (view.value === "plan") T[1] = dr.base[1];
        const lead0 = dr.t0.get(dr.lead) ?? dr.base;
        for (const id of dr.ids) {
          const t0 = dr.t0.get(id) ?? lead0;
          const target: V3 = aimMode.value === "stack" ? T : [T[0] + (t0[0] - lead0[0]), T[1] + (t0[1] - lead0[1]), T[2] + (t0[2] - lead0[2])];
          if (target[1] < 0) target[1] = 0;
          this.queueAim(id, target);
        }
        break;
      }
      default: break;
    }
  };

  private onUp = (e: PointerEvent) => {
    const [x, y] = this.pos(e);
    this.ptrs.delete(e.pointerId);
    clearTimeout(this.longPress);
    if (this.ptrs.size < 2) this.pinch = null;
    const dr = this.drag;
    const click = Math.hypot(x - this.downAt.x, y - this.downAt.y) < 5 && performance.now() - this.downAt.t < 600;
    if (tool.value === "aim" && !this.calibrate && click && e.button === 0 && !this.pinch && !dr) {
      const [u, v] = this.world(x, y);
      const T = setFromView(view.value, this.basePoint(), u, v);
      if (view.value === "plan") T[1] = 0;
      this.aimSelectionAt(T);
    }
    if (dr?.kind === "rubber") {
      this.rubber = null;
      if (click && this.pendingObj !== null) { selObj.value = this.pendingObj; if (!dr.add) selection.value = []; }
      else if (click && !dr.add) selection.value = [];
      this.pendingObj = null;
    }
    if (dr?.kind === "move" && !dr.moved && click && !(e.shiftKey || e.ctrlKey || e.metaKey)) {
      const fid = this.hitFixture(x, y);
      if (fid !== null && selection.value.length > 1) selection.value = [fid];
    }
    if (dr?.kind === "aim") this.flushAim(performance.now(), true);
    this.drag = null;
  };

  private onWheel = (e: WheelEvent) => {
    e.preventDefault();
    const [x, y] = this.pos(e);
    const [u, v] = this.world(x, y);
    const k = Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0016));
    this.cam.scale = clamp(this.cam.scale * k, 3, 600);
    this.cam.cx = u - (x - this.cam.w / 2) / this.cam.scale;
    this.cam.cy = v - (y - this.cam.h / 2) / this.cam.scale;
  };

  // ── context menu ──────────────────────────────────────
  private onContext = (e: MouseEvent) => {
    e.preventDefault();
    const [x, y] = this.pos(e);
    this.openMenu(x, y);
  };

  private openMenu(x: number, y: number) {
    const fid = this.hitFixture(x, y);
    const items: MenuItem[] = [];
    const [u, v] = this.world(x, y);
    const T = setFromView(view.value, this.basePoint(), u, v);
    if (view.value === "plan") T[1] = 0;
    const movers = selection.value.filter((id) => this.last.geos.get(id)?.moving);
    if (fid !== null) {
      if (!selection.value.includes(fid)) selection.value = [fid];
      const f = effFixtures().find((q) => q.id === fid);
      const sameProf = effFixtures().filter((q) => q.profile === f?.profile).map((q) => q.id);
      items.push({ label: t("Fixture {id} · {name}", { id: fid, name: f?.name ?? "" }), disabled: true });
      items.push({ label: t("Select all “{model}” ({n})", { model: f?.profile.split("/").pop() ?? "", n: sameProf.length }), run: () => (selection.value = sameProf) });
      for (const g of show.value?.groups ?? []) {
        if (g.fixtures.includes(fid)) items.push({ label: t("Select group “{name}”", { name: g.name }), run: () => (selection.value = g.fixtures.slice()) });
      }
      items.push({ sep: true, label: "" });
    }
    const m2 = selection.value.filter((id) => this.last.geos.get(id)?.moving);
    items.push({ label: tp(m2.length || movers.length || 0, "Aim {n} mover here", "Aim {n} movers here"), disabled: !(m2.length || movers.length), run: () => this.aimSelectionAt(T) });
    items.push({ label: t("Home pan/tilt (centre)"), disabled: !m2.length, run: () => this.homeSelection() });
    items.push({ label: t("Release pan/tilt"), disabled: !m2.length, run: () => fire({ type: "prog_release", fixtures: m2, attrs: ["pan", "tilt"] }) });
    items.push({ sep: true, label: "" });
    items.push({ label: t("Select all movers"), run: () => (selection.value = [...this.last.geos.values()].filter((g) => g.moving).map((g) => g.fx.id)) });
    items.push({ label: t("Select all"), run: () => (selection.value = effFixtures().map((f) => f.id)) });
    items.push({ label: t("Select none"), run: () => (selection.value = []) });
    items.push({ label: t("Fit view"), run: () => this.fit() });
    menu.value = { x, y, items };
  }

  // ── keyboard ──────────────────────────────────────────
  private onKey = (e: KeyboardEvent) => {
    if ((e.target as HTMLElement)?.matches?.("input,textarea,select")) return;
    if (e.code === "Space") { this.space = true; if (e.target === document.body) e.preventDefault(); }
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    switch (e.key.toLowerCase()) {
      case "a": tool.value = tool.value === "aim" ? "select" : "aim"; break;
      case "h": tool.value = tool.value === "hand" ? "select" : "hand"; break;
      case "f": this.fit(); break;
      case "v": view.value = view.value === "plan" ? "front" : view.value === "front" ? "side" : "plan"; break;
      case "+": case "=": this.zoomBy(1.25); break;
      case "-": this.zoomBy(0.8); break;
      case "escape": tool.value = "select"; selection.value = []; selObj.value = null; menu.value = null; break;
    }
  };
  private onKeyUp = (e: KeyboardEvent) => { if (e.code === "Space") this.space = false; };

  // ── background calibration ────────────────────────────
  /** Apply a two-point calibration: pts are world (view-plane) points picked on the image. */
  applyCalibration(pts: [number, number][], metres: number, alignHorizontal: boolean) {
    const bg = currentBg();
    if (!bg || pts.length < 2 || metres <= 0) return;
    const [a, b] = pts;
    const dist = Math.hypot(b[0] - a[0], b[1] - a[1]);
    if (dist < 1e-6) return;
    const k = metres / dist;
    const mx = (a[0] + b[0]) / 2, mz = (a[1] + b[1]) / 2;
    // scale the image about the picked midpoint
    const ncx = mx + (bg.cx - mx) * k, ncz = mz + (bg.cz - mz) * k;
    let rot = bg.rot;
    let cx = ncx, cz = ncz;
    if (alignHorizontal) {
      const ang = Math.atan2(b[1] - a[1], b[0] - a[0]) * (180 / Math.PI);
      rot = bg.rot - ang;
      const r = (-ang * Math.PI) / 180, c = Math.cos(r), s = Math.sin(r);
      const dx = ncx - mx, dz = ncz - mz;
      cx = mx + dx * c - dz * s; cz = mz + dx * s + dz * c;
    }
    setBg({ width: bg.width * k, cx, cz, rot });
  }

  // ── export ────────────────────────────────────────────
  exportPng(title: string) {
    const scale = 2;
    const w = this.cam.w, h = this.cam.h;
    const cv = document.createElement("canvas");
    cv.width = w * scale; cv.height = h * scale;
    const ctx = cv.getContext("2d")!;
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    const bg = currentBg();
    drawFrame({
      ctx, cam: { ...this.cam }, view: view.value, th: this.th, stage: effStage(), fixtures: effFixtures(), profiles: this.profiles(),
      frames: dmx, selection: new Set(), selObj: null, layers: layers.value, labelMode: labelMode.value, time: performance.now() / 1000,
      bg, bgImg: bg ? loadImage(bg.url) : null, rubber: null, hover: null, clean: true,
    });
    ctx.fillStyle = this.th.text; ctx.font = `700 14px ${this.th.ui}`; ctx.textAlign = "left"; ctx.textBaseline = "top";
    ctx.fillText(`${title} — ${view.value === "plan" ? t("Plan") : view.value === "front" ? t("Front elevation") : t("Side elevation")}`, 16, 14);
    ctx.fillStyle = this.th.dim; ctx.font = `11px ${this.th.ui}`;
    ctx.fillText(new Date().toLocaleString(), 16, 34);
    cv.toBlob((b) => {
      if (!b) return;
      const a = document.createElement("a");
      a.href = URL.createObjectURL(b);
      a.download = `${title.replace(/[^\w-]+/g, "_")}-${view.value}.png`;
      a.click();
      setTimeout(() => URL.revokeObjectURL(a.href), 2000);
    });
  }
}

const round3 = (n: number) => Math.round(n * 1000) / 1000;
export { planOf };
