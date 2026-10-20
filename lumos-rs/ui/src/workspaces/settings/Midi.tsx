import { EmptyState, PageHead, StatusDot, Toggle } from "../../ui";
import { initMidi, midiActivity, midiEnabled, midiError, midiPorts, midiState, setMidiEnabled } from "../../midi";

export function MidiPage() {
  const st = midiState.value;
  return (
    <div class="page">
      <PageHead title="MIDI" sub="Controllers are read by this browser (Web MIDI) and forwarded to the engine.">
        {st === "idle" && <button class="btn primary tall" onClick={() => initMidi()}>Enable MIDI access</button>}
      </PageHead>
      <div class="page-body col" style={{ gap: 16 }}>
        {(st === "denied" || st === "unsupported") && <div class="panel panel-body" style={{ borderColor: "var(--stop)" }} role="alert">{midiError.value}</div>}
        {st === "idle" && <EmptyState icon="🎹" title="MIDI is not active" hint="Click “Enable MIDI access”; the browser will ask for permission. Once granted, enabled ports reconnect automatically next time." />}
        {st === "ready" && (
          <div class="panel">
            <div class="panel-head">Input ports</div>
            {midiPorts.value.length === 0 && <div class="panel-body dim">No MIDI inputs detected. Plug in a controller — it appears here automatically.</div>}
            {midiPorts.value.map((p) => (
              <div class="vrow" key={p.id}>
                <StatusDot state={p.connected ? "ok" : "off"} title={p.connected ? "connected" : "disconnected"} />
                <span class="grow">{p.name} <span class="faint">{p.manufacturer}</span></span>
                <Toggle checked={midiEnabled.value.includes(p.name)} onChange={(on) => setMidiEnabled(p.name, on)} label="Forward to engine" />
              </div>
            ))}
          </div>
        )}
        <div class="panel">
          <div class="panel-head">Activity <span class="sp" /><button class="btn sm ghost" onClick={() => (midiActivity.value = [])}>Clear</button></div>
          <div class="mono scroll" style={{ maxHeight: 280, fontSize: "var(--fs-sm)" }}>
            {midiActivity.value.length === 0 ? <div class="panel-body faint">Move a fader or press a pad on an enabled port…</div> :
              midiActivity.value.map((a, i) => <div key={a.at + ":" + i} class="vrow" style={{ height: 26 }}><span class="faint" style={{ width: 150, overflow: "hidden", textOverflow: "ellipsis" }}>{a.port}</span><span>{a.text}</span></div>)}
          </div>
        </div>
      </div>
    </div>
  );
}
