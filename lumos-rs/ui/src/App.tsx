import { signal } from "@preact/signals";
import { useEffect, useRef, useState } from "preact/hooks";
import { connected, dmx, dmxVersion, fire, rt, send, show, shows, toast } from "./store";
import { workspaces } from "./workspaces";
import { BlindBanner, DialogHost, Kbd, Modal, ProgChips, Toasts, confirmDialog, promptDialog } from "./ui";
import { autoInitMidi } from "./midi";
import { CmdChip, CommandBar } from "./auto/CommandBar";
import { setCmdOpen } from "./auto/state";
import { downloadShow, importShow } from "./lib/api";
import { releaseDmx, requestDmx } from "./lib/dmxsub";
import { t, tp } from "./i18n";
import { tErr } from "./i18n/errors";
import { TestBanner } from "./workspaces/settings/OutputTest";
import { BackupsDialog, CsvImportDialog, MvrDialog, ReportsDialog } from "./workspaces/exchange/DataDialogs";

export const current = signal(localStorage.getItem("ws") ?? "live");
export function goto(id: string) {
  current.value = id;
  localStorage.setItem("ws", id);
}

const SHORTCUTS: [string, string][] = [
  ["1 … 7", "Switch workspace"],
  ["Ctrl/⌘ + B", "Toggle blackout"],
  ["Ctrl/⌘ + S", "Save show"],
  ["Ctrl/⌘ + Z", "Undo"],
  ["Ctrl/⌘ + Y  ·  Ctrl/⌘ + Shift + Z", "Redo"],
  ["/", "Command line"],
  ["?", "This help"],
  ["Esc", "Close dialogs"],
];

function ShowMenu() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const file = useRef<HTMLInputElement>(null);
  const [dataDlg, setDataDlg] = useState<"reports" | "csv" | "mvr" | "backups" | null>(null);
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
      <button class="btn" style={{ fontSize: "var(--fs-md)" }} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span class="faint" style={{ fontWeight: 500 }}>{t("Show")}</span> <b style={{ maxWidth: 220, overflow: "hidden", textOverflow: "ellipsis" }}>{name}</b>
        <Icon d="M6 9l6 6 6-6" size={16} />
      </button>
      {open && (
        <div class="menu" role="menu">
          <button role="menuitem" onClick={run(async () => {
            if (!(await confirmDialog(t("New show"), t("Discard the current show and start a new empty one?"), { danger: true, ok: t("New show") }))) return;
            const n = await promptDialog(t("New show"), t("Name"), t("Untitled"));
            if (n) await send({ type: "show_new", name: n }).catch(() => {});
          })}>{t("New…")}</button>
          <button role="menuitem" onClick={run(() => send({ type: "show_save", name: null }).then(() => toast(t("Show saved"), "ok", 1500)).catch(() => {}))}>{t("Save")} <span class="faint" style={{ float: "right" }}>Ctrl+S</span></button>
          <button role="menuitem" onClick={run(async () => {
            const n = await promptDialog(t("Save as"), t("Name"), name);
            if (n) await send({ type: "show_save", name: n }).then(() => toast(t("Saved “{n}”", { n }), "ok", 1500)).catch(() => {});
          })}>{t("Save as…")}</button>
          <button role="menuitem" onClick={run(async () => {
            const n = await promptDialog(t("Rename show"), t("Name"), name);
            if (n) await send({ type: "show_rename", name: n }).catch(() => {});
          })}>{t("Rename…")}</button>
          <hr />
          <div class="menu-label">{t("Load")}</div>
          {shows.value.length === 0 && <div class="faint" style={{ padding: "6px 12px" }}>{t("No saved shows")}</div>}
          <div style={{ maxHeight: 220, overflow: "auto", display: "flex", flexDirection: "column" }}>
            {shows.value.map((s) => (
              <button key={s.name} role="menuitem" onClick={run(async () => {
                if (await confirmDialog(t("Load show"), t("Load “{name}”? Unsaved changes in the current show are lost.", { name: s.name }), { ok: t("Load") }))
                  await send({ type: "show_load", name: s.name }).catch(() => {});
              })}>{s.name}</button>
            ))}
          </div>
          <hr />
          <button role="menuitem" onClick={run(() => downloadShow(name))}>{t("Download JSON")}</button>
          <button role="menuitem" onClick={() => file.current?.click()}>{t("Import JSON…")}</button>
          <hr />
          <div class="menu-label">{t("Data")}</div>
          <button role="menuitem" style={{ minHeight: 44 }} onClick={run(() => setDataDlg("reports"))}>{t("Reports…")}</button>
          <button role="menuitem" style={{ minHeight: 44 }} onClick={run(() => setDataDlg("csv"))}>{t("Patch CSV…")}</button>
          <button role="menuitem" style={{ minHeight: 44 }} onClick={run(() => setDataDlg("mvr"))}>{t("MVR…")}</button>
          <button role="menuitem" style={{ minHeight: 44 }} onClick={run(() => setDataDlg("backups"))}>{t("Backups…")}</button>
        </div>
      )}
      {dataDlg === "reports" && <ReportsDialog onClose={() => setDataDlg(null)} />}
      {dataDlg === "csv" && <CsvImportDialog onClose={() => setDataDlg(null)} />}
      {dataDlg === "mvr" && <MvrDialog onClose={() => setDataDlg(null)} />}
      {dataDlg === "backups" && <BackupsDialog onClose={() => setDataDlg(null)} />}
      <input ref={file} type="file" accept=".json,application/json" hidden onChange={async (e) => {
        const f = (e.target as HTMLInputElement).files?.[0];
        (e.target as HTMLInputElement).value = "";
        setOpen(false);
        if (!f) return;
        try { await importShow(f); toast(t("Show imported"), "ok"); } catch (err) { toast(tErr((err as Error).message), "err"); }
      }} />
    </div>
  );
}

const ICONS: Record<string, string> = {
  live: "M13 2 4 14h7l-1 8 9-12h-7l1-8z",
  desk: "M6 3v18M12 3v18M18 3v18M4 8h4M10 15h4M16 6h4",
  cues: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
  patch: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
  stage: "M12 2 3 7v10l9 5 9-5V7l-9-5zM3 7l9 5 9-5M12 12v10",
  plan: "M3 6l6-2 6 2 6-2v14l-6 2-6-2-6 2zM9 4v14M15 6v14",
  monitor: "M3 4h18v12H3zM8 20h8M12 16v4M7 12l3-3 3 2 4-4",
  vc: "M3 5h18v14H3zM7 9h2M11 9h2M15 9h2M7 13h2M11 13h2M15 13h2",
  timeline: "M3 12h18M7 8v8M12 5v14M17 9v6",
  settings: "M12 15a3 3 0 100-6 3 3 0 000 6zM4 12h2M18 12h2M12 4v2M12 18v2M6.3 6.3l1.4 1.4M16.3 16.3l1.4 1.4M17.7 6.3l-1.4 1.4M7.7 16.3l-1.4 1.4",
};

function Icon({ d, size = 26 }: { d: string; size?: number }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d={d} />
    </svg>
  );
}

function GrandMaster() {
  const v = rt.value?.grand_master ?? 1;
  const track = useRef<HTMLDivElement>(null);
  const set = (clientX: number) => {
    const r = track.current?.getBoundingClientRect();
    if (!r || r.width === 0) return;
    fire({ type: "grand_master", level: Math.min(1, Math.max(0, (clientX - r.left) / r.width)) });
  };
  const key = (e: KeyboardEvent) => {
    const step = e.shiftKey ? 0.1 : 0.02;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") { e.preventDefault(); fire({ type: "grand_master", level: Math.min(1, v + step) }); }
    if (e.key === "ArrowLeft" || e.key === "ArrowDown") { e.preventDefault(); fire({ type: "grand_master", level: Math.max(0, v - step) }); }
  };
  const pct = Math.round(v * 100);
  return (
    <div class="dock-master" title={t("Grand master")}>
      <div class="lbl"><span>{t("GRAND MASTER")}</span><b>{pct}%</b></div>
      <div ref={track} class="fader-h" role="slider" tabIndex={0} aria-label={t("Grand master")} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}
        onPointerDown={(e) => { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); set(e.clientX); }}
        onPointerMove={(e) => { if ((e.currentTarget as HTMLElement).hasPointerCapture(e.pointerId)) set(e.clientX); }}
        onKeyDown={key}>
        <div class="fill" style={{ width: `${pct}%` }} />
        <div class="cap" style={{ left: `${pct}%` }} />
      </div>
    </div>
  );
}

function UBars() {
  const [bars, setBars] = useState<{ u: number; max: number }[]>([]);
  useEffect(() => {
    const id = setInterval(() => {
      const out: { u: number; max: number }[] = [];
      for (const [u, d] of dmx) {
        let m = 0;
        for (let i = 0; i < 512; i++) if (d[i] > m) m = d[i];
        out.push({ u, max: m });
      }
      out.sort((a, b) => a.u - b.u);
      setBars(out.slice(0, 4));
    }, 250);
    return () => clearInterval(id);
  }, []);
  void dmxVersion.value;
  return (
    <span class="ubars hide-mid" aria-hidden="true">
      {bars.map((b) => (
        <span class="ubar" key={b.u} title={t("Universe {u}: peak {max}", { u: b.u, max: b.max })}>
          <span class="mono">U{b.u}</span><span class="bar"><i style={{ width: `${(b.max / 255) * 100}%` }} /></span>
        </span>
      ))}
    </span>
  );
}

function StatusChip() {
  const r = rt.value;
  const errs = r?.outputs.filter((o) => o.state === "error").length ?? 0;
  const oks = r?.outputs.filter((o) => o.state === "ok").length ?? 0;
  const last = r?.inputs.recent[0];
  const on = connected.value;
  const label = !on ? t("offline") : errs ? tp(errs, "{n} output error", "{n} output errors") : tp(oks, "{n} output OK", "{n} outputs OK");
  return (
    <span class={`status-chip ${errs || !on ? "err" : ""}`} role="status" title={`${on ? t("Engine running at {fps} fps", { fps: r?.fps.toFixed(1) ?? "?" }) : t("Disconnected")}\n${t("IN:")} ${last ?? t("idle")}`}>
      <span class={`dot ${on ? (errs ? "warn" : "ok") : "err"}`} />
      <span>{label}</span>
      {on && <span class="mono dim">{t("{fps} fps", { fps: r?.fps.toFixed(0) ?? "–" })}</span>}
    </span>
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
      if (mod && e.key.toLowerCase() === "s") { e.preventDefault(); void send({ type: "show_save", name: null }).then(() => toast(t("Show saved"), "ok", 1500)).catch(() => {}); return; }
      if ((e.target as HTMLElement)?.matches?.("input,textarea,select")) return;
      if (mod && !e.altKey && e.key.toLowerCase() === "z") { e.preventDefault(); void send({ type: e.shiftKey ? "redo" : "undo" }).catch(() => {}); return; }
      if (mod && !e.altKey && e.key.toLowerCase() === "y") { e.preventDefault(); void send({ type: "redo" }).catch(() => {}); return; }
      if (e.key === "/" && !mod && !e.altKey) { e.preventDefault(); setCmdOpen(true); return; }
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
  const navRef = useRef<HTMLElement>(null);
  useEffect(() => {
    navRef.current?.querySelector<HTMLElement>(".dock-btn.active")?.scrollIntoView({ inline: "nearest", block: "nearest" });
  }, [ws.id]);
  return (
    <div class="col" style={{ height: "100%", gap: 0 }}>
      {!connected.value && <div class="banner" role="alert">{t("Connection to the Lumos engine lost — reconnecting…")}</div>}
      <header class="topbar">
        <div class="logo">
          <svg viewBox="0 0 32 32" width="30" height="30" aria-hidden="true"><polygon points="16,2 29,9 29,23 16,30 3,23 3,9" fill="var(--amber)" /><polygon points="16,10 22,13.5 22,20.5 16,24 10,20.5 10,13.5" fill="var(--ink-1)" /></svg>
          <span class="hide-narrow">LUMOS</span>
        </div>
        <ShowMenu />
        <span class="sp" />
        <ProgChips />
        <CmdChip />
        <span class="tc hide-narrow" title={t("Show timecode")} style={{ color: tc?.running ? "var(--amber)" : "var(--text)" }}>{tc?.text ?? "00:00:00:00"}</span>
        <UBars />
        <StatusChip />
        <button class="btn icon ghost hide-narrow" aria-label={t("Keyboard shortcuts")} onClick={() => setHelp(true)}>?</button>
      </header>
      <TestBanner />
      <BlindBanner />
      <main class="grow" style={{ minHeight: 0, overflow: "hidden" }}><Cur /></main>
      <CommandBar />
      <footer class="dock">
        <nav class="dock-nav" ref={navRef} role="tablist" aria-label={t("Workspaces")}>
          {workspaces.map((w) => (
            <button key={w.id} role="tab" aria-selected={w.id === ws.id} class={`dock-btn ${w.id === ws.id ? "active" : ""}`} onClick={() => goto(w.id)} title={`${w.label} (${w.key})`}>
              <Icon d={ICONS[w.id] ?? ICONS.patch} />
              <span>{w.label}</span>
            </button>
          ))}
        </nav>
        <GrandMaster />
        <button class={`btn-blackout ${bo ? "active" : ""}`} aria-pressed={bo} onClick={() => send({ type: "blackout", on: !bo })} title={t("Blackout (Ctrl+B)")}>BLACKOUT</button>
      </footer>
      {help && (
        <Modal title={t("Keyboard shortcuts")} onClose={() => setHelp(false)}>
          <table class="tbl"><tbody>
            {SHORTCUTS.map(([k, d]) => <tr key={k}><td><Kbd>{k}</Kbd></td><td>{t(d)}</td></tr>)}
            {workspaces.map((w) => <tr key={w.id}><td><Kbd>{w.key}</Kbd></td><td>{w.label}</td></tr>)}
          </tbody></table>
        </Modal>
      )}
      <DialogHost />
      <Toasts />
    </div>
  );
}
