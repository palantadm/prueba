//! Machine-level assistant configuration: providers, keys, active model. Stored in `<data>/ai.json`
//! (0600, atomic writes), never inside the show, never sent to clients with keys.

use super::types::Target;
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderRec {
    pub id: String,
    pub name: String,
    pub kind: AiProviderKind,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub models: Vec<AiModelInfo>,
}

impl std::fmt::Debug for ProviderRec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRec")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiFile {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub active_provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "medium")]
    pub effort: String,
    #[serde(default = "smart")]
    pub mode: AiMode,
    #[serde(default = "normal_style")]
    pub style: String,
    #[serde(default)]
    pub providers: Vec<ProviderRec>,
}

fn yes() -> bool {
    true
}
fn medium() -> String {
    "medium".into()
}
fn normal_style() -> String {
    "normal".into()
}
fn smart() -> AiMode {
    AiMode::Smart
}

impl Default for AiFile {
    fn default() -> Self {
        AiFile { enabled: true, active_provider: String::new(), model: String::new(), effort: medium(), mode: AiMode::Smart, style: normal_style(), providers: vec![] }
    }
}

fn m(id: &str, label: &str, note: &str, vision: bool, tools: bool, reasoning: bool) -> AiModelInfo {
    AiModelInfo { id: id.into(), label: label.into(), note: note.into(), vision, tools, reasoning }
}

fn preset(id: &str, name: &str, kind: AiProviderKind, base: &str, needs_key: bool, doc: &str, models: Vec<AiModelInfo>) -> AiPreset {
    AiPreset { id: id.into(), name: name.into(), kind, base_url: base.into(), needs_key, doc_url: doc.into(), models }
}

/// Built-in catalog. Model lists are suggestions: `AiListModels` refreshes them from the service.
pub fn presets() -> Vec<AiPreset> {
    use AiProviderKind::*;
    vec![
        preset("anthropic", "Anthropic (Claude)", Anthropic, "https://api.anthropic.com", true, "https://console.anthropic.com/settings/keys", vec![
            m("claude-opus-5-5", "Opus 5.5", "La más capaz, por defecto", true, true, true),
            m("claude-sonnet-5-5", "Sonnet 5.5", "Rápida", true, true, true),
            m("claude-haiku-5-5", "Haiku 5.5", "Muy rápida y económica", true, true, true),
            m("claude-fable-5-1", "Fable 5.1", "Máxima capacidad, la más cara", true, true, true),
        ]),
        preset("openai", "OpenAI", OpenaiCompat, "https://api.openai.com/v1", true, "https://platform.openai.com/api-keys", vec![
            m("gpt-5", "GPT-5", "Razonamiento, la más capaz", true, true, true),
            m("gpt-5-mini", "GPT-5 mini", "Rápida y económica", true, true, true),
            m("gpt-4.1", "GPT-4.1", "Sin razonamiento, muy rápida", true, true, false),
            m("gpt-4o-mini", "GPT-4o mini", "Económica", true, true, false),
        ]),
        preset("gemini", "Google Gemini", OpenaiCompat, "https://generativelanguage.googleapis.com/v1beta/openai", true, "https://aistudio.google.com/apikey", vec![
            m("gemini-2.5-pro", "Gemini 2.5 Pro", "La más capaz", true, true, true),
            m("gemini-2.5-flash", "Gemini 2.5 Flash", "Rápida", true, true, true),
            m("gemini-2.0-flash", "Gemini 2.0 Flash", "Muy rápida", true, true, false),
        ]),
        preset("openrouter", "OpenRouter", OpenaiCompat, "https://openrouter.ai/api/v1", true, "https://openrouter.ai/keys", vec![
            m("openai/gpt-4o-mini", "GPT-4o mini", "Vía OpenRouter", true, true, false),
            m("google/gemini-2.5-flash", "Gemini 2.5 Flash", "Vía OpenRouter", true, true, false),
            m("anthropic/claude-sonnet-4.5", "Claude Sonnet 4.5", "Vía OpenRouter", true, true, false),
        ]),
        preset("groq", "Groq", OpenaiCompat, "https://api.groq.com/openai/v1", true, "https://console.groq.com/keys", vec![
            m("llama-3.3-70b-versatile", "Llama 3.3 70B", "Muy rápida", false, true, false),
            m("openai/gpt-oss-120b", "GPT-OSS 120B", "Razonamiento abierto", false, true, true),
        ]),
        preset("mistral", "Mistral", OpenaiCompat, "https://api.mistral.ai/v1", true, "https://console.mistral.ai/api-keys", vec![
            m("mistral-large-latest", "Mistral Large", "La más capaz", false, true, false),
            m("mistral-small-latest", "Mistral Small", "Económica", false, true, false),
            m("pixtral-large-latest", "Pixtral Large", "Con visión", true, true, false),
        ]),
        preset("deepseek", "DeepSeek", OpenaiCompat, "https://api.deepseek.com/v1", true, "https://platform.deepseek.com/api_keys", vec![
            m("deepseek-chat", "DeepSeek Chat", "Económica", false, true, false),
            m("deepseek-reasoner", "DeepSeek Reasoner", "Razonamiento", false, true, true),
        ]),
        preset("together", "Together AI", OpenaiCompat, "https://api.together.xyz/v1", true, "https://api.together.ai/settings/api-keys", vec![
            m("meta-llama/Llama-3.3-70B-Instruct-Turbo", "Llama 3.3 70B Turbo", "Abierta", false, true, false),
        ]),
        preset("xai", "xAI (Grok)", OpenaiCompat, "https://api.x.ai/v1", true, "https://console.x.ai", vec![
            m("grok-4", "Grok 4", "Razonamiento", true, true, true),
            m("grok-3-mini", "Grok 3 mini", "Rápida", false, true, true),
        ]),
        preset("perplexity", "Perplexity", OpenaiCompat, "https://api.perplexity.ai", true, "https://www.perplexity.ai/settings/api", vec![
            m("sonar", "Sonar", "Solo chat y ayuda (sin herramientas)", false, false, false),
        ]),
        preset("ollama", "Ollama (local)", OpenaiCompat, "http://localhost:11434/v1", false, "https://ollama.com/download", vec![
            m("llama3.1", "Llama 3.1", "Local; admite herramientas", false, true, false),
            m("qwen2.5", "Qwen 2.5", "Local; admite herramientas", false, true, false),
            m("llama3.2-vision", "Llama 3.2 Vision", "Local; con visión, sin herramientas", true, false, false),
        ]),
        preset("lmstudio", "LM Studio (local)", OpenaiCompat, "http://localhost:1234/v1", false, "https://lmstudio.ai", vec![]),
        preset("custom", "Personalizado (compatible OpenAI)", OpenaiCompat, "", true, "", vec![]),
    ]
}

pub fn preset_by_id(id: &str) -> Option<AiPreset> {
    presets().into_iter().find(|p| p.id == id)
}

/// Host part of a URL (lowercase, without port).
pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let auth = rest.split(['/', '?', '#']).next().unwrap_or("");
    let auth = auth.rsplit('@').next().unwrap_or(auth);
    let host = if let Some(h) = auth.strip_prefix('[') { h.split(']').next().unwrap_or("") } else { auth.split(':').next().unwrap_or("") };
    host.to_lowercase()
}

pub fn is_local(url: &str) -> bool {
    let h = host_of(url);
    h == "localhost" || h.ends_with(".localhost") || h == "::1" || h == "0.0.0.0" || h.starts_with("127.")
}

pub fn needs_key(rec: &ProviderRec) -> bool {
    match rec.kind {
        AiProviderKind::Anthropic => !is_local(&rec.base_url) || rec.base_url.contains("anthropic.com"),
        AiProviderKind::OpenaiCompat => !is_local(&rec.base_url),
    }
}

fn env_names(rec: &ProviderRec) -> Vec<&'static str> {
    match rec.id.as_str() {
        "anthropic" => vec!["ANTHROPIC_API_KEY"],
        "openai" => vec!["OPENAI_API_KEY"],
        "gemini" => vec!["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        "openrouter" => vec!["OPENROUTER_API_KEY"],
        "groq" => vec!["GROQ_API_KEY"],
        "mistral" => vec!["MISTRAL_API_KEY"],
        "deepseek" => vec!["DEEPSEEK_API_KEY"],
        "together" => vec!["TOGETHER_API_KEY"],
        "xai" => vec!["XAI_API_KEY"],
        "perplexity" => vec!["PERPLEXITY_API_KEY"],
        _ => match rec.kind {
            AiProviderKind::Anthropic => vec!["ANTHROPIC_API_KEY"],
            AiProviderKind::OpenaiCompat => vec![],
        },
    }
}

/// Stored key, else the provider's environment variable.
pub fn resolve_key(rec: &ProviderRec) -> Option<String> {
    if let Some(k) = rec.api_key.as_ref().filter(|k| !k.trim().is_empty()) {
        return Some(k.trim().to_string());
    }
    env_names(rec).into_iter().filter_map(|n| std::env::var(n).ok()).map(|k| k.trim().to_string()).find(|k| !k.is_empty())
}

pub fn slug_ok(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

pub fn normalize_base(kind: AiProviderKind, base: &str) -> Result<String, String> {
    let b = base.trim().trim_end_matches('/').to_string();
    if !(b.starts_with("http://") || b.starts_with("https://")) {
        return Err("La URL base debe empezar por http:// o https://".into());
    }
    if b.len() > 300 || b.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("URL base no válida".into());
    }
    Ok(match kind {
        AiProviderKind::Anthropic => b.strip_suffix("/v1").unwrap_or(&b).to_string(),
        AiProviderKind::OpenaiCompat => b,
    })
}

impl AiFile {
    pub fn load(path: &Path) -> AiFile {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp: PathBuf = path.with_extension("json.tmp");
        {
            use std::io::Write;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut f = opts.open(&tmp).map_err(|e| e.to_string())?;
            f.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
            f.sync_all().ok();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderRec> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn out(&self) -> AiConfigOut {
        AiConfigOut {
            enabled: self.enabled,
            active_provider: self.active_provider.clone(),
            model: self.model.clone(),
            effort: self.effort.clone(),
            mode: self.mode,
            style: self.style.clone(),
            providers: self
                .providers
                .iter()
                .map(|p| AiProviderOut {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    kind: p.kind,
                    base_url: p.base_url.clone(),
                    has_key: resolve_key(p).is_some(),
                    needs_key: needs_key(p),
                    models: p.models.clone(),
                    builtin: preset_by_id(&p.id).is_some(),
                })
                .collect(),
            presets: presets(),
        }
    }

    /// Apply a client change set. Validates everything before mutating.
    pub fn apply(&mut self, c: AiConfigIn) -> Result<(), String> {
        let mut next = self.clone();
        if let Some(pin) = c.provider {
            let id = pin.id.trim().to_string();
            if !slug_ok(&id) {
                return Err("Identificador de proveedor no válido (usa minúsculas, números, - y _)".into());
            }
            let preset = preset_by_id(&id);
            let name = if pin.name.trim().is_empty() { preset.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| id.clone()) } else { pin.name.trim().chars().take(60).collect() };
            let base_raw = if pin.base_url.trim().is_empty() { preset.as_ref().map(|p| p.base_url.clone()).unwrap_or_default() } else { pin.base_url.clone() };
            let base = normalize_base(pin.kind, &base_raw)?;
            let idx = next.providers.iter().position(|p| p.id == id);
            let models = match (pin.models, idx) {
                (Some(ms), _) => ms,
                (None, Some(i)) => next.providers[i].models.clone(),
                (None, None) => preset.as_ref().map(|p| p.models.clone()).unwrap_or_default(),
            };
            let mut seen = std::collections::HashSet::new();
            for mo in &models {
                if mo.id.trim().is_empty() || mo.id.len() > 120 || mo.id.chars().any(|c| c.is_control()) {
                    return Err("Identificador de modelo no válido".into());
                }
                if !seen.insert(mo.id.clone()) {
                    return Err(format!("Modelo duplicado: {}", mo.id));
                }
            }
            let key = match (pin.api_key, idx) {
                (Some(k), _) if k.trim().is_empty() => None,
                (Some(k), _) => Some(k.trim().to_string()),
                (None, Some(i)) => next.providers[i].api_key.clone(),
                (None, None) => None,
            };
            let rec = ProviderRec { id: id.clone(), name, kind: pin.kind, base_url: base, api_key: key, models };
            match idx {
                Some(i) => next.providers[i] = rec,
                None => next.providers.push(rec),
            }
            if next.active_provider.is_empty() {
                next.active_provider = id;
            }
        }
        if let Some(rm) = c.remove_provider {
            next.providers.retain(|p| p.id != rm);
            if next.active_provider == rm {
                next.active_provider = next.providers.first().map(|p| p.id.clone()).unwrap_or_default();
                next.model.clear();
            }
        }
        if let Some(a) = c.active_provider {
            if next.provider(&a).is_none() {
                return Err(format!("Proveedor desconocido: {a}"));
            }
            if next.active_provider != a {
                next.model.clear();
            }
            next.active_provider = a;
        }
        if let Some(mo) = c.model {
            let p = next.provider(&next.active_provider).ok_or("Elige primero un proveedor")?;
            if !p.models.iter().any(|x| x.id == mo) {
                return Err(format!("Modelo desconocido para este proveedor: {mo} (actualiza la lista de modelos)"));
            }
            next.model = mo;
        }
        if let Some(e) = c.effort {
            if !matches!(e.as_str(), "low" | "medium" | "high") {
                return Err("effort debe ser low, medium o high".into());
            }
            next.effort = e;
        }
        if let Some(md) = c.mode {
            next.mode = md;
        }
        if let Some(st) = c.style {
            if !matches!(st.as_str(), "normal" | "concise" | "caveman") {
                return Err("style debe ser normal, concise o caveman".into());
            }
            next.style = st;
        }
        if let Some(en) = c.enabled {
            next.enabled = en;
        }
        // default model for the active provider
        if next.model.is_empty() {
            if let Some(p) = next.provider(&next.active_provider) {
                next.model = p.models.iter().find(|x| x.tools).or(p.models.first()).map(|x| x.id.clone()).unwrap_or_default();
            }
        }
        *self = next;
        Ok(())
    }

    /// Resolve the request target for the active provider/model.
    pub fn target(&self) -> Result<Target, String> {
        if !self.enabled {
            return Err("El asistente está desactivado en Ajustes → Asistente IA.".into());
        }
        if self.provider(&self.active_provider).is_none() {
            return Err("El asistente no está configurado. Ve a Ajustes → Asistente IA y añade un proveedor (Claude, OpenAI, Gemini, Ollama…).".into());
        }
        self.target_for(&self.active_provider, Some(&self.model))
    }

    /// Target for any configured provider (model listing / connection test). `model` `None` or empty → first
    /// tool-capable model of the provider.
    pub fn target_for(&self, id: &str, model: Option<&str>) -> Result<Target, String> {
        let p = self.provider(id).ok_or_else(|| format!("Proveedor desconocido: {id}"))?;
        let model = match model.filter(|m| !m.is_empty()) {
            Some(m) => p.models.iter().find(|x| x.id == m).cloned().ok_or("Elige un modelo en Ajustes → Asistente IA.")?,
            None => p.models.iter().find(|x| x.tools).or(p.models.first()).cloned().ok_or("Este proveedor no tiene modelos: añade uno o pulsa «Actualizar modelos».")?,
        };
        let key = resolve_key(p);
        if key.is_none() && needs_key(p) {
            return Err(format!("Falta la clave de API de {}. Añádela en Ajustes → Asistente IA.", p.name));
        }
        let base_url = match p.kind {
            AiProviderKind::Anthropic => std::env::var("LUMOS_AI_BASE_URL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| p.base_url.clone()),
            AiProviderKind::OpenaiCompat => std::env::var("LUMOS_AI_OPENAI_BASE_URL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| p.base_url.clone()),
        };
        Ok(Target { kind: p.kind, base_url: base_url.trim_end_matches('/').to_string(), api_key: key, model })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(id: &str, key: Option<&str>) -> AiConfigIn {
        AiConfigIn {
            provider: Some(AiProviderIn { id: id.into(), name: String::new(), kind: preset_by_id(id).map(|p| p.kind).unwrap_or(AiProviderKind::OpenaiCompat), base_url: String::new(), api_key: key.map(|s| s.into()), models: None }),
            remove_provider: None,
            active_provider: None,
            model: None,
            enabled: None,
            effort: None, style: None,
            mode: None,
        }
    }

    fn empty() -> AiConfigIn {
        AiConfigIn { provider: None, remove_provider: None, active_provider: None, model: None, enabled: None, effort: None, style: None, mode: None }
    }

    #[test]
    fn preset_defaults_and_active_selection() {
        let mut c = AiFile::default();
        assert!(c.target().is_err());
        c.apply(add("anthropic", Some("sk-ant-secret"))).unwrap();
        assert_eq!(c.active_provider, "anthropic");
        assert_eq!(c.model, "claude-opus-5-5");
        let t = c.target().unwrap();
        assert_eq!(t.base_url, "https://api.anthropic.com");
        c.apply(add("ollama", None)).unwrap();
        c.apply(AiConfigIn { active_provider: Some("ollama".into()), ..empty() }).unwrap();
        assert_eq!(c.model, "llama3.1");
        let t = c.target().unwrap();
        assert!(t.api_key.is_none(), "local servers need no key");
        assert!(c.apply(AiConfigIn { model: Some("nope".into()), ..empty() }).is_err());
        assert!(c.apply(AiConfigIn { effort: Some("max".into()), ..empty() }).is_err());
    }

    #[test]
    fn key_handling_and_redaction() {
        let mut c = AiFile::default();
        c.apply(add("openai", Some("  sk-test-123  "))).unwrap();
        assert_eq!(c.providers[0].api_key.as_deref(), Some("sk-test-123"));
        let out = serde_json::to_string(&c.out()).unwrap();
        assert!(!out.contains("sk-test-123"), "key leaked to clients");
        assert!(out.contains("\"has_key\":true"));
        assert!(!format!("{c:?}").contains("sk-test-123"), "key leaked via Debug");
        // keep key when None, clear when Some("")
        c.apply(AiConfigIn { provider: Some(AiProviderIn { id: "openai".into(), name: "OpenAI".into(), kind: AiProviderKind::OpenaiCompat, base_url: "https://api.openai.com/v1/".into(), api_key: None, models: None }), ..empty() }).unwrap();
        assert_eq!(c.providers[0].api_key.as_deref(), Some("sk-test-123"));
        assert_eq!(c.providers[0].base_url, "https://api.openai.com/v1");
        c.apply(AiConfigIn { provider: Some(AiProviderIn { id: "openai".into(), name: "OpenAI".into(), kind: AiProviderKind::OpenaiCompat, base_url: "https://api.openai.com/v1".into(), api_key: Some(String::new()), models: None }), ..empty() }).unwrap();
        assert!(c.providers[0].api_key.is_none());
    }

    #[test]
    fn validation() {
        let mut c = AiFile::default();
        let bad = AiConfigIn { provider: Some(AiProviderIn { id: "Bad Id".into(), name: "x".into(), kind: AiProviderKind::OpenaiCompat, base_url: "http://x".into(), api_key: None, models: None }), ..empty() };
        assert!(c.apply(bad).is_err());
        let bad = AiConfigIn { provider: Some(AiProviderIn { id: "x".into(), name: "x".into(), kind: AiProviderKind::OpenaiCompat, base_url: "ftp://x".into(), api_key: None, models: None }), ..empty() };
        assert!(c.apply(bad).is_err());
        assert!(c.providers.is_empty(), "failed apply must not mutate");
        assert!(c.apply(AiConfigIn { active_provider: Some("zzz".into()), ..empty() }).is_err());
    }

    #[test]
    fn local_detection_and_needs_key() {
        assert!(is_local("http://localhost:11434/v1"));
        assert!(is_local("http://127.0.0.1:1234/v1"));
        assert!(is_local("http://[::1]:8000"));
        assert!(!is_local("https://api.openai.com/v1"));
        assert!(!is_local("http://192.168.1.20:11434/v1"));
        assert_eq!(host_of("https://user:pw@Example.com:8080/x"), "example.com");
    }

    #[test]
    fn persistence_permissions_roundtrip() {
        let dir = std::env::temp_dir().join(format!("lumos-ai-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ai.json");
        let mut c = AiFile::default();
        c.apply(add("groq", Some("gsk-abc"))).unwrap();
        c.save(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let back = AiFile::load(&path);
        assert_eq!(back.providers[0].api_key.as_deref(), Some("gsk-abc"));
        assert_eq!(back.active_provider, "groq");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
