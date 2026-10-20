export function Tabs<T extends string>(props: { value: T; options: { value: T; label: string; badge?: string | number }[]; onChange: (v: T) => void }) {
  return (
    <div class="tabs" role="tablist">
      {props.options.map((o) => (
        <button key={o.value} role="tab" aria-selected={o.value === props.value} class={`tab ${o.value === props.value ? "active" : ""}`} onClick={() => props.onChange(o.value)}>
          {o.label}{o.badge !== undefined && <span class="chip" style={{ marginLeft: 6, height: 18 }}>{o.badge}</span>}
        </button>
      ))}
    </div>
  );
}
