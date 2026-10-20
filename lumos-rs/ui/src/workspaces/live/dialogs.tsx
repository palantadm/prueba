import { useState } from "preact/hooks";
import { Modal } from "../../ui";
import { t } from "../../i18n";

export function PromptDialog(p: {
  title: string; label: string; initial?: string; ok?: string;
  onSubmit: (v: string) => void; onClose: () => void;
  extra?: preact.ComponentChildren;
}) {
  const [v, setV] = useState(p.initial ?? "");
  const go = () => { if (v.trim()) { p.onSubmit(v.trim()); p.onClose(); } };
  return (
    <Modal title={p.title} onClose={p.onClose} footer={<>
      <button class="btn" onClick={p.onClose}>{t("Cancel")}</button>
      <button class="btn primary" onClick={go} disabled={!v.trim()}>{p.ok ?? "OK"}</button>
    </>}>
      <div class="field">
        <label>{p.label}</label>
        <input class="input" autoFocus value={v} onInput={(e) => setV((e.target as HTMLInputElement).value)}
          onKeyDown={(e) => e.key === "Enter" && go()} aria-label={p.label} />
      </div>
      {p.extra}
    </Modal>
  );
}

export function ConfirmDialog(p: { title: string; text: string; ok?: string; danger?: boolean; onYes: () => void; onClose: () => void }) {
  return (
    <Modal title={p.title} onClose={p.onClose} footer={<>
      <button class="btn" onClick={p.onClose}>{t("Cancel")}</button>
      <button class={`btn ${p.danger ? "danger" : "primary"}`} onClick={() => { p.onYes(); p.onClose(); }}>{p.ok ?? "OK"}</button>
    </>}>
      <div>{p.text}</div>
    </Modal>
  );
}
