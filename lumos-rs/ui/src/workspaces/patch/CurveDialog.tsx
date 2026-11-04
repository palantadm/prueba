import { useState } from "preact/hooks";
import type { ResponseCurve } from "../../types";
import { send, show, toast } from "../../store";
import { Modal } from "../../ui";
import { t } from "../../i18n";

/* Intensity response (dimmer law) of the selected fixtures: how 0–100 % maps to what the fixture gets. */

const CURVES: { id: ResponseCurve; label: () => string; hint: () => string; f: (x: number) => number }[] = [
  { id: "linear", label: () => t("Linear"), hint: () => t("1:1. The default."), f: (x) => x },
  { id: "square", label: () => t("Square law"), hint: () => t("Slow at the bottom, like a tungsten lamp on a dimmer."), f: (x) => x * x },
  { id: "s_curve", label: () => t("S-curve"), hint: () => t("Soft at both ends."), f: (x) => x * x * (3 - 2 * x) },
  { id: "gamma", label: () => "γ 2.2", hint: () => t("Makes LED fixtures dim like a lamp to the eye."), f: (x) => Math.pow(x, 2.2) },
  { id: "switched", label: () => t("Non-dim"), hint: () => t("Off below 50 %, full above: contactors, fog machines, relays."), f: (x) => (x >= 0.5 ? 1 : 0) },
];

export function CurveDialog(props: { ids: number[]; onClose: () => void }) {
  const fx = (show.value?.fixtures ?? []).filter((f) => props.ids.includes(f.id));
  const [cur, setCur] = useState<ResponseCurve>(fx[0]?.curve ?? "linear");
  const apply = async () => {
    for (const f of fx) await send({ type: "fixture_update", fixture: { ...f, curve: cur } }, { quiet: true }).catch((e) => toast(String((e as Error).message ?? e), "err"));
    toast(t("Curve applied to {n} fixtures", { n: fx.length }), "ok", 2000);
    props.onClose();
  };
  const W = 120, H = 80;
  return (
    <Modal title={t("Dimmer curve")} width={520} onClose={props.onClose} footer={<><span class="grow dim">{t("{n} fixtures selected", { n: fx.length })}</span><button class="btn tall" onClick={props.onClose}>{t("Cancel")}</button><button class="btn tall primary" disabled={fx.length === 0} onClick={() => void apply()}>{t("Apply")}</button></>}>
      <div class="col" style={{ gap: 8 }}>
        {CURVES.map((c) => {
          const pts = Array.from({ length: 33 }, (_, i) => { const x = i / 32; return `${(x * W).toFixed(1)},${(H - c.f(x) * H).toFixed(1)}`; }).join(" ");
          return (
            <button key={c.id} class={`btn tall ${cur === c.id ? "primary" : ""}`} style={{ justifyContent: "flex-start", gap: 14, height: "auto", padding: 8 }} aria-pressed={cur === c.id} onClick={() => setCur(c.id)}>
              <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} aria-hidden="true"><rect width={W} height={H} fill="none" stroke="currentColor" opacity=".25" /><polyline points={pts} fill="none" stroke="currentColor" stroke-width="2" /></svg>
              <span style={{ textAlign: "left" }}><b>{c.label()}</b><br /><span class="dim">{c.hint()}</span></span>
            </button>
          );
        })}
        <div class="dim">{t("Applied at the output, after the masters. Fixtures without a dimmer channel get it on their colour channels. «Identify» ignores it so you always see the fixture.")}</div>
      </div>
    </Modal>
  );
}
