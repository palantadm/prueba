import { useState } from "preact/hooks";
import { send, show } from "../store";
import { Modal, Select, TextField } from "../ui";
import { confirmDialog } from "../ui";
import { t } from "../i18n";
import type { CueMark, MarkKind, ScriptAction } from "../types";
import { currentScript, editScript } from "./store";
import { ActionEditor, PresetPicker, Stepper } from "./Actions";
import { KIND_COLOR, MARK_KINDS, kindLabel, newMark, words } from "./util";
import { blockEditor, markEditor } from "./Reader";

export function MarkEditorHost() {
  const st = markEditor.value;
  const script = currentScript();
  if (!st || !script) return null;
  const block = script.blocks.find((b) => b.id === st.block);
  if (!block) return null;
  return <MarkEditor key={`${st.block}:${st.mark ?? "new"}`} blockId={st.block} markId={st.mark} word={st.word} />;
}

function MarkEditor(props: { blockId: number; markId: number | null; word: number }) {
  const script = currentScript()!;
  const block = script.blocks.find((b) => b.id === props.blockId)!;
  const existing = props.markId != null ? block.marks.find((m) => m.id === props.markId) : undefined;
  const [m, setM] = useState<CueMark>(() => (existing ? structuredClone(existing) : newMark(script, "light", props.word)));
  const [picking, setPicking] = useState(false);
  const ws = words(block.text);
  const close = () => (markEditor.value = null);
  const patch = (p: Partial<CueMark>) => setM((x) => ({ ...x, ...p }));

  const save = () => {
    editScript((s) => ({
      ...s,
      blocks: s.blocks.map((b) => {
        if (b.id !== block.id) return b;
        const rest = b.marks.filter((x) => x.id !== m.id);
        return { ...b, marks: [...rest, m] };
      }),
    }), true);
    close();
  };
  const remove = async () => {
    if (!(await confirmDialog(t("Delete cue"), t("Delete cue “{name}”?", { name: m.label || kindLabel(m.kind) }), { danger: true, ok: t("Delete") }))) return;
    editScript((s) => ({ ...s, blocks: s.blocks.map((b) => (b.id === block.id ? { ...b, marks: b.marks.filter((x) => x.id !== m.id) } : b)) }), true);
    close();
  };
  const move = () => { close(); markEditor.value = null; import("./store").then((s) => { s.moving.value = { block: block.id, mark: m.id }; }); };

  const cl = show.value?.cuelists ?? [];
  const link = m.link;
  const cues = cl.find((c) => c.id === link?.cuelist)?.cues ?? [];
  const before = ws.slice(Math.max(0, m.at_word - 4), m.at_word).join(" ");
  const after = ws.slice(m.at_word, m.at_word + 4).join(" ");

  return (
    <Modal title={existing ? t("Edit cue") : t("Add cue here")} width={720} onClose={close}
      footer={
        <>
          {existing && <button class="btn lg danger" onClick={() => void remove()}>{t("Delete")}</button>}
          {existing && (
            <button class="btn lg go" onClick={() => void send({ type: "script_fire", id: script.id, block: block.id, mark: m.id })} title={t("Saves first? No — fires the saved cue")}>▶ {t("Fire now")}</button>
          )}
          <span class="sp" />
          <button class="btn lg" onClick={close}>{t("Cancel")}</button>
          <button class="btn lg primary" onClick={save}>{t("Save")}</button>
        </>
      }>
      <div class="col" style={{ gap: 18 }}>
        <TextField label={t("Name")} value={m.label} placeholder={t("e.g. Sunrise on the patio")} onChange={(label) => patch({ label })} />

        <div class="field"><label>{t("Type")}</label>
          <div class="sc-kinds">
            {MARK_KINDS.map((k) => (
              <button key={k} class={`btn lg ${m.kind === k ? "sel" : ""}`} style={{ "--pk": KIND_COLOR[k] } as Record<string, string>} onClick={() => patch({ kind: k as MarkKind })}>
                <i class="sc-kdot" style={{ background: KIND_COLOR[k] }} />{kindLabel(k)}
              </button>
            ))}
          </div>
        </div>

        <div class="field"><label>{t("Position in the text")}</label>
          <div class="row" style={{ gap: 12, flexWrap: "wrap" }}>
            <Stepper value={m.at_word} min={0} max={ws.length} label={t("Word")} onChange={(at_word) => patch({ at_word })} />
            <div class="sc-ctx"><span class="dim">…{before} </span><b class="sc-caret">▮</b><span> {after}…</span></div>
            {existing && <button class="btn" onClick={move}>{t("Move…")}</button>}
          </div>
        </div>

        <div class="grid2">
          <div class="field"><label>{t("Standby (blocks before)")}</label>
            <Stepper value={m.standby_blocks} min={0} max={20} label={t("Standby")} onChange={(standby_blocks) => patch({ standby_blocks })} />
          </div>
          <TextField label={t("Note")} value={m.note} onChange={(note) => patch({ note })} />
        </div>

        <div class="field"><label>{t("Linked Lumos cue")}</label>
          <div class="grid2">
            <Select<number> value={link?.cuelist ?? 0} options={[{ value: 0, label: t("No link") }, ...cl.map((c) => ({ value: c.id, label: `${c.id} · ${c.name}` }))]}
              onChange={(id) => patch({ link: id ? { cuelist: id, number: cl.find((c) => c.id === id)?.cues[0]?.number ?? 1 } : null })} />
            {link && (
              <Select<number> value={link.number} options={cues.length ? cues.map((c) => ({ value: c.number, label: `${c.number}${c.label ? " · " + c.label : ""}` })) : [{ value: link.number, label: String(link.number) }]}
                onChange={(number) => patch({ link: { ...link, number } })} />
            )}
          </div>
          <span class="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("Firing the cue also GOes this Lumos cue; with follow mode, when it fires elsewhere the script jumps here.")}</span>
        </div>

        <div class="field"><label>{t("Actions")} ({m.actions.length})</label>
          <div class="col" style={{ gap: 10 }}>
            {m.actions.map((a, i) => (
              <ActionEditor key={i} a={a} onChange={(na: ScriptAction) => patch({ actions: m.actions.map((x, j) => (j === i ? na : x)) })}
                onRemove={() => patch({ actions: m.actions.filter((_, j) => j !== i) })} />
            ))}
            {picking
              ? <PresetPicker onPick={(a) => patch({ actions: [...m.actions, a] })} onClose={() => setPicking(false)} />
              : <button class="btn lg" onClick={() => setPicking(true)}>＋ {t("Add action")}</button>}
          </div>
        </div>
      </div>
    </Modal>
  );
}

export function BlockEditorHost() {
  const id = blockEditor.value;
  const script = currentScript();
  if (id == null || !script) return null;
  const b = script.blocks.find((x) => x.id === id);
  if (!b) return null;
  return <BlockEditor key={id} blockId={id} />;
}

import { BLOCK_KINDS, blockKindLabel, newBlock } from "./util";
import { Segmented, NumberField } from "../ui";
import type { ScriptKind } from "../types";

function BlockEditor(props: { blockId: number }) {
  const script = currentScript()!;
  const idx = script.blocks.findIndex((b) => b.id === props.blockId);
  const orig = script.blocks[idx];
  const [b, setB] = useState({ ...orig });
  const close = () => (blockEditor.value = null);
  const save = () => {
    editScript((s) => ({ ...s, blocks: s.blocks.map((x) => (x.id === b.id ? { ...x, kind: b.kind, speaker: b.kind === "speech" ? b.speaker : "", text: b.text, page: b.page } : x)) }), true);
    close();
  };
  const del = async () => {
    if (!(await confirmDialog(t("Delete block"), t("Delete this block and its {n} cue(s)?", { n: orig.marks.length }), { danger: true, ok: t("Delete") }))) return;
    editScript((s) => ({ ...s, blocks: s.blocks.filter((x) => x.id !== b.id) }), true);
    close();
  };
  const insert = (at: number) => {
    editScript((s) => { const nb = newBlock(s, "speech", ""); const blocks = [...s.blocks]; blocks.splice(at, 0, nb); return { ...s, blocks }; }, true);
    close();
  };
  return (
    <Modal title={t("Edit block")} width={720} onClose={close}
      footer={<><button class="btn lg danger" onClick={() => void del()}>{t("Delete")}</button><span class="sp" /><button class="btn lg" onClick={close}>{t("Cancel")}</button><button class="btn lg primary" onClick={save}>{t("Save")}</button></>}>
      <div class="col" style={{ gap: 16 }}>
        <Segmented<ScriptKind> value={b.kind} options={BLOCK_KINDS.map((k) => ({ value: k, label: blockKindLabel(k) }))} onChange={(kind) => setB({ ...b, kind })} />
        {b.kind === "speech" && <TextField label={t("Speaker")} value={b.speaker} onChange={(speaker) => setB({ ...b, speaker })} />}
        <div class="field"><label>{t("Text")}</label>
          <textarea class="input" rows={7} style={{ fontSize: 18, lineHeight: 1.5, minHeight: 160 }} value={b.text} onInput={(e) => setB({ ...b, text: (e.target as HTMLTextAreaElement).value })} /></div>
        <div class="row" style={{ gap: 12, flexWrap: "wrap" }}>
          <NumberField label={t("PDF page (0 = unknown)")} value={b.page} min={0} max={9999} onChange={(page) => setB({ ...b, page })} />
          <span class="sp" />
          <button class="btn lg" onClick={() => insert(idx)}>↑ {t("Insert block before")}</button>
          <button class="btn lg" onClick={() => insert(idx + 1)}>↓ {t("Insert block after")}</button>
        </div>
      </div>
    </Modal>
  );
}
