Los **executors** son los faders y botones de la Mesa (Desk), organizados en **páginas** (`page`) y **huecos** (`slot`, empezando en 0). Cada uno tiene una etiqueta y un tipo (`kind`):

| Tipo | Fader | Botón |
|---|---|---|
| `cuelist` | nivel (intensidad) de la cuelist | GO / BACK / FLASH |
| `group` | master de dimmer del grupo | flash |
| `scene` | nivel de la escena | enciende/apaga |
| `chase` | tamaño/nivel del chase | arranca/para |
| `effect` | profundidad del efecto | activa/desactiva |
| `speed` | BPM del speed master | tap tempo |
| `xfade` | crossfade manual de la cue en curso (0–1) | – |
| `master` | master general (`grand`) | – |
| `empty` | libre | – |

- Asignar o cambiar: en la mesa (espacio Mesa), pulsando el hueco del executor; se vacía desde el mismo menú.<!--ai--> Por protocolo: `exec_save` con el executor completo; `exec_remove` lo vacía.<!--/ai-->
- Los executors se pueden mapear a faders/botones MIDI u OSC mediante bindings (`entradas`) o a widgets de la consola virtual.
- El **master general** escala el dimmer (y el color de los aparatos sin dimmer propio, como un par RGB). El **blackout** fuerza la salida a cero. Ambos afectan a todo y son acciones en directo.
