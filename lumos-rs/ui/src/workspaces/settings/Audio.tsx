import { useEffect, useState } from "preact/hooks";
import type { AudioCfg, AudioIn, AudioRole, AudioRoute, AudioSource } from "../../types";
import { rt, send, toast } from "../../store";
import { NumberField, Select, StatusDot, TextField, Toggle } from "../../ui";
import { t } from "../../i18n";
import { SaveBar, SettingsPage } from "./Common";

/* Audio setup: which device and channels come in (and what each is for), which go out (matrix), the sound library,
   the audio-reactive values and the LTC state. Everything is applied by the Lumos engine, not by this window. */

const ROLES: AudioRole[] = ["off", "voice", "reactive", "ltc"];
const roleLabel = (r: AudioRole) => ({ off: t("Unused"), voice: t("Voice (Guion)"), reactive: t("Audio-reactive"), ltc: "LTC" })[r];
/** Rows of the output matrix: bus + bus channel. */
const ROWS: { source: AudioSource; ch: number; label: () => string }[] = [
  { source: "sounds", ch: 1, label: () => `${t("Sounds")} L` },
  { source: "sounds", ch: 2, label: () => `${t("Sounds")} R` },
  { source: "timeline", ch: 1, label: () => `${t("Timeline audio")} L` },
  { source: "timeline", ch: 2, label: () => `${t("Timeline audio")} R` },
  { source: "ltc", ch: 1, label: () => "LTC" },
];
const GAIN_STEPS = [0, -6, -12];
const MAX_SHOWN = 32;

const same = (a: AudioCfg, b: AudioCfg) => JSON.stringify(a) === JSON.stringify(b);
const fail = (e: unknown) => toast(String((e as Error)?.message ?? e), "err");

function Meter(props: { v: number; vertical?: boolean; title?: string }) {
  const p = Math.round(Math.max(0, Math.min(1, props.v)) * 100);
  return <span class={`au-meter ${props.vertical ? "v" : ""}`} title={props.title} aria-hidden="true"><i style={props.vertical ? { height: `${p}%` } : { width: `${p}%` }} /></span>;
}

export function AudioPage() {
  const a = rt.value?.audio;
  const [draft, setDraft] = useState<AudioCfg | null>(null);
  const cfg = a?.cfg;
  const dirty = !!(draft && cfg && !same(draft, cfg));
  useEffect(() => { if (cfg && !dirty) setDraft(structuredClone(cfg)); }, [JSON.stringify(cfg)]);
  if (!a || !draft || !cfg) return <SettingsPage title="Audio"><div class="dim">{t("Connecting…")}</div></SettingsPage>;
  if (!a.available) {
    return <SettingsPage title="Audio" sub={t("Audio inputs and outputs")}><div class="panel"><div class="panel-body dim">{t("This build of Lumos does not include audio. Install the desktop app to use sound, LTC and audio-reactive input.")}</div></div></SettingsPage>;
  }
  const running = a.input_running || a.output_running;
  const inDev = a.devices.find((d) => d.name === draft.input_device) ?? a.devices.find((d) => d.default_in);
  const outDev = a.devices.find((d) => d.name === draft.output_device) ?? a.devices.find((d) => d.default_out);
  const inN = Math.min(MAX_SHOWN, a.in_channels || inDev?.inputs || 2);
  const outN = Math.min(MAX_SHOWN, a.out_channels || outDev?.outputs || 2);
  const upd = (f: (c: AudioCfg) => void) => { const c = structuredClone(draft); f(c); setDraft(c); };
  const inCfg = (ch: number): AudioIn => draft.inputs.find((i) => i.channel === ch) ?? { channel: ch, label: "", role: "off", gain_db: 0 };
  const setIn = (ch: number, p: Partial<AudioIn>) => upd((c) => {
    const cur = c.inputs.find((i) => i.channel === ch);
    if (cur) Object.assign(cur, p); else c.inputs.push({ channel: ch, label: "", role: "off", gain_db: 0, ...p });
    // unused, unlabelled and untrimmed channels are not stored
    c.inputs = c.inputs.filter((i) => i.role !== "off" || i.label || i.gain_db).sort((x, y) => x.channel - y.channel);
  });
  const routeAt = (row: typeof ROWS[number], out: number) => draft.routes.find((r) => r.source === row.source && r.src_ch === row.ch && r.out_ch === out);
  // an empty matrix means the default (sounds/timeline stereo → outputs 1/2): show it as the cells it stands for
  const effective: AudioRoute[] = draft.routes.length ? draft.routes : [1, 2].flatMap((n) => (["sounds", "timeline"] as AudioSource[]).map((source) => ({ source, src_ch: n, out_ch: n, gain_db: 0 })));
  const cell = (row: typeof ROWS[number], out: number) => effective.find((r) => r.source === row.source && r.src_ch === row.ch && r.out_ch === out);
  const cycle = (row: typeof ROWS[number], out: number) => upd((c) => {
    if (!c.routes.length) c.routes = structuredClone(effective);
    const i = c.routes.findIndex((r) => r.source === row.source && r.src_ch === row.ch && r.out_ch === out);
    if (i < 0) { c.routes.push({ source: row.source, src_ch: row.ch, out_ch: out, gain_db: 0 }); return; }
    const next = GAIN_STEPS.indexOf(c.routes[i].gain_db) + 1;
    if (next === 0 || next >= GAIN_STEPS.length) c.routes.splice(i, 1); else c.routes[i].gain_db = GAIN_STEPS[next];
  });
  void routeAt;
  const save = () => void send({ type: "audio_configure", cfg: draft }).catch(fail);
  const toggleRun = () => void send({ type: "audio_run", on: !running }).catch(fail);
  const upload = async (f: File | undefined) => {
    if (!f) return;
    const fd = new FormData();
    fd.append("file", f);
    const r = await fetch("/api/media", { method: "POST", body: fd });
    if (r.ok) toast(t("Sound uploaded"), "ok"); else toast(await r.text(), "err");
  };
  const devOpts = (kind: "in" | "out") => [{ value: "", label: t("System default") }, ...a.devices.filter((d) => (kind === "in" ? d.inputs : d.outputs) > 0).map((d) => ({ value: d.name, label: `${d.name} (${kind === "in" ? d.inputs : d.outputs} ${kind === "in" ? t("in") : t("out")})` }))];

  return (
    <SettingsPage title="Audio" sub={t("Audio inputs and outputs: devices, which channels do what, and where sound goes.")} bar={
      <>
        <span class="row" style={{ gap: 8 }}><StatusDot state={running ? "ok" : "off"} title={running ? t("Audio open") : t("Audio closed")} />{running ? t("Audio open") : t("Audio closed")}</span>
        <button class={`btn tall ${running ? "" : "primary"}`} onClick={toggleRun}>{running ? t("Close audio") : t("Open audio")}</button>
      </>
    }>
      {a.error && <div class="panel"><div class="panel-body" style={{ color: "var(--red)" }}>⚠ {a.error}</div></div>}
      <SaveBar dirty={dirty} save={save} revert={() => setDraft(structuredClone(cfg))} />

      <div class="panel">
        <div class="panel-head">{t("Devices")}</div>
        <div class="panel-body grid2">
          <Select label={t("Input device")} value={draft.input_device} options={devOpts("in")} onChange={(v) => upd((c) => { c.input_device = v; })} />
          <Select label={t("Output device")} value={draft.output_device} options={devOpts("out")} onChange={(v) => upd((c) => { c.output_device = v; })} />
          <NumberField label={t("Master output (dB)")} value={draft.master_db} min={-60} max={12} step={1} float onChange={(v) => upd((c) => { c.master_db = v; })} />
          <div class="field"><label>{t("On start")}</label><Toggle checked={draft.autostart} onChange={(v) => upd((c) => { c.autostart = v; })} label={t("Open audio when Lumos starts")} /></div>
        </div>
        <div class="panel-body dim">{running ? `${a.in_channels ? `${t("Inputs")}: ${a.in_channels} ch` : ""} ${a.out_channels ? `· ${t("Outputs")}: ${a.out_channels} ch` : ""}` : t("Open audio to see the real channel count of the devices.")}</div>
      </div>

      <div class="panel">
        <div class="panel-head">{t("Inputs")} <span class="dim">— {t("what each channel is for")}</span></div>
        <div class="panel-body">
          <table class="tbl au-tbl">
            <thead><tr><th style={{ width: 54 }}>#</th><th style={{ width: 110 }}>{t("Level")}</th><th>{t("Name")}</th><th style={{ width: 190 }}>{t("Function")}</th><th style={{ width: 130 }}>{t("Trim (dB)")}</th></tr></thead>
            <tbody>
              {Array.from({ length: inN }, (_, k) => k + 1).map((ch) => {
                const c = inCfg(ch);
                return (
                  <tr key={ch}>
                    <td class="mono">{ch}</td>
                    <td><Meter v={a.in_levels[ch - 1] ?? 0} /></td>
                    <td><TextField value={c.label} placeholder={t("e.g. Actors mic")} onChange={(v) => setIn(ch, { label: v })} /></td>
                    <td><Select<AudioRole> value={c.role} options={ROLES.map((r) => ({ value: r, label: roleLabel(r) }))} onChange={(v) => setIn(ch, { role: v })} /></td>
                    <td><NumberField value={c.gain_db} min={-24} max={24} step={1} float onChange={(v) => setIn(ch, { gain_db: v })} /></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <div class="dim" style={{ marginTop: 8 }}>
            {t("Voice: feeds the Guion voice-follow (no channel marked = channel 1). Audio-reactive: level, bass/mid/treble and beat become sources in Controls. LTC: timecode signal for the Timecode source «LTC».")}
          </div>
        </div>
      </div>

      <div class="panel">
        <div class="panel-head">{t("Outputs")} <span class="dim">— {t("tap a cell: 0 dB → −6 → −12 → off")}</span></div>
        <div class="panel-body" style={{ overflowX: "auto" }}>
          <table class="au-matrix">
            <thead>
              <tr><th />{Array.from({ length: outN }, (_, k) => k + 1).map((o) => <th key={o}><Meter vertical v={a.out_levels[o - 1] ?? 0} /><div class="mono">{o}</div></th>)}</tr>
            </thead>
            <tbody>
              {ROWS.map((row) => (
                <tr key={`${row.source}${row.ch}`}>
                  <th class="au-row">{row.label()}</th>
                  {Array.from({ length: outN }, (_, k) => k + 1).map((o) => {
                    const r = cell(row, o);
                    return (
                      <td key={o}>
                        <button class={`au-cell ${r ? "on" : ""}`} aria-pressed={!!r} aria-label={`${row.label()} → ${o}`} onClick={() => cycle(row, o)}>{r ? (r.gain_db === 0 ? "0" : r.gain_db) : ""}</button>
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
          <div class="dim" style={{ marginTop: 8 }}>{t("Rows are what Lumos plays; columns are the physical outputs of the device. «Timeline audio» is the audio file of the timeline that is running. LTC sends the show timecode as an audio signal.")}</div>
        </div>
      </div>

      <div class="panel">
        <div class="panel-head">{t("Sounds")}</div>
        <div class="panel-body col" style={{ gap: 10 }}>
          <div class="row" style={{ gap: 10, flexWrap: "wrap" }}>
            <label class="btn tall">{t("Upload sound")}<input type="file" accept="audio/*,.wav,.mp3,.flac,.ogg,.m4a,.aac,.aif,.aiff" hidden onChange={(e) => { const i = e.target as HTMLInputElement; void upload(i.files?.[0]); i.value = ""; }} /></label>
            <button class="btn tall" disabled={a.playing.length === 0} onClick={() => void send({ type: "sound_stop", file: null }).catch(fail)}>■ {t("Stop all")}</button>
            <span class="dim">{t("From cues, macros and the Guion use the console: sound \"file.wav\" at 80 loop")}</span>
          </div>
          {a.sounds.length === 0 && <div class="dim">{t("No sounds yet. Upload a wav, mp3, flac or ogg file.")}</div>}
          {a.sounds.map((s) => {
            const p = a.playing.find((x) => x.file === s);
            return (
              <div class="vrow" key={s}>
                <button class="btn" aria-label={p ? t("Stop") : t("Play")} onClick={() => void send(p ? { type: "sound_stop", file: s } : { type: "sound_play", file: s, volume: 1, looped: false }).catch(fail)}>{p ? "■" : "▶"}</button>
                <span class="grow mono">{s}</span>
                {p && <span class="dim">{p.pos_s.toFixed(1)} / {p.len_s.toFixed(1)} s{p.looped ? " ↻" : ""}</span>}
              </div>
            );
          })}
        </div>
      </div>

      <div class="panel">
        <div class="panel-head">{t("Audio-reactive and LTC")}</div>
        <div class="panel-body grid2">
          {([[t("Level"), 0], [t("Bass"), 1], [t("Mid"), 2], [t("Treble"), 3]] as [string, number][]).map(([label, i]) => (
            <div class="row" key={label} style={{ gap: 10 }}><span style={{ width: 70 }}>{label}</span><Meter v={a.reactive[i]} /></div>
          ))}
          <div class="row" style={{ gap: 10 }}><span style={{ width: 70 }}>BPM</span><span class="mono">{a.bpm > 0 ? a.bpm : "—"}</span></div>
          <div class="row" style={{ gap: 10 }}><span style={{ width: 70 }}>LTC</span><StatusDot state={a.ltc.locked ? "ok" : "off"} /><span class="mono">{a.ltc.locked ? a.ltc.text : t("no signal")}</span></div>
        </div>
        <div class="panel-body dim">{t("To drive lights with the music: Settings → Controls → New → Source «Audio» (level, bass, mid, treble or beat) and pick the action.")}</div>
      </div>
    </SettingsPage>
  );
}
