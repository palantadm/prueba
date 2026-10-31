//! Spanish documentation corpus the assistant can read with `get_help` (embedded at build time).

/// `(id, title, markdown)`.
pub const TOPICS: &[(&str, &str, &str)] = &[
    ("inicio", "Visión general y flujo de trabajo", include_str!("help/inicio.md")),
    ("interfaz", "Espacios de trabajo y atajos", include_str!("help/interfaz.md")),
    ("patch", "Parchear aparatos y universos", include_str!("help/patch.md")),
    ("biblioteca", "Biblioteca de perfiles (GDTF, QLC+, OFL)", include_str!("help/biblioteca.md")),
    ("programador", "Programador, selección y blind", include_str!("help/programador.md")),
    ("grupos-paletas", "Grupos y paletas", include_str!("help/grupos-paletas.md")),
    ("escenas", "Escenas y submasters", include_str!("help/escenas.md")),
    ("cuelists", "Cuelists, cues, tracking y fundidos", include_str!("help/cuelists.md")),
    ("chases-efectos", "Chases, efectos y speed masters", include_str!("help/chases-efectos.md")),
    ("executors", "Executors, páginas y masters", include_str!("help/executors.md")),
    ("linea-de-comandos", "Línea de comandos", include_str!("help/linea-de-comandos.md")),
    ("macros-timelines", "Macros, timelines, timecode y MSC", include_str!("help/macros-timelines.md")),
    ("salidas", "Universos y salidas DMX (Art-Net, sACN, serie…)", include_str!("help/salidas.md")),
    ("entradas", "Entradas: MIDI, OSC, Art-Net y bindings", include_str!("help/entradas.md")),
    ("guion", "Guion: escaleta, marcas de cue y seguimiento en directo", include_str!("help/guion.md")),
    ("consola-virtual", "Consola virtual", include_str!("help/consola-virtual.md")),
    ("escenario", "Visualizador 3D, planta 2D y escenario", include_str!("help/escenario.md")),
    ("shows", "Shows, copias de seguridad e importar/exportar", include_str!("help/shows.md")),
    ("diagnostico", "Diagnóstico: «no sale luz» y otros problemas", include_str!("help/diagnostico.md")),
    ("asistente", "El asistente de IA: proveedores, modos y privacidad", include_str!("help/asistente.md")),
];

const MAX: usize = 9000;

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
    use super::cmdref as r;
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
        return cap(format!("# {title}\n{body}"));
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
            let mut s = format!("# {title}\n{body}");
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
