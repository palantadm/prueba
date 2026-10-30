import { useEffect, useRef, useState } from "preact/hooks";
import { t } from "../i18n";
import { setPdfOpen } from "./store";

/** Original PDF next to the reader; pdfjs-dist is imported lazily (only when opened). */
export function PdfPanel(props: { url: string; page: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const doc = useRef<any>(null);
  const [pages, setPages] = useState(0);
  const [manual, setManual] = useState<number | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const page = Math.max(1, Math.min(pages || 1, manual ?? (props.page || 1)));

  useEffect(() => { setManual(null); }, [props.page]);
  useEffect(() => {
    let dead = false;
    setErr(null); doc.current = null; setPages(0);
    (async () => {
      try {
        const pdfjs: any = await import("pdfjs-dist");
        const worker: any = await import("pdfjs-dist/build/pdf.worker.min.mjs?url");
        pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
        const d = await pdfjs.getDocument(props.url).promise;
        if (dead) return;
        doc.current = d; setPages(d.numPages);
      } catch (e) { if (!dead) setErr((e as Error).message || t("Could not open the PDF.")); }
    })();
    return () => { dead = true; };
  }, [props.url]);
  useEffect(() => {
    const d = doc.current, c = canvas.current;
    if (!d || !c || !pages) return;
    let dead = false;
    (async () => {
      const p = await d.getPage(page);
      const base = p.getViewport({ scale: 1 });
      const w = c.parentElement?.clientWidth ? c.parentElement.clientWidth - 24 : 520;
      const vp = p.getViewport({ scale: Math.min(2, Math.max(0.4, w / base.width)) * (window.devicePixelRatio || 1) });
      c.width = vp.width; c.height = vp.height;
      c.style.width = `${vp.width / (window.devicePixelRatio || 1)}px`;
      if (!dead) await p.render({ canvasContext: c.getContext("2d")!, viewport: vp }).promise;
    })().catch(() => { /* superseded render */ });
    return () => { dead = true; };
  }, [pages, page]);

  return (
    <aside class="sc-pdf" aria-label={t("Original PDF")}>
      <div class="sc-pdf-bar">
        <button class="btn" aria-label={t("Previous page")} disabled={page <= 1} onClick={() => setManual(page - 1)}>◀</button>
        <span class="grow" style={{ textAlign: "center" }}>{pages ? t("Page {n} of {m}", { n: page, m: pages }) : "…"}</span>
        <button class="btn" aria-label={t("Next page")} disabled={page >= pages} onClick={() => setManual(page + 1)}>▶</button>
        {manual != null && <button class="btn cyan" onClick={() => setManual(null)}>{t("Follow")}</button>}
        <button class="btn ghost icon" aria-label={t("Close")} onClick={() => setPdfOpen(false)}>✕</button>
      </div>
      <div class="sc-pdf-body">{err ? <div class="sc-err">{err}</div> : <canvas ref={canvas} />}</div>
    </aside>
  );
}
