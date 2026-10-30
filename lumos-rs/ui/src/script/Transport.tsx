import { useEffect, useState } from "preact/hooks";
import { send, show } from "../store";
import { confirmDialog } from "../ui";
import { t } from "../i18n";
import type { Script, ScriptRt } from "../types";
import { kindLabel, fmtClock, marksSummary, position } from "./util";

export function goScript(script: Script) {
  void send({ type: "script_go", id: script.id });
}
export function backScript(script: Script) {
  void send({ type: "script_back", id: script.id });
}

export function Transport(props: { script: Script; rt?: ScriptRt }) {
  const { script, rt } = props;
  const pos = position(script, rt);
  const [now, setNow] = useState(() => new Date());
  useEffect(() => { const i = setInterval(() => setNow(new Date()), 1000); return () => clearInterval(i); }, []);
  const nx = pos.next;
  const reset = async () => {
    if (await confirmDialog(t("Restart the script?"), t("Goes back to the beginning. Marks are not fired."), { ok: t("Restart") })) void send({ type: "script_reset", id: script.id });
  };
  return (
    <div class="sc-transport" role="group" aria-label={t("Script transport")}>
      <div class="sc-tr-btns">
        <button class="btn lg sc-back" onClick={() => backScript(script)} aria-keyshortcuts="Backspace">◀ {t("Step back")}</button>
        <button class="btn lg go sc-go" onClick={() => goScript(script)} disabled={!nx} aria-keyshortcuts="Space">▶ GO</button>
      </div>
      <div class="sc-tr-info" aria-live="polite">
        {nx ? (
          <div class={`sc-next ${pos.standby ? "standby" : ""}`}>
            {pos.standby ? "STANDBY · " : `${t("Next")}: `}{nx.mark.label || kindLabel(nx.mark.kind)}
            <small>{marksSummary(nx.mark, show.value)}</small>
          </div>
        ) : (
          <div class="sc-next dim">{script.blocks.some((b) => b.marks.length) ? t("End of the script") : t("No cues marked yet — use Edit")}</div>
        )}
        <div class="sc-last">{rt?.last_label ? `${t("Last")}: ${rt.last_label}` : t("Nothing fired yet")}</div>
      </div>
      <div class="sc-tr-right">
        <button class="switch-row sc-follow" role="switch" aria-checked={!!rt?.follow} title={t("When a linked Lumos cue is fired by hand, the script moves to its mark")} onClick={() => void send({ type: "script_follow", id: script.id, on: !rt?.follow })}>
          <span class={`switch ${rt?.follow ? "on" : ""}`} />
          {t("Follow Lumos")}
        </button>
        <button class="btn" onClick={reset}>{t("Restart")}</button>
        <span class="sc-clock" aria-label={t("Clock")}>{fmtClock(now)}</span>
      </div>
    </div>
  );
}
