import { useMemo, useState } from "preact/hooks";

export type PickOpt = { value: number; label: string; sub?: string };

/** Searchable single-choice picker for ids (executors, cuelists, groups…). */
export function Picker(p: { value: number; options: PickOpt[]; onChange: (v: number) => void; placeholder?: string; label?: string }) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const cur = p.options.find((o) => o.value === p.value);
  const list = useMemo(() => {
    const s = q.trim().toLowerCase();
    return s ? p.options.filter((o) => `${o.label} ${o.sub ?? ""} ${o.value}`.toLowerCase().includes(s)) : p.options;
  }, [q, p.options]);
  return (
    <div class="vc-pick">
      <button class="input vc-pick-btn" aria-haspopup="listbox" aria-expanded={open} aria-label={p.label} onClick={() => { setOpen(!open); setQ(""); }}>
        {cur ? <span>{cur.label}{cur.sub && <span class="faint"> · {cur.sub}</span>}</span> : <span class="faint">{p.placeholder ?? "Choose…"}</span>}
        <span class="faint">▾</span>
      </button>
      {open && <>
        <div class="vc-pop-back" onClick={() => setOpen(false)} />
        <div class="vc-pop" role="listbox">
          <input class="input" autofocus placeholder="Search…" value={q} onInput={(e) => setQ((e.target as HTMLInputElement).value)}
            onKeyDown={(e) => { if (e.key === "Escape") setOpen(false); if (e.key === "Enter" && list[0]) { p.onChange(list[0].value); setOpen(false); } }} />
          <div class="vc-pop-list">
            {list.length === 0 && <div class="faint" style={{ padding: 8 }}>No matches</div>}
            {list.map((o) => (
              <button key={o.value} role="option" aria-selected={o.value === p.value} class={`vc-pop-item ${o.value === p.value ? "active" : ""}`}
                onClick={() => { p.onChange(o.value); setOpen(false); }}>
                <span>{o.label}</span>{o.sub && <span class="faint">{o.sub}</span>}
              </button>
            ))}
          </div>
        </div>
      </>}
    </div>
  );
}
