import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import type { Fixture } from "../types";
import { library, selection, send, show, toast } from "../store";
import { EmptyState, NumberField, PageHead, Segmented, Select, TextField, VirtualList, confirmDialog } from "../ui";
import { collisions, footprint, modeOf, profileById } from "../lib/fixtures";
import { AddDialog } from "./patch/AddDialog";
import { UniverseMap } from "./patch/UniverseMap";

const ROW = 40;
const upd = (f: Fixture, p: Partial<Fixture>) => send({ type: "fixture_update", fixture: { ...f, ...p } }, { quiet: false }).catch(() => {});

function NameCell(props: { x: Fixture }) {
  const [v, setV] = useState(props.x.name);
  useEffect(() => setV(props.x.name), [props.x.name]);
  return <TextField class="bare" label="Name" value={v} onChange={setV} onCommit={(t) => t.trim() && t !== props.x.name && upd(props.x, { name: t })} />;
}

export function Patch() {
  const fixtures = show.value?.fixtures ?? [];
  const [view, setView] = useState<"table" | "map">("table");
  const [adding, setAdding] = useState(false);
  const [q, setQ] = useState("");
  const anchor = useRef<number | null>(null);
  const file = useRef<HTMLInputElement>(null);
  const sorted = useMemo(() => [...fixtures].sort((a, b) => a.id - b.id), [fixtures]);
  const bad = useMemo(() => collisions(fixtures), [fixtures, library.value, show.value?.profiles]);
  const f = q.trim().toLowerCase();
  const rows = f ? sorted.filter((x) => `${x.id} ${x.name} ${x.profile} ${x.mode}`.toLowerCase().includes(f)) : sorted;
  const sel = new Set(selection.value);
  const unis = (show.value?.universes ?? []).map((u) => ({ value: u.id, label: String(u.id) }));

  const click = (e: MouseEvent, id: number, idx: number) => {
    if (e.shiftKey && anchor.current !== null) {
      const a = rows.findIndex((r) => r.id === anchor.current);
      selection.value = rows.slice(Math.min(a, idx), Math.max(a, idx) + 1).map((r) => r.id);
    } else if (e.ctrlKey || e.metaKey) selection.value = sel.has(id) ? selection.value.filter((x) => x !== id) : [...selection.value, id];
    else selection.value = [id];
    anchor.current = id;
  };
  const del = async () => {
    const ids = selection.value.filter((id) => fixtures.some((x) => x.id === id));
    if (!ids.length) return;
    if (await confirmDialog("Delete fixtures", `Delete ${ids.length} fixture${ids.length > 1 ? "s" : ""}? Recorded cues keep their values.`, { danger: true, ok: "Delete" }))
      await send({ type: "fixture_remove", ids }).catch(() => {});
  };
  const cell = (n: number) => ({ width: n, flex: "none" as const });

  return (
    <div class="page">
      <PageHead title="Patch" sub={`${fixtures.length} fixtures · ${bad.size ? `${bad.size} with address conflicts` : "no conflicts"}`}>
        <input class="input" style={{ width: 160 }} placeholder="Filter…" aria-label="Filter fixtures" value={q} onInput={(e) => setQ((e.target as HTMLInputElement).value)} />
        <Segmented value={view} onChange={setView} label="View" options={[{ value: "table", label: "Table" }, { value: "map", label: "Universe map" }]} />
        <button class="btn tall" onClick={() => file.current?.click()} title="Import an Open Fixture Library JSON file">Import OFL…</button>
        <input ref={file} type="file" accept=".json,application/json" multiple hidden onChange={async (e) => {
          const files = Array.from((e.target as HTMLInputElement).files ?? []);
          (e.target as HTMLInputElement).value = "";
          for (const fl of files) await send({ type: "library_import_ofl", json: await fl.text() }).then(() => toast(`Imported ${fl.name}`, "ok", 1800)).catch(() => {});
        }} />
        <button class="btn tall danger" disabled={!selection.value.length} onClick={del}>Delete{selection.value.length ? ` (${selection.value.length})` : ""}</button>
        <button class="btn tall primary" onClick={() => setAdding(true)}>+ Add fixtures</button>
      </PageHead>
      {view === "map" ? <div class="grow scroll"><UniverseMap fixtures={fixtures} /></div> : <>
        <div class="vrow" style={{ background: "var(--bg-2)", height: 32, fontSize: "var(--fs-xs)", color: "var(--text-dim)", textTransform: "uppercase", letterSpacing: ".06em", fontWeight: 700 }}>
          <span style={cell(48)}>ID</span><span style={cell(170)}>Name</span><span class="grow hide-narrow">Profile</span><span class="grow-narrow" style={cell(110)}>Mode</span><span style={cell(70)}>Univ</span><span style={cell(76)}>Addr</span><span style={cell(40)}>Ch</span><span style={cell(52)}>End</span>
          <span class="hide-narrow" style={cell(66)}>X</span><span class="hide-narrow" style={cell(66)}>Y</span><span class="hide-narrow" style={cell(66)}>Z</span>
        </div>
        {rows.length === 0 ? <EmptyState icon="⌖" title={fixtures.length ? "No fixtures match the filter" : "Nothing patched yet"} hint="Add fixtures from the library, choose a universe and DMX address." >
          {!fixtures.length && <button class="btn primary tall" onClick={() => setAdding(true)}>+ Add fixtures</button>}</EmptyState> :
          <VirtualList class="grow" label="Fixtures" items={rows} rowHeight={ROW} render={(x, idx) => {
            const m = modeOf(x), fp = footprint(m), p = profileById(x.profile);
            return (
              <div key={x.id} role="listitem" class={`vrow ${sel.has(x.id) ? "sel" : ""} ${bad.has(x.id) ? "bad" : ""}`} style={{ height: ROW }} onClick={(e) => click(e as MouseEvent, x.id, idx)}>
                <span class="mono" style={cell(48)}>{x.id}</span>
                <span style={cell(170)} onClick={(e) => e.stopPropagation()}><NameCell x={x} /></span>
                <span class="grow dim hide-narrow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }} title={x.profile}>{p ? `${p.manufacturer} ${p.model}` : <span style={{ color: "var(--stop)" }}>missing: {x.profile}</span>}</span>
                <span style={cell(110)} onClick={(e) => e.stopPropagation()}>
                  <Select class="bare" label="Mode" value={x.mode} options={(p?.modes ?? [{ name: x.mode }]).map((md) => ({ value: md.name, label: md.name }))} onChange={(mode) => upd(x, { mode })} /></span>
                <span style={cell(70)} onClick={(e) => e.stopPropagation()}><Select<number> class="bare" label="Universe" value={x.universe} options={unis.some((u) => u.value === x.universe) ? unis : [...unis, { value: x.universe, label: String(x.universe) }]} onChange={(universe) => upd(x, { universe })} /></span>
                <span style={cell(76)} onClick={(e) => e.stopPropagation()}><NumberField class="bare" label="Address" value={x.address} min={1} max={512} onChange={(address) => upd(x, { address })} /></span>
                <span class="mono dim" style={cell(40)}>{fp}</span>
                <span class="mono" style={{ ...cell(52), color: bad.has(x.id) ? "var(--stop)" : undefined }}>{x.address + fp - 1}</span>
                {[0, 1, 2].map((i) => <span key={i} class="hide-narrow" style={cell(66)} onClick={(e) => e.stopPropagation()}>
                  <NumberField float class="bare" label={"XYZ"[i]} value={x.pos[i]} step={0.5} onChange={(v) => { const pos = [...x.pos] as [number, number, number]; pos[i] = v; upd(x, { pos }); }} /></span>)}
              </div>
            );
          }} />}
      </>}
      {adding && <AddDialog onClose={() => setAdding(false)} />}
    </div>
  );
}
