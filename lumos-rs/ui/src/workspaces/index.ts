import type { ComponentType } from "preact";
import { Live } from "./Live";
import { Desk } from "./Desk";
import { Patch } from "./Patch";
import { Cues } from "./Cues";
import { Monitor } from "./Monitor";
import { Stage3D } from "./Stage3D";
import { Settings } from "./Settings";

export type Workspace = { id: string; key: string; label: string; component: ComponentType };

/** Order = header order; `key` = keyboard shortcut. */
export const workspaces: Workspace[] = [
  { id: "live", key: "1", label: "Live", component: Live },
  { id: "desk", key: "2", label: "Desk", component: Desk },
  { id: "cues", key: "3", label: "Cues", component: Cues },
  { id: "patch", key: "4", label: "Patch", component: Patch },
  { id: "stage", key: "5", label: "3D", component: Stage3D },
  { id: "monitor", key: "6", label: "Monitor", component: Monitor },
  { id: "settings", key: "7", label: "Settings", component: Settings },
];
