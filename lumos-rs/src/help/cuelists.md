Una **cuelist** es una secuencia de **cues** numeradas (1, 2, 2.5…) que se lanzan con GO.

## Crear y editar
- `record cue 5 name Entrada` graba el programador como cue 5 en la cuelist por defecto (`cuelist 2` cambia la lista por defecto; `record cue 5 cuelist 2` la indica). Opciones: `replace`, `merge`, `release` (libera el programador al grabar).
- `update cue 5` vuelve a grabar el programador sobre la cue (`replace`, `track`, `cueonly`).
<!--ai-->- Estructurado: `cuelist_save`, `cue_save` (cue completa), `cue_update`, `cue_remove`, `cuelist_remove`.<!--/ai-->

## Tiempos de cada cue
`fade_in` (atributos que suben), `fade_out` (los que bajan) y `delay`, en segundos. `follow` lanza la siguiente cue automáticamente tras N segundos; `timecode` (`HH:MM:SS:FF`) lanza GO al llegar el timecode. `timing` permite tiempos distintos por grupo de atributos (intensity, position, color, beam, control); `curve` da la forma del fundido (linear, s_curve, square, inv_square, sine); `stagger` retrasa cada aparato en orden.

## Tracking
En una lista con `tracking`, cada cue guarda solo los **cambios** y el estado se acumula; una cue `block` guarda el estado completo y corta el seguimiento. `auto_mark` pre-posiciona los móviles apagados antes de que se enciendan.

## Reproducción
`go`, `back`, `pause`, `goto cue 10`, `release` (con `cuelist N`). `looped` hace que al final vuelva a la primera cue. `priority` decide los conflictos entre listas (dimmer por HTP, resto por prioridad y último GO). El fader de nivel de la cuelist escala el dimmer.

GO, back y goto cambian lo que ve el público: son acciones «en directo».
