import "../../styles/auto.css";
import { useState } from "preact/hooks";
import { rt, send, show, toast } from "../../store";
import type { Macro, MacroStep } from "../../types";
import { Modal, NumberField, confirmDialog } from "../../ui";
import { useLongPress } from "../../lib/hooks";
import { cmdHistory } from "../../auto/state";
import { tErr } from "../../i18n/errors";
import { t, tp } from "../../i18n";

const COLORS = ["#3cc4e8", "#35c46a", "#f5a524", "#e5484d", "#e266b8", "#b38cff", "#ff9a3d", "#8a93a0"];

function MacroButton({ m, running, onEdit }: { m: Macro; running: boolean; onEdit: () => void }) {
  const lp = useLongPress(onEdit);
  return (
    <div class={`au-macro ${running ? "run" : ""}`} style={{ "--mc": m.color || "var(--accent)" } as any}>
      <i class="bar" aria-hidden="true" />
      <button class="au-main" {...lp} aria-pressed={running}
        aria-label={running ? t("Stop macro {name}", { name: m.name }) : t("Run macro {name}", { name: m.name })}
        onClick={() => {
          if (lp.wasLong()) return;
          if (running) send({ type: "macro_stop", id: m.id });
          else send({ type: "macro_run", id: m.id });
        }}>
        <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{m.name}</span>
        <small>#{m.id} · {tp(m.steps.length, "{n} step", "{n} steps")}</small>
      </button>
      <button class="au-edit" onClick={onEdit} aria-label={t("Edit macro {name}", { name: m.name })} title={t("Edit")}>⋮</button>
    </div>
  );
}

export function Macros() {
  const macros = show.value?.macros ?? [];
  const running = new Set(rt.value?.macros_running ?? []);
  const [edit, setEdit] = useState<Macro | null>(null);
  const create = () => setEdit({ id: Math.max(0, ...macros.map((m) => m.id)) + 1, name: t("Macro {n}", { n: macros.length + 1 }), steps: [{ text: "", wait: 0 }], color: COLORS[macros.length % COLORS.length] });
  return (
    <div class="panel au-macros">
      <div class="panel-head">
        <span>{t("Macros")}</span><span class="sp" />
        {running.size > 0 && <button class="btn sm danger" onClick={() => send({ type: "macro_stop", id: null })}>{t("Stop all")}</button>}
        <button class="btn sm primary" style={{ minHeight: 36 }} onClick={create} aria-label={t("New macro")}>{t("+ New")}</button>
      </div>
      {macros.length === 0 && <div class="dim" style={{ padding: 12 }}>{t("No macros yet. A macro is a sequence of command-line statements with optional waits.")}</div>}
      <div class="au-mgrid">
        {macros.map((m) => <MacroButton key={m.id} m={m} running={running.has(m.id)} onEdit={() => setEdit(structuredClone(m))} />)}
      </div>
      {edit && <MacroEditor initial={edit} isNew={!macros.some((m) => m.id === edit.id)} onClose={() => setEdit(null)} />}
    </div>
  );
}

function MacroEditor({ initial, isNew, onClose }: { initial: Macro; isNew: boolean; onClose: () => void }) {
  const [m, setM] = useState<Macro>(initial);
  const [res, setRes] = useState<Record<number, { ok: boolean; msg: string }>>({});
  const [pick, setPick] = useState<string[] | null>(null);
  const setStep = (i: number, p: Partial<MacroStep>) => setM({ ...m, steps: m.steps.map((s, k) => (k === i ? { ...s, ...p } : s)) });
  const move = (i: number, d: number) => {
    const j = i + d;
    if (j < 0 || j >= m.steps.length) return;
    const steps = [...m.steps];
    [steps[i], steps[j]] = [steps[j], steps[i]];
    setM({ ...m, steps });
  };
  const test = async (i: number) => {
    const text = m.steps[i].text.trim();
    if (!text) return;
    try { await send({ type: "command_line", text }, { quiet: true }); setRes({ ...res, [i]: { ok: true, msg: t("OK") } }); }
    catch (e) { setRes({ ...res, [i]: { ok: false, msg: tErr((e as Error).message) } }); }
  };
  const recent = (() => {
    const fromLog = (rt.value?.cmd_log ?? []).filter((l) => l.ok).map((l) => l.text);
    return Array.from(new Set([...fromLog, ...cmdHistory.value.slice().reverse()])).slice(0, 12);
  })();
  const valid = m.name.trim() && m.steps.some((s) => s.text.trim());
  const save = async () => {
    const steps = m.steps.filter((s) => s.text.trim()).map((s) => ({ text: s.text.trim(), wait: Math.max(0, s.wait) }));
    try { await send({ type: "macro_save", macro: { ...m, name: m.name.trim(), steps } }); toast(t("Macro saved"), "ok", 1500); onClose(); } catch { /* toasted */ }
  };
  const del = async () => {
    if (!(await confirmDialog(t("Delete macro"), t("Delete macro “{name}”?", { name: m.name }), { danger: true, ok: t("Delete") }))) return;
    await send({ type: "macro_remove", id: m.id }).catch(() => {});
    onClose();
  };
  return (
    <Modal title={isNew ? t("New macro") : t("Edit macro")} onClose={onClose} width={720}
      footer={<>
        {!isNew && <button class="btn au-btn danger" onClick={del}>{t("Delete")}</button>}
        <span class="sp" />
        <button class="btn au-btn" onClick={onClose}>{t("Cancel")}</button>
        <button class="btn au-btn primary" disabled={!valid} onClick={save}>{t("Save")}</button>
      </>}>
      <div class="field"><label for="mac-name">{t("Name")}</label>
        <input id="mac-name" class="input" style={{ minHeight: 44 }} value={m.name} onInput={(e) => setM({ ...m, name: (e.target as HTMLInputElement).value })} /></div>
      <div class="field"><label>{t("Colour")}</label>
        <div class="row wrap" role="radiogroup" aria-label={t("Colour")}>
          {COLORS.map((c) => <button key={c} role="radio" aria-checked={m.color === c} class={`au-swatch ${m.color === c ? "sel" : ""}`} style={{ background: c }} onClick={() => setM({ ...m, color: c })} aria-label={c} />)}
          <input type="color" aria-label={t("Custom colour")} value={/^#[0-9a-f]{6}$/i.test(m.color) ? m.color : "#3cc4e8"} onInput={(e) => setM({ ...m, color: (e.target as HTMLInputElement).value })} style={{ width: 44, height: 44, padding: 0, border: 0, background: "none" }} />
        </div></div>
      <div class="label">{t("Steps")} <span class="faint">({t("command-line statements, run in order")})</span></div>
      <div class="au-steps">
        {m.steps.map((s, i) => (
          <div key={i}>
            <div class="au-step">
              <span class="idx mono">{i + 1}</span>
              <input class="input" style={{ minHeight: 44, fontFamily: "var(--font-mono)" }} value={s.text} spellcheck={false} placeholder={t("e.g. go cuelist 1")} aria-label={t("Step {n} command", { n: i + 1 })}
                onInput={(e) => setStep(i, { text: (e.target as HTMLInputElement).value })} />
              <div class="row" style={{ gap: 4 }} title={t("Seconds to wait after this step")}>
                <NumberField float min={0} max={3600} step={0.5} value={s.wait} width={64} label={t("Wait after step {n} (s)", { n: i + 1 })} class="bare" onChange={(v) => setStep(i, { wait: v })} /><span class="faint">s</span>
              </div>
              <div class="row" style={{ gap: 4 }}>
                <button class="btn au-btn" onClick={() => move(i, -1)} disabled={i === 0} aria-label={t("Move up")}>▲</button>
                <button class="btn au-btn" onClick={() => move(i, 1)} disabled={i === m.steps.length - 1} aria-label={t("Move down")}>▼</button>
                <button class="btn au-btn" onClick={() => test(i)} disabled={!s.text.trim()} title={t("Run only this step now")}>{t("Test")}</button>
                <button class="btn au-btn danger" onClick={() => setM({ ...m, steps: m.steps.filter((_, k) => k !== i) })} aria-label={t("Remove step")}>✕</button>
              </div>
            </div>
            {res[i] && <div class={`au-res ${res[i].ok ? "ok" : "err"}`} style={{ marginLeft: 34 }}>{res[i].ok ? "✓ " : "✕ "}{res[i].msg}</div>}
          </div>
        ))}
      </div>
      <div class="row wrap">
        <button class="btn au-btn" onClick={() => setM({ ...m, steps: [...m.steps, { text: "", wait: 0 }] })}>{t("+ Add step")}</button>
        <button class="btn au-btn" aria-expanded={pick !== null} onClick={() => setPick(pick ? null : [])}>{t("Add recent commands…")}</button>
      </div>
      {pick && (
        <div class="panel panel-body col" style={{ gap: 6 }}>
          {recent.length === 0 && <span class="dim">{t("No commands typed yet.")}</span>}
          {recent.map((c) => (
            <label key={c} class="check" style={{ minHeight: 44 }}>
              <input type="checkbox" checked={pick.includes(c)} onChange={(e) => setPick((e.target as HTMLInputElement).checked ? [...pick, c] : pick.filter((x) => x !== c))} />
              <span class="mono">{c}</span>
            </label>
          ))}
          {recent.length > 0 && <button class="btn au-btn primary" disabled={!pick.length} onClick={() => {
            const base = m.steps.filter((s) => s.text.trim());
            setM({ ...m, steps: [...base, ...recent.filter((c) => pick.includes(c)).map((text) => ({ text, wait: 0 }))] });
            setPick(null);
          }}>{t("Add selected")}</button>}
        </div>
      )}
    </Modal>
  );
}
