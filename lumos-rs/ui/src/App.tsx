import { signal } from "@preact/signals";
import { useEffect } from "preact/hooks";
import { connected, rt, send, show } from "./store";
import { workspaces } from "./workspaces";
import { Toasts } from "./ui";

export const current = signal(localStorage.getItem("ws") ?? "live");

export function App() {
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.matches?.("input,textarea,select")) return;
      const w = workspaces.find((w) => w.key === e.key);
      if (w) current.value = w.id;
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);
  const ws = workspaces.find((w) => w.id === current.value) ?? workspaces[0];
  const Cur = ws.component;
  return (
    <div class="col" style={{ height: "100%", gap: 0 }}>
      <header class="row" style={{ height: 44, padding: "0 12px", background: "var(--bg-1)", borderBottom: "1px solid var(--line)" }}>
        <b style={{ letterSpacing: ".2em", color: "var(--live)" }}>⬡ LUMOS</b>
        <nav class="row" style={{ marginLeft: 16, gap: 2 }}>
          {workspaces.map((w) => (
            <button key={w.id} class={`btn ghost ${w.id === ws.id ? "sel" : ""}`} onClick={() => { current.value = w.id; localStorage.setItem("ws", w.id); }}>
              {w.label}
            </button>
          ))}
        </nav>
        <span class="sp" />
        <span class="chip" title={connected.value ? "Connected" : "Disconnected"}><span class={`dot ${connected.value ? "ok" : "err"}`} />{show.value?.name ?? "—"}</span>
        <button class={`btn ${rt.value?.blackout ? "danger" : ""}`} onClick={() => send({ type: "blackout", on: !rt.value?.blackout })}>BLACKOUT</button>
      </header>
      <main class="grow" style={{ minHeight: 0, overflow: "hidden" }}><Cur /></main>
      <Toasts />
    </div>
  );
}
