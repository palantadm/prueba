// Minimal i18n. Keys are the English source strings; Spanish lives in i18n/es/*.ts.
// `t()` reads the `lang` signal, so components re-render when the language changes.
import { signal } from "@preact/signals";
import { es } from "./es";

export type Lang = "es" | "en";

function initial(): Lang {
  try {
    const v = localStorage.getItem("lang");
    if (v === "es" || v === "en") return v;
  } catch {}
  return "es";
}

export const lang = signal<Lang>(initial());
document.documentElement.lang = lang.value;

export function setLang(l: Lang) {
  lang.value = l;
  document.documentElement.lang = l;
  try { localStorage.setItem("lang", l); } catch {}
}

const missing = new Set<string>();
(globalThis as any).__i18nMissing = missing;

function fill(s: string, vars?: Record<string, string | number>): string {
  return vars ? s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m)) : s;
}

/** Translate an English source string. `{name}` placeholders are filled from `vars`. */
export function t(key: string, vars?: Record<string, string | number>): string {
  if (lang.value === "es") {
    const v = es[key];
    if (v === undefined) missing.add(key);
    return fill(v ?? key, vars);
  }
  return fill(key, vars);
}

/** Plural helper: `tp(n, "{n} fixture", "{n} fixtures")` (both forms need Spanish entries). */
export function tp(n: number, one: string, other: string): string {
  return t(n === 1 ? one : other, { n });
}
