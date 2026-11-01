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
import { Icon, ICONS } from "./ui/Icon";
import { NewWindowButton, WorkspaceGrid } from "./ui/NewWindow";
import { closeThisWindow, initWindowSync, initialWorkspace, isDesktopApp, isSecondary, moveToNextMonitor, saveWorkspace, toggleFullscreen, winId } from "./lib/windows";
import "./styles/windows.css";
import { AiPanel, QuickAsk } from "./ai/Panel";
import { AiButton, HelpButton } from "./ai/Buttons";
import { aiWorkspace, nav, toggleQuick } from "./ai/state";
import { setSettingsCat } from "./workspaces/Settings";
import { HelpCenter } from "./help/HelpCenter";
import { closeHelp, helpPage, openHelp } from "./help/state";
import { BackupsDialog, CsvImportDialog, MvrDialog, ReportsDialog } from "./workspaces/exchange/DataDialogs";

export const current = signal(initialWorkspace());
export function goto(id: string) {
  current.value = id;
  saveWorkspace(id);
}

/** Assistant plumbing shared by both shells: current workspace for its context, and its navigation requests. */
function useAiBridge() {
  useEffect(() => { aiWorkspace.value = current.value; }, [current.value]);
  const n = nav.value;
  useEffect(() => {
    if (!n) return;
    goto(n.ws);
    if (n.cat) setSettingsCat(n.cat);
    nav.value = null;
  }, [n]);
}


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
    <span class="ubars hide-lg" aria-hidden="true">
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

function MainApp() {
  useAiBridge();
  const bo = rt.value?.blackout ?? false;

  useEffect(() => { autoInitMidi(); initWindowSync(); }, []);
  useEffect(() => {
    const ids = (show.value?.universes ?? []).map((u) => u.id).sort((a, b) => a - b).slice(0, 12);
    requestDmx("shell", ids);
    return () => releaseDmx("shell");
  }, [(show.value?.universes ?? []).length]);

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "b") { e.preventDefault(); void send({ type: "blackout", on: !rt.value?.blackout }); return; }
      if (mod && !e.altKey && e.key.toLowerCase() === "k") { e.preventDefault(); toggleQuick(); return; }
      if (mod && e.key.toLowerCase() === "s") { e.preventDefault(); void send({ type: "show_save", name: null }).then(() => toast(t("Show saved"), "ok", 1500)).catch(() => {}); return; }
      if ((e.target as HTMLElement)?.matches?.("input,textarea,select")) return;
      if (mod && !e.altKey && e.key.toLowerCase() === "z") { e.preventDefault(); void send({ type: e.shiftKey ? "redo" : "undo" }).catch(() => {}); return; }
      if (mod && !e.altKey && e.key.toLowerCase() === "y") { e.preventDefault(); void send({ type: "redo" }).catch(() => {}); return; }
      if (e.key === "/" && !mod && !e.altKey) { e.preventDefault(); setCmdOpen(true); return; }
      if (e.key === "F1") { e.preventDefault(); if (helpPage.value) closeHelp(); else openHelp(); return; }
      if (e.key === "?") { if (helpPage.value) closeHelp(); else openHelp(); return; }
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
        <NewWindowButton />
        <span class="sp" />
        <ProgChips />
        <CmdChip />
        <HelpButton />
        <AiButton />
        <span class="tc hide-narrow" title={t("Show timecode")} style={{ color: tc?.running ? "var(--amber)" : "var(--text)" }}>{tc?.text ?? "00:00:00:00"}</span>
        <UBars />
        <StatusChip />
        <button class="btn icon ghost" aria-label={t("Help")} title={t("Help (?)")} onClick={() => openHelp()}>?</button>
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
      <HelpCenter />
      <AiPanel />
      <QuickAsk />
      <DialogHost />
      <Toasts />
    </div>
  );
}

function SecondaryApp() {
  const [pick, setPick] = useState(false);
  useAiBridge();
  const bo = rt.value?.blackout ?? false;
  useEffect(() => { initWindowSync(); }, []);
  useEffect(() => {
    // ?fs=1 → ask for fullscreen on the first tap (browsers require a user gesture)
    if (new URLSearchParams(location.search).get("fs") !== "1" || isDesktopApp) return;
    toast(t("Tap to go fullscreen"), "info", 6000);
    const h = () => { void document.documentElement.requestFullscreen?.().catch(() => undefined); };
    window.addEventListener("pointerdown", h, { once: true });
    return () => window.removeEventListener("pointerdown", h);
  }, []);
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "b") { e.preventDefault(); void send({ type: "blackout", on: !rt.value?.blackout }); return; }
      if (mod && !e.altKey && e.key.toLowerCase() === "k") { e.preventDefault(); toggleQuick(); return; }
      if ((e.target as HTMLElement)?.matches?.("input,textarea,select")) return;
      if (mod && !e.altKey && e.key.toLowerCase() === "z") { e.preventDefault(); void send({ type: e.shiftKey ? "redo" : "undo" }).catch(() => {}); return; }
      if (mod && !e.altKey && e.key.toLowerCase() === "y") { e.preventDefault(); void send({ type: "redo" }).catch(() => {}); return; }
      if (e.key === "F11") { e.preventDefault(); toggleFullscreen(); return; }
      if (mod || e.altKey) return;
      const w = workspaces.find((w) => w.key === e.key);
      if (w) goto(w.id);
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);
  const ws = workspaces.find((w) => w.id === current.value) ?? workspaces[0];
  const Cur = ws.component;
  return (
    <div class="col" style={{ height: "100%", gap: 0 }}>
      {!connected.value && <div class="banner" role="alert">{t("Connection to the Lumos engine lost — reconnecting…")}</div>}
      <header class="topbar sec-bar">
        <svg viewBox="0 0 32 32" width="30" height="30" aria-hidden="true"><polygon points="16,2 29,9 29,23 16,30 3,23 3,9" fill="var(--amber)" /><polygon points="16,10 22,13.5 22,20.5 16,24 10,20.5 10,13.5" fill="var(--ink-1)" /></svg>
        <button class="btn lg ws-pick" aria-haspopup="dialog" title={t("Switch workspace")} onClick={() => setPick(true)}>
          <span class="row" style={{ gap: 10 }}><Icon d={ICONS[ws.id] ?? ICONS.patch} size={22} />{ws.label}</span>
          <Icon d={ICONS.chevron} size={16} />
        </button>
        <span class="win-chip">{t("Window {n}", { n: winId ?? "" })}</span>
        <NewWindowButton compact />
        <HelpButton />
        <AiButton />
        <span class="sp" />
        <GrandMaster />
        <button class={`btn-blackout ${bo ? "active" : ""}`} aria-pressed={bo} onClick={() => send({ type: "blackout", on: !bo })} title={t("Blackout (Ctrl+B)")}>BLACKOUT</button>
        {isDesktopApp && <button class="btn lg icon" aria-label={t("Move to next monitor")} title={t("Move to next monitor")} onClick={moveToNextMonitor}><Icon d="M4 12h14M13 6l6 6-6 6" size={20} /></button>}
        <button class="btn lg icon" aria-label={t("Fullscreen")} title={`${t("Fullscreen")} (F11)`} onClick={toggleFullscreen}><Icon d={ICONS.fullscreen} size={20} /></button>
        <button class="btn lg icon" aria-label={t("Close window")} title={t("Close window")} onClick={closeThisWindow}><Icon d={ICONS.close} size={20} /></button>
      </header>
      <TestBanner />
      <BlindBanner />
      <main class="grow" style={{ minHeight: 0, overflow: "hidden" }}><Cur /></main>
      <AiPanel />
      <QuickAsk />
      {pick && (
        <Modal title={t("Switch workspace")} onClose={() => setPick(false)} width={620}>
          <WorkspaceGrid value={ws.id} onPick={(id) => { goto(id); setPick(false); }} />
        </Modal>
      )}
      <DialogHost />
      <Toasts />
    </div>
  );
}

export function App() {
  return isSecondary ? <SecondaryApp /> : <MainApp />;
}
