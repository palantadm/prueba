import { useState } from "preact/hooks";
import { rt, send, toast } from "../store";
import { Modal } from "../ui";
import { t } from "../i18n";

/* Console lock: a PIN freezes the console for every screen (and the assistant) until it is entered. Emergency blackout stays on. */

const fail = (e: unknown) => String((e as Error)?.message ?? e);

function Keypad(props: { value: string; onChange: (v: string) => void; onEnter?: () => void }) {
  const press = (d: string) => props.onChange((props.value + d).slice(0, 12));
  return (
    <div class="lk-pad">
      <div class="lk-dots" aria-live="polite" aria-label={t("PIN")}>{props.value ? "●".repeat(props.value.length) : "····"}</div>
      <div class="lk-keys">
        {["1", "2", "3", "4", "5", "6", "7", "8", "9"].map((d) => <button key={d} class="btn tall" onClick={() => press(d)}>{d}</button>)}
        <button class="btn tall" aria-label={t("Delete")} onClick={() => props.onChange(props.value.slice(0, -1))}>⌫</button>
        <button class="btn tall" onClick={() => press("0")}>0</button>
        <button class="btn tall primary" disabled={!props.onEnter} onClick={props.onEnter}>OK</button>
      </div>
    </div>
  );
}

export function LockOverlay() {
  const l = rt.value?.lock;
  const [pin, setPin] = useState("");
  const [err, setErr] = useState("");
  if (!l?.locked) return null;
  const go = () => void send({ type: "unlock", pin }, { quiet: true }).then(() => { setPin(""); setErr(""); }).catch((e) => { setErr(fail(e)); setPin(""); });
  return (
    <div class="lk-overlay" role="dialog" aria-modal="true" aria-label={t("Console locked")}>
      <div class="lk-box">
        <div class="lk-title">🔒 {t("Console locked")}</div>
        <Keypad value={pin} onChange={setPin} onEnter={pin.length >= 4 ? go : undefined} />
        {err && <div class="lk-err" role="alert">{err}</div>}
        <button class="btn tall danger" onClick={() => void send({ type: "blackout", on: !(rt.value?.blackout ?? false) })}>{rt.value?.blackout ? t("Blackout is ON — release") : t("Emergency blackout")}</button>
      </div>
    </div>
  );
}

export function LockButton() {
  const l = rt.value?.lock;
  const [dlg, setDlg] = useState(false);
  const click = () => { if (l?.pin_set) void send({ type: "lock" }).catch((e) => toast(fail(e), "err")); else setDlg(true); };
  return (
    <>
      <button class="btn icon ghost" aria-label={t("Lock the console")} title={l?.pin_set ? t("Lock the console") : t("Set a PIN to lock the console")} onClick={click} onContextMenu={(e) => { e.preventDefault(); setDlg(true); }}>🔒</button>
      {dlg && <PinDialog onClose={() => setDlg(false)} />}
    </>
  );
}

/** Set, change or remove the PIN. */
export function PinDialog(props: { onClose: () => void }) {
  const has = rt.value?.lock?.pin_set ?? false;
  const [step, setStep] = useState<"cur" | "new" | "again">(has ? "cur" : "new");
  const [cur, setCur] = useState("");
  const [pin, setPin] = useState("");
  const [again, setAgain] = useState("");
  const [err, setErr] = useState("");
  const save = (newPin: string) => void send({ type: "lock_set", pin: cur, new_pin: newPin }, { quiet: true }).then(() => { toast(newPin ? t("PIN saved") : t("PIN removed"), "ok", 2000); props.onClose(); }).catch((e) => { setErr(fail(e)); });
  const label = step === "cur" ? t("Current PIN") : step === "new" ? t("New PIN (4–12 digits)") : t("Repeat the new PIN");
  const value = step === "cur" ? cur : step === "new" ? pin : again;
  const set = step === "cur" ? setCur : step === "new" ? setPin : setAgain;
  const next = () => {
    setErr("");
    if (step === "cur") setStep("new");
    else if (step === "new") setStep("again");
    else if (pin !== again) { setErr(t("The PINs do not match")); setAgain(""); setStep("new"); setPin(""); }
    else save(pin);
  };
  return (
    <Modal title={t("Console PIN")} width={420} onClose={props.onClose}
      footer={<>{has && <button class="btn tall danger" disabled={!cur} onClick={() => save("")} title={t("Enter the current PIN first")}>{t("Remove PIN")}</button>}<span class="grow" /><button class="btn tall" onClick={props.onClose}>{t("Cancel")}</button></>}>
      <div class="col" style={{ gap: 10, alignItems: "center" }}>
        <div>{label}</div>
        <Keypad value={value} onChange={set} onEnter={value.length >= (step === "cur" ? 1 : 4) ? next : undefined} />
        {err && <div style={{ color: "var(--red)" }} role="alert">{err}</div>}
        <div class="dim">{t("With a PIN set, the lock button freezes every screen and the assistant until the PIN is entered. Emergency blackout always works.")}</div>
      </div>
    </Modal>
  );
}
