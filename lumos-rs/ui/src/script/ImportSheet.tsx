import { useRef, useState } from "preact/hooks";
import { send, show } from "../store";
import { Modal, TextField } from "../ui";
import { t } from "../i18n";
import type { Script, ScriptImport } from "../types";
import { ApiError, importDraft, uploadMedia } from "./api";
import { setActive } from "./store";
import { SCRIPT_TEST_ID, nextId } from "./util";

type Busy = "idle" | "parsing" | "saving";

/** Import a script from a file or pasted text; shows a draft summary before saving. */
export function ImportSheet(props: { onClose: () => void }) {
  const [tab, setTab] = useState<"file" | "text">("file");
  const [text, setText] = useState("");
  const [file, setFile] = useState<File | null>(null);
  const [draft, setDraft] = useState<ScriptImport | null>(null);
  const [name, setName] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<Busy>("idle");
  const [over, setOver] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  const parse = async (src: { file?: File; text?: string }) => {
    setErr(null); setBusy("parsing"); setDraft(null);
    try {
      const d = await importDraft(src);
      if (!d.blocks.length || d.blocks.every((b) => !b.text.trim())) { setErr(t("The script is empty — nothing to import.")); return; }
      setDraft(d);
      setName(d.name || src.file?.name.replace(/\.[^.]+$/, "") || t("New script"));
    } catch (e) {
      const status = e instanceof ApiError ? e.status : 0;
      if (status === 422) setErr(t("This PDF has no selectable text (it looks like a scan). Export it with text from the original document, or paste the text instead."));
      else if (status === 413) setErr(t("The file is too large."));
      else if (status === 0) setErr(t("Could not reach the server."));
      else setErr((e as Error).message || t("Could not read the file."));
    } finally { setBusy("idle"); }
  };
  const pick = (f: File | undefined | null) => { if (!f) return; setFile(f); void parse({ file: f }); };

  const save = async () => {
    if (!draft) return;
    setBusy("saving");
    try {
      let pdf: string | null = null;
      if (file && /\.pdf$/i.test(file.name)) { try { pdf = await uploadMedia(file); } catch { /* script still usable without the viewer */ } }
      const id = nextId((show.value?.scripts ?? []).filter((s) => s.id < SCRIPT_TEST_ID));
      const script: Script = { id, name: name.trim() || draft.name || t("New script"), blocks: draft.blocks, pdf };
      await send({ type: "script_save", script });
      setActive(id);
      props.onClose();
    } catch (e) { setErr((e as Error).message); setBusy("idle"); }
  };

  const marks = draft?.blocks.reduce((n, b) => n + b.marks.length, 0) ?? 0;
  const speeches = draft?.blocks.filter((b) => b.kind === "speech").length ?? 0;
  const heads = draft?.blocks.filter((b) => b.kind === "heading").length ?? 0;

  return (
    <Modal title={t("Import script")} width={640} onClose={props.onClose}
      footer={draft
        ? <><button class="btn lg" onClick={() => setDraft(null)} disabled={busy === "saving"}>{t("Back")}</button><button class="btn lg primary" disabled={busy === "saving"} onClick={save}>{busy === "saving" ? t("Saving…") : t("Save script")}</button></>
        : <button class="btn lg" onClick={props.onClose}>{t("Cancel")}</button>}>
      {!draft && (
        <div class="col" style={{ gap: 12 }}>
          <div class="seg" role="tablist">
            <button role="tab" aria-selected={tab === "file"} class={`seg-btn ${tab === "file" ? "on" : ""}`} onClick={() => setTab("file")}>{t("File")}</button>
            <button role="tab" aria-selected={tab === "text"} class={`seg-btn ${tab === "text" ? "on" : ""}`} onClick={() => setTab("text")}>{t("Paste text")}</button>
          </div>
          {tab === "file" ? (
            <div class={`sc-drop ${over ? "over" : ""}`}
              onDragOver={(e) => { e.preventDefault(); setOver(true); }} onDragLeave={() => setOver(false)}
              onDrop={(e) => { e.preventDefault(); setOver(false); pick(e.dataTransfer?.files[0]); }}>
              <div style={{ fontSize: 40 }} aria-hidden="true">📄</div>
              <div>{t("Drop a script here: PDF, TXT, Markdown or Fountain")}</div>
              <button class="btn lg primary" disabled={busy === "parsing"} onClick={() => input.current?.click()}>{busy === "parsing" ? t("Reading…") : t("Choose file")}</button>
              <input ref={input} type="file" hidden accept=".pdf,.txt,.md,.fountain,.fdx,text/plain,application/pdf" onChange={(e) => pick((e.target as HTMLInputElement).files?.[0])} />
            </div>
          ) : (
            <>
              <textarea class="input" style={{ minHeight: 220, fontFamily: "var(--font-ui)" }} placeholder={t("Paste the script or running order here")} value={text} onInput={(e) => setText((e.target as HTMLTextAreaElement).value)} />
              <button class="btn lg primary" disabled={!text.trim() || busy === "parsing"} onClick={() => { setFile(null); void parse({ text }); }}>{busy === "parsing" ? t("Reading…") : t("Analyse text")}</button>
            </>
          )}
          {err && <div class="sc-err" role="alert">{err}</div>}
        </div>
      )}
      {draft && (
        <div class="col" style={{ gap: 12 }}>
          <TextField label={t("Name")} value={name} onChange={setName} />
          <div class="sc-stats">
            <span class="sc-stat"><b>{draft.blocks.length}</b>{t("blocks")}</span>
            <span class="sc-stat"><b>{speeches}</b>{t("speeches")}</span>
            <span class="sc-stat"><b>{heads}</b>{t("headings")}</span>
            {draft.pages > 0 && <span class="sc-stat"><b>{draft.pages}</b>{t("pages")}</span>}
            {marks > 0 && <span class="sc-stat"><b>{marks}</b>{t("cues")}</span>}
          </div>
          {draft.warnings.map((w, i) => <div key={i} class="sc-warn">⚠ {w}</div>)}
          <div class="sc-preview" aria-label={t("Preview")}>
            {draft.blocks.slice(0, 14).map((b) => (
              <p key={b.id} style={{ fontWeight: b.kind === "heading" ? 800 : 400, fontStyle: b.kind === "direction" ? "italic" : "normal" }}>
                {b.speaker && <b style={{ color: "var(--cyan)" }}>{b.speaker}: </b>}{b.text.length > 140 ? b.text.slice(0, 140) + "…" : b.text}
              </p>
            ))}
            {draft.blocks.length > 14 && <p class="dim">… {t("{n} more", { n: draft.blocks.length - 14 })}</p>}
          </div>
          <div class="dim" style={{ fontSize: "var(--fs-sm)" }}>{t("You can fix the text and mark the cues afterwards in Edit mode.")}</div>
          {err && <div class="sc-err" role="alert">{err}</div>}
        </div>
      )}
    </Modal>
  );
}
