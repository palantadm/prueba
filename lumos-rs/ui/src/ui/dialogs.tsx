import { signal } from "@preact/signals";
import { useState } from "preact/hooks";
import { Modal } from "./Modal";
import { t } from "../i18n";

type Dlg =
  | { kind: "confirm"; title: string; message: string; danger?: boolean; ok: string; resolve: (v: boolean) => void }
  | { kind: "prompt"; title: string; label: string; value: string; resolve: (v: string | null) => void };
const dlg = signal<Dlg | null>(null);

export function confirmDialog(title: string, message: string, opts: { danger?: boolean; ok?: string } = {}): Promise<boolean> {
  return new Promise((resolve) => (dlg.value = { kind: "confirm", title, message, danger: opts.danger, ok: opts.ok ?? t("OK"), resolve }));
}
export function promptDialog(title: string, label: string, value = ""): Promise<string | null> {
  return new Promise((resolve) => (dlg.value = { kind: "prompt", title, label, value, resolve }));
}

function PromptBody(p: { d: Extract<Dlg, { kind: "prompt" }> }) {
  const [v, setV] = useState(p.d.value);
  const done = (r: string | null) => { dlg.value = null; p.d.resolve(r); };
  return (
    <Modal title={p.d.title} onClose={() => done(null)}
      footer={<><button class="btn" onClick={() => done(null)}>{t("Cancel")}</button><button class="btn primary" disabled={!v.trim()} onClick={() => done(v.trim())}>{t("OK")}</button></>}>
      <div class="field"><label>{p.d.label}</label>
        <input class="input" autofocus value={v} onInput={(e) => setV((e.target as HTMLInputElement).value)}
          onKeyDown={(e) => e.key === "Enter" && v.trim() && done(v.trim())} /></div>
    </Modal>
  );
}

/** Render once in App. */
export function DialogHost() {
  const d = dlg.value;
  if (!d) return null;
  if (d.kind === "prompt") return <PromptBody d={d} />;
  const done = (r: boolean) => { dlg.value = null; d.resolve(r); };
  return (
    <Modal title={d.title} onClose={() => done(false)}
      footer={<><button class="btn" onClick={() => done(false)}>{t("Cancel")}</button><button class={`btn ${d.danger ? "danger" : "primary"}`} onClick={() => done(true)}>{d.ok}</button></>}>
      <div>{d.message}</div>
    </Modal>
  );
}
