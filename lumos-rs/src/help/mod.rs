//! Spanish documentation corpus, embedded at build time. One source, two readers:
//!
//! * the **user manual** (Help window, `GET /api/help…`) works without any AI;
//! * the **assistant** reads the same pages through its `get_help` tool.
//!
//! Text between `<!--ai-->…<!--/ai-->` is only for the assistant (tool and protocol names); text between
//! `<!--ui-->…<!--/ui-->` is only for people (where to click). Everything else is shared.


/// `(id, title, markdown)`.
pub const TOPICS: &[(&str, &str, &str)] = &[
    ("inicio", "Visión general y flujo de trabajo", include_str!("inicio.md")),
    ("interfaz", "Espacios de trabajo y atajos", include_str!("interfaz.md")),
    ("patch", "Parchear aparatos y universos", include_str!("patch.md")),
    ("biblioteca", "Biblioteca de perfiles (GDTF, QLC+, OFL)", include_str!("biblioteca.md")),
    ("programador", "Programador, selección y blind", include_str!("programador.md")),
    ("grupos-paletas", "Grupos y paletas", include_str!("grupos-paletas.md")),
    ("escenas", "Escenas y submasters", include_str!("escenas.md")),
    ("cuelists", "Cuelists, cues, tracking y fundidos", include_str!("cuelists.md")),
    ("chases-efectos", "Chases, efectos y speed masters", include_str!("chases-efectos.md")),
    ("executors", "Executors, páginas y masters", include_str!("executors.md")),
    ("linea-de-comandos", "Línea de comandos", include_str!("linea-de-comandos.md")),
    ("macros-timelines", "Macros, timelines, timecode y MSC", include_str!("macros-timelines.md")),
    ("salidas", "Universos y salidas DMX (Art-Net, sACN, serie…)", include_str!("salidas.md")),
    ("entradas", "Entradas: MIDI, OSC, Art-Net y bindings", include_str!("entradas.md")),
    ("audio", "Audio: dispositivos, canales de entrada/salida, matriz, sonidos, LTC y audio reactivo", include_str!("audio.md")),
    ("guion", "Guion: escaleta, marcas de cue y seguimiento en directo", include_str!("guion.md")),
    ("consola-virtual", "Consola virtual", include_str!("consola-virtual.md")),
    ("programas-externos", "Programas externos por OSC (Resolume, QLab, Ableton…)", include_str!("programas-externos.md")),
    ("escenario", "Visualizador 3D, planta 2D y escenario", include_str!("escenario.md")),
    ("shows", "Shows, copias de seguridad e importar/exportar", include_str!("shows.md")),
    ("diagnostico", "Diagnóstico: «no sale luz» y otros problemas", include_str!("diagnostico.md")),
    ("asistente", "El asistente de IA: proveedores, modos y privacidad", include_str!("asistente.md")),
];

const MAX: usize = 9000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    Ai,
    Human,
}

/// Keep the blocks meant for `aud`, drop the others, remove the markers.
pub fn render(body: &str, aud: Audience) -> String {
    let (keep, drop) = match aud {
        Audience::Ai => ("ai", "ui"),
        Audience::Human => ("ui", "ai"),
    };
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(i) = rest.find("<!--") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 4..];
        let Some(j) = after.find("-->") else { out.push_str(&rest[i..]); return out };
        let tag = &after[..j];
        let tail = &after[j + 3..];
        if tag == keep {
            rest = tail; // marker only
        } else if tag == drop {
            let end = format!("<!--/{drop}-->");
            match tail.find(&end) {
                Some(k) => {
                    let mut t = &tail[k + end.len()..];
                    // a dropped whole line must not leave an empty one behind
                    if out.ends_with('\n') || out.is_empty() {
                        t = t.strip_prefix('\n').unwrap_or(t);
                    }
                    rest = t;
                }
                None => return out,
            }
        } else if tag == format!("/{keep}") {
            rest = tail;
        } else {
            out.push_str(&rest[i..i + 4 + j + 3]);
            rest = tail;
        }
    }
    out.push_str(rest);
    if out.contains("{{OSC_CATALOG}}") {
        out = out.replace("{{OSC_CATALOG}}", &osc_catalog_md(aud));
    }
    out
}

/// The built-in OSC catalog as a Markdown list (the assistant gets the ids it must send; people get the names).
fn osc_catalog_md(aud: Audience) -> String {
    let mut s = String::new();
    for sw in crate::oscpresets::builtin() {
        let _ = std::fmt::Write::write_fmt(&mut s, format_args!("\n### {} (puerto {})\n{}\n", sw.name, sw.port, sw.note));
        for a in &sw.actions {
            let kind = match a.kind {
                crate::model::OscKind::Trigger => "botón",
                crate::model::OscKind::Toggle => "interruptor",
                crate::model::OscKind::Fader => "fader",
            };
            let params = if a.params.is_empty() { String::new() } else { format!(" — pide: {}", a.params.iter().map(|p| if aud == Audience::Ai { format!("`{}` (def. {})", p.key, p.default) } else { p.label.clone() }).collect::<Vec<_>>().join(", ")) };
            if aud == Audience::Ai {
                let _ = std::fmt::Write::write_fmt(&mut s, format_args!("- `{}/{}` — {} ({kind}){params}\n", sw.id, a.id, a.label));
            } else {
                let _ = std::fmt::Write::write_fmt(&mut s, format_args!("- {} ({kind}){params}\n", a.label));
            }
        }
    }
    s
}

/// Normalised search words of a query (shared by the manual's topic and command search).
pub fn matcher_words(q: &str) -> Vec<String> {
    // a light stemmer: «apagar» finds «apaga», «escenas» finds «escena»
    norm(q)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(|w| if w.chars().count() >= 6 { w.chars().take(w.chars().count() - 2).collect() } else { w.to_string() })
        .collect()
}

/// Topic list `(id, title)`.
pub fn topic_list() -> Vec<(&'static str, &'static str)> {
    TOPICS.iter().map(|(i, t, _)| (*i, *t)).collect()
}

/// A topic for people: `(title, markdown)`.
pub fn human_topic(id: &str) -> Option<(&'static str, String)> {
    TOPICS.iter().find(|(i, _, _)| *i == id).map(|(_, t, b)| (*t, render(b, Audience::Human)))
}

/// Full-text search over the user manual: `(topic id, title, snippet, score)`, best first.
pub fn search_human(q: &str) -> Vec<(&'static str, &'static str, String, usize)> {
    let words = matcher_words(q);
    if words.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    for (id, title, body) in TOPICS {
        let text = render(body, Audience::Human);
        let head = norm(&format!("{id} {title}"));
        let mut score = 0usize;
        for w in &words {
            if head.contains(w.as_str()) {
                score += 6;
            }
            score += norm(&text).matches(w.as_str()).count().min(5);
        }
        if score == 0 {
            continue;
        }
        // snippet: the first line that contains the most query words
        let best = text.lines().max_by_key(|l| { let n = norm(l); words.iter().filter(|w| n.contains(w.as_str())).count() }).unwrap_or("");
        let snippet: String = best.replace("**", "").trim().trim_start_matches(['#', '-', ' ']).chars().take(160).collect();
        out.push((*id, *title, snippet, score));
    }
    out.sort_by(|a, b| b.3.cmp(&a.3));
    out
}

fn norm(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            c => c,
        })
        .collect()
}

fn index() -> String {
    let mut s = String::from("# Documentación de Lumos\nTemas disponibles (llama a get_help con el id):\n");
    for (id, title, _) in TOPICS {
        s.push_str(&format!("- `{id}` — {title}\n"));
    }
    s.push_str("- `comandos` — Función y formato EXACTO de cada comando de send_command, por grupos (`comandos escenas`, `comandos salidas`…) o uno solo (`comando fixture_add`)\n");
    s
}

fn cap(mut s: String) -> String {
    if s.len() > MAX {
        let mut cut = MAX;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push_str("\n…[recortado]");
    }
    s
}

/// `comandos` (index of groups), `comandos <group words>` (all cards of a group) and `comando <type>` (one card).
fn commands_topic(q: &str) -> Option<String> {
    use crate::ai::cmdref as r;
    let rest = q.strip_prefix("comandos").or_else(|| q.strip_prefix("comando")).or_else(|| q.strip_prefix("commands")).or_else(|| q.strip_prefix("command"))?.trim();
    if !(q.starts_with("comando") || q.starts_with("command")) {
        return None;
    }
    if rest.is_empty() {
        let mut s = String::from("# Comandos de send_command\nLlamada: `send_command {\"command\": {\"type\": \"<tipo>\", ...}}`. Pide el grupo con `get_help comandos <grupo>` o un comando con `get_help comando <tipo>`; cada ficha trae qué hace, el JSON exacto y la forma de consola si existe.\n\n");
        s.push_str(&r::index());
        return Some(s);
    }
    let t = rest.replace([' ', '-'], "_");
    if let Some(x) = r::find(&t) {
        return Some(r::card(x));
    }
    let words: Vec<String> = rest.split_whitespace().map(norm).collect();
    for (g, tys) in r::GROUPS {
        let gn = norm(g);
        if words.iter().all(|w| gn.contains(w.as_str())) {
            let mut s = format!("# {g}\n");
            for ty in *tys {
                if let Some(x) = r::find(ty) {
                    s.push_str(&r::card(x));
                    s.push('\n');
                }
            }
            return Some(cap(s));
        }
    }
    let near = r::nearest(&t, 3);
    Some(format!("No hay un comando «{rest}». Los más parecidos:\n{}", near.iter().map(|x| r::card(x)).collect::<Vec<_>>().join("\n")))
}

/// Look a topic up by id, or search all topics by words. Empty → the index.
pub fn lookup(topic: &str) -> String {
    let q = norm(topic.trim());
    if q.is_empty() || q == "indice" || q == "index" {
        return index();
    }
    if let Some(c) = commands_topic(&q) {
        return c;
    }
    let slug = q.replace([' ', '_'], "-");
    if let Some((_, title, body)) = TOPICS.iter().find(|(id, _, _)| *id == slug) {
        return cap(format!("# {title}\n{}", render(body, Audience::Ai)));
    }
    // free-text search: score = distinct query words found (title/id weigh more)
    let words: Vec<&str> = q.split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() >= 3).collect();
    let mut scored: Vec<(usize, usize)> = TOPICS
        .iter()
        .enumerate()
        .map(|(i, (id, title, body))| {
            let head = norm(&format!("{id} {title}"));
            let text = norm(body);
            let score: usize = words.iter().map(|w| if head.contains(w) { 5 } else if text.contains(w) { 1 } else { 0 }).sum();
            (score, i)
        })
        .filter(|(s, _)| *s > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    match scored.first() {
        None => format!("No hay un tema llamado «{topic}».\n\n{}", index()),
        Some(&(_, i)) => {
            let (id, title, body) = TOPICS[i];
            let others: Vec<&str> = scored.iter().skip(1).take(3).map(|&(_, j)| TOPICS[j].0).collect();
            let mut s = format!("# {title}\n{}", render(body, Audience::Ai));
            if !others.is_empty() {
                s.push_str(&format!("\n\nTemas relacionados: {}", others.iter().map(|o| format!("`{o}`")).collect::<Vec<_>>().join(", ")));
            }
            let _ = id;
            cap(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audience_blocks_are_balanced_and_filtered() {
        let b = "común\n<!--ai-->solo IA `get_show`<!--/ai-->\n<!--ui-->solo personas: pulsa Ajustes<!--/ui-->\nfin";
        let ai = render(b, Audience::Ai);
        let ui = render(b, Audience::Human);
        assert!(ai.contains("get_show") && !ai.contains("pulsa Ajustes") && !ai.contains("<!--"), "{ai}");
        assert!(ui.contains("pulsa Ajustes") && !ui.contains("get_show") && !ui.contains("<!--"), "{ui}");
        assert!(!ui.contains("\n\n"), "a dropped line leaves no blank line: {ui:?}");
        for (id, _, body) in TOPICS {
            for tag in ["ai", "ui"] {
                assert_eq!(body.matches(&format!("<!--{tag}-->")).count(), body.matches(&format!("<!--/{tag}-->")).count(), "{id}: unbalanced <!--{tag}-->");
            }
            assert!(!render(body, Audience::Ai).contains("<!--") && !render(body, Audience::Human).contains("<!--"), "{id}");
        }
    }

    /// The user manual must not talk in assistant tool names or protocol command names.
    #[test]
    fn the_user_manual_has_no_tool_or_protocol_jargon() {
        use crate::ai::cmdref::REF;
        let tools = ["get_show", "get_runtime", "get_fixture_state", "get_dmx", "get_profile", "get_help", "search_library", "send_command", "console_command", "ask_user", "check_patch", "diagnose_outputs"];
        for (id, _, body) in TOPICS {
            let t = render(body, Audience::Human);
            for k in tools {
                assert!(!t.contains(k), "{id}: «{k}» in the human manual");
            }
            for r in REF.iter().filter(|r| r.ty.contains('_')) {
                assert!(!t.contains(&format!("`{}", r.ty)), "{id}: protocol command `{}` in the human manual", r.ty);
            }
        }
    }

    #[test]
    fn human_search_finds_pages_without_ai() {
        let r = search_human("Art-Net nodo");
        assert_eq!(r.first().map(|x| x.0), Some("salidas"), "{:?}", r.iter().map(|x| x.0).collect::<Vec<_>>());
        assert!(search_human("gallego whisper voz")[0].0 == "guion");
        assert!(search_human("").is_empty() && search_human("zzzzqqq").is_empty());
        assert!(human_topic("guion").is_some() && human_topic("nope").is_none());
    }

    #[test]
    fn index_lists_every_topic() {
        let i = lookup("");
        for (id, _, _) in TOPICS {
            assert!(i.contains(&format!("`{id}`")));
        }
    }

    #[test]
    fn command_cards_by_type_and_group() {
        assert!(lookup("comando fixture_add").contains("\"type\":\"fixture_add\""));
        assert!(lookup("comandos escenas").contains("scene_go"));
        assert!(lookup("comandos").contains("Grupos, paletas y efectos"));
        assert!(lookup("comando effect_run").contains("effect_toggle"));
        assert!(lookup("").contains("`comandos`"));
    }

    #[test]
    fn exact_and_fuzzy_lookup() {
        assert!(lookup("cuelists").starts_with("# Cuelists"));
        assert!(lookup("Línea de comandos").starts_with("# Línea de comandos"));
        assert!(lookup("no sale luz apagón").contains("Diagnóstico"));
        assert!(lookup("zzzzqq").contains("No hay un tema"));
    }

    #[test]
    fn topics_are_nonempty_and_bounded() {
        for (id, title, body) in TOPICS {
            assert!(body.len() > 300, "{id} too short");
            assert!(body.len() < MAX, "{id} too long: {}", body.len());
            assert!(!title.is_empty());
            assert!(!body.contains("sk-"), "{id} contains something that looks like a key");
        }
    }

    #[test]
    fn command_line_examples_in_help_parse() {
        // Every fenced example line in the command-line topic must at least be non-empty text.
        let body = TOPICS.iter().find(|t| t.0 == "linea-de-comandos").unwrap().2;
        assert!(body.contains("record cue"));
    }
}
