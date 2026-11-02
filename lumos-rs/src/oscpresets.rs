//! Ready-made OSC controls for other programs (Resolume, QLab, Ableton Live…), so nobody types an OSC path by hand.
//!
//! The built-in catalog lives here; the user can add programs of their own (`Show.osc_software`). Both use the same
//! types, and `resolve` turns "program + action + params" into the concrete address and arguments to send.

use crate::model::*;
use std::collections::BTreeMap;

fn p(key: &str, label: &str, default: &str) -> OscParam {
    OscParam { key: key.into(), label: label.into(), default: default.into() }
}

struct B {
    id: &'static str,
    label: &'static str,
    group: &'static str,
    address: &'static str,
    args: &'static [&'static str],
    value: OscValue,
    kind: OscKind,
    params: Vec<OscParam>,
    min: f32,
    max: f32,
    note: &'static str,
}

fn a(id: &'static str, label: &'static str, group: &'static str, address: &'static str, kind: OscKind, value: OscValue, params: Vec<OscParam>) -> B {
    B { id, label, group, address, args: &[], value, kind, params, min: 0.0, max: 1.0, note: "" }
}

impl B {
    fn args(mut self, args: &'static [&'static str]) -> Self {
        self.args = args;
        self
    }
    fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }
    fn note(mut self, n: &'static str) -> Self {
        self.note = n;
        self
    }
    fn build(self) -> OscAction {
        OscAction {
            id: self.id.into(),
            label: self.label.into(),
            group: self.group.into(),
            address: self.address.into(),
            args: self.args.iter().map(|s| s.to_string()).collect(),
            value: self.value,
            kind: self.kind,
            params: self.params,
            min: self.min,
            max: self.max,
            note: self.note.into(),
        }
    }
}

fn sw(id: &str, name: &str, port: u16, note: &str, actions: Vec<B>) -> OscSoftware {
    OscSoftware { id: id.into(), name: name.into(), port, note: note.into(), actions: actions.into_iter().map(B::build).collect(), builtin: true }
}

use OscKind::{Fader, Toggle, Trigger};
use OscValue::{Float, Int, None as NoVal};

/// The catalog shipped with Lumos.
pub fn builtin() -> Vec<OscSoftware> {
    let layer = || p("layer", "Capa", "1");
    let clip = || p("clip", "Clip", "1");
    vec![
        sw(
            "resolume",
            "Resolume Arena / Avenue",
            7000,
            "En Resolume: Preferencias → OSC → activa «OSC Input» y usa este puerto (por defecto 7000). Capas, clips y columnas se cuentan desde 1. Los faders envían 0–1.",
            vec![
                a("clip_connect", "Lanzar clip", "Clips", "/composition/layers/{layer}/clips/{clip}/connect", Trigger, Int, vec![layer(), clip()]),
                a("column_connect", "Lanzar columna", "Clips", "/composition/columns/{column}/connect", Trigger, Int, vec![p("column", "Columna", "1")]),
                a("layer_clear", "Vaciar capa", "Capas", "/composition/layers/{layer}/clear", Trigger, Int, vec![layer()]),
                a("disconnect_all", "Desconectar todo", "Composición", "/composition/disconnectall", Trigger, Int, vec![]),
                a("layer_opacity", "Opacidad de capa", "Capas", "/composition/layers/{layer}/video/opacity", Fader, Float, vec![layer()]),
                a("layer_volume", "Volumen de capa", "Capas", "/composition/layers/{layer}/audio/volume", Fader, Float, vec![layer()]),
                a("layer_bypass", "Capa oculta (bypass)", "Capas", "/composition/layers/{layer}/bypassed", Toggle, Int, vec![layer()]),
                a("layer_solo", "Solo de capa", "Capas", "/composition/layers/{layer}/solo", Toggle, Int, vec![layer()]),
                a("clip_position", "Posición del clip", "Clips", "/composition/layers/{layer}/clips/{clip}/transport/position", Fader, Float, vec![layer(), clip()]),
                a("master_opacity", "Master de vídeo", "Composición", "/composition/video/opacity", Fader, Float, vec![]),
                a("master_volume", "Master de audio", "Composición", "/composition/audio/volume", Fader, Float, vec![]),
                a("crossfader", "Crossfader", "Composición", "/composition/crossfader/phase", Fader, Float, vec![]),
                a("deck_select", "Elegir deck", "Composición", "/composition/decks/{deck}/select", Trigger, Int, vec![p("deck", "Deck", "1")]),
                a("tempo_tap", "Tap tempo", "Tempo", "/composition/tempocontroller/tempotap", Trigger, Int, vec![]),
                a("tempo_resync", "Resincronizar tempo", "Tempo", "/composition/tempocontroller/resync", Trigger, Int, vec![]),
            ],
        ),
        sw(
            "qlab",
            "QLab",
            53000,
            "En QLab: Ajustes del workspace → Network → activa OSC (puerto 53000). El número de cue es el que ves en QLab (puede llevar letras).",
            vec![
                a("go", "GO", "Transporte", "/go", Trigger, NoVal, vec![]),
                a("stop_all", "Parar todo", "Transporte", "/stop", Trigger, NoVal, vec![]),
                a("pause_all", "Pausar todo", "Transporte", "/pause", Trigger, NoVal, vec![]),
                a("resume_all", "Reanudar todo", "Transporte", "/resume", Trigger, NoVal, vec![]),
                a("panic", "Panic (cortar todo)", "Transporte", "/panic", Trigger, NoVal, vec![]),
                a("reset", "Volver al inicio", "Transporte", "/reset", Trigger, NoVal, vec![]),
                a("select_next", "Cue siguiente (seleccionar)", "Selección", "/select/next", Trigger, NoVal, vec![]),
                a("select_prev", "Cue anterior (seleccionar)", "Selección", "/select/previous", Trigger, NoVal, vec![]),
                a("cue_start", "Lanzar cue", "Cues", "/cue/{cue}/start", Trigger, NoVal, vec![p("cue", "Número de cue", "1")]),
                a("cue_stop", "Parar cue", "Cues", "/cue/{cue}/stop", Trigger, NoVal, vec![p("cue", "Número de cue", "1")]),
                a("cue_load", "Cargar cue", "Cues", "/cue/{cue}/load", Trigger, NoVal, vec![p("cue", "Número de cue", "1")]),
            ],
        ),
        sw(
            "ableton",
            "Ableton Live (AbletonOSC)",
            11000,
            "Necesita el script AbletonOSC (github.com/ideoforms/AbletonOSC) activado en Live; escucha en el puerto 11000. Pistas, clips y escenas se cuentan desde 0.",
            vec![
                a("play", "Play", "Transporte", "/live/song/start_playing", Trigger, NoVal, vec![]),
                a("continue", "Continuar", "Transporte", "/live/song/continue_playing", Trigger, NoVal, vec![]),
                a("stop", "Stop", "Transporte", "/live/song/stop_playing", Trigger, NoVal, vec![]),
                a("stop_clips", "Parar todos los clips", "Transporte", "/live/song/stop_all_clips", Trigger, NoVal, vec![]),
                a("tap_tempo", "Tap tempo", "Transporte", "/live/song/tap_tempo", Trigger, NoVal, vec![]),
                a("tempo", "Tempo (BPM)", "Transporte", "/live/song/set/tempo", Fader, Float, vec![]).range(60.0, 200.0),
                a("clip_fire", "Lanzar clip", "Clips", "/live/clip/fire", Trigger, NoVal, vec![p("track", "Pista (desde 0)", "0"), p("clip", "Clip (desde 0)", "0")]).args(&["i:{track}", "i:{clip}"]),
                a("clip_stop", "Parar clip", "Clips", "/live/clip/stop", Trigger, NoVal, vec![p("track", "Pista (desde 0)", "0"), p("clip", "Clip (desde 0)", "0")]).args(&["i:{track}", "i:{clip}"]),
                a("scene_fire", "Lanzar escena", "Clips", "/live/scene/fire", Trigger, NoVal, vec![p("scene", "Escena (desde 0)", "0")]).args(&["i:{scene}"]),
                a("track_volume", "Volumen de pista", "Pistas", "/live/track/set/volume", Fader, Float, vec![p("track", "Pista (desde 0)", "0")]).args(&["i:{track}"]).note("0,85 ≈ 0 dB"),
            ],
        ),
        sw(
            "millumin",
            "Millumin",
            5000,
            "En Millumin: Preferencias → OSC. Comprueba las direcciones con la documentación de tu versión de Millumin.",
            vec![
                a("column_launch", "Lanzar columna", "Columnas", "/millumin/action/launchColumn", Trigger, NoVal, vec![p("column", "Columna", "1")]).args(&["i:{column}"]).note("Cuenta desde 1"),
                a("column_next", "Columna siguiente", "Columnas", "/millumin/action/launchNextColumn", Trigger, NoVal, vec![]),
                a("column_prev", "Columna anterior", "Columnas", "/millumin/action/launchPreviousColumn", Trigger, NoVal, vec![]),
            ],
        ),
        sw(
            "companion",
            "Bitfocus Companion",
            12321,
            "En Companion: Ajustes → Protocolos → OSC (puerto 12321). Aprieta los botones de tu Stream Deck virtual.",
            vec![
                a("press_v3", "Pulsar botón (v3)", "Botones", "/location/{page}/{row}/{column}/press", Trigger, NoVal, vec![p("page", "Página", "1"), p("row", "Fila", "0"), p("column", "Columna", "0")]).note("Companion 3 en adelante; filas y columnas desde 0"),
                a("press_v2", "Pulsar botón (v2)", "Botones", "/press/bank/{page}/{button}", Trigger, NoVal, vec![p("page", "Página", "1"), p("button", "Botón", "1")]).note("Companion 2"),
            ],
        ),
        sw(
            "isadora",
            "Isadora",
            1234,
            "En Isadora: Preferencias → Communications → OSC. Cada canal es una entrada «OSC Listener».",
            vec![a("channel", "Valor al canal", "Canales", "/isadora/{channel}", Fader, Float, vec![p("channel", "Canal", "1")])],
        ),
    ]
}

/// The built-in catalog followed by the user's own programs (a user program never replaces a built-in id).
pub fn all(user: &[OscSoftware]) -> Vec<OscSoftware> {
    let mut v = builtin();
    for u in user {
        if v.iter().all(|b| b.id != u.id) {
            let mut u = u.clone();
            u.builtin = false;
            v.push(u);
        }
    }
    v
}

pub fn find<'a>(all: &'a [OscSoftware], action: &str) -> Result<(&'a OscSoftware, &'a OscAction), String> {
    let (s, a) = action.split_once('/').ok_or_else(|| format!("«{action}» debe ser «programa/acción», p. ej. resolume/clip_connect"))?;
    let sw = all.iter().find(|x| x.id == s).ok_or_else(|| format!("No hay un programa «{s}». Disponibles: {}", all.iter().map(|x| x.id.as_str()).collect::<Vec<_>>().join(", ")))?;
    let ac = sw.actions.iter().find(|x| x.id == a).ok_or_else(|| format!("«{s}» no tiene la acción «{a}». Disponibles: {}", sw.actions.iter().map(|x| x.id.as_str()).collect::<Vec<_>>().join(", ")))?;
    Ok((sw, ac))
}

fn param_ok(v: &str) -> bool {
    !v.is_empty() && v.len() <= 40 && v.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
}

fn fill(text: &str, action: &OscAction, given: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('}') else { return Err(format!("Falta «}}» en «{text}»")) };
        let key = &rest[i + 1..i + j];
        let def = action.params.iter().find(|p| p.key == key).ok_or_else(|| format!("«{{{key}}}» no es un parámetro de la acción «{}»", action.id))?;
        let v = given.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()).unwrap_or(def.default.as_str());
        if !param_ok(v) {
            return Err(format!("Valor no válido para «{}»: «{v}» (solo letras, números, espacio, punto, guion)", def.label));
        }
        out.push_str(v);
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Concrete `(address, fixed args)` for an action with the user's parameters.
pub fn resolve(action: &OscAction, params: &BTreeMap<String, String>) -> Result<(String, Vec<String>), String> {
    for k in params.keys() {
        if action.params.iter().all(|p| &p.key != k) {
            return Err(format!("La acción «{}» no tiene el parámetro «{k}»; tiene: {}", action.id, if action.params.is_empty() { "ninguno".into() } else { action.params.iter().map(|p| p.key.as_str()).collect::<Vec<_>>().join(", ") }));
        }
    }
    let address = fill(&action.address, action, params)?;
    if address.contains(' ') {
        return Err("Una dirección OSC no puede llevar espacios".into());
    }
    let args = action.args.iter().map(|x| fill(x, action, params)).collect::<Result<Vec<_>, _>>()?;
    Ok((address, args))
}

/// Check a user-written program before saving it.
pub fn validate(s: &OscSoftware) -> Result<(), String> {
    let id_ok = !s.id.is_empty() && s.id.len() <= 32 && s.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !id_ok {
        return Err("El id del programa: minúsculas, números, _ y -".into());
    }
    if builtin().iter().any(|b| b.id == s.id) {
        return Err(format!("«{}» es un programa del catálogo de Lumos; elige otro id", s.id));
    }
    if s.name.trim().is_empty() {
        return Err("El programa necesita un nombre".into());
    }
    if s.port == 0 {
        return Err("El puerto no puede ser 0".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for a in &s.actions {
        let ok = !a.id.is_empty() && a.id.len() <= 40 && a.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
        if !ok || !seen.insert(a.id.clone()) {
            return Err(format!("Acción «{}»: id vacío, repetido o con caracteres no válidos", a.id));
        }
        if !a.address.starts_with('/') {
            return Err(format!("Acción «{}»: la dirección OSC empieza por «/»", a.id));
        }
        if !(a.min.is_finite() && a.max.is_finite()) {
            return Err(format!("Acción «{}»: rango no válido", a.id));
        }
        // every placeholder must be declared
        resolve(a, &BTreeMap::new()).map_err(|e| format!("Acción «{}»: {e}", a.id))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        let all = builtin();
        let mut ids = std::collections::BTreeSet::new();
        for s in &all {
            assert!(ids.insert(s.id.clone()), "duplicate software {}", s.id);
            assert!(s.port > 0 && !s.actions.is_empty() && s.builtin);
            let mut aids = std::collections::BTreeSet::new();
            for a in &s.actions {
                assert!(aids.insert(a.id.clone()), "duplicate action {}/{}", s.id, a.id);
                assert!(a.address.starts_with('/'), "{}/{}", s.id, a.id);
                // every placeholder is declared and every default resolves to a clean address
                let (addr, args) = resolve(a, &BTreeMap::new()).unwrap_or_else(|e| panic!("{}/{}: {e}", s.id, a.id));
                assert!(!addr.contains('{') && !addr.contains(' '), "{addr}");
                assert!(args.iter().all(|x| !x.contains('{')));
                // every declared param is used
                for p in &a.params {
                    let tag = format!("{{{}}}", p.key);
                    assert!(a.address.contains(&tag) || a.args.iter().any(|x| x.contains(&tag)), "{}/{} param {} unused", s.id, a.id, p.key);
                }
                if a.kind == Fader {
                    assert!(a.value != NoVal, "a fader must send its value: {}/{}", s.id, a.id);
                }
            }
        }
    }

    #[test]
    fn resolume_clip_address_is_filled_in() {
        let all = all(&[]);
        let (_, a) = find(&all, "resolume/clip_connect").unwrap();
        let mut p = BTreeMap::new();
        p.insert("layer".to_string(), "2".to_string());
        p.insert("clip".to_string(), "5".to_string());
        assert_eq!(resolve(a, &p).unwrap().0, "/composition/layers/2/clips/5/connect");
        assert_eq!(resolve(a, &BTreeMap::new()).unwrap().0, "/composition/layers/1/clips/1/connect", "defaults");
        let (_, ab) = find(&all, "ableton/clip_fire").unwrap();
        p.clear();
        p.insert("track".into(), "3".into());
        assert_eq!(resolve(ab, &p).unwrap(), ("/live/clip/fire".to_string(), vec!["i:3".to_string(), "i:0".to_string()]));
    }

    #[test]
    fn bad_input_is_refused_with_a_useful_message() {
        let all = all(&[]);
        assert!(find(&all, "resolume").unwrap_err().contains("programa/acción"));
        assert!(find(&all, "nope/x").unwrap_err().contains("resolume"));
        assert!(find(&all, "resolume/nope").unwrap_err().contains("clip_connect"));
        let (_, a) = find(&all, "resolume/clip_connect").unwrap();
        let mut p = BTreeMap::new();
        p.insert("layer".into(), "1/../x".into());
        assert!(resolve(a, &p).is_err(), "no slashes smuggled into the path");
        p.clear();
        p.insert("colour".into(), "1".into());
        assert!(resolve(a, &p).unwrap_err().contains("layer"));
    }

    #[test]
    fn user_software_is_validated_and_cannot_shadow_the_catalog() {
        let ok = OscSoftware {
            id: "mi-sw".into(),
            name: "Mi programa".into(),
            port: 9000,
            note: String::new(),
            builtin: false,
            actions: vec![OscAction { id: "fade".into(), label: "Fundido".into(), group: String::new(), address: "/mix/{ch}/fade".into(), args: vec![], value: Float, kind: Fader, params: vec![p("ch", "Canal", "1")], min: 0.0, max: 1.0, note: String::new() }],
        };
        validate(&ok).unwrap();
        let mut bad = ok.clone();
        bad.id = "resolume".into();
        assert!(validate(&bad).unwrap_err().contains("catálogo"));
        let mut bad = ok.clone();
        bad.actions[0].params.clear();
        assert!(validate(&bad).unwrap_err().contains("ch"), "undeclared placeholder");
        let mut bad = ok.clone();
        bad.actions[0].address = "mix".into();
        assert!(validate(&bad).is_err());
        assert_eq!(all(&[ok.clone()]).iter().filter(|s| s.id == "mi-sw").count(), 1);
    }
}
