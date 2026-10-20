// Feedback presets for common controllers and the "auto-build from executors" generator.
import type { FeedbackTarget, Show, VcPage, VcWidget } from "../types";
import { newPage, newWidget } from "./model";
import { t } from "../i18n";

export type FbPreset = {
  id: string; label: string; hint: string;
  /** colour/velocity choices (plain controller velocity values) */
  colors?: { label: string; on: number; off?: number }[];
  make: (opts: { number: number; channel: number; on?: number; off?: number }) => FeedbackTarget;
};

const midi = (kind: "note" | "cc", scale: boolean, on = 127, off = 0) =>
  (o: { number: number; channel: number; on?: number; off?: number }): FeedbackTarget =>
    ({ type: "midi", port: null, kind, channel: o.channel, number: o.number, on: o.on ?? on, off: o.off ?? off, scale });

export const FB_PRESETS: FbPreset[] = [
  { id: "note", label: "Generic note LED", hint: "Note on (127) when active, off (0) otherwise.", make: midi("note", false) },
  { id: "cc_led", label: "Generic CC LED", hint: "Control change 127 / 0 (e.g. KORG nanoKONTROL2 buttons).", make: midi("cc", false) },
  { id: "cc_fader", label: "CC fader / motor fader (scaled)", hint: "Sends the 0..127 position as CC — BCF2000, X-Touch (standard mode), nanoKONTROL.", make: midi("cc", true) },
  {
    id: "apc", label: "Akai APC mini / APC40 pad", hint: "Pad LED colour is the note velocity.",
    colors: [{ label: "Green", on: 1 }, { label: "Green blink", on: 2 }, { label: "Red", on: 3 }, { label: "Red blink", on: 4 }, { label: "Yellow", on: 5 }, { label: "Yellow blink", on: 6 }],
    make: midi("note", false, 1, 0),
  },
  {
    id: "lp_classic", label: "Novation Launchpad (Mini MK1 / S)", hint: "Common velocities: red 15, green 60, amber 63, yellow 62.",
    colors: [{ label: "Green", on: 60 }, { label: "Red", on: 15 }, { label: "Amber", on: 63 }, { label: "Yellow", on: 62 }],
    make: midi("note", false, 60, 0),
  },
  {
    id: "lp_mk2", label: "Novation Launchpad MK2 / Pro (palette)", hint: "Palette index as velocity: green 21, red 5, yellow 13, blue 45, white 3 — verify with your programmer's reference.",
    colors: [{ label: "Green", on: 21 }, { label: "Red", on: 5 }, { label: "Yellow", on: 13 }, { label: "Blue", on: 45 }, { label: "White", on: 3 }],
    make: midi("note", false, 21, 0),
  },
  { id: "osc", label: "TouchOSC / OSC float", hint: "Sends 1.0 / 0.0 (or the 0..1 value for sliders) to host:port/address.",
    make: (o) => ({ type: "osc", host: "192.168.1.50", port: 9000, address: `/lumos/${o.number}`, on: o.on ?? 1, off: o.off ?? 0 }) },
];

/** Page mirroring executor page 1: faders (level), GO / FLASH buttons, cue list widgets. */
export function autoBuildFromExecutors(show: Show, startId: number, pageId: number): { page: VcPage; nextId: number } {
  const pg = newPage(pageId, t("Executors"), 16, 10);
  let id = startId;
  const execs = show.executors.filter((e) => e.page === 0 && e.kind.type !== "empty").sort((a, b) => a.slot - b.slot);
  const add = (w: VcWidget) => pg.widgets.push(w);
  let col = 0;
  for (const e of execs) {
    const label = e.label || t("Exec {n}", { n: e.slot + 1 });
    if (e.kind.type === "cuelist") {
      const f = newWidget("slider", id++, col, 0); f.label = label; f.h = 6; f.w = 2; f.action = { type: "exec", exec: e.id, op: "level" }; f.color = "#f5a524"; add(f);
      const go = newWidget("button", id++, col, 6); go.label = t("GO"); go.w = 2; go.h = 2; go.action = { type: "exec", exec: e.id, op: "go" }; go.color = "#35c46a"; add(go);
      const fl = newWidget("button", id++, col, 8); fl.label = t("FLASH"); fl.w = 2; fl.h = 2; fl.mode = "momentary"; fl.action = { type: "exec", exec: e.id, op: "flash" }; fl.color = "#3cc4e8"; add(fl);
    } else if (e.kind.type === "group") {
      const f = newWidget("slider", id++, col, 0); f.label = label; f.h = 6; f.w = 2; f.action = { type: "exec", exec: e.id, op: "level" }; f.color = "#f2d98d"; add(f);
      const fl = newWidget("button", id++, col, 6); fl.label = t("FLASH"); fl.w = 2; fl.h = 2; fl.mode = "momentary"; fl.action = { type: "exec", exec: e.id, op: "flash" }; add(fl);
    } else if (e.kind.type === "effect") {
      const b = newWidget("button", id++, col, 6); b.label = label; b.w = 2; b.h = 2; b.mode = "toggle"; b.action = { type: "exec", exec: e.id, op: "toggle" }; b.color = "#b38cff"; add(b);
    } else if (e.kind.type === "master") {
      const f = newWidget("slider", id++, col, 0); f.label = label; f.h = 6; f.w = 2; f.action = { type: "grand_master" }; f.color = "#e5484d"; add(f);
    }
    col += 2;
    if (col + 2 > pg.cols) break;
  }
  if (!pg.widgets.length) {
    const gm = newWidget("slider", id++, 0, 0); gm.label = t("Grand master"); gm.h = 6; gm.action = { type: "grand_master" }; add(gm);
    const bo = newWidget("button", id++, 1, 0); bo.label = t("BLACKOUT"); bo.mode = "toggle"; bo.action = { type: "blackout" }; bo.color = "#e5484d"; add(bo);
  }
  // one cue list widget per cuelist, to the right
  let cy = 0;
  for (const cl of show.cuelists.slice(0, 3)) {
    const c = newWidget("cue_list", id++, Math.min(col, pg.cols - 4), cy);
    c.label = cl.name; c.action = { type: "cuelist", cuelist: cl.id, op: "go" }; c.w = Math.min(4, pg.cols - c.x); c.h = 3; pg.widgets.push(c); cy += 3;
  }
  return { page: pg, nextId: id };
}
