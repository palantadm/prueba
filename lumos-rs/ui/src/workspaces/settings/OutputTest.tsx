import { useState } from "preact/hooks";
import type { TestMode } from "../../types";
import { fire, rt, send, show } from "../../store";
import { NumberField, Segmented, Select } from "../../ui";
import { t } from "../../i18n";

type Kind = "level" | "chase" | "channel";

export const stopTest = () => send({ type: "output_test", mode: { type: "off" } }).catch(() => {});

/** Always-visible banner (rendered by App) while a test pattern overrides the output. */
export function TestBanner() {
  if (!rt.value?.testing) return null;
  return (
    <div role="alert" style={{ display: "flex", alignItems: "center", gap: 12, padding: "6px 12px", background: "var(--live-dim)", borderBottom: "1px solid var(--live)", color: "var(--live)", fontWeight: 700 }}>
      <span class="dot warn" />
      <span class="grow">{t("OUTPUT TEST ACTIVE — the show is not being output")}</span>
      <button class="btn danger" style={{ minHeight: 44 }} onClick={stopTest}>{t("Stop test")}</button>
    </div>
  );
}

export function TestPanel() {
  const [kind, setKind] = useState<Kind>("level");
  const [uni, setUni] = useState(0);
  const [level, setLevel] = useState(255);
  const [chan, setChan] = useState(1);
  const testing = !!rt.value?.testing;
  const universes = show.value?.universes ?? [];
  const u = uni === 0 ? null : uni;
  const mode = (lv = level): TestMode =>
    kind === "level" ? { type: "level", universe: u, level: lv }
    : kind === "chase" ? { type: "chase", universe: u }
    : { type: "channel", universe: uni || universes[0]?.id || 1, channel: chan, level: lv };
  const start = () => send({ type: "output_test", mode: mode() }).catch(() => {});
  return (
    <div class="panel" style={{ margin: "8px 12px 0" }}>
      <div class="panel-head row" style={{ gap: 8 }}>
        <span class="grow">{t("Output test")}</span>
        {testing && <span class="chip warn" style={{ minHeight: 28 }}>{t("TEST ACTIVE")}</span>}
      </div>
      <div class="panel-body col" style={{ gap: 10 }}>
        <div class="dim" style={{ fontSize: "var(--fs-sm)" }}>{t("Overrides the output with a test pattern to check cabling and receivers. It stops by itself after 5 minutes.")}</div>
        <div class="row" style={{ gap: 12, flexWrap: "wrap", alignItems: "flex-end" }}>
          <Segmented<Kind> value={kind} onChange={setKind} label={t("Test pattern")}
            options={[{ value: "level", label: t("All at level") }, { value: "chase", label: t("Channel chase") }, { value: "channel", label: t("Single channel") }]} />
          <Select<number> label={t("Universe")} value={uni} onChange={setUni}
            options={[...(kind === "channel" ? [] : [{ value: 0, label: t("All universes") }]), ...universes.map((x) => ({ value: x.id, label: `${x.id} · ${x.name}` }))]} />
          {kind === "channel" && <NumberField label={t("Channel")} width={110} value={chan} min={1} max={512} onChange={setChan} />}
        </div>
        {kind !== "chase" && (
          <div class="field"><label>{t("Level")}: {Math.round((level / 255) * 100)}%</label>
            <input type="range" min={0} max={255} value={level} style={{ height: 44 }} aria-label={t("Level")}
              onInput={(e) => { const v = Number((e.target as HTMLInputElement).value); setLevel(v); if (testing) fire({ type: "output_test", mode: mode(v) }); }} /></div>
        )}
        <div class="row" style={{ gap: 8, flexWrap: "wrap" }}>
          <button class="btn tall primary" style={{ minHeight: 44 }} onClick={start}>{testing ? t("Apply") : t("Start test")}</button>
          <button class="btn tall danger" style={{ minHeight: 44 }} disabled={!testing} onClick={stopTest}>{t("Stop test")}</button>
        </div>
      </div>
    </div>
  );
}
