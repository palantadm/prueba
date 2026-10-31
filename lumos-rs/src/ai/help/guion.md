Espacio **Guion**: la escaleta o texto de la obra, en el orden en que se dice, con **marcas de cue** que lanzan cosas en directo y un lector que va avanzando.

## Estructura (`get_show` sección `scripts`, `id` para el detalle)
- **Script** `{id, name, blocks[], pdf?}`; el PDF original se muestra al lado.
- **Bloque** `{id, kind, speaker, text, page, marks[]}`; `kind`: `heading` (acto/escena), `speech` (texto hablado), `direction` (acotación), `note` (nota privada del regidor).
- **Marca** `{id, label, kind, at_word, actions[], link?, standby_blocks, note}`; `kind`: `light`, `sound`, `video`, `set`, `other`. `at_word` = palabra del bloque donde se canta el cue. `standby_blocks` = cuántos bloques antes se avisa «AVISO».
- `link {cuelist, number}`: al lanzar la marca se hace GO a ese cue; y si ese cue se lanza desde otro sitio (modo seguir), el guion salta a la marca.
- **Acciones** de una marca (se ejecutan en el motor, aunque la interfaz esté cerrada): `console {text}` (línea de comandos: `go cuelist 1`, `scene 3 on`, `macro 2`), `osc {host,port,address,args}` (QLab, Resolume…), `midi {port,kind,channel,number,value}`, `http {method,url,body}` (VLC, OBS, webhooks).

## Importar
Desde el espacio Guion: PDF, .txt, .md o Fountain (`/api/script/import`). Devuelve un borrador (bloques + avisos) que se revisa y se guarda con `script_save`. Los PDF escaneados (imagen) no se pueden leer: hace falta un PDF con texto.

## Mandos
`script_go {id}` (lanza la siguiente marca y avanza), `script_back`, `script_goto {id, block, mark?}`, `script_fire {id, block, mark}` (lanza una marca concreta), `script_follow {id, on}` (el guion sigue a los cues), `script_reset`. El estado en vivo está en `get_runtime` → `scripts`: bloque actual, última marca, siguiente `(bloque, marca)`.

## Consejos
- `script_save` guarda el guion entero: lee primero con `get_show` (`id`), cambia lo mínimo y reenvía todo, con los ids de bloque y marca existentes; los ids nuevos deben ser únicos dentro del guion.
- Lanzar marcas (`script_go`, `script_fire`) ejecuta acciones reales (luces, vídeo, sonido): es acción en directo.
- Ante «no pasa nada al lanzar»: revisa las acciones de la marca (host/puerto OSC, URL HTTP, texto de consola válido con `console_command`) y el estado `scripts` del runtime.
