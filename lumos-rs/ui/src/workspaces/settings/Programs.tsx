import { useEffect, useState } from "preact/hooks";
import type { OscAction, OscSoftware, OscTarget } from "../../types";
import { fire, send, show, toast } from "../../store";
import { EmptyState, Modal, NumberField, Select, TextField } from "../../ui";
import { SettingsPage } from "./Common";
import { loadOscCatalog, oscCatalog, placeholders } from "../../lib/oscCatalog";
import { t } from "../../i18n";

/* Other programs on the network that the console sends OSC to (Resolume, QLab…), and programs the user adds. */

const KIND_LABEL = () => ({ trigger: t("Button"), toggle: t("Switch"), fader: t("Fader") });

function TargetDialog({ init, onClose }: { init: OscTarget; onClose: () => void }) {
  const [x, setX] = useState(init);
  const sw = oscCatalog.value.find((s) => s.id === x.software);
  return (
    <Modal title={x.id ? t("Edit program") : t("New program on the network")} onClose={onClose} footer={<>
      <span class="sp" /><button class="btn" onClick={onClose}>{t("Cancel")}</button>
      <button class="btn primary" onClick={() => { void send({ type: "osc_target_save", target: { ...x, name: x.name.trim() || sw?.name || "" } }).then(onClose).catch(() => {}); }}>{t("Save")}</button>
    </>}>
      <TextField label={t("Name")} value={x.name} onChange={(name) => setX({ ...x, name })} placeholder="PC vídeo" />
      <div class="field"><label>{t("Program")}</label>
        <select class="input" value={x.software} onChange={(e) => {
          const id = (e.target as HTMLSelectElement).value;
          const s = oscCatalog.value.find((q) => q.id === id);
          setX({ ...x, software: id, port: s ? s.port : x.port, name: x.name || s?.name || "" });
        }}>
          <option value="">{t("Other / generic")}</option>
          {oscCatalog.value.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
        </select>
      </div>
      <TextField label={t("IP address or name")} value={x.host} onChange={(host) => setX({ ...x, host })} placeholder="192.168.1.50" mono />
      <NumberField label={t("Port")} value={x.port} min={1} max={65535} onChange={(port) => setX({ ...x, port })} />
      {sw && <p class="muted" style={{ fontSize: "var(--fs-xs)" }}>{sw.note}</p>}
    </Modal>
  );
}

const emptyAction = (): OscAction => ({ id: "", label: "", group: "", address: "/", args: [], value: "float", kind: "trigger", params: [], min: 0, max: 1, note: "" });

function SoftwareDialog({ init, onClose }: { init: OscSoftware; onClose: () => void }) {
  const [x, setX] = useState<OscSoftware>(init);
  const isNew = !show.value?.osc_software?.some((s) => s.id === init.id);
  const setA = (i: number, f: (a: OscAction) => void) => setX((p) => { const n = structuredClone(p); f(n.actions[i]); return n; });
  // keep the declared params in step with the {placeholders} typed in the address
  const sync = (a: OscAction) => {
    const keys = placeholders(a.address, a.args);
    a.params = keys.map((k) => a.params.find((p) => p.key === k) ?? { key: k, label: k, default: "1" });
  };
  const slug = (s: string) => s.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return (
    <Modal title={isNew ? t("New own program") : t("Edit own program")} onClose={onClose} footer={<>
      {!isNew && <button class="btn danger" onClick={() => { void send({ type: "osc_software_remove", id: x.id }).then(onClose).catch(() => {}); }}>{t("Delete")}</button>}
      <span class="sp" /><button class="btn" onClick={onClose}>{t("Cancel")}</button>
      <button class="btn primary" onClick={() => {
        const out = structuredClone(x);
        out.id = out.id || slug(out.name);
        out.actions.forEach((a) => { a.id = a.id || slug(a.label); });
        void send({ type: "osc_software_save", software: out }).then(onClose).catch(() => {});
      }}>{t("Save")}</button>
    </>}>
      <TextField label={t("Name")} value={x.name} onChange={(name) => setX({ ...x, name })} />
      <NumberField label={t("Usual port")} value={x.port} min={1} max={65535} onChange={(port) => setX({ ...x, port })} />
      <div class="panel-head">{t("Actions")}</div>
      {x.actions.map((a, i) => (
        <div key={i} class="col" style={{ gap: 6, borderTop: "1px solid var(--line)", paddingTop: 8 }}>
          <div class="row" style={{ gap: 6 }}>
            <input class="input grow" value={a.label} placeholder={t("Name (e.g. Mute channel)")} onInput={(e) => setA(i, (q) => { q.label = (e.target as HTMLInputElement).value; })} />
            <select class="input" value={a.kind} onChange={(e) => setA(i, (q) => { q.kind = (e.target as HTMLSelectElement).value as OscAction["kind"]; if (q.kind === "fader" && q.value === "none") q.value = "float"; })}>
              {Object.entries(KIND_LABEL()).map(([k, l]) => <option key={k} value={k}>{l}</option>)}
            </select>
            <button class="btn sm" aria-label={t("Remove")} onClick={() => setX((p) => ({ ...p, actions: p.actions.filter((_, j) => j !== i) }))}>✕</button>
          </div>
          <input class="input mono" value={a.address} placeholder="/ch/{channel}/mute" onInput={(e) => setA(i, (q) => { q.address = (e.target as HTMLInputElement).value; sync(q); })} />
          <div class="row wrap" style={{ gap: 6 }}>
            <select class="input" value={a.value} aria-label={t("Value sent")} onChange={(e) => setA(i, (q) => { q.value = (e.target as HTMLSelectElement).value as OscAction["value"]; })}>
              <option value="float">{t("Sends a decimal number")}</option>
              <option value="int">{t("Sends a whole number")}</option>
              <option value="none">{t("Sends no number")}</option>
            </select>
            {a.kind === "fader" && <>
              <NumberField class="bare" float step={0.1} value={a.min} onChange={(v) => setA(i, (q) => { q.min = v; })} />
              <span class="faint">→</span>
              <NumberField class="bare" float step={0.1} value={a.max} onChange={(v) => setA(i, (q) => { q.max = v; })} />
            </>}
          </div>
          {a.params.map((p, k) => (
            <div key={p.key} class="row" style={{ gap: 6 }}>
              <span class="mono faint">{`{${p.key}}`}</span>
              <input class="input grow" value={p.label} aria-label={t("Name")} onInput={(e) => setA(i, (q) => { q.params[k].label = (e.target as HTMLInputElement).value; })} />
              <input class="input" style={{ width: 80 }} value={p.default} aria-label={t("Default")} onInput={(e) => setA(i, (q) => { q.params[k].default = (e.target as HTMLInputElement).value; })} />
            </div>
          ))}
        </div>
      ))}
      <button class="btn" onClick={() => setX((p) => ({ ...p, actions: [...p.actions, emptyAction()] }))}>{t("+ Action")}</button>
    </Modal>
  );
}

export function ProgramsPage() {
  useEffect(() => { void loadOscCatalog(); }, []);
  const [target, setTarget] = useState<OscTarget | null>(null);
  const [own, setOwn] = useState<OscSoftware | null>(null);
  const targets = show.value?.osc_targets ?? [];
  const mine = show.value?.osc_software ?? [];
  const nameOf = (id: string) => oscCatalog.value.find((s) => s.id === id)?.name ?? t("Generic");
  return (
    <SettingsPage title={t("External programs")} sub={t("Control Resolume, QLab, Ableton… from console buttons and faders, with ready-made OSC actions.")}>
      <div class="panel">
        <div class="panel-head row"><span class="grow">{t("Programs on the network")}</span>
          <button class="btn primary" onClick={() => setTarget({ id: 0, name: "", software: "resolume", host: "127.0.0.1", port: 7000 })}>{t("+ New")}</button></div>
        <div class="panel-body col" style={{ gap: 6 }}>
          {targets.length === 0 && <EmptyState title={t("No programs yet")} hint={t("Add the computer where Resolume or QLab runs. Then, in the Virtual console, use «Add from a program».")} />}
          {targets.map((x) => (
            <div key={x.id} class="row" style={{ gap: 8, alignItems: "center" }}>
              <b>{x.name}</b><span class="faint">{nameOf(x.software)}</span><span class="mono grow">{x.host}:{x.port}</span>
              <button class="btn sm" onClick={() => setTarget(x)}>{t("Edit")}</button>
              <button class="btn sm danger" onClick={() => fire({ type: "osc_target_remove", id: x.id })}>{t("Delete")}</button>
            </div>
          ))}
        </div>
      </div>
      <div class="panel">
        <div class="panel-head row"><span class="grow">{t("Programs included")}</span></div>
        <div class="panel-body col" style={{ gap: 4 }}>
          {oscCatalog.value.filter((s) => s.builtin).map((s) => <div key={s.id}><b>{s.name}</b> <span class="faint">· {s.actions.length} {s.actions.length === 1 ? t("action") : t("actions")} · {t("port")} {s.port}</span></div>)}
        </div>
      </div>
      <div class="panel">
        <div class="panel-head row"><span class="grow">{t("Own programs")}</span>
          <button class="btn" onClick={() => setOwn({ id: "", name: "", port: 8000, note: "", builtin: false, actions: [emptyAction()] })}>{t("+ Own program")}</button></div>
        <div class="panel-body col" style={{ gap: 6 }}>
          {mine.length === 0 && <span class="faint">{t("If your program is not in the list, add it here with its OSC addresses.")}</span>}
          {mine.map((s) => <div key={s.id} class="row" style={{ gap: 8 }}><b>{s.name}</b><span class="faint grow">{s.actions.length} {t("actions")}</span><button class="btn sm" onClick={() => setOwn(s)}>{t("Edit")}</button></div>)}
        </div>
      </div>
      {target && <TargetDialog init={target} onClose={() => setTarget(null)} />}
      {own && <SoftwareDialog init={own} onClose={() => setOwn(null)} />}
    </SettingsPage>
  );
}
