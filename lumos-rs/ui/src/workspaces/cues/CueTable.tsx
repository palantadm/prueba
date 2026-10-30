import { useEffect, useState } from "preact/hooks";
import { send, toast } from "../../store";
import type { Cue, Cuelist, CuelistRt } from "../../types";
import { fmtNum } from "../../lib/attrs";
import { Modal } from "../../ui";
import { t } from "../../i18n";

export const TC_RE = /^\d{2}:\d{2}:\d{2}:\d{2}$/;

/** Text cell committing on blur / Enter, reverting on Escape. */
export function Cell(p: { value: string; onCommit: (v: string) => void; num?: boolean; width?: number; placeholder?: string; label: string }) {
  const [v, setV] = useState(p.value);
  useEffect(() => setV(p.value), [p.value]);
  return (
    <input class={`cell ${p.num ? "num" : ""}`} inputMode={p.num ? "decimal" : undefined} value={v} placeholder={p.placeholder} aria-label={p.label} style={p.width ? { width: p.width } : undefined}
      onInput={(e) => setV((e.target as HTMLInputElement).value)}
      onBlur={() => v !== p.value && p.onCommit(v)}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        if (e.key === "Escape") { setV(p.value); (e.target as HTMLInputElement).blur(); }
        e.stopPropagation();
      }} />
  );
}


export function CueTable(p: { list: Cuelist; rt?: CuelistRt; selected: number | null; onSelect: (n: number) => void }) {
  const { list } = p;
  const cues = [...list.cues].sort((a, b) => a.number - b.number);
  const save = (c: Cue) => send({ type: "cue_save", cuelist: list.id, cue: c });
  const secs = (v: string, fallback: number) => { const n = parseFloat(v); return Number.isFinite(n) && n >= 0 ? n : fallback; };

  const renumber = async (c: Cue, v: string) => {
    const n = parseFloat(v);
    if (!Number.isFinite(n) || n <= 0) return toast(t("Cue number must be > 0"), "err");
    if (n === c.number) return;
    if (list.cues.some((x) => x.number === n)) return toast(t("Cue {n} already exists", { n: fmtNum(n) }), "err");
    try {
      await send({ type: "cue_remove", cuelist: list.id, number: c.number });
      await send({ type: "cue_save", cuelist: list.id, cue: { ...c, number: n } });
      p.onSelect(n);
    } catch { /* toasted */ }
  };

  return (
    <div class="panel cu-tbl scroll">
      <table aria-label={t("Cues")}>
        <colgroup><col style={{ width: 76 }} /><col /><col style={{ width: 84 }} /><col style={{ width: 84 }} /><col style={{ width: 84 }} /><col style={{ width: 84 }} /></colgroup>
        <thead><tr>
          <th>{t("Cue")}</th><th>{t("Label")}</th><th>{t("Fade in")}</th><th>{t("Fade out")}</th><th>{t("Delay")}</th><th>{t("Follow")}</th>
        </tr></thead>
        <tbody>
          {cues.length === 0 && <tr class="empty"><td colSpan={6}>{t("No cues. Record from the programmer (Live) or insert an empty cue.")}</td></tr>}
          {cues.map((c) => {
            const run = p.rt?.current === c.number, next = p.rt?.next === c.number;
            return (
              <tr key={c.number} class={`${run ? "run" : ""} ${next && !run ? "next" : ""} ${p.selected === c.number ? "sel" : ""}`}
                onClick={() => p.onSelect(c.number)} onDblClick={() => send({ type: "go_to", cuelist: list.id, number: c.number })}
                title={t("Double-click: GO TO this cue")}>
                <td><Cell num label={t("Cue {n} number", { n: fmtNum(c.number) })} value={fmtNum(c.number)} onCommit={(v) => renumber(c, v)} /></td>
                <td>
                  <div class="nm">
                    {run && <span class="tag run">{t("Current")}</span>}
                    {next && !run && <span class="tag next">{t("Next")}</span>}
                    <Cell label={t("Label")} value={c.label} placeholder="—" onCommit={(v) => save({ ...c, label: v })} />
                    {c.timecode && <span class="tag tc mono" title={c.timecode}>TC</span>}
                    {c.block && <span class="tag blk">{t("Block")}</span>}
                  </div>
                </td>
                <td><Cell num label={t("Fade in")} value={fmtNum(c.fade_in)} onCommit={(v) => save({ ...c, fade_in: secs(v, c.fade_in) })} /></td>
                <td><Cell num label={t("Fade out")} value={fmtNum(c.fade_out)} onCommit={(v) => save({ ...c, fade_out: secs(v, c.fade_out) })} /></td>
                <td><Cell num label={t("Delay")} value={fmtNum(c.delay)} onCommit={(v) => save({ ...c, delay: secs(v, c.delay) })} /></td>
                <td><Cell num label={t("Follow")} placeholder="—" value={c.follow == null ? "" : fmtNum(c.follow)}
                  onCommit={(v) => save({ ...c, follow: v.trim() === "" ? null : secs(v, c.follow ?? 0) })} /></td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

export function InsertCue(p: { list: Cuelist; after: number | null; onClose: () => void; onDone: (n: number) => void }) {
  const nums = p.list.cues.map((c) => c.number).sort((a, b) => a - b);
  const def = (() => {
    if (!nums.length) return 1;
    if (p.after === null) return Math.floor(nums[nums.length - 1]) + 1;
    const nx = nums.find((n) => n > p.after!);
    return nx === undefined ? Math.floor(p.after) + 1 : +((p.after + nx) / 2).toFixed(3);
  })();
  const [n, setN] = useState(String(def));
  const [label, setLabel] = useState("");
  const go = async () => {
    const num = parseFloat(n);
    if (!Number.isFinite(num) || num <= 0) return toast(t("Invalid cue number"), "err");
    if (nums.includes(num)) return toast(t("Cue {n} already exists", { n: fmtNum(num) }), "err");
    try {
      await send({ type: "cue_save", cuelist: p.list.id, cue: { number: num, label, fade_in: 2, fade_out: 2, delay: 0, follow: null, timecode: null, changes: {}, block: false, refs: {}, timing: {}, curve: "linear", stagger: 0, notes: "" } });
      p.onDone(num); p.onClose();
    } catch { /* toasted */ }
  };
  return (
    <Modal title={t("Insert cue")} onClose={p.onClose} footer={<><button class="btn lg" onClick={p.onClose}>{t("Cancel")}</button><button class="btn lg primary" onClick={go}>{t("Insert")}</button></>}>
      <div class="row">
        <div class="field"><label>{t("Number")}</label><input class="input num" autoFocus value={n} onInput={(e) => setN((e.target as HTMLInputElement).value)} onKeyDown={(e) => e.key === "Enter" && go()} /></div>
        <div class="field grow"><label>{t("Label")}</label><input class="input" value={label} onInput={(e) => setLabel((e.target as HTMLInputElement).value)} onKeyDown={(e) => e.key === "Enter" && go()} /></div>
      </div>
    </Modal>
  );
}
