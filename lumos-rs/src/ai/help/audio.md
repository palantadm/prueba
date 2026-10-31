Ajustes → **Audio**: qué dispositivos usa Lumos, qué hace cada canal de entrada, a dónde sale cada sonido y cómo se ven los niveles. Lo gestiona el motor de Lumos (app de escritorio), no el navegador. La configuración es de la máquina (`audio.json`), no del show.

## Dispositivos y canales
- `input_device` / `output_device`: nombre del dispositivo (`""` = predeterminado del sistema). Lista y canales reales en `get_runtime` → `audio` (`devices`, `in_channels`, `out_channels`, solo con el audio abierto).
- `audio_run {on:true}` abre los dispositivos; `autostart` los abre al iniciar. Un `sound_play` abre la salida solo si hace falta.

## Entradas: función de cada canal (`inputs`)
Cada canal de entrada (1…N) puede tener `role`:
- `voice` — alimenta el seguimiento del Guion por voz (sin ningún canal marcado se usa el canal 1).
- `reactive` — análisis de la música: nivel, graves, medios, agudos y beat/BPM. Aparecen como fuente **Audio** en Controles (`binding_save` con `source {type:"audio", band: level|bass|mid|treble|beat}`) y mueven un master, un atributo, un executor o disparan GO/flash con cada golpe.
- `ltc` — señal de timecode LTC; con el timecode del show en la fuente `ltc` (`settings_save` → `timecode.source: "ltc"`) mueve timelines y cues por timecode.
- `off` — sin uso. `gain_db` recorta la entrada (−24…+24) y `label` la nombra.

## Salidas: matriz (`routes`)
Cada ruta lleva un **bus** a un canal físico: `{source, src_ch, out_ch, gain_db}`.
- `source: "sounds"` (los sonidos que lanzas), `"timeline"` (el archivo de audio de la timeline en marcha) y `"ltc"` (el timecode del show como señal de audio, mono: `src_ch` 1).
- `src_ch` 1 = izquierdo/mono, 2 = derecho; `out_ch` = canal de salida del dispositivo (1…N).
- `routes` vacío = sonidos y timeline en estéreo a las salidas 1 y 2. `master_db` es la salida general.
- Ejemplo: sonidos a las salidas 3-4 y LTC a la 8: `[{"source":"sounds","src_ch":1,"out_ch":3},{"source":"sounds","src_ch":2,"out_ch":4},{"source":"ltc","src_ch":1,"out_ch":8}]` (cada ruta con `gain_db`).
- `audio_configure` guarda **todo el objeto**: lee `get_runtime` → `audio.cfg` y reenvía el conjunto con tus cambios.

## Sonidos
- Archivos wav, mp3, flac, ogg, m4a/aac y aiff de la carpeta de medios (se suben en Ajustes → Audio o con la timeline). Máximo 20 minutos cada uno.
- Consola: `sound "gong.wav"`, `sound "ambiente.mp3" at 60 loop`, `sound stop` / `sound stop "gong.wav"`. Funciona en macros, timelines, marcas del Guion (acción de consola) y mapeos. Sin extensión (`sound gong`) busca `gong.*`.
- `sound_play {file, volume 0..1, looped}` / `sound_stop {file}` por `send_command`.
- Una timeline con archivo de audio lo reproduce sincronizado: al arrancar, al saltar a otro punto y al parar.

## Diagnóstico
- «No suena»: ¿audio abierto (`audio.output_running`)? ¿`out_channels` ≥ canal de la ruta? ¿ruta del bus correcto (`sounds`/`timeline`)? ¿`master_db`? ¿volumen del dispositivo del sistema? `audio.playing` muestra lo que suena y `audio.out_levels` el nivel por canal físico.
- «No entra señal»: `audio.in_levels` por canal; revisa dispositivo de entrada y permisos del micrófono del sistema (macOS/Windows piden permiso).
- LTC: `audio.ltc.locked` y `text`; el decodificador espera 24–30 fps (usa el fps del timecode de Ajustes).
- Reactivo: `audio.reactive` [nivel, graves, medios, agudos] y `audio.bpm`; si es 0, no hay canal con `role: reactive` o no hay señal.
