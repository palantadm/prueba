import { useEffect, useRef, useState } from "preact/hooks";
import { signal } from "@preact/signals";
import "../styles/layout.css";
import { registry } from "../layout/registry";
import { promptDialog, confirmDialog } from "../ui";
import { t } from "../i18n";

/* Free window layout: any workspace in a floating window you can move and resize, saved as named layouts. */

type Win = { id: number; ws: string; x: number; y: number; w: number; h: number; z: number; max?: boolean };
type Store = { active: string; layouts: Record<string, Win[]> };

const KEY = "lumos.layouts.v1";
const GRID = 10;

function load(): Store {
  try {
    const s = JSON.parse(localStorage.getItem(KEY) ?? "null") as Store | null;
    if (s && s.layouts && typeof s.active === "string" && s.layouts[s.active]) return s;
  } catch { /* private window or corrupt: start fresh */ }
  return { active: "Principal", layouts: { Principal: [] } };
}
function persist(s: Store) { try { localStorage.setItem(KEY, JSON.stringify(s)); } catch { /* storage unavailable */ } }

const store = signal<Store>(load());
const commit = (s: Store) => { store.value = s; persist(s); };
const wins = () => store.value.layouts[store.value.active] ?? [];
const setWins = (w: Win[]) => commit({ ...store.value, layouts: { ...store.value.layouts, [store.value.active]: w } });
const snap = (v: number, on: boolean) => (on ? Math.round(v / GRID) * GRID : v);
let zTop = 10;

const PRESETS: Record<string, (W: number, H: number) => Omit<Win, "id" | "z">[]> = {
  "Live + Cues + Monitor": (W, H) => [
    { ws: "live", x: 0, y: 0, w: Math.round(W * 0.58), h: H },
    { ws: "cues", x: Math.round(W * 0.58), y: 0, w: Math.round(W * 0.42), h: Math.round(H * 0.6) },
    { ws: "monitor", x: Math.round(W * 0.58), y: Math.round(H * 0.6), w: Math.round(W * 0.42), h: Math.round(H * 0.4) },
  ],
  "3D + Live": (W, H) => [
    { ws: "stage", x: 0, y: 0, w: Math.round(W * 0.55), h: H },
    { ws: "live", x: Math.round(W * 0.55), y: 0, w: Math.round(W * 0.45), h: H },
  ],
  "Consola virtual + Cues": (W, H) => [
    { ws: "vc", x: 0, y: 0, w: Math.round(W * 0.6), h: H },
    { ws: "cues", x: Math.round(W * 0.6), y: 0, w: Math.round(W * 0.4), h: H },
  ],
};

export function Layout() {
  const desk = useRef<HTMLDivElement>(null);
  const [grid, setGrid] = useState(true);
  const [menu, setMenu] = useState(false);
  const drag = useRef<{ id: number; kind: "move" | "size"; sx: number; sy: number; ox: number; oy: number; ow: number; oh: number } | null>(null);
  const list = registry.list.filter((w) => w.id !== "layout");
  const cur = wins();
  const names = Object.keys(store.value.layouts);

  const size = () => ({ W: desk.current?.clientWidth ?? 1200, H: desk.current?.clientHeight ?? 700 });
  const nextId = () => Math.max(0, ...cur.map((w) => w.id)) + 1;
  const add = (ws: string) => {
    const { W, H } = size();
    const n = cur.length;
    const w = Math.min(Math.round(W * 0.5), 640), h = Math.min(Math.round(H * 0.6), 420);
    setWins([...cur, { id: nextId(), ws, x: snap(30 + (n % 6) * 30, true), y: snap(20 + (n % 6) * 30, true), w, h, z: ++zTop }]);
    setMenu(false);
  };
  const patch = (id: number, p: Partial<Win>) => setWins(cur.map((w) => (w.id === id ? { ...w, ...p } : w)));
  const raise = (id: number) => { if (cur.find((w) => w.id === id)?.z !== zTop) patch(id, { z: ++zTop }); };
  const tile = () => {
    const { W, H } = size();
    const n = cur.length;
    if (!n) return;
    const cols = Math.ceil(Math.sqrt(n)), rows = Math.ceil(n / cols);
    setWins(cur.map((w, i) => ({ ...w, max: false, x: Math.round((i % cols) * (W / cols)), y: Math.round(Math.floor(i / cols) * (H / rows)), w: Math.round(W / cols), h: Math.round(H / rows) })));
  };
  const applyPreset = (name: string) => {
    const { W, H } = size();
    setWins(PRESETS[name](W, H).map((w, i) => ({ ...w, id: i + 1, z: ++zTop })));
    setMenu(false);
  };
  const switchLayout = (name: string) => commit({ ...store.value, active: name });
  const newLayout = async () => {
    const n = (await promptDialog(t("New layout"), t("Name"), t("Layout {n}", { n: names.length + 1 })))?.trim();
    if (!n) return;
    if (store.value.layouts[n]) return switchLayout(n);
    commit({ active: n, layouts: { ...store.value.layouts, [n]: [] } });
  };
  const renameLayout = async () => {
    const n = (await promptDialog(t("Rename layout"), t("Name"), store.value.active))?.trim();
    if (!n || n === store.value.active || store.value.layouts[n]) return;
    const { [store.value.active]: mine, ...rest } = store.value.layouts;
    commit({ active: n, layouts: { ...rest, [n]: mine } });
  };
  const deleteLayout = async () => {
    if (names.length < 2 || !(await confirmDialog(t("Delete layout “{name}”?", { name: store.value.active }), t("The windows of this layout are removed; the views themselves are not touched."), { danger: true, ok: t("Delete") }))) return;
    const { [store.value.active]: _gone, ...rest } = store.value.layouts;
    commit({ active: Object.keys(rest)[0], layouts: rest });
  };

  useEffect(() => {
    const move = (e: PointerEvent) => {
      const d = drag.current; if (!d) return;
      const dx = e.clientX - d.sx, dy = e.clientY - d.sy;
      if (d.kind === "move") patch(d.id, { x: Math.max(0, snap(d.ox + dx, grid)), y: Math.max(0, snap(d.oy + dy, grid)) });
      else patch(d.id, { w: Math.max(260, snap(d.ow + dx, grid)), h: Math.max(160, snap(d.oh + dy, grid)) });
    };
    const up = () => { drag.current = null; };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    return () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", up); };
  });

  return (
    <div class="lw">
      <div class="lw-bar">
        <select class="input" aria-label={t("Layout")} value={store.value.active} onChange={(e) => switchLayout((e.target as HTMLSelectElement).value)}>
          {names.map((n) => <option key={n} value={n}>{n}</option>)}
        </select>
        <button class="btn sm" onClick={() => void newLayout()} title={t("New layout")}>＋</button>
        <button class="btn sm" onClick={() => void renameLayout()}>{t("Rename")}</button>
        <button class="btn sm danger" disabled={names.length < 2} onClick={() => void deleteLayout()}>{t("Delete")}</button>
        <span class="lw-sep" />
        <div class="lw-add">
          <button class="btn sm primary" onClick={() => setMenu(!menu)} aria-expanded={menu}>{t("+ Window")}</button>
          {menu && <>
            <div class="lw-back" onClick={() => setMenu(false)} />
            <div class="lw-menu" role="menu">
              {list.map((w) => <button key={w.id} role="menuitem" class="btn sm" onClick={() => add(w.id)}>{w.label}</button>)}
              <div class="lw-menu-h">{t("Presets")}</div>
              {Object.keys(PRESETS).map((p) => <button key={p} role="menuitem" class="btn sm" onClick={() => applyPreset(p)}>{p}</button>)}
            </div>
          </>}
        </div>
        <button class="btn sm" disabled={!cur.length} onClick={tile}>{t("Tile")}</button>
        <label class="lw-snap"><input type="checkbox" checked={grid} onChange={(e) => setGrid((e.target as HTMLInputElement).checked)} />{t("Snap to grid")}</label>
        <span class="sp" />
        <span class="faint">{t("{n} windows", { n: cur.length })}</span>
      </div>
      <div class="lw-desk" ref={desk}>
        {cur.length === 0 && <div class="lw-empty"><b>{t("Free window layout")}</b><span>{t("Add windows with “+ Window”, drag them by the title and resize from the corner. Each layout is saved under its name.")}</span></div>}
        {cur.map((w) => {
          const comp = list.find((x) => x.id === w.ws);
          const Comp = comp?.component;
          const style = w.max ? { left: 0, top: 0, width: "100%", height: "100%", zIndex: w.z } : { left: w.x, top: w.y, width: w.w, height: w.h, zIndex: w.z };
          return (
            <section key={w.id} class={`lw-win ${w.max ? "max" : ""}`} style={style} onPointerDown={() => raise(w.id)} aria-label={comp?.label}>
              <header class="lw-title"
                onPointerDown={(e) => { if ((e.target as HTMLElement).closest("select,button")) return; drag.current = { id: w.id, kind: "move", sx: e.clientX, sy: e.clientY, ox: w.x, oy: w.y, ow: w.w, oh: w.h }; }}
                onDblClick={() => patch(w.id, { max: !w.max })}>
                <select class="lw-sel" aria-label={t("Window content")} value={w.ws} onChange={(e) => patch(w.id, { ws: (e.target as HTMLSelectElement).value })}>
                  {list.map((x) => <option key={x.id} value={x.id}>{x.label}</option>)}
                </select>
                <span class="sp" />
                <button class="lw-btn" aria-label={w.max ? t("Restore") : t("Maximize")} onClick={() => patch(w.id, { max: !w.max })}>{w.max ? "❐" : "▢"}</button>
                <button class="lw-btn" aria-label={t("Close window")} onClick={() => setWins(cur.filter((x) => x.id !== w.id))}>✕</button>
              </header>
              <div class="lw-body">{Comp ? <Comp /> : <div class="faint" style={{ padding: 12 }}>{t("This view no longer exists.")}</div>}</div>
              {!w.max && <div class="lw-grip" role="separator" aria-label={t("Resize")}
                onPointerDown={(e) => { e.stopPropagation(); drag.current = { id: w.id, kind: "size", sx: e.clientX, sy: e.clientY, ox: w.x, oy: w.y, ow: w.w, oh: w.h }; }} />}
            </section>
          );
        })}
      </div>
    </div>
  );
}
