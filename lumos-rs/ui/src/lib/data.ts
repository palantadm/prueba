// REST helpers for reports, CSV/MVR exchange (wave 2).
export type ReportKind = "patch" | "channels" | "cues" | "power" | "scenes";

export const absoluteUrl = (path: string) => new URL(path, location.origin).toString();
export const reportPath = (k: ReportKind, lang: string) => `/api/report/${k}.pdf?lang=${lang}`;

export async function saveFromUrl(path: string, filename: string): Promise<void> {
  const r = await fetch(path);
  if (!r.ok) throw new Error(`${path} ${r.status} ${await r.text().catch(() => "")}`);
  const blob = await r.blob();
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 3000);
}

export async function uploadFile<T>(path: string, file: File): Promise<T> {
  const fd = new FormData();
  fd.append("file", file, file.name);
  const r = await fetch(path, { method: "POST", body: fd });
  const txt = await r.text();
  if (!r.ok) throw new Error(txt || `${path} ${r.status}`);
  try { return JSON.parse(txt) as T; } catch { return {} as T; }
}

export type CsvResult = { imported: number; created?: number; updated?: number; removed?: number; errors: { line: number; error: string }[] };
export type MvrResult = { imported?: number; created?: number; updated?: number; unmatched?: unknown[]; skipped?: unknown[]; warnings?: unknown[]; errors?: unknown[]; [k: string]: unknown };

export const fmtBytes = (n: number) => (n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1048576).toFixed(1)} MB`);
export const fmtWhen = (unix: number) => new Date(unix * 1000).toLocaleString();
