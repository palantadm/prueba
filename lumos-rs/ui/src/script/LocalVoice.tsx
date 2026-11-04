import { useState } from "preact/hooks";
import { rt, send, toast } from "../store";
import { t } from "../i18n";
import type { SpeechCfg, SpeechRt } from "../types";

/* Offline voice-follow (Whisper, runs inside Lumos): mic on/off, language, model download, microphone and the
   "also fire the cues" switch. The engine does the listening, so it keeps following with this panel closed. */

const LANGS: [string, string][] = [["es", "Castellano"], ["gl", "Galego"], ["ca", "Català"], ["eu", "Euskara"], ["en", "English"], ["pt", "Português"], ["fr", "Français"]];

export function LocalVoicePanel(props: { sp: SpeechRt; scriptId: number }) {
  const { sp } = props;
  const [advanced, setAdvanced] = useState(false);
  const cfg = sp.cfg;
  const patch = (p: Partial<SpeechCfg>) => void send({ type: "speech_configure", cfg: { ...cfg, ...p, script: props.scriptId } }).catch((e) => toast(String(e.message ?? e), "err"));
  const model = sp.models.find((m) => m.id === cfg.model);
  const dl = sp.download;
  const custom = cfg.custom_model.trim().length > 0;
  const toggle = () => void send({ type: "speech_listen", on: !sp.listening }).catch((e) => toast(String(e.message ?? e), "err"));
  return (
    <div class="sc-voice-bar sc-lv" role="group" aria-label={t("Local voice")}>
      <button class={`btn ${sp.listening ? "danger" : "primary"}`} aria-pressed={sp.listening} disabled={!sp.model_ready || !!dl} onClick={toggle}>
        {sp.listening ? `■ ${t("Stop listening")}` : `🎙 ${t("Listen")}`}
      </button>
      <span class="sc-lv-meter" aria-hidden="true" title={t("Microphone level")}><i style={{ width: `${Math.round(sp.level * 100)}%` }} /></span>
      <label class="sc-lv-field">{t("Language")}
        <select class="input" value={cfg.lang} onChange={(e) => patch({ lang: (e.target as HTMLSelectElement).value })}>
          {LANGS.map(([c, n]) => <option value={c}>{n}</option>)}
        </select>
      </label>
      {!custom && (
        <label class="sc-lv-field">{t("Model")}
          <select class="input" value={cfg.model} onChange={(e) => patch({ model: (e.target as HTMLSelectElement).value })}>
            {sp.models.map((m) => <option value={m.id}>{m.installed ? "✓ " : ""}{m.label}</option>)}
          </select>
        </label>
      )}
      {!custom && model && !model.installed && !dl && (
        <button class="btn primary" onClick={() => void send({ type: "speech_model_get", id: model.id }).catch((e) => toast(String(e.message ?? e), "err"))}>⬇ {t("Download model")} ({model.size_mb} MB)</button>
      )}
      {dl && (
        <span class="sc-lv-dl" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(dl[1] * 100)}>
          {t("Downloading")} {Math.round(dl[1] * 100)} %<span class="sc-lv-bar"><i style={{ width: `${dl[1] * 100}%` }} /></span>
        </span>
      )}
      <button class="switch-row" role="switch" aria-checked={cfg.fire_cues} onClick={() => patch({ fire_cues: !cfg.fire_cues })} title={t("Cues only fire on complete phrases")}>
        <span class={`switch ${cfg.fire_cues ? "on" : ""}`} />{t("Also fire the cues")}
      </button>
      <button class="btn" aria-expanded={advanced} onClick={() => setAdvanced(!advanced)}>{t("More")}</button>
      {advanced && (
        <div class="sc-lv-adv">
          <label class="sc-lv-field grow">{t("Custom model file (.bin, ggml)")}
            <input class="input" type="text" value={cfg.custom_model} placeholder="/ruta/ggml-gallego.bin" onChange={(e) => patch({ custom_model: (e.target as HTMLInputElement).value })} />
          </label>
          <span class="dim">{t("Offline. The model is downloaded once and stays on this computer. The microphone and its channel are chosen in Settings → Audio.")}</span>
        </div>
      )}
      <span class="grow dim sc-lv-heard">
        {sp.error ? `⚠ ${sp.error}` : sp.listening ? (sp.heard ? `“${sp.heard}”` : t("Say the next line…")) : !sp.model_ready ? t("Download a model to start") : t("Ready")}
      </span>
    </div>
  );
}

export const speechOf = (): SpeechRt | undefined => rt.value?.speech;
