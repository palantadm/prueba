// Audio helpers for the timeline: upload, decode and downsample to peaks.
import { useEffect, useState } from "preact/hooks";
import { t } from "../../i18n";

export async function uploadAudio(file: File): Promise<string> {
  const fd = new FormData();
  fd.append("file", file);
  const r = await fetch("/api/media", { method: "POST", body: fd });
  if (!r.ok) throw new Error(t("Upload failed ({status})", { status: r.status }));
  return ((await r.json()) as { url: string }).url;
}

export type Peaks = { peaks: Float32Array; duration: number; perSec: number };
const cache = new Map<string, Promise<Peaks>>();

/** Decode with WebAudio, then keep the max |sample| per 10 ms bucket. */
export function loadPeaks(url: string): Promise<Peaks> {
  let p = cache.get(url);
  if (!p) {
    p = (async () => {
      const buf = await (await fetch(url)).arrayBuffer();
      const Ctx = (window.AudioContext ?? (window as any).webkitAudioContext) as typeof AudioContext;
      const ctx = new Ctx();
      try {
        const a = await ctx.decodeAudioData(buf);
        const perSec = 100;
        const n = Math.max(1, Math.ceil(a.duration * perSec));
        const peaks = new Float32Array(n);
        const step = a.sampleRate / perSec;
        for (let c = 0; c < a.numberOfChannels; c++) {
          const d = a.getChannelData(c);
          for (let i = 0; i < n; i++) {
            let m = peaks[i];
            const s = Math.floor(i * step), e = Math.min(d.length, Math.floor((i + 1) * step));
            for (let k = s; k < e; k += 4) { const v = Math.abs(d[k]); if (v > m) m = v; }
            peaks[i] = m;
          }
        }
        return { peaks, duration: a.duration, perSec };
      } finally {
        void ctx.close();
      }
    })();
    p.catch(() => cache.delete(url));
    cache.set(url, p);
  }
  return p;
}

export function usePeaks(url: string | null): { data: Peaks | null; error: string } {
  const [data, setData] = useState<Peaks | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    setData(null); setError("");
    if (!url) return;
    let alive = true;
    loadPeaks(url).then((d) => alive && setData(d)).catch((e) => alive && setError(String(e?.message ?? e)));
    return () => { alive = false; };
  }, [url]);
  return { data, error };
}
