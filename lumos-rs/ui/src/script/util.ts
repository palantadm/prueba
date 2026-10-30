import type { CueMark, MarkKind, Script, ScriptAction, ScriptBlock, ScriptKind, ScriptRt, Show } from "../types";
import { t } from "../i18n";

export const KIND_COLOR: Record<MarkKind, string> = {
  light: "var(--amber)", sound: "var(--cyan)", video: "var(--violet)", set: "var(--green)", other: "var(--mute)",
};
export const KIND_FILL: Record<MarkKind, string> = {
  light: "var(--amber-fill)", sound: "var(--cyan-fill)", video: "var(--violet-fill)", set: "var(--green-fill)", other: "var(--ink-3)",
};
export const MARK_KINDS: MarkKind[] = ["light", "sound", "video", "set", "other"];
export const kindLabel = (k: MarkKind): string =>
  ({ light: t("Mark: Light"), sound: t("Mark: Sound"), video: t("Mark: Video"), set: t("Mark: Scene"), other: t("Mark: Other") })[k];
export const blockKindLabel = (k: ScriptKind): string =>
  ({ heading: t("Heading"), speech: t("Speech"), direction: t("Stage direction"), note: t("Note") })[k];
export const BLOCK_KINDS: ScriptKind[] = ["heading", "speech", "direction", "note"];

export const SCRIPT_TEST_ID = 4_000_000_000;

export function words(text: string): string[] {
  return text.split(/\s+/).filter(Boolean);
}
export const nextId = (items: { id: number }[]): number => items.reduce((m, x) => Math.max(m, x.id), 0) + 1;

/** Marks of a block in reading order (stable for equal `at_word`). */
export function sortedMarks(b: ScriptBlock): CueMark[] {
  return b.marks.map((m, i) => ({ m, i })).sort((a, b2) => a.m.at_word - b2.m.at_word || a.i - b2.i).map((x) => x.m);
}

/** Normalise before saving: marks sorted, `at_word` clamped to the text. */
export function normalizeScript(s: Script): Script {
  return {
    ...s,
    blocks: s.blocks.map((b) => {
      const n = words(b.text).length;
      const marks = sortedMarks({ ...b, marks: b.marks.map((m) => ({ ...m, at_word: Math.max(0, Math.min(n, m.at_word)) })) });
      return { ...b, marks };
    }),
  };
}

export type FlatMark = { bi: number; block: ScriptBlock; mark: CueMark; idx: number };
export function flatMarks(s: Script): FlatMark[] {
  const out: FlatMark[] = [];
  s.blocks.forEach((block, bi) => sortedMarks(block).forEach((mark) => out.push({ bi, block, mark, idx: out.length })));
  return out;
}

export function blockIndex(s: Script, id: number): number {
  return s.blocks.findIndex((b) => b.id === id);
}

export type Position = {
  /** block index of the reader (-1 = before start) */
  bi: number;
  /** marks with idx <= passed are done */
  passed: number;
  next: FlatMark | null;
  standby: boolean;
  flat: FlatMark[];
};
export function position(s: Script, rt: ScriptRt | undefined): Position {
  const flat = flatMarks(s);
  const bi = rt && rt.block ? blockIndex(s, rt.block) : -1;
  const last = rt?.mark != null ? (flat.find((f) => f.block.id === rt.block && f.mark.id === rt.mark) ?? flat.find((f) => f.mark.id === rt.mark))?.idx ?? -1 : -1;
  const firstOfBlock = bi >= 0 ? flat.find((f) => f.bi >= bi)?.idx ?? flat.length : 0;
  const passed = Math.max(last, firstOfBlock - 1);
  const next = rt?.next ? flat.find((f) => f.block.id === rt.next![0] && f.mark.id === rt.next![1]) ?? null : flat[passed + 1] ?? null;
  const standby = !!next && next.mark.standby_blocks > 0 && next.bi - bi <= next.mark.standby_blocks;
  return { bi, passed, next, standby, flat };
}

export function actionSummary(a: ScriptAction): string {
  switch (a.type) {
    case "console": return a.text.trim() || t("Command");
    case "osc": return `OSC ${a.host}:${a.port} ${a.address}${a.args.length ? " " + a.args.join(" ") : ""}`;
    case "midi": return `MIDI ${({ note: t("Note"), cc: "CC", program: "PC" } as Record<string, string>)[a.kind]} ${a.number}/${a.value} ${t("ch {n}", { n: a.channel })}`;
    case "http": return `${a.method} ${a.url}`;
  }
}
export function marksSummary(m: CueMark, show: Show | null): string {
  const parts = m.actions.map(actionSummary);
  if (m.link) {
    const cl = show?.cuelists.find((c) => c.id === m.link!.cuelist);
    parts.unshift(t("Cue {n} · {list}", { n: fmt(m.link.number), list: cl?.name ?? `#${m.link.cuelist}` }));
  }
  return parts.join(" · ") || t("(no actions)");
}
export const fmt = (n: number) => (Number.isInteger(n) ? String(n) : String(n));

export function newMark(script: Script, kind: MarkKind = "light", atWord = 0): CueMark {
  const id = script.blocks.reduce((m, b) => b.marks.reduce((mm, x) => Math.max(mm, x.id), m), 0) + 1;
  return { id, label: "", kind, at_word: atWord, actions: [], link: null, standby_blocks: 2, note: "" };
}
export function newBlock(script: Script, kind: ScriptKind = "speech", text = ""): ScriptBlock {
  return { id: nextId(script.blocks), kind, speaker: "", text, page: 0, marks: [] };
}
export function blankScript(show: Show | null, name: string): Script {
  const id = nextId((show?.scripts ?? []).filter((s) => s.id < SCRIPT_TEST_ID));
  return { id, name, pdf: null, blocks: [{ id: 1, kind: "heading", speaker: "", text: name, page: 0, marks: [] }, { id: 2, kind: "direction", speaker: "", text: t("Write or paste the first lines here."), page: 0, marks: [] }] };
}

/** Split block `bi` before word `w`. */
export function splitBlock(s: Script, bi: number, w: number): Script {
  const b = s.blocks[bi];
  const ws = words(b.text);
  if (w <= 0 || w >= ws.length) return s;
  const id = nextId(s.blocks);
  const first: ScriptBlock = { ...b, text: ws.slice(0, w).join(" "), marks: b.marks.filter((m) => m.at_word < w) };
  const second: ScriptBlock = { ...b, id, text: ws.slice(w).join(" "), marks: b.marks.filter((m) => m.at_word >= w).map((m) => ({ ...m, at_word: m.at_word - w })) };
  const blocks = [...s.blocks];
  blocks.splice(bi, 1, first, second);
  return { ...s, blocks };
}
/** Merge block `bi` into the previous one. */
export function mergeBlock(s: Script, bi: number): Script {
  if (bi <= 0) return s;
  const prev = s.blocks[bi - 1], cur = s.blocks[bi];
  const n = words(prev.text).length;
  const merged: ScriptBlock = { ...prev, text: `${prev.text} ${cur.text}`.trim(), marks: [...prev.marks, ...cur.marks.map((m) => ({ ...m, at_word: m.at_word + n }))] };
  const blocks = [...s.blocks];
  blocks.splice(bi - 1, 2, merged);
  return { ...s, blocks };
}

export const fmtClock = (d: Date) => d.toLocaleTimeString([], { hour12: false });
