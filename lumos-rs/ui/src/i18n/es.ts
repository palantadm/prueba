import { shell } from "./es/shell";
import { live } from "./es/live";
import { stage } from "./es/stage";
import { vc } from "./es/vc";
import { script } from "./es/script";

export const es: Record<string, string> = { ...shell, ...live, ...stage, ...vc, ...script };
