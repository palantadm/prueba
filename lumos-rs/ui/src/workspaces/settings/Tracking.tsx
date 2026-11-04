import { useState } from "preact/hooks";
import type { PsnAxis, TrackFollow } from "../../types";
import { fire, rt, send, show, toast } from "../../store";
import { EmptyState, Modal, NumberField, Select, TextField, Toggle } from "../../ui";
import { useSettingsDraft } from "../../lib/draft";
import { SaveBar, SettingsPage } from "./Common";
import { t } from "../../i18n";

/* Tracking input (PosiStageNet: BlackTrax, Zactrack…) and the follow rules that point moving heads at trackers. */

const AXES: PsnAxis[] = ["x", "y", "z"];
const axisOpts = () => AXES.map((a) => ({ value: a, label: a.toUpperCase() }));
const fmt = (v: number) => v.toFixed(2);

function FollowDialog({ init, onClose }: { init: TrackFollow; onClose: () => void }) {
  const [f, setF] = useState(init);
  const groups = show.value?.groups ?? [];
  const live = rt.value?.psn?.trackers ?? [];
  return (
    <Modal title={f.id ? t("Edit follow") : t("New follow")} onClose={onClose} footer={<>
      <span class="sp" /><button class="btn" onClick={onClose}>{t("Cancel")}</button>
      <button class="btn primary" disabled={!f.group} onClick={() => { void send({ type: "follow_save", follow: f }).then(onClose).catch(() => {}); }}>{t("Save")}</button>
    </>}>
      <TextField label={t("Name")} value={f.name} placeholder={t("(auto)")} onChange={(name) => setF({ ...f, name })} />
      <div class="field"><label>{t("Tracker")}</label>
        <div class="row" style={{ gap: 6 }}>
          <NumberField value={f.tracker} min={0} max={65535} onChange={(tracker) => setF({ ...f, tracker })} />
          <select class="input" value="" onChange={(e) => { const v = Number((e.target as HTMLSelectElement).value); if (!Number.isNaN(v) && (e.target as HTMLSelectElement).value !== "") setF({ ...f, tracker: v }); }} aria-label={t("Trackers seen")}>
            <option value="">{t("Seen now…")}</option>
            {live.map((x) => <option key={x.id} value={x.id}>{x.id}{x.name ? ` · ${x.name}` : ""}</option>)}
          </select>
        </div>
      </div>
      <div class="field"><label>{t("Moving heads that follow (group)")}</label>
        <select class="input" value={f.group} onChange={(e) => setF({ ...f, group: Number((e.target as HTMLSelectElement).value) })}>
          <option value={0}>{t("— choose a group —")}</option>
          {groups.map((g) => <option key={g.id} value={g.id}>{g.name} ({g.fixtures.length})</option>)}
        </select>
      </div>
      <NumberField label={t("Aim height above the tracker (m)")} float step={0.1} value={f.height} min={-5} max={20} onChange={(height) => setF({ ...f, height })} />
      <NumberField label={t("Smoothing (ms)")} value={f.smooth_ms} min={0} max={10000} step={50} onChange={(smooth_ms) => setF({ ...f, smooth_ms })} />
      <div class="field"><label>{t("Light")}</label>
        <div class="row" style={{ gap: 8 }}>
          <Toggle checked={f.intensity !== null} onChange={(on) => setF({ ...f, intensity: on ? 1 : null })} label={t("Turn on while tracked, off when lost")} />
          {f.intensity !== null && <NumberField float step={0.05} min={0} max={1} value={f.intensity} onChange={(intensity) => setF({ ...f, intensity })} />}
        </div>
      </div>
    </Modal>
  );
}

export function TrackingPage() {
  const { draft, patch, dirty, save, revert } = useSettingsDraft();
  const [edit, setEdit] = useState<TrackFollow | null>(null);
  if (!draft) return <EmptyState title={t("Waiting for the engine…")} />;
  const p = rt.value?.psn;
  const c = draft.psn;
  const follows = show.value?.follows ?? [];
  const groups = show.value?.groups ?? [];
  const m = c.map;
  return (
    <SettingsPage title={t("Person tracking")} sub={t("Receive positions from BlackTrax, Zactrack… (PosiStageNet) and make moving heads follow people.")} bar={<SaveBar {...{ dirty, save, revert }} />}>
      <div class="panel">
        <div class="panel-head row"><span class="grow">{t("PosiStageNet input")}</span>
          <span class={`chip ${p?.listening ? (p.error ? "warn" : "ok") : ""}`}>{p?.listening ? t("listening") : t("off")}</span></div>
        <div class="panel-body grid2">
          <Toggle checked={c.enabled} onChange={(enabled) => patch((d) => { d.psn.enabled = enabled; })} label={t("Receive tracking")} />
          <NumberField label={t("Port")} value={c.port} min={1} max={65535} onChange={(port) => patch((d) => { d.psn.port = port; })} />
          <TextField label={t("Multicast group")} mono value={c.group} onChange={(group) => patch((d) => { d.psn.group = group; })} />
          <TextField label={t("Network card IP (optional)")} mono value={c.iface} placeholder="192.168.1.20" onChange={(iface) => patch((d) => { d.psn.iface = iface; })} />
        </div>
        {p?.error && <div class="panel-body err">{p.error}</div>}
      </div>
      <div class="panel">
        <div class="panel-head">{t("Axes: tracker → stage")}</div>
        <div class="panel-body">
          <table class="tbl">
            <thead><tr><th>{t("Stage")}</th><th>{t("Takes the tracker axis")}</th><th>{t("Invert")}</th><th>{t("Offset (m)")}</th></tr></thead>
            <tbody>
              {(["x", "y", "z"] as const).map((ax, i) => (
                <tr key={ax}>
                  <td><b>{ax.toUpperCase()}</b></td>
                  <td><Select value={m[`to_${ax}`]} options={axisOpts()} onChange={(v) => patch((d) => { d.psn.map[`to_${ax}`] = v; })} /></td>
                  <td><Toggle checked={m[`invert_${ax}`]} onChange={(v) => patch((d) => { d.psn.map[`invert_${ax}`] = v; })} label={t("Invert")} /></td>
                  <td><NumberField float step={0.1} value={m.offset[i]} onChange={(v) => patch((d) => { d.psn.map.offset[i] = v; })} /></td>
                </tr>
              ))}
            </tbody>
          </table>
          <div style={{ marginTop: 8, maxWidth: 240 }}><NumberField label={t("Scale (1 = metres)")} float step={0.1} value={m.scale} onChange={(scale) => patch((d) => { d.psn.map.scale = scale; })} /></div>
        </div>
        <div class="panel-body faint">{t("Stage axes: X to the right, Y up, Z depth. Most systems send X right, Y forward, Z up.")}</div>
      </div>
      <div class="panel">
        <div class="panel-head row"><span class="grow">{t("Trackers")}{p?.system ? ` · ${p.system}` : ""}</span><span class="faint">{p?.packets ?? 0} {t("packets")}</span></div>
        <div class="panel-body">
          {(p?.trackers.length ?? 0) === 0 ? <span class="faint">{c.enabled ? t("No trackers received yet.") : t("Switch the input on and save.")}</span> : (
            <table class="tbl"><thead><tr><th>ID</th><th>{t("Name")}</th><th>X</th><th>Y</th><th>Z</th><th /></tr></thead>
              <tbody>{p!.trackers.map((x) => (
                <tr key={x.id} class={x.active ? "" : "off"}><td class="mono">{x.id}</td><td>{x.name || "—"}</td><td class="mono">{fmt(x.x)}</td><td class="mono">{fmt(x.y)}</td><td class="mono">{fmt(x.z)}</td><td>{x.active ? <span class="chip ok">{t("active")}</span> : <span class="chip">{t("lost")}</span>}</td></tr>
              ))}</tbody></table>
          )}
        </div>
      </div>
      <div class="panel">
        <div class="panel-head row"><span class="grow">{t("Follows")}</span>
          <button class="btn primary" onClick={() => setEdit({ id: 0, name: "", enabled: true, tracker: p?.trackers[0]?.id ?? 1, group: groups[0]?.id ?? 0, height: 1.5, smooth_ms: 300, intensity: 1 })}>{t("+ New follow")}</button></div>
        <div class="panel-body col" style={{ gap: 6 }}>
          {follows.length === 0 && <span class="faint">{t("A follow points a group of moving heads at a tracker. Create the group first.")}</span>}
          {follows.map((f) => {
            const on = p?.follows.find((x) => x.id === f.id)?.tracking;
            return (
              <div key={f.id} class="row" style={{ gap: 8, alignItems: "center" }}>
                <Toggle checked={f.enabled} onChange={(enabled) => fire({ type: "follow_set", id: f.id, enabled })} />
                <b>{f.name}</b>
                <span class="faint grow">{t("tracker {n}", { n: f.tracker })} → {groups.find((g) => g.id === f.group)?.name ?? `#${f.group}`}</span>
                {f.enabled && <span class={`chip ${on ? "ok" : "warn"}`}>{on ? t("tracking") : t("waiting for tracker")}</span>}
                <button class="btn sm" onClick={() => setEdit(f)}>{t("Edit")}</button>
                <button class="btn sm danger" onClick={() => fire({ type: "follow_remove", id: f.id })}>{t("Delete")}</button>
              </div>
            );
          })}
        </div>
      </div>
      {edit && <FollowDialog init={edit} onClose={() => setEdit(null)} />}
    </SettingsPage>
  );
}
