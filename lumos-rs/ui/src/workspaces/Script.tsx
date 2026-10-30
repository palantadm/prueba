import "../styles/script.css";
import { useEffect, useState } from "preact/hooks";
import { rt, send, show, toast } from "../store";
import { EmptyState, Select, confirmDialog, promptDialog } from "../ui";
import { useKey } from "../lib/hooks";
import { t } from "../i18n";
import { Reader } from "../script/Reader";
import { Transport, backScript, goScript } from "../script/Transport";
import { ImportSheet } from "../script/ImportSheet";
import { MarkEditorHost, BlockEditorHost } from "../script/MarkEditor";
import { PdfPanel } from "../script/PdfPanel";
import { VoiceBar, useVoiceFollow, voiceSupported } from "../script/Voice";
import { ApiError, suggestMarks } from "../script/api";
import { activeId, currentScript, editMode, fontSize, pdfOpen, setActive, setFontSize, setPdfOpen, sel, moving, suggestions, visibleScripts, saveScript } from "../script/store";
import { blankScript } from "../script/util";

export function Script() {
  const list = visibleScripts();
  const cur = currentScript();
  const [importing, setImporting] = useState(false);
  const [suggesting, setSuggesting] = useState(false);
  const [voice, setVoice] = useState(false);
  const [voiceFire, setVoiceFire] = useState(false);
  const srt = rt.value?.scripts.find((s) => s.id === cur?.id);

  useEffect(() => {
    if (!cur && list.length) setActive(list[0].id);
    else if (cur && activeId.value !== cur.id) setActive(cur.id);
  }, [cur?.id, list.length]);
  useEffect(() => { if (!editMode.value) { sel.value = null; moving.value = null; suggestions.value = []; } }, [editMode.value]);
  useEffect(() => { suggestions.value = []; }, [cur?.id]);

  const voiceState = useVoiceFollow(cur ?? { id: 0, name: "", blocks: [], pdf: null }, srt, voice && !!cur, voiceFire);

  let lastGo = 0;
  const modalOpen = () => !!document.querySelector(".modal-back");
  useKey((e) => {
    if (!cur || modalOpen() || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.code === "Space") { e.preventDefault(); const n = Date.now(); if (n - lastGo > 200) { lastGo = n; if (!editMode.value) goScript(cur); } }
    else if (e.key === "Backspace") { e.preventDefault(); if (!editMode.value) backScript(cur); }
    else if (e.key.toLowerCase() === "e") editMode.value = !editMode.value;
  }, [cur?.id]);

  const create = async () => {
    const name = await promptDialog(t("New script"), t("Name"), t("Script {n}", { n: list.length + 1 }));
    if (!name) return;
    const s = blankScript(show.value, name);
    await send({ type: "script_save", script: s }).then(() => { setActive(s.id); editMode.value = true; }).catch(() => {});
  };
  const rename = async () => {
    if (!cur) return;
    const name = await promptDialog(t("Rename script"), t("Name"), cur.name);
    if (name) saveScript({ ...cur, name }, true);
  };
  const remove = async () => {
    if (!cur) return;
    if (!(await confirmDialog(t("Remove script"), t("Remove script “{name}”, with its {n} cue(s)?", { name: cur.name, n: cur.blocks.reduce((a, b) => a + b.marks.length, 0) }), { danger: true, ok: t("Remove") }))) return;
    await send({ type: "script_remove", id: cur.id }).catch(() => {});
    setActive(null);
  };
  const suggest = async () => {
    if (!cur) return;
    setSuggesting(true);
    try {
      const s = await suggestMarks(cur.blocks);
      suggestions.value = s;
      toast(s.length ? t("{n} suggested cue(s) — accept or discard each one", { n: s.length }) : t("No cues suggested"), s.length ? "ok" : "info");
    } catch (e) {
      toast(e instanceof ApiError ? e.message : t("Could not get suggestions"), "err");
    } finally { setSuggesting(false); }
  };

  const head = (
    <div class="sc-head">
      {list.length > 0 && (
        <Select<number> class="sc-pick" label={t("Script")} value={cur?.id ?? 0} options={list.map((s) => ({ value: s.id, label: s.name }))} onChange={(v) => setActive(v)} />
      )}
      <span class="grow" />
      <button class="btn" onClick={() => setImporting(true)}>⤓ {t("Import")}</button>
      <button class="btn" onClick={create}>＋ {t("New")}</button>
      {cur && (
        <>
          <button class="btn" onClick={rename}>{t("Rename")}</button>
          <button class="btn danger" onClick={remove}>{t("Remove")}</button>
          <span class="stepper" role="group" aria-label={t("Text size")}>
            <button class="btn" aria-label={t("Smaller text")} disabled={fontSize.value <= 20} onClick={() => setFontSize(fontSize.value - 4)}>A−</button>
            <button class="btn" aria-label={t("Larger text")} disabled={fontSize.value >= 48} onClick={() => setFontSize(fontSize.value + 4)}>A+</button>
          </span>
          {cur.pdf && <button class={`btn ${pdfOpen.value ? "sel" : ""}`} aria-pressed={pdfOpen.value} onClick={() => setPdfOpen(!pdfOpen.value)}>PDF</button>}
          {voiceSupported() && <button class={`btn ${voice ? "sel" : ""}`} aria-pressed={voice} title={t("Experimental: follows the actors' voice")} onClick={() => setVoice(!voice)}>🎙 {t("Voice")}</button>}
          {editMode.value && <button class="btn" disabled={suggesting} onClick={suggest}>{suggesting ? t("Analysing…") : `✦ ${t("Suggest cues")}`}</button>}
          <button class={`btn ${editMode.value ? "primary" : ""}`} aria-pressed={editMode.value} onClick={() => (editMode.value = !editMode.value)}>{editMode.value ? t("Done") : t("Edit")}</button>
        </>
      )}
    </div>
  );

  return (
    <div class="sc-root">
      {head}
      {cur ? (
        <>
          <div class="sc-body">
            <div class="sc-main">
              <Reader script={cur} rt={srt} />
              {voice && <VoiceBar heard={voiceState.heard} err={voiceState.err} fireCues={voiceFire} onFireCues={setVoiceFire} />}
            </div>
            {pdfOpen.value && cur.pdf && <PdfPanel url={cur.pdf} page={cur.blocks.find((b) => b.id === srt?.block)?.page ?? 1} />}
          </div>
          {!editMode.value && <Transport script={cur} rt={srt} />}
        </>
      ) : (
        <EmptyState icon="📜" title={t("No script yet")} hint={t("Import the script or running order (PDF, text, Fountain) to mark where each light, sound or video cue goes and follow it live.")}>
          <div class="row" style={{ gap: 12, marginTop: 12 }}>
            <button class="btn lg primary" onClick={() => setImporting(true)}>⤓ {t("Import script")}</button>
            <button class="btn lg" onClick={create}>＋ {t("Blank script")}</button>
          </div>
        </EmptyState>
      )}
      {importing && <ImportSheet onClose={() => setImporting(false)} />}
      <MarkEditorHost />
      <BlockEditorHost />
    </div>
  );
}
