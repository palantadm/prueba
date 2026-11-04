import type { ComponentChildren } from "preact";
import { useEffect, useRef, useState } from "preact/hooks";

/** Fixed-row-height windowed list. Keeps 256+ rows cheap. */
export function VirtualList<T>(props: { items: T[]; rowHeight: number; render: (item: T, index: number) => ComponentChildren; overscan?: number; class?: string; label?: string }) {
  const box = useRef<HTMLDivElement>(null);
  const [top, setTop] = useState(0);
  const [h, setH] = useState(600);
  useEffect(() => {
    const el = box.current!;
    const ro = new ResizeObserver(() => setH(el.clientHeight));
    ro.observe(el);
    setH(el.clientHeight);
    return () => ro.disconnect();
  }, []);
  const rh = props.rowHeight, os = props.overscan ?? 6;
  const start = Math.max(0, Math.floor(top / rh) - os);
  const end = Math.min(props.items.length, Math.ceil((top + h) / rh) + os);
  return (
    <div ref={box} class={`scroll ${props.class ?? ""}`} role="list" aria-label={props.label} onScroll={(e) => setTop((e.target as HTMLElement).scrollTop)}>
      <div style={{ height: props.items.length * rh, position: "relative" }}>
        <div style={{ position: "absolute", top: start * rh, left: 0, right: 0 }}>
          {props.items.slice(start, end).map((it, i) => props.render(it, start + i))}
        </div>
      </div>
    </div>
  );
}
