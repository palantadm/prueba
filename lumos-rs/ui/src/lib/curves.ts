import type { FadeCurve } from "../types";

/** Fade-curve shaping of linear progress x∈[0,1] (matches the engine's curve names). */
export function curveValue(c: FadeCurve, x: number): number {
  const v = Math.max(0, Math.min(1, x));
  switch (c) {
    case "s_curve": return v * v * (3 - 2 * v);
    case "square": return v * v;
    case "inv_square": return 1 - (1 - v) * (1 - v);
    case "sine": return (1 - Math.cos(Math.PI * v)) / 2;
    default: return v;
  }
}

export const CURVES: { id: FadeCurve; label: string }[] = [
  { id: "linear", label: "Linear" },
  { id: "s_curve", label: "S-curve" },
  { id: "square", label: "Square" },
  { id: "inv_square", label: "Inverse square" },
  { id: "sine", label: "Sine" },
];
