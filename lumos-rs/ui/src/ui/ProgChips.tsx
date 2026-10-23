import { useEffect } from "preact/hooks";
import { rt, send } from "../store";
import { t } from "../i18n";

/** Header controls for programming: undo / redo (server-side history) and BLIND mode. */
export function ProgChips() {
  const r = rt.value;
  const undo = r?.undo ?? 0, redo = r?.redo ?? 0, blind = r?.blind ?? false;
  const btn = { minHeight: 44, minWidth: 44, position: "relative" as const };
  const badge = (n: number) => n > 0 && <span class="chip" style={{ height: 16, padding: "0 5px", marginLeft: 4, fontSize: 10 }}>{n}</span>;
  return (
    <div class="row" style={{ gap: 4 }} role="group" aria-label={t("History and blind")}>
      <button class="btn tall" style={btn} disabled={!undo} aria-label={t("Undo")} title={t("Undo (Ctrl+Z)")} onClick={() => send({ type: "undo" }).catch(() => {})}>↶{badge(undo)}</button>
      <button class="btn tall" style={btn} disabled={!redo} aria-label={t("Redo")} title={t("Redo (Ctrl+Y)")} onClick={() => send({ type: "redo" }).catch(() => {})}>↷{badge(redo)}</button>
      <button class={`btn tall ${blind ? "blind-on" : ""}`} style={btn} aria-pressed={blind} title={t("Blind: edit the programmer without sending it to the output")}
        onClick={() => send({ type: "blind_set", on: !blind }).catch(() => {})}>BLIND</button>
    </div>
  );
}

export function BlindBanner() {
  const on = !!rt.value?.blind;
  useEffect(() => {
    document.body.classList.toggle("blind", on);
    return () => document.body.classList.remove("blind");
  }, [on]);
  if (!on) return null;
  return <div class="blind-banner" role="status">{t("BLIND MODE — the output does not include the programmer")}</div>;
}
