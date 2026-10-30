import { useState } from "preact/hooks";
import { signal } from "@preact/signals";
import { selection } from "../../store";
import type { Fixture } from "../../types";
import {
  ALL_ATTRS, attrColor, attrHome, attrLabel, capLabel, channelOf, fixtureById, GROUP_COLOR, hexToRgb, pct, rgbToHex, unionAttrs,
} from "../../lib/attrs";
import { attrState, releaseAttrs, setAttrs } from "../../lib/program";
import { colorHistory, pushColor, attrView } from "../../lib/state";
import { useDmxTick } from "../../lib/hooks";
import { ColorWheel, Fader, Knob, XYPad } from "../../widgets";
import { t } from "../../i18n";
import { FanDialog } from "./FanDialog";
import { Sheet } from "./Sheet";
import { ViewSwitch } from "./ViewSwitch";

type Family = "intensity" | "position" | "color" | "beam" | "control";
const FAMILY_LABEL: Record<Family, string> = { intensity: "Intensity", position: "Position", color: "Color", beam: "Beam", control: "Control" };
/** Selected attribute family (kept while switching selections; falls back to the first available). */
const familyTab = signal<Family>("color");

const sel = (): Fixture[] => selection.value.map((i) => fixtureById(i)).filter((f): f is Fixture => !!f);

function Card(p: { title: string; group: keyof typeof GROUP_COLOR; children: preact.ComponentChildren; onRelease?: () => void; held?: boolean }) {
  return (
    <section class="lv-card" aria-label={t(p.title)}>
      {p.held && p.onRelease && (
        <div class="lv-card-h">
          <span class="sp" />
          <button class="btn" onClick={p.onRelease}>{t("Release")}</button>
        </div>
      )}
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

export function AttrPanels() {
  return attrView.value === "sheet" ? <Sheet /> : <AttrFaders />;
}

function AttrFaders() {
  useDmxTick(10);
  const fxs = sel();
  const [fanOpen, setFanOpen] = useState(false);
  const head = (extra?: preact.ComponentChildren) => (
    <div class="panel-head lv-attrhead">
      <span>{t("Attributes")}</span><span class="sp" />{extra}
      <ViewSwitch />
    </div>
  );
  if (!fxs.length) {
    return (
      <div class="panel lv-col">{head()}
        <div class="panel-body dim lv-empty">{t("Select fixtures in the pool to edit their attributes.")}</div>
      </div>
    );
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
  const control = [...ctrlAttrs, ...other];

  const families: { id: Family; attrs: string[] }[] = ([
    { id: "intensity" as Family, attrs: has.has("dimmer") ? ["dimmer"] : [] },
    { id: "position" as Family, attrs: posAttrs },
    { id: "color" as Family, attrs: colorAttrs },
    { id: "beam" as Family, attrs: beamAttrs },
    { id: "control" as Family, attrs: control },
  ]).filter((f) => f.attrs.length > 0);
  const cur = families.find((f) => f.id === familyTab.value) ?? families[0];

  return (
    <div class="panel lv-col">
      {head(<>
        <span class="chip">{t("{n} selected", { n: fxs.length })}</span>
        <button class="btn" disabled={fxs.length < 2} onClick={() => setFanOpen(true)} title={t("Spread any attribute across the selection")}>{t("↔ Fan")}</button>
      </>)}
      <div class="lv-famtabs" role="tablist" aria-label={t("Attributes")}>
        {families.map((f) => (
          <button key={f.id} role="tab" aria-selected={cur?.id === f.id} class={`lv-famtab ${cur?.id === f.id ? "active" : ""} ${anyHeld(f.attrs) ? "held" : ""}`}
            onClick={() => (familyTab.value = f.id)}>
            <i style={{ background: GROUP_COLOR[f.id] }} />{t(FAMILY_LABEL[f.id])}
          </button>
        ))}
      </div>
      <div class="lv-scroll lv-famBody">
        {cur?.id === "intensity" && <IntensityCard fxs={fxs} held={anyHeld(["dimmer"])} onRelease={rel(["dimmer"])} />}
        {cur?.id === "position" && <PositionCard fxs={fxs} attrs={posAttrs} held={anyHeld(posAttrs)} onRelease={rel(posAttrs)} />}
        {cur?.id === "color" && <ColorCard fxs={fxs} attrs={colorAttrs} held={anyHeld(colorAttrs)} onRelease={rel(colorAttrs)} />}
        {cur?.id === "beam" && <EncoderCard title="Beam" group="beam" fxs={fxs} attrs={beamAttrs} onRelease={rel(beamAttrs)} />}
        {cur?.id === "control" && <EncoderCard title="Control" group="control" fxs={fxs} attrs={control} onRelease={rel(control)} />}
      </div>
      {fanOpen && <FanDialog ids={selection.value} attrs={[...has].sort((a, b) => (ALL_ATTRS.indexOf(a) + 1 || 99) - (ALL_ATTRS.indexOf(b) + 1 || 99))} onClose={() => setFanOpen(false)} />}
    </div>
  );
}

function IntensityCard({ fxs, held, onRelease }: { fxs: Fixture[]; held: boolean; onRelease: () => void }) {
  const set = (v: number) => setAttrs(selection.value, { dimmer: v });
  const s = attrState(fxs, "dimmer");
  return (
    <Card title="Intensity" group="intensity" held={held} onRelease={onRelease}>
      <div class="lv-intensity">
        <div class="col" style={{ alignItems: "center", gap: 10 }}>
          <Fader value={s.value} held={s.held} height={300} width={84} color={attrColor("dimmer")} ariaLabel={attrLabel("dimmer")}
            defaultValue={attrHome("dimmer")} onChange={(v) => set(v)} />
          <div class="lv-bigval mono" style={{ color: s.held ? "var(--amber)" : undefined }}>{pct(s.value)}</div>
        </div>
        <div class="col lv-quick">
          {[[t("Full"), 1], ["75%", 0.75], ["50%", 0.5], ["25%", 0.25], [t("Out"), 0]].map(([label, v]) => (
            <button key={String(label)} class="btn lg" onClick={() => set(v as number)}>{label}</button>
          ))}
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
      <div class="lv-attr-row lv-pos">
        <div class="col lv-pad">
          <XYPad x={pan.value} y={tilt.value} held={pan.held || tilt.held}
            onChange={(x, y) => setAttrs(ids, { pan: x, tilt: y })} />
          <div class="row mono lv-padread">
            <span>{t("Pan")} <b style={{ color: pan.held ? "var(--amber)" : undefined }}>{pct(pan.value)}</b></span>
            <span class="sp" />
            <span>{t("Tilt")} <b style={{ color: tilt.held ? "var(--amber)" : undefined }}>{pct(tilt.value)}</b></span>
          </div>
        </div>
        <div class="col lv-posctl">
          <div class="label">{t("Fine")}</div>
          <div class="lv-nudge">
            <span />
            <button class="btn lg" aria-label={t("Tilt up")} onClick={() => nudge("tilt", 0.002)}>▲</button>
            <span />
            <button class="btn lg" aria-label={t("Pan left")} onClick={() => nudge("pan", -0.002)}>◀</button>
            <button class="btn lg" onClick={() => setAttrs(ids, { pan: 0.5, tilt: 0.5 })}>{t("Home")}</button>
            <button class="btn lg" aria-label={t("Pan right")} onClick={() => nudge("pan", 0.002)}>▶</button>
            <span />
            <button class="btn lg" aria-label={t("Tilt down")} onClick={() => nudge("tilt", -0.002)}>▼</button>
            <span />
          </div>
          <div class="label" style={{ marginTop: 6 }}>{t("Fan across selection")}</div>
          <Fader horizontal width="100%" value={fan} onChange={setFan} color="var(--attr-position)" ariaLabel={t("Fan width")} />
          <button class="btn lg" disabled={ids.length < 2} onClick={doFan}>{t("Fan pan ({n}%)", { n: Math.round(fan * 200) })}</button>
        </div>
      </div>
    </Card>
  );
}

const TRACK: Record<string, string> = {
  red: "linear-gradient(90deg,#000,#ff3030)", green: "linear-gradient(90deg,#000,#30ff50)", blue: "linear-gradient(90deg,#000,#3080ff)",
  white: "linear-gradient(90deg,#000,#fff)", amber: "linear-gradient(90deg,#000,#ffb02e)", uv: "linear-gradient(90deg,#000,#7a2cff)",
  lime: "linear-gradient(90deg,#000,#8bd450)", cto: "linear-gradient(90deg,#fff,#ff9a3d)",
};
const WARM: [number, number, number] = [1, 0.78, 0.52];
const COOL: [number, number, number] = [0.86, 0.93, 1];

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
      <div class="lv-colorlayout">
        {hasRgb && (
          <div class="lv-wheelcol">
            <ColorWheel rgb={rgb} held={r.held || g.held || b.held} onChange={apply} onEnd={(c) => pushColor(rbHex(c))} size={292} />
            <div class="row" style={{ width: "100%" }}>
              <button class="btn lg grow" onClick={() => { apply(WARM); pushColor(rgbToHex(...WARM)); }}>{t("Warm white")} <span class="mono dim">3200K</span></button>
              <button class="btn lg grow" onClick={() => { apply(COOL); pushColor(rgbToHex(...COOL)); }}>{t("Cool white")} <span class="mono dim">5600K</span></button>
            </div>
            <div class="row" style={{ width: "100%" }}>
              <span class="lv-sw lv-sw-big" style={{ background: hex }} />
              <input class="input mono grow" aria-label={t("Hex colour")} value={hexIn ?? hex}
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
        <div class="lv-slidercol">
          {sliders.map((a) => {
            const s = attrState(fxs, a);
            return (
              <div class="lv-rgbrow" key={a}>
                <span class="lv-rgblabel">{attrLabel(a)}</span>
                <Fader horizontal width="100%" height={56} value={s.value} held={s.held} color={attrColor(a)} gradient={TRACK[a]} ariaLabel={attrLabel(a)}
                  onChange={(v) => setAttrs(ids, { [a]: v })} />
                <span class="lv-val mono" style={{ color: s.held ? "var(--amber)" : undefined }}>{Math.round(s.value * 255)}</span>
                {s.held ? <button class="btn ghost icon" aria-label={t("Release {label}", { label: attrLabel(a) })} onClick={() => releaseAttrs(ids, [a])}>✕</button> : <span />}
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
const rbHex = (c: [number, number, number]) => rgbToHex(...c);

function EncoderCard({ title, group, fxs, attrs, onRelease }: { title: string; group: keyof typeof GROUP_COLOR; fxs: Fixture[]; attrs: string[]; onRelease: () => void }) {
  const ids = selection.value;
  const wheels = attrs.filter((a) => (a === "gobo" || a === "prism") && fxs.some((f) => channelOf(f, a)?.capabilities.length));
  const states = attrs.map((a) => attrState(fxs, a));
  const held = states.some((s) => s.held);
  return (
    <Card title={title} group={group} held={held} onRelease={onRelease}>
      <div class="lv-attr-row lv-knobs">
        {attrs.map((a, i) => (
          <Knob key={a} label={attrLabel(a)} value={states[i].value} held={states[i].held} color={attrColor(a)}
            onChange={(v) => setAttrs(ids, { [a]: v })} onRelease={() => releaseAttrs(ids, [a])} />
        ))}
      </div>
      {wheels.map((a) => {
        const s = attrState(fxs, a);
        return (
          <div key={a} style={{ marginTop: 12 }}>
            <div class="label">{t("{attr} slots", { attr: attrLabel(a) })}</div>
            <Slots fxs={fxs} attr={a} value={s.value} held={s.held} />
          </div>
        );
      })}
    </Card>
  );
}
