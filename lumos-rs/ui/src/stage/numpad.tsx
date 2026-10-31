// Touch numeric keypad sheet: opens when a 3D property cell is focused on a coarse pointer.
import { signal } from "@preact/signals";
import { useEffect, useState } from "preact/hooks";
import { t } from "../i18n";

interface PadReq { label: string; value: number; step: number; apply: (v: number) => void }
const padReq = signal<PadReq | null>(null);

export const coarsePointer = (): boolean => typeof window !== "undefined" && !!window.matchMedia?.("(pointer: coarse)").matches;
export function openPad(req: PadReq) { padReq.value = req; }

const KEYS = ["7", "8", "9", "4", "5", "6", "1", "2", "3", "±", "0", "."];

export function NumPadHost() {
  const req = padReq.value;
  const [txt, setTxt] = useState("");
  useEffect(() => { if (req) setTxt(String(Math.round(req.value * 1000) / 1000)); }, [req]);
  if (!req) return null;
  const close = () => (padReq.value = null);
  const press = (k: string) => {
    if (k === "±") setTxt((s) => (s.startsWith("-") ? s.slice(1) : "-" + s));
    else if (k === ".") setTxt((s) => (s.includes(".") ? s : (s === "" || s === "-" ? s + "0" : s) + "."));
    else setTxt((s) => (s === "0" ? k : s === "-0" ? "-" + k : s + k));
  };
  const nudge = (d: number) => { const n = parseFloat(txt); setTxt(String(Math.round(((Number.isFinite(n) ? n : 0) + d) * 1000) / 1000)); };
  const ok = () => { const n = parseFloat(txt); if (Number.isFinite(n)) req.apply(n); close(); };
  return (
    <div class="st-pad-back" onPointerDown={close}>
      <div class="st-pad" role="dialog" aria-label={req.label} onPointerDown={(e) => e.stopPropagation()}>
        <div class="st-h">{req.label}</div>
        <div class="st-pad-disp mono">{txt === "" ? "0" : txt}</div>
        <div class="st-pad-step">
          <button class="btn" onClick={() => nudge(-req.step)}>− {req.step}</button>
          <button class="btn" onClick={() => nudge(req.step)}>+ {req.step}</button>
          <button class="btn" aria-label={t("Delete")} onClick={() => setTxt((s) => s.slice(0, -1))}>⌫</button>
        </div>
        <div class="st-pad-keys">
          {KEYS.map((k) => <button key={k} class="btn lg mono" onClick={() => press(k)}>{k}</button>)}
        </div>
        <div class="st-pad-foot">
          <button class="btn lg" onClick={close}>{t("Cancel")}</button>
          <button class="btn lg cyan" onClick={ok}>{t("Apply")}</button>
        </div>
      </div>
    </div>
  );
}
