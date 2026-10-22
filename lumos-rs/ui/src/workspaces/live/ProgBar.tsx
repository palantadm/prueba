import { useEffect, useRef, useState } from "preact/hooks";
import { rt, selection, send, show, toast } from "../../store";
import type { PaletteKind, RecordMode } from "../../types";
import { Modal } from "../../ui";
import { t } from "../../i18n";
import { activeCuelist } from "../../lib/state";
import { releaseAttrs, setAttrs } from "../../lib/program";

const KIND_LABEL: Record<PaletteKind, string> = { intensity: "Intensity", position: "Position", color: "Color", beam: "Beam", all: "All" };
const heldCount = () => Object.values(rt.value?.programmer ?? {}).filter((a) => a && Object.keys(a).length).length;

export function ProgBar() {
  const [recCue, setRecCue] = useState(false);
  const [recPal, setRecPal] = useState(false);
  const [hl, setHl] = useState(false);
  const saved = useRef<Map<number, number | undefined>>(new Map());
  const hlIds = useRef<number[]>([]);

  // Highlight: full dimmer on selection, restored afterwards (never recorded unless the user records meanwhile).
  const restore = () => {
    const none: number[] = [], vals = new Map<number, number>();
    for (const id of hlIds.current) {
      const v = saved.current.get(id);
      if (v === undefined) none.push(id); else vals.set(id, v);
    }
    if (none.length) releaseAttrs(none, ["dimmer"]);
    for (const [id, v] of vals) setAttrs([id], { dimmer: v });
    hlIds.current = []; saved.current = new Map();
  };
  const apply = () => {
    hlIds.current = [...selection.value];
    saved.current = new Map(hlIds.current.map((id) => [id, rt.value?.programmer[id]?.dimmer]));
    setAttrs(hlIds.current, { dimmer: 1 });
  };
  useEffect(() => {
    if (!hl) return;
    apply();
    return restore;
  }, [hl, selection.value.join(",")]);

  return (
    <div class="lv-progbar" role="toolbar" aria-label={t("Programmer")}>
      <span class="label">{t("Programmer")}</span>
      <span class="chip warn" title={t("Fixtures holding programmer values")}>{t("{n} held", { n: heldCount() })}</span>
      <button class="btn" onClick={() => releaseAttrs(selection.value.length ? selection.value : null, null)} title={t("Release the selection's programmer values")}>{t("Clear sel")}</button>
      <button class="btn danger" onClick={() => releaseAttrs(null, null)} title={t("Release everything (Ctrl+Backspace)")}>{t("Release all")}</button>
      <span class="sp" />
      <button class={`btn ${hl ? "on" : ""}`} aria-pressed={hl} disabled={!selection.value.length && !hl} onClick={() => setHl(!hl)} title={t("Full dimmer on selection, not recorded")}>{t("☼ Highlight")}</button>
      <button class="btn" disabled={!heldCount()} onClick={() => setRecPal(true)}>{t("● Palette")}</button>
      <button class="btn primary" disabled={!heldCount()} onClick={() => setRecCue(true)}>{t("● Record cue")}</button>
      {recCue && <RecordCue onClose={() => setRecCue(false)} />}
      {recPal && <RecordPalette onClose={() => setRecPal(false)} />}
    </div>
  );
}

function RecordCue({ onClose }: { onClose: () => void }) {
  const lists = show.value?.cuelists ?? [];
  const [list, setList] = useState<number>(activeCuelist.value ?? lists[0]?.id ?? 0);
  const [number, setNumber] = useState<string>("");
  const [mode, setMode] = useState<RecordMode>("merge");
  const [label, setLabel] = useState("");
  const [release, setRelease] = useState(true);
  const cl = lists.find((l) => l.id === list);

  const createList = async () => {
    const id = Math.max(0, ...lists.map((l) => l.id)) + 1;
    await send({ type: "cuelist_save", cuelist: { id, name: t("Cuelist {n}", { n: id }), cues: [], looped: false, tracking: true, priority: 0, auto_mark: false } });
    setList(id);
  };
  const go = async () => {
    if (!list) return toast(t("Choose or create a cuelist"), "err");
    const n = number.trim() === "" ? null : parseFloat(number);
    if (n !== null && !Number.isFinite(n)) return toast(t("Invalid cue number"), "err");
    try {
      await send({ type: "prog_record_cue", cuelist: list, number: n, mode, label, release, refs: true });
      activeCuelist.value = list;
      toast(t("Recorded cue {n} in {name}", { n: n ?? t("(next)"), name: cl?.name ?? list }), "ok");
      onClose();
    } catch { /* toast shown */ }
  };
  return (
    <Modal title={t("Record cue")} onClose={onClose} footer={<>
      <button class="btn" onClick={onClose}>{t("Cancel")}</button>
      <button class="btn primary" onClick={go}>{t("Record")}</button>
    </>}>
      <div class="row">
        <div class="field grow"><label>{t("Cuelist")}</label>
          <select class="input" value={list} onChange={(e) => setList(Number((e.target as HTMLSelectElement).value))}>
            {lists.length === 0 && <option value={0}>{t("— none —")}</option>}
            {lists.map((l) => <option key={l.id} value={l.id}>{t("{name} ({n} cues)", { name: l.name, n: l.cues.length })}</option>)}
          </select></div>
        <button class="btn" style={{ alignSelf: "flex-end" }} onClick={createList}>{t("+ New")}</button>
      </div>
      <div class="row">
        <div class="field grow"><label>{t("Cue # (blank = next)")}</label>
          <input class="input num" placeholder={cl ? String(Math.floor(Math.max(0, ...cl.cues.map((c) => c.number))) + 1) : "1"}
            value={number} onInput={(e) => setNumber((e.target as HTMLInputElement).value)} /></div>
        <div class="field grow"><label>{t("Existing cue")}</label>
          <select class="input" value="" onChange={(e) => setNumber((e.target as HTMLSelectElement).value)}>
            <option value="">—</option>
            {cl?.cues.map((c) => <option key={c.number} value={c.number}>{c.number} {c.label}</option>)}
          </select></div>
      </div>
      <div class="field"><label>{t("Label")}</label><input class="input" value={label} onInput={(e) => setLabel((e.target as HTMLInputElement).value)} /></div>
      <div class="row">
        <button class={`btn ${mode === "merge" ? "sel" : ""}`} onClick={() => setMode("merge")}>{t("Merge")}</button>
        <button class={`btn ${mode === "replace" ? "sel" : ""}`} onClick={() => setMode("replace")}>{t("Replace")}</button>
        <span class="sp" />
        <label class="check"><input type="checkbox" checked={release} onChange={(e) => setRelease((e.target as HTMLInputElement).checked)} />{t("Release programmer after")}</label>
      </div>
    </Modal>
  );
}

function RecordPalette({ onClose }: { onClose: () => void }) {
  const [kind, setKind] = useState<PaletteKind>("color");
  const [name, setName] = useState("");
  const n = show.value?.palettes.length ?? 0;
  const ok = () => { send({ type: "prog_record_palette", id: null, fixtures: selection.value, kind, name: name.trim() || t("Palette {n}", { n: n + 1 }) }); onClose(); };
  return (
    <Modal title={t("Record palette")} onClose={onClose} footer={<><button class="btn" onClick={onClose}>{t("Cancel")}</button><button class="btn primary" onClick={ok}>{t("Record")}</button></>}>
      <div class="row wrap">{(["intensity", "position", "color", "beam", "all"] as PaletteKind[]).map((k) => (
        <button key={k} class={`btn ${kind === k ? "sel" : ""}`} onClick={() => setKind(k)}>{t(KIND_LABEL[k])}</button>))}</div>
      <div class="field"><label>{t("Name")}</label><input class="input" autoFocus value={name} onInput={(e) => setName((e.target as HTMLInputElement).value)} onKeyDown={(e) => e.key === "Enter" && ok()} /></div>
      <div class="dim">{t("Records from the programmer values of the {n} selected fixture(s).", { n: selection.value.length })}</div>
    </Modal>
  );
}

