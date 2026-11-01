//! Reference of every `send_command` command for the assistant: what it does, when to use it, the exact JSON, and the
//! console equivalent when there is one. Every example is validated against the real `Command` type by the tests, so the
//! text the model reads can never drift from the protocol.

use serde_json::Value;

pub struct Entry {
    /// `type` tag (snake_case).
    pub ty: &'static str,
    /// Spanish: what it does and when to use it.
    pub what: &'static str,
    /// Exact example (valid JSON; ids are illustrative).
    pub example: &'static str,
    /// Same thing in the console language, when one exists (use `console_command` then; it is shorter).
    pub console: Option<&'static str>,
}

const fn e(ty: &'static str, what: &'static str, example: &'static str, console: Option<&'static str>) -> Entry {
    Entry { ty, what, example, console }
}

pub const GROUPS: &[(&str, &[&str])] = &[
    ("Programador", &["prog_set", "prog_release", "prog_apply_palette", "prog_record_palette", "prog_record_cue", "cue_load", "cue_update", "blind_set", "undo", "redo", "unpark", "dmx_set"]),
    ("Patch y librería", &["profile_save", "fixture_add", "fixture_update", "fixture_remove", "fixture_morph", "library_import_ofl", "library_remove"]),
    ("Grupos, paletas y efectos", &["group_save", "group_remove", "palette_save", "palette_remove", "effect_save", "effect_remove", "effect_toggle"]),
    ("Escenas", &["scene_record", "scene_save", "scene_remove", "scene_go", "scene_solo", "scene_level", "scene_morph", "scenes_release"]),
    ("Cuelists y cues", &["cuelist_save", "cuelist_remove", "cue_save", "cue_remove", "go", "back", "go_to", "pause", "release_cuelist", "cuelist_manual"]),
    ("Chases y velocidad", &["chase_save", "chase_remove", "chase_run", "chase_bpm", "chase_tap", "chase_nudge", "speed_save", "speed_remove", "speed_set", "speed_tap"]),
    ("Executors y masters", &["exec_save", "exec_remove", "exec_level", "exec_go", "exec_back", "exec_flash", "grand_master", "blackout"]),
    ("Macros y timelines", &["macro_save", "macro_remove", "macro_run", "macro_stop", "timeline_save", "timeline_remove", "timeline_run", "timeline_stop", "timeline_seek", "timecode_ctl"]),
    ("Apuntado, píxeles y calendario", &["aim", "pixel_save", "pixel_remove", "pixel_run", "schedule_save", "schedule_remove"]),
    ("Audio", &["audio_configure", "audio_run", "sound_play", "sound_stop"]),
    ("Guion", &["script_save", "script_remove", "script_go", "script_back", "script_goto", "script_fire", "script_follow", "script_reset", "speech_configure", "speech_listen", "speech_model_get", "speech_model_remove"]),
    ("Salidas, red y ajustes", &["universe_save", "universe_remove", "output_test", "nodes_scan", "settings_save", "binding_save", "binding_remove", "midi_enable"]),
    ("Localizar y copias", &["identify", "locate_save", "locate_capture", "backup_create", "backup_restore", "backup_delete"]),
    ("Escenario, consola virtual y shows", &["lock", "show_merge", "stage_save", "vc_save", "vc_press", "vc_value", "vc_xy", "show_new", "show_save", "show_load", "show_rename"]),
];

pub const REF: &[Entry] = &[
    // ── programmer ──
    e("prog_set", "Pone valores (0..1) en el PROGRAMADOR de los aparatos; sale a escena salvo en blind. Para atributos sueltos de aparatos ya elegidos.", r#"{"type":"prog_set","fixtures":[1,2,3],"attrs":{"dimmer":1,"red":1,"green":0,"blue":0}}"#, Some("fixture 1 thru 3 at 100; red at 100; green at 0; blue at 0")),
    e("prog_release", "Libera el programador (todo, unos aparatos o unos atributos).", r#"{"type":"prog_release","fixtures":null,"attrs":null}"#, Some("clear")),
    e("prog_apply_palette", "Aplica una paleta (id de get_show palettes) a unos aparatos en el programador.", r#"{"type":"prog_apply_palette","fixtures":[1,2],"palette":3}"#, Some("fixture 1 thru 2 apply palette 3")),
    e("prog_record_palette", "Graba el programador de unos aparatos como paleta nueva (id null) o existente. kind: intensity|position|color|beam|all.", r#"{"type":"prog_record_palette","id":null,"fixtures":[1,2],"kind":"color","name":"Rojo"}"#, Some("fixture 1 thru 2; record palette color Rojo")),
    e("prog_record_cue", "Graba el programador como cue de una cuelist (number null = siguiente). mode: merge|replace.", r#"{"type":"prog_record_cue","cuelist":1,"number":3,"mode":"merge","label":"Entrada","release":true}"#, Some("record cue 3 cuelist 1 name Entrada")),
    e("cue_load", "Carga los valores de un cue en el programador para editarlos.", r#"{"type":"cue_load","cuelist":1,"number":3}"#, None),
    e("cue_update", "Actualiza un cue con lo que hay en el programador. mode: merge|replace|track_forward.", r#"{"type":"cue_update","cuelist":1,"number":3,"mode":"merge","refs":true}"#, Some("update cue 3 cuelist 1")),
    e("blind_set", "Activa/desactiva blind (el programador no sale a escena).", r#"{"type":"blind_set","on":true}"#, Some("blind on")),
    e("undo", "Deshace el último cambio.", r#"{"type":"undo"}"#, Some("undo")),
    e("redo", "Rehace.", r#"{"type":"redo"}"#, Some("redo")),
    e("unpark", "Libera canales DMX aparcados (todos si ambos son null).", r#"{"type":"unpark","universe":null,"channel":null}"#, Some("unpark")),
    e("dmx_set", "Aparca un canal DMX a un valor 0-255 (ignora todo lo demás) o lo libera con value null.", r#"{"type":"dmx_set","universe":1,"channel":5,"value":255}"#, Some("park 1/5 at 100")),
    // ── patch ──
    e("fixture_add", "Añade aparatos al patch. address null = primeros huecos libres (recomendado). Da siempre name. Antes: search_library + get_profile para profile y mode.", r#"{"type":"fixture_add","profile":"generic/rgb-par","mode":"3ch","count":4,"universe":1,"address":null,"name":"PAR"}"#, None),
    e("fixture_update", "Modifica un aparato: basta `id` + los campos que cambian (name, pos [x,y,z] m, rot grados, universe, address, mode, profile, curve = ley de dimmer linear|square|s_curve|gamma|switched). Lumos completa el resto y rechaza si pisa canales.", r#"{"type":"fixture_update","fixture":{"id":3,"name":"Frente 3","pos":[1.5,5.0,0.5]}}"#, None),
    e("fixture_remove", "Elimina aparatos por id (también de grupos, cues y escenas).", r#"{"type":"fixture_remove","ids":[9,10]}"#, None),
    e("fixture_morph", "Cambia el modelo/modo de aparatos conservando sus valores (previsualiza antes con /api/morph/preview). repack: recoloca direcciones.", r#"{"type":"fixture_morph","ids":[1,2],"profile":"generic/rgbw-par","mode":"4ch","repack":true}"#, None),
    e("profile_save", "Crea o corrige un perfil de aparato escrito a mano (editor de perfiles). id en minúsculas tipo «fabricante/modelo»; modes[].channels[] con attr (dimmer, red, pan…), offset (0 = canal 1), fine (offset del canal fino o null), default 0..1, invert y capabilities para ruedas (min,max,label). No sobrescribe la biblioteca de Lumos y no deja quitar un modo en uso. Después se parchea con fixture_add.", r#"{"type":"profile_save","profile":{"id":"acme/par-led","manufacturer":"Acme","model":"Par LED","category":"par","modes":[{"name":"4ch","channels":[{"attr":"dimmer","offset":0,"fine":null,"default":0,"invert":false,"capabilities":[]},{"attr":"red","offset":1,"fine":null,"default":0,"invert":false,"capabilities":[]},{"attr":"green","offset":2,"fine":null,"default":0,"invert":false,"capabilities":[]},{"attr":"blue","offset":3,"fine":null,"default":0,"invert":false,"capabilities":[]}]}],"beam_angle":[25,25],"pan_range":0,"tilt_range":0,"power_w":60}}"#, None),
    e("library_import_ofl", "Importa un perfil desde el JSON del Open Fixture Library (texto completo).", r#"{"type":"library_import_ofl","json":"{...JSON de OFL...}"}"#, None),
    e("library_remove", "Quita un perfil importado de la biblioteca (no puede estar en uso).", r#"{"type":"library_remove","id":"acme/par-led"}"#, None),
    // ── pools ──
    e("group_save", "Crea (id 0) o reemplaza un grupo de aparatos.", r#"{"type":"group_save","group":{"id":0,"name":"Frontales","fixtures":[1,2,3,4]}}"#, Some("fixture 1 thru 4; record group 5 Frontales")),
    e("group_remove", "Elimina un grupo.", r#"{"type":"group_remove","id":5}"#, Some("delete group 5")),
    e("palette_save", "Crea (id 0) o reemplaza una paleta con valores comunes. kind: intensity|position|color|beam|all.", r#"{"type":"palette_save","palette":{"id":0,"name":"Ámbar","kind":"color","values":{"red":1,"green":0.6,"blue":0}}}"#, None),
    e("palette_remove", "Elimina una paleta.", r#"{"type":"palette_remove","id":3}"#, Some("delete palette 3")),
    e("effect_save", "Crea (id 0) o reemplaza un efecto: onda sobre un atributo. shape: sine|triangle|saw|square|random|circle|eight|ramp|pulse. rate en ciclos/s; depth 0..1.", r#"{"type":"effect_save","effect":{"id":0,"name":"Barrido","attr":"pan","shape":"sine","rate":0.5,"depth":0.5,"phase_spread":0,"fixtures":[1,2,3,4]}}"#, None),
    e("effect_remove", "Elimina un efecto.", r#"{"type":"effect_remove","id":5}"#, Some("delete effect 5")),
    e("effect_toggle", "Arranca/para un efecto guardado.", r#"{"type":"effect_toggle","id":5,"on":true}"#, Some("effect 5 on")),
    // ── scenes ──
    e("scene_record", "Graba una escena desde el programador (from_output false) o desde lo que sale ahora (true). id null = nueva.", r#"{"type":"scene_record","id":null,"name":"Azul","from_output":false,"fixtures":null,"merge":false}"#, Some("record scene 2 Azul")),
    e("scene_save", "Crea (id 0) o reemplaza una escena con valores por aparato.", r#"{"type":"scene_save","scene":{"id":0,"name":"Rojo","values":{"1":{"dimmer":1,"red":1}},"fade_in":1,"fade_out":1}}"#, None),
    e("scene_remove", "Elimina una escena.", r#"{"type":"scene_remove","id":2}"#, Some("delete scene 2")),
    e("scene_go", "Enciende/apaga una escena con sus tiempos de fundido.", r#"{"type":"scene_go","id":2,"on":true}"#, Some("scene 2 on")),
    e("scene_solo", "Enciende una escena y apaga las demás.", r#"{"type":"scene_solo","id":2}"#, Some("scene solo 2")),
    e("scene_level", "Nivel 0..1 de una escena (submaster).", r#"{"type":"scene_level","id":2,"level":0.6}"#, Some("scene 2 at 60")),
    e("scene_morph", "Mezcla dos escenas: t 0..1 (null = automático).", r#"{"type":"scene_morph","a":1,"b":2,"t":0.5}"#, None),
    e("scenes_release", "Apaga todas las escenas.", r#"{"type":"scenes_release"}"#, Some("scenes release")),
    // ── cuelists ──
    e("cuelist_save", "Crea (id 0) o reemplaza una cuelist entera con sus cues. Para un cue suelto usa cue_save. Lee antes con get_show cuelists.", r#"{"type":"cuelist_save","cuelist":{"id":0,"name":"Acto 1","cues":[{"number":1,"label":"Inicio","changes":{"1":{"dimmer":1}}}]}}"#, None),
    e("cuelist_remove", "Elimina una cuelist.", r#"{"type":"cuelist_remove","id":2}"#, Some("delete cuelist 2")),
    e("cue_save", "Crea o reemplaza UN cue (por number) en una cuelist. changes: {id de aparato: {atributo: 0..1}}.", r#"{"type":"cue_save","cuelist":1,"cue":{"number":2,"label":"Fondo","fade_in":3,"changes":{"1":{"dimmer":0.5},"2":{"dimmer":0.5}}}}"#, None),
    e("cue_remove", "Elimina un cue.", r#"{"type":"cue_remove","cuelist":1,"number":2}"#, Some("delete cue 2 cuelist 1")),
    e("go", "GO de una cuelist (siguiente cue).", r#"{"type":"go","cuelist":1}"#, Some("cuelist 1; go")),
    e("back", "ATRÁS en una cuelist.", r#"{"type":"back","cuelist":1}"#, Some("back cuelist 1")),
    e("go_to", "Salta a un cue concreto.", r#"{"type":"go_to","cuelist":1,"number":10}"#, Some("goto cue 10 cuelist 1")),
    e("pause", "Pausa el fundido en curso.", r#"{"type":"pause","cuelist":1}"#, Some("pause cuelist 1")),
    e("release_cuelist", "Libera una cuelist (deja de mandar).", r#"{"type":"release_cuelist","cuelist":1}"#, Some("release cuelist 1")),
    e("cuelist_manual", "Fundido manual: progress 0..1 o null para volver a automático.", r#"{"type":"cuelist_manual","cuelist":1,"progress":0.5}"#, None),
    // ── chases / speed ──
    e("chase_save", "Crea (id 0) o reemplaza un chase: un paso por aparato/escena. dir: forward|backward|bounce|random. hold en pulsos, fade 0..1.", r#"{"type":"chase_save","chase":{"id":0,"name":"Persecución","bpm":120,"dir":"forward","speed_master":null,"steps":[{"values":{"1":{"dimmer":1,"red":1}},"hold":1,"fade":0},{"values":{"2":{"dimmer":1,"red":1}},"hold":1,"fade":0}]}}"#, None),
    e("chase_remove", "Elimina un chase.", r#"{"type":"chase_remove","id":1}"#, Some("delete chase 1")),
    e("chase_run", "Arranca/para un chase.", r#"{"type":"chase_run","id":1,"on":true}"#, Some("chase 1 on")),
    e("chase_bpm", "Pone el tempo de un chase.", r#"{"type":"chase_bpm","id":1,"bpm":128}"#, Some("chase 1 bpm 128")),
    e("chase_tap", "Tap tempo de un chase.", r#"{"type":"chase_tap","id":1}"#, Some("chase 1 tap")),
    e("chase_nudge", "Avanza (+1) o retrocede (-1) un paso.", r#"{"type":"chase_nudge","id":1,"delta":1}"#, Some("chase 1 next")),
    e("speed_save", "Crea (id 0) o reemplaza un master de velocidad (BPM compartido).", r#"{"type":"speed_save","master":{"id":0,"name":"Tempo","bpm":120}}"#, None),
    e("speed_remove", "Elimina un master de velocidad.", r#"{"type":"speed_remove","id":1}"#, None),
    e("speed_set", "Pone los BPM de un master de velocidad.", r#"{"type":"speed_set","id":1,"bpm":126}"#, None),
    e("speed_tap", "Tap tempo de un master de velocidad.", r#"{"type":"speed_tap","id":1}"#, None),
    // ── executors / masters ──
    e("exec_save", "Crea (id 0) o reemplaza un executor (botón+fader) en página/slot. kind.type: empty|cuelist|group|scene|chase|speed|x_fade|effect|master.", r#"{"type":"exec_save","executor":{"id":0,"page":0,"slot":0,"label":"Chase 1","kind":{"type":"chase","chase":1}}}"#, None),
    e("exec_remove", "Quita un executor.", r#"{"type":"exec_remove","id":5}"#, None),
    e("exec_level", "Nivel 0..1 del fader de un executor.", r#"{"type":"exec_level","id":5,"level":0.8}"#, Some("exec 5 at 80")),
    e("exec_go", "GO / pulsar el botón de un executor.", r#"{"type":"exec_go","id":5}"#, Some("exec 5 go")),
    e("exec_back", "ATRÁS de un executor.", r#"{"type":"exec_back","id":5}"#, Some("exec 5 back")),
    e("exec_flash", "Flash de un executor: down true = pulsado, false = soltado.", r#"{"type":"exec_flash","id":5,"down":true}"#, Some("exec 5 flash on")),
    e("grand_master", "Master general 0..1.", r#"{"type":"grand_master","level":0.8}"#, Some("gm 80")),
    e("blackout", "Blackout on/off.", r#"{"type":"blackout","on":true}"#, Some("blackout on")),
    // ── macros / timelines ──
    e("macro_save", "Crea (id 0) o reemplaza una macro: lista de sentencias de consola con esperas en segundos.", r#"{"type":"macro_save","macro":{"id":0,"name":"Apertura","steps":[{"text":"scene 1 on","wait":2},{"text":"go cuelist 1","wait":0}]}}"#, None),
    e("macro_remove", "Elimina una macro.", r#"{"type":"macro_remove","id":3}"#, Some("delete macro 3")),
    e("macro_run", "Ejecuta una macro.", r#"{"type":"macro_run","id":3}"#, Some("macro 3")),
    e("macro_stop", "Para una macro (o todas con id null).", r#"{"type":"macro_stop","id":null}"#, Some("macro stop")),
    e("timeline_save", "Crea (id 0) o reemplaza una timeline: sentencias de consola en segundos.", r#"{"type":"timeline_save","timeline":{"id":0,"name":"Show","length":60,"events":[{"at":0,"text":"scene 1 on"},{"at":30,"text":"go cuelist 1"}]}}"#, None),
    e("timeline_remove", "Elimina una timeline.", r#"{"type":"timeline_remove","id":1}"#, Some("delete timeline 1")),
    e("timeline_run", "Reproduce una timeline (from en segundos o null).", r#"{"type":"timeline_run","id":1,"from":null}"#, Some("timeline 1 run")),
    e("timeline_stop", "Para una timeline.", r#"{"type":"timeline_stop","id":1}"#, Some("timeline 1 stop")),
    e("timeline_seek", "Salta a un segundo de la timeline.", r#"{"type":"timeline_seek","id":1,"t":30}"#, Some("timeline 1 seek 30")),
    e("timecode_ctl", "Controla el timecode interno: running y frame inicial (o null).", r#"{"type":"timecode_ctl","running":true,"frames":null}"#, None),
    // ── script ──
    e("script_save", "Crea (id 0) o reemplaza un guion entero: bloques con marcas de cue y acciones. Lee antes con get_show scripts. kind de bloque: heading|speech|direction|note; kind de marca: light|sound|video|set|other.", r#"{"type":"script_save","script":{"id":0,"name":"Acto 1","blocks":[{"id":1,"kind":"speech","speaker":"ANA","text":"Se apagan las luces.","marks":[{"id":1,"label":"LX 1","kind":"light","at_word":0,"actions":[{"type":"console","text":"go cuelist 1"}]}]}]}}"#, None),
    e("script_remove", "Elimina un guion.", r#"{"type":"script_remove","id":1}"#, None),
    e("script_go", "GO del guion: lanza la siguiente marca y avanza.", r#"{"type":"script_go","id":1}"#, Some("script 1 go")),
    e("script_back", "Atrás en el guion.", r#"{"type":"script_back","id":1}"#, Some("script 1 back")),
    e("script_goto", "Coloca el lector en un bloque (y marca opcional) sin lanzar nada.", r#"{"type":"script_goto","id":1,"block":12,"mark":null}"#, Some("script 1 goto 12")),
    e("script_fire", "Lanza una marca concreta.", r#"{"type":"script_fire","id":1,"block":12,"mark":3}"#, Some("script 1 fire 12 3")),
    e("script_follow", "El guion sigue a los cues lanzados desde cualquier sitio.", r#"{"type":"script_follow","id":1,"on":true}"#, Some("script 1 follow on")),
    e("script_reset", "Vuelve al principio del guion.", r#"{"type":"script_reset","id":1}"#, Some("script 1 reset")),
    e("speech_configure", "Preferencias del seguimiento del guion por VOZ local (Whisper, sin internet): lang (es, gl, ca, en…), model (tiny|base-q5_1|small-q5_1|medium-q5_0|large-v3-turbo-q5_0), custom_model (ruta a un .bin ggml propio, p. ej. un ajuste fino en gallego), fire_cues (la voz también lanza las marcas; solo en frases finales), device (micrófono, \"\" = por defecto), script (id, 0 = el primero).", r#"{"type":"speech_configure","cfg":{"lang":"gl","model":"small-q5_1","custom_model":"","fire_cues":false,"device":"","script":0}}"#, None),
    e("speech_listen", "Empieza/para de escuchar el micrófono y seguir el guion por voz. Necesita un guion y el modelo descargado. Abre el micrófono: acción en directo.", r#"{"type":"speech_listen","on":true}"#, None),
    e("speech_model_get", "Descarga un modelo de voz del catálogo (decenas o cientos de MB); el progreso sale en get_runtime → speech.download.", r#"{"type":"speech_model_get","id":"small-q5_1"}"#, None),
    e("speech_model_remove", "Borra un modelo de voz descargado.", r#"{"type":"speech_model_remove","id":"tiny"}"#, None),
    // ── aim ──
    e("aim", "Apunta cabezas móviles (pan/tilt al programador) a un punto de la sala en metros (x = derecha del escenario, y = arriba, z = hacia el público), a un objeto del escenario (target.type object) o a otro aparato (fixture). height sube el punto (pecho de un actor ≈ 1.4). Mismo cálculo que el 3D/2D.", r#"{"type":"aim","fixtures":[1,2,3],"target":{"type":"point","x":1.5,"y":0,"z":2},"height":1.4}"#, Some("fixture 1 thru 3 aim 1.5 0 2 height 1.4")),
    e("pixel_save", "Crea (id 0) o reemplaza un mapa de píxeles: una fila o rejilla de aparatos RGB pintada con un patrón. fixtures = aparatos en orden de píxel; width_px = píxeles por fila; kind: solid|gradient|rainbow|wave|chase|noise|strobe; color_a/color_b = RGB 0..1; speed = ciclos/s (negativo = al revés); size = repeticiones del patrón; angle = dirección en grados (0 = izquierda→derecha, 90 = arriba→abajo); width/softness solo en chase; level 0..1.", r#"{"type":"pixel_save","map":{"id":0,"name":"Arcoíris","fixtures":[1,2,3,4,5,6,7,8],"width_px":8,"zigzag":false,"kind":"rainbow","color_a":[1,0,0],"color_b":[0,0,1],"speed":0.25,"size":1,"angle":0,"width":0.25,"softness":0,"level":1,"drive_dimmer":true}}"#, None),
    e("pixel_remove", "Elimina un mapa de píxeles.", r#"{"type":"pixel_remove","id":1}"#, None),
    e("pixel_run", "Arranca o para un mapa de píxeles (capa de reproducción: por encima de cuelists y escenas, por debajo del programador).", r#"{"type":"pixel_run","id":1,"on":true}"#, Some("pixel 1 on")),
    e("schedule_save", "Crea (id 0) o reemplaza una regla del calendario: ejecuta una línea de consola a una hora (when.type time {hour,minute}), al amanecer (sunrise {offset_min}) o al atardecer (sunset {offset_min}) en los días elegidos. days = bits Lun=1, Mar=2, Mié=4, Jue=8, Vie=16, Sáb=32, Dom=64 (127 = todos). Amanecer/atardecer necesitan la ubicación en Ajustes (settings.location {lat,lon}).", r#"{"type":"schedule_save","rule":{"id":0,"name":"Luces de entrada","enabled":true,"when":{"type":"sunset","offset_min":-15},"days":127,"action":"scene 4 on"}}"#, None),
    e("schedule_remove", "Elimina una regla del calendario.", r#"{"type":"schedule_remove","id":1}"#, None),
    // ── audio ──
    e("audio_configure", "Configura el audio de esta máquina: dispositivos (\"\" = predeterminado), qué hace cada canal de ENTRADA (role: off|voice|reactive|ltc, gain_db) y la MATRIZ de SALIDA (source: sounds|timeline|ltc, src_ch 1=izq/mono 2=der, out_ch = canal físico, gain_db). routes vacío = sonidos L/R a salidas 1/2. Lee antes get_runtime → audio (dispositivos y canales reales) y reenvía el objeto completo.", r#"{"type":"audio_configure","cfg":{"input_device":"","output_device":"","inputs":[{"channel":1,"label":"Micro actores","role":"voice","gain_db":0},{"channel":3,"label":"LTC","role":"ltc","gain_db":0}],"routes":[{"source":"sounds","src_ch":1,"out_ch":1,"gain_db":0},{"source":"sounds","src_ch":2,"out_ch":2,"gain_db":0},{"source":"ltc","src_ch":1,"out_ch":8,"gain_db":0}],"master_db":0,"autostart":false}}"#, None),
    e("audio_run", "Abre (on true) o cierra los dispositivos de audio de la configuración. Abre el sonido en directo.", r#"{"type":"audio_run","on":true}"#, None),
    e("sound_play", "Reproduce un archivo de audio de la carpeta de medios por el bus «sounds» (volume 0..1, looped). Abre la salida si hace falta. Las marcas del guion, macros y timelines lo usan con la consola.", r#"{"type":"sound_play","file":"gong.wav","volume":0.8,"looped":false}"#, Some("sound \"gong.wav\" at 80")),
    e("sound_stop", "Para un sonido (file) o todos (null), con un fundido corto.", r#"{"type":"sound_stop","file":null}"#, Some("sound stop")),
    // ── data & network ──
    e("backup_create", "Crea una copia de seguridad del show.", r#"{"type":"backup_create","label":"Antes del ensayo"}"#, None),
    e("backup_restore", "Restaura una copia (id de get_show/UI). Destructivo: solo si el usuario lo pide.", r#"{"type":"backup_restore","id":"2026-01-01T10-00-00"}"#, None),
    e("backup_delete", "Elimina una copia.", r#"{"type":"backup_delete","id":"2026-01-01T10-00-00"}"#, None),
    e("nodes_scan", "Busca nodos Art-Net (ArtPoll). diagnose_outputs ya lo hace.", r#"{"type":"nodes_scan"}"#, None),
    e("output_test", "Test de salida: mode.type = off | level {universe,level 0-255} | chase {universe} | channel {universe,channel,level}. SUSTITUYE la salida normal hasta apagarlo.", r#"{"type":"output_test","mode":{"type":"level","universe":1,"level":30}}"#, None),
    e("identify", "Localizar: enciende un aparato para verlo (dimmer, color, lámpara…). on false lo apaga.", r#"{"type":"identify","fixture":3,"on":true}"#, Some("fixture 3 locate")),
    e("locate_save", "Ajusta qué valores usa «Localizar» para un perfil (attrs null = valores por defecto).", r#"{"type":"locate_save","profile":"acme/mover","attrs":{"dimmer":1,"white":1,"shutter":0.5}}"#, None),
    e("locate_capture", "Toma los valores actuales de un aparato como ajuste de Localizar de su perfil.", r#"{"type":"locate_capture","fixture":3}"#, None),
    // ── I/O ──
    e("universe_save", "Crea o reemplaza un universo y TODAS sus salidas (lee antes get_show universes y reenvía las que no cambian). kind.type: art_net {host,port,net,subnet,universe} | sacn {universe,priority,unicast} | osc | serial {driver,port} | usb.", r#"{"type":"universe_save","universe":{"id":1,"name":"Frente","outputs":[{"enabled":true,"kind":{"type":"art_net","host":"192.168.1.50","port":6454,"net":0,"subnet":0,"universe":0}}]}}"#, None),
    e("universe_remove", "Elimina un universo (los aparatos que lo usan quedarán sin salida).", r#"{"type":"universe_remove","id":2}"#, None),
    e("settings_save", "Guarda los ajustes del show (nombre de nodo, tick, OSC/Art-Net in, timecode, MSC…). Lee get_show settings y reenvía el objeto COMPLETO cambiando solo lo necesario.", r#"{"type":"settings_save","settings":{"node_name":"Lumos","tick_hz":44,"osc_in":{"enabled":false,"port":8000},"artnet_in":{"enabled":false,"map":[]},"timecode":{"source":"off","fps":25,"offset_frames":0},"osc_feedback":{"enabled":false,"host":"","port":9000}}}"#, None),
    e("binding_save", "Crea (id 0) o reemplaza un mapeo de control externo MIDI/OSC → acción.", r#"{"type":"binding_save","binding":{"id":0,"label":"Fader 1","source":{"type":"midi","port":null,"kind":"cc","channel":null,"number":1},"action":{"type":"exec_level","exec":1}}}"#, None),
    e("binding_remove", "Elimina un mapeo.", r#"{"type":"binding_remove","id":2}"#, None),
    e("midi_enable", "Activa/desactiva un puerto MIDI (output true = salida, false = entrada) por nombre.", r#"{"type":"midi_enable","output":false,"name":"Launchpad","enabled":true}"#, None),
    // ── stage / vc / show ──
    e("stage_save", "Guarda el escenario 3D entero (tamaño y objetos). Lee get_show stage y reenvía completo. kind: truss|platform|wall|column|curtain|tree|box|screen.", r#"{"type":"stage_save","stage":{"size":[12,8,6],"objects":[{"id":1,"kind":"truss","name":"Truss","pos":[0,5.7,1.8],"rot":[0,0,0],"size":[10.8,0.3,0.3],"color":"gray"}]}}"#, None),
    e("vc_save", "Guarda TODAS las páginas de la consola virtual. Lee get_show vconsole y reenvía completo. kind: button|slider|knob|xy_pad|cue_list|speed_dial|label|frame|clock; mode: momentary|toggle|trigger.", r#"{"type":"vc_save","pages":[{"id":1,"name":"Principal","cols":8,"rows":6,"widgets":[{"id":1,"kind":"button","label":"GO","x":0,"y":0,"w":2,"h":1,"color":"green","action":{"type":"cuelist","cuelist":1,"op":"go"},"mode":"trigger"}]}]}"#, None),
    e("vc_press", "Pulsa/suelta un widget de la consola virtual (id de get_show vconsole).", r#"{"type":"vc_press","widget":1,"down":true}"#, None),
    e("vc_value", "Pone el valor 0..1 de un fader/slider de la consola virtual.", r#"{"type":"vc_value","widget":2,"value":0.7}"#, None),
    e("vc_xy", "Mueve un XY pad (x,y 0..1).", r#"{"type":"vc_xy","widget":3,"x":0.5,"y":0.5}"#, None),
    e("lock", "Bloquea la mesa para todos pidiendo el PIN (necesita un PIN definido). Solo si el usuario lo pide; para desbloquear hace falta el PIN, que la IA no maneja. El blackout de emergencia sigue disponible.", r#"{"type":"lock"}"#, None),
    e("show_merge", "Añade partes de OTRO show guardado a este (se puede deshacer): parts = fixtures (patch), groups, palettes, scenes, cuelists, chases, effects, macros, timelines, scripts, locate. Los ids que chocan se renumeran y las referencias los siguen. Sin fusionar fixtures, solo valen los aparatos con el mismo número y perfil. Las macros, timelines y acciones del guion que llaman a cosas por número NO se reescriben.", r#"{"type":"show_merge","name":"Obra anterior","parts":["groups","palettes","cuelists"]}"#, None),
    e("show_new", "Nuevo show vacío. DESTRUCTIVO: solo si el usuario lo pide.", r#"{"type":"show_new","name":"Obra"}"#, None),
    e("show_save", "Guarda el show (name null = el actual).", r#"{"type":"show_save","name":null}"#, None),
    e("show_load", "Carga otro show por nombre. Sustituye el actual: solo si el usuario lo pide.", r#"{"type":"show_load","name":"Obra"}"#, None),
    e("show_rename", "Renombra el show actual.", r#"{"type":"show_rename","name":"Obra 2"}"#, None),
];

/// Commands that `send_command` refuses by design (UI plumbing, assistant control, raw streams).
pub const NOT_FOR_AI: &[&str] = &["command_line", "midi_in", "subscribe_dmx", "learn_start", "learn_stop", "ai_send", "ai_cancel", "ai_confirm", "ai_answer", "ai_reset", "ai_undo", "ai_configure", "ai_list_models", "ai_test_provider", "unlock", "lock_set"];

pub fn find(ty: &str) -> Option<&'static Entry> {
    REF.iter().find(|x| x.ty == ty)
}

fn lev(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i];
        for j in 1..=b.len() {
            cur.push((prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1])));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// The closest known types to a mistyped one (also matches when the model used a console word or a different word order).
pub fn nearest(ty: &str, n: usize) -> Vec<&'static Entry> {
    let t = ty.trim().to_lowercase().replace([' ', '-'], "_");
    let words: Vec<&str> = t.split('_').filter(|w| w.len() > 1).collect();
    let mut scored: Vec<(i32, &Entry)> = REF
        .iter()
        .map(|x| {
            let overlap = words.iter().filter(|w| x.ty.split('_').any(|p| p == **w || p.starts_with(**w))).count() as i32;
            // the object (`effect`, `chase`, `exec`…) is what the model gets right; the verb is what it invents
            let obj = x.ty.split('_').next();
            let same_object = words.first().map_or(false, |w| obj == Some(*w)) || words.last().map_or(false, |w| obj == Some(*w));
            let verb_like = words.iter().any(|w| ["start", "play", "launch", "enable", "activate", "on", "off", "stop", "disable", "trigger", "fire", "press", "toggle", "run", "exec", "execute"].contains(w))
                && ["run", "toggle", "go", "stop", "fire", "press", "flash"].contains(&x.ty.rsplit('_').next().unwrap_or(""));
            let d = lev(&t, x.ty) as i32 - overlap * 3 - if same_object { 8 } else { 0 } - if verb_like && same_object { 4 } else { 0 };
            (d, x)
        })
        .collect();
    scored.sort_by_key(|(d, x)| (*d, x.ty));
    scored.into_iter().take(n).map(|(_, x)| x).collect()
}

/// `type — what` + example (+ console form) for one command: what the model needs to call it correctly.
pub fn card(x: &Entry) -> String {
    let mut s = format!("**{}** — {}\n  send_command {{\"command\": {}}}", x.ty, x.what, x.example);
    if let Some(c) = x.console {
        s.push_str(&format!("\n  consola (console_command): `{c}`"));
    }
    s
}

/// Everything, grouped. Used by `get_help comandos`.
pub fn full() -> String {
    let mut s = String::from("# Comandos de send_command: función y formato exacto\nSe llaman como `send_command {\"command\": <objeto>}` (el objeto lleva `type` + campos). Los ejemplos son válidos tal cual; cambia solo los ids/valores. Si hay forma de consola, es más corta con `console_command`.\n");
    for (g, tys) in GROUPS {
        s.push_str(&format!("\n## {g}\n"));
        for t in *tys {
            if let Some(x) = find(t) {
                s.push_str(&card(x));
                s.push('\n');
            }
        }
    }
    s
}

/// Compact one-line-per-command index (type, what-first-sentence) for the tool description.
pub fn index() -> String {
    let mut s = String::new();
    for (g, tys) in GROUPS {
        s.push_str(&format!("{g}: "));
        s.push_str(&tys.join(", "));
        s.push('\n');
    }
    s
}

pub fn example_value(x: &Entry) -> Value {
    serde_json::from_str(x.example).expect("example is JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Command;

    #[test]
    fn every_command_is_documented_and_every_example_is_valid() {
        let cat = crate::ai::tools::command_catalog();
        let mut missing = Vec::new();
        for (name, _) in cat {
            if NOT_FOR_AI.contains(&name.as_str()) {
                continue;
            }
            if find(name).is_none() {
                missing.push(name.clone());
            }
        }
        assert!(missing.is_empty(), "commands without a reference entry: {missing:?}");
        for x in REF {
            assert!(cat.iter().any(|(n, _)| n == x.ty), "{} is not a Command type", x.ty);
            let v = example_value(x);
            assert_eq!(v["type"], x.ty, "example type mismatch");
            if x.ty == "fixture_update" {
                continue; // partial by design; completed against the live fixture (covered by an engine test)
            }
            if let Err(err) = serde_json::from_value::<Command>(v) {
                panic!("example of {} is not a valid Command: {err}\n{}", x.ty, x.example);
            }
        }
        // every grouped name exists, each documented command is in exactly one group
        for (_, tys) in GROUPS {
            for t in *tys {
                assert!(find(t).is_some(), "group lists unknown {t}");
            }
        }
        for x in REF {
            assert_eq!(GROUPS.iter().filter(|(_, t)| t.contains(&x.ty)).count(), 1, "{} must be in exactly one group", x.ty);
        }
    }

    #[test]
    fn console_forms_parse() {
        for x in REF {
            if let Some(c) = x.console {
                crate::engine::cmdline_check(c).unwrap_or_else(|e| panic!("console form of {}: «{c}» → {e}", x.ty));
            }
        }
    }

    #[test]
    fn nearest_finds_the_right_command() {
        assert!(nearest("effect_run", 3).iter().any(|x| x.ty == "effect_toggle"));
        assert!(nearest("add_fixture", 3).iter().any(|x| x.ty == "fixture_add"));
        assert!(nearest("exec go", 3).iter().any(|x| x.ty == "exec_go"));
        assert!(nearest("chase_start", 3).iter().any(|x| x.ty == "chase_run"));
    }
}
