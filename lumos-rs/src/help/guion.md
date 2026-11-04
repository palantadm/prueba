Espacio **Guion**: la escaleta o texto de la obra, en el orden en que se dice, con **marcas de cue** que lanzan cosas en directo y un lector que va avanzando.

## Estructura
- **Script** `{id, name, blocks[], pdf?}`; el PDF original se muestra al lado.
- **Bloque** `{id, kind, speaker, text, page, marks[]}`; `kind`: `heading` (acto/escena), `speech` (texto hablado), `direction` (acotación), `note` (nota privada del regidor).
<!--ai-->- **Marca** `{id, label, kind, at_word, actions[], link?, standby_blocks, note}`; `kind`: `light`, `sound`, `video`, `set`, `other`. `at_word` = palabra del bloque donde se canta el cue. `standby_blocks` = cuántos bloques antes se avisa «AVISO».<!--/ai--><!--ui-->- **Marca**: tiene etiqueta, tipo (luz, sonido, vídeo, escena, otro), la palabra del bloque donde se canta el cue (`at_word`), una o varias **acciones**, un cue enlazado opcional y un aviso («AVISO») unos bloques antes.<!--/ui-->
- `link {cuelist, number}`: al lanzar la marca se hace GO a ese cue; y si ese cue se lanza desde otro sitio (modo seguir), el guion salta a la marca.
- **Acciones** de una marca (se ejecutan en el motor, aunque la interfaz esté cerrada): **consola** (línea de comandos: `go cuelist 1`, `scene 3 on`, `macro 2`), **OSC** (host, puerto, dirección, argumentos) (QLab, Resolume…), **MIDI** (puerto, tipo, canal, número, valor), **HTTP** (método, URL, cuerpo) (VLC, OBS, webhooks).

## Importar
Desde el espacio Guion: PDF, .txt, .md o Fountain. Devuelve un borrador (bloques + avisos) que se revisa y se guarda. Los PDF escaneados (imagen) no se pueden leer: hace falta un PDF con texto.

## Mandos
<!--ai-->`script_go {id}` (lanza la siguiente marca y avanza), `script_back`, `script_goto {id, block, mark?}`, `script_fire {id, block, mark}` (lanza una marca concreta), `script_follow {id, on}` (el guion sigue a los cues), `script_reset`. El estado en vivo está en `get_runtime` → `scripts`: bloque actual, última marca, siguiente `(bloque, marca)`.<!--/ai--><!--ui-->Barra inferior del Guion: **GO** lanza la siguiente marca y avanza; **Atrás** vuelve a armar la anterior; pulsando un bloque o una marca colocas el lector sin lanzar nada; «Seguir» hace que el guion siga a los cues lanzados desde cualquier sitio; «Reiniciar» vuelve al principio. Espacio = GO, Retroceso = Atrás.<!--/ui-->

## Consejos
<!--ai-->- `script_save` guarda el guion entero: lee primero con `get_show` (`id`), cambia lo mínimo y reenvía todo, con los ids de bloque y marca existentes; los ids nuevos deben ser únicos dentro del guion.<!--/ai-->
- Lanzar marcas ejecuta acciones reales (luces, vídeo, sonido): es acción en directo.
<!--ai-->- Ante «no pasa nada al lanzar»: revisa las acciones de la marca (host/puerto OSC, URL HTTP, texto de consola válido con `console_command`) y el estado `scripts` del runtime.<!--/ai--><!--ui-->- Ante «no pasa nada al lanzar»: revisa las acciones de la marca (host y puerto OSC, URL HTTP, texto de consola válido) y la línea de estado del Guion.<!--/ui-->

## Seguimiento por voz (sin internet)
Botón 🎙 **Voz** del espacio Guion. Lumos escucha el micrófono y mueve el lector según lo que dicen los actores, con reconocimiento **local** (Whisper): no sale audio del equipo. Es el motor de Lumos, no el navegador: sigue funcionando con la interfaz cerrada.
- **Primer uso:** elegir idioma (`es`, `gl`, `ca`, `eu`, `en`…) y descargar un modelo (**Small**, 181 MB, es un buen equilibrio; **Medium** y **Large v3 turbo** aciertan más y piden un PC potente). Para gallego mejor que el genérico se puede usar un ajuste fino convertido a ggml (en «Más» → «Archivo de modelo propio»).
- **Cómo sigue:** compara las últimas palabras oídas con las ~48 siguientes del guion (ignora acotaciones y notas), tolerante a errores de reconocimiento. Solo avanza, nunca retrocede. El texto que viene ayuda al reconocimiento (nombres propios, palabras raras).
- **Cues:** por defecto la voz solo mueve el lector. Con «Disparar también los cues» también lanza la marca siguiente, **solo con frases completas** (un parcial puede corregirse; un cue lanzado no).
<!--ai-->- **Comandos:** `speech_configure {cfg}`, `speech_listen {on}`, `speech_model_get {id}`, `speech_model_remove {id}`. Estado en `get_runtime` → `speech` (escuchando, modelo listo, descarga, última frase oída, error).<!--/ai-->
- **Límites:** retardo de 1–2 s; en una sala con mucho ruido o voces solapadas falla más; los números dichos en cifras («3») no coinciden con «tres». Si falla, GO a mano.
