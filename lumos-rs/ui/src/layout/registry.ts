import type { ComponentType } from "preact";

/** Filled by workspaces/index.ts (avoids an import cycle with the layout workspace that hosts them). */
export type HostedWorkspace = { id: string; label: string; component: ComponentType };
export const registry: { list: HostedWorkspace[] } = { list: [] };
