import { t } from "../i18n";
// Attribute vocabulary (mirror of model.rs ATTRS) for pickers in Settings/Patch.
export const ATTRS: { name: string; group: string; label: string }[] = ([
  ["dimmer", "intensity", "Dimmer"], ["pan", "position", "Pan"], ["tilt", "position", "Tilt"],
  ["red", "color", "Red"], ["green", "color", "Green"], ["blue", "color", "Blue"], ["white", "color", "White"],
  ["amber", "color", "Amber"], ["uv", "color", "UV"], ["lime", "color", "Lime"], ["cto", "color", "CTO"],
  ["colorwheel", "color", "Color wheel"], ["gobo", "beam", "Gobo"], ["gobo_rot", "beam", "Gobo rot"],
  ["prism", "beam", "Prism"], ["prism_rot", "beam", "Prism rot"], ["frost", "beam", "Frost"], ["zoom", "beam", "Zoom"],
  ["focus", "beam", "Focus"], ["iris", "beam", "Iris"], ["shutter", "beam", "Shutter/Strobe"],
  ["speed", "control", "Speed"], ["effect", "control", "Effect"], ["control", "control", "Control"],
] as [string, string, string][]).map(([name, group, label]) => ({ name, group, get label() { return t(label); } }));

export const attrLabel = (a: string) => ATTRS.find((x) => x.name === a)?.label ?? a;
export const attrColor = (a: string) => `var(--attr-${ATTRS.find((x) => x.name === a)?.group ?? "control"})`;
