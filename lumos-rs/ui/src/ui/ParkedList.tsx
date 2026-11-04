import { rt } from "../store";
import { Modal } from "./Modal";
import { channelOwner, attrLabel } from "../lib/attrs";
import { parkedList, unparkAll, unparkChannel } from "../lib/park";
import { t, tp } from "../i18n";

/** Button showing how many channels are parked; opens the parked list. */
export function ParkedButton(p: { onOpen: () => void }) {
  const n = rt.value?.parked?.length ?? 0;
  return (
    <button class={`btn ${n ? "on" : ""}`} style={{ minHeight: 44 }} onClick={p.onOpen} aria-label={t("Parked channels")}>
      {t("Parked")}{n > 0 && <span class="chip warn" style={{ height: 18, marginLeft: 4 }}>{n}</span>}
    </button>
  );
}

export function ParkedList(p: { onClose: () => void }) {
  const list = [...parkedList()].sort((a, b) => a.universe - b.universe || a.channel - b.channel);
  return (
    <Modal title={t("Parked channels")} onClose={p.onClose} footer={<>
      <button class="btn danger" style={{ minHeight: 44 }} disabled={!list.length} onClick={() => { unparkAll(); p.onClose(); }}>{t("Unpark all")}</button>
      <span class="sp" />
      <button class="btn" style={{ minHeight: 44 }} onClick={p.onClose}>{t("Close")}</button>
    </>}>
      {list.length === 0 ? <div class="dim">{t("No channels are parked. Park a channel from the Desk or by pinning it in the Monitor.")}</div> : (
        <>
          <div class="dim">{tp(list.length, "{n} channel held at a fixed value", "{n} channels held at fixed values")}</div>
          <div class="col" style={{ gap: 6, maxHeight: 360, overflow: "auto" }}>
            {list.map((c) => {
              const o = channelOwner(c.universe, c.channel);
              return (
                <div key={`${c.universe}:${c.channel}`} class="row" style={{ minHeight: 48, padding: "0 8px", background: "var(--bg-2)", borderRadius: 6 }}>
                  <b class="mono" style={{ minWidth: 70 }}>U{c.universe}.{String(c.channel).padStart(3, "0")}</b>
                  <span class="mono" style={{ color: "var(--live)", minWidth: 56 }}>{c.value} · {Math.round((c.value / 255) * 100)}%</span>
                  <span class="grow dim" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{o ? `#${o.fx.id} ${o.fx.name} — ${attrLabel(o.attr)}` : t("unpatched")}</span>
                  <button class="btn" style={{ minHeight: 44 }} onClick={() => unparkChannel(c.universe, c.channel)}>{t("Unpark")}</button>
                </div>
              );
            })}
          </div>
        </>
      )}
    </Modal>
  );
}
