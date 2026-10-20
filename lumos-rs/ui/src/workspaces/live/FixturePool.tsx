import { useRef, useState } from "preact/hooks";
import { rt, selection, send, show } from "../../store";
import type { Fixture, Group } from "../../types";
import { fixtureLevel, fixtureRgb, hasAttr } from "../../lib/attrs";
import { useDmxTick, useLongPress } from "../../lib/hooks";
import { ConfirmDialog, PromptDialog } from "./dialogs";
import { Modal } from "../../ui";

const sortedFx = (): Fixture[] => [...(show.value?.fixtures ?? [])].sort((a, b) => a.id - b.id);

export function FixturePool() {
  useDmxTick(10);
  const fixtures = sortedFx();
  const sel = new Set(selection.value);
  const anchor = useRef<number | null>(null);
  const wrap = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const drag = useRef<{ x: number; y: number; add: boolean; base: number[]; moved: boolean } | null>(null);

  const click = (id: number, e: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }) => {
    const ids = fixtures.map((f) => f.id);
    if (e.shiftKey && anchor.current !== null) {
      const a = ids.indexOf(anchor.current), b = ids.indexOf(id);
      const [lo, hi] = a < b ? [a, b] : [b, a];
      selection.value = ids.slice(lo, hi + 1);
    } else if (e.ctrlKey || e.metaKey) {
      selection.value = sel.has(id) ? selection.value.filter((x) => x !== id) : [...selection.value, id];
      anchor.current = id;
    } else {
      selection.value = [id];
      anchor.current = id;
    }
  };

  const down = (e: PointerEvent) => {
    if (e.button !== 0) return;
    drag.current = { x: e.clientX, y: e.clientY, add: e.ctrlKey || e.metaKey, base: selection.value, moved: false };
  };
  const move = (e: PointerEvent) => {
    const d = drag.current;
    if (!d || e.pointerType === "touch") return;
    if (!d.moved && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 6) return;
    if (!d.moved) { d.moved = true; (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); }
    const r = wrap.current!.getBoundingClientRect();
    const b = { x0: Math.min(d.x, e.clientX) - r.left, y0: Math.min(d.y, e.clientY) - r.top, x1: Math.max(d.x, e.clientX) - r.left, y1: Math.max(d.y, e.clientY) - r.top };
    setBox(b);
    const hit: number[] = [];
    wrap.current!.querySelectorAll<HTMLElement>("[data-fid]").forEach((el) => {
      const c = el.getBoundingClientRect();
      const x0 = c.left - r.left, y0 = c.top - r.top;
      if (x0 < b.x1 && x0 + c.width > b.x0 && y0 < b.y1 && y0 + c.height > b.y0) hit.push(Number(el.dataset.fid));
    });
    selection.value = d.add ? [...new Set([...d.base, ...hit])] : hit;
  };
  const up = (e: PointerEvent) => {
    const d = drag.current;
    drag.current = null;
    if (d?.moved) { setBox(null); (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId); }
    else if (!(e.target as HTMLElement).closest("[data-fid]") && !d?.add) selection.value = [];
  };

  const ids = fixtures.map((f) => f.id);
  const step = (dir: 1 | -1) => {
    if (!ids.length) return;
    const cur = selection.value.length ? ids.indexOf(selection.value[selection.value.length - 1]) : dir === 1 ? -1 : 0;
    const n = (cur + dir + ids.length) % ids.length;
    selection.value = [ids[n]]; anchor.current = ids[n];
  };
  const prog = rt.value?.programmer ?? {};
  return (
    <div class="panel lv-col pool">
      <div class="panel-head">
        <span>Fixtures</span><span class="sp" />
        <span class="chip"><span class="lv-chipsel">{selection.value.length}</span> / {fixtures.length}</span>
      </div>
      <div class="lv-selbar" role="toolbar" aria-label="Selection">
        <button class="btn sm" onClick={() => (selection.value = ids)}>All</button>
        <button class="btn sm" onClick={() => (selection.value = [])}>None</button>
        <button class="btn sm" onClick={() => (selection.value = ids.filter((i) => !sel.has(i)))}>Invert</button>
        <button class="btn sm" onClick={() => (selection.value = selection.value.filter((_, i) => i % 2 === 0))}>Odd</button>
        <button class="btn sm" onClick={() => (selection.value = selection.value.filter((_, i) => i % 2 === 1))}>Even</button>
        <button class="btn sm" onClick={() => step(-1)} aria-label="Previous fixture">◀ Prev</button>
        <button class="btn sm" onClick={() => step(1)} aria-label="Next fixture">Next ▶</button>
      </div>
      <GroupsRow />
      <div class="lv-scroll">
        <div ref={wrap} class="lv-pool" onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up}>
          {fixtures.length === 0 && <div class="dim" style={{ gridColumn: "1/-1", padding: 16 }}>No fixtures patched. Add fixtures in the Patch workspace.</div>}
          {fixtures.map((fx) => {
            const lvl = fixtureLevel(fx);
            const held = !!prog[fx.id] && Object.keys(prog[fx.id]!).length > 0;
            const [r, g, b] = fixtureRgb(fx);
            const hasColor = hasAttr(fx, "red") || hasAttr(fx, "green") || hasAttr(fx, "blue");
            return (
              <div key={fx.id} data-fid={fx.id} class={`lv-fx ${sel.has(fx.id) ? "sel" : ""} ${held ? "held" : ""}`}
                role="button" aria-pressed={sel.has(fx.id)} aria-label={`Fixture ${fx.id} ${fx.name}`}
                onClick={(e) => click(fx.id, e)}>
                <span class="lv-fx-id">{fx.id}</span>
                <span class="lv-fx-name">{fx.name}</span>
                {hasColor && lvl > 0 && <span class="lv-fx-sw" style={{ background: `rgb(${r * 255 | 0},${g * 255 | 0},${b * 255 | 0})` }} />}
                <span class="lv-fx-bar"><i style={{ width: `${lvl * 100}%` }} /></span>
              </div>
            );
          })}
          {box && <div class="lv-box" style={{ left: box.x0, top: box.y0, width: box.x1 - box.x0, height: box.y1 - box.y0 }} />}
        </div>
      </div>
    </div>
  );
}

function GroupsRow() {
  const groups = show.value?.groups ?? [];
  const [naming, setNaming] = useState(false);
  const [edit, setEdit] = useState<Group | null>(null);
  const nextId = () => Math.max(0, ...groups.map((g) => g.id)) + 1;
  return (
    <div class="lv-groups" role="toolbar" aria-label="Groups">
      {groups.map((g) => <GroupBtn key={g.id} g={g} onEdit={() => setEdit(g)} />)}
      <button class="btn sm" disabled={!selection.value.length} onClick={() => setNaming(true)} title="Record group from selection">+ Group</button>
      {naming && (
        <PromptDialog title="Record group" label="Group name" initial={`Group ${nextId()}`} ok="Record" onClose={() => setNaming(false)}
          onSubmit={(name) => send({ type: "group_save", group: { id: nextId(), name, fixtures: [...selection.value].sort((a, b) => a - b) } })} />
      )}
      {edit && <GroupEdit g={edit} onClose={() => setEdit(null)} />}
    </div>
  );
}

function GroupBtn({ g, onEdit }: { g: Group; onEdit: () => void }) {
  const lp = useLongPress(onEdit);
  const active = g.fixtures.length > 0 && g.fixtures.length === selection.value.length && g.fixtures.every((f) => selection.value.includes(f));
  return (
    <button class={`btn sm ${active ? "sel" : ""}`} {...lp as any}
      onClick={() => { if (!lp.wasLong()) selection.value = [...g.fixtures]; }}
      title={`${g.fixtures.length} fixtures — right-click / long-press to edit`}>
      {g.name} <span class="faint">{g.fixtures.length}</span>
    </button>
  );
}

function GroupEdit({ g, onClose }: { g: Group; onClose: () => void }) {
  const [name, setName] = useState(g.name);
  const [del, setDel] = useState(false);
  if (del) return <ConfirmDialog title="Delete group" text={`Delete group “${g.name}”?`} ok="Delete" danger onClose={onClose}
    onYes={() => send({ type: "group_remove", id: g.id })} />;
  return (
    <Modal title={`Group “${g.name}”`} onClose={onClose} footer={<>
      <button class="btn danger" onClick={() => setDel(true)}>Delete</button><span class="sp" />
      <button class="btn" onClick={onClose}>Cancel</button>
      <button class="btn primary" onClick={() => { send({ type: "group_save", group: { ...g, name: name.trim() || g.name } }); onClose(); }}>Save name</button>
    </>}>
      <div class="field"><label>Name</label><input class="input" value={name} onInput={(e) => setName((e.target as HTMLInputElement).value)} /></div>
      <div class="row">
        <button class="btn" disabled={!selection.value.length}
          onClick={() => { send({ type: "group_save", group: { ...g, fixtures: [...selection.value].sort((a, b) => a - b) } }); onClose(); }}>
          Replace members with current selection ({selection.value.length})</button>
      </div>
      <div class="dim">Members: {g.fixtures.join(", ") || "—"}</div>
    </Modal>
  );
}
