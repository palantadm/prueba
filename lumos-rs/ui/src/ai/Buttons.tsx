// Top-bar entry points: the assistant button and the context-help button.
import { workspaces } from "../workspaces";
import { t } from "../i18n";
import { Icon } from "../ui/Icon";
import { ASSISTANT_ICON } from "./Panel";
import { aiWorkspace, askAssistant, panelOpen } from "./state";

export function AiButton({ compact }: { compact?: boolean }) {
  const on = panelOpen.value;
  return (
    <button class={`btn tall ${on ? "sel" : ""}`} aria-pressed={on} onClick={() => { panelOpen.value = !panelOpen.value; }}
      title={`${t("Assistant")} (Ctrl+K)`} aria-label={t("Assistant")}>
      <Icon d={ASSISTANT_ICON} size={20} />
      {!compact && <span class="hide-mid">{t("Assistant")}</span>}
    </button>
  );
}

/** Opens the assistant with “explain this screen” prefilled (not sent: the user can edit it first). */
export function HelpButton() {
  const label = workspaces.find((w) => w.id === aiWorkspace.value)?.label ?? "";
  const ask = () => askAssistant(t("Explain this screen to me (“{ws}”): what it is for and how to use it.", { ws: label }), { send: false });
  return (
    <button class="btn tall ghost ai-help" onClick={ask} title={t("Ask the assistant about this screen")} aria-label={t("Ask the assistant about this screen")}>
      <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M4 5h16v11H10l-5 4v-4H4z" /><path d="M10 9.5a2 2 0 114 0c0 1.4-2 1.5-2 3M12 14.6h.01" />
      </svg>
      <span class="hide-mid">{t("Help")}</span>
    </button>
  );
}
