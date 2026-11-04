import { useEffect, useRef, useState } from "preact/hooks";
import { rt } from "../store";
import { helpQuery, logEntries, logErrors, logOpen, type LogEntry } from "../lib/log";
import { openHelp } from "../help/state";
import { t } from "../i18n";

type Tab = "all" | "err" | "scripts" | "inputs";
const clock = (ms: number) => new Date(ms).toLocaleTimeString([], { hour12: false });

/** Live log: everything the interface and the engine report, with a button to the manual for each problem. */
export function LogWindow() {
  const [tab, setTab] = useState<Tab>("all");
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const drag = useRef<{ dx: number; dy: number } | null>(null);
  const [follow, setFollow] = useState(true);

  const base = Date.now() - (rt.value ? performance.now() : 0);
  const luaLines: LogEntry[] = (rt.value?.lua?.log ?? []).map((l, i) => ({ id: -1000 - i, t: base + l.t * 1000, level: l.ok ? "info" : "err", source: `lua · ${l.script}`, msg: l.text }));
  const inputLines: LogEntry[] = (rt.value?.inputs?.recent ?? []).slice().reverse().map((m, i) => ({ id: -5000 - i, t: Date.now() - (rt.value?.inputs?.recent.length ?? 0) * 0 - i, level: "info" as const, source: "entrada", msg: m }));
  const mine = logEntries.value;
  const all = [...mine, ...luaLines].sort((a, b) => a.t - b.t);
  const shown = tab === "err" ? all.filter((e) => e.level === "err") : tab === "scripts" ? luaLines : tab === "inputs" ? inputLines : all;
  useEffect(() => { if (follow && list.current) list.current.scrollTop = list.current.scrollHeight; }, [shown.length, tab, follow]);
  useEffect(() => { if (logOpen.value) logErrors.value = 0; }, [logOpen.value, mine.length]);
  if (!logOpen.value) return null;

  const copy = () => {
    const txt = shown.map((e) => `${clock(e.t)} [${e.level}] ${e.source}: ${e.msg}`).join("\n");
    void navigator.clipboard?.writeText(txt);
  };
  const tabBtn = (id: Tab, label: string) => <button role="tab" aria-selected={tab === id} class={`btn sm ${tab === id ? "sel" : ""}`} onClick={() => setTab(id)}>{label}</button>;
  const style = pos ? { left: pos.x, top: pos.y, right: "auto", bottom: "auto" } : undefined;
  return (
    <div ref={box} class="logwin" style={style} role="dialog" aria-label={t("Log")}>
      <div class="logwin-head"
        onPointerDown={(e) => { const r = box.current!.getBoundingClientRect(); drag.current = { dx: e.clientX - r.left, dy: e.clientY - r.top }; (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); }}
        onPointerMove={(e) => { if (drag.current) setPos({ x: Math.max(0, e.clientX - drag.current.dx), y: Math.max(0, e.clientY - drag.current.dy) }); }}
        onPointerUp={() => { drag.current = null; }}>
        <b>{t("Log")}</b>
        <span class="faint">{t("live")}</span>
        <span class="sp" />
        <button class="btn sm" onPointerDown={(e) => e.stopPropagation()} onClick={copy} title={t("Copy what is shown, to paste in a bug report")}>{t("Copy")}</button>
        <button class="btn sm" onPointerDown={(e) => e.stopPropagation()} onClick={() => { logEntries.value = []; }}>{t("Clear")}</button>
        <button class="btn sm" onPointerDown={(e) => e.stopPropagation()} onClick={() => { logOpen.value = false; }} aria-label={t("Close")}>✕</button>
      </div>
      <div class="logwin-tabs" role="tablist">
        {tabBtn("all", t("All"))}{tabBtn("err", t("Errors"))}{tabBtn("scripts", "Lua")}{tabBtn("inputs", t("Inputs"))}
        <span class="sp" />
        <label class="faint" style={{ display: "flex", gap: 4, alignItems: "center" }}><input type="checkbox" checked={follow} onChange={(e) => setFollow((e.target as HTMLInputElement).checked)} />{t("Follow")}</label>
      </div>
      <div class="logwin-list mono" ref={list}>
        {shown.length === 0 && <div class="faint" style={{ padding: 10 }}>{tab === "err" ? t("No errors. Anything that goes wrong shows up here.") : t("Nothing yet. Messages appear here as they happen.")}</div>}
        {shown.map((e) => (
          <div key={e.id} class={`logline ${e.level}`}>
            <span class="faint">{clock(e.t)}</span>
            <span class="src">{e.source}</span>
            <span class="msg">{e.msg}</span>
            {e.level === "err" && <button class="btn sm" onClick={() => openHelp(`search:${helpQuery(e.msg + " " + e.source)}`)} title={t("Open the manual on this")}>?</button>}
          </div>
        ))}
      </div>
    </div>
  );
}
