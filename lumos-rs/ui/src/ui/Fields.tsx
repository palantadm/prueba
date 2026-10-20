import type { ComponentChildren } from "preact";
import { useEffect, useState } from "preact/hooks";

let uid = 0;
const nextId = () => `f${++uid}`;

/** Labelled form row. Pass `for` to bind the label to a control id. */
export function Field(props: { label: string; hint?: string; children: ComponentChildren; for?: string; class?: string }) {
  return (
    <div class={`field ${props.class ?? ""}`}>
      <label for={props.for}>{props.label}</label>
      {props.children}
      {props.hint && <span class="faint" style={{ fontSize: "var(--fs-xs)" }}>{props.hint}</span>}
    </div>
  );
}

export function TextField(props: {
  value: string; onChange: (v: string) => void; placeholder?: string; label?: string; mono?: boolean;
  disabled?: boolean; class?: string; hint?: string; invalid?: boolean; onCommit?: (v: string) => void;
}) {
  const [id] = useState(nextId);
  const input = (
    <input id={id} class={`input ${props.mono ? "mono" : ""} ${props.class ?? ""}`} value={props.value}
      placeholder={props.placeholder} disabled={props.disabled} aria-invalid={props.invalid}
      style={props.invalid ? { borderColor: "var(--stop)" } : undefined}
      aria-label={props.label}
      onInput={(e) => props.onChange((e.target as HTMLInputElement).value)}
      onBlur={(e) => props.onCommit?.((e.target as HTMLInputElement).value)}
      onKeyDown={(e) => e.key === "Enter" && props.onCommit?.((e.target as HTMLInputElement).value)} />
  );
  return props.label && !props.class?.includes("bare") ? <Field label={props.label} for={id} hint={props.hint}>{input}</Field> : input;
}

/** Numeric input that edits freely and commits a clamped integer/float on blur / Enter. */
export function NumberField(props: {
  value: number; onChange: (v: number) => void; min?: number; max?: number; step?: number; label?: string;
  disabled?: boolean; class?: string; hint?: string; width?: number; float?: boolean;
}) {
  const [id] = useState(nextId);
  const [txt, setTxt] = useState(String(props.value));
  useEffect(() => setTxt(String(props.value)), [props.value]);
  const commit = () => {
    let n = props.float ? parseFloat(txt) : parseInt(txt, 10);
    if (Number.isNaN(n)) { setTxt(String(props.value)); return; }
    if (props.min !== undefined) n = Math.max(props.min, n);
    if (props.max !== undefined) n = Math.min(props.max, n);
    setTxt(String(n));
    if (n !== props.value) props.onChange(n);
  };
  const bump = (d: number) => {
    let n = props.value + d * (props.step ?? 1);
    if (props.min !== undefined) n = Math.max(props.min, n);
    if (props.max !== undefined) n = Math.min(props.max, n);
    props.onChange(n);
  };
  const input = (
    <input id={id} class={`input num ${props.class ?? ""}`} inputMode="decimal" value={txt} disabled={props.disabled}
      aria-label={props.label} style={props.width ? { width: props.width } : undefined}
      onInput={(e) => setTxt((e.target as HTMLInputElement).value)} onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        else if (e.key === "ArrowUp") { e.preventDefault(); bump(1); }
        else if (e.key === "ArrowDown") { e.preventDefault(); bump(-1); }
      }} />
  );
  return props.label && !props.class?.includes("bare") ? <Field label={props.label} for={id} hint={props.hint}>{input}</Field> : input;
}

export type Opt<T> = { value: T; label: string; disabled?: boolean };

export function Select<T extends string | number>(props: {
  value: T; options: Opt<T>[]; onChange: (v: T) => void; label?: string; disabled?: boolean; class?: string; hint?: string;
}) {
  const [id] = useState(nextId);
  const num = typeof props.value === "number";
  const sel = (
    <select id={id} class={`input ${props.class ?? ""}`} value={String(props.value)} disabled={props.disabled} aria-label={props.label}
      onChange={(e) => { const v = (e.target as HTMLSelectElement).value; props.onChange((num ? Number(v) : v) as T); }}>
      {props.options.map((o) => <option key={String(o.value)} value={String(o.value)} disabled={o.disabled}>{o.label}</option>)}
    </select>
  );
  return props.label && !props.class?.includes("bare") ? <Field label={props.label} for={id} hint={props.hint}>{sel}</Field> : sel;
}

export function Toggle(props: { checked: boolean; onChange: (v: boolean) => void; label?: string; disabled?: boolean; title?: string }) {
  return (
    <label class="check" title={props.title} style={props.disabled ? { opacity: 0.4 } : undefined}>
      <input type="checkbox" checked={props.checked} disabled={props.disabled} onChange={(e) => props.onChange((e.target as HTMLInputElement).checked)} />
      {props.label && <span>{props.label}</span>}
    </label>
  );
}

/** Segmented button group (single choice). */
export function Segmented<T extends string | number>(props: { value: T; options: Opt<T>[]; onChange: (v: T) => void; label?: string; size?: "sm" | "md" }) {
  return (
    <div class="seg" role="radiogroup" aria-label={props.label}>
      {props.options.map((o) => (
        <button key={String(o.value)} role="radio" aria-checked={o.value === props.value} disabled={o.disabled}
          class={`seg-btn ${o.value === props.value ? "active" : ""} ${props.size ?? ""}`} onClick={() => props.onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}
