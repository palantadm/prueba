import { signal } from "@preact/signals";

/** Everything the interface reports (toasts, failed uploads, script errors…), newest last, kept in memory for the log window. */
export type LogLevel = "info" | "ok" | "err";
export type LogEntry = { id: number; t: number; level: LogLevel; source: string; msg: string };

export const logEntries = signal<LogEntry[]>([]);
export const logOpen = signal(false);
export const logErrors = signal(0);
let next = 1;

export function logAdd(level: LogLevel, source: string, msg: string) {
  const text = String(msg ?? "").trim();
  if (!text) return;
  const e: LogEntry = { id: next++, t: Date.now(), level, source, msg: text };
  const cur = logEntries.value;
  logEntries.value = cur.length >= 400 ? [...cur.slice(cur.length - 399), e] : [...cur, e];
  if (level === "err") logErrors.value += 1;
}

/** Search words to open the manual with for a message. */
export function helpQuery(msg: string): string {
  const m = msg.toLowerCase();
  const rules: [RegExp, string][] = [
    [/qxf|qlc|gdtf|\.json|fixture|aparato|biblioteca|import/, "importar biblioteca"],
    [/art-?net|sacn|dmx|universo|salida|output/, "art-net"],
    [/osc|resolume|qlab/, "osc"],
    [/lua|script/, "lua"],
    [/rdm/, "rdm"],
    [/psn|tracker|blacktrax/, "seguimiento"],
    [/midi/, "midi"],
    [/audio|ltc|micr/, "audio"],
    [/mvr|csv/, "patch"],
  ];
  for (const [re, q] of rules) if (re.test(m)) return q;
  return m.replace(/[^\p{L}\p{N} ]+/gu, " ").split(/\s+/).filter((w) => w.length > 3).slice(0, 4).join(" ");
}

if (typeof window !== "undefined") {
  window.addEventListener("error", (e) => logAdd("err", "interfaz", e.message || "Error de la interfaz"));
  window.addEventListener("unhandledrejection", (e) => logAdd("err", "interfaz", String((e.reason as Error)?.message ?? e.reason)));
}
