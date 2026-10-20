// Global client state + WebSocket link to the Lumos engine.
// The server is authoritative: the UI only sends Commands and renders what comes back.
import { signal } from "@preact/signals";
import { tErr } from "./i18n/errors";
import type { Command, FixtureProfile, Runtime, ServerMsg, Show, ShowInfo, Source } from "./types";

export const connected = signal(false);
export const show = signal<Show | null>(null);
export const rt = signal<Runtime | null>(null);
export const library = signal<FixtureProfile[]>([]);
export const shows = signal<ShowInfo[]>([]);
export const serverLog = signal<{ level: string; msg: string; at: number }[]>([]);

/** Fixture ids selected in the UI (client-side, shared across workspaces). */
export const selection = signal<number[]>([]);

/** Latest binary DMX frames by universe. Bump `dmxVersion` on every update. */
export const dmx = new Map<number, Uint8Array>();
export const dmxVersion = signal(0);

// ── toasts ──────────────────────────────────────────────
export type Toast = { id: number; msg: string; kind: "info" | "ok" | "err" };
export const toasts = signal<Toast[]>([]);
let toastId = 1;
export function toast(msg: string, kind: Toast["kind"] = "info", ms = 3500) {
  const t = { id: toastId++, msg, kind };
  toasts.value = [...toasts.value, t];
  setTimeout(() => (toasts.value = toasts.value.filter((x) => x.id !== t.id)), ms);
}

// ── link ────────────────────────────────────────────────
let ws: WebSocket | null = null;
let seq = 1;
const pending = new Map<string, { ok: () => void; err: (e: Error) => void }>();
const learnedListeners = new Set<(s: Source) => void>();
let dmxSub: number[] | null | undefined = undefined; // remembered for reconnects

/** Late-bound hooks so modules that import the store can receive server events without import cycles. */
export const hooks: { midiOut?: (port: string | null, bytes: number[]) => void } = {};

export function onLearned(cb: (s: Source) => void) {
  learnedListeners.add(cb);
  return () => learnedListeners.delete(cb);
}

/** Send a command. Resolves on `ack.ok`, rejects with the server's error. Errors also toast unless `quiet`. */
export function send(cmd: Command, opts: { quiet?: boolean } = {}): Promise<void> {
  return new Promise((ok, err) => {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      const e = new Error("Not connected");
      if (!opts.quiet) toast(tErr(e.message), "err");
      return err(e);
    }
    const id = String(seq++);
    pending.set(id, {
      ok,
      err: (e) => {
        if (!opts.quiet) toast(tErr(e.message), "err");
        err(e);
      },
    });
    ws.send(JSON.stringify({ t: "cmd", id, cmd }));
  });
}

/** Fire-and-forget (high-rate controls such as faders). */
export function fire(cmd: Command) {
  if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ t: "cmd", id: "", cmd }));
}

export function subscribeDmx(universes: number[] | null) {
  dmxSub = universes;
  fire({ type: "subscribe_dmx", universes });
}

function onMessage(ev: MessageEvent) {
  if (typeof ev.data !== "string") {
    const b = new Uint8Array(ev.data as ArrayBuffer);
    if (b.length >= 515 && b[0] === 0x01) {
      const u = (b[1] << 8) | b[2];
      const prev = dmx.get(u);
      if (prev) prev.set(b.subarray(3, 515));
      else dmx.set(u, b.slice(3, 515));
      dmxVersion.value++;
    }
    return;
  }
  const m = JSON.parse(ev.data) as ServerMsg;
  switch (m.t) {
    case "show": show.value = m.show; break;
    case "rt": rt.value = m.rt; break;
    case "library": library.value = m.profiles; break;
    case "shows": shows.value = m.shows; break;
    case "learned": learnedListeners.forEach((f) => f(m.source)); break;
    case "midi_out": hooks.midiOut?.(m.port, m.bytes); break;
    case "log":
      serverLog.value = [{ level: m.level, msg: m.msg, at: Date.now() }, ...serverLog.value].slice(0, 200);
      if (m.level === "error") toast(tErr(m.msg), "err");
      break;
    case "ack": {
      const p = pending.get(m.id);
      if (p) {
        pending.delete(m.id);
        m.ok ? p.ok() : p.err(new Error(m.err ?? "Command failed"));
      }
      break;
    }
    case "hello": break;
  }
}

export function connect() {
  const url = `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`;
  ws = new WebSocket(url);
  ws.binaryType = "arraybuffer";
  ws.onopen = () => {
    connected.value = true;
    if (dmxSub !== undefined) fire({ type: "subscribe_dmx", universes: dmxSub });
  };
  ws.onmessage = onMessage;
  ws.onclose = () => {
    connected.value = false;
    pending.forEach((p) => p.err(new Error("Connection lost")));
    pending.clear();
    setTimeout(connect, 1000);
  };
  ws.onerror = () => ws?.close();
}

// ── helpers shared by workspaces ────────────────────────
export function profileOf(id: string): FixtureProfile | undefined {
  return library.value.find((p) => p.id === id);
}
