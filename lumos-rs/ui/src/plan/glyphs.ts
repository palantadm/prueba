// Canvas2D fixture symbols for plan / elevation views.
import type { FxGeo } from "./fixture";
import { apply, proj, type ViewKind } from "./math";

export interface Theme {
  bg0: string; bg1: string; bg2: string; bg3: string; bg4: string; line: string; lineStrong: string;
  text: string; dim: string; faint: string; accent: string; accentDim: string; live: string; go: string; stop: string;
  mono: string; ui: string;
}

export function readTheme(): Theme {
  const cs = getComputedStyle(document.documentElement);
  const g = (n: string, d: string) => cs.getPropertyValue(n).trim() || d;
  return {
    bg0: g("--bg-0", "#0b0c0e"), bg1: g("--bg-1", "#111316"), bg2: g("--bg-2", "#181b1f"), bg3: g("--bg-3", "#20242a"), bg4: g("--bg-4", "#2a2f37"),
    line: g("--line", "#272b32"), lineStrong: g("--line-strong", "#343a43"), text: g("--text", "#d9dde3"), dim: g("--text-dim", "#8a93a0"),
    faint: g("--text-faint", "#5a626d"), accent: g("--accent", "#3cc4e8"), accentDim: g("--accent-dim", "#1b5666"), live: g("--live", "#f5a524"),
    go: g("--go", "#35c46a"), stop: g("--stop", "#e5484d"), mono: g("--font-mono", "monospace"), ui: g("--font-ui", "sans-serif"),
  };
}

export const rgba = (r: number, g: number, b: number, a: number) =>
  `rgba(${Math.round(r * 255)},${Math.round(g * 255)},${Math.round(b * 255)},${a})`;

/** Symbol diameter in px for a given zoom (grows with zoom but stays readable). */
export const symbolSize = (scale: number) => Math.max(15, Math.min(54, 0.6 * scale));

function rr(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
}

/** Angle on screen of the fixture's local +X axis in `view`. */
function localAngle(g: FxGeo, view: ViewKind): number {
  const [u, v] = proj(view, apply(g.R, [1, 0, 0]));
  return Math.atan2(v, u);
}

export interface GlyphCtx {
  ctx: CanvasRenderingContext2D;
  th: Theme;
  view: ViewKind;
  scale: number;
  /** Selected / hovered. */
  sel: boolean;
}

/** Draw the fixture body at screen (sx, sy). Returns the approximate hit radius. */
export function drawBody(c: GlyphCtx, g: FxGeo, sx: number, sy: number): number {
  const { ctx, th, view } = c;
  const S = symbolSize(c.scale);
  const st = g.st;
  const on = g.lit;
  const lensA = on ? 0.35 + 0.65 * st.bright : 0;
  const lens = on ? rgba(st.r, st.g, st.b, 1) : th.bg3;
  const stroke = c.sel ? th.accent : on ? th.text : th.dim;
  ctx.save();
  ctx.translate(sx, sy);
  ctx.lineWidth = c.sel ? 2 : 1.4;
  ctx.strokeStyle = stroke;
  ctx.fillStyle = th.bg2;
  ctx.lineJoin = "round";
  ctx.lineCap = "round";

  if (g.glyph === "mover") {
    if (view === "plan") planMover(c, g, S, lens, lensA);
    else elevMover(c, g, S, lens, lensA);
    ctx.restore();
    return S * 0.75;
  }

  ctx.rotate(localAngle(g, view));
  const long = view === "plan" ? 1 : 1; // bars keep their width in every view
  switch (g.glyph) {
    case "bar": {
      const L = Math.max(S * 2.3, c.scale * 1.0) * long, H = S * 0.38;
      rr(ctx, -L / 2, -H / 2, L, H, 3); ctx.fill(); ctx.stroke();
      const n = 8;
      for (let i = 0; i < n; i++) {
        ctx.beginPath();
        ctx.arc(-L / 2 + (L * (i + 0.5)) / n, 0, H * 0.28, 0, 7);
        ctx.fillStyle = on ? lens : th.bg4; ctx.fill();
      }
      ctx.restore();
      return L / 2;
    }
    case "strobe": {
      const W = S * 1.5, H = S * 0.75;
      rr(ctx, -W / 2, -H / 2, W, H, 3); ctx.fill(); ctx.stroke();
      ctx.fillStyle = on ? lens : th.bg4;
      for (let i = 0; i < 4; i++) for (let j = 0; j < 2; j++) ctx.fillRect(-W / 2 + 3 + i * (W - 6) / 4 + 1, -H / 2 + 3 + j * (H - 6) / 2 + 1, (W - 6) / 4 - 2, (H - 6) / 2 - 2);
      ctx.restore();
      return W / 2;
    }
    case "blinder": {
      const W = S * 1.4, H = S * 0.7;
      rr(ctx, -W / 2, -H / 2, W, H, 4); ctx.fill(); ctx.stroke();
      for (const k of [-0.25, 0.25]) {
        ctx.beginPath(); ctx.arc(W * k, 0, H * 0.3, 0, 7);
        ctx.fillStyle = on ? lens : th.bg4; ctx.fill(); ctx.stroke();
      }
      ctx.restore();
      return W / 2;
    }
    case "fog": {
      rr(ctx, -S * 0.6, -S * 0.35, S * 1.2, S * 0.7, 5); ctx.fill(); ctx.stroke();
      ctx.fillStyle = th.dim;
      for (const [x, y, r] of [[-0.25, 0.05, 0.18], [0, -0.05, 0.22], [0.26, 0.05, 0.16]]) { ctx.beginPath(); ctx.arc(S * x, S * y, S * r, 0, 7); ctx.fill(); }
      ctx.restore();
      return S * 0.6;
    }
    case "other": {
      rr(ctx, -S * 0.42, -S * 0.42, S * 0.84, S * 0.84, 4); ctx.fill(); ctx.stroke();
      ctx.beginPath(); ctx.arc(0, 0, S * 0.18, 0, 7); ctx.fillStyle = on ? lens : th.bg4; ctx.fill();
      ctx.restore();
      return S * 0.5;
    }
    default: {
      // par / ledpar / fresnel / profile: round can with yoke brackets
      const R = S * 0.46;
      ctx.beginPath(); ctx.moveTo(-R * 1.35, 0); ctx.lineTo(-R, 0); ctx.moveTo(R, 0); ctx.lineTo(R * 1.35, 0); ctx.stroke();
      ctx.beginPath(); ctx.arc(0, 0, R, 0, 7); ctx.fill(); ctx.stroke();
      if (g.glyph === "ledpar") {
        const dots: [number, number][] = [[0, 0], ...Array.from({ length: 6 }, (_, i) => [Math.cos((i * Math.PI) / 3), Math.sin((i * Math.PI) / 3)] as [number, number])];
        for (const [x, y] of dots) { ctx.beginPath(); ctx.arc(x * R * 0.55, y * R * 0.55, R * 0.2, 0, 7); ctx.fillStyle = on ? lens : th.bg4; ctx.fill(); }
      } else if (g.glyph === "fresnel") {
        ctx.beginPath(); ctx.arc(0, 0, R * 0.66, 0, 7); ctx.moveTo(R * 0.36, 0); ctx.arc(0, 0, R * 0.36, 0, 7);
        ctx.lineWidth = 1; ctx.stroke(); ctx.lineWidth = c.sel ? 2 : 1.4;
        ctx.beginPath(); ctx.arc(0, 0, R * 0.22, 0, 7); ctx.fillStyle = on ? lens : th.bg4; ctx.fill();
        for (const a of [0, 1, 2, 3]) { // barn doors
          const t = (a * Math.PI) / 2 + Math.PI / 4;
          ctx.beginPath(); ctx.moveTo(Math.cos(t) * R, Math.sin(t) * R); ctx.lineTo(Math.cos(t) * R * 1.35, Math.sin(t) * R * 1.35); ctx.stroke();
        }
      } else if (g.glyph === "profile") {
        ctx.beginPath(); ctx.moveTo(-R * 0.55, -R * 0.55); ctx.lineTo(R * 0.55, R * 0.55); ctx.moveTo(R * 0.55, -R * 0.55); ctx.lineTo(-R * 0.55, R * 0.55);
        ctx.lineWidth = 1; ctx.stroke(); ctx.lineWidth = c.sel ? 2 : 1.4;
        ctx.beginPath(); ctx.arc(0, 0, R * 0.3, 0, 7); ctx.fillStyle = on ? lens : th.bg4; ctx.fill();
      } else {
        ctx.beginPath(); ctx.arc(0, 0, R * 0.62, 0, 7); ctx.fillStyle = on ? lens : th.bg4; ctx.fill();
        ctx.beginPath(); ctx.arc(0, 0, R * 0.62, 0, 7); ctx.lineWidth = 1; ctx.stroke();
      }
      ctx.restore();
      return R * 1.35;
    }
  }
  void lensA;
}

// ── moving head ─────────────────────────────────────────

function planMover(c: GlyphCtx, g: FxGeo, S: number, lens: string, lensA: number) {
  const { ctx, th } = c;
  const st = g.st;
  // heading of the pan axis (beam direction when tilt → 90°), in plan
  const hd = apply(g.R, [-Math.sin(g.P), 0, -Math.cos(g.P)]);
  const ang = Math.atan2(hd[2], hd[0]);
  const sinT = Math.sin(g.T);
  const fore = Math.min(1, Math.abs(sinT));
  const bh = [g.dir[0], g.dir[2]];
  // base (rotates with the fixture yaw)
  ctx.save();
  ctx.rotate(Math.atan2(apply(g.R, [1, 0, 0])[2], apply(g.R, [1, 0, 0])[0]));
  rr(ctx, -S * 0.34, -S * 0.34, S * 0.68, S * 0.68, 5); ctx.fill(); ctx.stroke();
  ctx.restore();
  // yoke + head along the heading line
  ctx.save();
  ctx.rotate(ang);
  const arm = S * 0.5;
  ctx.lineWidth = 3; ctx.strokeStyle = th.dim;
  ctx.beginPath(); ctx.moveTo(-arm, -S * 0.36); ctx.lineTo(arm, -S * 0.36); ctx.moveTo(-arm, S * 0.36); ctx.lineTo(arm, S * 0.36); ctx.stroke();
  ctx.lineWidth = c.sel ? 2 : 1.4; ctx.strokeStyle = c.sel ? th.accent : g.lit ? th.text : th.dim;
  const L = S * (0.62 + 0.5 * fore), Wd = S * 0.46;
  const off = Math.sign(sinT || 1) * S * 0.12 * fore;
  rr(ctx, -L / 2 + off, -Wd / 2, L, Wd, Wd / 2); ctx.fillStyle = th.bg3; ctx.fill(); ctx.stroke();
  ctx.restore();
  // lens
  const tx = bh[0] * S * 0.42, ty = bh[1] * S * 0.42;
  ctx.beginPath(); ctx.arc(tx, ty, S * 0.15, 0, 7); ctx.fillStyle = lens; ctx.fill();
  ctx.lineWidth = 1; ctx.strokeStyle = c.sel ? th.accent : th.dim; ctx.stroke();
  if (lensA > 0) glow(ctx, tx, ty, S * 0.9, g, lensA * 0.5);
  // pan direction arrow
  const mag = Math.hypot(bh[0], bh[1]);
  if (mag > 0.08) {
    const ux = bh[0] / mag, uy = bh[1] / mag;
    const x0 = ux * S * 0.62, y0 = uy * S * 0.62, x1 = ux * S * 0.98, y1 = uy * S * 0.98;
    ctx.strokeStyle = c.sel ? th.accent : th.live; ctx.fillStyle = ctx.strokeStyle; ctx.lineWidth = 1.6;
    ctx.beginPath(); ctx.moveTo(x0, y0); ctx.lineTo(x1, y1); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(x1 + ux * 5, y1 + uy * 5); ctx.lineTo(x1 - ux * 1 - uy * 4, y1 - uy * 1 + ux * 4); ctx.lineTo(x1 - ux * 1 + uy * 4, y1 - uy * 1 - ux * 4); ctx.closePath(); ctx.fill();
  } else {
    ctx.strokeStyle = th.live; ctx.lineWidth = 1.4;
    ctx.beginPath(); ctx.arc(0, 0, S * 0.09, 0, 7); ctx.stroke();
  }
  void st;
}

function elevMover(c: GlyphCtx, g: FxGeo, S: number, lens: string, lensA: number) {
  const { ctx, th, view } = c;
  // axis of the head projected into the view
  const [au, av] = proj(view, g.dir);
  const m = Math.hypot(au, av);
  const ax = m > 1e-3 ? au / m : 0, ay = m > 1e-3 ? av / m : 1;
  const fore = Math.max(0.38, m);
  const pivotY = S * 0.62;
  // base
  ctx.fillStyle = th.bg2;
  rr(ctx, -S * 0.3, -S * 0.1, S * 0.6, S * 0.14, 3); ctx.fill(); ctx.stroke();
  // yoke
  ctx.lineWidth = 3; const keep = ctx.strokeStyle; ctx.strokeStyle = th.dim;
  ctx.beginPath(); ctx.moveTo(-S * 0.26, S * 0.04); ctx.lineTo(-S * 0.26, pivotY); ctx.moveTo(S * 0.26, S * 0.04); ctx.lineTo(S * 0.26, pivotY); ctx.stroke();
  ctx.strokeStyle = keep; ctx.lineWidth = c.sel ? 2 : 1.4;
  // head capsule centred on the pivot, lens at the beam end
  ctx.save();
  ctx.translate(0, pivotY);
  ctx.rotate(Math.atan2(ay, ax));
  const Lh = S * 0.95 * fore, Wd = S * 0.5;
  rr(ctx, -Lh * 0.34, -Wd / 2, Lh, Wd, Wd / 2); ctx.fillStyle = th.bg3; ctx.fill(); ctx.stroke();
  ctx.beginPath(); ctx.arc(-Lh * 0.34, 0, 2.5, 0, 7); ctx.fillStyle = th.dim; ctx.fill();
  ctx.beginPath(); ctx.ellipse(Lh * 0.66 - 1, 0, 3 + Wd * 0.1 * (1 - fore), Wd * 0.4, 0, 0, 7); ctx.fillStyle = lens; ctx.fill();
  ctx.restore();
  if (lensA > 0) glow(ctx, ax * Lh * 0.66, pivotY + ay * Lh * 0.66, S * 0.9, g, lensA * 0.5);
}

export function glow(ctx: CanvasRenderingContext2D, x: number, y: number, r: number, g: FxGeo, a: number) {
  const st = g.st;
  const gr = ctx.createRadialGradient(x, y, 0, x, y, r);
  gr.addColorStop(0, rgba(st.r, st.g, st.b, Math.min(1, a)));
  gr.addColorStop(1, rgba(st.r, st.g, st.b, 0));
  const prev = ctx.globalCompositeOperation;
  ctx.globalCompositeOperation = "lighter";
  ctx.fillStyle = gr;
  ctx.beginPath(); ctx.arc(x, y, r, 0, 7); ctx.fill();
  ctx.globalCompositeOperation = prev;
}

export function strobeBolt(ctx: CanvasRenderingContext2D, x: number, y: number, s: number, color: string) {
  ctx.save();
  ctx.translate(x, y);
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.moveTo(s * 0.15, -s * 0.5); ctx.lineTo(-s * 0.3, s * 0.08); ctx.lineTo(-s * 0.02, s * 0.08); ctx.lineTo(-s * 0.15, s * 0.5);
  ctx.lineTo(s * 0.32, -s * 0.12); ctx.lineTo(s * 0.04, -s * 0.12); ctx.closePath();
  ctx.fill();
  ctx.restore();
}

/** Procedural gobo pattern drawn as a dark overlay inside the footprint (cx, cy, radius r px). */
export function goboGlyph(ctx: CanvasRenderingContext2D, cx: number, cy: number, r: number, index: number, label: string) {
  let h = 0;
  for (let i = 0; i < label.length; i++) h = (h * 31 + label.charCodeAt(i)) >>> 0;
  const kind = (index + h) % 6;
  ctx.save();
  ctx.translate(cx, cy);
  ctx.fillStyle = "rgba(0,0,0,0.55)";
  ctx.strokeStyle = "rgba(0,0,0,0.6)";
  ctx.lineWidth = Math.max(1, r * 0.1);
  switch (kind) {
    case 0: // dots
      for (let i = 0; i < 7; i++) { const a = (i / 7) * 6.283, d = i === 0 ? 0 : r * 0.55; ctx.beginPath(); ctx.arc(Math.cos(a) * d, Math.sin(a) * d, r * 0.14, 0, 7); ctx.fill(); }
      break;
    case 1: // stripes
      for (let i = -2; i <= 2; i++) ctx.fillRect(-r, i * r * 0.36 - r * 0.07, r * 2, r * 0.14);
      break;
    case 2: // star
      ctx.beginPath();
      for (let i = 0; i < 10; i++) { const a = (i / 10) * 6.283 - 1.57, d = i % 2 ? r * 0.28 : r * 0.78; ctx[i ? "lineTo" : "moveTo"](Math.cos(a) * d, Math.sin(a) * d); }
      ctx.closePath(); ctx.fill();
      break;
    case 3: // rings
      for (const k of [0.35, 0.65, 0.92]) { ctx.beginPath(); ctx.arc(0, 0, r * k, 0, 7); ctx.stroke(); }
      break;
    case 4: // triangle breakup
      ctx.beginPath(); ctx.moveTo(0, -r * 0.7); ctx.lineTo(r * 0.65, r * 0.5); ctx.lineTo(-r * 0.65, r * 0.5); ctx.closePath(); ctx.fill();
      break;
    default: // cross
      ctx.fillRect(-r * 0.09, -r * 0.8, r * 0.18, r * 1.6); ctx.fillRect(-r * 0.8, -r * 0.09, r * 1.6, r * 0.18);
  }
  ctx.restore();
}
