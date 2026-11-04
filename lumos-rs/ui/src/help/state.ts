import { signal } from "@preact/signals";

/** The Help window: closed (null) or open on a page: `topic:<id>`, `commands`, `shortcuts` or `search:<text>`. */
export const helpPage = signal<string | null>(null);
export const openHelp = (page = "topic:inicio") => { helpPage.value = page; };
export const closeHelp = () => { helpPage.value = null; };
// the desktop app's Help menu calls this
(window as unknown as { __lumosOpenHelp?: (p?: string) => void }).__lumosOpenHelp = (p) => openHelp(p || "topic:inicio");
