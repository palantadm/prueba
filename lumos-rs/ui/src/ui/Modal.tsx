import type { ComponentChildren } from "preact";
import { useEffect } from "preact/hooks";
import { toasts } from "../store";

export function Modal(props: {
  title: string;
  onClose: () => void;
  children: ComponentChildren;
  footer?: ComponentChildren;
  width?: number;
}) {
  useEffect(() => {
    const h = (e: KeyboardEvent) => e.key === "Escape" && props.onClose();
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [props.onClose]);
  return (
    <div class="modal-back" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="modal" role="dialog" aria-modal="true" aria-label={props.title} style={props.width ? { width: props.width } : undefined}>
        <div class="modal-head">
          <span class="grow">{props.title}</span>
          <button class="btn ghost icon" aria-label="Close" onClick={props.onClose}>✕</button>
        </div>
        <div class="modal-body">{props.children}</div>
        {props.footer && <div class="modal-foot">{props.footer}</div>}
      </div>
    </div>
  );
}

export function Toasts() {
  return (
    <div class="toasts" aria-live="polite">
      {toasts.value.map((t) => (
        <div key={t.id} class={`toast ${t.kind}`}>{t.msg}</div>
      ))}
    </div>
  );
}
