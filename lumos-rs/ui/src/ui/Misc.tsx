import type { ComponentChildren } from "preact";

export function EmptyState(props: { title: string; hint?: string; children?: ComponentChildren; icon?: string }) {
  return (
    <div class="empty">
      <div class="empty-icon" aria-hidden="true">{props.icon ?? "◌"}</div>
      <div style={{ fontWeight: 700, fontSize: "var(--fs-lg)" }}>{props.title}</div>
      {props.hint && <div class="dim" style={{ maxWidth: 420, textAlign: "center" }}>{props.hint}</div>}
      {props.children}
    </div>
  );
}

export function Kbd(props: { children: ComponentChildren }) {
  return <kbd class="kbd">{props.children}</kbd>;
}

export function StatusDot(props: { state: "ok" | "err" | "warn" | "off"; title?: string }) {
  return <span class={`dot ${props.state === "off" ? "" : props.state}`} title={props.title} role="img" aria-label={props.title ?? props.state} />;
}

export function PageHead(props: { title: string; sub?: string; children?: ComponentChildren }) {
  return (
    <div class="page-head">
      <div class="grow"><h2>{props.title}</h2>{props.sub && <div class="dim">{props.sub}</div>}</div>
      <div class="row">{props.children}</div>
    </div>
  );
}
