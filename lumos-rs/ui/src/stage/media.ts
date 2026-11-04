// Media sources for stage screens → THREE textures.
//
// Bringing NDI / Resolume / OBS / Watchout output in: browsers cannot receive NDI directly, so open the
// NDI Monitor (or the Resolume/OBS output window) and use "Screen capture" — the captured window
// becomes a live texture. MJPEG/HTTP-video URLs and HLS (Safari) also work.
import { t } from "../i18n";
import * as THREE from "three";
import { signal } from "@preact/signals";
import type { MediaSource } from "../types";

export const streamVersion = signal(0);
const streams = new Map<string, MediaStream>();

export function hasStream(kind: "webcam" | "screen") {
  return streams.has(kind);
}

/** Must be called from a user gesture (button click). */
export async function startCapture(kind: "webcam" | "screen"): Promise<void> {
  stopCapture(kind);
  const md = navigator.mediaDevices;
  if (!md) throw new Error(t("Media capture needs a secure context (https or localhost)"));
  const s = kind === "webcam"
    ? await md.getUserMedia({ video: { width: 1280, height: 720 }, audio: false })
    : await md.getDisplayMedia({ video: true, audio: false });
  s.getVideoTracks().forEach((tr) => tr.addEventListener("ended", () => { stopCapture(kind); }));
  streams.set(kind, s);
  streamVersion.value++;
}

export function stopCapture(kind: "webcam" | "screen") {
  streams.get(kind)?.getTracks().forEach((t) => t.stop());
  if (streams.delete(kind)) streamVersion.value++;
}

export async function uploadMedia(file: File): Promise<string> {
  const fd = new FormData();
  fd.append("file", file);
  const r = await fetch("/api/media", { method: "POST", body: fd });
  if (!r.ok) throw new Error(t("Upload failed ({status})", { status: r.status }));
  const j = (await r.json()) as { url: string };
  return j.url;
}

export interface MediaEntry {
  tex: THREE.Texture | null;
  color: THREE.Color;
  /** per-frame update (canvas sources); returns true if the texture changed */
  tick(now: number): void;
  /** average colour (for LED halo), sampled ≤5 Hz */
  avg: THREE.Color;
  dispose(): void;
}

function testPattern(): THREE.CanvasTexture {
  const c = document.createElement("canvas");
  c.width = 512; c.height = 288;
  const g = c.getContext("2d")!;
  const bars = ["#c0c0c0", "#c0c000", "#00c0c0", "#00c000", "#c000c0", "#c00000", "#0000c0"];
  const w = c.width / bars.length;
  bars.forEach((col, i) => { g.fillStyle = col; g.fillRect(i * w, 0, w + 1, c.height * 0.72); });
  g.fillStyle = "#101010"; g.fillRect(0, c.height * 0.72, c.width, c.height * 0.28);
  g.fillStyle = "#e0e0e0"; g.font = "bold 28px sans-serif"; g.textAlign = "center";
  g.fillText(t("NO SIGNAL"), c.width / 2, c.height * 0.9);
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

const VIDEO_RE = /\.(mp4|webm|ogv|mov|m3u8)(\?|#|$)/i;
const sampler = document.createElement("canvas");
sampler.width = sampler.height = 4;
const sctx = sampler.getContext("2d", { willReadFrequently: true })!;

function sampleAvg(src: CanvasImageSource, into: THREE.Color) {
  try {
    sctx.drawImage(src, 0, 0, 4, 4);
    const d = sctx.getImageData(0, 0, 4, 4).data;
    let r = 0, g = 0, b = 0;
    for (let i = 0; i < d.length; i += 4) { r += d[i]; g += d[i + 1]; b += d[i + 2]; }
    const n = d.length / 4 * 255;
    into.setRGB(r / n, g / n, b / n, THREE.SRGBColorSpace);
  } catch { /* tainted */ }
}

function videoEntry(stream: MediaStream | null, url: string, loop: boolean): MediaEntry {
  const v = document.createElement("video");
  v.muted = true; v.playsInline = true; v.loop = loop; v.autoplay = true;
  v.crossOrigin = "anonymous";
  if (stream) v.srcObject = stream; else v.src = url;
  v.play().catch(() => undefined);
  const tex = new THREE.VideoTexture(v);
  tex.colorSpace = THREE.SRGBColorSpace;
  const avg = new THREE.Color(0.5, 0.5, 0.5);
  let last = 0;
  return {
    tex, color: new THREE.Color(1, 1, 1), avg,
    tick(now) { if (now - last > 200 && v.readyState >= 2) { last = now; sampleAvg(v, avg); } },
    dispose() { v.pause(); v.removeAttribute("src"); v.srcObject = null; tex.dispose(); },
  };
}

function mjpegEntry(url: string): MediaEntry {
  const img = new Image();
  img.crossOrigin = "anonymous";
  img.src = url;
  const c = document.createElement("canvas");
  c.width = 640; c.height = 360;
  const g = c.getContext("2d")!;
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  const avg = new THREE.Color(0.5, 0.5, 0.5);
  let last = 0;
  return {
    tex, color: new THREE.Color(1, 1, 1), avg,
    tick(now) {
      if (!img.naturalWidth) return;
      if (c.width !== img.naturalWidth) { c.width = img.naturalWidth; c.height = img.naturalHeight; }
      g.drawImage(img, 0, 0, c.width, c.height);
      tex.needsUpdate = true;
      if (now - last > 200) { last = now; sampleAvg(c, avg); }
    },
    dispose() { img.src = ""; tex.dispose(); },
  };
}

function imageEntry(url: string): MediaEntry {
  const avg = new THREE.Color(0.5, 0.5, 0.5);
  const e: MediaEntry = { tex: null, color: new THREE.Color(1, 1, 1), avg, tick() { /* static */ }, dispose() { e.tex?.dispose(); } };
  new THREE.TextureLoader().load(url, (t) => {
    t.colorSpace = THREE.SRGBColorSpace;
    e.tex = t;
    if (t.image) sampleAvg(t.image as CanvasImageSource, avg);
  }, undefined, () => { e.tex = null; });
  return e;
}

/** Caches one entry per distinct media source. */
export class MediaCache {
  private map = new Map<string, MediaEntry>();
  private fallback = testPattern();
  private ver = -1;

  get(m: MediaSource | null | undefined): MediaEntry {
    if (this.ver !== streamVersion.value) { this.clearStreams(); this.ver = streamVersion.value; }
    const key = m ? `${m.kind}|${m.src}|${m.looped}` : "none";
    let e = this.map.get(key);
    if (!e) { e = this.create(m); this.map.set(key, e); }
    return e;
  }

  private clearStreams() {
    for (const [k, e] of this.map) if (k.startsWith("webcam|") || k.startsWith("screen|")) { e.dispose(); this.map.delete(k); }
  }

  private create(m: MediaSource | null | undefined): MediaEntry {
    const pattern = (): MediaEntry => ({ tex: this.fallback, color: new THREE.Color(1, 1, 1), avg: new THREE.Color(0.35, 0.35, 0.35), tick() { /* */ }, dispose() { /* shared */ } });
    if (!m || m.kind === "none") return pattern();
    switch (m.kind) {
      case "color": {
        const col = new THREE.Color(m.src || "#ffffff");
        return { tex: null, color: col, avg: col.clone(), tick() { /* */ }, dispose() { /* */ } };
      }
      case "image": return m.src ? imageEntry(m.src) : pattern();
      case "video": return m.src ? videoEntry(null, m.src, m.looped) : pattern();
      case "url":
        if (!m.src) return pattern();
        return VIDEO_RE.test(m.src) ? videoEntry(null, m.src, m.looped) : mjpegEntry(m.src);
      case "webcam":
      case "screen": {
        const s = streams.get(m.kind);
        return s ? videoEntry(s, "", false) : pattern();
      }
    }
    return pattern();
  }

  tick(now: number) { for (const e of this.map.values()) e.tick(now); }

  dispose() {
    for (const e of this.map.values()) e.dispose();
    this.map.clear();
    this.fallback.dispose();
  }
}
