import type { ComponentChildren } from "preact";
import { PageHead } from "../../ui";

export function SaveBar(props: { dirty: boolean; save: () => void; revert: () => void }) {
  return (
    <div class="dirty-bar" role="status">
      <span class={`chip ${props.dirty ? "warn" : "ok"}`}>{props.dirty ? "Unsaved changes" : "Saved"}</span>
      <button class="btn tall" disabled={!props.dirty} onClick={props.revert}>Revert</button>
      <button class="btn tall primary" disabled={!props.dirty} onClick={props.save}>Save</button>
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
