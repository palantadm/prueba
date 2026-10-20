import { signal } from "@preact/signals";
import { useEffect, useRef, useState } from "preact/hooks";
import { connected, dmx, dmxVersion, fire, rt, send, show, shows, toast } from "./store";
import { workspaces } from "./workspaces";
import { DialogHost, Kbd, Modal, Toasts, confirmDialog, promptDialog } from "./ui";
import { autoInitMidi } from "./midi";
import { downloadShow, importShow } from "./lib/api";
import { releaseDmx, requestDmx } from "./lib/dmxsub";

export const current = signal(localStorage.getItem("ws") ?? "live");
export function goto(id: string) {
  current.value = id;
  localStorage.setItem("ws", id);
}

const SHORTCUTS: [string, string][] = [
  ["1 … 7", "Switch workspace"],
  ["Ctrl/⌘ + B", "Toggle blackout"],
  ["Ctrl/⌘ + S", "Save show"],
  ["?", "This help"],
  ["Esc", "Close dialogs"],
];

function ShowMenu() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const file = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!open) return;
    const h = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    window.addEventListener("mousedown", h);
    return () => window.removeEventListener("mousedown", h);
  }, [open]);
  const run = (f: () => void | Promise<void>) => () => { setOpen(false); void f(); };
  const name = show.value?.name ?? "—";
  return (
    <div ref={ref} style={{ position: "relative" }}>
      <button class="btn tall" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span class="dim">Show</span> <b style={{ maxWidth: 160, overflow: "hidden", textOverflow: "ellipsis" }}>{name}</b> ▾
      </button>
      {open && (
        <div class="menu" role="menu">
          <button role="menuitem" onClick={run(async () => {
            if (!(await confirmDialog("New show", "Discard the current show and start a new empty one?", { danger: true, ok: "New show" }))) return;
            const n = await promptDialog("New show", "Name", "Untitled");
            if (n) await send({ type: "show_new", name: n }).catch(() => {});
          })}>New…</button>
          <button role="menuitem" onClick={run(() => send({ type: "show_save", name: null }).then(() => toast("Show saved", "ok", 1500)).catch(() => {}))}>Save <span class="faint" style={{ float: "right" }}>Ctrl+S</span></button>
          <button role="menuitem" onClick={run(async () => {
            const n = await promptDialog("Save as", "Name", name);
            if (n) await send({ type: "show_save", name: n }).then(() => toast(`Saved “${n}”`, "ok", 1500)).catch(() => {});
          })}>Save as…</button>
          <button role="menuitem" onClick={run(async () => {
            const n = await promptDialog("Rename show", "Name", name);
            if (n) await send({ type: "show_rename", name: n }).catch(() => {});
          })}>Rename…</button>
          <hr />
          <div class="menu-label">Load</div>
          {shows.value.length === 0 && <div class="faint" style={{ padding: "6px 12px" }}>No saved shows</div>}
          <div style={{ maxHeight: 220, overflow: "auto", display: "flex", flexDirection: "column" }}>
            {shows.value.map((s) => (
              <button key={s.name} role="menuitem" onClick={run(async () => {
                if (await confirmDialog("Load show", `Load “${s.name}”? Unsaved changes in the current show are lost.`, { ok: "Load" }))
                  await send({ type: "show_load", name: s.name }).catch(() => {});
              })}>{s.name}</button>
            ))}
          </div>
          <hr />
          <button role="menuitem" onClick={run(() => downloadShow(name))}>Download JSON</button>
          <button role="menuitem" onClick={() => file.current?.click()}>Import JSON…</button>
        </div>
      )}
      <input ref={file} type="file" accept=".json,application/json" hidden onChange={async (e) => {
        const f = (e.target as HTMLInputElement).files?.[0];
        (e.target as HTMLInputElement).value = "";
        setOpen(false);
        if (!f) return;
        try { await importShow(f); toast("Show imported", "ok"); } catch (err) { toast((err as Error).message, "err"); }
      }} />
    </div>
  );
}

function GrandMaster() {
  const v = rt.value?.grand_master ?? 1;
  return (
    <div class="gm" title="Grand master">
      <span class="label">GM</span>
      <input type="range" min={0} max={1000} value={Math.round(v * 1000)} aria-label="Grand master"
        onInput={(e) => fire({ type: "grand_master", level: Number((e.target as HTMLInputElement).value) / 1000 })} />
      <span class="mono" style={{ width: 38, textAlign: "right" }}>{Math.round(v * 100)}%</span>
    </div>
  );
}

function StatusBar() {
  const [bars, setBars] = useState<{ u: number; max: number }[]>([]);
  useEffect(() => {
    const t = setInterval(() => {
      const out: { u: number; max: number }[] = [];
      for (const [u, d] of dmx) {
        let m = 0;
        for (let i = 0; i < 512; i++) if (d[i] > m) m = d[i];
        out.push({ u, max: m });
      }
      out.sort((a, b) => a.u - b.u);
      setBars(out.slice(0, 12));
    }, 250);
    return () => clearInterval(t);
  }, []);
  void dmxVersion.value;
  const r = rt.value;
  const errs = r?.outputs.filter((o) => o.state === "error").length ?? 0;
  const oks = r?.outputs.filter((o) => o.state === "ok").length ?? 0;
  const last = r?.inputs.recent[0];
  return (
    <footer class="statusbar" role="status">
      {bars.map((b) => (
        <span class="ubar" key={b.u} title={`Universe ${b.u}: peak ${b.max}`}>
          <span class="mono">U{b.u}</span><span class="bar"><i style={{ width: `${(b.max / 255) * 100}%` }} /></span>
        </span>
      ))}
      <span class="sp" />
      <span class={errs ? "chip err" : "chip"} style={{ height: 18 }}>{errs ? `${errs} output error${errs > 1 ? "s" : ""}` : `${oks} output${oks === 1 ? "" : "s"} OK`}</span>
      <span title="Last input event" style={{ maxWidth: 260, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
        IN: {last ?? <span class="faint">idle</span>}
      </span>
    </footer>
  );
}

export function App() {
  const [help, setHelp] = useState(false);
  const bo = rt.value?.blackout ?? false;

  useEffect(() => { autoInitMidi(); }, []);
  useEffect(() => {
    const ids = (show.value?.universes ?? []).map((u) => u.id).sort((a, b) => a - b).slice(0, 12);
    requestDmx("shell", ids);
    return () => releaseDmx("shell");
  }, [(show.value?.universes ?? []).length]);

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "b") { e.preventDefault(); void send({ type: "blackout", on: !rt.value?.blackout }); return; }
      if (mod && e.key.toLowerCase() === "s") { e.preventDefault(); void send({ type: "show_save", name: null }).then(() => toast("Show saved", "ok", 1500)).catch(() => {}); return; }
      if ((e.target as HTMLElement)?.matches?.("input,textarea,select")) return;
      if (e.key === "?") { setHelp((v) => !v); return; }
      if (mod || e.altKey) return;
      const w = workspaces.find((w) => w.key === e.key);
      if (w) goto(w.id);
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const ws = workspaces.find((w) => w.id === current.value) ?? workspaces[0];
  const Cur = ws.component;
  const tc = rt.value?.timecode;
  return (
    <div class="col" style={{ height: "100%", gap: 0 }}>
      {!connected.value && <div class="banner" role="alert">Connection to the Lumos engine lost — reconnecting…</div>}
      <header class="app-head">
        <b style={{ letterSpacing: ".2em", color: "var(--live)", whiteSpace: "nowrap" }}>⬡ LUMOS</b>
        <nav class="app-tabs" role="tablist" aria-label="Workspaces">
          {workspaces.map((w) => (
            <button key={w.id} role="tab" aria-selected={w.id === ws.id} class={`btn tall ghost ${w.id === ws.id ? "sel" : ""}`} onClick={() => goto(w.id)} title={`${w.label} (${w.key})`}>
              {w.label}
            </button>
          ))}
        </nav>
        <span class="sp" />
        <ShowMenu />
        <GrandMaster />
        <span class="hdr-chip mono" title="Show timecode" style={{ color: tc?.running ? "var(--live)" : "var(--text-dim)" }}>{tc?.text ?? "00:00:00:00"}</span>
        <span class="hdr-chip" title={connected.value ? `Engine running at ${rt.value?.fps.toFixed(1) ?? "?"} fps` : "Disconnected"}>
          <span class={`dot ${connected.value ? "ok" : "err"}`} />
          <span class="mono">{connected.value ? `${rt.value?.fps.toFixed(0) ?? "–"} fps` : "offline"}</span>
        </span>
        <button class={`btn tall ${bo ? "blink" : "danger"}`} aria-pressed={bo} onClick={() => send({ type: "blackout", on: !bo })} title="Blackout (Ctrl+B)">BLACKOUT</button>
        <button class="btn tall ghost icon hide-mid" aria-label="Keyboard shortcuts" onClick={() => setHelp(true)}>?</button>
      </header>
      <main class="grow" style={{ minHeight: 0, overflow: "hidden" }}><Cur /></main>
      <StatusBar />
      {help && (
        <Modal title="Keyboard shortcuts" onClose={() => setHelp(false)}>
          <table class="tbl"><tbody>
            {SHORTCUTS.map(([k, d]) => <tr key={k}><td><Kbd>{k}</Kbd></td><td>{d}</td></tr>)}
            {workspaces.map((w) => <tr key={w.id}><td><Kbd>{w.key}</Kbd></td><td>{w.label}</td></tr>)}
          </tbody></table>
        </Modal>
      )}
      <DialogHost />
      <Toasts />
    </div>
  );
}
