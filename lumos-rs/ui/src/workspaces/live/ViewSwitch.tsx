import { attrView, setAttrView } from "../../lib/state";
import { t } from "../../i18n";

/** Faders/knobs ↔ fixture sheet switch shown in the attribute panel header. */
export function ViewSwitch() {
  const v = attrView.value;
  return (
    <div class="row" style={{ gap: 2 }} role="tablist" aria-label={t("Attribute view")}>
      <button role="tab" aria-selected={v === "faders"} class={`btn sm ${v === "faders" ? "sel" : ""}`} style={{ minHeight: 44 }} onClick={() => setAttrView("faders")}>{t("Faders")}</button>
      <button role="tab" aria-selected={v === "sheet"} class={`btn sm ${v === "sheet" ? "sel" : ""}`} style={{ minHeight: 44 }} onClick={() => setAttrView("sheet")}>{t("Sheet")}</button>
    </div>
  );
}
