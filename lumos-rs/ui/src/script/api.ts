import type { CueMark, ScriptBlock, ScriptImport } from "../types";

export class ApiError extends Error {
  constructor(public status: number, msg: string) { super(msg); }
}

async function errText(r: Response): Promise<string> {
  const raw = await r.text().catch(() => "");
  try { const j = JSON.parse(raw); return String(j.error ?? j.message ?? raw); } catch { return raw || `HTTP ${r.status}`; }
}

/** Parse a script draft from a file (.pdf .txt .md .fountain) or pasted text. */
export async function importDraft(src: { file?: File; text?: string }): Promise<ScriptImport> {
  const fd = new FormData();
  if (src.file) fd.append("file", src.file, src.file.name);
  else fd.append("text", src.text ?? "");
  const r = await fetch("/api/script/import", { method: "POST", body: fd });
  if (!r.ok) throw new ApiError(r.status, await errText(r));
  return (await r.json()) as ScriptImport;
}

export async function uploadMedia(file: File): Promise<string> {
  const fd = new FormData();
  fd.append("file", file, file.name);
  const r = await fetch("/api/media", { method: "POST", body: fd });
  if (!r.ok) throw new ApiError(r.status, await errText(r));
  const j = await r.json();
  return String(j.url ?? j.path ?? "");
}

export type Suggestion = { block: number; mark: CueMark };

/** Tolerant parser: accepts `[{block, mark}]`, `{marks:[…]}`, `{suggestions:[…]}`, or blocks carrying marks. */
export function parseSuggestions(j: unknown, existing: ScriptBlock[]): Suggestion[] {
  const out: Suggestion[] = [];
  const arr: any[] = Array.isArray(j) ? j : (j as any)?.marks ?? (j as any)?.suggestions ?? (j as any)?.blocks ?? [];
  for (const x of arr) {
    if (x && x.mark && (x.block != null || x.block_id != null)) out.push({ block: Number(x.block ?? x.block_id), mark: x.mark as CueMark });
    else if (x && Array.isArray(x.marks) && x.id != null) {
      const have = existing.find((b) => b.id === x.id);
      for (const m of x.marks as CueMark[]) if (!have || !have.marks.some((h) => h.id === m.id && h.label === m.label)) out.push({ block: x.id, mark: m });
    } else if (x && x.block_id != null && x.kind) out.push({ block: Number(x.block_id), mark: x as CueMark });
  }
  return out;
}

export async function suggestMarks(blocks: ScriptBlock[]): Promise<Suggestion[]> {
  const r = await fetch("/api/script/suggest", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ blocks }) });
  if (!r.ok) throw new ApiError(r.status, await errText(r));
  return parseSuggestions(await r.json(), blocks);
}
