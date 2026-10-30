import { useEffect } from "preact/hooks";
import { rt, send } from "../store";
import { t } from "../i18n";

/** Header controls for programming: undo / redo (server-side history) and BLIND mode. */
export function ProgChips() {
  const r = rt.value;
  const undo = r?.undo ?? 0, redo = r?.redo ?? 0, blind = r?.blind ?? false;
  const btn = { minHeight: 48, minWidth: 48, position: "relative" as const };
  const ic = (d: string) => <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={d} /></svg>;
  const badge = (n: number) => n > 0 && <span class="chip" style={{ height: 16, padding: "0 5px", marginLeft: 4, fontSize: 10 }}>{n}</span>;
  return (
    <div class="row" style={{ gap: 6 }} role="group" aria-label={t("History and blind")}>
      <button class="btn tall" style={btn} disabled={!undo} aria-label={t("Undo")} title={t("Undo (Ctrl+Z)")} onClick={() => send({ type: "undo" }).catch(() => {})}>{ic("M9 14 4 9l5-5M4 9h11a5 5 0 010 10h-3")}{badge(undo)}</button>
      <button class="btn tall" style={btn} disabled={!redo} aria-label={t("Redo")} title={t("Redo (Ctrl+Y)")} onClick={() => send({ type: "redo" }).catch(() => {})}>{ic("M15 14l5-5-5-5M20 9H9a5 5 0 000 10h3")}{badge(redo)}</button>
      <button class={`btn tall ${blind ? "blind-on" : ""}`} style={btn} aria-pressed={blind} title={t("Blind: edit the programmer without sending it to the output")}
        onClick={() => send({ type: "blind_set", on: !blind }).catch(() => {})}>{ic("M3 3l18 18M10.6 6.1A10 10 0 0112 6c5 0 9 6 9 6a15 15 0 01-3.2 3.7M6.6 6.6A15 15 0 003 12s4 6 9 6a9 9 0 004-1")}<span class="hide-mid">Blind</span></button>
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
