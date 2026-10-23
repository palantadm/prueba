import { signal } from "@preact/signals";

function load<T>(k: string, d: T): T {
  try { const v = localStorage.getItem(k); return v === null ? d : (JSON.parse(v) as T); } catch { return d; }
}
function persist<T>(k: string, v: T) { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* ignore */ } }

export const cmdOpen = signal<boolean>(load("cmd.open", false));
export const cmdText = signal("");
export const cmdLogOpen = signal<boolean>(load("cmd.log", false));
export const cmdSoftOpen = signal<boolean>(load("cmd.soft", false));
export const cmdHistory = signal<string[]>(load("cmd.hist", []));
/** Bumped to ask the command input to take focus. */
export const cmdFocus = signal(0);
/** Local error of the last submitted line (ack error), cleared when typing. */
export const cmdError = signal("");

export function setCmdOpen(v: boolean) { cmdOpen.value = v; persist("cmd.open", v); if (v) cmdFocus.value++; }
export function toggleCmd() { setCmdOpen(!cmdOpen.value); }
export function setLogOpen(v: boolean) { cmdLogOpen.value = v; persist("cmd.log", v); }
export function setSoftOpen(v: boolean) { cmdSoftOpen.value = v; persist("cmd.soft", v); }
export function pushHistory(line: string) {
  const h = cmdHistory.value.filter((x) => x !== line);
  h.push(line);
  cmdHistory.value = h.slice(-100);
  persist("cmd.hist", cmdHistory.value);
}
