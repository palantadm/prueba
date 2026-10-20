// Ref-counted DMX subscription: the union of every owner's request is sent to the server.
import { subscribeDmx } from "../store";

const owners = new Map<string, number[] | null>();

function apply() {
  let all = false;
  const set = new Set<number>();
  for (const v of owners.values()) { if (v === null) all = true; else v.forEach((u) => set.add(u)); }
  subscribeDmx(all ? null : [...set].sort((a, b) => a - b));
}

/** Register (or update) `owner`'s interest. `null` = all universes. */
export function requestDmx(owner: string, universes: number[] | null) { owners.set(owner, universes); apply(); }
export function releaseDmx(owner: string) { owners.delete(owner); apply(); }
