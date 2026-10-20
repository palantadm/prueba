import { shell } from "./es/shell";
import { live } from "./es/live";
import { stage } from "./es/stage";
import { vc } from "./es/vc";

export const es: Record<string, string> = { ...shell, ...live, ...stage, ...vc };
