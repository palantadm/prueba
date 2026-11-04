import "../../styles/ai.css";
import { useEffect, useState } from "preact/hooks";
import { Modal, Segmented, Select, TextField, confirmDialog } from "../../ui";
import { SwitchRow } from "../cues/SwitchRow";
import { SettingsPage } from "./Common";
import { t, tp } from "../../i18n";
import { toast } from "../../store";
import { tErr } from "../../i18n/errors";
import { aiConfig, aiTests, configure, listModels, testProvider } from "../../ai/state";
import type { AiMode, AiModelInfo, AiPreset, AiProviderKind, AiProviderOut } from "../../types";

const isLocal = (url: string) => /^https?:\/\/(localhost|127\.|\[::1\]|0\.0\.0\.0)/i.test(url);
const kindLabel = (k: AiProviderKind) => (k === "anthropic" ? "Anthropic API" : t("OpenAI-compatible API"));
const LOCAL_TAG = () => t("Local · your data stays on this computer");

const MODE_HELP: Record<AiMode, () => string> = {
  ask: () => t("It reads freely but asks before it changes anything, in the show or live."),
  smart: () => t("It reads and edits the show on its own, and asks before anything that changes the live output."),
  auto: () => t("It does everything without asking, live output included. You can still undo its changes."),
};

function TestChip({ id }: { id: string }) {
  const r = aiTests.value[id];
  if (!r) return null;
  if (r === "running") return <span class="chip"><span class="ai-spin" aria-hidden="true" />{t("Testing…")}</span>;
  return <span class={`chip ${r.ok ? "ok" : "err"}`} role="status" style={{ height: "auto", minHeight: 30, whiteSpace: "normal" }}>
    {r.ok ? `✓ ${t("Connected")} · ${r.ms} ms` : `✕ ${tErr(r.message)}`}
  </span>;
}

// ── provider sheet ──────────────────────────────────────

type Draft = { id: string; name: string; kind: AiProviderKind; base_url: string; models: AiModelInfo[]; has_key: boolean; needs_key: boolean; doc_url: string; isNew: boolean };

function uniqueId(base: string, taken: string[]) {
  let id = base;
  for (let n = 2; taken.includes(id); n++) id = `${base}-${n}`;
  return id;
}

function fromProvider(p: AiProviderOut): Draft {
  const preset = aiConfig.value?.presets.find((x) => x.base_url === p.base_url || x.id === p.id);
  return { id: p.id, name: p.name, kind: p.kind, base_url: p.base_url, models: p.models, has_key: p.has_key, needs_key: p.needs_key, doc_url: preset?.doc_url ?? "", isNew: false };
}
function fromPreset(pr: AiPreset, taken: string[]): Draft {
  const custom = pr.id === "custom" || !pr.base_url;
  return { id: uniqueId(pr.id, taken), name: pr.name, kind: pr.kind, base_url: pr.base_url, models: pr.models, has_key: false, needs_key: pr.needs_key, doc_url: custom ? "" : pr.doc_url, isNew: true };
}

function ProviderSheet({ init, onClose }: { init: Draft; onClose: () => void }) {
  const cfg = aiConfig.value!;
  const [d, setD] = useState(init);
  const [key, setKey] = useState("");
  const [clearKey, setClearKey] = useState(false);
  const [edited, setEdited] = useState<AiModelInfo[] | null>(null);
  const [model, setModel] = useState(() => (cfg.active_provider === init.id && cfg.model ? cfg.model : init.models[0]?.id ?? ""));
  const [saving, setSaving] = useState(false);

  const live = cfg.providers.find((p) => p.id === d.id);
  const models = edited ?? live?.models ?? d.models;
  const exists = !!live;
  const hasKey = !clearKey && (key.length > 0 || (live?.has_key ?? false));
  const local = isLocal(d.base_url);

  // keep the model choice valid when the list changes (e.g. after a refresh)
  useEffect(() => { if (!models.some((m) => m.id === model)) setModel(models[0]?.id ?? ""); }, [models.length]);

  const body = (extra: Partial<Parameters<typeof configure>[0]> = {}) => ({
    provider: { id: d.id, name: d.name.trim() || d.id, kind: d.kind, base_url: d.base_url.trim(), api_key: clearKey ? "" : key ? key : null, models },
    ...extra,
  });
  const save = async (use = false) => {
    setSaving(true);
    try {
      await configure(body(use || !cfg.active_provider ? { active_provider: d.id, model: model || null, enabled: cfg.enabled ? null : true } : cfg.active_provider === d.id && model ? { model } : {}));
      setKey(""); setClearKey(false); setEdited(null);
      return true;
    } catch (e) {
      toast(tErr((e as Error).message), "err");
      return false;
    } finally { setSaving(false); }
  };
  const test = async () => { if (await save()) await testProvider(d.id); };
  const refresh = async () => {
    if (!(await save())) return;
    try { await listModels(d.id); toast(t("Model list updated"), "ok", 1800); } catch (e) { toast(tErr((e as Error).message), "err"); }
  };
  const remove = async () => {
    if (!(await confirmDialog(t("Remove provider"), t("Remove “{n}” and its saved key?", { n: d.name }), { danger: true, ok: t("Remove") }))) return;
    try { await configure({ remove_provider: d.id }); onClose(); } catch (e) { toast(tErr((e as Error).message), "err"); }
  };
  const flag = (id: string, f: "tools" | "vision" | "reasoning", v: boolean) => setEdited(models.map((m) => (m.id === id ? { ...m, [f]: v } : m)));

  return (
    <Modal title={exists ? t("Provider: {n}", { n: d.name }) : t("Add provider")} onClose={onClose} width={640}
      footer={<>
        <button class="btn tall" onClick={onClose}>{t("Close")}</button>
        <button class="btn tall" disabled={saving || !d.base_url.trim()} onClick={() => void save(false).then((ok) => ok && onClose())}>{t("Save")}</button>
        <button class="btn tall primary" disabled={saving || !d.base_url.trim() || !model} onClick={() => void save(true).then((ok) => ok && onClose())}>{t("Save and use")}</button>
      </>}>
      {local && <span class="chip ok" style={{ alignSelf: "flex-start" }}>{LOCAL_TAG()}</span>}
      <TextField label={t("Name")} value={d.name} onChange={(v) => setD({ ...d, name: v })} />
      <TextField label={t("Base URL")} mono value={d.base_url} onChange={(v) => setD({ ...d, base_url: v })} hint={kindLabel(d.kind)} />

      <div class="field">
        <label for="ai-key">{t("API key")}</label>
        <div class="row wrap">
          <input id="ai-key" class="input grow" type="password" autocomplete="off" spellcheck={false} value={key} aria-describedby="ai-key-hint"
            placeholder={live?.has_key && !clearKey ? "••••••••••••" : d.needs_key ? t("Paste your key") : t("Not needed")}
            onInput={(e) => { setKey((e.target as HTMLInputElement).value); setClearKey(false); }} />
          {hasKey && <span class="chip ok">{t("Saved ✓")}</span>}
          {live?.has_key && !clearKey && <button class="btn tall danger" onClick={() => { setClearKey(true); setKey(""); }}>{t("Delete key")}</button>}
        </div>
        <span id="ai-key-hint" class="faint" style={{ fontSize: "var(--fs-xs)" }}>
          {clearKey ? t("The key will be removed when you save.") : d.needs_key ? t("The key is stored on this computer and never shown again.") : t("This provider does not need a key.")}
          {d.doc_url && <> <a href={d.doc_url} target="_blank" rel="noopener noreferrer" style={{ color: "var(--cyan)" }}>{d.needs_key ? t("Get a key") : t("Install guide")}</a></>}
        </span>
      </div>

      <div class="row wrap">
        <button class="btn tall" disabled={saving || !d.base_url.trim()} onClick={() => void test()}>{t("Test connection")}</button>
        <button class="btn tall" disabled={saving || !d.base_url.trim()} onClick={() => void refresh()}>{t("Update models")}</button>
        <TestChip id={d.id} />
      </div>

      <div class="field">
        <label>{t("Models")} · {tp(models.length, "{n} model", "{n} models")}</label>
        {models.length === 0 && <div class="dim">{t("No models yet. Press “Update models” or add them after saving.")}</div>}
        <div>
          {models.map((m) => (
            <div key={m.id} class="ai-model">
              <div class="ai-model-name"><b>{m.label || m.id}</b>{m.label && m.label !== m.id && <span class="faint mono" style={{ fontSize: "var(--fs-xs)" }}> {m.id}</span>}{m.note && <div class="faint" style={{ fontSize: "var(--fs-xs)" }}>{m.note}</div>}</div>
              <label class="check"><input type="checkbox" checked={m.tools} onChange={(e) => flag(m.id, "tools", (e.target as HTMLInputElement).checked)} />{t("Tools")}</label>
              <label class="check"><input type="checkbox" checked={m.vision} onChange={(e) => flag(m.id, "vision", (e.target as HTMLInputElement).checked)} />{t("Vision")}</label>
              <label class="check"><input type="checkbox" checked={m.reasoning} onChange={(e) => flag(m.id, "reasoning", (e.target as HTMLInputElement).checked)} />{t("Reasoning")}</label>
            </div>
          ))}
        </div>
        <span class="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("Without “Tools” the model can only chat; it cannot operate Lumos.")}</span>
      </div>

      {models.length > 0 && (
        <Select label={t("Model to use")} value={model} onChange={setModel} options={models.map((m) => ({ value: m.id, label: m.label || m.id }))} />
      )}
      {exists && <button class="btn tall danger" style={{ alignSelf: "flex-start" }} onClick={() => void remove()}>{t("Remove provider")}</button>}
    </Modal>
  );
}

// ── page ────────────────────────────────────────────────

export function AiPage() {
  const cfg = aiConfig.value;
  const [sheet, setSheet] = useState<Draft | null>(null);
  const [picker, setPicker] = useState(false);

  if (!cfg) return <SettingsPage title={t("Assistant")}><div class="dim">{t("Loading…")}</div></SettingsPage>;
  const taken = cfg.providers.map((p) => p.id);
  const run = (p: Promise<void>) => p.catch((e) => toast(tErr((e as Error).message), "err"));

  return (
    <SettingsPage title={t("Assistant")} sub={t("An AI that answers questions and operates Lumos for you. Works with any provider.")}>
      <div class="panel"><div class="panel-body col">
        <SwitchRow on={cfg.enabled} label={t("Enable the assistant")} onChange={(v) => void run(configure({ enabled: v }))} />
        <div class="field">
          <label>{t("How much should it ask?")}</label>
          <Segmented value={cfg.mode} label={t("Safety mode")} onChange={(m) => void run(configure({ mode: m }))}
            options={[{ value: "ask", label: t("Always ask") }, { value: "smart", label: t("Smart") }, { value: "auto", label: t("Automatic") }]} />
          <span class="dim">{MODE_HELP[cfg.mode]()}</span>
        </div>
        <div class="field">
          <label>{t("Reply style")}</label>
          <Segmented value={cfg.style} label={t("Reply style")} onChange={(v) => void run(configure({ style: v }))}
            options={[{ value: "normal", label: t("Normal") }, { value: "concise", label: t("Concise") }, { value: "caveman", label: t("Caveman (fewest tokens)") }]} />
          <span class="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("Shorter replies use fewer tokens (cheaper and faster). Caveman drops every filler word but keeps numbers, names and commands intact.")}</span>
        </div>
        <div class="field">
          <label>{t("Reasoning effort")}</label>
          <Segmented value={cfg.effort} label={t("Reasoning effort")} onChange={(v) => void run(configure({ effort: v }))}
            options={[{ value: "low", label: t("Low") }, { value: "medium", label: t("Medium") }, { value: "high", label: t("High") }]} />
          <span class="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("Higher effort thinks longer: better answers, slower and more expensive. Only affects models with reasoning.")}</span>
        </div>
      </div></div>

      <div class="panel">
        <div class="panel-head">{t("Providers")}</div>
        <div class="panel-body col">
          {cfg.providers.length === 0 && <div class="dim">{t("No provider yet. Add one to start.")}</div>}
          {cfg.providers.map((p) => {
            const act = p.id === cfg.active_provider;
            return (
              <div key={p.id} class={`ai-prov ${act ? "active" : ""}`}>
                <div class="ai-prov-main">
                  <b>{p.name}</b>
                  <span class="ai-prov-url" title={p.base_url}>{kindLabel(p.kind)} · {p.base_url}</span>
                  <span class="row wrap" style={{ gap: 6 }}>
                    {act && <span class="chip cyan-chip" style={{ background: "var(--cyan-fill)", borderColor: "var(--cyan)" }}>{t("Active")}{cfg.model ? ` · ${cfg.model}` : ""}</span>}
                    {p.needs_key ? <span class={`chip ${p.has_key ? "ok" : "warn"}`}>{p.has_key ? t("Key saved ✓") : t("Key missing")}</span> : <span class="chip">{t("No key needed")}</span>}
                    <span class="chip">{tp(p.models.length, "{n} model", "{n} models")}</span>
                    {isLocal(p.base_url) && <span class="chip ok">{LOCAL_TAG()}</span>}
                  </span>
                </div>
                {!act && <button class="btn tall" onClick={() => void run(configure({ active_provider: p.id, model: p.models.find((m) => m.tools)?.id ?? p.models[0]?.id ?? null }))}>{t("Use")}</button>}
                <button class="btn tall" onClick={() => setSheet(fromProvider(p))}>{t("Edit")}</button>
              </div>
            );
          })}
          <button class="btn tall primary" style={{ alignSelf: "flex-start" }} onClick={() => setPicker(true)}>＋ {t("Add provider")}</button>
        </div>
      </div>

      <div class="ai-note" role="note">
        {t("Conversations, the show content needed to answer and any attached images are sent to the chosen provider. With Ollama or LM Studio nothing leaves your computer.")}
        <br />
        {t("Usage is billed by your provider. The token counters in the panel are only a guide.")}
      </div>

      {picker && (
        <Modal title={t("Add provider")} onClose={() => setPicker(false)} width={720}>
          <div class="dim">{t("Pick a service. Any provider with an OpenAI-compatible API works, including local servers.")}</div>
          <div class="ai-presets">
            {cfg.presets.map((pr) => (
              <div key={pr.id} class="ai-preset-cell">
                <button class="ai-preset" onClick={() => { setPicker(false); setSheet(fromPreset(pr, taken)); }}>
                  <b>{pr.id === "custom" ? t("Custom") : pr.name}</b>
                  <span class="faint" style={{ fontSize: "var(--fs-xs)" }}>{pr.id === "custom" ? t("Any OpenAI-compatible server") : pr.needs_key ? t("Needs an API key") : t("No key needed")}</span>
                  {isLocal(pr.base_url) && <span class="chip ok" style={{ height: "auto", whiteSpace: "normal", padding: "3px 10px" }}>{LOCAL_TAG()}</span>}
                </button>
                {pr.doc_url && pr.id !== "custom" && <a class="ai-preset-link" href={pr.doc_url} target="_blank" rel="noopener noreferrer">{pr.needs_key ? t("Get a key") : t("Install guide")}</a>}
              </div>
            ))}
          </div>
        </Modal>
      )}
      {sheet && <ProviderSheet init={sheet} onClose={() => setSheet(null)} />}
    </SettingsPage>
  );
}
