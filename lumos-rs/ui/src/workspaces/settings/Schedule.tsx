import { useState } from "preact/hooks";
import type { ScheduleRule, ScheduleWhen } from "../../types";
import { rt, send, show, toast } from "../../store";
import { EmptyState, Modal, NumberField, Segmented, TextField, Toggle } from "../../ui";
import { useSettingsDraft } from "../../lib/draft";
import { SaveBar, SettingsPage } from "./Common";
import { t } from "../../i18n";

/* Calendar and astronomical clock: run a console line at a time, at sunrise or at sunset on chosen weekdays. */

const DAYS = () => [t("Mon"), t("Tue"), t("Wed"), t("Thu"), t("Fri"), t("Sat"), t("Sun")];
const fail = (e: unknown) => toast(String((e as Error)?.message ?? e), "err");
const whenText = (w: ScheduleWhen) => {
  const off = (n: number) => (n === 0 ? "" : ` ${n > 0 ? "+" : "−"}${Math.abs(n)} min`);
  return w.type === "time" ? `${String(w.hour).padStart(2, "0")}:${String(w.minute).padStart(2, "0")}` : w.type === "sunrise" ? `${t("Sunrise")}${off(w.offset_min)}` : `${t("Sunset")}${off(w.offset_min)}`;
};
const daysText = (d: number) => (d === 127 ? t("Every day") : DAYS().filter((_, i) => d & (1 << i)).join(" "));

export function SchedulePage() {
  const { draft, patch, dirty, save, revert } = useSettingsDraft();
  const [edit, setEdit] = useState<ScheduleRule | null>(null);
  if (!draft) return <EmptyState title={t("Waiting for the engine…")} />;
  const rules = show.value?.schedule ?? [];
  const s = rt.value?.schedule;
  const loc = draft.location;
  const geo = () => navigator.geolocation?.getCurrentPosition((p) => patch((d) => { d.location = { lat: Math.round(p.coords.latitude * 1e4) / 1e4, lon: Math.round(p.coords.longitude * 1e4) / 1e4 }; }), () => toast(t("Could not get the location"), "err"));
  return (
    <SettingsPage title={t("Schedule")} sub={t("Run things by the clock: lights on at sunset, a scene every weekday at 8:00…")} bar={<SaveBar {...{ dirty, save, revert }} />}>
      <div class="panel">
        <div class="panel-head">{t("Location (for sunrise and sunset)")}</div>
        <div class="panel-body grid2">
          <NumberField label={t("Latitude (° N)")} value={loc?.lat ?? 0} min={-90} max={90} step={0.01} float onChange={(lat) => patch((d) => { d.location = { lat, lon: d.location?.lon ?? 0 }; })} />
          <NumberField label={t("Longitude (° E, west is negative)")} value={loc?.lon ?? 0} min={-180} max={180} step={0.01} float onChange={(lon) => patch((d) => { d.location = { lat: d.location?.lat ?? 0, lon }; })} />
          <div class="row" style={{ gap: 8 }}><button class="btn" onClick={geo}>{t("Use my location")}</button>{loc && <button class="btn" onClick={() => patch((d) => { d.location = null; })}>{t("Clear")}</button>}</div>
          <div class="mono">{s ? `${t("Now")} ${s.now} · ☀↑ ${s.sunrise ?? "—"} · ☀↓ ${s.sunset ?? "—"}` : ""}</div>
        </div>
      </div>
      <div class="panel">
        <div class="panel-head row"><span class="grow">{t("Rules")}</span><button class="btn primary" onClick={() => setEdit({ id: 0, name: t("New rule"), enabled: true, when: { type: "time", hour: 19, minute: 0 }, days: 127, action: "" })}>{t("+ New rule")}</button></div>
        <div class="panel-body col" style={{ gap: 6 }}>
          {rules.length === 0 && <div class="dim">{t("No rules yet.")}</div>}
          {rules.map((r) => {
            const n = s?.next.find((x) => x.rule === r.id);
            return (
              <div class="vrow" key={r.id}>
                <Toggle checked={r.enabled} onChange={(enabled) => void send({ type: "schedule_save", rule: { ...r, enabled } }).catch(fail)} label="" />
                <div class="grow" style={{ cursor: "pointer", minHeight: 44 }} onClick={() => setEdit(structuredClone(r))}>
                  <b>{r.name}</b> <span class="dim">· {whenText(r.when)} · {daysText(r.days)}</span><br />
                  <span class="mono dim">{r.action}</span>
                </div>
                <span class="mono dim">{r.enabled && n ? `${n.days_ahead === 0 ? t("today") : n.days_ahead === 1 ? t("tomorrow") : t("in {n} days", { n: n.days_ahead })} ${n.at}` : ""}</span>
              </div>
            );
          })}
        </div>
      </div>
      {edit && <RuleEditor r={edit} exists={rules.some((x) => x.id === edit.id)} onClose={() => setEdit(null)} />}
    </SettingsPage>
  );
}

function RuleEditor({ r: init, exists, onClose }: { r: ScheduleRule; exists: boolean; onClose: () => void }) {
  const [r, setR] = useState<ScheduleRule>(init);
  const up = (p: Partial<ScheduleRule>) => setR({ ...r, ...p });
  const kind = r.when.type;
  const setKind = (k: ScheduleWhen["type"]) => up({ when: k === "time" ? { type: "time", hour: 19, minute: 0 } : { type: k, offset_min: 0 } });
  return (
    <Modal title={exists ? t("Rule") : t("New rule")} width={520} onClose={onClose}
      footer={<>{exists && <button class="btn tall danger" onClick={() => void send({ type: "schedule_remove", id: r.id }).then(onClose).catch(fail)}>{t("Delete")}</button>}<span class="grow" /><button class="btn tall" onClick={onClose}>{t("Cancel")}</button><button class="btn tall primary" onClick={() => void send({ type: "schedule_save", rule: r }).then(onClose).catch(fail)}>{t("Save")}</button></>}>
      <div class="col" style={{ gap: 12 }}>
        <TextField label={t("Name")} value={r.name} onChange={(name) => up({ name })} />
        <Segmented<ScheduleWhen["type"]> value={kind} onChange={setKind} options={[{ value: "time", label: t("Time") }, { value: "sunrise", label: t("Sunrise") }, { value: "sunset", label: t("Sunset") }]} />
        {r.when.type === "time" ? (
          <div class="grid2">
            <NumberField label={t("Hour")} value={r.when.hour} min={0} max={23} onChange={(hour) => up({ when: { type: "time", hour, minute: (r.when as { minute: number }).minute } })} />
            <NumberField label={t("Minute")} value={r.when.minute} min={0} max={59} onChange={(minute) => up({ when: { type: "time", hour: (r.when as { hour: number }).hour, minute } })} />
          </div>
        ) : (
          <NumberField label={t("Minutes after (− before) the sun")} value={r.when.offset_min} min={-360} max={360} onChange={(offset_min) => up({ when: { type: (r.when as { type: "sunrise" | "sunset" }).type, offset_min } })} />
        )}
        <div class="field"><label>{t("Days")}</label>
          <div class="row" style={{ gap: 6, flexWrap: "wrap" }}>
            {DAYS().map((d, i) => <button key={i} class={`btn ${r.days & (1 << i) ? "primary" : ""}`} aria-pressed={!!(r.days & (1 << i))} style={{ minWidth: 52, minHeight: 48 }} onClick={() => up({ days: r.days ^ (1 << i) })}>{d}</button>)}
          </div>
        </div>
        <TextField label={t("Console line to run")} mono value={r.action} placeholder="scene 4 on" onChange={(action) => up({ action })} />
        <div class="dim">{t("Any console line: scene 4 on · go cuelist 2 · macro 3 · gm 80 · blackout off · sound \"aviso.wav\". The engine runs it even with no screen open.")}</div>
      </div>
    </Modal>
  );
}
