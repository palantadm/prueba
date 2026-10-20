import { useEffect, useState } from "preact/hooks";
import type { TcSource } from "../../types";
import { fire, rt, show } from "../../store";
import { EmptyState, NumberField, Segmented, Select, TextField } from "../../ui";
import { useSettingsDraft } from "../../lib/draft";
import { fetchNet } from "../../lib/api";
import { SaveBar, SettingsPage } from "./Common";
import { lang, setLang, t, type Lang } from "../../i18n";

export function TimecodePage() {
  const { draft, patch, dirty, save, revert } = useSettingsDraft();
  const tc = rt.value?.timecode;
  if (!draft) return <EmptyState title={t("Waiting for the engine…")} />;
  const tcg = draft.timecode;
  return (
    <SettingsPage title={t("Timecode")} sub={t("Fire cues that carry a timecode from MTC or the internal clock.")} bar={<SaveBar {...{ dirty, save, revert }} />}>
      <div class="panel" style={{ textAlign: "center", padding: 24 }}>
        <div class="mono" style={{ fontSize: 64, fontWeight: 700, letterSpacing: ".04em", color: tc?.running ? "var(--live)" : "var(--text-dim)" }} aria-live="off">{tc?.text ?? "00:00:00:00"}</div>
        <div class="dim">{tc?.running ? t("running") : t("stopped")} · {tc?.frames ?? 0} {t("frames")}</div>
        {tcg.source === "internal" && (
          <div class="row" style={{ justifyContent: "center", marginTop: 12 }}>
            <button class="btn tall go" onClick={() => fire({ type: "timecode_ctl", running: true, frames: null })}>{t("▶ Start")}</button>
            <button class="btn tall" onClick={() => fire({ type: "timecode_ctl", running: false, frames: null })}>{t("■ Stop")}</button>
            <button class="btn tall" onClick={() => fire({ type: "timecode_ctl", running: false, frames: 0 })}>{t("⏮ Reset")}</button>
          </div>
        )}
      </div>
      <div class="panel panel-body col" style={{ gap: 12 }}>
        <div class="field"><label>{t("Source")}</label>
          <Segmented<TcSource> value={tcg.source} onChange={(v) => patch((s) => (s.timecode.source = v))} options={[{ value: "off", label: t("Off") }, { value: "mtc", label: "MIDI (MTC)" }, { value: "internal", label: t("Internal clock") }]} /></div>
        {tcg.source === "mtc" && <div class="dim">{t("MTC arrives through an enabled MIDI input port (Settings → MIDI).")}</div>}
        <div class="grid2">
          <Select<number> label={t("Frame rate")} value={tcg.fps} onChange={(fps) => patch((s) => (s.timecode.fps = fps))}
            options={[24, 25, 29.97, 30].map((v) => ({ value: v, label: t("{v} fps", { v }) }))} />
          <NumberField label={t("Offset (frames)")} value={tcg.offset_frames} min={-864000} max={864000} onChange={(v) => patch((s) => (s.timecode.offset_frames = v))} />
        </div>
      </div>
    </SettingsPage>
  );
}

export function NetworkPage() {
  const { draft, patch, dirty, save, revert } = useSettingsDraft();
  const [ips, setIps] = useState<string[] | null>(null);
  const [err, setErr] = useState("");
  useEffect(() => { fetchNet().then(setIps).catch((e) => setErr(String(e.message ?? e))); }, []);
  const port = location.port || (location.protocol === "https:" ? "443" : "80");
  const urls = [location.origin, ...(ips ?? []).map((ip) => `${location.protocol}//${ip}:${port}`)].filter((u, i, a) => a.indexOf(u) === i);
  return (
    <SettingsPage title={t("Network")} sub={t("Open this console from a tablet or phone on the same network.")} bar={draft ? <SaveBar {...{ dirty, save, revert }} /> : undefined}>
      <div class="panel">
        <div class="panel-head">{t("Console URLs")}</div>
        <div class="panel-body col">
          {urls.map((u) => <a key={u} class="mono" style={{ color: "var(--accent)", fontSize: "var(--fs-lg)" }} href={u} target="_blank" rel="noreferrer">{u}</a>)}
          {err && <div class="dim">{t("Could not list network interfaces ({err}).", { err })}</div>}
          {ips && ips.length === 0 && <div class="dim">{t("No LAN address found — the server may be bound to localhost only (start it with --bind 0.0.0.0).")}</div>}
        </div>
      </div>
      {draft && <div class="panel panel-body"><TextField label={t("Node name")} hint={t("Shown to Art-Net / sACN receivers.")} value={draft.node_name} onChange={(v) => patch((s) => (s.node_name = v))} /></div>}
    </SettingsPage>
  );
}

export function GeneralPage() {
  const { draft, patch, dirty, save, revert } = useSettingsDraft();
  if (!draft) return <EmptyState title={t("Waiting for the engine…")} />;
  return (
    <SettingsPage title={t("General")} bar={<SaveBar {...{ dirty, save, revert }} />}>
      <div class="panel panel-body col" style={{ gap: 12 }}>
        <div class="field"><label>{t("Language")}</label>
          <Segmented<Lang> label={t("Language")} value={lang.value} onChange={setLang} options={[{ value: "es", label: "Español" }, { value: "en", label: "English" }]} />
          <span class="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("Applies immediately on this device.")}</span></div>
        <TextField label={t("Node name")} value={draft.node_name} onChange={(v) => patch((s) => (s.node_name = v))} />
        <NumberField label={t("Engine rate (Hz)")} width={120} hint={t("44 Hz is the DMX512 maximum for full frames. Lower it for slow networks.")} value={draft.tick_hz} min={10} max={100} onChange={(v) => patch((s) => (s.tick_hz = v))} />
        <div class="dim">{t("Show: {name} · measured engine rate {fps} fps", { name: show.value?.name ?? "—", fps: rt.value?.fps.toFixed(1) ?? "–" })}</div>
      </div>
    </SettingsPage>
  );
}
