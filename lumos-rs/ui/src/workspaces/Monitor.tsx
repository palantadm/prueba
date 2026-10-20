import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { dmx, selection, show } from "../store";
import { EmptyState, PageHead, Segmented, Toggle } from "../ui";
import { footprint, modeOf, profileById } from "../lib/fixtures";
import { attrLabel } from "../lib/shell-attrs";
import { releaseDmx, requestDmx } from "../lib/dmxsub";
import { t } from "../i18n";

type Fmt = "dec" | "hex" | "pct";
type Info = { fixture: number; name: string; attr: string } | null;
const CELL_H = 38;

function css(name: string) { return getComputedStyle(document.documentElement).getPropertyValue(name).trim(); }
const fmtVal = (v: number, f: Fmt) => (f === "hex" ? v.toString(16).toUpperCase().padStart(2, "0") : f === "pct" ? `${Math.round((v / 255) * 100)}` : String(v));

export function Monitor() {
  const unis = useMemo(() => [...(show.value?.universes ?? [])].map((u) => u.id).sort((a, b) => a - b), [show.value?.universes]);
  const [u, setU] = useState<number>(Number(localStorage.getItem("mon.u")) || unis[0] || 1);
  const [fmt, setFmt] = useState<Fmt>((localStorage.getItem("mon.fmt") as Fmt) || "dec");
  const [nz, setNz] = useState(false);
  const [hover, setHover] = useState<number | null>(null);
  const [pin, setPin] = useState<number | null>(null);
  const [active, setActive] = useState<Set<number>>(new Set());
  const wrap = useRef<HTMLDivElement>(null);
  const cv = useRef<HTMLCanvasElement>(null);
  const st = useRef({ u, fmt, nz, hover: null as number | null, pin: null as number | null, dirty: true, cols: 32, prev: new Uint8Array(512), flash: new Float32Array(512) });
  Object.assign(st.current, { u, fmt, nz, hover, pin, dirty: true });

  useEffect(() => { if (unis.length && !unis.includes(u)) setU(unis[0]); }, [unis.join(",")]);
  useEffect(() => { requestDmx("monitor", unis.length > 24 ? [u] : unis); return () => releaseDmx("monitor"); }, [u, unis.length > 24, unis.length]);
  useEffect(() => { localStorage.setItem("mon.u", String(u)); localStorage.setItem("mon.fmt", fmt); }, [u, fmt]);
  useEffect(() => {
    const t = setInterval(() => {
      const s = new Set<number>();
      for (const [k, d] of dmx) { for (let i = 0; i < 512; i++) if (d[i]) { s.add(k); break; } }
      setActive((p) => (p.size === s.size && [...s].every((x) => p.has(x)) ? p : s));
    }, 400);
    return () => clearInterval(t);
  }, []);

  const chInfo = useMemo(() => {
    const m = new Map<number, Info>();
    for (const f of show.value?.fixtures ?? []) {
      if (f.universe !== u) continue;
      const mode = modeOf(f);
      if (!mode) continue;
      for (const c of mode.channels) {
        m.set(f.address + c.offset, { fixture: f.id, name: f.name, attr: c.attr });
        if (c.fine !== null) m.set(f.address + c.fine, { fixture: f.id, name: f.name, attr: `${c.attr} fine` });
      }
      void footprint; void profileById;
    }
    return m;
  }, [u, show.value?.fixtures, show.value?.profiles, show.value?.universes]);
  const chInfoRef = useRef(chInfo);
  chInfoRef.current = chInfo;

  useEffect(() => {
    const canvas = cv.current!, box = wrap.current!;
    const ctx = canvas.getContext("2d")!;
    const col = { bg: css("--bg-1"), bg2: css("--bg-2"), line: css("--line"), text: css("--text"), faint: css("--text-faint"), accent: css("--accent"), live: css("--live"), dim: css("--text-dim") };
    let w = 0, raf = 0, dpr = 1;
    const layout = () => {
      w = box.clientWidth;
      const cols = Math.max(8, Math.min(32, 2 ** Math.floor(Math.log2(Math.max(8, w / 46)))));
      st.current.cols = cols;
      const h = (512 / cols) * CELL_H;
      dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr);
      canvas.style.width = `${w}px`; canvas.style.height = `${h}px`;
      st.current.dirty = true;
    };
    const ro = new ResizeObserver(layout);
    ro.observe(box);
    layout();
    const draw = (now: number) => {
      raf = requestAnimationFrame(draw);
      const s = st.current;
      const data = dmx.get(s.u);
      let flashing = false;
      for (let i = 0; i < 512; i++) {
        const v = data ? data[i] : 0;
        if (v !== s.prev[i]) { s.prev[i] = v; s.flash[i] = now; s.dirty = true; }
        if (now - s.flash[i] < 400) flashing = true;
      }
      if (!s.dirty && !flashing) return;
      s.dirty = false;
      const cols = s.cols, cw = w / cols;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.fillStyle = col.bg; ctx.fillRect(0, 0, w, (512 / cols) * CELL_H);
      ctx.textBaseline = "alphabetic";
      for (let i = 0; i < 512; i++) {
        const x = (i % cols) * cw, y = Math.floor(i / cols) * CELL_H, v = s.prev[i];
        if (s.nz && v === 0) { ctx.fillStyle = col.line; ctx.fillRect(x + 1, y + CELL_H - 2, cw - 2, 1); continue; }
        ctx.fillStyle = col.bg2; ctx.fillRect(x + 1, y + 1, cw - 2, CELL_H - 2);
        if (v) { ctx.globalAlpha = 0.55; ctx.fillStyle = col.accent; const bh = ((CELL_H - 2) * v) / 255; ctx.fillRect(x + 1, y + CELL_H - 1 - bh, cw - 2, bh); ctx.globalAlpha = 1; }
        const age = now - s.flash[i];
        if (age < 400) { ctx.globalAlpha = 1 - age / 400; ctx.strokeStyle = col.live; ctx.lineWidth = 2; ctx.strokeRect(x + 1.5, y + 1.5, cw - 3, CELL_H - 3); ctx.globalAlpha = 1; }
        if (s.hover === i + 1 || s.pin === i + 1) { ctx.strokeStyle = col.text; ctx.lineWidth = 1; ctx.strokeRect(x + 0.5, y + 0.5, cw - 1, CELL_H - 1); }
        ctx.fillStyle = col.faint; ctx.font = "9px ui-monospace, monospace"; ctx.textAlign = "left"; ctx.fillText(String(i + 1), x + 4, y + 11);
        ctx.fillStyle = v ? col.text : col.faint; ctx.font = "600 13px ui-monospace, monospace"; ctx.textAlign = "center";
        ctx.fillText(fmtVal(v, s.fmt), x + cw / 2, y + CELL_H - 9);
      }
    };
    raf = requestAnimationFrame(draw);
    return () => { cancelAnimationFrame(raf); ro.disconnect(); };
  }, []);

  const chAt = (e: MouseEvent) => {
    const r = cv.current!.getBoundingClientRect();
    const cols = st.current.cols, cw = r.width / cols;
    const c = Math.floor((e.clientX - r.left) / cw), row = Math.floor((e.clientY - r.top) / CELL_H);
    const ch = row * cols + c + 1;
    return c >= 0 && c < cols && ch >= 1 && ch <= 512 ? ch : null;
  };
  const shown = pin ?? hover;
  const data = dmx.get(u);
  const v = shown && data ? data[shown - 1] : 0;
  const info = shown ? chInfo.get(shown) ?? null : null;
  return (
    <div class="page">
      <PageHead title={t("DMX monitor")} sub={t("Final merged output, as sent to the outputs.")}>
        <Segmented<Fmt> value={fmt} onChange={setFmt} label={t("Number format")} options={[{ value: "dec", label: "DEC" }, { value: "hex", label: "HEX" }, { value: "pct", label: "%" }]} />
        <Toggle checked={nz} onChange={setNz} label={t("Only non-zero")} />
      </PageHead>
      {unis.length === 0 ? <EmptyState title={t("No universes")} hint={t("Add universes in Settings.")} /> : <>
        <div class="row scroll" role="tablist" aria-label={t("Universes")} style={{ padding: "6px 12px", gap: 4, flex: "none", borderBottom: "1px solid var(--line)" }}>
          {unis.map((id) => (
            <button key={id} role="tab" aria-selected={id === u} class={`btn sm ${id === u ? "sel" : ""}`} style={{ minWidth: 44 }} onClick={() => { setU(id); setPin(null); }} title={show.value?.universes.find((x) => x.id === id)?.name}>
              <span class={`dot ${active.has(id) ? "ok" : ""}`} style={{ width: 6, height: 6 }} />U{id}
            </button>
          ))}
        </div>
        <div class="grow scroll" ref={wrap} style={{ padding: 0 }}>
          <canvas ref={cv} role="img" aria-label={t("DMX values of universe {u}", { u })} style={{ display: "block", cursor: "crosshair" }}
            onMouseMove={(e) => setHover(chAt(e as MouseEvent))} onMouseLeave={() => setHover(null)}
            onClick={(e) => { const c = chAt(e as MouseEvent); setPin(c === pin ? null : c); const f = c ? chInfo.get(c) : null; if (f) selection.value = [f.fixture]; }} />
        </div>
        <div class="statusbar" style={{ height: 34, fontSize: "var(--fs-sm)" }} aria-live="polite">
          {shown ? <>
            <b class="mono" style={{ color: "var(--text)" }}>U{u}.{String(shown).padStart(3, "0")}</b>
            <span class="mono" style={{ color: "var(--text)" }}>{v} · 0x{v.toString(16).toUpperCase().padStart(2, "0")} · {Math.round((v / 255) * 100)}%</span>
            {info ? <span style={{ color: "var(--text)" }}>#{info.fixture} {info.name} — <span style={{ color: "var(--live)" }}>{attrLabel(info.attr.replace(" fine", ""))}{info.attr.endsWith("fine") ? ` ${t("(fine)")}` : ""}</span></span> : <span class="faint">{t("unpatched")}</span>}
            {pin && <span class="faint">{t("(pinned — click again to release)")}</span>}
          </> : <span class="faint">{t("Hover a channel for details, click to pin and select its fixture.")}</span>}
        </div>
      </>}
    </div>
  );
}
