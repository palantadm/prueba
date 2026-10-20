import { useEffect, useRef, useState } from "preact/hooks";
import { dmxVersion, subscribeDmx } from "../store";

/** Re-render at most `hz` times/s while DMX frames are arriving. Also subscribes to all universes. */
export function useDmxTick(hz = 10) {
  const [n, set] = useState(0);
  useEffect(() => {
    subscribeDmx(null);
    let last = -1;
    const t = setInterval(() => {
      const v = dmxVersion.peek();
      if (v !== last) { last = v; set((x) => x + 1); }
    }, 1000 / hz);
    return () => clearInterval(t);
  }, [hz]);
  return n;
}

/** Long-press (touch) + context-menu (mouse) → one callback. */
export function useLongPress(cb: () => void, ms = 550) {
  const t = useRef<number>(0);
  const fired = useRef(false);
  const cancel = () => { clearTimeout(t.current); };
  return {
    onPointerDown: (e: PointerEvent) => {
      if (e.pointerType === "mouse") return;
      fired.current = false;
      t.current = window.setTimeout(() => { fired.current = true; cb(); }, ms);
    },
    onPointerUp: cancel,
    onPointerLeave: cancel,
    onPointerCancel: cancel,
    onContextMenu: (e: Event) => { e.preventDefault(); cb(); },
    /** true right after a long-press fired — use to swallow the click */
    wasLong: () => { const f = fired.current; fired.current = false; return f; },
  };
}

export function useKey(handler: (e: KeyboardEvent) => void, deps: unknown[] = []) {
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      if (el?.matches?.("input,textarea,select,[contenteditable]")) return;
      handler(e);
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}

/** rAF-driven clock for animations. */
export function useClock(active = true) {
  const [t, set] = useState(0);
  useEffect(() => {
    if (!active) return;
    let id = 0;
    const loop = (ts: number) => { set(ts / 1000); id = requestAnimationFrame(loop); };
    id = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(id);
  }, [active]);
  return t;
}
