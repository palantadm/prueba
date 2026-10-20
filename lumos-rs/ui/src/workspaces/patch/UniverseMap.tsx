import { useMemo } from "preact/hooks";
import type { Fixture } from "../../types";
import { selection, show } from "../../store";
import { footprint, hueFor, modeOf } from "../../lib/fixtures";

type R = { f: Fixture; a: number; b: number };

export function UniverseMap(props: { fixtures: Fixture[] }) {
  const ids = [...new Set([...(show.value?.universes ?? []).map((u) => u.id), ...props.fixtures.map((f) => f.universe)])].sort((a, b) => a - b);
  const by = useMemo(() => {
    const m = new Map<number, R[]>();
    for (const f of props.fixtures) {
      const r = { f, a: Math.max(1, f.address), b: Math.min(512, f.address + footprint(modeOf(f)) - 1) };
      (m.get(f.universe) ?? m.set(f.universe, []).get(f.universe)!).push(r);
    }
    return m;
  }, [props.fixtures, show.value?.profiles.length]);
  const used = ids.filter((u) => by.has(u));
  const shown = used.length ? used : ids.slice(0, 4);
  return (
    <div class="page-body col" style={{ gap: 16 }}>
      <div class="dim">Each bar is one universe (channels 1–512). Red stripes mark overlapping fixtures. Click a block to select its fixture.</div>
      {shown.map((u) => {
        const list = (by.get(u) ?? []).sort((x, y) => x.a - y.a);
        const ov: [number, number][] = [];
        for (let i = 0; i < list.length; i++) for (let j = i + 1; j < list.length && list[j].a <= list[i].b; j++) ov.push([list[j].a, Math.min(list[i].b, list[j].b)]);
        const usedCh = list.reduce((n, r) => n + (r.b - r.a + 1), 0);
        return (
          <div key={u} class="col" style={{ gap: 4 }}>
            <div class="row"><b>Universe {u}</b><span class="dim">{show.value?.universes.find((x) => x.id === u)?.name}</span><span class="sp" /><span class="mono dim">{usedCh}/512 ch · {list.length} fixtures</span></div>
            <div class="mapbar" role="img" aria-label={`Universe ${u} address map`}>
              {list.map((r) => (
                <div key={r.f.id} class="blk" title={`#${r.f.id} ${r.f.name} · ${r.a}–${r.b}`}
                  style={{ left: `${((r.a - 1) / 512) * 100}%`, width: `${((r.b - r.a + 1) / 512) * 100}%`, background: `hsl(${hueFor(r.f.profile)} 55% ${selection.value.includes(r.f.id) ? 80 : 58}%)`, outline: selection.value.includes(r.f.id) ? "2px solid #fff" : "none", outlineOffset: -2 }}
                  onClick={(e) => { selection.value = e.ctrlKey || e.metaKey ? [...new Set([...selection.value, r.f.id])] : [r.f.id]; }}>{r.f.id}</div>
              ))}
              {ov.map(([a, b], i) => <div key={i} class="ovl" style={{ left: `${((a - 1) / 512) * 100}%`, width: `${((b - a + 1) / 512) * 100}%` }} />)}
            </div>
            <div class="row mono faint" style={{ justifyContent: "space-between", fontSize: 10 }}>{[1, 128, 256, 384, 512].map((n) => <span key={n}>{n}</span>)}</div>
          </div>
        );
      })}
    </div>
  );
}
