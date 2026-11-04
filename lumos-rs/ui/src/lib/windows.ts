// Multi-window support: secondary windows (?win=<id>), opening windows (desktop app or browser popup),
// and UI-only state shared between windows of the same machine (BroadcastChannel).
import { effect, type Signal } from "@preact/signals";
import { selection, toast } from "../store";
import { t } from "../i18n";

const q = new URLSearchParams(location.search);

/** Id of this window when it is a secondary one (`?win=2`), otherwise null (main window). */
export const winId: string | null = q.get("win");
export const isSecondary = winId !== null;

/** localStorage key holding this window's current workspace (secondary windows must not overwrite the main one). */
export const wsKey = isSecondary ? `ws:${winId}` : "ws";

/** The desktop wrapper (Tauri) marks the windows it creates with `?app=1`. */
export const isDesktopApp: boolean = (() => {
  try {
    if (q.get("app") === "1") sessionStorage.setItem("lumos-app", "1");
    return sessionStorage.getItem("lumos-app") === "1";
  } catch {
    return q.get("app") === "1";
  }
})();

export function initialWorkspace(): string {
  const fromUrl = q.get("ws");
  if (fromUrl) {
    // a secondary window remembers what it was opened with, so it is restored after a restart
    if (isSecondary) saveWorkspace(fromUrl);
    return fromUrl;
  }
  try { return localStorage.getItem(wsKey) ?? "live"; } catch { return "live"; }
}

export function saveWorkspace(id: string) {
  try { localStorage.setItem(wsKey, id); } catch { /* storage blocked */ }
}

function nextWinId(): number {
  let n = 1;
  try { n = parseInt(localStorage.getItem("lumos-win-seq") ?? "1", 10) || 1; } catch { /* ignore */ }
  const id = Math.max(2, n + 1);
  try { localStorage.setItem("lumos-win-seq", String(id)); } catch { /* ignore */ }
  return id;
}

export type OpenOpts = { ws: string; monitor?: boolean; fullscreen?: boolean };

type ScreenInfo = { left: number; top: number; width: number; height: number; isPrimary?: boolean };
type Win = Window & { getScreenDetails?: () => Promise<{ screens: ScreenInfo[] }> };

/** True when the browser reports more than one screen (Chromium: `screen.isExtended`). */
export function hasSecondScreen(): boolean {
  return isDesktopApp || Boolean((window.screen as Screen & { isExtended?: boolean }).isExtended);
}

/** Opens a new window showing workspace `ws`. Desktop app: the wrapper intercepts `window.open` and creates a native window. */
export async function openWindow(o: OpenOpts): Promise<boolean> {
  const id = nextWinId();
  const url = new URL(location.href);
  url.search = "";
  url.searchParams.set("ws", o.ws);
  url.searchParams.set("win", String(id));
  if (o.monitor) url.searchParams.set("mon", "next");
  if (o.fullscreen) url.searchParams.set("fs", "1");
  if (isDesktopApp) url.searchParams.set("app", "1");

  let features = "popup=yes,width=1280,height=800";
  const w = window as Win;
  if (!isDesktopApp && o.monitor && w.getScreenDetails) {
    try {
      const d = await w.getScreenDetails();
      const here = { x: window.screenX, y: window.screenY };
      const other = d.screens.find((s) => !(here.x >= s.left && here.x < s.left + s.width && here.y >= s.top && here.y < s.top + s.height)) ?? d.screens[0];
      if (other) features = `popup=yes,left=${other.left},top=${other.top},width=${other.width},height=${other.height}`;
    } catch {
      toast(t("Screen permission denied; opening on this screen"), "info", 3500);
    }
  }
  const win = window.open(url.toString(), `lumos-${id}`, features);
  if (!win && !isDesktopApp) {
    toast(t("Pop-ups are blocked: allow them for this site to open more windows"), "err", 5000);
    return false;
  }
  return true;
}

function desktopCmd(cmd: string) {
  location.assign(`/__lumos/${cmd}`);
}

export function closeThisWindow() {
  if (isDesktopApp) desktopCmd("close");
  else window.close();
}

export function toggleFullscreen() {
  if (isDesktopApp) { desktopCmd("fullscreen"); return; }
  if (document.fullscreenElement) void document.exitFullscreen();
  else void document.documentElement.requestFullscreen?.().catch(() => undefined);
}

export function moveToNextMonitor() {
  if (isDesktopApp) desktopCmd("move");
}

// ── state shared between windows of this machine ─────────────────────────
const tabId = Math.random().toString(36).slice(2);
let channel: BroadcastChannel | null = null;
const shared = new Map<string, Signal<unknown>>();
let applying = false;

/** Mirror a UI-only signal across this machine's windows (value must be JSON-serialisable). */
export function shareSignal<T>(key: string, sig: Signal<T>) {
  if (shared.has(key) || typeof BroadcastChannel === "undefined") return;
  shared.set(key, sig as Signal<unknown>);
  let last = JSON.stringify(sig.peek());
  effect(() => {
    const v = JSON.stringify(sig.value);
    if (applying || v === last) return;
    last = v;
    channel?.postMessage({ k: key, v: sig.peek(), from: tabId });
  });
  (sig as unknown as { __lastRemote?: (s: string) => void }).__lastRemote = (s) => { last = s; };
}

export function initWindowSync() {
  if (channel || typeof BroadcastChannel === "undefined") return;
  channel = new BroadcastChannel("lumos-ui");
  shareSignal("selection", selection);
  channel.onmessage = (ev: MessageEvent) => {
    const m = ev.data as { k: string; v: unknown; from: string };
    if (!m || m.from === tabId) return;
    const sig = shared.get(m.k);
    if (!sig) return;
    const s = JSON.stringify(m.v);
    if (JSON.stringify(sig.peek()) === s) return;
    (sig as unknown as { __lastRemote?: (s: string) => void }).__lastRemote?.(s);
    applying = true;
    try { sig.value = m.v; } finally { applying = false; }
  };
}
