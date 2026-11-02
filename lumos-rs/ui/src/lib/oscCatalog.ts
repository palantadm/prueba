import { computed, signal } from "@preact/signals";
import { show } from "../store";
import type { OscAction, OscSoftware } from "../types";

/** Programs shipped with Lumos (read-only) followed by the ones the user added to the show. */
const builtin = signal<OscSoftware[]>([]);
let loading: Promise<void> | null = null;

export function loadOscCatalog(): Promise<void> {
  loading ??= fetch("/api/osc/catalog")
    .then((r) => (r.ok ? r.json() : []))
    .then((j: OscSoftware[]) => { builtin.value = j; })
    .catch(() => { loading = null; });
  return loading;
}

export const oscCatalog = computed<OscSoftware[]>(() => {
  const user = (show.value?.osc_software ?? []).filter((u) => builtin.value.every((b) => b.id !== u.id));
  return [...builtin.value, ...user];
});

export const findAction = (id: string): { sw: OscSoftware; a: OscAction } | null => {
  const [s, a] = id.split("/");
  const sw = oscCatalog.value.find((x) => x.id === s);
  const act = sw?.actions.find((x) => x.id === a);
  return sw && act ? { sw, a: act } : null;
};

/** `{key}` placeholders used by an action (declared params first, then any extra found in the text). */
export function placeholders(address: string, args: string[]): string[] {
  const out: string[] = [];
  for (const m of [address, ...args].join(" ").matchAll(/\{([^}]+)\}/g)) if (!out.includes(m[1])) out.push(m[1]);
  return out;
}
