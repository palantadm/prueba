import { rt, show } from "../../store";
import { EmptyState, NumberField, StatusDot, TextField, Toggle } from "../../ui";
import { useSettingsDraft } from "../../lib/draft";
import { SaveBar, SettingsPage } from "./Common";

export function OscPage() {
  const { draft, patch, dirty, save, revert } = useSettingsDraft();
  const inp = rt.value?.inputs;
  if (!draft) return <EmptyState title="Waiting for the engine…" />;
  const oscBindings = (show.value?.bindings ?? []).filter((b) => b.source.type === "osc");
  return (
    <SettingsPage title="OSC" sub="Receive control messages and send feedback to surfaces such as TouchOSC." bar={<SaveBar {...{ dirty, save, revert }} />}>
      <div class="panel">
        <div class="panel-head">Input <span class="sp" />
          <StatusDot state={inp?.osc_listening ? "ok" : inp?.osc_error ? "err" : "off"} />
          <span>{inp?.osc_listening ? `listening on UDP ${inp.osc_port}` : "not listening"}</span></div>
        <div class="panel-body col" style={{ gap: 12 }}>
          <Toggle checked={draft.osc_in.enabled} onChange={(enabled) => patch((s) => (s.osc_in.enabled = enabled))} label="Enable OSC input" />
          <NumberField label="UDP port" width={120} value={draft.osc_in.port} min={1} max={65535} onChange={(port) => patch((s) => (s.osc_in.port = port))} />
          {inp?.osc_error && <div role="alert" style={{ color: "var(--stop)" }}>{inp.osc_error}</div>}
        </div>
      </div>
      <div class="panel">
        <div class="panel-head">Feedback output</div>
        <div class="panel-body col" style={{ gap: 12 }}>
          <Toggle checked={draft.osc_feedback.enabled} onChange={(enabled) => patch((s) => (s.osc_feedback.enabled = enabled))} label="Send executor / master state back to a surface" />
          <div class="grid2">
            <TextField label="Host" mono value={draft.osc_feedback.host} onChange={(host) => patch((s) => (s.osc_feedback.host = host))} />
            <NumberField label="Port" value={draft.osc_feedback.port} min={1} max={65535} onChange={(port) => patch((s) => (s.osc_feedback.port = port))} />
          </div>
        </div>
      </div>
      <div class="panel">
        <div class="panel-head">Addresses you have mapped</div>
        {oscBindings.length === 0 ? <div class="panel-body dim">None yet. Create OSC bindings under <b>Controls</b> (any address you like) — they appear here.</div> :
          <table class="tbl"><tbody>{oscBindings.map((b) => <tr key={b.id}><td class="mono">{b.source.type === "osc" ? b.source.address : ""}</td><td>{b.label}</td><td class="dim">{b.enabled ? "" : "disabled"}</td></tr>)}</tbody></table>}
      </div>
      <div class="panel">
        <div class="panel-head">Recent input</div>
        <div class="mono scroll" style={{ maxHeight: 220, fontSize: "var(--fs-sm)" }}>
          {(inp?.recent ?? []).length === 0 ? <div class="panel-body faint">No input received yet.</div> : inp!.recent.map((r, i) => <div class="vrow" style={{ height: 26 }} key={i}>{r}</div>)}
        </div>
      </div>
    </SettingsPage>
  );
}

export function ArtNetInPage() {
  const { draft, patch, dirty, save, revert } = useSettingsDraft();
  const inp = rt.value?.inputs;
  if (!draft) return <EmptyState title="Waiting for the engine…" />;
  const a = draft.artnet_in;
  return (
    <SettingsPage title="Art-Net input" sub="Merge external Art-Net DMX (HTP) into Lumos universes." bar={<SaveBar {...{ dirty, save, revert }} />}>
      <div class="panel">
        <div class="panel-head">Receiver <span class="sp" /><StatusDot state={inp?.artnet_listening ? "ok" : inp?.artnet_error ? "err" : "off"} /><span>{inp?.artnet_listening ? "listening on UDP 6454" : "not listening"}</span></div>
        <div class="panel-body col" style={{ gap: 12 }}>
          <Toggle checked={a.enabled} onChange={(v) => patch((s) => (s.artnet_in.enabled = v))} label="Enable Art-Net input" />
          {inp?.artnet_error && <div role="alert" style={{ color: "var(--stop)" }}>{inp.artnet_error}</div>}
        </div>
      </div>
      <div class="panel">
        <div class="panel-head">Universe map <span class="sp" /><button class="btn sm" onClick={() => patch((s) => s.artnet_in.map.push({ net: 0, subnet: 0, universe: 0, target: show.value?.universes[0]?.id ?? 1 }))}>+ Add</button></div>
        {a.map.length === 0 ? <div class="panel-body dim">No mappings: incoming Art-Net is ignored.</div> :
          <table class="tbl"><thead><tr><th>Net</th><th>Sub-net</th><th>Universe</th><th>→ Lumos universe</th><th /></tr></thead><tbody>
            {a.map.map((m, i) => (
              <tr key={i}>
                <td><NumberField class="bare" width={80} value={m.net} min={0} max={127} onChange={(v) => patch((s) => (s.artnet_in.map[i].net = v))} /></td>
                <td><NumberField class="bare" width={80} value={m.subnet} min={0} max={15} onChange={(v) => patch((s) => (s.artnet_in.map[i].subnet = v))} /></td>
                <td><NumberField class="bare" width={80} value={m.universe} min={0} max={15} onChange={(v) => patch((s) => (s.artnet_in.map[i].universe = v))} /></td>
                <td><NumberField class="bare" width={100} value={m.target} min={1} onChange={(v) => patch((s) => (s.artnet_in.map[i].target = v))} /></td>
                <td><button class="btn sm danger" aria-label="Remove mapping" onClick={() => patch((s) => s.artnet_in.map.splice(i, 1))}>✕</button></td>
              </tr>))}
          </tbody></table>}
      </div>
    </SettingsPage>
  );
}
