import { useEffect, useState } from "preact/hooks";
import { send, show, toast } from "../store";
import { Modal, Select, TextField } from "../ui";
import { t } from "../i18n";
import { loadOscCatalog, oscCatalog } from "../lib/oscCatalog";
import { nav } from "../ai/state";
import { pageId } from "./editor";

/** Put a ready-made control of another program (Resolume, QLab…) on the console: no OSC path typing. */
export function AddFromProgram({ onClose }: { onClose: () => void }) {
  useEffect(() => { void loadOscCatalog(); }, []);
  const targets = show.value?.osc_targets ?? [];
  const [target, setTarget] = useState(targets[0]?.id ?? 0);
  const tg = targets.find((x) => x.id === target);
  const sw = oscCatalog.value.find((x) => x.id === tg?.software) ?? oscCatalog.value[0];
  const [actionId, setActionId] = useState("");
  const [params, setParams] = useState<Record<string, string>>({});
  const [label, setLabel] = useState("");
  const cur = sw?.actions.find((a) => a.id === actionId) ?? sw?.actions[0];
  const full = cur && sw ? `${sw.id}/${cur.id}` : "";
  const groups = [...new Set((sw?.actions ?? []).map((a) => a.group))];
  const body = () => ({ target, action: full, params: Object.fromEntries((cur?.params ?? []).map((p) => [p.key, params[p.key] ?? p.default])) });

  if (!targets.length) {
    return (
      <Modal title={t("Add control from a program")} onClose={onClose} footer={<><span class="sp" /><button class="btn" onClick={onClose}>{t("Close")}</button><button class="btn primary" onClick={() => { nav.value = { ws: "settings", cat: "programs" }; onClose(); }}>{t("Add a program")}</button></>}>
        <p>{t("First register the program (its IP and port) in Settings → External programs. You only do it once.")}</p>
      </Modal>
    );
  }
  return (
    <Modal title={t("Add control from a program")} onClose={onClose} footer={<>
      <button class="btn" disabled={!cur} onClick={() => send({ type: "osc_send", ...body(), value: null }).then(() => toast(t("Sent"), "info", 1200)).catch((e) => toast(String(e?.message ?? e), "err"))}>{t("Try it")}</button>
      <span class="sp" />
      <button class="btn" onClick={onClose}>{t("Cancel")}</button>
      <button class="btn primary" disabled={!cur} onClick={() => { void send({ type: "vc_add_osc", ...body(), page: show.value?.vconsole.some((p) => p.id === pageId.value) ? pageId.value : null, label: label.trim() || null }).then(onClose).catch((e) => toast(String(e?.message ?? e), "err")); }}>{t("Add to console")}</button>
    </>}>
      <div class="field"><label>{t("Program on the network")}</label>
        <Select value={target} options={targets.map((x) => ({ value: x.id, label: `${x.name} · ${x.host}:${x.port}` }))} onChange={(v) => { setTarget(v); setActionId(""); setParams({}); }} />
      </div>
      {sw && <p class="muted" style={{ fontSize: "var(--fs-xs)" }}>{sw.note}</p>}
      <div class="field"><label>{t("Action")}</label>
        <select class="input" value={cur?.id ?? ""} onChange={(e) => { setActionId((e.target as HTMLSelectElement).value); setParams({}); }}>
          {groups.map((g) => (
            <optgroup key={g} label={g}>{(sw?.actions ?? []).filter((a) => a.group === g).map((a) => <option key={a.id} value={a.id}>{a.label}</option>)}</optgroup>
          ))}
        </select>
      </div>
      {cur?.params.map((p) => (
        <TextField key={`${full}.${p.key}`} label={p.label} value={params[p.key] ?? p.default} onChange={(v) => setParams({ ...params, [p.key]: v })} />
      ))}
      <TextField label={t("Name on the button (optional)")} value={label} placeholder={t("(auto)")} onChange={setLabel} />
      {cur && <p class="faint mono" style={{ fontSize: "var(--fs-xs)" }}>{cur.address.replace(/\{(\w+)\}/g, (_, k) => params[k] ?? cur.params.find((p) => p.key === k)?.default ?? k)}{cur.note ? ` — ${cur.note}` : ""}</p>}
    </Modal>
  );
}
