import type { Fixture } from "../../types";
import { rt, send, show, toast } from "../../store";
import { Modal } from "../../ui";
import { t } from "../../i18n";

/** Per-model "locate" look: what the Identify button makes this fixture do. */
export function LocateDialog(props: { fixture: Fixture; onClose: () => void }) {
  const f = props.fixture;
  const saved = show.value?.locate?.[f.profile];
  const held = rt.value?.programmer?.[f.id] ?? {};
  const identifying = (rt.value?.identifying ?? []).includes(f.id);
  const fmt = (a: Record<string, number | undefined>) =>
    Object.entries(a).map(([k, v]) => `${k} ${Math.round((v ?? 0) * 100)}%`).join(" · ");
  const heldKeys = Object.keys(held);
  const test = () => send({ type: "identify", fixture: f.id, on: !identifying }, { quiet: true }).catch(() => {});
  const capture = () =>
    send({ type: "locate_capture", fixture: f.id }).then(() => toast(t("Locate look saved for {p}", { p: f.profile }), "ok")).catch(() => {});
  const reset = () =>
    send({ type: "locate_save", profile: f.profile, attrs: null }).then(() => toast(t("Back to automatic locate"), "ok")).catch(() => {});
  return (
    <Modal
      title={t("Locate look — {p}", { p: f.profile })}
      onClose={props.onClose}
      width={620}
      footer={<button class="btn lg" onClick={props.onClose}>{t("Close")}</button>}
    >
      <p class="dim" style={{ margin: 0 }}>
        {t("Some fixtures need more than the dimmer to emit light: a colour channel, an open shutter, a lamp-on value on the control channel… Set it once for this model and every fixture of the same model will use it.")}
      </p>
      <ol class="dim" style={{ margin: 0, paddingLeft: 20, display: "flex", flexDirection: "column", gap: 6 }}>
        <li>{t("Press “Test locate” and look at the fixture.")}</li>
        <li>{t("If it stays dark, go to Live, select fixture {id} and set values until it lights (dimmer, colour or white, shutter open, control channel).", { id: f.id })}</li>
        <li>{t("Come back here and save the values held in the programmer as this model's locate look.")}</li>
      </ol>
      <div class="row wrap">
        <button class={`btn lg ${identifying ? "on" : ""}`} onClick={test}>{identifying ? t("Stop test") : t("Test locate")}</button>
        <button class="btn lg primary" disabled={!heldKeys.length} onClick={capture}>{t("Save programmer values as locate look")}</button>
        <button class="btn lg" disabled={!saved} onClick={reset}>{t("Back to automatic")}</button>
      </div>
      <div>
        <div class="label">{t("Held in the programmer for fixture {id}", { id: f.id })}</div>
        <div class="mono" style={{ marginTop: 6 }}>{heldKeys.length ? fmt(held) : <span class="dim">{t("nothing held")}</span>}</div>
      </div>
      <div>
        <div class="label">{t("Saved locate look for this model")}</div>
        <div class="mono" style={{ marginTop: 6 }}>{saved ? fmt(saved) : <span class="dim">{t("automatic")}</span>}</div>
      </div>
    </Modal>
  );
}
