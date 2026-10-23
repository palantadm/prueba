import "../styles/live.css";
import { useMemo, useState } from "preact/hooks";
import { dmx, fire, library, rt, show } from "../store";
import { Modal, ParkedButton, ParkedList } from "../ui";
import { okey, overrides, parkedList, parkedValue, setOverride, unparkChannel } from "../lib/park";
import { attrColor, attrLabel, fixtureAttrs, modeOf } from "../lib/attrs";
import { useDmxTick } from "../lib/hooks";
import { Fader } from "../widgets";
import { t } from "../i18n";

const PAGE = 32;
const key = okey;
const setOv = setOverride;

/** Channel actions: park at a value, unpark, flash — big touch targets. */
function ChannelDialog(p: { uni: number; ch: number; live: number; parked: number | undefined; onClose: () => void }) {
  const [v, setV] = useState(String(p.parked ?? p.live));
  const n = Math.max(0, Math.min(255, Math.round(Number(v) || 0)));
  return (
    <Modal title={t("Channel U{u}.{c}", { u: p.uni, c: p.ch })} onClose={p.onClose} footer={<button class="btn" style={{ minHeight: 44 }} onClick={p.onClose}>{t("Close")}</button>}>
      <div class="dim">{p.parked !== undefined ? t("Parked at {v}", { v: p.parked }) : t("Output now: {v}", { v: p.live })}</div>
      <div class="field"><label>{t("Value (0–255)")}</label>
        <div class="row">
          <input class="input num" style={{ minHeight: 44 }} type="number" min={0} max={255} autoFocus value={v} onInput={(e) => setV((e.target as HTMLInputElement).value)} />
          <button class="btn lg primary" style={{ minHeight: 44 }} onClick={() => { setOv(p.uni, p.ch, n); p.onClose(); }}>{t("Park at {v}", { v: n })}</button>
        </div>
      </div>
      <div class="row wrap">
        {[0, 64, 128, 255].map((x) => <button key={x} class="btn" style={{ minHeight: 44, minWidth: 64 }} onClick={() => setV(String(x))}>{x}</button>)}
      </div>
      <button class="btn lg" style={{ minHeight: 44 }} disabled={p.parked === undefined && overrides.value[key(p.uni, p.ch)] === undefined}
        onClick={() => { unparkChannel(p.uni, p.ch); p.onClose(); }}>{t("Unpark")}</button>
    </Modal>
  );
}

export function Desk() {
  useDmxTick(15);
  const universes = show.value?.universes ?? [];
  const [uni, setUni] = useState<number>(universes[0]?.id ?? 1);
  const [page, setPage] = useState(0);
  const [pct, setPct] = useState(false);
  const [chDlg, setChDlg] = useState<number | null>(null);
  const [parkedOpen, setParkedOpen] = useState(false);
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

  void rt.value?.parked;
  const heldOnPage = Object.keys(overrides.value).filter((k) => { const [u, c] = k.split(":").map(Number); return u === uni && c > page * PAGE && c <= (page + 1) * PAGE; });
  const fmt = (v: number) => (pct ? `${Math.round((v / 255) * 100)}%` : String(v));
  return (
    <div class="lv-desk">
      <div class="row wrap" style={{ padding: "var(--s-2) var(--s-3)", borderBottom: "1px solid var(--line)", background: "var(--bg-1)" }}>
        <label class="label" for="desk-uni">{t("Universe")}</label>
        <select id="desk-uni" class="input" style={{ width: 200 }} value={uni} onChange={(e) => { setUni(Number((e.target as HTMLSelectElement).value)); setPage(0); }}>
          {universes.map((u) => <option key={u.id} value={u.id}>{u.id} — {u.name}</option>)}
        </select>
        <div class="row" role="tablist" aria-label={t("Channel page")}>
          {Array.from({ length: 512 / PAGE }, (_, i) => (
            <button key={i} role="tab" aria-selected={i === page} class={`btn sm ${i === page ? "sel" : ""}`} onClick={() => setPage(i)}>{i * PAGE + 1}</button>
          ))}
        </div>
        <span class="sp" />
        <ParkedButton onOpen={() => setParkedOpen(true)} />
        <button class={`btn sm ${pct ? "sel" : ""}`} onClick={() => setPct(!pct)}>{pct ? "%" : "0–255"}</button>
        <button class="btn sm danger" disabled={!heldOnPage.length} onClick={() => heldOnPage.forEach((k) => { const [u, c] = k.split(":").map(Number); setOv(u, c, null); })}>{t("Release page")}</button>
      </div>
      <div class="lv-desk-strips" role="group" aria-label={t("DMX faders")}>
        {Array.from({ length: PAGE }, (_, i) => {
          const ch = page * PAGE + i + 1;
          const ov = overrides.value[key(uni, ch)] ?? parkedValue(uni, ch);
          const live = frame ? frame[ch - 1] : 0;
          const v = ov ?? live;
          const o = owners.get(ch);
          return (
            <div key={ch} class={`lv-ch ${ov !== undefined ? "held" : ""}`}>
              <button class="lv-ch-n lv-ch-btn" aria-label={t("Channel {n} actions (park / unpark)", { n: ch })} onClick={() => setChDlg(ch)}>{ov !== undefined ? <b style={{ color: "var(--live)" }}>P</b> : null}{ch}</button>
              <Fader value={v / 255} held={ov !== undefined} color={o ? attrColor(o.attr) : "var(--accent)"} width={36} height="auto"
                ariaLabel={t("Channel {n}", { n: ch })} onChange={(x) => setOv(uni, ch, Math.round(x * 255))} />
              <div class="lv-ch-v" style={{ color: ov !== undefined ? "var(--live)" : undefined }}>{fmt(v)}</div>
              <button class="btn sm" aria-label={t("Flash channel {n}", { n: ch })}
                onPointerDown={(e) => { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); fire({ type: "dmx_set", universe: uni, channel: ch, value: 255 }); }}
                onPointerUp={() => fire({ type: "dmx_set", universe: uni, channel: ch, value: overrides.value[key(uni, ch)] ?? null })}
                onPointerCancel={() => fire({ type: "dmx_set", universe: uni, channel: ch, value: overrides.value[key(uni, ch)] ?? null })}>{t("FL")}</button>
              <button class="btn sm ghost" disabled={ov === undefined} aria-label={t("Release channel {n}", { n: ch })} onClick={() => setOv(uni, ch, null)}>{t("REL")}</button>
              <div class="lv-ch-lbl" title={o ? `${o.name} · ${attrLabel(o.attr)}${o.fine ? " " + t("(fine)") : ""}` : ""}>
                {o ? <>{o.name}<br />{attrLabel(o.attr)}{o.fine ? " ʟ" : ""}</> : ""}
              </div>
            </div>
          );
        })}
      </div>
      {chDlg !== null && <ChannelDialog uni={uni} ch={chDlg} live={frame ? frame[chDlg - 1] : 0} parked={overrides.value[key(uni, chDlg)] ?? parkedValue(uni, chDlg)} onClose={() => setChDlg(null)} />}
      {parkedOpen && <ParkedList onClose={() => setParkedOpen(false)} />}
    </div>
  );
}
