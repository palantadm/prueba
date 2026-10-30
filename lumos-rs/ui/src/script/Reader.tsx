import { signal } from "@preact/signals";
import { useEffect, useRef, useState } from "preact/hooks";
import { send, show } from "../store";
import { Modal } from "../ui";
import { useLongPress } from "../lib/hooks";
import { t } from "../i18n";
import type { CueMark, Script, ScriptBlock, ScriptRt } from "../types";
import { editMode, editScript, fontSize, moving, sel, suggestions, updateMark } from "./store";
import { KIND_COLOR, KIND_FILL, blockKindLabel, kindLabel, marksSummary, mergeBlock, position, sortedMarks, splitBlock, words, type Position } from "./util";

/** Modals driven by signals so any part of the workspace can open them. */
export const markEditor = signal<{ block: number; mark: number | null; word: number } | null>(null);
export const blockEditor = signal<number | null>(null);
const pillSheet = signal<{ block: number; mark: number } | null>(null);
const blockSheet = signal<number | null>(null);

type PillState = "done" | "next" | "standby" | "idle";

function Pill(props: { script: Script; block: ScriptBlock; mark: CueMark; state: PillState; ghost?: boolean; onAccept?: () => void; onReject?: () => void }) {
  const { block, mark, state } = props;
  const edit = editMode.value;
  const lp = useLongPress(() => { if (edit) moving.value = { block: block.id, mark: mark.id }; });
  const label = mark.label || kindLabel(mark.kind);
  const isMoving = moving.value?.block === block.id && moving.value.mark === mark.id;
  const style = { "--pk": KIND_COLOR[mark.kind], "--pf": KIND_FILL[mark.kind] } as Record<string, string>;
  if (props.ghost) {
    return (
      <span class="sc-pill ghost" style={style} data-kind={mark.kind}>
        <span class="lbl">{label}</span>
        <button class="sc-pill-btn ok" aria-label={t("Accept")} onClick={props.onAccept}>✓</button>
        <button class="sc-pill-btn no" aria-label={t("Discard")} onClick={props.onReject}>✕</button>
      </span>
    );
  }
  return (
    <button
      type="button" data-kind={mark.kind} data-mark={mark.id}
      class={`sc-pill ${state} ${isMoving ? "moving" : ""}`} style={style}
      title={marksSummary(mark, show.value)}
      {...lp}
      onClick={(e) => {
        e.stopPropagation();
        if (lp.wasLong()) return;
        if (edit) markEditor.value = { block: block.id, mark: mark.id, word: mark.at_word };
        else pillSheet.value = { block: block.id, mark: mark.id };
      }}
    >
      <i class="dot" />
      <span class="lbl">{label}</span>
      {state === "done" && <b class="chk">✓</b>}
      {state === "next" && <b class="chk">▶</b>}
      {state === "standby" && <b class="stb">STANDBY</b>}
    </button>
  );
}

function BlockView(props: { script: Script; block: ScriptBlock; bi: number; pos: Position; cur: boolean; future: boolean }) {
  const { script, block, bi, pos } = props;
  const edit = editMode.value;
  const ws = words(block.text);
  const marks = sortedMarks(block);
  const sg = suggestions.value.filter((s) => s.block === block.id);
  const flatBy = new Map(pos.flat.filter((f) => f.block.id === block.id).map((f) => [f.mark.id, f.idx]));
  const stateOf = (m: CueMark): PillState => {
    const idx = flatBy.get(m.id) ?? -1;
    if (idx <= pos.passed) return "done";
    if (pos.next && pos.next.block.id === block.id && pos.next.mark.id === m.id) return pos.standby ? "standby" : "next";
    return "idle";
  };
  const selWord = sel.value?.block === block.id ? sel.value.word : -1;

  const tapWord = (w: number) => {
    if (moving.value) {
      const mv = moving.value;
      moving.value = null;
      editScript((s) => {
        let mk: CueMark | undefined;
        const blocks = s.blocks.map((b) => {
          if (b.id !== mv.block) return b;
          mk = b.marks.find((m) => m.id === mv.mark);
          return { ...b, marks: b.marks.filter((m) => m.id !== mv.mark) };
        });
        if (!mk) return s;
        const moved = { ...mk, at_word: w };
        return { ...s, blocks: blocks.map((b) => (b.id === block.id ? { ...b, marks: [...b.marks, moved] } : b)) };
      }, true);
      return;
    }
    sel.value = { block: block.id, word: w };
  };

  const nodes: preact.ComponentChildren[] = [];
  const pushMarksAt = (w: number, tail = false) => {
    for (const m of marks) if (tail ? m.at_word >= ws.length : m.at_word === w) nodes.push(<Pill key={`m${m.id}`} script={script} block={block} mark={m} state={stateOf(m)} />);
    for (const g of sg) if (tail ? g.mark.at_word >= ws.length : g.mark.at_word === w) {
      nodes.push(
        <Pill key={`g${g.mark.id}-${g.mark.at_word}-${g.mark.label}`} script={script} block={block} mark={g.mark} state="idle" ghost
          onAccept={() => { suggestions.value = suggestions.value.filter((x) => x !== g); editScript((s) => ({ ...s, blocks: s.blocks.map((b) => (b.id === block.id ? { ...b, marks: [...b.marks, { ...g.mark, id: Math.max(0, ...s.blocks.flatMap((x) => x.marks.map((m) => m.id))) + 1 }] } : b)) }), true); }}
          onReject={() => { suggestions.value = suggestions.value.filter((x) => x !== g); }} />,
      );
    }
  };
  if (!edit) {
    // read mode: text runs between pills
    let w = 0;
    const cuts = [...new Set([...marks.map((m) => Math.min(m.at_word, ws.length)), ...sg.map((g) => Math.min(g.mark.at_word, ws.length))])].sort((a, b) => a - b);
    for (const c of cuts) {
      if (c > w) nodes.push(<span key={`t${w}`} class="sc-run">{ws.slice(w, c).join(" ")} </span>);
      if (c >= ws.length) { /* tail handled below */ } else pushMarksAt(c);
      w = Math.max(w, c);
    }
    if (w < ws.length) nodes.push(<span key={`t${w}`} class="sc-run">{ws.slice(w).join(" ")}</span>);
    pushMarksAt(ws.length, true);
  } else {
    ws.forEach((word, i) => {
      pushMarksAt(i);
      nodes.push(
        <span key={`w${i}`} class={`sc-w ${selWord === i ? "sel" : ""} ${moving.value ? "target" : ""}`} data-b={block.id} data-w={i}
          onClick={(e) => { e.stopPropagation(); tapWord(i); }}>{word} </span>,
      );
    });
    pushMarksAt(ws.length, true);
    if (!ws.length) nodes.push(<span key="empty" class="sc-run faint" onClick={() => tapWord(0)}>{t("(empty block)")}</span>);
  }

  const cls = `sc-b ${block.kind} ${props.cur ? "cur" : ""} ${props.future ? "" : "past"} ${edit ? "editing" : ""}`;
  return (
    <div id={`sb-${block.id}`} class={cls} data-bi={bi} onClick={() => { if (!edit) blockSheet.value = block.id; }}>
      {block.kind === "speech" && <div class="sc-spk">{block.speaker}</div>}
      {block.kind === "direction" && <div class="sc-spk dim">{t("Stage direction")}</div>}
      {block.kind === "note" && <div class="sc-spk note">{t("NOTE")}</div>}
      {block.kind === "heading" && <div class="sc-spk" />}
      <div class="sc-txt">{nodes}</div>
      {edit && <button class="sc-blk-btn btn icon" aria-label={t("Edit block")} onClick={(e) => { e.stopPropagation(); blockEditor.value = block.id; }}>⋮</button>}
    </div>
  );
}

export function Reader(props: { script: Script; rt?: ScriptRt }) {
  const { script, rt } = props;
  const pos = position(script, rt);
  const scroller = useRef<HTMLDivElement>(null);
  const [paused, setPaused] = useState(false);
  const progRef = useRef(0);

  const scrollToCur = (smooth = true) => {
    const sc = scroller.current;
    if (!sc) return;
    const el = rt?.block ? (sc.querySelector(`#sb-${rt.block}`) as HTMLElement | null) : null;
    const top = el ? Math.max(0, el.offsetTop - (sc.clientHeight * 0.36 - el.offsetHeight / 2)) : 0;
    progRef.current = Date.now();
    sc.scrollTo({ top, behavior: smooth ? "smooth" : "auto" });
  };
  useEffect(() => { if (!paused) scrollToCur(true); }, [rt?.block, script.id, fontSize.value]);
  useEffect(() => { setPaused(false); setTimeout(() => scrollToCur(false), 30); }, [script.id]);
  const pause = () => setPaused(true);

  const selBlock = sel.value ? script.blocks.findIndex((b) => b.id === sel.value!.block) : -1;
  return (
    <div class="sc-reader" style={{ "--sc-fs": `${fontSize.value}px` } as Record<string, string>}>
      {moving.value && (
        <div class="sc-banner">
          {t("Tap the word where the cue goes")}
          <button class="btn sm" onClick={() => (moving.value = null)}>{t("Cancel")}</button>
        </div>
      )}
      <div ref={scroller} class="sc-scroll" onWheel={pause} onTouchStart={pause} onPointerDown={(e) => { if (e.pointerType === "mouse" && (e.target as HTMLElement).classList.contains("sc-scroll")) pause(); }}>
        <div class="sc-pad-top" />
        {script.blocks.map((b, bi) => (
          <BlockView key={b.id} script={script} block={b} bi={bi} pos={pos} cur={bi === pos.bi} future={bi >= pos.bi} />
        ))}
        <div class="sc-pad-bot" />
      </div>
      {paused && !editMode.value && (
        <button class="btn lg cyan sc-return" onClick={() => { setPaused(false); scrollToCur(true); }}>↧ {t("Back to position")}</button>
      )}
      {editMode.value && sel.value && !moving.value && selBlock >= 0 && (
        <EditBar script={script} bi={selBlock} word={sel.value.word} />
      )}
      <PillSheet script={script} />
      <BlockSheet script={script} />
    </div>
  );
}

function EditBar(props: { script: Script; bi: number; word: number }) {
  const { script, bi, word } = props;
  const b = script.blocks[bi];
  const ws = words(b.text);
  const shown = ws[word] ?? ws[ws.length - 1] ?? "";
  return (
    <div class="sc-editbar" role="toolbar" aria-label={t("Edit")}>
      <span class="sc-editbar-lbl">{t("Before")} <b>«{shown}»</b></span>
      <button class="btn lg primary" onClick={() => (markEditor.value = { block: b.id, mark: null, word })}>＋ {t("Cue here")}</button>
      <button class="btn lg" disabled={word <= 0 || word >= ws.length} onClick={() => { editScript((s) => splitBlock(s, bi, word), true); sel.value = null; }}>{t("Split here")}</button>
      <button class="btn lg" disabled={bi <= 0} onClick={() => { editScript((s) => mergeBlock(s, bi), true); sel.value = null; }}>{t("Merge with previous")}</button>
      <button class="btn lg" onClick={() => (blockEditor.value = b.id)}>{t("Edit block")}</button>
      <button class="btn lg ghost icon" aria-label={t("Close")} onClick={() => (sel.value = null)}>✕</button>
    </div>
  );
}

function PillSheet(props: { script: Script }) {
  const ps = pillSheet.value;
  if (!ps) return null;
  const b = props.script.blocks.find((x) => x.id === ps.block);
  const m = b?.marks.find((x) => x.id === ps.mark);
  if (!b || !m) return null;
  const close = () => (pillSheet.value = null);
  return (
    <Modal title={m.label || kindLabel(m.kind)} width={520} onClose={close}
      footer={<button class="btn lg" onClick={close}>{t("Close")}</button>}>
      <div class="sc-sheet-sum">{marksSummary(m, show.value)}</div>
      <div class="col" style={{ gap: 10 }}>
        <button class="btn lg go" onClick={() => { void send({ type: "script_fire", id: props.script.id, block: b.id, mark: m.id }); close(); }}>▶ {t("Fire now")}</button>
        <button class="btn lg" onClick={() => { void send({ type: "script_goto", id: props.script.id, block: b.id, mark: m.id }); close(); }}>{t("Go here (without firing)")}</button>
        <button class="btn lg" onClick={() => { close(); editMode.value = true; markEditor.value = { block: b.id, mark: m.id, word: m.at_word }; }}>{t("Edit cue")}</button>
      </div>
    </Modal>
  );
}

function BlockSheet(props: { script: Script }) {
  const id = blockSheet.value;
  if (id == null) return null;
  const b = props.script.blocks.find((x) => x.id === id);
  if (!b) return null;
  const close = () => (blockSheet.value = null);
  return (
    <Modal title={blockKindLabel(b.kind)} width={520} onClose={close}
      footer={<button class="btn lg" onClick={close}>{t("Close")}</button>}>
      <div class="sc-sheet-sum">{b.speaker ? <b>{b.speaker}: </b> : null}{b.text.length > 160 ? b.text.slice(0, 160) + "…" : b.text}</div>
      <div class="col" style={{ gap: 10 }}>
        <button class="btn lg cyan" onClick={() => { void send({ type: "script_goto", id: props.script.id, block: b.id, mark: null }); close(); }}>{t("Place the script here")}</button>
        <button class="btn lg" onClick={() => { close(); editMode.value = true; sel.value = { block: b.id, word: 0 }; }}>{t("Edit block")}</button>
      </div>
    </Modal>
  );
}
