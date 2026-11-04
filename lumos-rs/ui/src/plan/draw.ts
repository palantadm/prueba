// Frame renderer for the 2D visualiser (Canvas2D, HiDPI).
import { t } from "../i18n";
import type { Fixture, FixtureProfile, Stage, StageObject } from "../types";
import { computeFixture, objObb, obstacles, beamOf, angleOf, type FxGeo } from "./fixture";
import { drawBody, glow, goboGlyph, rgba, strobeBolt, symbolSize, type Theme } from "./glyphs";
import {
  D2R, apply, boxCorners, hull, niceStep, proj, toScreen, toScreenP, type Cam, type V3, type ViewKind,
} from "./math";
import type { Bg, LabelMode, Layers } from "./state";
import { getSmoke } from "../stage/smoke";

export interface DrawInput {
  ctx: CanvasRenderingContext2D;
  cam: Cam;
  view: ViewKind;
  th: Theme;
  stage: Stage | null;
  fixtures: Fixture[];
  profiles: Map<string, FixtureProfile>;
  frames: Map<number, Uint8Array>;
  selection: Set<number>;
  selObj: number | null;
  layers: Layers;
  labelMode: LabelMode;
  time: number;
  bg: Bg | null;
  bgImg: HTMLImageElement | null;
  rubber: { x0: number; y0: number; x1: number; y1: number } | null;
  hover: number | null;
  /** Hide UI chrome (handles, rubber band, selection) — used by PNG export. */
  clean?: boolean;
}

export interface FxHit { id: number; x: number; y: number; r: number }
export interface ObjHit { id: number; poly: [number, number][] }
export interface DrawOut {
  fx: FxHit[];
  obj: ObjHit[];
  handles: { id: number; x: number; y: number; target: V3 }[];
  geos: Map<number, FxGeo>;
}

const imgCache = new Map<string, HTMLImageElement | null>();
export function loadImage(url: string): HTMLImageElement | null {
  if (!url) return null;
  if (imgCache.has(url)) return imgCache.get(url) ?? null;
  imgCache.set(url, null);
  const im = new Image();
  im.onload = () => imgCache.set(url, im);
  im.src = url;
  return null;
}

// ── frame ───────────────────────────────────────────────

export function drawFrame(d: DrawInput): DrawOut {
  const { ctx, cam, view, th } = d;
  const out: DrawOut = { fx: [], obj: [], handles: [], geos: new Map() };
  ctx.fillStyle = th.bg0;
  ctx.fillRect(0, 0, cam.w, cam.h);

  if (d.layers.background && d.bg && d.bgImg && view === "plan") drawBg(d);
  drawStageAndGrid(d);
  if (d.layers.objects && d.stage) {
    for (const o of d.stage.objects) {
      const poly = drawObject(d, o);
      if (poly) out.obj.push({ id: o.id, poly });
    }
  }

  if (d.layers.smoke !== false) drawSmoke(d);

  // fixture geometry
  const obs = obstacles(d.stage);
  const geos: FxGeo[] = [];
  for (const fx of d.fixtures) {
    const prof = d.profiles.get(fx.profile);
    const sel = d.selection.has(fx.id);
    const g = computeFixture(fx, prof, d.frames.get(fx.universe), d.time, obs, d.layers.beams || d.layers.footprints || sel);
    // selected movers always get a beam (aim handle), even when dark
    if (!g.beam && g.moving && g.plan) g.beam = beamOf(g.origin, g.dir, Math.max(0.01, (angleOf(g.plan, g.st) * D2R) / 2), obs);
    geos.push(g);
    out.geos.set(fx.id, g);
  }

  // footprints (under beams), then beams
  if (d.layers.footprints) for (const g of geos) drawFootprint(d, g);
  if (d.layers.beams) for (const g of geos) drawBeam(d, g);

  // bodies (sorted: far→near is irrelevant in 2D; selected last)
  const order = geos.slice().sort((a, b) => Number(d.selection.has(a.fx.id)) - Number(d.selection.has(b.fx.id)));
  for (const g of order) {
    const [sx, sy] = toScreenP(cam, view, g.pos);
    if (sx < -80 || sy < -80 || sx > cam.w + 80 || sy > cam.h + 80) continue;
    const sel = d.selection.has(g.fx.id);
    const r = drawBody({ ctx, th, view, scale: cam.scale, sel: sel && !d.clean }, g, sx, sy);
    out.fx.push({ id: g.fx.id, x: sx, y: sy, r: Math.max(14, r) });
    if (g.lit && g.st.strobeHz > 0) strobeBolt(ctx, sx + r * 0.85, sy - r * 0.85, 12, th.live);
    if (sel && !d.clean) {
      ctx.save();
      ctx.strokeStyle = th.accent; ctx.setLineDash([4, 3]); ctx.lineWidth = 1.4;
      ctx.beginPath(); ctx.arc(sx, sy, Math.max(14, r) + 4, 0, 7); ctx.stroke();
      ctx.restore();
    } else if (d.hover === g.fx.id) {
      ctx.save(); ctx.strokeStyle = th.dim; ctx.lineWidth = 1; ctx.beginPath(); ctx.arc(sx, sy, Math.max(14, r) + 3, 0, 7); ctx.stroke(); ctx.restore();
    }
    drawLabel(d, g, sx, sy, Math.max(14, r), sel);
  }

  // aim handles
  if (!d.clean && d.layers.handles) {
    for (const g of geos) {
      if (!g.moving || !g.beam || !d.selection.has(g.fx.id)) continue;
      const [sx, sy] = toScreenP(cam, view, g.pos);
      const [hx, hy] = toScreenP(cam, view, g.beam.hit);
      ctx.save();
      ctx.setLineDash([3, 4]); ctx.strokeStyle = th.accent; ctx.globalAlpha = 0.6; ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(sx, sy); ctx.lineTo(hx, hy); ctx.stroke();
      ctx.restore();
      ctx.save();
      ctx.lineWidth = 2; ctx.strokeStyle = th.accent; ctx.fillStyle = rgba(0.1, 0.45, 0.55, 0.45);
      ctx.beginPath(); ctx.arc(hx, hy, 9, 0, 7); ctx.fill(); ctx.stroke();
      ctx.beginPath(); ctx.moveTo(hx - 14, hy); ctx.lineTo(hx - 5, hy); ctx.moveTo(hx + 5, hy); ctx.lineTo(hx + 14, hy);
      ctx.moveTo(hx, hy - 14); ctx.lineTo(hx, hy - 5); ctx.moveTo(hx, hy + 5); ctx.lineTo(hx, hy + 14); ctx.stroke();
      ctx.restore();
      out.handles.push({ id: g.fx.id, x: hx, y: hy, target: g.beam.hit });
    }
  }

  if (!d.clean && d.rubber) {
    const r = d.rubber;
    ctx.save();
    ctx.fillStyle = rgba(0.24, 0.77, 0.91, 0.12); ctx.strokeStyle = th.accent; ctx.lineWidth = 1; ctx.setLineDash([5, 3]);
    ctx.fillRect(r.x0, r.y0, r.x1 - r.x0, r.y1 - r.y0); ctx.strokeRect(r.x0, r.y0, r.x1 - r.x0, r.y1 - r.y0);
    ctx.restore();
  }
  drawScaleBar(d);
  drawViewTag(d);
  return out;
}

// ── background / grid / stage ───────────────────────────

function drawBg(d: DrawInput) {
  const { ctx, cam, bg, bgImg } = d;
  if (!bg || !bgImg) return;
  const wM = bg.width, hM = bg.width * bg.aspect;
  const [cx, cy] = toScreen(cam, bg.cx, bg.cz);
  ctx.save();
  ctx.globalAlpha = bg.opacity;
  ctx.translate(cx, cy);
  ctx.rotate(bg.rot * D2R);
  ctx.drawImage(bgImg, (-wM / 2) * cam.scale, (-hM / 2) * cam.scale, wM * cam.scale, hM * cam.scale);
  ctx.restore();
}

function drawStageAndGrid(d: DrawInput) {
  const { ctx, cam, view, th, stage } = d;
  const sz = stage?.size ?? [12, 8, 6];
  const [w, dd, h] = sz;
  // visible range in view metres
  const u0 = (0 - cam.w / 2) / cam.scale + cam.cx, u1 = (cam.w - cam.w / 2) / cam.scale + cam.cx;
  const v0 = (0 - cam.h / 2) / cam.scale + cam.cy, v1 = (cam.h - cam.h / 2) / cam.scale + cam.cy;

  // stage floor / ground
  ctx.save();
  if (view === "plan") {
    const [x0, y0] = toScreen(cam, -w / 2, -dd / 2), [x1, y1] = toScreen(cam, w / 2, dd / 2);
    ctx.fillStyle = th.bg1; ctx.fillRect(x0, y0, x1 - x0, y1 - y0);
  } else {
    const half = view === "front" ? w / 2 : dd / 2;
    const [x0, y0] = toScreen(cam, -half, 0), [x1] = toScreen(cam, half, 0);
    const [, yb] = toScreen(cam, 0, 0.5);
    ctx.fillStyle = th.bg1; ctx.fillRect(x0, y0, x1 - x0, yb - y0);
    const [, yt] = toScreen(cam, 0, -h);
    ctx.fillStyle = rgba(0.06, 0.07, 0.08, 0.5); ctx.fillRect(x0, yt, x1 - x0, y0 - yt);
  }
  ctx.restore();

  if (d.layers.grid) {
    const minor = niceStep(26 / cam.scale);
    const major = minor * 5;
    ctx.save();
    ctx.font = `10px ${th.mono}`;
    ctx.textBaseline = "top";
    for (let pass = 0; pass < 2; pass++) {
      const step = pass === 0 ? minor : major;
      ctx.strokeStyle = pass === 0 ? th.line : th.lineStrong;
      ctx.globalAlpha = pass === 0 ? 0.55 : 0.9;
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let u = Math.ceil(u0 / step) * step; u <= u1; u += step) {
        if (pass === 0 && Math.abs(u / major - Math.round(u / major)) < 1e-6) continue;
        const [x] = toScreen(cam, u, 0);
        ctx.moveTo(Math.round(x) + 0.5, 0); ctx.lineTo(Math.round(x) + 0.5, cam.h);
      }
      for (let v = Math.ceil(v0 / step) * step; v <= v1; v += step) {
        if (pass === 0 && Math.abs(v / major - Math.round(v / major)) < 1e-6) continue;
        const [, y] = toScreen(cam, 0, v);
        ctx.moveTo(0, Math.round(y) + 0.5); ctx.lineTo(cam.w, Math.round(y) + 0.5);
      }
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    ctx.fillStyle = th.faint;
    for (let u = Math.ceil(u0 / major) * major; u <= u1; u += major) {
      const [x] = toScreen(cam, u, 0);
      ctx.fillText(`${+u.toFixed(2)}`, x + 3, 3);
    }
    for (let v = Math.ceil(v0 / major) * major; v <= v1; v += major) {
      const [, y] = toScreen(cam, 0, v);
      const val = view === "plan" ? v : -v;
      ctx.fillText(`${+val.toFixed(2)}`, 4, y + 3);
    }
    ctx.restore();
  }

  // stage outline + datum
  ctx.save();
  ctx.strokeStyle = th.dim; ctx.lineWidth = 1.5;
  ctx.font = `600 10px ${th.ui}`;
  ctx.fillStyle = th.dim;
  if (view === "plan") {
    const [x0, y0] = toScreen(cam, -w / 2, -dd / 2), [x1, y1] = toScreen(cam, w / 2, dd / 2);
    ctx.strokeRect(x0, y0, x1 - x0, y1 - y0);
    ctx.textAlign = "center"; ctx.textBaseline = "top";
    ctx.fillText("▼  " + t("AUDIENCE"), (x0 + x1) / 2, y1 + 8);
    ctx.textBaseline = "bottom";
    ctx.fillText(t("BACKSTAGE"), (x0 + x1) / 2, y0 - 6);
    // centre-line + plaster line
    ctx.setLineDash([8, 5]); ctx.strokeStyle = th.faint; ctx.lineWidth = 1;
    const [cx0] = toScreen(cam, 0, 0);
    ctx.beginPath(); ctx.moveTo(cx0, y0); ctx.lineTo(cx0, y1); ctx.stroke();
  } else {
    const half = view === "front" ? w / 2 : dd / 2;
    const [x0, y0] = toScreen(cam, -half, 0), [x1] = toScreen(cam, half, 0);
    ctx.beginPath(); ctx.moveTo(x0, y0); ctx.lineTo(x1, y0); ctx.stroke();
    const [, yh] = toScreen(cam, 0, -h);
    ctx.setLineDash([8, 5]); ctx.strokeStyle = th.faint; ctx.lineWidth = 1;
    ctx.beginPath(); ctx.moveTo(x0, yh); ctx.lineTo(x1, yh); ctx.stroke();
    ctx.setLineDash([]);
    ctx.textAlign = "left"; ctx.textBaseline = "bottom";
    ctx.fillText(t("GRID {h} m", { h }), x0 + 4, yh - 3);
    ctx.textBaseline = "top"; ctx.textAlign = view === "side" ? "right" : "center";
    ctx.fillText(view === "side" ? t("AUDIENCE") + "  ▶" : "◀ −X   ·   " + t("FRONT VIEW") + "   ·   +X ▶", view === "side" ? x1 - 4 : (x0 + x1) / 2, y0 + 8);
  }
  ctx.restore();
}

function drawViewTag(d: DrawInput) {
  if (d.clean) return;
  const { ctx, th, view } = d;
  ctx.save();
  ctx.font = `700 11px ${th.ui}`; ctx.fillStyle = th.faint; ctx.textAlign = "right"; ctx.textBaseline = "top";
  ctx.fillText(view === "plan" ? t("PLAN VIEW") : view === "front" ? t("FRONT ELEVATION") : t("SIDE ELEVATION"), d.cam.w - 12, 62);
  ctx.restore();
}

function drawScaleBar(d: DrawInput) {
  const { ctx, cam, th } = d;
  const step = niceStep(90 / cam.scale);
  const px = step * cam.scale;
  const x = 16, y = cam.h - 18;
  ctx.save();
  ctx.strokeStyle = th.text; ctx.fillStyle = th.text; ctx.lineWidth = 2;
  ctx.beginPath(); ctx.moveTo(x, y - 4); ctx.lineTo(x, y); ctx.lineTo(x + px, y); ctx.lineTo(x + px, y - 4); ctx.stroke();
  ctx.font = `600 11px ${th.mono}`; ctx.textBaseline = "bottom";
  ctx.fillText(step >= 1 ? `${step} m` : `${Math.round(step * 100)} cm`, x + px + 8, y + 4);
  ctx.restore();
}

// ── labels ──────────────────────────────────────────────

function drawLabel(d: DrawInput, g: FxGeo, sx: number, sy: number, r: number, sel: boolean) {
  if (d.labelMode === "none" || d.cam.scale < 14) return;
  const { ctx, th } = d;
  const fx = g.fx;
  const text = d.labelMode === "id" ? String(fx.id) : d.labelMode === "name" ? fx.name : `${fx.universe}.${fx.address}`;
  ctx.save();
  ctx.font = `600 ${sel ? 12 : 11}px ${th.mono}`;
  ctx.textAlign = "center"; ctx.textBaseline = "top";
  const y = sy + r + 3;
  const w = ctx.measureText(text).width + 8;
  if (sel) { ctx.fillStyle = th.accentDim; ctx.beginPath(); ctx.roundRect(sx - w / 2, y - 1, w, 15, 4); ctx.fill(); }
  else { ctx.fillStyle = rgba(0.05, 0.05, 0.06, 0.65); ctx.beginPath(); ctx.roundRect(sx - w / 2, y - 1, w, 15, 4); ctx.fill(); }
  ctx.fillStyle = sel ? th.text : th.dim;
  ctx.fillText(text, sx, y + 1);
  ctx.restore();
}

// ── beams ───────────────────────────────────────────────

function drawBeam(d: DrawInput, g: FxGeo) {
  const b = g.beam;
  if (!b || !g.lit) return;
  const { ctx, cam, view } = d;
  const st = g.st;
  const pts = [b.origin, ...b.ends].map((p) => toScreenP(cam, view, p));
  const poly = hull(pts);
  if (poly.length < 3) return;
  const [ox, oy] = toScreenP(cam, view, b.origin);
  const [hx, hy] = toScreenP(cam, view, b.hit);
  const dist = Math.hypot(hx - ox, hy - oy);
  const a = Math.min(0.4, (view === "plan" ? 0.05 : 0.10) + (view === "plan" ? 0.12 : 0.24) * st.bright) * (b.hitKind === "open" ? 0.8 : 1);
  ctx.save();
  ctx.globalCompositeOperation = "lighter";
  let fill: CanvasGradient | string;
  if (dist > 4) {
    const gr = ctx.createLinearGradient(ox, oy, hx, hy);
    gr.addColorStop(0, rgba(st.r, st.g, st.b, a));
    gr.addColorStop(1, rgba(st.r, st.g, st.b, b.hitKind === "open" ? 0 : a * 0.35));
    fill = gr;
  } else fill = rgba(st.r, st.g, st.b, a * 0.6);
  ctx.fillStyle = fill;
  ctx.beginPath();
  poly.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
  ctx.closePath(); ctx.fill();
  // crisp edges for the real cone (origin → two silhouette points)
  ctx.strokeStyle = rgba(st.r, st.g, st.b, Math.min(0.55, 0.15 + 0.3 * st.bright));
  ctx.lineWidth = 1;
  ctx.stroke();
  ctx.restore();
  if (g.moving) glow(ctx, ox, oy, symbolSize(cam.scale) * 0.7, g, st.bright * 0.5);
}

function drawFootprint(d: DrawInput, g: FxGeo) {
  const b = g.beam;
  if (!b || !g.lit || b.floorEnds.length < 4) return;
  const { ctx, cam, view, th } = d;
  const st = g.st;
  const pts = b.floorEnds.map((p) => toScreenP(cam, view, p));
  let minx = 1e9, maxx = -1e9, miny = 1e9, maxy = -1e9, cx = 0, cy = 0;
  for (const [x, y] of pts) { minx = Math.min(minx, x); maxx = Math.max(maxx, x); miny = Math.min(miny, y); maxy = Math.max(maxy, y); cx += x; cy += y; }
  cx /= pts.length; cy /= pts.length;
  ctx.save();
  if (view !== "plan" || (maxx - minx < 1.5 && maxy - miny < 1.5)) {
    // seen edge-on: a bright line on the floor
    ctx.globalCompositeOperation = "lighter";
    ctx.strokeStyle = rgba(st.r, st.g, st.b, Math.min(1, 0.4 + st.bright * 0.6));
    ctx.lineWidth = 4; ctx.lineCap = "round";
    ctx.beginPath(); ctx.moveTo(minx, cy); ctx.lineTo(Math.max(maxx, minx + 2), cy); ctx.stroke();
    ctx.restore();
    return;
  }
  const poly = hull(pts);
  const R = Math.max(maxx - minx, maxy - miny) / 2;
  ctx.globalCompositeOperation = "lighter";
  const gr = ctx.createRadialGradient(cx, cy, 0, cx, cy, Math.max(4, R));
  gr.addColorStop(0, rgba(st.r, st.g, st.b, Math.min(0.5, 0.12 + 0.34 * st.bright)));
  gr.addColorStop(0.8, rgba(st.r, st.g, st.b, Math.min(0.34, 0.08 + 0.24 * st.bright)));
  gr.addColorStop(1, rgba(st.r, st.g, st.b, 0.06));
  ctx.fillStyle = gr;
  ctx.beginPath(); poly.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y))); ctx.closePath(); ctx.fill();
  ctx.globalCompositeOperation = "source-over";
  ctx.strokeStyle = rgba(st.r, st.g, st.b, 0.85); ctx.lineWidth = 1.4; ctx.stroke();
  // gobo pattern inside the footprint
  if (g.ex.gobo && Math.min(maxx - minx, maxy - miny) > 16) {
    ctx.save();
    ctx.beginPath(); poly.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y))); ctx.closePath(); ctx.clip();
    const gim = g.ex.gobo.image ? loadImage(g.ex.gobo.image) : null;
    if (gim) {
      ctx.globalCompositeOperation = "multiply";
      ctx.drawImage(gim, minx, miny, maxx - minx, maxy - miny);
    } else {
      goboGlyph(ctx, cx, cy, Math.min(maxx - minx, maxy - miny) * 0.42, g.ex.gobo.index, g.ex.gobo.label);
    }
    ctx.restore();
  }
  if (g.ex.prism) {
    ctx.strokeStyle = rgba(st.r, st.g, st.b, 0.5); ctx.setLineDash([3, 3]);
    ctx.beginPath(); ctx.arc(cx, cy, R * 1.25, 0, 7); ctx.stroke();
  }
  void th;
  ctx.restore();
}

// ── stage objects ───────────────────────────────────────

function pointInPoly(x: number, y: number, poly: [number, number][]) {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i], [xj, yj] = poly[j];
    if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}
export { pointInPoly };

function drawObject(d: DrawInput, o: StageObject): [number, number][] | null {
  const { ctx, cam, view, th } = d;
  const ob = objObb(o);
  const pts = boxCorners(ob).map((p) => toScreenP(cam, view, p));
  let poly = hull(pts);
  const [cx, cy] = toScreenP(cam, view, ob.c);
  if (poly.length < 3) return null;
  const sel = d.selObj === o.id && !d.clean;
  const col = o.color || th.dim;
  ctx.save();
  ctx.lineJoin = "round";
  ctx.lineWidth = sel ? 2.2 : 1.4;
  const outline = sel ? th.accent : col;
  const path = (p: [number, number][]) => { ctx.beginPath(); p.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y))); ctx.closePath(); };
  const proj2 = (v: V3) => proj(view, apply(ob.R, v));
  switch (o.kind) {
    case "truss": {
      const A = proj2([ob.h[0], 0, 0]);
      const wy = proj2([0, ob.h[1], 0]), wz = proj2([0, 0, ob.h[2]]);
      const W = Math.hypot(wy[0], wy[1]) > Math.hypot(wz[0], wz[1]) ? wy : wz;
      const ax = A[0] * cam.scale, ay = A[1] * cam.scale, wx = W[0] * cam.scale, wy2 = W[1] * cam.scale;
      const L = Math.hypot(ax, ay);
      const half = Math.max(3, Math.hypot(wx, wy2));
      const nx = L > 1e-3 ? -ay / L : 0, ny = L > 1e-3 ? ax / L : 1;
      const wxx = nx * half, wyy = ny * half;
      ctx.fillStyle = rgba(0.6, 0.63, 0.66, 0.1);
      ctx.strokeStyle = outline;
      path(poly); ctx.fill(); ctx.stroke();
      if (L > 12) {
        ctx.lineWidth = 1;
        ctx.beginPath();
        const seg = Math.max(8, 0.5 * cam.scale);
        const n = Math.max(1, Math.floor((2 * L) / seg));
        for (let i = 0; i <= n; i++) {
          const t = -1 + (2 * i) / n;
          const px = cx + (ax * t), py = cy + (ay * t);
          const sgn = i % 2 ? 1 : -1;
          if (i === 0) ctx.moveTo(px + wxx * sgn, py + wyy * sgn); else ctx.lineTo(px + wxx * sgn, py + wyy * sgn);
        }
        ctx.stroke();
        ctx.beginPath(); ctx.moveTo(cx - ax, cy - ay); ctx.lineTo(cx + ax, cy + ay); ctx.stroke();
      }
      break;
    }
    case "platform": {
      ctx.fillStyle = rgba(0.16, 0.18, 0.2, 0.85); ctx.strokeStyle = outline;
      path(poly); ctx.fill(); ctx.stroke();
      ctx.save(); path(poly); ctx.clip(); ctx.strokeStyle = rgba(0.5, 0.55, 0.6, 0.18); ctx.lineWidth = 1;
      const xs = poly.map((p) => p[0]), ys = poly.map((p) => p[1]);
      const x0 = Math.min(...xs), x1 = Math.max(...xs), y0 = Math.min(...ys), y1 = Math.max(...ys);
      ctx.beginPath();
      for (let k = -(y1 - y0); k < x1 - x0; k += 9) { ctx.moveTo(x0 + k, y1); ctx.lineTo(x0 + k + (y1 - y0), y0); }
      ctx.stroke(); ctx.restore();
      break;
    }
    case "wall": {
      ctx.fillStyle = rgba(0.12, 0.13, 0.15, 0.95); ctx.strokeStyle = outline; ctx.lineWidth = sel ? 2.4 : 2;
      path(poly); ctx.fill(); ctx.stroke();
      break;
    }
    case "column": {
      if (view === "plan") {
        const r = (Math.max(o.size[0], o.size[2]) / 2) * cam.scale;
        ctx.fillStyle = rgba(0.23, 0.24, 0.27, 0.95); ctx.strokeStyle = outline;
        ctx.beginPath(); ctx.arc(cx, cy, Math.max(3, r), 0, 7); ctx.fill(); ctx.stroke();
        ctx.beginPath(); ctx.moveTo(cx - r * 0.7, cy - r * 0.7); ctx.lineTo(cx + r * 0.7, cy + r * 0.7); ctx.moveTo(cx + r * 0.7, cy - r * 0.7); ctx.lineTo(cx - r * 0.7, cy + r * 0.7); ctx.lineWidth = 1; ctx.stroke();
        poly = pts.length ? [[cx - r, cy - r], [cx + r, cy - r], [cx + r, cy + r], [cx - r, cy + r]] : poly;
      } else {
        ctx.fillStyle = rgba(0.23, 0.24, 0.27, 0.95); ctx.strokeStyle = outline; path(poly); ctx.fill(); ctx.stroke();
      }
      break;
    }
    case "curtain": {
      ctx.fillStyle = rgba(0.35, 0.05, 0.08, 0.35); ctx.strokeStyle = outline;
      if (view === "plan") {
        const A = proj2([ob.h[0], 0, 0]);
        const ax = A[0] * cam.scale, ay = A[1] * cam.scale, L = Math.hypot(ax, ay) || 1;
        const nx = -ay / L, ny = ax / L;
        ctx.lineWidth = 2; ctx.beginPath();
        const n = Math.max(4, Math.floor(L / 5));
        for (let i = 0; i <= n * 2; i++) {
          const t = -1 + i / n, s = i % 2 ? 3 : -3;
          const px = cx + ax * t + nx * s, py = cy + ay * t + ny * s;
          i ? ctx.lineTo(px, py) : ctx.moveTo(px, py);
        }
        ctx.strokeStyle = col; ctx.stroke();
      } else {
        path(poly); ctx.fill(); ctx.stroke();
        ctx.save(); path(poly); ctx.clip(); ctx.strokeStyle = col; ctx.globalAlpha = 0.45; ctx.lineWidth = 1;
        const xs = poly.map((p) => p[0]), ys = poly.map((p) => p[1]);
        ctx.beginPath(); for (let x = Math.min(...xs); x < Math.max(...xs); x += 7) { ctx.moveTo(x, Math.min(...ys)); ctx.lineTo(x, Math.max(...ys)); } ctx.stroke(); ctx.restore();
      }
      break;
    }
    case "tree": {
      ctx.fillStyle = rgba(0.12, 0.3, 0.14, 0.85); ctx.strokeStyle = outline;
      if (view === "plan") {
        const r = (Math.max(o.size[0], o.size[2]) / 2) * cam.scale;
        ctx.beginPath(); ctx.arc(cx, cy, Math.max(4, r), 0, 7); ctx.fill(); ctx.stroke();
        ctx.beginPath(); ctx.arc(cx, cy, Math.max(2, r * 0.18), 0, 7); ctx.fillStyle = rgba(0.3, 0.2, 0.1, 1); ctx.fill();
        poly = [[cx - r, cy - r], [cx + r, cy - r], [cx + r, cy + r], [cx - r, cy + r]];
      } else {
        const [, yb] = toScreenP(cam, view, [ob.c[0], ob.c[1] - ob.h[1], ob.c[2]]);
        const [, yt] = toScreenP(cam, view, [ob.c[0], ob.c[1] + ob.h[1], ob.c[2]]);
        const rw = (ob.h[0]) * cam.scale;
        ctx.fillStyle = rgba(0.3, 0.2, 0.1, 1); ctx.fillRect(cx - 2, yt + (yb - yt) * 0.5, 4, (yb - yt) * 0.5);
        ctx.fillStyle = rgba(0.12, 0.3, 0.14, 0.85);
        ctx.beginPath(); ctx.ellipse(cx, yt + (yb - yt) * 0.32, Math.max(4, rw), Math.max(4, (yb - yt) * 0.34), 0, 0, 7); ctx.fill(); ctx.stroke();
      }
      break;
    }
    case "screen": {
      const led = o.screen_type === "led";
      const mc = o.media?.kind === "color" && o.media.src ? o.media.src : null;
      ctx.fillStyle = mc ?? (led ? rgba(0.08, 0.1, 0.13, 1) : rgba(0.85, 0.86, 0.88, 0.85));
      ctx.strokeStyle = sel ? th.accent : led ? th.accent : th.text;
      ctx.lineWidth = view === "plan" ? 3 : sel ? 2.4 : 1.6;
      path(poly); ctx.fill(); ctx.stroke();
      ctx.fillStyle = led ? th.accent : th.bg0;
      ctx.font = `700 9px ${th.ui}`; ctx.textAlign = "center"; ctx.textBaseline = "middle";
      if (view !== "plan") ctx.fillText(led ? "LED" : t("PROJ"), cx, cy);
      break;
    }
    default: {
      ctx.fillStyle = rgba(0.2, 0.22, 0.25, 0.9); ctx.strokeStyle = outline;
      path(poly); ctx.fill(); ctx.stroke();
      ctx.lineWidth = 1; ctx.beginPath();
      ctx.moveTo(poly[0][0], poly[0][1]); ctx.lineTo(poly[Math.floor(poly.length / 2)][0], poly[Math.floor(poly.length / 2)][1]); ctx.stroke();
    }
  }
  if (sel) {
    ctx.strokeStyle = th.accent; ctx.lineWidth = 1; ctx.setLineDash([4, 3]);
    path(poly); ctx.stroke();
  }
  if (cam.scale > 26 && o.name) {
    ctx.font = `600 10px ${th.ui}`; ctx.fillStyle = th.faint; ctx.textAlign = "center"; ctx.textBaseline = "top";
    const ys = poly.map((p) => p[1]);
    ctx.fillText(o.name, cx, Math.max(...ys) + 3);
  }
  ctx.restore();
  return poly;
}

export function fitCam(view: ViewKind, stage: Stage | null, fixtures: Fixture[], w: number, h: number): Cam {
  const sz = stage?.size ?? [12, 8, 6];
  let u0 = 0, u1 = 0, v0 = 0, v1 = 0;
  const pad = 1.4;
  if (view === "plan") { u0 = -sz[0] / 2; u1 = sz[0] / 2; v0 = -sz[1] / 2; v1 = sz[1] / 2; }
  else if (view === "front") { u0 = -sz[0] / 2; u1 = sz[0] / 2; v0 = -sz[2]; v1 = 0; }
  else { u0 = -sz[1] / 2; u1 = sz[1] / 2; v0 = -sz[2]; v1 = 0; }
  for (const f of fixtures) {
    const [u, v] = proj(view, [f.pos[0], f.pos[1], f.pos[2]]);
    u0 = Math.min(u0, u); u1 = Math.max(u1, u); v0 = Math.min(v0, v); v1 = Math.max(v1, v);
  }
  const bw = (u1 - u0 + pad * 2) || 1, bh = (v1 - v0 + pad * 2) || 1;
  const scale = Math.max(4, Math.min((w - 40) / bw, (h - 110) / bh));
  return { cx: (u0 + u1) / 2, cy: (v0 + v1) / 2 + (view === "plan" ? 0.2 : 0), scale, w, h };
}

// ── smoke layer: optical-depth projection of the shared smoke simulation ──
let smokeCv: HTMLCanvasElement | null = null;
let smokeKey = "";

function drawSmoke(d: DrawInput) {
  const sim = getSmoke();
  const { ctx, cam, view, stage } = d;
  if (stage) sim.setStageSize([stage.size[0], stage.size[1], stage.size[2]]);
  sim.sync(d.fixtures, d.profiles, d.frames);
  sim.update(d.time);
  if (!sim.hasActivity() || sim.meanDensity < 1e-6) return;
  const key = `${view}|${sim.version}|${sim.nx}x${sim.ny}x${sim.nz}`;
  const pr = sim.projection(view);
  if (!smokeCv) smokeCv = document.createElement("canvas");
  if (key !== smokeKey || smokeCv.width !== pr.w || smokeCv.height !== pr.h) {
    smokeCv.width = pr.w; smokeCv.height = pr.h;
    const g = smokeCv.getContext("2d")!;
    const img = g.createImageData(pr.w, pr.h);
    const flip = view !== "plan"; // elevations: world y grows upwards, screen v downwards
    for (let r = 0; r < pr.h; r++) {
      const srcRow = flip ? pr.h - 1 - r : r;
      for (let c = 0; c < pr.w; c++) {
        const tau = pr.data[c + pr.w * srcRow];
        const a = 1 - Math.exp(-0.5 * tau);
        const o = (c + pr.w * r) * 4;
        img.data[o] = 196; img.data[o + 1] = 210; img.data[o + 2] = 232; img.data[o + 3] = Math.min(255, a * 190);
      }
    }
    g.putImageData(img, 0, 0);
    smokeKey = key;
  }
  const [xa, ya] = toScreen(cam, pr.u0, view === "plan" ? pr.v0 : -pr.v1);
  const [xb, yb] = toScreen(cam, pr.u1, view === "plan" ? pr.v1 : -pr.v0);
  ctx.save();
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(smokeCv, xa, ya, xb - xa, yb - ya);
  ctx.restore();
}
