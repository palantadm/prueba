// Command-line helpers: local keyword list (fallback), server help fetch, soft keys, token insertion.
import { signal } from "@preact/signals";
import { t } from "../i18n";

export type HelpEntry = { text: string; desc: string };
export type CmdHelp = { keywords: string[]; examples: HelpEntry[]; raw: unknown };

/** Fallback vocabulary when the engine's /api/cmdline/help is unavailable (English + Spanish aliases). */
export const LOCAL_KEYWORDS: string[] = [
  "fixture", "fx", "aparato", "group", "grupo", "all", "todos", "odd", "even", "impares", "pares", "next", "prev", "thru", "hasta",
  "at", "full", "out", "home", "colour", "color", "dimmer", "pan", "tilt", "red", "green", "blue", "white", "zoom", "focus", "iris",
  "record", "grabar", "update", "actualizar", "delete", "borrar", "cue", "cuelist", "palette", "paleta", "scene", "escena", "apply",
  "go", "back", "pause", "release", "liberar", "goto", "solo", "on", "off", "toggle", "chase", "macro", "timeline",
  "run", "stop", "blackout", "gm", "clear", "clr", "park", "undo", "redo", "blind", "name",
];

export const helpData = signal<CmdHelp | null>(null);
let helpPromise: Promise<void> | null = null;

function entry(x: any): HelpEntry | null {
  if (typeof x === "string") return { text: x, desc: "" };
  if (x && typeof x === "object") {
    const text = x.text ?? x.example ?? x.syntax ?? x.cmd ?? x.command ?? x.word ?? x.keyword ?? x.name;
    if (text === undefined) return null;
    return { text: String(text), desc: String(x.desc ?? x.description ?? x.help ?? x.doc ?? "") };
  }
  return null;
}

function arr(j: any, ...keys: string[]): any[] {
  for (const k of keys) if (Array.isArray(j?.[k])) return j[k];
  return [];
}

/** Fetch the grammar summary once (tolerant to the exact JSON shape). */
export function loadHelp(): Promise<void> {
  if (!helpPromise) {
    helpPromise = fetch("/api/cmdline/help")
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
      .then((j) => {
        const kw = arr(j, "keywords", "words", "vocabulary").map(entry).filter(Boolean).map((e) => (e as HelpEntry).text);
        const ex = arr(j, "examples", "samples").map(entry).filter(Boolean) as HelpEntry[];
        helpData.value = { keywords: kw, examples: ex, raw: j };
      })
      .catch(() => { helpPromise = null; /* retry on next open */ });
  }
  return helpPromise;
}

export function keywords(): string[] {
  const s = helpData.value?.keywords ?? [];
  return s.length ? Array.from(new Set([...s, ...LOCAL_KEYWORDS])) : LOCAL_KEYWORDS;
}

/** Last token being typed and where it starts. */
export function currentToken(text: string): { prefix: string; start: number } {
  const m = /[A-Za-zÀ-ÿ_]*$/.exec(text);
  const prefix = m ? m[0] : "";
  return { prefix, start: text.length - prefix.length };
}

export function suggest(text: string, max = 8): string[] {
  const { prefix } = currentToken(text);
  if (!prefix) return [];
  const p = prefix.toLowerCase();
  return keywords().filter((k) => k.toLowerCase().startsWith(p) && k.toLowerCase() !== p).slice(0, max);
}

export function applySuggestion(text: string, word: string): string {
  const { start } = currentToken(text);
  return text.slice(0, start) + word + " ";
}

/** Insert a soft-key token with sensible spacing. */
export function appendToken(text: string, tok: string): string {
  if (tok === ";") return text.replace(/\s*$/, "") + "; ";
  if (/^[0-9.]$/.test(tok)) {
    if (!text || /[\s;]$/.test(text) || /[0-9.]$/.test(text)) return text + tok;
    return text + " " + tok;
  }
  const base = !text || /[\s;]$/.test(text) ? text : text + " ";
  return base + tok + (tok.endsWith(" ") ? "" : " ");
}

export type SoftKey = { label: string; tok: string; kind?: "sel" | "val" | "act" | "play" };

/** Rows of on-screen keys. Tokens are English keywords (Spanish aliases work too). */
export function softKeys(): SoftKey[] {
  return [
    { label: t("Fixture"), tok: "fixture ", kind: "sel" },
    { label: t("Group"), tok: "group ", kind: "sel" },
    { label: t("All"), tok: "all ", kind: "sel" },
    { label: "Thru", tok: "thru ", kind: "sel" },
    { label: "+", tok: "+ ", kind: "sel" },
    { label: "−", tok: "- ", kind: "sel" },
    { label: "At", tok: "at ", kind: "val" },
    { label: "Full", tok: "full", kind: "val" },
    { label: "Out", tok: "out", kind: "val" },
    { label: "Home", tok: "home", kind: "val" },
    { label: "Dimmer", tok: "dimmer ", kind: "val" },
    { label: "Pan", tok: "pan ", kind: "val" },
    { label: "Tilt", tok: "tilt ", kind: "val" },
    { label: t("Record"), tok: "record ", kind: "act" },
    { label: t("Update"), tok: "update ", kind: "act" },
    { label: t("Delete"), tok: "delete ", kind: "act" },
    { label: "Cue", tok: "cue ", kind: "act" },
    { label: t("Palette"), tok: "palette ", kind: "act" },
    { label: t("Scene"), tok: "scene ", kind: "act" },
    { label: "Go", tok: "go", kind: "play" },
    { label: "Back", tok: "back", kind: "play" },
    { label: "Goto", tok: "goto cue ", kind: "play" },
    { label: "On", tok: "on", kind: "play" },
    { label: "Off", tok: "off", kind: "play" },
    { label: "Macro", tok: "macro ", kind: "play" },
    { label: "Timeline", tok: "timeline ", kind: "play" },
    { label: "Blackout", tok: "blackout ", kind: "play" },
    { label: "GM", tok: "gm ", kind: "play" },
    { label: "Clear", tok: "clear", kind: "act" },
    { label: "Undo", tok: "undo", kind: "act" },
    { label: "Redo", tok: "redo", kind: "act" },
    { label: "Blind", tok: "blind ", kind: "act" },
    { label: "Park", tok: "park ", kind: "act" },
  ];
}

export const KEYPAD = ["7", "8", "9", "4", "5", "6", "1", "2", "3", "0", ".", ";"];

/** Static help content (Spanish via t()). Syntax stays literal. */
export function helpSections(): { title: string; rows: [string, string][] }[] {
  return [
    {
      title: t("Selection"),
      rows: [
        ["fixture 1 thru 10", t("Select fixtures 1 to 10")],
        ["fixture 1 thru 10 + 15 - 4", t("Add 15, remove 4")],
        ["group 2", t("Select the fixtures of group 2")],
        ["all · odd · even · next · prev", t("All, odd, even, next or previous fixture")],
      ],
    },
    {
      title: t("Values"),
      rows: [
        ["at 50 · at full · at out · at home", t("Set the selection's dimmer / reset to home")],
        ["pan at 30 · red at 100", t("Set one attribute (percent)")],
        ["fixture 3 colour red", t("Set a named colour")],
        ["fixture 1 thru 8 at 70; red at 100", t("Several statements separated by ;")],
      ],
    },
    {
      title: t("Recording"),
      rows: [
        ["record cue 5 [cuelist 2]", t("Record the programmer into a cue")],
        ["update cue 5 · delete cue 5", t("Update or delete a cue")],
        ["record palette 3 name Azul · record scene 2", t("Record palettes and scenes")],
        ["apply palette 3", t("Apply a palette to the selection")],
      ],
    },
    {
      title: t("Playback"),
      rows: [
        ["go · back · pause · release [cuelist 2]", t("Transport of a cuelist")],
        ["goto cue 12", t("Jump to a cue")],
        ["scene 4 on · scene 4 off · scene 4 solo", t("Scenes")],
        ["chase 2 on · macro 3 · timeline 1 run", t("Chases, macros and timelines")],
        ["blackout on · gm 80 · clear", t("Masters and programmer")],
        ["park 1/12 at 255 · undo · redo · blind on", t("Park, history and blind mode")],
      ],
    },
  ];
}

/** Format seconds as mm:ss.d */
export function fmtTime(s: number): string {
  const sign = s < 0 ? "-" : "";
  s = Math.abs(s);
  const m = Math.floor(s / 60);
  const r = s - m * 60;
  return `${sign}${String(m).padStart(2, "0")}:${r.toFixed(1).padStart(4, "0")}`;
}

export function fmtTc(sec: number, fps: number): string {
  const f = Math.max(1, Math.round(fps));
  const total = Math.max(0, Math.round(sec * f));
  const ff = total % f;
  const ss = Math.floor(total / f) % 60;
  const mm = Math.floor(total / f / 60) % 60;
  const hh = Math.floor(total / f / 3600);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(hh)}:${p(mm)}:${p(ss)}:${p(ff)}`;
}

export function parseTc(s: string, fps: number): number | null {
  const m = /^(\d{1,2}):(\d{2}):(\d{2})[:;](\d{2})$/.exec(s.trim());
  if (!m) return null;
  const f = Math.max(1, Math.round(fps));
  return Number(m[1]) * 3600 + Number(m[2]) * 60 + Number(m[3]) + Number(m[4]) / f;
}
