// Assistant state: config from the server, per-window chat session, event reducer.
// Provider-neutral on purpose — the UI never assumes which model/vendor is behind it.
import { computed, effect, signal } from "@preact/signals";
import { connected, hooks, selection, send, toast } from "../store";
import { lang, t } from "../i18n";
import { tErr } from "../i18n/errors";
import { winId } from "../lib/windows";
import type { AiConfigIn, AiConfigOut, AiEvent, AiMode, AiModelInfo, AiProviderOut, AiRisk, AiToolState, ServerMsg } from "../types";

// One server-side conversation per window *and tab*: the id lives in sessionStorage next to the persisted transcript,
// so a reload resumes the same conversation and a second tab never shares (or fights over) it.
const tabId = (() => {
  try {
    let v = sessionStorage.getItem("ai.tab");
    if (!v) { v = Math.random().toString(36).slice(2, 8); sessionStorage.setItem("ai.tab", v); }
    return v;
  } catch { return Math.random().toString(36).slice(2, 8); }
})();
export const SESSION = `ai-${winId ?? "main"}-${tabId}`;

// ── server config ───────────────────────────────────────
export const aiConfig = signal<AiConfigOut | null>(null);
export const aiTests = signal<Record<string, "running" | { ok: boolean; message: string; ms: number }>>({});

export const activeProvider = computed<AiProviderOut | undefined>(() => {
  const c = aiConfig.value;
  return c?.providers.find((p) => p.id === c.active_provider);
});
export const activeModel = computed<AiModelInfo | undefined>(() => {
  const c = aiConfig.value;
  return activeProvider.value?.models.find((m) => m.id === c?.model);
});
/** Enabled, an active provider with a key when it needs one, and a model chosen. */
export const aiReady = computed(() => {
  const c = aiConfig.value;
  const p = activeProvider.value;
  return !!(c && c.enabled && p && (!p.needs_key || p.has_key) && c.model);
});

/** Full `AiConfigIn` with every field null except the ones given. */
export function configure(part: Partial<AiConfigIn>): Promise<void> {
  const cfg: AiConfigIn = {
    provider: null, remove_provider: null, active_provider: null, model: null, enabled: null, effort: null, mode: null, style: null, ...part,
  };
  return send({ type: "ai_configure", cfg });
}

export function testProvider(id: string): Promise<void> {
  aiTests.value = { ...aiTests.value, [id]: "running" };
  return send({ type: "ai_test_provider", provider: id }).catch((e) => {
    aiTests.value = { ...aiTests.value, [id]: { ok: false, message: tErr((e as Error).message), ms: 0 } };
  });
}
export const listModels = (id: string) => send({ type: "ai_list_models", provider: id });

// ── chat state ──────────────────────────────────────────
export type Thumb = { media_type: string; data: string; url: string };

export type Item =
  | { k: "user"; id: number; text: string; thumbs: string[] }
  | { k: "ai"; id: number; text: string; streaming: boolean }
  | { k: "tool"; id: number; call: string; name: string; summary: string; risk: AiRisk; state: AiToolState; result: string }
  | { k: "confirm"; id: number; call: string; name: string; summary: string; risk: AiRisk; args: string; resolved?: "yes" | "no" }
  | { k: "ask"; id: number; call: string; question: string; options: string[]; answered?: string }
  | { k: "error"; id: number; msg: string }
  | { k: "info"; id: number; text: string };

export const panelOpen = signal(false);
export const quickOpen = signal(false);
export const draft = signal("");
export const aiWorkspace = signal("live");
export const busy = signal(false);
export const statusLine = signal("");
export const usage = signal({ inp: 0, out: 0 });
export const changes = signal(0);
export const speakReplies = signal(localStorage.getItem("ai.speak") === "1");
export const focusInput = signal(0);
/** Navigation requests from the assistant (App owns the router). */
export const nav = signal<{ ws: string; cat?: string } | null>(null);

const KEY = `ai.items.${SESSION}`;
let nid = 1;
const next = () => nid++;

let staleTurn = false;
function load(): Item[] {
  try {
    const raw = sessionStorage.getItem(KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw) as Item[];
    nid = arr.reduce((m, i) => Math.max(m, i.id), 0) + 1;
    // A reloaded page can't resume a turn: close its open cards and cancel the server side (see below).
    return arr.map((i) => {
      if (i.k === "ai" && i.streaming) { staleTurn = true; return { ...i, streaming: false }; }
      if (i.k === "tool" && i.state === "running") { staleTurn = true; return { ...i, state: "error" as AiToolState }; }
      if (i.k === "confirm" && !i.resolved) { staleTurn = true; return { ...i, resolved: "no" as const }; }
      if (i.k === "ask" && i.answered === undefined) { staleTurn = true; return { ...i, answered: "—" }; }
      return i;
    });
  } catch { return []; }
}
export const items = signal<Item[]>(load());
if (staleTurn) {
  const stop = effect(() => { if (connected.value) { void send({ type: "ai_cancel", session: SESSION }, { quiet: true }).catch(() => {}); queueMicrotask(() => stop()); } });
}

let saveT: ReturnType<typeof setTimeout> | undefined;
effect(() => {
  const arr = items.value;
  clearTimeout(saveT);
  saveT = setTimeout(() => {
    try {
      const slim = arr.slice(-200).map((i) => (i.k === "tool" && i.result.length > 2000 ? { ...i, result: i.result.slice(0, 2000) + "…" } : i));
      sessionStorage.setItem(KEY, JSON.stringify(slim));
    } catch { /* quota */ }
  }, 400);
});

const push = (i: Item) => { items.value = [...items.value, i]; };
const patch = (pred: (i: Item) => boolean, fn: (i: Item) => Item) => { items.value = items.value.map((i) => (pred(i) ? fn(i) : i)); };
// Some providers reuse call ids across turns ("call_0"), so cards are matched only within the current turn.
const turnStart = () => { const a = items.value; for (let i = a.length - 1; i >= 0; i--) if (a[i].k === "user") return i; return -1; };
const inTurn = (kind: Item["k"], call: string) => { const a = items.value; const s = turnStart(); return a.some((i, n) => n > s && i.k === kind && (i as { call?: string }).call === call); };
const patchCall = (kind: Item["k"], call: string, fn: (i: Item) => Item) => {
  const s = turnStart();
  items.value = items.value.map((i, n) => (n > s && i.k === kind && (i as { call?: string }).call === call ? fn(i) : i));
};
const lastAi = () => { const a = items.value; const l = a[a.length - 1]; return l && l.k === "ai" ? l : undefined; };
const closeStream = () => { flush(); const l = lastAi(); if (l?.streaming) patch((i) => i.id === l.id, (i) => ({ ...(i as Item & { k: "ai" }), streaming: false })); };

// Text deltas arrive many times per frame: coalesce.
let pendingText = "";
let raf = 0;
function flush() {
  if (raf) { cancelAnimationFrame(raf); raf = 0; }
  if (!pendingText) return;
  const add = pendingText;
  pendingText = "";
  const l = lastAi();
  if (l?.streaming) patch((i) => i.id === l.id, (i) => ({ ...(i as Item & { k: "ai" }), text: (i as Item & { k: "ai" }).text + add }));
  else push({ k: "ai", id: next(), text: add, streaming: true });
}

let autoApproveTurn = false;
let cancelTimer: ReturnType<typeof setTimeout> | undefined;

function endTurn() {
  busy.value = false;
  statusLine.value = "";
  autoApproveTurn = false;
  clearTimeout(cancelTimer);
}

export function handleAiEvent(ev: AiEvent) {
  switch (ev.type) {
    case "started":
      busy.value = true;
      statusLine.value = t("Thinking…");
      break;
    case "text":
      pendingText += ev.delta;
      statusLine.value = "";
      if (!raf) raf = requestAnimationFrame(() => { raf = 0; flush(); });
      break;
    case "status":
      statusLine.value = ev.text;
      break;
    case "tool": {
      closeStream();
      const has = inTurn("tool", ev.call);
      if (has) patchCall("tool", ev.call, (i) => ({ ...(i as Item & { k: "tool" }), summary: ev.summary || (i as Item & { k: "tool" }).summary, risk: ev.risk, state: ev.state, result: ev.result }));
      else push({ k: "tool", id: next(), call: ev.call, name: ev.name, summary: ev.summary, risk: ev.risk, state: ev.state, result: ev.result });
      // the confirmation, if any, is settled: the tool card now carries the outcome
      { const st = turnStart(); items.value = items.value.filter((i, n) => !(n > st && i.k === "confirm" && i.call === ev.call)); }
      break;
    }
    case "confirm": {
      closeStream();
      if (autoApproveTurn) {
        push({ k: "confirm", id: next(), call: ev.call, name: ev.name, summary: ev.summary, risk: ev.risk, args: ev.args, resolved: "yes" });
        void send({ type: "ai_confirm", session: SESSION, call: ev.call, approve: true }, { quiet: true }).catch(() => {});
      } else {
        push({ k: "confirm", id: next(), call: ev.call, name: ev.name, summary: ev.summary, risk: ev.risk, args: ev.args });
        panelOpen.value = true;
      }
      break;
    }
    case "ask":
      closeStream();
      push({ k: "ask", id: next(), call: ev.call, question: ev.question, options: ev.options });
      panelOpen.value = true;
      break;
    case "done": {
      closeStream();
      usage.value = { inp: usage.value.inp + ev.input_tokens, out: usage.value.out + ev.output_tokens };
      changes.value = ev.changes;
      const last = [...items.value].reverse().find((i) => i.k === "ai");
      endTurn();
      if (speakReplies.value && last && last.k === "ai") speak(last.text);
      break;
    }
    case "error":
      closeStream();
      push({ k: "error", id: next(), msg: tErr(ev.msg) });
      endTurn();
      break;
  }
}

// ── actions ─────────────────────────────────────────────
function context() {
  return { workspace: aiWorkspace.value, selection: [...selection.value], lang: lang.value };
}

export async function aiSend(text: string, images: Thumb[] = []) {
  text = text.trim();
  if ((!text && !images.length) || busy.value) return;
  stopSpeaking();
  push({ k: "user", id: next(), text, thumbs: images.map((i) => i.url) });
  busy.value = true;
  autoApproveTurn = false;
  statusLine.value = t("Sending…");
  try {
    await send({ type: "ai_send", session: SESSION, text, images: images.map((i) => ({ media_type: i.media_type, data: i.data })), context: context() }, { quiet: true });
  } catch (e) {
    push({ k: "error", id: next(), msg: tErr((e as Error).message) });
    endTurn();
  }
}

/** Open the panel and ask (or prefill when the assistant isn't ready / is busy). */
export function askAssistant(text: string, opts: { send?: boolean } = {}) {
  panelOpen.value = true;
  quickOpen.value = false;
  if (opts.send !== false && aiReady.value && !busy.value) void aiSend(text);
  else { draft.value = text; focusInput.value++; }
}

export function openAiSettings() {
  panelOpen.value = false;
  quickOpen.value = false;
  nav.value = { ws: "settings", cat: "ai" };
}

/** Ctrl/Cmd+K: quick-ask palette, or focus the input when the panel is already open. */
export function toggleQuick() {
  if (panelOpen.value) { focusInput.value++; return; }
  quickOpen.value = !quickOpen.value;
}

export function aiCancel() {
  void send({ type: "ai_cancel", session: SESSION }, { quiet: true }).catch(() => {});
  statusLine.value = t("Stopping…");
  clearTimeout(cancelTimer);
  cancelTimer = setTimeout(() => { if (busy.value) { closeStream(); endTurn(); } }, 2500);
}

export function aiConfirm(call: string, approve: boolean, allTurn = false) {
  if (allTurn) autoApproveTurn = true;
  patchCall("confirm", call, (i) => ({ ...(i as Item & { k: "confirm" }), resolved: approve ? "yes" : "no" }));
  void send({ type: "ai_confirm", session: SESSION, call, approve }, { quiet: true }).catch((e) => toast(tErr((e as Error).message), "err"));
}

export function aiAnswer(call: string, answer: string) {
  patchCall("ask", call, (i) => ({ ...(i as Item & { k: "ask" }), answered: answer }));
  void send({ type: "ai_answer", session: SESSION, call, answer }, { quiet: true }).catch((e) => toast(tErr((e as Error).message), "err"));
}

export function aiReset() {
  void send({ type: "ai_reset", session: SESSION }, { quiet: true }).catch(() => {});
  items.value = [];
  usage.value = { inp: 0, out: 0 };
  changes.value = 0;
  pendingText = "";
  endTurn();
  stopSpeaking();
}

export async function aiUndo() {
  try {
    await send({ type: "ai_undo", session: SESSION }, { quiet: true });
    push({ k: "info", id: next(), text: t("The assistant's changes were undone.") });
    changes.value = 0;
  } catch (e) {
    toast(tErr((e as Error).message), "err");
  }
}

export const setMode = (mode: AiMode) => configure({ mode }).catch(() => {});

// ── speech out ──────────────────────────────────────────
export const canSpeak = typeof window !== "undefined" && "speechSynthesis" in window;
export function setSpeak(on: boolean) {
  speakReplies.value = on;
  localStorage.setItem("ai.speak", on ? "1" : "0");
  if (!on) stopSpeaking();
}
export function stopSpeaking() { if (canSpeak) window.speechSynthesis.cancel(); }
function plain(md: string) {
  return md.replace(/```[\s\S]*?```/g, " ").replace(/`([^`]*)`/g, "$1").replace(/[*_#>]+/g, "").replace(/\[([^\]]*)\]\([^)]*\)/g, "$1").replace(/\s+/g, " ").trim();
}
export function speak(md: string) {
  if (!canSpeak) return;
  const u = new SpeechSynthesisUtterance(plain(md).slice(0, 1500));
  u.lang = lang.value === "es" ? "es-ES" : "en-US";
  window.speechSynthesis.cancel();
  window.speechSynthesis.speak(u);
}

// ── wire into the store ─────────────────────────────────
hooks.ai = (m: ServerMsg) => {
  if (m.t === "ai_config") aiConfig.value = m.cfg;
  else if (m.t === "ai_event") { if (m.session === SESSION) handleAiEvent(m.ev); }
  else if (m.t === "ai_test") aiTests.value = { ...aiTests.value, [m.provider]: { ok: m.ok, message: m.message, ms: m.ms } };
};

// ── workspace suggestions ───────────────────────────────
export function suggestions(ws: string): string[] {
  const general = [t("Why is there no light on fixture 3?"), t("Teach me how to make a chase")];
  const by: Record<string, string[]> = {
    live: [t("Create a sunrise scene with the selected fixtures"), t("Set the selected fixtures to a warm white at 70%")],
    desk: [t("Which channels are parked and why?")],
    patch: [t("Add 8 LED pars on universe 1"), t("Check my patch for address overlaps")],
    cues: [t("Set a 3 s fade on all cues"), t("Explain what tracking does in my cue list")],
    script: [t("Suggest cues from the stage directions"), t("Link each cue mark to the matching Lumos cue")],
    settings: [t("Help me send Art-Net to a node at 192.168.1.50")],
    vc: [t("Build a page with GO, BACK and one slider per group")],
    stage: [t("Turn on the fog machine and make the beams visible")],
    timeline: [t("Create a timeline that fires cue 1 at 10 s and cue 2 at 25 s")],
  };
  return [...(by[ws] ?? []), ...general].slice(0, 4);
}
