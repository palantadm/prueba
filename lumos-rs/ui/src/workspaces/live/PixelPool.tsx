import { useState } from "preact/hooks";
import type { PixelKind, PixelMap } from "../../types";
import { rt, selection, send, show, toast } from "../../store";
import { hexToRgb, rgbToHex } from "../../lib/attrs";
import { Modal, NumberField, Select, TextField, Toggle } from "../../ui";
import { t } from "../../i18n";

/* Pixel maps: a row / grid of fixtures painted with a moving pattern (rainbow, gradient, chase…). */

const KINDS: { value: PixelKind; label: () => string }[] = [
  { value: "rainbow", label: () => t("Rainbow") }, { value: "gradient", label: () => t("Gradient") }, { value: "wave", label: () => t("Wave") },
  { value: "chase", label: () => t("Chase bar") }, { value: "noise", label: () => t("Noise") }, { value: "strobe", label: () => t("Strobe") }, { value: "solid", label: () => t("Solid colour") },
];
const fail = (e: unknown) => toast(String((e as Error)?.message ?? e), "err");

function blank(n: number): PixelMap {
  const fx = [...selection.value];
  return { id: 0, name: t("Pixel map {n}", { n: n + 1 }), fixtures: fx, width_px: Math.max(1, Math.min(fx.length || 8, 16)), zigzag: false, kind: "rainbow", color_a: [1, 0, 0], color_b: [0, 0, 1], speed: 0.25, size: 1, angle: 0, width: 0.25, softness: 0.3, level: 1, drive_dimmer: true };
}

export function PixelPool() {
  const maps = show.value?.pixelmaps ?? [];
  const running = new Set(rt.value?.pixels_running ?? []);
  const [edit, setEdit] = useState<PixelMap | null>(null);
  return (
    <>
      <div class="lv-selbar">
        <button class="btn primary" style={{ minHeight: 44 }} disabled={selection.value.length === 0} onClick={() => setEdit(blank(maps.length))}>{t("+ New pixel map")}</button>
        <span class="dim" style={{ alignSelf: "center" }}>{t("uses the selection in order ({n})", { n: selection.value.length })}</span>
      </div>
      <div class="lv-scroll">
        {maps.length === 0 && <div class="dim" style={{ padding: 12 }}>{t("No pixel maps yet. Select a row of fixtures (in order) and create one.")}</div>}
        {maps.map((m) => {
          const on = running.has(m.id);
          return (
            <div class="lv-fxrow" key={m.id}>
              <button class={`btn ${on ? "on" : ""}`} style={{ minHeight: 44, minWidth: 56 }} aria-pressed={on} aria-label={t("Toggle {name}", { name: m.name })} onClick={() => void send({ type: "pixel_run", id: m.id, on: !on }).catch(fail)}>{on ? "ON" : "OFF"}</button>
              <div class="grow" style={{ cursor: "pointer", minHeight: 44 }} onClick={() => setEdit(structuredClone(m))}>
                <div style={{ fontWeight: 700 }}>{m.name}</div>
                <div class="dim" style={{ fontSize: 11 }}>{KINDS.find((k) => k.value === m.kind)?.label()} · {t("{n} fx", { n: m.fixtures.length })} · {m.width_px} {t("per row")}</div>
              </div>
              <span class="px-sw" style={{ background: `linear-gradient(90deg, ${rgbToHex(...m.color_a)}, ${m.kind === "rainbow" ? "#0f0, #00f" : rgbToHex(...m.color_b)})` }} />
            </div>
          );
        })}
      </div>
      {edit && <PixelEditor m={edit} exists={maps.some((x) => x.id === edit.id)} onClose={() => setEdit(null)} />}
    </>
  );
}

function PixelEditor({ m: init, exists, onClose }: { m: PixelMap; exists: boolean; onClose: () => void }) {
  const [m, setM] = useState<PixelMap>(init);
  const up = (p: Partial<PixelMap>) => setM({ ...m, ...p });
  const save = () => void send({ type: "pixel_save", map: m }).then(onClose).catch(fail);
  const colour = (label: string, key: "color_a" | "color_b") => (
    <div class="field"><label>{label}</label><input type="color" class="input" style={{ minHeight: 48 }} value={rgbToHex(...m[key])} onInput={(e) => { const c = hexToRgb((e.target as HTMLInputElement).value); if (c) up({ [key]: c } as Partial<PixelMap>); }} /></div>
  );
  const rows = Math.ceil(m.fixtures.length / Math.max(1, m.width_px));
  return (
    <Modal title={exists ? t("Pixel map") : t("New pixel map")} width={560} onClose={onClose}
      footer={<>{exists && <button class="btn tall danger" onClick={() => void send({ type: "pixel_remove", id: m.id }).then(onClose).catch(fail)}>{t("Delete")}</button>}<span class="grow" /><button class="btn tall" onClick={onClose}>{t("Cancel")}</button><button class="btn tall primary" disabled={m.fixtures.length === 0} onClick={save}>{t("Save")}</button></>}>
      <div class="col" style={{ gap: 12 }}>
        <TextField label={t("Name")} value={m.name} onChange={(name) => up({ name })} />
        <div class="row" style={{ gap: 10, flexWrap: "wrap" }}>
          <span>{t("{n} fixtures in pixel order", { n: m.fixtures.length })} · {rows} {t("rows")}</span>
          <button class="btn" disabled={selection.value.length === 0} onClick={() => up({ fixtures: [...selection.value] })}>{t("Use current selection")}</button>
        </div>
        <div class="grid2">
          <NumberField label={t("Pixels per row")} value={m.width_px} min={1} max={512} onChange={(width_px) => up({ width_px })} />
          <div class="field"><label>{t("Wiring")}</label><Toggle checked={m.zigzag} onChange={(zigzag) => up({ zigzag })} label={t("Zig-zag (every other row backwards)")} /></div>
        </div>
        <Select<PixelKind> label={t("Pattern")} value={m.kind} options={KINDS.map((k) => ({ value: k.value, label: k.label() }))} onChange={(kind) => up({ kind })} />
        {m.kind !== "rainbow" && <div class="grid2">{colour(m.kind === "solid" ? t("Colour") : t("Colour A"), "color_a")}{m.kind !== "solid" && colour(t("Colour B / background"), "color_b")}</div>}
        <div class="grid2">
          <NumberField label={t("Speed (cycles/s, − = reverse)")} value={m.speed} min={-50} max={50} step={0.05} float onChange={(speed) => up({ speed })} />
          <NumberField label={t("Repeats across the map")} value={m.size} min={0.05} max={64} step={0.25} float onChange={(size) => up({ size })} />
          <NumberField label={t("Direction (°, 0 = left→right, 90 = down)")} value={m.angle} min={-360} max={360} step={15} float onChange={(angle) => up({ angle })} />
          <NumberField label={t("Level (0–1)")} value={m.level} min={0} max={1} step={0.05} float onChange={(level) => up({ level })} />
          {m.kind === "chase" && <NumberField label={t("Bar width (0–1)")} value={m.width} min={0.02} max={1} step={0.05} float onChange={(width) => up({ width })} />}
          {m.kind === "chase" && <NumberField label={t("Edge softness (0–1)")} value={m.softness} min={0} max={1} step={0.1} float onChange={(softness) => up({ softness })} />}
        </div>
        <Toggle checked={m.drive_dimmer} onChange={(drive_dimmer) => up({ drive_dimmer })} label={t("Also hold the dimmer at full")} />
        <div class="dim">{t("Runs above cuelists and scenes and below the programmer. Select the fixtures in the order they are wired, left to right.")}</div>
      </div>
    </Modal>
  );
}
