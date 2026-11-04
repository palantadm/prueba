// Spanish for the engine's morph preview/error strings (they arrive in English from the server).
import { t } from "./index";
import { tErr } from "./errors";

const ATTR = (a: string) => a;
type P = [RegExp, string, (m: RegExpMatchArray) => Record<string, string>];

const PATTERNS: P[] = [
  [/^Footprint grows from (\d+) to (\d+) channels and overlaps neighbouring fixtures — use re-address \(repack\)\.$/,
    "Footprint grows from {a} to {b} channels and overlaps neighbouring fixtures — use re-address (repack).", (m) => ({ a: m[1], b: m[2] })],
  [/^Not available in the new fixture: (.+)\.$/, "Not available in the new fixture: {list}.", (m) => ({ list: m[1] })],
  [/^Different resolution \(8\/16-bit\) for: (.+)\.$/, "Different resolution (8/16-bit) for: {list}.", (m) => ({ list: m[1] })],
  [/^Unknown profile '(.+)'$/, "Unknown profile “{p}”", (m) => ({ p: m[1] })],
  [/^Profile has no mode '(.+)'$/, "The profile has no mode “{m}”", (m) => ({ m: m[1] })],
  [/^Fixture (\d+) does not exist$/, "Fixture {id} does not exist", (m) => ({ id: m[1] })],
];

/** Translate a morph message (warning, note or error); `attr` localises attribute names inside lists. */
export function tMorph(msg: string, attr: (a: string) => string = ATTR): string {
  for (const [re, key, vars] of PATTERNS) {
    const m = msg.match(re);
    if (m) {
      const v = vars(m);
      if (v.list) v.list = v.list.split(/,\s*/).map(attr).join(", ");
      return t(key, v);
    }
  }
  const tr = t(msg);
  return tr !== msg ? tr : tErr(msg);
}
