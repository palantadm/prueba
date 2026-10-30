/** Touch switch with a label: `<button role=switch>` (60×34) inside a ≥ 48 px row. */
export function SwitchRow(p: { on: boolean; onChange: (v: boolean) => void; label: string; title?: string; disabled?: boolean }) {
  return (
    <div class="switch-row" title={p.title} style={p.disabled ? { opacity: 0.4, pointerEvents: "none" } : undefined}
      onClick={() => p.onChange(!p.on)}>
      <button type="button" role="switch" aria-checked={p.on} aria-label={p.label} class={`switch ${p.on ? "on" : ""}`}
        onClick={(e) => { e.stopPropagation(); p.onChange(!p.on); }} />
      <span>{p.label}</span>
    </div>
  );
}
