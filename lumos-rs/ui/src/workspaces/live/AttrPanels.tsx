import { useState } from "preact/hooks";
import { selection } from "../../store";
import type { Fixture } from "../../types";
import {
  ALL_ATTRS, attrColor, attrHome, attrLabel, capLabel, channelOf, fixtureById, GROUP_COLOR, hexToRgb, pct, rgbToHex, unionAttrs,
} from "../../lib/attrs";
import { attrState, releaseAttrs, setAttrs } from "../../lib/program";
import { attrView, colorHistory, pushColor, setAttrView } from "../../lib/state";
import { useDmxTick } from "../../lib/hooks";
import { ColorPicker, Fader, Knob, XYPad } from "../../widgets";
import { t } from "../../i18n";
import { FanDialog } from "./FanDialog";
import { Sheet } from "./Sheet";
import { ViewSwitch } from "./ViewSwitch";

const sel = (): Fixture[] => selection.value.map((i) => fixtureById(i)).filter((f): f is Fixture => !!f);

function Card(p: { title: string; group: keyof typeof GROUP_COLOR; children: preact.ComponentChildren; onRelease?: () => void; held?: boolean }) {
  return (
    <section class="lv-card" aria-label={t(p.title)}>
      <div class="lv-card-h" style={{ color: GROUP_COLOR[p.group] }}>
        <i style={{ background: GROUP_COLOR[p.group] }} />{t(p.title)}<span class="sp" />
        {p.held && p.onRelease && <button class="btn sm" onClick={p.onRelease}>{t("Release")}</button>}
      </div>
      {p.children}
    </section>
  );
}

/** Capability slot buttons for a wheel-type attribute. */
function Slots({ fxs, attr, value, held }: { fxs: Fixture[]; attr: string; value: number; held: boolean }) {
  const ch = fxs.map((f) => channelOf(f, attr)).find((c) => c && c.capabilities.length);
  if (!ch) return null;
  const dmxv = Math.round(value * 255);
  return (
    <div class="lv-slots" role="group" aria-label={t("{attr} slots", { attr: attrLabel(attr) })}>
      {ch.capabilities.map((c, i) => {
        const on = dmxv >= c.min && dmxv <= c.max;
        return (
          <button key={i} class={`lv-slot ${on && held ? "on" : ""}`} title={`${c.min}–${c.max}`}
            onClick={() => setAttrs(selection.value, { [attr]: (c.min + c.max) / 2 / 255 })}>
            {c.color && <span class="lv-sw" style={{ background: c.color }} />}{capLabel(c.label)}
          </button>
        );
      })}
    </div>
  );
}

function AttrFader({ fxs, attr, height = 190 }: { fxs: Fixture[]; attr: string; height?: number }) {
  const s = attrState(fxs, attr);
  return (
    <div class="col" style={{ alignItems: "center", gap: 6 }}>
      <Fader value={s.value} held={s.held} height={height} width={56} color={attrColor(attr)} ariaLabel={attrLabel(attr)}
        defaultValue={attrHome(attr)} onChange={(v) => setAttrs(selection.value, { [attr]: v })} />
      <div class="mono" style={{ color: s.held ? "var(--live)" : undefined }}>{pct(s.value)}</div>
    </div>
  );
}

export function AttrPanels() {
  return attrView.value === "sheet" ? <Sheet /> : <AttrFaders />;
}

function AttrFaders() {
  useDmxTick(10);
  const fxs = sel();
  const [fanOpen, setFanOpen] = useState(false);
  if (!fxs.length) {
    return <div class="panel lv-col"><div class="panel-head"><span>{t("Attributes")}</span><span class="sp" /><ViewSwitch /></div>
      <div class="panel-body dim">{t("Select fixtures in the pool to edit their attributes.")}</div></div>;
  }
  const has = unionAttrs(fxs);
  const anyHeld = (attrs: string[]) => attrs.some((a) => attrState(fxs, a).held);
  const rel = (attrs: string[]) => () => releaseAttrs(selection.value, attrs);
  const list = (arr: string[]) => arr.filter((a) => has.has(a));

  const posAttrs = list(["pan", "tilt"]);
  const colorAttrs = list(["red", "green", "blue", "white", "amber", "uv", "lime", "cto", "colorwheel"]);
  const beamAttrs = list(["zoom", "focus", "iris", "frost", "prism", "prism_rot", "gobo", "gobo_rot", "shutter"]);
  const ctrlAttrs = list(["speed", "effect", "control"]);
  const known = new Set(["dimmer", ...posAttrs, ...colorAttrs, ...beamAttrs, ...ctrlAttrs]);
  const other = [...has].filter((a) => !known.has(a));

  return (
    <div class="panel lv-col">
      <div class="panel-head"><span>{t("Attributes")}</span><span class="sp" /><span class="chip">{t("{n} selected", { n: fxs.length })}</span>
        <button class="btn sm" style={{ minHeight: 44 }} disabled={fxs.length < 2} onClick={() => setFanOpen(true)} title={t("Spread any attribute across the selection")}>{t("↔ Fan")}</button>
        <ViewSwitch />
      </div>
      <div class="lv-scroll" style={{ padding: "var(--s-2)" }}>
        {has.has("dimmer") && <IntensityCard fxs={fxs} held={anyHeld(["dimmer"])} onRelease={rel(["dimmer"])} />}
        {posAttrs.length > 0 && <PositionCard fxs={fxs} attrs={posAttrs} held={anyHeld(posAttrs)} onRelease={rel(posAttrs)} />}
        {colorAttrs.length > 0 && <ColorCard fxs={fxs} attrs={colorAttrs} held={anyHeld(colorAttrs)} onRelease={rel(colorAttrs)} />}
        {beamAttrs.length > 0 && <EncoderCard title="Beam" group="beam" fxs={fxs} attrs={beamAttrs} onRelease={rel(beamAttrs)} />}
        {(ctrlAttrs.length > 0 || other.length > 0) && <EncoderCard title="Control" group="control" fxs={fxs} attrs={[...ctrlAttrs, ...other]} onRelease={rel([...ctrlAttrs, ...other])} />}
      </div>
      {fanOpen && <FanDialog ids={selection.value} attrs={[...has].sort((a, b) => (ALL_ATTRS.indexOf(a) + 1 || 99) - (ALL_ATTRS.indexOf(b) + 1 || 99))} onClose={() => setFanOpen(false)} />}
    </div>
  );
}

function IntensityCard({ fxs, held, onRelease }: { fxs: Fixture[]; held: boolean; onRelease: () => void }) {
  const set = (v: number) => setAttrs(selection.value, { dimmer: v });
  return (
    <Card title="Intensity" group="intensity" held={held} onRelease={onRelease}>
      <div class="row" style={{ alignItems: "flex-end", gap: 24 }}>
        <AttrFader fxs={fxs} attr="dimmer" height={150} />
        <div class="col" style={{ minWidth: 100 }}>
          <button class="btn" onClick={() => set(1)}>{t("Full")}</button>
          <button class="btn" onClick={() => set(0.5)}>50%</button>
          <button class="btn" onClick={() => set(0)}>{t("Out")}</button>
        </div>
      </div>
    </Card>
  );
}

function PositionCard({ fxs, attrs, held, onRelease }: { fxs: Fixture[]; attrs: string[]; held: boolean; onRelease: () => void }) {
  const pan = attrState(fxs, "pan"), tilt = attrState(fxs, "tilt");
  const [fan, setFan] = useState(0.3);
  const ids = selection.value;
  const nudge = (a: "pan" | "tilt", d: number) => setAttrs(ids, { [a]: Math.max(0, Math.min(1, (a === "pan" ? pan.value : tilt.value) + d)) });
  const doFan = () => {
    const withPan = ids.filter((i) => { const f = fixtureById(i); return f && channelOf(f, "pan"); });
    const n = withPan.length;
    if (n < 2) return;
    withPan.forEach((id, i) => {
      const v = Math.max(0, Math.min(1, pan.value + (i / (n - 1) - 0.5) * fan * 2));
      setAttrs([id], { pan: v });
    });
  };
  return (
    <Card title="Position" group="position" held={held} onRelease={onRelease}>
      <div class="lv-attr-row">
        <div class="col" style={{ flex: "1 1 180px", maxWidth: 270 }}>
          <XYPad x={pan.value} y={tilt.value} held={pan.held || tilt.held}
            onChange={(x, y) => setAttrs(ids, { pan: x, tilt: y })} />
          <div class="row mono dim" style={{ justifyContent: "space-between" }}>
            <span>{t("Pan")} {pct(pan.value)}</span><span>{t("Tilt")} {pct(tilt.value)}</span>
          </div>
        </div>
        <div class="col" style={{ minWidth: 150 }}>
          <div class="label">{t("Fine")}</div>
          <div class="row">
            <button class="btn" aria-label={t("Pan left")} onClick={() => nudge("pan", -0.002)}>◀</button>
            <button class="btn" aria-label={t("Pan right")} onClick={() => nudge("pan", 0.002)}>▶</button>
            <button class="btn" aria-label={t("Tilt up")} onClick={() => nudge("tilt", 0.002)}>▲</button>
            <button class="btn" aria-label={t("Tilt down")} onClick={() => nudge("tilt", -0.002)}>▼</button>
          </div>
          <button class="btn" onClick={() => setAttrs(ids, { pan: 0.5, tilt: 0.5 })}>{t("Home")}</button>
          <div class="label" style={{ marginTop: 6 }}>{t("Fan across selection")}</div>
          <Fader horizontal width={150} value={fan} onChange={setFan} color="var(--attr-position)" ariaLabel={t("Fan width")} />
          <button class="btn" disabled={ids.length < 2} onClick={doFan}>{t("Fan pan ({n}%)", { n: Math.round(fan * 200) })}</button>
        </div>
      </div>
    </Card>
  );
}

function ColorCard({ fxs, attrs, held, onRelease }: { fxs: Fixture[]; attrs: string[]; held: boolean; onRelease: () => void }) {
  const ids = selection.value;
  const r = attrState(fxs, "red"), g = attrState(fxs, "green"), b = attrState(fxs, "blue");
  const rgb: [number, number, number] = [r.value, g.value, b.value];
  const hex = rgbToHex(...rgb);
  const hasRgb = attrs.includes("red") || attrs.includes("green") || attrs.includes("blue");
  const [hexIn, setHexIn] = useState<string | null>(null);
  const apply = (c: [number, number, number]) => setAttrs(ids, { red: c[0], green: c[1], blue: c[2] });
  const sliders = attrs.filter((a) => a !== "colorwheel");
  const wheel = attrState(fxs, "colorwheel");
  return (
    <Card title="Color" group="color" held={held} onRelease={onRelease}>
      <div class="lv-attr-row">
        {hasRgb && (
          <div class="col" style={{ flex: "1 1 200px", maxWidth: 260 }}>
            <ColorPicker rgb={rgb} onChange={apply} onEnd={(c) => pushColor(rbHex(c))} size={240} />
            <div class="row">
              <span class="lv-sw" style={{ background: hex, width: 28, height: 28 }} />
              <input class="input mono" aria-label={t("Hex colour")} value={hexIn ?? hex} style={{ width: 100 }}
                onInput={(e) => { const v = (e.target as HTMLInputElement).value; setHexIn(v); const c = hexToRgb(v); if (c) apply(c); }}
                onBlur={() => { if (hexIn && hexToRgb(hexIn)) pushColor(rgbToHex(...hexToRgb(hexIn)!)); setHexIn(null); }} />
            </div>
            <div class="lv-hist" aria-label={t("Recent colours")}>
              {colorHistory.value.map((c) => (
                <button key={c} style={{ background: c }} aria-label={t("Colour {c}", { c })} onClick={() => { const v = hexToRgb(c); if (v) apply(v); }} />
              ))}
            </div>
          </div>
        )}
        <div class="col" style={{ flex: "1 1 220px" }}>
          {sliders.map((a) => {
            const s = attrState(fxs, a);
            return (
              <div class="lv-rgbrow" key={a}>
                <span style={{ color: a === "red" ? "#ff6b6b" : a === "green" ? "#6bd66b" : a === "blue" ? "#6b8bff" : "var(--text-dim)" }}>{attrLabel(a)}</span>
                <Fader horizontal width="100%" height={28} value={s.value} held={s.held} color={attrColor(a)} ariaLabel={attrLabel(a)}
                  onChange={(v) => setAttrs(ids, { [a]: v })} />
                <span class="mono" style={{ color: s.held ? "var(--live)" : undefined }}>{Math.round(s.value * 255)}</span>
                {s.held ? <button class="lv-x" aria-label={t("Release {label}", { label: attrLabel(a) })} onClick={() => releaseAttrs(ids, [a])}>✕</button> : <span />}
              </div>
            );
          })}
          {attrs.includes("colorwheel") && (
            <>
              <div class="label" style={{ marginTop: 6 }}>{t("Colour wheel")}</div>
              <Slots fxs={fxs} attr="colorwheel" value={wheel.value} held={wheel.held} />
            </>
          )}
        </div>
      </div>
    </Card>
  );
}
const SLIDER_COLOR: Record<string, string> = { red: "#e5484d", green: "#35c46a", blue: "#5b8def", white: "#e8e8e8", amber: "#f5a524", uv: "#a070ff", lime: "#b6e04a" };
const rbHex = (c: [number, number, number]) => rgbToHex(...c);

function EncoderCard({ title, group, fxs, attrs, onRelease }: { title: string; group: keyof typeof GROUP_COLOR; fxs: Fixture[]; attrs: string[]; onRelease: () => void }) {
  const ids = selection.value;
  const wheels = attrs.filter((a) => (a === "gobo" || a === "prism") && fxs.some((f) => channelOf(f, a)?.capabilities.length));
  const states = attrs.map((a) => attrState(fxs, a));
  const held = states.some((s) => s.held);
  return (
    <Card title={title} group={group} held={held} onRelease={onRelease}>
      <div class="lv-attr-row" style={{ gap: 12 }}>
        {attrs.map((a, i) => (
          <Knob key={a} label={attrLabel(a)} value={states[i].value} held={states[i].held} color={attrColor(a)}
            onChange={(v) => setAttrs(ids, { [a]: v })} onRelease={() => releaseAttrs(ids, [a])} />
        ))}
      </div>
      {wheels.map((a) => {
        const s = attrState(fxs, a);
        return (
          <div key={a} style={{ marginTop: 8 }}>
            <div class="label">{t("{attr} slots", { attr: attrLabel(a) })}</div>
            <Slots fxs={fxs} attr={a} value={s.value} held={s.held} />
          </div>
        );
      })}
    </Card>
  );
}
