import { useMemo, useState } from "preact/hooks";
import type { FixtureProfile } from "../../types";
import { library, send, show } from "../../store";
import { EmptyState, Modal, NumberField, Segmented, Select, TextField, VirtualList } from "../../ui";
import { footprint } from "../../lib/fixtures";
import { ATTRS } from "../../lib/shell-attrs";

type Item = { h: string } | { p: FixtureProfile };

export function AddDialog(props: { onClose: () => void }) {
  const profiles = [...(show.value?.profiles ?? []), ...library.value.filter((l) => !show.value?.profiles.some((p) => p.id === l.id))];
  const [q, setQ] = useState("");
  const [sel, setSel] = useState<FixtureProfile | null>(profiles[0] ?? null);
  const [modeName, setModeName] = useState<string>("");
  const [count, setCount] = useState(1);
  const unis = show.value?.universes ?? [];
  const [uni, setUni] = useState(unis[0]?.id ?? 1);
  const [auto, setAuto] = useState<"auto" | "manual">("auto");
  const [addr, setAddr] = useState(1);
  const [name, setName] = useState("");
  const items = useMemo<Item[]>(() => {
    const f = q.trim().toLowerCase();
    const list = profiles.filter((p) => !f || `${p.manufacturer} ${p.model} ${p.category}`.toLowerCase().includes(f));
    list.sort((a, b) => a.manufacturer.localeCompare(b.manufacturer) || a.model.localeCompare(b.model));
    const out: Item[] = [];
    let last = "";
    for (const p of list) { if (p.manufacturer !== last) { out.push({ h: p.manufacturer }); last = p.manufacturer; } out.push({ p }); }
    return out;
  }, [q, profiles.length]);
  const mode = sel?.modes.find((m) => m.name === modeName) ?? sel?.modes[0];
  const fp = footprint(mode);
  const label = (a: string) => ATTRS.find((x) => x.name === a)?.label ?? a;
  const ok = !!sel && !!mode && (auto === "auto" || addr + fp * count - 1 <= 512);
  return (
    <Modal title="Add fixtures" width={820} onClose={props.onClose}
      footer={<><button class="btn" onClick={props.onClose}>Cancel</button>
        <button class="btn primary" disabled={!ok} onClick={() => send({ type: "fixture_add", profile: sel!.id, mode: mode!.name, count, universe: uni, address: auto === "auto" ? null : addr, name: name.trim() || null }).then(props.onClose).catch(() => {})}>
          Add {count} × {sel?.model ?? ""}</button></>}>
      <div class="row" style={{ alignItems: "stretch", gap: 16, minHeight: 380 }}>
        <div class="col" style={{ width: 300, flex: "none" }}>
          <input class="input" placeholder="Search manufacturer, model, type…" aria-label="Search library" value={q} onInput={(e) => setQ((e.target as HTMLInputElement).value)} autofocus />
          <div class="panel grow" style={{ display: "flex", flexDirection: "column", height: 340 }}>
            {items.length === 0 ? <EmptyState title="No match" /> :
              <VirtualList class="grow" rowHeight={34} items={items} label="Fixture library" render={(it) => "h" in it ?
                <div key={"h" + it.h} style={{ height: 34, display: "flex", alignItems: "center", padding: "0 12px", background: "var(--bg-2)", fontSize: "var(--fs-xs)", fontWeight: 700, letterSpacing: ".06em", textTransform: "uppercase", color: "var(--text-dim)" }}>{it.h}</div> :
                <div key={it.p.id} role="listitem" class={`vrow ${sel?.id === it.p.id ? "sel" : ""}`} style={{ height: 34, cursor: "pointer" }} tabIndex={0}
                  onClick={() => { setSel(it.p); setModeName(""); }} onKeyDown={(e) => e.key === "Enter" && setSel(it.p)}>
                  <span class="grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{it.p.model}</span>
                  {it.p.model3d && <span class="chip ok" title="Real 3D model and optics (GDTF)">3D</span>}
                  <span class="chip" title={`Source: ${it.p.source ?? "builtin"}`}>{it.p.source ?? "builtin"}</span>
                </div>} />}
          </div>
        </div>
        <div class="col grow" style={{ gap: 12 }}>
          {sel && mode ? <>
            <div><b style={{ fontSize: "var(--fs-lg)" }}>{sel.manufacturer} {sel.model}</b> <span class="dim">· {sel.category}{sel.power_w ? ` · ${sel.power_w} W` : ""}</span>
              <span class="chip" style={{ marginLeft: 8 }}>{sel.source ?? "builtin"}</span>{sel.model3d && <span class="chip ok" style={{ marginLeft: 4 }}>3D model · {sel.model3d.wheels.reduce((n, w) => n + w.slots.length, 0)} wheel slots</span>}
              {sel.source && sel.source !== "builtin" && show.value?.profiles.some((p) => p.id === sel.id) && !show.value?.fixtures.some((f) => f.profile === sel.id) &&
                <button class="btn sm ghost" style={{ marginLeft: 8 }} onClick={() => send({ type: "library_remove", id: sel.id }).then(() => setSel(null)).catch(() => {})}>Remove from show</button>}</div>
            <Select label="Mode" value={mode.name} options={sel.modes.map((m) => ({ value: m.name, label: `${m.name} (${footprint(m)} ch)` }))} onChange={setModeName} />
            <div class="panel scroll" style={{ maxHeight: 150 }}>
              <table class="tbl"><thead><tr><th style={{ width: 50 }}>Ch</th><th>Attribute</th><th>Notes</th></tr></thead><tbody>
                {mode.channels.slice().sort((a, b) => a.offset - b.offset).map((c) => <tr key={c.offset}><td class="mono">{c.offset + 1}</td><td>{label(c.attr)}</td><td class="dim">{c.fine !== null ? `16-bit (fine ${c.fine + 1})` : ""}{c.capabilities.length ? ` ${c.capabilities.length} slots` : ""}</td></tr>)}
              </tbody></table>
            </div>
            <div class="grid2">
              <NumberField label="Quantity" value={count} min={1} max={512} onChange={setCount} />
              <Select<number> label="Universe" value={uni} options={unis.map((u) => ({ value: u.id, label: `${u.id} · ${u.name}` }))} onChange={setUni} />
            </div>
            <div class="row" style={{ alignItems: "flex-end" }}>
              <div class="field"><label>Address</label><Segmented value={auto} onChange={setAuto} options={[{ value: "auto", label: "First free" }, { value: "manual", label: "Start at" }]} /></div>
              {auto === "manual" && <NumberField class="bare" width={90} value={addr} min={1} max={512} onChange={setAddr} />}
            </div>
            {auto === "manual" && addr + fp * count - 1 > 512 && <div style={{ color: "var(--stop)" }}>{count} × {fp} channels from {addr} runs past 512.</div>}
            <TextField label="Name prefix (optional)" value={name} placeholder={sel.model} onChange={setName} />
          </> : <EmptyState title="Select a fixture type" />}
        </div>
      </div>
    </Modal>
  );
}
