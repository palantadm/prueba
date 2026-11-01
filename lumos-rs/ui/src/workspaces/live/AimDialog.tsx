import { useRef, useState } from "preact/hooks";
import type { AimTarget } from "../../types";
import { selection, send, show, toast } from "../../store";
import { Modal, NumberField, Segmented, Select } from "../../ui";
import { t } from "../../i18n";

/* Point the selected moving heads at a spot of the stage. The pad is the stage seen from above (audience at the bottom):
   dragging it is a hands-on follow-spot — the engine recomputes pan/tilt for every selected head. */

export function AimDialog(props: { onClose: () => void }) {
  const stage = show.value?.stage;
  const [w, d] = [stage?.size[0] ?? 12, stage?.size[1] ?? 8];
  const objects = stage?.objects ?? [];
  const fixtures = show.value?.fixtures ?? [];
  const [mode, setMode] = useState<"point" | "object" | "fixture">("point");
  const [p, setP] = useState<[number, number]>([0, 0]); // x, z
  const [height, setHeight] = useState(1.4);
  const [obj, setObj] = useState(objects[0]?.id ?? 0);
  const [fx, setFx] = useState(fixtures[0]?.id ?? 0);
  const pad = useRef<HTMLDivElement>(null);
  const last = useRef(0);
  const ids = selection.value;

  const fire = (target: AimTarget, h: number, quiet = true) =>
    void send({ type: "aim", fixtures: ids, target, height: h }, { quiet }).catch((e) => toast(String((e as Error).message ?? e), "err"));
  const aimPoint = (x: number, z: number, force = false) => {
    setP([x, z]);
    const now = performance.now();
    if (!force && now - last.current < 33) return;
    last.current = now;
    fire({ type: "point", x, y: 0, z }, height);
  };
  const drag = (e: PointerEvent) => {
    const el = pad.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const x = ((e.clientX - r.left) / r.width - 0.5) * w;
    const z = ((e.clientY - r.top) / r.height - 0.5) * d;
    aimPoint(Math.round(x * 100) / 100, Math.round(z * 100) / 100, e.type === "pointerup");
  };
  const down = (e: PointerEvent) => {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag(e);
  };
  const dot = { left: `${(p[0] / w + 0.5) * 100}%`, top: `${(p[1] / d + 0.5) * 100}%` };
  const go = () => fire(mode === "point" ? { type: "point", x: p[0], y: 0, z: p[1] } : mode === "object" ? { type: "object", id: obj } : { type: "fixture", id: fx }, height, false);
  return (
    <Modal title={t("Aim at…")} onClose={props.onClose} width={560} footer={<><span class="grow dim">{t("{n} fixtures selected", { n: ids.length })}</span><button class="btn tall primary" disabled={ids.length === 0} onClick={() => { go(); props.onClose(); }}>{t("Aim and close")}</button></>}>
      <div class="col" style={{ gap: 12 }}>
        <Segmented value={mode} onChange={setMode} options={[{ value: "point", label: t("Point") }, { value: "object", label: t("Stage object") }, { value: "fixture", label: t("Fixture") }]} />
        {mode === "point" && (
          <>
            <div ref={pad} class="aim-pad" role="application" aria-label={t("Stage from above: drag to aim")} onPointerDown={down} onPointerMove={(e) => e.buttons && drag(e)} onPointerUp={drag}>
              <span class="aim-aud">{t("Audience")}</span>
              {objects.map((o) => <i key={o.id} class="aim-obj" title={o.name} style={{ left: `${((o.pos[0] - o.size[0] / 2) / w + 0.5) * 100}%`, top: `${((o.pos[2] - o.size[2] / 2) / d + 0.5) * 100}%`, width: `${(o.size[0] / w) * 100}%`, height: `${(o.size[2] / d) * 100}%` }} />)}
              <b class="aim-dot" style={dot} />
            </div>
            <div class="grid2">
              <NumberField label="X (m)" value={p[0]} step={0.1} float onChange={(x) => aimPoint(x, p[1], true)} />
              <NumberField label="Z (m)" value={p[1]} step={0.1} float onChange={(z) => aimPoint(p[0], z, true)} />
            </div>
          </>
        )}
        {mode === "object" && <Select label={t("Object")} value={obj} options={objects.map((o) => ({ value: o.id, label: `${o.name} (#${o.id})` }))} onChange={setObj} />}
        {mode === "fixture" && <Select label={t("Aim at fixture")} value={fx} options={fixtures.map((f) => ({ value: f.id, label: `${f.id} · ${f.name}` }))} onChange={setFx} />}
        <NumberField label={t("Height of the aim point (m)")} value={height} min={-2} max={10} step={0.1} float onChange={setHeight} hint={t("0 = floor · 1.4 ≈ an actor's chest · 1.7 ≈ the face")} />
        <div class="dim">{t("X: stage right (+). Z: towards the audience (+). The heads move straight away; the programmer keeps the pan and tilt.")}</div>
      </div>
    </Modal>
  );
}
