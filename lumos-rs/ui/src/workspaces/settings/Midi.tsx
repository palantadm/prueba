import { EmptyState, PageHead, StatusDot, Toggle } from "../../ui";
import { initMidi, midiActivity, midiEnabled, midiError, midiOutEnabled, midiOutputs, midiPorts as webPorts, midiRelay, midiState, sendMidi, setMidiEnabled, setMidiOutEnabled, setMidiRelay } from "../../midi";
import { t } from "../../i18n";
import { midiPorts, rt, send } from "../../store";
import type { MidiPortInfo } from "../../types";

function NativeMidiPage({ inputs, outputs }: { inputs: MidiPortInfo[]; outputs: MidiPortInfo[] }) {
  const enable = (output: boolean, name: string, enabled: boolean) => send({ type: "midi_enable", output, name, enabled });
  const recent = rt.value?.inputs.recent ?? [];
  return (
    <div class="page">
      <PageHead title="MIDI" sub={t("MIDI controllers are read by the Lumos engine on this computer and work in any window or browser.")} />
      <div class="page-body col" style={{ gap: 16 }}>
        <div class="panel">
          <div class="panel-head">{t("Input ports")}</div>
          {inputs.length === 0 && <div class="panel-body dim">{t("No MIDI inputs detected. Plug in a controller — it appears here automatically.")}</div>}
          {inputs.map((p) => (
            <div class="vrow" key={p.name}>
              <StatusDot state={p.enabled ? "ok" : "off"} title={p.enabled ? t("Port enabled") : t("Port disabled")} />
              <span class="grow">{p.name}</span>
              <Toggle checked={p.enabled} onChange={(on) => enable(false, p.name, on)} label={t("Forward to engine")} />
            </div>
          ))}
        </div>
        <div class="panel">
          <div class="panel-head">{t("Output ports · controller feedback")}</div>
          {outputs.length === 0 && <div class="panel-body dim">{t("No MIDI outputs detected.")}</div>}
          {outputs.map((p) => (
            <div class="vrow" key={p.name}>
              <StatusDot state={p.enabled ? "ok" : "off"} title={p.enabled ? t("Port enabled") : t("Port disabled")} />
              <span class="grow">{p.name}</span>
              <Toggle checked={p.enabled} onChange={(on) => enable(true, p.name, on)} label={t("Send feedback")} />
            </div>
          ))}
        </div>
        <div class="faint" style={{ fontSize: "var(--fs-sm)" }}>{t("Enabled ports reconnect automatically when plugged in again.")}</div>
        <div class="panel">
          <div class="panel-head">{t("Activity")}</div>
          <div class="mono scroll" style={{ maxHeight: 280, fontSize: "var(--fs-sm)" }}>
            {recent.length === 0 ? <div class="panel-body faint">{t("Move a fader or press a pad on an enabled port…")}</div> :
              recent.map((a, i) => <div key={i} class="vrow" style={{ height: 26 }}><span>{a}</span></div>)}
          </div>
        </div>
      </div>
    </div>
  );
}

export function MidiPage() {
  const nat = midiPorts.value;
  if (nat?.native) return <NativeMidiPage inputs={nat.inputs} outputs={nat.outputs} />;
  const st = midiState.value;
  return (
    <div class="page">
      <PageHead title="MIDI" sub={t("Controllers are read by this browser (Web MIDI) and forwarded to the engine.")}>
        {st === "idle" && <button class="btn primary tall" onClick={() => initMidi()}>{t("Enable MIDI access")}</button>}
      </PageHead>
      <div class="page-body col" style={{ gap: 16 }}>
        {(st === "denied" || st === "unsupported") && <div class="panel panel-body" style={{ borderColor: "var(--stop)" }} role="alert">{midiError.value}</div>}
        {st === "idle" && <EmptyState icon="🎹" title={t("MIDI is not active")} hint={t("Click “Enable MIDI access”; the browser will ask for permission. Once granted, enabled ports reconnect automatically next time.")} />}
        {st === "ready" && (
          <div class="panel">
            <div class="panel-head">{t("Input ports")}</div>
            {webPorts.value.length === 0 && <div class="panel-body dim">{t("No MIDI inputs detected. Plug in a controller — it appears here automatically.")}</div>}
            {webPorts.value.map((p) => (
              <div class="vrow" key={p.id}>
                <StatusDot state={p.connected ? "ok" : "off"} title={p.connected ? t("connected") : t("disconnected")} />
                <span class="grow">{p.name} <span class="faint">{p.manufacturer}</span></span>
                <Toggle checked={midiEnabled.value.includes(p.name)} onChange={(on) => setMidiEnabled(p.name, on)} label={t("Forward to engine")} />
              </div>
            ))}
          </div>
        )}
        {st === "ready" && (
          <div class="panel">
            <div class="panel-head">{t("Output ports · controller feedback")} <span class="sp" />
              <Toggle checked={midiRelay.value} onChange={setMidiRelay} label={t("Relay engine feedback from this browser")} title={t("Turn off on all but one browser to avoid duplicate messages")} />
            </div>
            {midiOutputs.value.length === 0 && <div class="panel-body dim">{t("No MIDI outputs detected.")}</div>}
            {midiOutputs.value.map((p) => (
              <div class="vrow" key={p.id}>
                <StatusDot state={p.connected ? "ok" : "off"} title={p.connected ? t("connected") : t("disconnected")} />
                <span class="grow">{p.name} <span class="faint">{p.manufacturer}</span></span>
                <button class="btn sm" title={t("Sends note 60 on/off on channel 1")} onClick={() => { sendMidi(p.name, [0x90, 60, 127], true); setTimeout(() => sendMidi(p.name, [0x80, 60, 0], true), 250); }}>{t("Test")}</button>
                <Toggle checked={midiOutEnabled.value.includes(p.name)} onChange={(on) => setMidiOutEnabled(p.name, on)} label={t("Send feedback")} />
              </div>
            ))}
          </div>
        )}
        <div class="panel">
          <div class="panel-head">{t("Activity")} <span class="sp" /><button class="btn sm ghost" onClick={() => (midiActivity.value = [])}>{t("Clear")}</button></div>
          <div class="mono scroll" style={{ maxHeight: 280, fontSize: "var(--fs-sm)" }}>
            {midiActivity.value.length === 0 ? <div class="panel-body faint">{t("Move a fader or press a pad on an enabled port…")}</div> :
              midiActivity.value.map((a, i) => <div key={a.at + ":" + i} class="vrow" style={{ height: 26 }}><span class="faint" style={{ width: 150, overflow: "hidden", textOverflow: "ellipsis" }}>{a.port}</span><span>{a.text}</span></div>)}
          </div>
        </div>
      </div>
    </div>
  );
}
