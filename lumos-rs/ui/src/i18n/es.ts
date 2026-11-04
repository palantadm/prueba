import { shell } from "./es/shell";
import { live } from "./es/live";
import { stage } from "./es/stage";
import { vc } from "./es/vc";
import { script } from "./es/script";
import { ai } from "./es/ai";
import { settingsUi } from "./es/settings";

export const es: Record<string, string> = { ...shell, ...live, ...stage, ...vc, ...script, ...ai, ...settingsUi };
