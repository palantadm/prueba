import { useMemo, useState } from "preact/hooks";
import { Modal } from "../../ui";
import { attrColor, attrLabel, hasAttr, fixtureById } from "../../lib/attrs";
import { fanValues, type FanMode } from "../../lib/fan";
import { setAttrs } from "../../lib/program";
import { t } from "../../i18n";

const MODES: { id: FanMode; label: string }[] = [
  { id: "linear", label: "Linear" },
  { id: "center", label: "Centre-out" },
  { id: "random", label: "Random" },
];

/** Fan / spread any attribute across fixtures, in the order they are given (selection order). */
export function FanDialog(p: { ids: number[]; attrs: string[]; initialAttr?: string; onClose: () => void }) {
  const [attr, setAttr] = useState(p.initialAttr && p.attrs.includes(p.initialAttr) ? p.initialAttr : p.attrs[0] ?? "dimmer");
  const [from, setFrom] = useState(0);
  const [to, setTo] = useState(1);
  const [mode, setMode] = useState<FanMode>("linear");
  const [reverse, setReverse] = useState(false);
  const [seed, setSeed] = useState(1);
  const targets = useMemo(() => p.ids.filter((id) => { const f = fixtureById(id); return f && hasAttr(f, attr); }), [p.ids.join(","), attr]);
  const vals = useMemo(() => fanValues(targets.length, from, to, mode, reverse, seed), [targets.length, from, to, mode, reverse, seed]);
  const apply = () => {
    targets.forEach((id, i) => setAttrs([id], { [attr]: vals[i] }));
    p.onClose();
  };
  const pctIn = (v: number, set: (n: number) => void, label: string) => (
    <div class="field"><label>{label}</label>
      <div class="row">
        <input type="range" min={0} max={100} value={Math.round(v * 100)} aria-label={label} style={{ flex: 1, minHeight: 44 }} onInput={(e) => set(Number((e.target as HTMLInputElement).value) / 100)} />
        <input class="input num" type="number" min={0} max={100} value={Math.round(v * 100)} style={{ width: 72, minHeight: 44 }}
          onInput={(e) => set(Math.max(0, Math.min(100, Number((e.target as HTMLInputElement).value) || 0)) / 100)} />
        <span class="dim">%</span>
      </div>
    </div>
  );
  return (
    <Modal title={t("Fan / spread")} onClose={p.onClose} footer={<>
      <button class="btn lg" style={{ minHeight: 44 }} onClick={p.onClose}>{t("Cancel")}</button>
      <button class="btn lg primary" style={{ minHeight: 44 }} disabled={targets.length < 1} onClick={apply}>{t("Apply to {n} fixtures", { n: targets.length })}</button>
    </>}>
      <div class="field"><label>{t("Attribute")}</label>
        <div class="row wrap" role="tablist" aria-label={t("Attribute")}>
          {p.attrs.map((a) => (
            <button key={a} role="tab" aria-selected={a === attr} class={`btn ${a === attr ? "sel" : ""}`} style={{ minHeight: 44, borderColor: a === attr ? attrColor(a) : undefined }} onClick={() => setAttr(a)}>{attrLabel(a)}</button>
          ))}
        </div>
      </div>
      {pctIn(from, setFrom, t("From"))}
      {pctIn(to, setTo, t("To"))}
      <div class="row wrap">
        {MODES.map((m) => <button key={m.id} class={`btn ${mode === m.id ? "sel" : ""}`} style={{ minHeight: 44 }} onClick={() => setMode(m.id)}>{t(m.label)}</button>)}
        {mode === "random" && <button class="btn" style={{ minHeight: 44 }} onClick={() => setSeed((s) => s + 1)}>{t("↻ Re-roll")}</button>}
        <span class="sp" />
        <label class="check" style={{ minHeight: 44 }}><input type="checkbox" checked={reverse} onChange={(e) => setReverse((e.target as HTMLInputElement).checked)} />{t("Reverse order")}</label>
      </div>
      <div class="field"><label>{t("Preview (selection order)")}</label>
        <div class="row" style={{ alignItems: "flex-end", gap: 4, height: 70, overflowX: "auto" }} aria-label={t("Preview (selection order)")}>
          {targets.map((id, i) => (
            <div key={id} class="col" style={{ alignItems: "center", gap: 2, flex: "none", width: 30 }} title={`#${id}: ${Math.round(vals[i] * 100)}%`}>
              <i style={{ width: 18, height: Math.max(2, vals[i] * 44), background: attrColor(attr), borderRadius: 2 }} />
              <span class="mono faint" style={{ fontSize: 9 }}>{id}</span>
            </div>
          ))}
          {targets.length === 0 && <span class="dim">{t("No selected fixture has this attribute.")}</span>}
        </div>
      </div>
    </Modal>
  );
}
