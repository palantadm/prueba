El **programador** es el área de trabajo manual: guarda los valores que pones a los aparatos seleccionados y **tiene prioridad sobre cuelists, escenas y efectos** (último valor gana, LTP) para los atributos que contiene.

## Selección
- En la interfaz: pulsa aparatos o grupos; también «Todos / Ninguno / Invertir / Impares / Pares».
- En la línea de comandos: `fixture 1 thru 8 + 12 - 4`, `group 2`, `all`, `odd`, `even`, `next`, `prev`.
- La selección vive en la sesión de interfaz o dentro de un mismo texto de comando.

## Poner valores
- `fixture 1 thru 4 at 70` (dimmer por defecto), `red at 100`, `pan at 30`, `fixture 1 thru 8 at 0 thru 100` (abanico), `colour orange`.
- Por protocolo: `prog_set` con `fixtures` y `attrs` (valores 0–1).
- `locate`/`identify` enciende un aparato para localizarlo.

## Liberar y deshacer
- `clear` (o `prog_release`) vacía el programador; puedes liberar solo ciertos aparatos o atributos.
- `blind on`: el programador deja de salir a escena para preparar un look sin que se vea; las ediciones en blind son seguras.

## Grabar
`record cue`, `record scene`, `record palette`, `record group` toman lo que haya en el programador (tema `cuelists`, `escenas`, `grupos-paletas`). Después de grabar, suele convenir `clear`.

## Masters
El programador se mezcla antes del grand master y del blackout: si todo está a oscuras, mira esos dos primero (`diagnostico`).
