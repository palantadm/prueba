import type { ComponentType } from "preact";
import { t } from "../i18n";
import { Live } from "./Live";
import { Desk } from "./Desk";
import { Patch } from "./Patch";
import { Cues } from "./Cues";
import { Monitor } from "./Monitor";
import { Stage3D } from "./Stage3D";
import { Plan2D } from "./Plan2D";
import { Settings } from "./Settings";
import { VConsole } from "./VConsole";
import { Timeline } from "./Timeline";

export type Workspace = { id: string; key: string; label: string; component: ComponentType };

/** Order = header order; `key` = keyboard shortcut. */
export const workspaces: Workspace[] = [
  { id: "live", key: "1", get label() { return t("Live"); }, component: Live },
  { id: "desk", key: "2", get label() { return t("Desk"); }, component: Desk },
  { id: "cues", key: "3", get label() { return t("Cues"); }, component: Cues },
  { id: "patch", key: "4", get label() { return t("Patch"); }, component: Patch },
  { id: "stage", key: "5", get label() { return t("3D"); }, component: Stage3D },
  { id: "plan", key: "8", get label() { return t("2D"); }, component: Plan2D },
  { id: "monitor", key: "6", get label() { return t("Monitor"); }, component: Monitor },
  { id: "settings", key: "7", get label() { return t("Settings"); }, component: Settings },
  { id: "vc", key: "9", get label() { return t("Console"); }, component: VConsole },
  { id: "timeline", key: "0", get label() { return t("Timeline"); }, component: Timeline },
];
