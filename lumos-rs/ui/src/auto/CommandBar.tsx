import "../styles/auto.css";
import { useEffect, useRef, useState } from "preact/hooks";
import { rt, send } from "../store";
import { Modal } from "../ui";
import { t } from "../i18n";
import { tErr } from "../i18n/errors";
import {
  KEYPAD, appendToken, applySuggestion, helpData, helpSections, keywords, loadHelp, softKeys, suggest,
} from "./grammar";
import {
  cmdError, cmdFocus, cmdHistory, cmdLogOpen, cmdOpen, cmdSoftOpen, cmdText, pushHistory, setCmdOpen, setLogOpen, setSoftOpen, toggleCmd,
} from "./state";

/** Header toggle for the command bar. */
export function CmdChip() {
  const last = rt.value?.cmd_log?.[rt.value.cmd_log.length - 1];
  return (
    <button class={`btn tall ghost ${cmdOpen.value ? "sel" : ""}`} onClick={toggleCmd} aria-pressed={cmdOpen.value}
      title={t("Command line (/)")} aria-label={t("Command line")}>
      <span class="mono" aria-hidden="true">&gt;_</span>
      <span class="hide-mid">{t("Command line")}</span>
      {last && !last.ok && <span class="dot err" role="img" aria-label={t("Last command failed")} />}
    </button>
  );
}

function HelpSheet({ onClose }: { onClose: () => void }) {
  const h = helpData.value;
  const [kw, setKw] = useState(false);
  return (
    <Modal title={t("Command line help")} onClose={onClose} width={760} footer={<button class="btn tall primary" onClick={onClose}>{t("Close")}</button>}>
      <div class="dim">{t("Type statements separated by ; or newlines. Keywords work in English and Spanish. Fixture selection stays active for the following statements in the same line.")}</div>
      {helpSections().map((s) => (
        <div key={s.title} class="panel">
          <div class="panel-head">{s.title}</div>
          <table class="tbl"><tbody>
            {s.rows.map(([a, b]) => (
              <tr key={a}>
                <td><code class="au-code">{a}</code></td>
                <td class="dim">{b}</td>
                <td style={{ width: 90 }}>
                  <button class="btn sm au-use" onClick={() => { cmdText.value = a.split(" · ")[0].replace(/\[.*?\]/g, "").trim(); setCmdOpen(true); onClose(); }}>{t("Use")}</button>
                </td>
              </tr>
            ))}
          </tbody></table>
        </div>
      ))}
      {h && h.statements.length > 0 && (
        <div class="panel">
          <div class="panel-head">{t("Engine syntax")}</div>
          <table class="tbl"><tbody>
            {h.statements.map((s) => (
              <tr key={s.name}><td><code class="au-code">{s.syntax}</code></td><td class="dim">{s.description}</td>
                <td style={{ width: 90 }}>{s.examples[0] && <button class="btn sm au-use" onClick={() => { cmdText.value = s.examples[0]; setCmdOpen(true); onClose(); }}>{t("Use")}</button>}</td></tr>
            ))}
          </tbody></table>
        </div>
      )}
      {h && h.statements.length === 0 && h.examples.length > 0 && (
        <div class="panel">
          <div class="panel-head">{t("Examples from the engine")}</div>
          <table class="tbl"><tbody>
            {h.examples.map((e) => (
              <tr key={e.text}><td><code class="au-code">{e.text}</code></td><td class="dim">{e.desc}</td>
                <td style={{ width: 90 }}><button class="btn sm au-use" onClick={() => { cmdText.value = e.text; setCmdOpen(true); onClose(); }}>{t("Use")}</button></td></tr>
            ))}
          </tbody></table>
        </div>
      )}
      <div>
        <button class="btn tall" aria-expanded={kw} onClick={() => setKw(!kw)}>{kw ? t("Hide keywords") : t("Show all keywords")}</button>
        {kw && <div class="row wrap" style={{ marginTop: 8, gap: 6 }}>{keywords().map((k) => <span key={k} class="chip mono">{k}</span>)}</div>}
      </div>
    </Modal>
  );
}

export function CommandBar() {
  const input = useRef<HTMLInputElement>(null);
  const [sel, setSel] = useState(-1);
  const [hide, setHide] = useState(false);
  const [help, setHelp] = useState(false);
  const histIdx = useRef(-1);
  const draft = useRef("");
  const logEl = useRef<HTMLDivElement>(null);

  useEffect(() => { if (cmdOpen.value) void loadHelp(); }, [cmdOpen.value]);
  useEffect(() => { if (cmdOpen.value) input.current?.focus(); }, [cmdFocus.value]);

  const log = rt.value?.cmd_log ?? [];
  useEffect(() => { logEl.current?.scrollTo({ top: 1e9 }); }, [log.length, cmdLogOpen.value, cmdError.value]);

  if (!cmdOpen.value) return null;

  const text = cmdText.value;
  const sugg = hide ? [] : suggest(text);
  const set = (v: string) => { cmdText.value = v; cmdError.value = ""; setSel(-1); setHide(false); histIdx.current = -1; };

  const submit = async () => {
    const line = text.trim();
    if (!line) return;
    pushHistory(line);
    histIdx.current = -1;
    cmdError.value = "";
    try {
      await send({ type: "command_line", text: line }, { quiet: true });
      cmdText.value = "";
    } catch (e) {
      cmdError.value = tErr((e as Error).message);
    }
  };

  const onKey = (e: KeyboardEvent) => {
    e.stopPropagation();
    const h = cmdHistory.value;
    if (e.key === "Enter") {
      e.preventDefault();
      if (sugg.length && sel >= 0) { cmdText.value = applySuggestion(text, sugg[sel]); setSel(-1); return; }
      void submit();
    } else if (e.key === "Tab") {
      if (sugg.length) { e.preventDefault(); cmdText.value = applySuggestion(text, sugg[Math.max(0, sel)]); setSel(-1); }
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const d = e.key === "ArrowDown" ? 1 : -1;
      if (sugg.length) { setSel((sel + d + sugg.length) % sugg.length); return; }
      if (!h.length) return;
      if (histIdx.current === -1) { if (d > 0) return; draft.current = text; histIdx.current = h.length - 1; }
      else histIdx.current += d;
      if (histIdx.current >= h.length) { histIdx.current = -1; cmdText.value = draft.current; return; }
      histIdx.current = Math.max(0, histIdx.current);
      cmdText.value = h[histIdx.current];
    } else if (e.key === "Escape") {
      if (sugg.length) setHide(true);
      else if (text) set("");
      else { input.current?.blur(); setCmdOpen(false); }
    }
  };

  const tok = (s: string) => { set(appendToken(cmdText.value, s)); if (!cmdSoftOpen.value) input.current?.focus(); };
  const keep = (e: Event) => e.preventDefault(); // keep the input focused while tapping soft keys
  const lines = cmdLogOpen.value ? log.slice(-30) : log.slice(-1);
  const kinds = softKeys();

  return (
    <section class="cmdbar" aria-label={t("Command line")}>
      {(lines.length > 0 || cmdError.value) && (
        <div class={`cmd-log ${cmdLogOpen.value ? "open" : ""}`} ref={logEl} role="log" aria-live="polite">
          {lines.map((l, i) => (
            <div key={`${log.length - lines.length + i}`} class={`cmd-line ${l.ok ? "ok" : "err"}`}>
              <span aria-hidden="true">{l.ok ? "✓" : "✕"}</span>
              <span class="mono">{l.text}</span>
              {l.msg && <span class="dim"> — {l.ok ? l.msg : tErr(l.msg)}</span>}
            </div>
          ))}
          {cmdError.value && <div class="cmd-line err"><span aria-hidden="true">✕</span><span>{cmdError.value}</span></div>}
        </div>
      )}
      <div class="cmd-row">
        <span class="mono cmd-prompt" aria-hidden="true">&gt;</span>
        <div class="grow" style={{ position: "relative" }}>
          <input ref={input} class="input cmd-input" value={text} spellcheck={false} autocomplete="off" autocapitalize="off"
            inputMode={cmdSoftOpen.value ? "none" : "text"} aria-label={t("Command line")}
            placeholder={t("fixture 1 thru 10 at 50 …   (Tab = complete, ↑↓ = history)")}
            onInput={(e) => set((e.target as HTMLInputElement).value)} onKeyDown={onKey} />
          {sugg.length > 0 && (
            <div class="cmd-pop" role="listbox" aria-label={t("Suggestions")}>
              {sugg.map((s, i) => (
                <button key={s} role="option" aria-selected={i === sel} class={`cmd-sug mono ${i === sel ? "sel" : ""}`}
                  onMouseDown={keep} onClick={() => { cmdText.value = applySuggestion(text, s); setSel(-1); input.current?.focus(); }}>{s}</button>
              ))}
            </div>
          )}
        </div>
        <button class="btn au-btn go" onMouseDown={keep} onClick={() => void submit()} disabled={!text.trim()} aria-label={t("Send command")}>↵</button>
        <button class="btn au-btn" onMouseDown={keep} onClick={() => set("")} disabled={!text} aria-label={t("Clear line")} title={t("Clear line")}>⌫</button>
        <button class={`btn au-btn ${cmdSoftOpen.value ? "sel" : ""}`} aria-pressed={cmdSoftOpen.value} onMouseDown={keep} onClick={() => setSoftOpen(!cmdSoftOpen.value)} title={t("On-screen keys")} aria-label={t("On-screen keys")}>⌨</button>
        <button class={`btn au-btn ${cmdLogOpen.value ? "sel" : ""}`} aria-pressed={cmdLogOpen.value} onMouseDown={keep} onClick={() => setLogOpen(!cmdLogOpen.value)} title={t("Command log")} aria-label={t("Command log")}>☰</button>
        <button class="btn au-btn" onClick={() => setHelp(true)} title={t("Help")} aria-label={t("Help")}>?</button>
        <button class="btn au-btn ghost" onClick={() => setCmdOpen(false)} title={t("Hide command line")} aria-label={t("Hide command line")}>▾</button>
      </div>
      {cmdSoftOpen.value && (
        <div class="cmd-soft" role="group" aria-label={t("On-screen keys")}>
          <div class="cmd-keypad">
            {KEYPAD.map((k) => <button key={k} class="btn au-key" onMouseDown={keep} onClick={() => tok(k)}>{k}</button>)}
          </div>
          <div class="cmd-keys">
            {kinds.map((k) => <button key={k.tok + k.label} class={`btn au-key ${k.kind ?? ""}`} onMouseDown={keep} onClick={() => tok(k.tok)}>{k.label}</button>)}
          </div>
        </div>
      )}
      {help && <HelpSheet onClose={() => setHelp(false)} />}
    </section>
  );
}
