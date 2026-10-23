import "../../styles/auto.css";
import { useEffect, useRef, useState } from "preact/hooks";
import { rt, send, show, toast } from "../../store";
import type { Timeline, TlEvent } from "../../types";
import { NumberField, Segmented, Toggle, confirmDialog, promptDialog } from "../../ui";
import { fmtTc, fmtTime, parseTc } from "../../auto/grammar";
import { lsGet, lsSet } from "../../lib/state";
import { useKey } from "../../lib/hooks";
import { tErr } from "../../i18n/errors";
import { t } from "../../i18n";
import { uploadAudio, usePeaks } from "./audio";

const RULER_H = 44;
const LANE_H = 50;
const WAVE_H = 64;
type Snap = "tenth" | "frame" | "off";
const clamp = (v: number, a: number, b: number) => Math.max(a, Math.min(b, v));

function CommitInput(p: { value: string; onCommit: (v: string) => void; mono?: boolean; label: string; placeholder?: string }) {
  const [v, setV] = useState(p.value);
  useEffect(() => setV(p.value), [p.value]);
  return (
    <input class="input" style={{ minHeight: 44, fontFamily: p.mono ? "var(--font-mono)" : undefined }} value={v} aria-label={p.label} placeholder={p.placeholder} spellcheck={false}
      onInput={(e) => setV((e.target as HTMLInputElement).value)}
      onBlur={() => v !== p.value && p.onCommit(v)}
      onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Enter") (e.target as HTMLInputElement).blur(); if (e.key === "Escape") { setV(p.value); (e.target as HTMLInputElement).blur(); } }} />
  );
}

function evWidth(e: TlEvent): number {
  return Math.min(220, Math.max(60, 28 + (e.label || e.text).length * 7.5));
}

export function TimelineEditor({ tl, onRename, onDelete }: { tl: Timeline; onRename: () => void; onDelete: () => void }) {
  const [d, setD] = useState<Timeline>(() => structuredClone(tl));
  const dRef = useRef(d); dRef.current = d;
  const drag = useRef<{ i: number; x0: number; at0: number; moved: boolean } | null>(null);
  const [sel, setSel] = useState<number | null>(null);
  const [pps, setPps] = useState<number>(lsGet("tl.pps", 40));
  const [snap, setSnap] = useState<Snap>(lsGet("tl.snap", "tenth"));
  const [follow, setFollow] = useState<boolean>(lsGet("tl.follow", true));
  const [audioOn, setAudioOn] = useState<boolean>(true);
  const [vw, setVw] = useState(800);
  const [sx, setSx] = useState(0);
  const [probe, setProbe] = useState<{ ok: boolean; msg: string } | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);
  const ruler = useRef<HTMLCanvasElement>(null);
  const wave = useRef<HTMLCanvasElement>(null);
  const ph = useRef<HTMLDivElement>(null);
  const clock = useRef<HTMLDivElement>(null);
  const tcOut = useRef<HTMLSpanElement>(null);
  const audio = useRef<HTMLAudioElement>(null);
  const file = useRef<HTMLInputElement>(null);
  const ppsRef = useRef(pps); ppsRef.current = pps;
  const fps = show.value?.settings.timecode.fps ?? 25;

  // adopt server state when not editing
  const tlJson = JSON.stringify(tl);
  useEffect(() => { if (!drag.current) setD(structuredClone(tl)); }, [tlJson]);
  useEffect(() => { setSel(null); setProbe(null); }, [tl.id]);

  const trt = rt.value?.timelines.find((x) => x.id === tl.id);
  const running = !!trt?.running;
  const base = useRef({ t: 0, at: performance.now(), running: false });
  const lastTrt = useRef<unknown>(null);
  if (trt !== lastTrt.current) { lastTrt.current = trt; base.current = { t: trt?.t ?? base.current.t, at: performance.now(), running }; }
  const live = useRef({ len: d.length, looped: d.looped, follow, tcStart: d.tc_start, fps });
  live.current = { len: d.length, looped: d.looped, follow, tcStart: d.tc_start, fps };

  const snapT = (x: number) => {
    const v = snap === "tenth" ? Math.round(x * 10) / 10 : snap === "frame" ? Math.round(x * fps) / fps : x;
    return clamp(Math.round(v * 1000) / 1000, 0, dRef.current.length);
  };
  const commit = (next: Timeline) => {
    setD(next);
    void send({ type: "timeline_save", timeline: next });
  };
  const sortedWith = (tlx: Timeline, keep: TlEvent | null): [Timeline, number | null] => {
    const events = [...tlx.events].sort((a, b) => a.at - b.at);
    return [{ ...tlx, events }, keep ? events.indexOf(keep) : null];
  };

  // viewport size + scroll position
  useEffect(() => {
    const el = scroller.current!;
    const ro = new ResizeObserver(() => setVw(el.clientWidth));
    ro.observe(el);
    setVw(el.clientWidth);
    let raf = 0;
    const onScroll = () => { cancelAnimationFrame(raf); raf = requestAnimationFrame(() => setSx(el.scrollLeft)); };
    el.addEventListener("scroll", onScroll, { passive: true });
    const onWheel = (e: WheelEvent) => {
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        zoomAt(ppsRef.current * (e.deltaY < 0 ? 1.15 : 1 / 1.15), e.clientX - el.getBoundingClientRect().left);
      } else if (Math.abs(e.deltaY) > Math.abs(e.deltaX) && el.scrollHeight <= el.clientHeight + 2) {
        e.preventDefault();
        el.scrollLeft += e.deltaY;
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => { ro.disconnect(); el.removeEventListener("scroll", onScroll); el.removeEventListener("wheel", onWheel); cancelAnimationFrame(raf); };
  }, []);

  /** Zoom keeping the time under viewport x fixed. */
  const zoomAt = (next: number, vx: number) => {
    const el = scroller.current!;
    const n = clamp(next, 4, 600);
    const tAnchor = (el.scrollLeft + vx) / ppsRef.current;
    ppsRef.current = n;
    setPps(n); lsSet("tl.pps", n);
    requestAnimationFrame(() => { el.scrollLeft = Math.max(0, tAnchor * n - vx); setSx(el.scrollLeft); });
  };
  const zoomBtn = (f: number) => zoomAt(ppsRef.current * f, vw / 2);
  const fit = () => { const n = clamp((vw - 40) / Math.max(1, d.length), 4, 600); setPps(n); lsSet("tl.pps", n); requestAnimationFrame(() => { scroller.current!.scrollLeft = 0; setSx(0); }); };

  // pinch zoom (touch) via two pointers on the scroller
  const pointers = useRef(new Map<number, { x: number; y: number }>());
  const pinch = useRef<{ d0: number; pps0: number; t0: number; mid0: number } | null>(null);
  const onPtrDown = (e: PointerEvent) => {
    if (e.pointerType !== "touch") return;
    pointers.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.current.size === 2) {
      const [a, b] = [...pointers.current.values()];
      const el = scroller.current!, left = el.getBoundingClientRect().left;
      const mid = (a.x + b.x) / 2 - left;
      pinch.current = { d0: Math.hypot(a.x - b.x, a.y - b.y), pps0: ppsRef.current, t0: (el.scrollLeft + mid) / ppsRef.current, mid0: mid };
      drag.current = null;
    }
  };
  const onPtrMove = (e: PointerEvent) => {
    if (e.pointerType !== "touch" || !pointers.current.has(e.pointerId)) return;
    pointers.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    const p = pinch.current;
    if (p && pointers.current.size >= 2) {
      const [a, b] = [...pointers.current.values()];
      const n = clamp(p.pps0 * (Math.hypot(a.x - b.x, a.y - b.y) / Math.max(1, p.d0)), 4, 600);
      ppsRef.current = n; setPps(n);
      const el = scroller.current!;
      el.scrollLeft = Math.max(0, p.t0 * n - p.mid0);
    }
  };
  const onPtrEnd = (e: PointerEvent) => {
    pointers.current.delete(e.pointerId);
    if (pointers.current.size < 2 && pinch.current) { pinch.current = null; lsSet("tl.pps", ppsRef.current); }
  };

  // lanes (greedy packing by pixel overlap)
  const pos = (() => {
    const ends: number[] = [];
    return d.events.map((e) => {
      const left = e.at * pps;
      let lane = ends.findIndex((x) => x + 6 <= left);
      if (lane < 0) lane = ends.length;
      ends[lane] = left + evWidth(e);
      return { left, lane };
    });
  })();
  const lanes = Math.max(1, ...pos.map((p) => p.lane + 1));
  const peaks = usePeaks(d.audio);
  const waveH = d.audio ? WAVE_H : 0;
  const evTop = (lane: number) => RULER_H + waveH + 8 + lane * LANE_H;
  const innerH = RULER_H + waveH + 16 + lanes * LANE_H + 40;
  const innerW = Math.max(vw, d.length * pps + 160);

  // ruler canvas
  useEffect(() => {
    const c = ruler.current; if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.max(1, Math.floor(vw * dpr)); c.height = RULER_H * dpr;
    c.style.width = `${vw}px`; c.style.height = `${RULER_H}px`; c.style.transform = `translateX(${sx}px)`;
    const g = c.getContext("2d")!; g.scale(dpr, dpr);
    const cs = getComputedStyle(document.documentElement);
    const dim = cs.getPropertyValue("--text-dim").trim() || "#888", line = cs.getPropertyValue("--line-strong").trim() || "#444";
    const stop = cs.getPropertyValue("--stop").trim() || "#e55";
    g.clearRect(0, 0, vw, RULER_H);
    const steps = [0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600];
    const step = steps.find((s) => s * pps >= 70) ?? 600;
    const minor = step / (step >= 1 ? (step % 5 === 0 ? 5 : 4) : 5);
    g.font = "11px ui-monospace, monospace"; g.textBaseline = "top";
    const t0 = Math.floor(sx / pps / minor) * minor, t1 = (sx + vw) / pps;
    for (let tt = t0; tt <= t1 + minor; tt += minor) {
      const x = tt * pps - sx;
      const major = Math.abs(tt / step - Math.round(tt / step)) < 1e-6;
      g.strokeStyle = major ? dim : line; g.lineWidth = 1;
      g.beginPath(); g.moveTo(Math.round(x) + 0.5, major ? 22 : 32); g.lineTo(Math.round(x) + 0.5, RULER_H); g.stroke();
      if (major) { g.fillStyle = dim; g.fillText(fmtTime(tt).replace(/^00:/, ""), x + 4, 6); }
    }
    const xe = d.length * pps - sx;
    if (xe > -2 && xe < vw + 2) { g.strokeStyle = stop; g.lineWidth = 2; g.beginPath(); g.moveTo(xe, 0); g.lineTo(xe, RULER_H); g.stroke(); }
  }, [pps, sx, vw, d.length]);

  // waveform canvas
  useEffect(() => {
    const c = wave.current; if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.max(1, Math.floor(vw * dpr)); c.height = WAVE_H * dpr;
    c.style.width = `${vw}px`; c.style.height = `${WAVE_H}px`; c.style.transform = `translateX(${sx}px)`;
    const g = c.getContext("2d")!; g.scale(dpr, dpr); g.clearRect(0, 0, vw, WAVE_H);
    const pk = peaks.data; if (!pk) return;
    g.fillStyle = getComputedStyle(document.documentElement).getPropertyValue("--attr-effect").trim() || "#b38cff";
    for (let x = 0; x < vw; x++) {
      const a = Math.floor(((sx + x) / pps) * pk.perSec), b = Math.max(a + 1, Math.floor(((sx + x + 1) / pps) * pk.perSec));
      if (a >= pk.peaks.length) break;
      let m = 0;
      for (let k = a; k < Math.min(b, pk.peaks.length); k++) if (pk.peaks[k] > m) m = pk.peaks[k];
      const h = Math.max(1, m * (WAVE_H - 6));
      g.fillRect(x, (WAVE_H - h) / 2, 1, h);
    }
  }, [pps, sx, vw, peaks.data]);

  // playhead / clock loop (interpolates between 20 Hz runtime updates)
  useEffect(() => {
    let id = 0;
    const loop = () => {
      const b = base.current, L = live.current;
      let tt = b.t;
      if (b.running) { tt = b.t + (performance.now() - b.at) / 1000; if (tt > L.len) tt = L.looped ? tt % Math.max(0.001, L.len) : L.len; }
      if (ph.current) ph.current.style.transform = `translateX(${tt * ppsRef.current}px)`;
      if (clock.current) clock.current.textContent = fmtTime(tt);
      if (tcOut.current) tcOut.current.textContent = fmtTc(tt, L.fps);
      const el = scroller.current;
      if (b.running && L.follow && el) {
        const x = tt * ppsRef.current;
        if (x > el.scrollLeft + el.clientWidth - 80 || x < el.scrollLeft) el.scrollLeft = Math.max(0, x - 80);
      }
      id = requestAnimationFrame(loop);
    };
    loop();
    return () => cancelAnimationFrame(id);
  }, []);

  // audio sync
  useEffect(() => {
    const a = audio.current; if (!a || !d.audio) return;
    a.muted = !audioOn;
    const b = base.current;
    if (running) {
      const want = b.t + (performance.now() - b.at) / 1000;
      if (Math.abs(a.currentTime - want) > 0.35) { try { a.currentTime = want; } catch { /* not ready */ } }
      if (a.paused) void a.play().catch(() => {});
    } else {
      a.pause();
      try { a.currentTime = b.t; } catch { /* not ready */ }
    }
  }, [trt, running, audioOn, d.audio]);

  const playheadNow = () => {
    const b = base.current;
    return b.running ? b.t + (performance.now() - b.at) / 1000 : b.t;
  };

  // ---- editing
  const addAt = (at: number) => {
    const ev: TlEvent = { at: snapT(at), text: "go", label: "" };
    const [next, idx] = sortedWith({ ...dRef.current, events: [...dRef.current.events, ev] }, ev);
    commit(next); setSel(idx);
  };
  const update = (i: number, p: Partial<TlEvent>) => {
    const cur = dRef.current;
    const ev = { ...cur.events[i], ...p };
    const events = cur.events.map((x, k) => (k === i ? ev : x));
    const [next, idx] = sortedWith({ ...cur, events }, ev);
    commit(next); setSel(idx);
  };
  const remove = (i: number) => { const cur = dRef.current; commit({ ...cur, events: cur.events.filter((_, k) => k !== i) }); setSel(null); };
  const dupe = (i: number) => { const cur = dRef.current; const e = { ...cur.events[i], at: snapT(cur.events[i].at + 1) }; const [n, idx] = sortedWith({ ...cur, events: [...cur.events, e] }, e); commit(n); setSel(idx); };

  const evDown = (i: number, e: PointerEvent) => {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag.current = { i, x0: e.clientX, at0: dRef.current.events[i].at, moved: false };
    setSel(i);
  };
  const evMove = (e: PointerEvent) => {
    const g = drag.current; if (!g) return;
    const dx = e.clientX - g.x0;
    if (Math.abs(dx) > 4) g.moved = true;
    if (!g.moved) return;
    const at = snapT(g.at0 + dx / ppsRef.current);
    setD((prev) => ({ ...prev, events: prev.events.map((x, k) => (k === g.i ? { ...x, at } : x)) }));
  };
  const evUp = () => {
    const g = drag.current; drag.current = null;
    if (g?.moved) { const cur = dRef.current; const [n, idx] = sortedWith(cur, cur.events[g.i]); commit(n); setSel(idx); }
  };

  // taps on the ruler add an event, taps on the lane background seek; mouse drags pan
  const tap = useRef<{ x: number; t: number; sl: number; kind: "ruler" | "bg"; mouse: boolean } | null>(null);
  const bgDown = (kind: "ruler" | "bg") => (e: PointerEvent) => {
    if (pointers.current.size > 1) return;
    tap.current = { x: e.clientX, t: performance.now(), sl: scroller.current!.scrollLeft, kind, mouse: e.pointerType === "mouse" };
    if (e.pointerType === "mouse") (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const bgMove = (e: PointerEvent) => {
    const g = tap.current;
    if (g?.mouse && Math.abs(e.clientX - g.x) > 6) scroller.current!.scrollLeft = g.sl - (e.clientX - g.x);
  };
  const bgUp = (e: PointerEvent) => {
    const g = tap.current; tap.current = null;
    if (!g || pinch.current) return;
    if (Math.abs(e.clientX - g.x) < 6 && performance.now() - g.t < 600) {
      const x = e.clientX - inner.current!.getBoundingClientRect().left;
      const at = clamp(x / ppsRef.current, 0, dRef.current.length);
      if (g.kind === "ruler") addAt(at);
      else { setSel(null); base.current = { t: snapT(at), at: performance.now(), running: base.current.running }; void send({ type: "timeline_seek", id: dRef.current.id, t: snapT(at) }, { quiet: true }); }
    }
  };

  const run = (from: number | null) => {
    void send({ type: "timeline_run", id: d.id, from }, { quiet: false });
    audio.current?.play().catch(() => {});
  };
  useKey((e) => {
    if (e.key === " ") { e.preventDefault(); if (running) void send({ type: "timeline_stop", id: d.id }); else run(null); }
    else if ((e.key === "Delete" || e.key === "Backspace") && sel !== null) { e.preventDefault(); remove(sel); }
    else if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && sel !== null) {
      e.preventDefault();
      const step = (e.shiftKey ? 1 : snap === "frame" ? 1 / fps : 0.1) * (e.key === "ArrowLeft" ? -1 : 1);
      update(sel, { at: snapT(dRef.current.events[sel].at + step) });
    }
  }, [running, sel, d.id, snap, fps]);

  const probeCmd = async (text: string) => {
    if (!text.trim()) return;
    try { await send({ type: "command_line", text }, { quiet: true }); setProbe({ ok: true, msg: t("OK") }); }
    catch (e) { setProbe({ ok: false, msg: tErr((e as Error).message) }); }
  };

  const onAudio = async (f: File | undefined) => {
    if (!f) return;
    try { const url = await uploadAudio(f); commit({ ...dRef.current, audio: url }); toast(t("Audio attached"), "ok", 1500); }
    catch (e) { toast(tErr((e as Error).message), "err"); }
  };

  const ev = sel !== null ? d.events[sel] : undefined;
  const tcStartTxt = d.tc_start ?? "";
  const tcOk = !d.tc_start || parseTc(d.tc_start, fps) !== null;
  const ph0 = playheadNow();

  return (
    <div class="tl-main">
      <div class="panel tl-transport">
        <button class={`btn au-btn ${running ? "danger" : "go"}`} style={{ minWidth: 96 }} onClick={() => (running ? send({ type: "timeline_stop", id: d.id }) : run(null))} aria-label={running ? t("Stop") : t("Run")}>
          {running ? "■ " + t("Stop") : "▶ " + t("Run")}
        </button>
        <button class="btn au-btn" onClick={() => run(0)} title={t("Run from the start")} aria-label={t("Run from the start")}>⏮▶</button>
        <button class="btn au-btn" onClick={() => { base.current = { t: 0, at: performance.now(), running: base.current.running }; void send({ type: "timeline_seek", id: d.id, t: 0 }, { quiet: true }); scroller.current!.scrollLeft = 0; }} title={t("Go to start")} aria-label={t("Go to start")}>⏮</button>
        <div>
          <div class={`tl-clock ${running ? "" : "idle"}`} ref={clock} aria-live="off">00:00.0</div>
          <div class="faint mono" style={{ fontSize: "var(--fs-xs)" }}>TC <span ref={tcOut}>00:00:00:00</span></div>
        </div>
        <span class="sp" />
        <Toggle checked={d.looped} label={t("Loop")} onChange={(v) => commit({ ...d, looped: v })} />
        <Toggle checked={follow} label={t("Follow")} onChange={(v) => { setFollow(v); lsSet("tl.follow", v); }} />
        <Segmented<Snap> label={t("Snap")} value={snap} onChange={(v) => { setSnap(v); lsSet("tl.snap", v); }}
          options={[{ value: "tenth", label: "0.1 s" }, { value: "frame", label: t("Frame") }, { value: "off", label: t("Off") }]} />
        <button class="btn au-btn" onClick={() => zoomBtn(1 / 1.5)} aria-label={t("Zoom out")}>−</button>
        <button class="btn au-btn" onClick={() => zoomBtn(1.5)} aria-label={t("Zoom in")}>+</button>
        <button class="btn au-btn" onClick={fit}>{t("Fit")}</button>
      </div>

      <div class="panel tl-transport" style={{ paddingTop: 0 }}>
        <b style={{ fontSize: "var(--fs-lg)" }}>{d.name}</b>
        <button class="btn au-btn" onClick={onRename} aria-label={t("Rename")} title={t("Rename")}>✎</button>
        <NumberField float min={1} max={86400} step={1} width={90} value={d.length} label={t("Length (s)")} class="bare" onChange={(v) => commit({ ...d, length: v })} /><span class="faint">s</span>
        <label class="check" style={{ minHeight: 44 }}>
          <input type="checkbox" checked={!!d.tc_start} onChange={(e) => commit({ ...d, tc_start: (e.target as HTMLInputElement).checked ? "00:00:00:00" : null })} />
          <span>{t("Follow show timecode from")}</span>
        </label>
        <input class="input mono" style={{ width: 130, minHeight: 44, borderColor: tcOk ? undefined : "var(--stop)" }} disabled={!d.tc_start} value={tcStartTxt} aria-label={t("Timecode start")} placeholder="HH:MM:SS:FF"
          onInput={(e) => setD({ ...d, tc_start: (e.target as HTMLInputElement).value })}
          onBlur={() => tcOk && d.tc_start !== tl.tc_start && commit(d)} onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} />
        <span class="sp" />
        <input ref={file} type="file" accept="audio/*" hidden onChange={(e) => { const f = (e.target as HTMLInputElement).files?.[0]; (e.target as HTMLInputElement).value = ""; void onAudio(f); }} />
        <button class="btn au-btn" onClick={() => file.current?.click()}>{d.audio ? t("Replace audio…") : t("Attach audio…")}</button>
        {d.audio && <>
          <button class={`btn au-btn ${audioOn ? "sel" : ""}`} aria-pressed={audioOn} onClick={() => setAudioOn(!audioOn)} aria-label={t("Play audio with the timeline")} title={t("Play audio with the timeline")}>{audioOn ? "🔊" : "🔇"}</button>
          {peaks.data && Math.abs(peaks.data.duration - d.length) > 0.5 && <button class="btn au-btn" onClick={() => commit({ ...d, length: Math.ceil(peaks.data!.duration * 10) / 10 })}>{t("Length = audio ({s} s)", { s: peaks.data.duration.toFixed(1) })}</button>}
          <button class="btn au-btn ghost" onClick={() => commit({ ...d, audio: null })} aria-label={t("Remove audio")}>✕</button>
        </>}
        <button class="btn au-btn danger" onClick={onDelete} aria-label={t("Delete timeline")}>🗑</button>
      </div>

      <div class="tl-scroll" ref={scroller} onPointerDown={onPtrDown} onPointerMove={onPtrMove} onPointerUp={onPtrEnd} onPointerCancel={onPtrEnd} data-testid="tl-scroll">
        <div class="tl-inner" ref={inner} style={{ width: innerW, height: Math.max(innerH, 0) }}>
          <div class="tl-ruler" style={{ width: innerW }} onPointerDown={bgDown("ruler")} onPointerMove={bgMove} onPointerUp={bgUp} title={t("Tap to add an event here")}>
            <canvas ref={ruler} />
          </div>
          {d.audio && <div style={{ position: "absolute", top: RULER_H, left: 0, width: innerW, height: WAVE_H, pointerEvents: "none" }}><canvas ref={wave} style={{ display: "block", opacity: 0.55 }} /></div>}
          <div style={{ position: "absolute", left: 0, top: RULER_H + waveH, width: innerW, bottom: 0 }} onPointerDown={bgDown("bg")} onPointerMove={bgMove} onPointerUp={bgUp} />
          {d.events.map((e, i) => (
            <div key={`${i}`} class={`tl-ev ${sel === i ? "sel" : ""} ${running && e.at <= ph0 ? "done" : ""}`} role="button" tabIndex={0} data-testid="tl-event"
              style={{ left: pos[i].left, top: evTop(pos[i].lane) }} title={e.text}
              aria-label={t("Event at {s}: {text}", { s: fmtTime(e.at), text: e.label || e.text })}
              onPointerDown={(ev2) => evDown(i, ev2 as PointerEvent)} onPointerMove={(ev2) => evMove(ev2 as PointerEvent)} onPointerUp={evUp} onPointerCancel={evUp}
              onKeyDown={(k) => { if (k.key === "Enter") setSel(i); }}>
              <span class="when">{fmtTime(e.at)}</span><span>{e.label || e.text}</span>
            </div>
          ))}
          <div class="tl-ph" ref={ph} style={{ left: 0 }} />
        </div>
      </div>

      <div class="panel">
        {!ev || sel === null ? (
          <div class="dim" style={{ padding: 12 }}>
            {t("Tap the ruler to add an event, or")} <button class="btn au-btn" onClick={() => addAt(ph0)}>{t("+ Event at playhead")}</button>
            <span class="faint"> {t("· {n} events · Space = run/stop · ←/→ nudge · Del = delete", { n: d.events.length })}</span>
          </div>
        ) : (
          <div class="tl-insp">
            <div class="field"><label>{t("Time (s)")}</label>
              <NumberField float class="bare" min={0} max={d.length} step={0.1} value={ev.at} label={t("Event time (s)")} onChange={(v) => update(sel, { at: snapT(v) })} />
              <span class="faint mono" style={{ fontSize: "var(--fs-xs)" }}>TC {fmtTc(ev.at, fps)}</span></div>
            <div class="field"><label>{t("Label")}</label><CommitInput value={ev.label} label={t("Event label")} onCommit={(v) => update(sel, { label: v })} /></div>
            <div class="field"><label>{t("Command")}</label><CommitInput mono value={ev.text} label={t("Event command")} placeholder="go cuelist 1" onCommit={(v) => update(sel, { text: v })} />
              {probe && <span class={`au-res ${probe.ok ? "ok" : "err"}`}>{probe.ok ? "✓ " : "✕ "}{probe.msg}</span>}</div>
            <div class="row" style={{ gap: 6 }}>
              <button class="btn au-btn" onClick={() => void probeCmd(ev.text)} title={t("Run this command now")}>{t("Test")}</button>
              <button class="btn au-btn" onClick={() => dupe(sel)} aria-label={t("Duplicate event")}>⧉</button>
              <button class="btn au-btn danger" onClick={() => remove(sel)} aria-label={t("Delete event")}>✕</button>
            </div>
          </div>
        )}
      </div>
      {d.audio && <audio ref={audio} src={d.audio} preload="auto" />}
    </div>
  );
}
