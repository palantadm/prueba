//! Tool definitions, risk classification and execution for the assistant.
//!
//! Every tool is described to the model with a portable JSON schema (simple types only, every tool has at
//! least one optional property so strict providers accept it). Results are compact text capped at ~20 KB.

use super::types::ToolDef;
use super::AiState;
use crate::engine::Handle;
use crate::model::*;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub const MAX_RESULT: usize = 20_000;

// ───────────────────────────── catalog generated from model.rs ─────────────────────────────

const MODEL_SRC: &str = include_str!("../model.rs");

fn snake(s: &str) -> String {
    let mut o = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                o.push('_');
            }
            o.extend(c.to_lowercase());
        } else {
            o.push(c);
        }
    }
    o
}

/// Body text between the braces of `pub enum|struct <name> {` (brace balanced).
fn item_body(name: &str) -> Option<(bool, &'static str)> {
    for (kw, is_enum) in [("pub enum ", true), ("pub struct ", false)] {
        let pat = format!("{kw}{name} {{");
        if let Some(i) = MODEL_SRC.find(&pat) {
            let start = i + pat.len();
            let mut depth = 1;
            for (j, c) in MODEL_SRC[start..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some((is_enum, &MODEL_SRC[start..start + j]));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// Compact one-line description of a model item: comments dropped, `serde(default)` fields marked `?`.
fn compact(body: &str) -> String {
    let mut out = String::new();
    let mut opt = false;
    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        if t.starts_with("#[") {
            if t.contains("default") {
                opt = true;
            }
            continue;
        }
        let mut l = t.trim_start_matches("pub ").to_string();
        if let Some(c) = l.find(':') {
            let head = &l[..c];
            if opt && head.chars().all(|ch| ch.is_alphanumeric() || ch == '_') && !head.is_empty() {
                l.insert(c, '?');
            }
        }
        opt = false;
        out.push_str(&l);
        out.push(' ');
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `type_name {fields}` per Command variant (snake_case `type` tag).
pub fn command_catalog() -> &'static [(String, String)] {
    static C: OnceLock<Vec<(String, String)>> = OnceLock::new();
    C.get_or_init(|| {
        let Some((_, body)) = item_body("Command") else { return vec![] };
        // strip comments / attributes first, then split variants at top-level commas
        let mut cleaned = String::new();
        for line in body.lines() {
            let t = line.trim();
            if t.starts_with("//") || t.starts_with("#[") || t.is_empty() {
                // keep `serde(default…)` markers inside variant fields
                if t.starts_with("#[serde(default") {
                    cleaned.push_str("§ ");
                }
                continue;
            }
            cleaned.push_str(t);
            cleaned.push(' ');
        }
        let mut variants = Vec::new();
        let (mut depth, mut cur) = (0i32, String::new());
        for c in cleaned.chars() {
            match c {
                '{' => {
                    depth += 1;
                    cur.push(c)
                }
                '}' => {
                    depth -= 1;
                    cur.push(c)
                }
                ',' if depth == 0 => {
                    variants.push(std::mem::take(&mut cur));
                }
                _ => cur.push(c),
            }
        }
        if !cur.trim().is_empty() {
            variants.push(cur);
        }
        variants
            .into_iter()
            .filter_map(|v| {
                let v = v.trim().trim_start_matches("§").trim().to_string();
                if v.is_empty() {
                    return None;
                }
                let (name, rest) = match v.find('{') {
                    Some(i) => (v[..i].trim().to_string(), v[i..].replace("pub ", "").replace("§ ", "?").split_whitespace().collect::<Vec<_>>().join(" ")),
                    None => (v.clone(), String::new()),
                };
                if name.is_empty() || !name.chars().next().unwrap().is_uppercase() {
                    return None;
                }
                Some((snake(&name), rest))
            })
            .collect()
    })
}

pub fn type_def(name: &str) -> Option<String> {
    let (is_enum, body) = item_body(name)?;
    let c = compact(body);
    Some(if is_enum { format!("{name} = {c}") } else { format!("{name} {{{c}}}") })
}

fn send_command_description() -> String {
    let skip = ["command_line", "midi_in", "subscribe_dmx", "learn_start", "learn_stop", "ai_"];
    let mut s = String::from(
        "Send ONE raw Lumos command as a JSON object `{\"type\": \"<name>\", ...fields}` (fields marked `?` are optional). \
         Attribute values are 0..1 floats; a PerFixture map is `{\"<fixture id>\": {\"dimmer\": 1, \"red\": 0.5}}`. \
         Prefer `console_command` for simple operations (selection/at/record/go) and use this for structured edits \
         (saving cuelists, palettes, scenes, effects, chases, macros, timelines, universes, patch, vconsole). \
         Look up ids with get_show first; never invent ids. Command types:\n",
    );
    for (n, f) in command_catalog() {
        if skip.iter().any(|k| n.starts_with(k)) {
            continue;
        }
        s.push_str(&format!("- {n} {f}\n"));
    }
    s.push_str("Nested types:\n");
    for t in [
        "Attrs", "Fixture", "Group", "Palette", "PaletteKind", "Cue", "Cuelist", "AttrTiming", "Scene", "SceneKind", "Effect", "Shape", "Chase", "ChaseStep", "ChaseDir", "SpeedMaster", "Macro", "MacroStep", "Timeline",
        "TlEvent", "Executor", "ExecKind", "UniverseCfg", "OutputCfg", "OutputKind", "TestMode", "RecordMode", "UpdateMode", "Binding", "Source", "Action",
    ] {
        if t == "Attrs" {
            s.push_str("Attrs = {\"<attr>\": 0..1}; attr names: dimmer pan tilt red green blue white amber uv lime cto colorwheel gobo gobo_rot prism prism_rot frost zoom focus iris shutter speed effect control\n");
        } else if let Some(d) = type_def(t) {
            s.push_str(&d);
            s.push('\n');
        }
    }
    s
}

// ───────────────────────────── tool definitions ─────────────────────────────

fn tool(name: &str, description: &str, props: Value, required: &[&str]) -> ToolDef {
    let mut schema = json!({ "type": "object", "properties": props });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    ToolDef { name: name.into(), description: description.into(), schema }
}

pub fn help_topics_line() -> String {
    super::help::TOPICS.iter().map(|(id, _, _)| *id).collect::<Vec<_>>().join(", ")
}

pub fn definitions() -> Vec<ToolDef> {
    vec![
        tool(
            "console_command",
            "Run console command-line text (the same language as the command bar), e.g. `fixture 1 thru 4 at 70; red at 100`, `record scene 3 Amanecer`, `go`, `scene 2 on`, `gm 80`. \
             Statements are separated by `;` or newlines and share a selection. Returns one result line per statement. See get_help topic `linea-de-comandos` for the grammar.",
            json!({ "text": { "type": "string", "description": "Command text. Spanish or English keywords." } }),
            &["text"],
        ),
        tool("send_command", &send_command_description(), json!({ "command": { "type": "object", "description": "The command object; must contain `type`." } }), &["command"]),
        tool(
            "get_show",
            "Read the show document, compactly. `section`: all (counts + names, default), fixtures, groups, palettes, scenes, cuelists, chases, macros, timelines, universes, settings, vconsole, stage, speed_masters, executors, effects, bindings, scripts (Guion: blocks + cue marks), locate (per-profile «Localizar» overrides). \
             `id` returns full detail of one item (e.g. a cuelist with all its cues). `limit` caps list length (default 60).",
            json!({
                "section": { "type": "string", "description": "Which part of the show." },
                "id": { "type": "integer", "description": "Item id for full detail." },
                "limit": { "type": "integer", "description": "Maximum items." }
            }),
            &[],
        ),
        tool("get_runtime", "Live state: grand master, blackout, blind, programmer contents, running cuelists/scenes/chases/effects, output status per universe, input status, timecode, recent command-line results.", json!({ "verbose": { "type": "boolean", "description": "Include executor and virtual-console state." } }), &[]),
        tool(
            "get_fixture_state",
            "Diagnose fixtures: profile/mode/address, programmer values, resolved attribute values, the DMX bytes actually output, and hints about why a fixture may be dark (blackout, grand master, blind, parked channels, dimmer at 0, universe without output…).",
            json!({ "ids": { "type": "array", "items": { "type": "integer" }, "description": "Fixture ids (max 20)." } }),
            &["ids"],
        ),
        tool(
            "get_dmx",
            "Read the DMX frame of a universe (non-zero channels in a range, with the fixture that owns each channel).",
            json!({ "universe": { "type": "integer" }, "from": { "type": "integer", "description": "First channel (1-512), default 1." }, "to": { "type": "integer", "description": "Last channel, default from+63." } }),
            &["universe"],
        ),
        tool(
            "diagnose_outputs",
            "Diagnose the DMX output chain and network: per universe/output config (Art-Net, sACN, OSC, serial, USB), live status and frame counters, Art-Net nodes that answer ArtPoll (re-scans first, ~3 s), \
             this machine's IP addresses, serial ports, universe/port-address mismatches, subnet problems, blackout/test/park overrides. Returns findings with a concrete fix each. \
             Use it whenever an output does not work or a node/fixture does not react; fix with `send_command universe_save` (ask first if the fix is ambiguous) and confirm with `output_test`.",
            json!({
                "universe": { "type": "integer", "description": "Only this universe (default: all)." },
                "scan": { "type": "boolean", "description": "Poll for Art-Net nodes first (default true)." }
            }),
            &[],
        ),
        tool("search_library", "Search fixture profiles (built-in, GDTF, QLC+, OFL imports) by manufacturer/model/category words. Use before adding fixtures.", json!({ "query": { "type": "string" }, "limit": { "type": "integer" } }), &["query"]),
        tool("get_profile", "Modes, channels, attributes and capabilities (gobo/colour slots) of one fixture profile.", json!({ "id": { "type": "string", "description": "Profile id from search_library." } }), &["id"]),
        tool(
            "get_help",
            &format!("Documentation of Lumos (Spanish). Topics: {}. Call with no topic for the index.", help_topics_line()),
            json!({ "topic": { "type": "string" } }),
            &[],
        ),
        tool(
            "ask_user",
            "Ask the user a short question and wait for the answer. Use ONLY when the request is truly ambiguous or a choice changes the result; offer 2-5 quick options when possible.",
            json!({ "question": { "type": "string" }, "options": { "type": "array", "items": { "type": "string" } } }),
            &["question"],
        ),
        tool("wait", "Wait up to 10 seconds (to let a fade or chase run before inspecting the result).", json!({ "seconds": { "type": "number" } }), &["seconds"]),
    ]
}

/// Tool names as emitted by some local models carry chat-template debris (`get_show<|channel|>`,
/// `functions.get_show`, quotes…). Reduce them to a known tool name when that is unambiguous; otherwise return them trimmed.
pub fn clean_tool_name(raw: &str) -> String {
    let mut s = raw.split("<|").next().unwrap_or("").trim();
    s = s.trim_matches(|c: char| c == '`' || c == '"' || c == '\'' || c.is_whitespace());
    for p in ["functions.", "functions:", "function.", "tools.", "tool.", "default_api.", "api."] {
        if let Some(r) = s.strip_prefix(p) {
            s = r;
        }
    }
    let end = s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).unwrap_or(s.len());
    let core = &s[..end];
    definitions().iter().find(|d| d.name.eq_ignore_ascii_case(core)).map(|d| d.name.clone()).unwrap_or_else(|| s.to_string())
}

pub const READ_TOOLS: &[&str] = &["get_show", "get_runtime", "get_fixture_state", "get_dmx", "diagnose_outputs", "search_library", "get_profile", "get_help", "ask_user", "wait"];

// ───────────────────────────── risk ─────────────────────────────

/// Risk of one `Command`. `blind`: programmer edits don't reach the output.
pub fn command_risk(c: &Command, blind: bool) -> AiRisk {
    use Command::*;
    match c {
        // programmer: live unless blind
        ProgSet { .. } | ProgRelease { .. } | ProgApplyPalette { .. } | CueLoad { .. } => {
            if blind {
                AiRisk::Edit
            } else {
                AiRisk::Live
            }
        }
        // show-data edits (undoable)
        ProgRecordPalette { .. } | ProgRecordCue { .. } | FixtureAdd { .. } | FixtureUpdate { .. } | FixtureMorph { .. } | LibraryImportOfl { .. } | GroupSave { .. } | GroupRemove { .. } | PaletteSave { .. } | PaletteRemove { .. }
        | EffectSave { .. } | EffectRemove { .. } | SceneRecord { .. } | SceneSave { .. } | SceneRemove { .. } | CueUpdate { .. } | ChaseSave { .. } | ChaseRemove { .. } | SpeedSave { .. } | SpeedRemove { .. }
        | MacroSave { .. } | MacroRemove { .. } | TimelineSave { .. } | TimelineRemove { .. } | CuelistSave { .. } | CueSave { .. } | CueRemove { .. } | ExecSave { .. } | ExecRemove { .. } | SettingsSave { .. } | BindingSave { .. }
        | BindingRemove { .. } | StageSave { .. } | VcSave { .. } | ShowRename { .. } | ShowSave { .. } | BackupCreate { .. } | BlindSet { .. } | Undo | Redo | NodesScan | ScriptSave { .. } | ScriptRemove { .. } | LocateSave { .. } | LocateCapture { .. } => AiRisk::Edit,
        // everything else changes output now, is destructive, or touches the outside world
        _ => AiRisk::Live,
    }
}

/// Classify a console command-line text statement by statement.
pub fn console_risk(text: &str, blind: bool) -> AiRisk {
    let mut risk = AiRisk::Edit;
    for st in text.split([';', '\n']) {
        let st = st.trim().to_lowercase();
        if st.is_empty() || st.starts_with('#') {
            continue;
        }
        let words: Vec<&str> = st.split_whitespace().collect();
        let has = |w: &[&str]| words.iter().any(|x| w.contains(x));
        let first = words[0];
        let live_first = ["go", "back", "goto", "pause", "release", "scene", "escena", "scenes", "chase", "macro", "timeline", "blackout", "bo", "gm", "master", "park", "unpark", "identify", "locate", "clear", "clr", "delete", "borrar", "undo", "deshacer", "redo", "rehacer"];
        let edit_first = ["record", "grabar", "update", "actualizar", "apply", "aplicar", "blind", "cuelist", "fixture", "fx", "aparato", "group", "grupo", "all", "todos", "none", "odd", "even", "impares", "pares", "next", "prev", "siguiente", "anterior"];
        let is_value = has(&["at", "@", "en", "full", "out", "home", "colour", "color", "pleno", "half", "mitad", "cero", "off", "max"]) || ["dimmer", "pan", "tilt", "red", "green", "blue", "white", "amber", "uv", "zoom", "focus", "iris", "shutter", "gobo", "prism", "frost", "cto"].contains(&first);
        let r = if first == "blind" || first == "undo" || first == "redo" || first == "deshacer" || first == "rehacer" {
            AiRisk::Edit
        } else if ["delete", "borrar"].contains(&first) {
            AiRisk::Live
        } else if live_first.contains(&first) {
            AiRisk::Live
        } else if ["record", "grabar", "update", "actualizar"].contains(&first) {
            AiRisk::Edit
        } else if is_value {
            if blind { AiRisk::Edit } else { AiRisk::Live }
        } else if edit_first.contains(&first) || first.chars().next().map_or(false, |c| c.is_ascii_digit()) {
            AiRisk::Edit
        } else {
            AiRisk::Live
        };
        if r == AiRisk::Live {
            risk = AiRisk::Live;
        }
    }
    risk
}

/// Risk of a tool call (as the model will invoke it).
pub fn tool_risk(name: &str, input: &Value, blind: bool) -> AiRisk {
    if READ_TOOLS.contains(&name) {
        return AiRisk::Read;
    }
    match name {
        "console_command" => console_risk(input.get("text").and_then(|t| t.as_str()).unwrap_or(""), blind),
        "send_command" => match serde_json::from_value::<Command>(input.get("command").cloned().unwrap_or(Value::Null)) {
            Ok(c) => command_risk(&c, blind),
            Err(_) => AiRisk::Live,
        },
        _ => AiRisk::Live,
    }
}

// ───────────────────────────── human summaries ─────────────────────────────

fn label_of(t: &str) -> &'static str {
    match t {
        "prog_set" => "Poner valores en el programador",
        "prog_release" => "Liberar el programador",
        "prog_apply_palette" => "Aplicar paleta",
        "prog_record_palette" => "Grabar paleta desde el programador",
        "prog_record_cue" => "Grabar cue desde el programador",
        "fixture_add" => "Añadir aparatos al patch",
        "fixture_update" => "Modificar aparato",
        "fixture_remove" => "Eliminar aparatos",
        "fixture_morph" => "Intercambiar modelo de aparatos (morph)",
        "library_import_ofl" => "Importar perfil de aparato",
        "group_save" => "Guardar grupo",
        "group_remove" => "Eliminar grupo",
        "palette_save" => "Guardar paleta",
        "palette_remove" => "Eliminar paleta",
        "effect_save" => "Guardar efecto",
        "effect_remove" => "Eliminar efecto",
        "effect_toggle" => "Activar/desactivar efecto",
        "scene_record" => "Grabar escena",
        "scene_save" => "Guardar escena",
        "scene_remove" => "Eliminar escena",
        "scene_go" => "Lanzar/liberar escena",
        "scene_solo" => "Escena en solitario",
        "scene_level" => "Nivel de escena",
        "scene_morph" => "Morph entre escenas",
        "scenes_release" => "Liberar todas las escenas",
        "undo" => "Deshacer",
        "redo" => "Rehacer",
        "blind_set" => "Modo blind",
        "cue_load" => "Cargar cue al programador",
        "cue_update" => "Actualizar cue",
        "unpark" => "Liberar canales aparcados",
        "chase_save" => "Guardar chase",
        "chase_remove" => "Eliminar chase",
        "chase_run" => "Arrancar/parar chase",
        "chase_bpm" => "BPM del chase",
        "speed_save" => "Guardar master de velocidad",
        "speed_set" => "Velocidad (BPM)",
        "macro_save" => "Guardar macro",
        "macro_remove" => "Eliminar macro",
        "macro_run" => "Ejecutar macro",
        "timeline_save" => "Guardar timeline",
        "timeline_remove" => "Eliminar timeline",
        "timeline_run" => "Reproducir timeline",
        "timeline_stop" => "Parar timeline",
        "backup_create" => "Crear copia de seguridad",
        "backup_restore" => "Restaurar copia de seguridad",
        "backup_delete" => "Eliminar copia de seguridad",
        "output_test" => "Test de salida",
        "identify" => "Identificar aparato",
        "cuelist_save" => "Guardar lista de cues",
        "cuelist_remove" => "Eliminar lista de cues",
        "cue_save" => "Guardar cue",
        "cue_remove" => "Eliminar cue",
        "go" => "GO",
        "back" => "ATRÁS",
        "go_to" => "Ir al cue",
        "pause" => "Pausa",
        "release_cuelist" => "Liberar lista de cues",
        "exec_save" => "Guardar ejecutor",
        "exec_remove" => "Quitar ejecutor",
        "exec_level" => "Nivel de ejecutor",
        "exec_go" => "GO de ejecutor",
        "exec_back" => "ATRÁS de ejecutor",
        "exec_flash" => "Flash de ejecutor",
        "grand_master" => "Master general",
        "blackout" => "Blackout",
        "dmx_set" => "Aparcar canal DMX",
        "universe_save" => "Guardar universo y salidas",
        "universe_remove" => "Eliminar universo",
        "settings_save" => "Guardar ajustes del show",
        "binding_save" => "Guardar mapeo de control",
        "binding_remove" => "Eliminar mapeo de control",
        "stage_save" => "Guardar escenario",
        "show_new" => "Nuevo show",
        "show_save" => "Guardar show",
        "show_load" => "Cargar show",
        "show_rename" => "Renombrar show",
        "vc_save" => "Guardar consola virtual",
        "library_remove" => "Quitar perfil de la librería",
        "timecode_ctl" => "Control del timecode",
        "nodes_scan" => "Buscar nodos Art-Net",
        "script_save" => "Guardar guion",
        "script_remove" => "Eliminar guion",
        "script_go" => "GO del guion",
        "script_back" => "Atrás en el guion",
        "script_goto" => "Ir a un punto del guion",
        "script_fire" => "Lanzar una marca del guion",
        "script_follow" => "Seguimiento del guion",
        "script_reset" => "Reiniciar el guion",
        "locate_save" => "Guardar ajuste de Localizar del perfil",
        "locate_capture" => "Capturar ajuste de Localizar desde un aparato",
        _ => "",
    }
}

fn short(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}

/// One-line Spanish description of a tool call.
pub fn summary(name: &str, input: &Value) -> String {
    match name {
        "console_command" => format!("Consola: «{}»", short(input.get("text").and_then(|t| t.as_str()).unwrap_or(""), 120)),
        "send_command" => {
            let c = input.get("command").cloned().unwrap_or(Value::Null);
            let t = c.get("type").and_then(|t| t.as_str()).unwrap_or("?");
            let label = label_of(t);
            let named = c.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()).or_else(|| c.pointer("/scene/name").and_then(|n| n.as_str()).map(|s| s.to_string())).or_else(|| c.pointer("/cuelist/name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .or_else(|| c.pointer("/palette/name").and_then(|n| n.as_str()).map(|s| s.to_string())).or_else(|| c.pointer("/group/name").and_then(|n| n.as_str()).map(|s| s.to_string())).or_else(|| c.pointer("/macro/name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .or_else(|| c.pointer("/chase/name").and_then(|n| n.as_str()).map(|s| s.to_string())).or_else(|| c.pointer("/effect/name").and_then(|n| n.as_str()).map(|s| s.to_string())).or_else(|| c.pointer("/timeline/name").and_then(|n| n.as_str()).map(|s| s.to_string()));
            let mut base = if label.is_empty() { format!("Comando {t}") } else { label.to_string() };
            if let Some(n) = named {
                base.push_str(&format!(" «{}»", short(&n, 40)));
            }
            if matches!(t, "fixture_add") {
                base.push_str(&format!(" ({} × {})", c.get("count").and_then(|x| x.as_u64()).unwrap_or(1), c.get("profile").and_then(|x| x.as_str()).unwrap_or("?")));
            }
            base
        }
        "get_show" => format!("Consultar el show ({})", input.get("section").and_then(|s| s.as_str()).unwrap_or("resumen")),
        "get_runtime" => "Consultar el estado en vivo".into(),
        "get_fixture_state" => "Diagnosticar aparatos".into(),
        "diagnose_outputs" => "Diagnosticar salidas y red".into(),
        "get_dmx" => format!("Leer DMX del universo {}", input.get("universe").and_then(|u| u.as_u64()).unwrap_or(0)),
        "search_library" => format!("Buscar en la librería: «{}»", short(input.get("query").and_then(|s| s.as_str()).unwrap_or(""), 50)),
        "get_profile" => format!("Ver perfil {}", short(input.get("id").and_then(|s| s.as_str()).unwrap_or(""), 60)),
        "get_help" => format!("Consultar la ayuda ({})", input.get("topic").and_then(|s| s.as_str()).unwrap_or("índice")),
        "ask_user" => "Preguntar al usuario".into(),
        "wait" => "Esperar".into(),
        other => format!("Herramienta {other}"),
    }
}

// ───────────────────────────── execution ─────────────────────────────

pub fn cap(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut cut = max;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        let extra = s.len() - cut;
        s.truncate(cut);
        s.push_str(&format!("\n…[truncado: {extra} bytes más; usa `id`, `limit` o un rango más estrecho]"));
    }
    s
}

fn num(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(|x| x.as_i64().or_else(|| x.as_f64().map(|f| f as i64)))
}

fn fx_row(f: &Fixture, st: &AiState) -> Value {
    let (prof, mode) = st.profile_mode(&f.profile, &f.mode);
    json!({
        "id": f.id, "name": f.name, "profile": f.profile, "mode": f.mode, "universe": f.universe, "address": f.address,
        "channels": mode.map(|m| crate::library::mode_footprint(m)).unwrap_or(0),
        "known_profile": prof.is_some(),
    })
}

fn attrs_compact(a: &Attrs) -> Value {
    Value::Object(a.iter().map(|(k, v)| (k.clone(), json!((*v as f64 * 1000.0).round() / 1000.0))).collect::<Map<_, _>>())
}

fn per_fixture_compact(p: &PerFixture) -> Value {
    Value::Object(p.iter().map(|(k, v)| (k.to_string(), attrs_compact(v))).collect::<Map<_, _>>())
}

fn cue_row(c: &Cue, detail: bool) -> Value {
    let mut v = json!({
        "number": c.number, "label": c.label, "fade_in": c.fade_in, "fade_out": c.fade_out, "delay": c.delay,
        "follow": c.follow, "timecode": c.timecode, "fixtures": c.changes.len(), "refs": c.refs.values().map(|m| m.len()).sum::<usize>(), "notes": c.notes,
    });
    if detail {
        v["changes"] = per_fixture_compact(&c.changes);
        if !c.refs.is_empty() {
            v["refs_map"] = json!(c.refs);
        }
        if !c.timing.is_empty() {
            v["timing"] = json!(c.timing);
        }
        v["curve"] = json!(c.curve);
        v["stagger"] = json!(c.stagger);
        v["block"] = json!(c.block);
    }
    v
}

pub fn get_show(show: &Show, st: &AiState, input: &Value) -> Result<String, String> {
    let section = input.get("section").and_then(|s| s.as_str()).unwrap_or("all");
    let id = num(input, "id");
    let limit = num(input, "limit").unwrap_or(60).clamp(1, 500) as usize;
    let list = |v: Vec<Value>| json!({ "count": v.len(), "items": v.into_iter().take(limit).collect::<Vec<_>>() });
    let v: Value = match section {
        "all" => {
            let names = |it: Vec<(u32, String)>| Value::Array(it.into_iter().take(limit).map(|(i, n)| json!({ "id": i, "name": n })).collect());
            json!({
                "name": show.name,
                "universes": show.universes.iter().map(|u| json!({ "id": u.id, "name": u.name, "outputs": u.outputs.len() })).collect::<Vec<_>>(),
                "fixtures": { "count": show.fixtures.len(), "first": show.fixtures.iter().take(12).map(|f| json!({ "id": f.id, "name": f.name, "profile": f.profile, "u": f.universe, "addr": f.address })).collect::<Vec<_>>() },
                "groups": names(show.groups.iter().map(|g| (g.id, format!("{} ({} aparatos)", g.name, g.fixtures.len()))).collect()),
                "palettes": names(show.palettes.iter().map(|p| (p.id, format!("{} [{:?}]", p.name, p.kind))).collect()),
                "scenes": names(show.scenes.iter().map(|p| (p.id, p.name.clone())).collect()),
                "cuelists": show.cuelists.iter().take(limit).map(|c| json!({ "id": c.id, "name": c.name, "cues": c.cues.len() })).collect::<Vec<_>>(),
                "chases": names(show.chases.iter().map(|p| (p.id, p.name.clone())).collect()),
                "effects": names(show.effects.iter().map(|p| (p.id, p.name.clone())).collect()),
                "macros": names(show.macros.iter().map(|p| (p.id, p.name.clone())).collect()),
                "timelines": names(show.timelines.iter().map(|p| (p.id, p.name.clone())).collect()),
                "executors": show.executors.len(),
                "vconsole_pages": show.vconsole.len(),
                "stage_objects": show.stage.objects.len(),
                "profiles_in_show": show.profiles.len(),
            })
        }
        "fixtures" => match id {
            Some(i) => {
                let f = show.fixtures.iter().find(|f| f.id as i64 == i).ok_or("No existe ese aparato")?;
                let mut r = fx_row(f, st);
                r["pos"] = json!(f.pos);
                r["rot"] = json!(f.rot);
                r
            }
            None => list(show.fixtures.iter().map(|f| fx_row(f, st)).collect()),
        },
        "groups" => list(show.groups.iter().filter(|g| id.map_or(true, |i| g.id as i64 == i)).map(|g| json!({ "id": g.id, "name": g.name, "fixtures": g.fixtures })).collect()),
        "palettes" => list(
            show.palettes
                .iter()
                .filter(|p| id.map_or(true, |i| p.id as i64 == i))
                .map(|p| json!({ "id": p.id, "name": p.name, "kind": p.kind, "values": attrs_compact(&p.values), "per_fixture": if id.is_some() { per_fixture_compact(&p.per_fixture) } else { json!(p.per_fixture.len()) } }))
                .collect(),
        ),
        "scenes" => list(
            show.scenes
                .iter()
                .filter(|s| id.map_or(true, |i| s.id as i64 == i))
                .map(|s| {
                    let mut r = json!({ "id": s.id, "name": s.name, "fade_in": s.fade_in, "fade_out": s.fade_out, "kind": s.kind, "fixtures": s.values.len() });
                    if id.is_some() {
                        r["values"] = per_fixture_compact(&s.values);
                        r["refs"] = json!(s.refs);
                    }
                    r
                })
                .collect(),
        ),
        "cuelists" => list(
            show.cuelists
                .iter()
                .filter(|c| id.map_or(true, |i| c.id as i64 == i))
                .map(|c| {
                    let detail = id.is_some();
                    json!({ "id": c.id, "name": c.name, "looped": c.looped, "tracking": c.tracking, "auto_mark": c.auto_mark, "priority": c.priority, "cues": c.cues.iter().take(limit).map(|q| cue_row(q, detail)).collect::<Vec<_>>(), "cue_count": c.cues.len() })
                })
                .collect(),
        ),
        "chases" => list(show.chases.iter().filter(|c| id.map_or(true, |i| c.id as i64 == i)).map(|c| json!({ "id": c.id, "name": c.name, "bpm": c.bpm, "dir": c.dir, "speed_master": c.speed_master, "steps": c.steps.len(), "detail": if id.is_some() { serde_json::to_value(&c.steps).unwrap_or_default() } else { Value::Null } })).collect()),
        "speed_masters" => list(show.speed_masters.iter().map(|s| json!({ "id": s.id, "name": s.name, "bpm": s.bpm })).collect()),
        "effects" => list(show.effects.iter().filter(|e| id.map_or(true, |i| e.id as i64 == i)).map(|e| serde_json::to_value(e).unwrap_or_default()).collect()),
        "macros" => list(show.macros.iter().filter(|m| id.map_or(true, |i| m.id as i64 == i)).map(|m| serde_json::to_value(m).unwrap_or_default()).collect()),
        "timelines" => list(show.timelines.iter().filter(|m| id.map_or(true, |i| m.id as i64 == i)).map(|m| serde_json::to_value(m).unwrap_or_default()).collect()),
        "executors" => list(show.executors.iter().map(|e| serde_json::to_value(e).unwrap_or_default()).collect()),
        "universes" => list(show.universes.iter().filter(|u| id.map_or(true, |i| u.id as i64 == i)).map(|u| serde_json::to_value(u).unwrap_or_default()).collect()),
        "settings" => serde_json::to_value(&show.settings).unwrap_or_default(),
        "bindings" => list(show.bindings.iter().map(|b| serde_json::to_value(b).unwrap_or_default()).collect()),
        "vconsole" => list(show.vconsole.iter().map(|p| json!({ "id": p.id, "name": p.name, "cols": p.cols, "rows": p.rows, "widgets": p.widgets.iter().map(|w| json!({ "id": w.id, "kind": w.kind, "label": w.label, "x": w.x, "y": w.y, "w": w.w, "h": w.h, "action": w.action })).collect::<Vec<_>>() })).collect()),
        "scripts" => list(show.scripts.iter().filter(|x| id.map_or(true, |i| x.id as i64 == i)).map(|x| if id.is_some() { serde_json::to_value(x).unwrap_or_default() } else { json!({ "id": x.id, "name": x.name, "blocks": x.blocks.len(), "marks": x.blocks.iter().map(|b| b.marks.len()).sum::<usize>(), "pdf": x.pdf }) }).collect()),
        "locate" => json!({ "profiles": show.locate.iter().map(|(k, v)| json!({ "profile": k, "attrs": attrs_compact(v) })).collect::<Vec<_>>() }),
        "stage" => json!({ "size": show.stage.size, "objects": show.stage.objects.iter().map(|o| json!({ "id": o.id, "kind": o.kind, "name": o.name, "pos": o.pos, "size": o.size })).collect::<Vec<_>>() }),
        other => return Err(format!("Sección desconocida: {other}")),
    };
    Ok(cap(serde_json::to_string(&v).unwrap_or_default(), MAX_RESULT))
}

pub fn get_runtime(st: &AiState, verbose: bool) -> String {
    let rt = &st.rt;
    let prog: Value = Value::Object(rt.programmer.iter().take(40).map(|(k, v)| (k.to_string(), attrs_compact(v))).collect());
    let mut v = json!({
        "grand_master": rt.grand_master, "blackout": rt.blackout, "blind": rt.blind, "fps": rt.fps, "undo_steps": rt.undo, "redo_steps": rt.redo,
        "programmer": { "fixtures": rt.programmer.len(), "values": prog },
        "cuelists": rt.cuelists.iter().filter(|c| c.state != PlayState::Idle || c.current.is_some()).collect::<Vec<_>>(),
        "scenes_active": rt.scenes.iter().filter(|s| s.active || s.level > 0.0).collect::<Vec<_>>(),
        "morph": rt.morph, "chases": rt.chases.iter().filter(|c| c.running).collect::<Vec<_>>(), "effects_running": rt.effects_running,
        "speed_masters": rt.speed, "timelines": rt.timelines.iter().filter(|t| t.running).collect::<Vec<_>>(),
        "outputs": rt.outputs, "inputs": rt.inputs, "timecode": rt.timecode.text, "timecode_running": rt.timecode.running,
        "parked": rt.parked, "testing": rt.testing, "identifying": rt.identifying, "scripts": rt.scripts, "macros_running": rt.macros_running, "learn": rt.learn,
        "last_commands": rt.cmd_log.iter().rev().take(6).collect::<Vec<_>>(),
    });
    if verbose {
        v["executors"] = json!(rt.executors);
        v["vc"] = json!(rt.vc);
    }
    cap(serde_json::to_string(&v).unwrap_or_default(), MAX_RESULT)
}

pub fn get_fixture_state(st: &AiState, input: &Value) -> Result<String, String> {
    let ids: Vec<u32> = input.get("ids").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64()).map(|x| x as u32).collect()).unwrap_or_default();
    if ids.is_empty() {
        return Err("Indica `ids` (lista de números de aparato).".into());
    }
    let mut out = Vec::new();
    for id in ids.into_iter().take(20) {
        let Some(f) = st.show.fixtures.iter().find(|f| f.id == id) else {
            out.push(json!({ "id": id, "error": "no existe" }));
            continue;
        };
        let (prof, mode) = st.profile_mode(&f.profile, &f.mode);
        let foot = mode.map(|m| crate::library::mode_footprint(m)).unwrap_or(0) as usize;
        let frame = st.frames.iter().find(|(u, _)| *u == f.universe).map(|(_, d)| d.as_slice());
        let from = (f.address as usize).saturating_sub(1);
        let bytes: Vec<u8> = frame.map(|d| d[from.min(d.len())..(from + foot).min(d.len())].to_vec()).unwrap_or_default();
        let uni = st.show.universes.iter().find(|u| u.id == f.universe);
        let mut notes: Vec<String> = Vec::new();
        if prof.is_none() {
            notes.push(format!("Perfil «{}» no encontrado en la librería.", f.profile));
        } else if mode.is_none() {
            notes.push(format!("El perfil no tiene el modo «{}».", f.mode));
        }
        if st.rt.blackout {
            notes.push("BLACKOUT activo: todo está a oscuras.".into());
        }
        if st.rt.grand_master < 0.05 {
            notes.push(format!("Master general al {:.0}%.", st.rt.grand_master * 100.0));
        }
        if st.rt.blind && st.rt.programmer.contains_key(&id) {
            notes.push("Modo BLIND: los valores del programador NO salen.".into());
        }
        match uni {
            None => notes.push(format!("El universo {} no existe en el show.", f.universe)),
            Some(u) if u.outputs.iter().all(|o| !o.enabled) => notes.push(format!("El universo {} no tiene ninguna salida activa (Ajustes → Universos).", f.universe)),
            _ => {}
        }
        for o in st.rt.outputs.iter().filter(|o| o.universe == f.universe && o.state == OutState::Error) {
            notes.push(format!("Salida {} del universo {} con error: {}", o.index, f.universe, o.message));
        }
        let parked: Vec<_> = st.rt.parked.iter().filter(|p| p.universe == f.universe && (p.channel as usize) >= f.address as usize && (p.channel as usize) < f.address as usize + foot).collect();
        if !parked.is_empty() {
            notes.push(format!("Hay {} canal(es) aparcado(s) que fijan su valor.", parked.len()));
        }
        if st.rt.testing {
            notes.push("Test de salida activo: sustituye a la salida normal.".into());
        }
        let others: Vec<u32> = st.show.fixtures.iter().filter(|g| g.id != f.id && g.universe == f.universe && (g.address as usize) < f.address as usize + foot && (f.address as usize) < g.address as usize + st.profile_mode(&g.profile, &g.mode).1.map(|m| crate::library::mode_footprint(m) as usize).unwrap_or(1)).map(|g| g.id).collect();
        if !others.is_empty() {
            notes.push(format!("Direcciones solapadas con los aparatos {others:?}."));
        }
        if !bytes.is_empty() && bytes.iter().all(|b| *b == 0) {
            notes.push("Todos sus canales DMX salen a 0.".into());
        }
        let channels: Vec<Value> = mode
            .map(|m| m.channels.iter().map(|c| json!({ "attr": c.attr, "ch": f.address as usize + c.offset as usize, "value": bytes.get(c.offset as usize).copied() })).collect())
            .unwrap_or_default();
        out.push(json!({
            "id": id, "name": f.name, "profile": f.profile, "mode": f.mode, "universe": f.universe, "address": f.address, "channels_used": foot,
            "programmer": st.rt.programmer.get(&id).map(attrs_compact), "resolved": st.merged.get(&id).map(attrs_compact), "dmx": channels, "notes": notes,
        }));
    }
    Ok(cap(serde_json::to_string(&out).unwrap_or_default(), MAX_RESULT))
}

pub fn get_dmx(st: &AiState, input: &Value) -> Result<String, String> {
    let u = num(input, "universe").ok_or("Falta `universe`")? as u16;
    let from = num(input, "from").unwrap_or(1).clamp(1, 512) as usize;
    let to = num(input, "to").unwrap_or(from as i64 + 63).clamp(from as i64, 512) as usize;
    let frame = st.frames.iter().find(|(x, _)| *x == u).map(|(_, d)| d.clone()).ok_or(format!("El universo {u} no existe o no tiene datos"))?;
    let mut owners: BTreeMap<usize, String> = BTreeMap::new();
    for f in st.show.fixtures.iter().filter(|f| f.universe == u) {
        if let (_, Some(m)) = st.profile_mode(&f.profile, &f.mode) {
            for c in &m.channels {
                owners.insert(f.address as usize + c.offset as usize, format!("{}#{}:{}", f.name, f.id, c.attr));
            }
        }
    }
    let rows: Vec<Value> = (from..=to).filter(|ch| frame.get(ch - 1).copied().unwrap_or(0) != 0).map(|ch| json!({ "ch": ch, "v": frame[ch - 1], "by": owners.get(&ch) })).collect();
    Ok(cap(json!({ "universe": u, "range": [from, to], "nonzero": rows.len(), "channels": rows }).to_string(), MAX_RESULT))
}

pub fn search_library(st: &AiState, input: &Value) -> String {
    let q = input.get("query").and_then(|q| q.as_str()).unwrap_or("").to_lowercase();
    let limit = num(input, "limit").unwrap_or(15).clamp(1, 50) as usize;
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut scored: Vec<(usize, &FixtureProfile)> = st
        .profiles
        .iter()
        .filter_map(|p| {
            let hay = format!("{} {} {} {}", p.id, p.manufacturer, p.model, p.category).to_lowercase();
            let hits = words.iter().filter(|w| hay.contains(**w)).count();
            if words.is_empty() || hits == words.len() {
                Some((hits * 10 + if p.model.to_lowercase() == q { 5 } else { 0 }, p))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
    let total = scored.len();
    let rows: Vec<Value> = scored
        .into_iter()
        .take(limit)
        .map(|(_, p)| json!({ "id": p.id, "manufacturer": p.manufacturer, "model": p.model, "category": p.category, "source": p.source, "modes": p.modes.iter().map(|m| json!({ "name": m.name, "channels": crate::library::mode_footprint(m) })).collect::<Vec<_>>() }))
        .collect();
    cap(json!({ "total": total, "shown": rows.len(), "profiles": rows }).to_string(), MAX_RESULT)
}

pub fn get_profile(st: &AiState, input: &Value) -> Result<String, String> {
    let id = input.get("id").and_then(|i| i.as_str()).ok_or("Falta `id`")?;
    let p = st.profiles.iter().find(|p| p.id == id).ok_or(format!("Perfil «{id}» no encontrado (usa search_library)"))?;
    let modes: Vec<Value> = p
        .modes
        .iter()
        .map(|m| {
            json!({ "name": m.name, "channels": crate::library::mode_footprint(m), "map": m.channels.iter().map(|c| json!({ "attr": c.attr, "offset": c.offset, "fine": c.fine, "default": c.default, "caps": c.capabilities.iter().take(24).map(|k| format!("{}-{} {}", k.min, k.max, k.label)).collect::<Vec<_>>() })).collect::<Vec<_>>() })
        })
        .collect();
    Ok(cap(json!({ "id": p.id, "manufacturer": p.manufacturer, "model": p.model, "category": p.category, "beam_angle": p.beam_angle, "pan_range": p.pan_range, "tilt_range": p.tilt_range, "power_w": p.power_w, "modes": modes }).to_string(), MAX_RESULT))
}

/// Execute a read/command tool. `ask_user` and `wait` are handled by the agent loop.
pub async fn execute(h: &Handle, name: &str, input: &Value) -> Result<String, String> {
    match name {
        "console_command" => {
            let text = input.get("text").and_then(|t| t.as_str()).ok_or("Falta `text`")?.to_string();
            let n = text.split([';', '\n']).filter(|s| !s.trim().is_empty() && !s.trim().starts_with('#')).count().clamp(1, 30);
            h.command(Command::CommandLine { text }).await?;
            let st = h.ai_state().await.ok_or("Motor detenido")?;
            let lines: Vec<String> = st.rt.cmd_log.iter().rev().take(n).rev().map(|l| format!("{} {} → {}", if l.ok { "OK " } else { "ERR" }, l.text, l.msg)).collect();
            if lines.iter().any(|l| l.starts_with("ERR")) {
                Err(cap(lines.join("\n"), MAX_RESULT))
            } else {
                Ok(cap(lines.join("\n"), MAX_RESULT))
            }
        }
        "send_command" => {
            let raw = input.get("command").cloned().ok_or("Falta `command`")?;
            let ty = raw.get("type").and_then(|t| t.as_str()).unwrap_or("").to_string();
            if ty.starts_with("ai_") {
                return Err("Ese comando no está permitido para el asistente.".into());
            }
            let cmd: Command = serde_json::from_value(raw).map_err(|e| format!("Comando no válido: {e}. Revisa los campos del tipo `{ty}`."))?;
            h.command(cmd).await?;
            Ok(format!("OK ({ty}). Usa get_show/get_runtime para ver el resultado."))
        }
        "get_show" | "get_runtime" | "get_fixture_state" | "get_dmx" | "search_library" | "get_profile" => {
            let st = h.ai_state().await.ok_or("Motor detenido")?;
            match name {
                "get_show" => get_show(&st.show, &st, input),
                "get_runtime" => Ok(get_runtime(&st, input.get("verbose").and_then(|v| v.as_bool()).unwrap_or(false))),
                "get_fixture_state" => get_fixture_state(&st, input),
                "get_dmx" => get_dmx(&st, input),
                "search_library" => Ok(search_library(&st, input)),
                _ => get_profile(&st, input),
            }
        }
        "diagnose_outputs" => {
            if input.get("scan").and_then(|v| v.as_bool()).unwrap_or(true) {
                h.command(Command::NodesScan).await?;
                tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
            }
            let st = h.ai_state().await.ok_or("Motor detenido")?;
            let serial: Vec<String> = crate::io::list_serial_ports().into_iter().map(|(n, _)| n).collect();
            let u = num(input, "universe").map(|u| u as u16);
            Ok(cap(super::netdiag::analyze(&st, &serial, u).to_string(), MAX_RESULT))
        }
        "get_help" => Ok(super::help::lookup(input.get("topic").and_then(|t| t.as_str()).unwrap_or(""))),
        other => Err(format!("Herramienta desconocida: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_are_cleaned() {
        for (raw, want) in [("get_show<|channel|>", "get_show"), ("functions.get_runtime", "get_runtime"), (" `Console_Command` ", "console_command"), ("get_show<|channel|>commentary", "get_show"), ("diagnose_outputs\n", "diagnose_outputs")] {
            assert_eq!(clean_tool_name(raw), want, "{raw}");
        }
        assert_eq!(clean_tool_name("nope<|x|>"), "nope");
    }

    #[test]
    fn catalog_covers_every_command_variant() {
        let cat = command_catalog();
        let names: Vec<&str> = cat.iter().map(|(n, _)| n.as_str()).collect();
        for must in ["prog_set", "scene_record", "go", "fixture_add", "cue_save", "universe_save", "undo", "ai_send", "subscribe_dmx", "macro_save", "timeline_run", "backup_restore", "ai_test_provider"] {
            assert!(names.contains(&must), "missing {must} in {names:?}");
        }
        let go = cat.iter().find(|(n, _)| n == "go").unwrap();
        assert_eq!(go.1, "{ cuelist: u32 }");
        let rec = cat.iter().find(|(n, _)| n == "prog_record_cue").unwrap();
        assert!(rec.1.contains("?refs: bool"), "serde(default) fields are marked optional: {}", rec.1);
        assert!(cat.len() > 100);
    }

    #[test]
    fn description_lists_types_and_is_bounded() {
        let d = send_command_description();
        assert!(d.contains("- scene_go { id: u32, on: bool }"));
        assert!(d.contains("Cue {number: f64"));
        assert!(!d.contains("- ai_send"), "assistant commands are hidden from the model");
        assert!(d.len() < 24_000, "description too large: {}", d.len());
    }

    #[test]
    fn tool_schemas_are_portable() {
        for t in definitions() {
            let p = t.schema["properties"].as_object().unwrap();
            assert!(!p.is_empty(), "{} needs at least one property (Gemini rejects empty objects)", t.name);
            let s = t.schema.to_string();
            for bad in ["oneOf", "anyOf", "$ref", "additionalProperties", "allOf"] {
                assert!(!s.contains(bad), "{} uses {bad}", t.name);
            }
        }
        assert_eq!(definitions().len(), 12);
    }

    #[test]
    fn risk_table() {
        let c = |v: Value| serde_json::from_value::<Command>(v).unwrap();
        use AiRisk::*;
        let t: Vec<(Value, bool, AiRisk)> = vec![
            (json!({"type":"go","cuelist":1}), false, Live),
            (json!({"type":"grand_master","level":0.5}), false, Live),
            (json!({"type":"blackout","on":true}), true, Live),
            (json!({"type":"prog_set","fixtures":[1],"attrs":{"dimmer":1.0}}), false, Live),
            (json!({"type":"prog_set","fixtures":[1],"attrs":{"dimmer":1.0}}), true, Edit),
            (json!({"type":"scene_record","id":null,"name":"x","from_output":false,"fixtures":null,"merge":false}), false, Edit),
            (json!({"type":"palette_save","palette":{"id":0,"name":"p","kind":"color"}}), false, Edit),
            (json!({"type":"fixture_remove","ids":[1]}), false, Live),
            (json!({"type":"cuelist_remove","id":1}), false, Live),
            (json!({"type":"universe_save","universe":{"id":1,"name":"u"}}), false, Live),
            (json!({"type":"show_load","name":"x"}), false, Live),
            (json!({"type":"backup_restore","id":"x"}), false, Live),
            (json!({"type":"dmx_set","universe":1,"channel":1,"value":5}), false, Live),
            (json!({"type":"output_test","mode":{"type":"off"}}), false, Live),
            (json!({"type":"undo"}), false, Edit),
            (json!({"type":"blind_set","on":true}), false, Edit),
            (json!({"type":"macro_save","macro":{"id":0,"name":"m","steps":[]}}), false, Edit),
            (json!({"type":"library_remove","id":"x"}), false, Live),
            (json!({"type":"midi_enable","output":false,"name":"x","enabled":true}), false, Live),
        ];
        for (v, blind, want) in t {
            let cmd = c(v.clone());
            assert_eq!(command_risk(&cmd, blind), want, "{v} blind={blind}");
        }
    }

    #[test]
    fn console_risk_table() {
        use AiRisk::*;
        let cases = [
            ("go", false, Live),
            ("fixture 1 thru 4 at 50", false, Live),
            ("fixture 1 thru 4 at 50", true, Edit),
            ("fixture 1 thru 4", false, Edit),
            ("record cue 5 name Hola", false, Edit),
            ("record scene 3 Amanecer", false, Edit),
            ("delete cue 3", false, Live),
            ("scene 2 on", false, Live),
            ("blackout on", true, Live),
            ("gm 50", false, Live),
            ("blind on", false, Edit),
            ("fixture 1 at 50; go", true, Live),
            ("# comentario", false, Edit),
            ("frobnicate", false, Live),
            ("rojo at 100", true, Edit), // a known attribute alias, blind
            ("rojo at 100", false, Live),
        ];
        for (t, blind, want) in cases {
            assert_eq!(console_risk(t, blind), want, "{t} blind={blind}");
        }
    }

    #[test]
    fn summaries_are_spanish_one_liners() {
        assert_eq!(summary("console_command", &json!({"text":"go"})), "Consola: «go»");
        let s = summary("send_command", &json!({"command":{"type":"scene_save","scene":{"name":"Amanecer"}}}));
        assert_eq!(s, "Guardar escena «Amanecer»");
        let s = summary("send_command", &json!({"command":{"type":"fixture_add","profile":"generic/rgb-par","count":4}}));
        assert!(s.contains("4 × generic/rgb-par"));
        assert!(summary("send_command", &json!({"command":{"type":"zzz"}})).contains("zzz"));
    }

    #[test]
    fn truncation_note() {
        let s = cap("é".repeat(30_000), 100);
        assert!(s.contains("truncado") && s.len() < 400);
    }
}
