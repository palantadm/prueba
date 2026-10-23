import "../../styles/auto.css";
import { useState } from "preact/hooks";
import { rt, send, show } from "../../store";
import { EmptyState, NumberField, Select, TextField, Toggle } from "../../ui";
import { useSettingsDraft } from "../../lib/draft";
import { SaveBar, SettingsPage } from "./Common";
import { t } from "../../i18n";

const CMD = { GO: 0x01, STOP: 0x02, RESUME: 0x03, LOAD: 0x05, ALL_OFF: 0x08, RESET: 0x0a } as const;

/** Build an MSC lighting SysEx: F0 7F <dev> 02 01 <cmd> [cue 00 list] F7 */
export function mscBytes(device: number, cmd: number, cue?: string, list?: string): number[] {
  const ascii = (s: string) => Array.from(s).map((c) => c.charCodeAt(0) & 0x7f);
  const data = cue ? [...ascii(cue), ...(list ? [0, ...ascii(list)] : [])] : [];
  return [0xf0, 0x7f, device & 0x7f, 0x02, 0x01, cmd, ...data, 0xf7];
}

export function MscPage() {
  const { draft, patch, dirty, save, revert } = useSettingsDraft();
  const [cue, setCue] = useState("1");
  const [list, setList] = useState("1");
  const [sent, setSent] = useState("");
  if (!draft) return <EmptyState title={t("Waiting for the engine…")} />;
  const msc = draft.msc ?? { enabled: false, device_id: 127, map: [] };
  const cuelists = show.value?.cuelists ?? [];
  const recent = (rt.value?.inputs.recent ?? []).filter((l) => /msc/i.test(l)).slice(0, 12);
  const test = (name: keyof typeof CMD, withCue: boolean) => {
    const bytes = mscBytes(msc.device_id, CMD[name], withCue ? cue : undefined, withCue ? list : undefined);
    setSent(`${name}${withCue ? ` cue ${cue} / list ${list}` : ""}`);
    void send({ type: "midi_in", port: "ui-test", bytes }, { quiet: true }).catch(() => {});
  };
  const setMap = (i: number, p: Partial<{ msc_list: string; cuelist: number }>) =>
    patch((s) => { s.msc.map[i] = { ...s.msc.map[i], ...p }; });
  return (
    <SettingsPage title={t("Show Control (MSC)")} sub={t("MIDI Show Control: trigger cuelists from a stage-management desk or software (SysEx over an enabled MIDI input).")} bar={<SaveBar {...{ dirty, save, revert }} />}>
      <div class="panel panel-body col" style={{ gap: 12 }}>
        <Toggle checked={msc.enabled} label={t("Enable MSC")} onChange={(v) => patch((s) => { s.msc ??= { enabled: false, device_id: 127, map: [] }; s.msc.enabled = v; })} />
        <div class="grid2">
          <NumberField label={t("Device ID")} hint={t("0–111 = this device, 127 = all-call (accept everything)")} value={msc.device_id} min={0} max={127}
            onChange={(v) => patch((s) => { s.msc ??= { enabled: false, device_id: 127, map: [] }; s.msc.device_id = v; })} />
        </div>
        <div class="dim">{t("MSC arrives through an enabled MIDI input port (Settings → MIDI) as SysEx. Supported: GO, STOP, RESUME, LOAD, ALL_OFF, RESET.")}</div>
      </div>

      <div class="panel">
        <div class="panel-head"><span>{t("Cue list mapping")}</span><span class="sp" />
          <button class="btn sm" style={{ minHeight: 36 }} onClick={() => patch((s) => { s.msc ??= { enabled: false, device_id: 127, map: [] }; s.msc.map.push({ msc_list: String(s.msc.map.length + 1), cuelist: cuelists[0]?.id ?? 1 }); })}>{t("+ Add mapping")}</button></div>
        <div class="panel-body col">
          <div class="dim">{t("An MSC cue list id (text) is routed to a Lumos cuelist. Messages without a list go to the first cuelist.")}</div>
          {msc.map.length === 0 && <div class="faint">{t("No mappings — everything goes to the first cuelist.")}</div>}
          {msc.map.map((m, i) => (
            <div key={i} class="row" style={{ gap: 8 }}>
              <TextField class="bare mono" label={t("MSC list")} value={m.msc_list} onChange={(v) => setMap(i, { msc_list: v })} placeholder="1" />
              <span aria-hidden="true">→</span>
              <Select<number> class="bare" label={t("Cuelist")} value={m.cuelist} onChange={(v) => setMap(i, { cuelist: v })}
                options={[...cuelists.map((c) => ({ value: c.id, label: `${c.id} · ${c.name}` })), ...(cuelists.some((c) => c.id === m.cuelist) ? [] : [{ value: m.cuelist, label: `${m.cuelist}` }])]} />
              <button class="btn au-btn danger" aria-label={t("Remove mapping")} onClick={() => patch((s) => { s.msc.map.splice(i, 1); })}>✕</button>
            </div>
          ))}
        </div>
      </div>

      <div class="panel">
        <div class="panel-head">{t("Test")}</div>
        <div class="panel-body col">
          <div class="dim">{t("Sends a simulated MSC message to the engine (works without a MIDI device). Save settings first so the device ID and mapping apply.")}</div>
          <div class="msc-test">
            <label class="row">{t("Cue")} <input class="input num" style={{ width: 90, minHeight: 44 }} value={cue} onInput={(e) => setCue((e.target as HTMLInputElement).value)} aria-label={t("Cue number")} /></label>
            <label class="row">{t("List")} <input class="input num" style={{ width: 90, minHeight: 44 }} value={list} onInput={(e) => setList((e.target as HTMLInputElement).value)} aria-label={t("MSC list")} /></label>
            <button class="btn au-btn go" onClick={() => test("GO", true)}>GO {t("cue")}</button>
            <button class="btn au-btn go" onClick={() => test("GO", false)}>GO</button>
            <button class="btn au-btn" onClick={() => test("STOP", false)}>STOP</button>
            <button class="btn au-btn" onClick={() => test("RESUME", false)}>RESUME</button>
            <button class="btn au-btn" onClick={() => test("LOAD", true)}>LOAD</button>
            <button class="btn au-btn danger" onClick={() => test("ALL_OFF", false)}>ALL OFF</button>
            <button class="btn au-btn danger" onClick={() => test("RESET", false)}>RESET</button>
          </div>
          {sent && <div class="chip ok" style={{ alignSelf: "flex-start" }}>{t("Sent: {m}", { m: sent })}</div>}
        </div>
      </div>

      <div class="panel">
        <div class="panel-head">{t("Last MSC messages received")}</div>
        <div class="panel-body msc-recent" role="log" aria-live="polite">
          {recent.length === 0 && <span class="faint">{t("Nothing received yet.")}</span>}
          {recent.map((l, i) => <div key={i}>{l}</div>)}
        </div>
      </div>
    </SettingsPage>
  );
}
