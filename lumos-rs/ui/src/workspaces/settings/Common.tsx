import type { ComponentChildren } from "preact";
import { PageHead } from "../../ui";
import { t } from "../../i18n";

export function SaveBar(props: { dirty: boolean; save: () => void; revert: () => void }) {
  return (
    <div class="dirty-bar" role="status">
      <span class={`chip ${props.dirty ? "warn" : "ok"}`}>{props.dirty ? t("Unsaved changes") : t("Saved")}</span>
      <button class="btn tall" disabled={!props.dirty} onClick={props.revert}>{t("Revert")}</button>
      <button class="btn tall primary" disabled={!props.dirty} onClick={props.save}>{t("Save")}</button>
    </div>
  );
}

export function SettingsPage(props: { title: string; sub?: string; bar?: ComponentChildren; children: ComponentChildren }) {
  return (
    <div class="page">
      <PageHead title={props.title} sub={props.sub}>{props.bar}</PageHead>
      <div class="page-body col" style={{ gap: 16, maxWidth: 900 }}>{props.children}</div>
    </div>
  );
}
