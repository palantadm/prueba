import { useEffect, useRef, useState } from "preact/hooks";
import "../styles/plan.css";
import { library, selection, show, toast } from "../store";
import { releaseDmx, requestDmx } from "../lib/dmxsub";
import { PlanController } from "../plan/controller";
import { aimMode, currentBg, editLayout, hud, labelMode, layers, menu, selObj, setBg, tool, view, type LabelMode } from "../plan/state";
import { promptDialog, DialogHost } from "../ui";
import { angleOf } from "../plan/fixture";
import type { ViewKind } from "../plan/math";

const VIEWS: [ViewKind, string, string][] = [["plan", "Plan", "Top-down plan"], ["front", "Front", "Front elevation (from the audience)"], ["side", "Side", "Side elevation"]];

function Seg<T extends string>(p: { value: T; opts: [T, string, string?][]; onPick: (v: T) => void }) {
  return (
    <div class="pl-group" role="group">
      {p.opts.map(([v, label, title]) => (
        <button key={v} class={`btn pl-btn ${p.value === v ? "sel" : ""}`} title={title} aria-pressed={p.value === v} onClick={() => p.onPick(v)}>{label}</button>
      ))}
    </div>
  );
}

async function uploadImage(file: File): Promise<string> {
  const fd = new FormData();
  fd.append("file", file);
  const r = await fetch("/api/media", { method: "POST", body: fd });
  if (!r.ok) throw new Error(`Upload failed (${r.status})`);
  return ((await r.json()) as { url: string }).url;
}

function BackgroundPanel(p: { ctl: PlanController; close: () => void }) {
  const bg = currentBg();
  const [align, setAlign] = useState(true);
  const file = useRef<HTMLInputElement>(null);
  const pick = async (f: File | undefined) => {
    if (!f) return;
    try {
      const url = await uploadImage(f);
      const im = new Image();
      im.onload = () => {
        setBg({ url, aspect: im.naturalHeight / im.naturalWidth, width: show.value?.stage.size[0] ?? 12, cx: 0, cz: 0, rot: 0, opacity: 0.5 });
        layers.value = { ...layers.value, background: true };
        view.value = "plan";
        toast("Floor plan loaded — use Calibrate to set its scale", "ok");
      };
      im.src = url;
    } catch (e) { toast((e as Error).message, "err"); }
  };
  const calibrate = () => {
    view.value = "plan";
    p.close();
    toast("Click point A, then point B on the plan image", "info", 5000);
    p.ctl.calibrate = {
      pts: [],
      done: async (pts) => {
        const v = await promptDialog("Calibrate floor plan", "Real distance between the two points (metres)", "5");
        const m = v === null ? NaN : parseFloat(v.replace(",", "."));
        if (m > 0) { p.ctl.applyCalibration(pts, m, align); toast("Plan calibrated", "ok"); }
      },
    };
  };
  const num = (label: string, val: number, set: (n: number) => void, step = 0.1) => (
    <label class="pl-num"><span>{label}</span>
      <input class="input num" type="number" step={step} value={+val.toFixed(3)} onInput={(e) => { const n = parseFloat((e.target as HTMLInputElement).value); if (!Number.isNaN(n)) set(n); }} />
    </label>
  );
  return (
    <div class="pl-pop" onPointerDown={(e) => e.stopPropagation()}>
      <div class="label">Floor-plan background</div>
      <div class="row wrap">
        <button class="btn pl-btn" onClick={() => file.current?.click()}>{bg ? "Replace image…" : "Upload image…"}</button>
        {bg && <button class="btn pl-btn danger" onClick={() => setBg(null)}>Remove</button>}
        <input ref={file} type="file" accept="image/*" hidden onChange={(e) => pick((e.target as HTMLInputElement).files?.[0])} />
      </div>
      {bg ? (
        <>
          <label class="pl-num"><span>Opacity</span>
            <input type="range" min="0.05" max="1" step="0.05" value={bg.opacity} onInput={(e) => setBg({ opacity: parseFloat((e.target as HTMLInputElement).value) })} />
          </label>
          {num("Width m", bg.width, (n) => n > 0.1 && setBg({ width: n }), 0.5)}
          {num("Rotation °", bg.rot, (n) => setBg({ rot: n }), 0.5)}
          {num("Offset X m", bg.cx, (n) => setBg({ cx: n }))}
          {num("Offset Z m", bg.cz, (n) => setBg({ cz: n }))}
          <label class="check"><input type="checkbox" checked={align} onChange={(e) => setAlign((e.target as HTMLInputElement).checked)} /> Align picked line to horizontal</label>
          <button class="btn pl-btn primary" onClick={calibrate}>Calibrate with two points…</button>
        </>
      ) : <div class="faint" style={{ fontSize: "var(--fs-xs)" }}>Upload a PNG/JPG of the venue plan, then calibrate it by clicking two points a known distance apart.</div>}
    </div>
  );
}

function LayersPanel() {
  const L = layers.value;
  const t = (k: keyof typeof L, label: string) => (
    <label class="check pl-check"><input type="checkbox" checked={L[k]} onChange={(e) => (layers.value = { ...L, [k]: (e.target as HTMLInputElement).checked })} /> {label}</label>
  );
  return (
    <div class="pl-pop" onPointerDown={(e) => e.stopPropagation()}>
      <div class="label">Layers</div>
      {t("beams", "Beams")}{t("footprints", "Footprints (floor)")}{t("objects", "Stage objects")}{t("grid", "Grid & rulers")}{t("background", "Floor-plan image")}{t("handles", "Aim handles")}
    </div>
  );
}

function Legend() {
  const items: [string, string][] = [
    ["○", "Par"], ["◉", "LED par"], ["◎", "Fresnel"], ["⊗", "Profile"], ["▭", "Bar / blinder"], ["▦", "Strobe"],
    ["⌖", "Aim handle (drag)"], ["⚡", "Strobing"], ["⬭", "Floor footprint"], ["◣", "Beam cone"],
  ];
  return (
    <div class="pl-legend">
      <div class="label">Legend</div>
      <div class="pl-leg-grid">{items.map(([g, t]) => <span key={t}><b>{g}</b>{t}</span>)}</div>
      <div class="faint pl-leg-note">Moving head: base · yoke · head turns with PAN, the lens end follows TILT; arrow = beam heading.</div>
    </div>
  );
}

function Info(p: { ctl: PlanController }) {
  const [, tick] = useState(0);
  useEffect(() => { const t = setInterval(() => tick((n) => n + 1), 250); return () => clearInterval(t); }, []);
  const ids = selection.value;
  if (ids.length !== 1) return ids.length > 1 ? <div class="pl-info"><b>{ids.length}</b> fixtures selected · aim mode <b>{aimMode.value === "stack" ? "Stack" : "Fan"}</b></div> : null;
  const g = p.ctl.geoOf(ids[0]);
  if (!g) return null;
  const deg = 180 / Math.PI;
  return (
    <div class="pl-info">
      <b>{g.fx.id}</b> {g.fx.name}<span class="faint"> · {g.prof?.manufacturer} {g.prof?.model}</span>
      {g.moving && <div class="mono">Pan {(g.P * deg).toFixed(0)}° · Tilt {(g.T * deg).toFixed(0)}°{g.plan ? ` · Beam ${angleOf(g.plan, g.st).toFixed(0)}°` : ""}{g.beam ? ` · Throw ${g.beam.dist.toFixed(1)} m` : ""}</div>}
      {!g.moving && <div class="mono">U{g.fx.universe}.{g.fx.address} · {(g.st.bright * 100).toFixed(0)}%</div>}
    </div>
  );
}

export function Plan2D() {
  const host = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const ctl = useRef<PlanController | null>(null);
  const [pop, setPop] = useState<"layers" | "bg" | null>(null);
  const [legend, setLegend] = useState(false);

  useEffect(() => {
    requestDmx("plan", null);
    const c = new PlanController(canvas.current!);
    ctl.current = c;
    (window as unknown as { __plan?: PlanController }).__plan = c;
    return () => { c.dispose(); releaseDmx("plan"); ctl.current = null; delete (window as unknown as { __plan?: PlanController }).__plan; };
  }, []);

  useEffect(() => { ctl.current?.fit(); }, [library.value.length === 0]);

  const m = menu.value;
  const hv = hud.value;
  const nMov = show.value?.fixtures.length ?? 0;
  return (
    <div class="pl-root" ref={host} onPointerDown={() => pop && setPop(null)}>
      <canvas ref={canvas} class="pl-canvas" />
      <div class="pl-bar" onPointerDown={(e) => e.stopPropagation()}>
        <Seg value={view.value} opts={VIEWS} onPick={(v) => (view.value = v)} />
        <div class="pl-group" role="group">
          <button class={`btn pl-btn ${tool.value === "select" ? "sel" : ""}`} title="Select / drag (Esc)" onClick={() => (tool.value = "select")}>↖ Select</button>
          <button class={`btn pl-btn ${tool.value === "hand" ? "sel" : ""}`} title="Pan the view (H, or hold Space)" onClick={() => (tool.value = tool.value === "hand" ? "select" : "hand")}>✋ Pan</button>
          <button class={`btn pl-btn ${tool.value === "aim" ? "sel" : ""}`} title="Click the stage to aim selected moving heads (A)" onClick={() => (tool.value = tool.value === "aim" ? "select" : "aim")}>⌖ Aim</button>
        </div>
        <Seg value={aimMode.value} opts={[["stack", "Stack", "All selected heads aim at the same point"], ["fan", "Fan", "Keep each head's offset while dragging the aim handle"]]} onPick={(v) => (aimMode.value = v)} />
        <button class={`btn pl-btn ${editLayout.value ? "" : "on"}`} title="Lock layout: fixtures and objects can't be dragged" onClick={() => (editLayout.value = !editLayout.value)}>{editLayout.value ? "🔓 Edit layout" : "🔒 Locked"}</button>
        <span class="sp" />
        <select class="input pl-sel" value={labelMode.value} aria-label="Labels" onChange={(e) => (labelMode.value = (e.target as HTMLSelectElement).value as LabelMode)}>
          <option value="id">Labels: ID</option><option value="name">Labels: Name</option><option value="addr">Labels: Address</option><option value="none">Labels: off</option>
        </select>
        <div class="pl-group">
          <button class="btn pl-btn" title="Zoom out (−)" onClick={() => ctl.current?.zoomBy(0.8)}>−</button>
          <button class="btn pl-btn" title="Fit to stage (F)" onClick={() => ctl.current?.fit()}>Fit</button>
          <button class="btn pl-btn" title="Zoom in (+)" onClick={() => ctl.current?.zoomBy(1.25)}>+</button>
        </div>
        <button class={`btn pl-btn ${pop === "layers" ? "sel" : ""}`} onClick={() => setPop(pop === "layers" ? null : "layers")}>▤ Layers</button>
        <button class={`btn pl-btn ${pop === "bg" ? "sel" : ""}`} onClick={() => setPop(pop === "bg" ? null : "bg")}>🗺 Plan image</button>
        <button class="btn pl-btn" title="Export the current view as PNG" onClick={() => ctl.current?.exportPng(show.value?.name ?? "lumos")}>⭳ PNG</button>
        <button class={`btn pl-btn ${legend ? "sel" : ""}`} title="Legend" onClick={() => setLegend(!legend)}>?</button>
        {pop === "layers" && <LayersPanel />}
        {pop === "bg" && ctl.current && <BackgroundPanel ctl={ctl.current} close={() => setPop(null)} />}
      </div>
      {tool.value === "aim" && <div class="pl-banner">AIM — click the stage to point the selected moving heads (Esc to stop)</div>}
      {selObj.value !== null && <div class="pl-banner pl-banner-b">Object selected — drag to move{editLayout.value ? "" : " (layout locked)"}</div>}
      {ctl.current && <Info ctl={ctl.current} />}
      {legend && <Legend />}
      <div class="pl-hud">
        {view.value === "plan" ? `X ${hv.x.toFixed(2)}  Z ${hv.z.toFixed(2)}` : view.value === "front" ? `X ${hv.x.toFixed(2)}  Y ${hv.y.toFixed(2)}` : `Z ${hv.z.toFixed(2)}  Y ${hv.y.toFixed(2)}`} m · {hv.scale.toFixed(0)} px/m · {hv.fps} fps · {nMov} fixtures
      </div>
      {m && (
        <div class="pl-menu" style={{ left: Math.min(m.x, (host.current?.clientWidth ?? 9999) - 240), top: Math.min(m.y, (host.current?.clientHeight ?? 9999) - m.items.length * 36 - 8) }} onPointerDown={(e) => e.stopPropagation()}>
          {m.items.map((it, i) => it.sep ? <div key={i} class="pl-sep" /> : (
            <button key={i} class="pl-item" disabled={it.disabled || !it.run} onClick={() => { menu.value = null; it.run?.(); }}>{it.label}</button>
          ))}
        </div>
      )}
      <DialogHost />
    </div>
  );
}
