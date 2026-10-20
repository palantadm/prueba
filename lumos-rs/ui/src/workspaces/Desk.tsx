import "../styles/live.css";
import { signal } from "@preact/signals";
import { useMemo, useState } from "preact/hooks";
import { dmx, fire, library, show } from "../store";
import { attrColor, attrLabel, fixtureAttrs, modeOf } from "../lib/attrs";
import { useDmxTick } from "../lib/hooks";
import { Fader } from "../widgets";

const PAGE = 32;
const overrides = signal<Record<string, number>>({});
const key = (u: number, c: number) => `${u}:${c}`;

function setOv(u: number, c: number, v: number | null) {
  const o = { ...overrides.value };
  if (v === null) delete o[key(u, c)]; else o[key(u, c)] = v;
  overrides.value = o;
  fire({ type: "dmx_set", universe: u, channel: c, value: v });
}

export function Desk() {
  useDmxTick(15);
  const universes = show.value?.universes ?? [];
  const [uni, setUni] = useState<number>(universes[0]?.id ?? 1);
  const [page, setPage] = useState(0);
  const [pct, setPct] = useState(false);
  const frame = dmx.get(uni);

  const owners = useMemo(() => {
    const m = new Map<number, { name: string; attr: string; fine: boolean }>();
    for (const fx of show.value?.fixtures ?? []) {
      if (fx.universe !== uni) continue;
      for (const c of modeOf(fx)?.channels ?? []) {
        m.set(fx.address + c.offset, { name: fx.name || `#${fx.id}`, attr: c.attr, fine: false });
        if (c.fine != null) m.set(fx.address + c.fine, { name: fx.name || `#${fx.id}`, attr: c.attr, fine: true });
      }
    }
    return m;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [uni, show.value?.fixtures, library.value]);
  void fixtureAttrs;

  const heldOnPage = Object.keys(overrides.value).filter((k) => { const [u, c] = k.split(":").map(Number); return u === uni && c > page * PAGE && c <= (page + 1) * PAGE; });
  const fmt = (v: number) => (pct ? `${Math.round((v / 255) * 100)}%` : String(v));
  return (
    <div class="lv-desk">
      <div class="row wrap" style={{ padding: "var(--s-2) var(--s-3)", borderBottom: "1px solid var(--line)", background: "var(--bg-1)" }}>
        <label class="label" for="desk-uni">Universe</label>
        <select id="desk-uni" class="input" style={{ width: 200 }} value={uni} onChange={(e) => { setUni(Number((e.target as HTMLSelectElement).value)); setPage(0); }}>
          {universes.map((u) => <option key={u.id} value={u.id}>{u.id} — {u.name}</option>)}
        </select>
        <div class="row" role="tablist" aria-label="Channel page">
          {Array.from({ length: 512 / PAGE }, (_, i) => (
            <button key={i} role="tab" aria-selected={i === page} class={`btn sm ${i === page ? "sel" : ""}`} onClick={() => setPage(i)}>{i * PAGE + 1}</button>
          ))}
        </div>
        <span class="sp" />
        <button class={`btn sm ${pct ? "sel" : ""}`} onClick={() => setPct(!pct)}>{pct ? "%" : "0–255"}</button>
        <button class="btn sm danger" disabled={!heldOnPage.length} onClick={() => heldOnPage.forEach((k) => { const [u, c] = k.split(":").map(Number); setOv(u, c, null); })}>Release page</button>
      </div>
      <div class="lv-desk-strips" role="group" aria-label="DMX faders">
        {Array.from({ length: PAGE }, (_, i) => {
          const ch = page * PAGE + i + 1;
          const ov = overrides.value[key(uni, ch)];
          const live = frame ? frame[ch - 1] : 0;
          const v = ov ?? live;
          const o = owners.get(ch);
          return (
            <div key={ch} class={`lv-ch ${ov !== undefined ? "held" : ""}`}>
              <div class="lv-ch-n">{ch}</div>
              <Fader value={v / 255} held={ov !== undefined} color={o ? attrColor(o.attr) : "var(--accent)"} width={36} height={260}
                ariaLabel={`Channel ${ch}`} onChange={(x) => setOv(uni, ch, Math.round(x * 255))} />
              <div class="lv-ch-v" style={{ color: ov !== undefined ? "var(--live)" : undefined }}>{fmt(v)}</div>
              <button class="btn sm" aria-label={`Flash channel ${ch}`}
                onPointerDown={(e) => { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); fire({ type: "dmx_set", universe: uni, channel: ch, value: 255 }); }}
                onPointerUp={() => fire({ type: "dmx_set", universe: uni, channel: ch, value: overrides.value[key(uni, ch)] ?? null })}
                onPointerCancel={() => fire({ type: "dmx_set", universe: uni, channel: ch, value: overrides.value[key(uni, ch)] ?? null })}>FL</button>
              <button class="btn sm ghost" disabled={ov === undefined} aria-label={`Release channel ${ch}`} onClick={() => setOv(uni, ch, null)}>REL</button>
              <div class="lv-ch-lbl" title={o ? `${o.name} · ${attrLabel(o.attr)}${o.fine ? " (fine)" : ""}` : ""}>
                {o ? <>{o.name}<br />{attrLabel(o.attr)}{o.fine ? " ʟ" : ""}</> : ""}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
