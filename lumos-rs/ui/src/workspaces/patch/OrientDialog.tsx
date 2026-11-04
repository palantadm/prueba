import { useState } from "preact/hooks";
import { send, show, toast } from "../../store";
import { Modal, Toggle } from "../../ui";
import { t } from "../../i18n";

/** Pan/tilt wiring of the selected moving heads: mounted upside down, on their side, mirrored rigs. */
export function OrientDialog(props: { ids: number[]; onClose: () => void }) {
  const fx = (show.value?.fixtures ?? []).filter((f) => props.ids.includes(f.id));
  const first = fx[0]?.orient;
  const [o, setO] = useState({ swap_pan_tilt: first?.swap_pan_tilt ?? false, invert_pan: first?.invert_pan ?? false, invert_tilt: first?.invert_tilt ?? false });
  const apply = async () => {
    for (const f of fx) await send({ type: "fixture_update", fixture: { ...f, orient: o } }, { quiet: true }).catch((e) => toast(String((e as Error).message ?? e), "err"));
    toast(t("Pan/tilt applied to {n} fixtures", { n: fx.length }), "ok", 2000);
    props.onClose();
  };
  return (
    <Modal title={t("Pan / tilt wiring")} width={480} onClose={props.onClose} footer={<><span class="grow dim">{t("{n} fixtures selected", { n: fx.length })}</span><button class="btn tall" onClick={props.onClose}>{t("Cancel")}</button><button class="btn tall primary" disabled={fx.length === 0} onClick={() => void apply()}>{t("Apply")}</button></>}>
      <div class="col" style={{ gap: 10 }}>
        <Toggle checked={o.invert_pan} onChange={(v) => setO({ ...o, invert_pan: v })} label={t("Invert pan")} />
        <Toggle checked={o.invert_tilt} onChange={(v) => setO({ ...o, invert_tilt: v })} label={t("Invert tilt")} />
        <Toggle checked={o.swap_pan_tilt} onChange={(v) => setO({ ...o, swap_pan_tilt: v })} label={t("Swap pan and tilt")} />
        <div class="dim">{t("Applied at the output, so cues, palettes and effects keep their values. Swap is done first, then the inversions.")}</div>
      </div>
    </Modal>
  );
}
