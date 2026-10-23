import { useMemo, useRef, useState } from "preact/hooks";
import { rt, selection, show } from "../../store";
import type { Fixture } from "../../types";
import { ALL_ATTRS, attrColor, attrLabel, fixtureAttrs, fixtureById, hasAttr, readAttr } from "../../lib/attrs";
import { releaseAttrs, setAttrs } from "../../lib/program";
import { useDmxTick } from "../../lib/hooks";
import { Modal } from "../../ui";
import { FanDialog } from "./FanDialog";
import { t } from "../../i18n";

type Unit = "pct" | "dmx";
type Cell = { r: number; c: number };

const fmt = (v: number, u: Unit) => (u === "pct" ? String(Math.round(v * 100)) : String(Math.round(v * 255)));
const parse = (s: string, u: Unit): number | null => {
  const n = parseFloat(s.replace(",", ".").replace("%", "").trim());
  if (!Number.isFinite(n)) return null;
  return Math.max(0, Math.min(1, u === "pct" ? n / 100 : n / 255));
};

/** Fixture sheet: fixtures × attributes spreadsheet with inline editing, paste and fill. */
export function Sheet() {
  useDmxTick(10);
  const [unit, setUnit] = useState<Unit>("pct");
  const [anchor, setAnchor] = useState<Cell | null>(null);
  const [colMenu, setColMenu] = useState<string | null>(null);
  const [fan, setFan] = useState<string | null>(null);
  const drag = useRef<{ c: number; v: number } | null>(null);

  const all = show.value?.fixtures ?? [];
  const scoped = selection.value.length > 0;
  const rows: Fixture[] = useMemo(
    () => (scoped ? selection.value.map((i) => fixtureById(i)).filter((f): f is Fixture => !!f) : [...all].sort((a, b) => a.id - b.id)),
    [scoped, selection.value.join(","), all],
  );
  const cols = useMemo(() => {
    const s = new Set<string>();
    for (const f of rows) for (const a of fixtureAttrs(f)) s.add(a);
    const known = ALL_ATTRS.filter((a) => s.has(a));
    return [...known, ...[...s].filter((a) => !ALL_ATTRS.includes(a)).sort()];
  }, [rows]);

  const held = (f: Fixture, a: string) => rt.value?.programmer[f.id]?.[a];
  const valueOf = (f: Fixture, a: string): number | undefined => held(f, a) ?? readAttr(f, a);
  const setCell = (r: number, c: number, v: number) => {
    const f = rows[r], a = cols[c];
    if (f && a && hasAttr(f, a)) setAttrs([f.id], { [a]: v });
  };

  const paste = (e: ClipboardEvent, r0: number, c0: number) => {
    const text = e.clipboardData?.getData("text") ?? "";
    if (!/[\t\n]/.test(text) && rows.length === 0) return;
    const grid = text.replace(/\r/g, "").replace(/\n$/, "").split("\n").map((l) => l.split("\t"));
    if (grid.length === 1 && grid[0].length === 1) return; // single value: normal input paste
    e.preventDefault();
    grid.forEach((line, i) => line.forEach((cell, j) => { const v = parse(cell, unit); if (v !== null) setCell(r0 + i, c0 + j, v); }));
  };

  const fillDown = () => {
    if (!anchor) return;
    const src = rows[anchor.r], a = cols[anchor.c];
    const v = src && a ? valueOf(src, a) : undefined;
    if (v === undefined) return;
    for (let r = anchor.r + 1; r < rows.length; r++) setCell(r, anchor.c, v);
  };
  const startFill = (e: PointerEvent, r: number, c: number) => {
    const v = valueOf(rows[r], cols[c]);
    if (v === undefined) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag.current = { c, v };
  };
  const moveFill = (e: PointerEvent) => {
    if (!drag.current) return;
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest("[data-r]") as HTMLElement | null;
    if (el && Number(el.dataset.c) === drag.current.c) setCell(Number(el.dataset.r), drag.current.c, drag.current.v);
  };

  const colIds = (a: string) => rows.filter((f) => hasAttr(f, a)).map((f) => f.id);
  const menuAttr = colMenu;
  return (
    <div class="panel lv-col">
      <div class="panel-head">
        <span>{t("Fixture sheet")}</span><span class="sp" />
        <span class="chip">{scoped ? t("{n} selected", { n: rows.length }) : t("All fixtures ({n})", { n: rows.length })}</span>
        <button class="btn sm" style={{ minHeight: 44 }} onClick={() => setUnit(unit === "pct" ? "dmx" : "pct")} aria-label={t("Switch units")}>{unit === "pct" ? "%" : "0–255"}</button>
        <button class="btn sm" style={{ minHeight: 44 }} disabled={!anchor} onClick={fillDown} title={t("Copy the active cell down the column")}>{t("Fill ↓")}</button>
      </div>
      <div class="lv-scroll" style={{ padding: 0 }}>
        {rows.length === 0 ? <div class="panel-body dim">{t("No fixtures patched.")}</div> : (
          <table class="tbl lv-sheet" aria-label={t("Fixture sheet")}>
            <thead><tr>
              <th style={{ minWidth: 110 }}>{t("Fixture")}</th>
              {cols.map((a) => (
                <th key={a}><button class="lv-sheet-h" style={{ color: attrColor(a) }} onClick={() => setColMenu(a)} aria-label={t("Column tools: {attr}", { attr: attrLabel(a) })}>{attrLabel(a)} ▾</button></th>
              ))}
            </tr></thead>
            <tbody onPointerMove={moveFill} onPointerUp={() => (drag.current = null)} onPointerCancel={() => (drag.current = null)}>
              {rows.map((f, r) => (
                <tr key={f.id}>
                  <th class="lv-sheet-row" scope="row">
                    <button class="lv-sheet-h" onClick={() => (selection.value = [f.id])} title={t("Select only this fixture")}>
                      <b class="mono">{f.id}</b> {f.name}
                    </button>
                  </th>
                  {cols.map((a, c) => {
                    const ok = hasAttr(f, a);
                    const h = held(f, a);
                    const v = ok ? valueOf(f, a) : undefined;
                    const isA = anchor?.r === r && anchor?.c === c;
                    return (
                      <td key={a} data-r={r} data-c={c} class={`lv-sheet-c ${h !== undefined ? "held" : ""} ${isA ? "act" : ""} ${ok ? "" : "na"}`}>
                        {ok ? (
                          <>
                            <SheetInput value={v === undefined ? "" : fmt(v, unit)} held={h !== undefined} label={`${f.name} ${attrLabel(a)}`}
                              onFocus={() => setAnchor({ r, c })}
                              onPaste={(e) => paste(e, r, c)}
                              onCommit={(s) => { const n = parse(s, unit); if (n !== null) setCell(r, c, n); }}
                              onNav={(dr, dc) => {
                                const nr = Math.max(0, Math.min(rows.length - 1, r + dr)), nc = Math.max(0, Math.min(cols.length - 1, c + dc));
                                (document.querySelector(`td[data-r="${nr}"][data-c="${nc}"] input`) as HTMLInputElement | null)?.focus();
                              }} />
                            {isA && v !== undefined && <span class="lv-fillh" aria-label={t("Drag to fill")} onPointerDown={(e) => startFill(e, r, c)} />}
                          </>
                        ) : <span class="faint">—</span>}
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      {menuAttr && (
        <ColumnMenu attr={menuAttr} unit={unit} ids={colIds(menuAttr)} onClose={() => setColMenu(null)} onFan={() => { setFan(menuAttr); setColMenu(null); }} />
      )}
      {fan && <FanDialog ids={rows.map((f) => f.id)} attrs={cols} initialAttr={fan} onClose={() => setFan(null)} />}
    </div>
  );
}

function SheetInput(p: { value: string; held: boolean; label: string; onCommit: (s: string) => void; onFocus: () => void; onPaste: (e: ClipboardEvent) => void; onNav: (dr: number, dc: number) => void }) {
  const [v, setV] = useState<string | null>(null);
  return (
    <input class="cell num lv-sheet-in" inputMode="decimal" aria-label={p.label} value={v ?? p.value} style={{ color: p.held ? "var(--live)" : undefined }}
      onFocus={(e) => { p.onFocus(); (e.target as HTMLInputElement).select(); }}
      onInput={(e) => setV((e.target as HTMLInputElement).value)}
      onPaste={p.onPaste as unknown as (e: Event) => void}
      onBlur={() => { if (v !== null) p.onCommit(v); setV(null); }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") { (e.target as HTMLInputElement).blur(); p.onNav(1, 0); }
        else if (e.key === "Escape") { setV(null); (e.target as HTMLInputElement).blur(); }
        else if (e.key === "ArrowDown") { e.preventDefault(); (e.target as HTMLInputElement).blur(); p.onNav(1, 0); }
        else if (e.key === "ArrowUp") { e.preventDefault(); (e.target as HTMLInputElement).blur(); p.onNav(-1, 0); }
        else if (e.key === "Tab") { e.preventDefault(); (e.target as HTMLInputElement).blur(); p.onNav(0, e.shiftKey ? -1 : 1); }
      }} />
  );
}

function ColumnMenu(p: { attr: string; unit: Unit; ids: number[]; onClose: () => void; onFan: () => void }) {
  const [val, setVal] = useState("");
  const apply = () => { const n = parse(val, p.unit); if (n !== null) setAttrs(p.ids, { [p.attr]: n }); p.onClose(); };
  return (
    <Modal title={t("Column: {attr}", { attr: attrLabel(p.attr) })} onClose={p.onClose}>
      <div class="field"><label>{t("Set all to ({u})", { u: p.unit === "pct" ? "%" : "0–255" })}</label>
        <div class="row">
          <input class="input num" style={{ minHeight: 44 }} autoFocus inputMode="decimal" value={val} onInput={(e) => setVal((e.target as HTMLInputElement).value)} onKeyDown={(e) => e.key === "Enter" && apply()} />
          <button class="btn primary" style={{ minHeight: 44 }} disabled={parse(val, p.unit) === null} onClick={apply}>{t("Set")}</button>
        </div>
      </div>
      <button class="btn lg" style={{ minHeight: 44 }} onClick={p.onFan}>{t("Fan across fixtures…")}</button>
      <button class="btn lg danger" style={{ minHeight: 44 }} onClick={() => { releaseAttrs(p.ids, [p.attr]); p.onClose(); }}>{t("Release column")}</button>
    </Modal>
  );
}
