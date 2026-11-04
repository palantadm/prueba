El **programador** es el área de trabajo manual: guarda los valores que pones a los aparatos seleccionados y **tiene prioridad sobre cuelists, escenas y efectos** (último valor gana, LTP) para los atributos que contiene.

## Selección
- En la interfaz: pulsa aparatos o grupos; también «Todos / Ninguno / Invertir / Impares / Pares».
- En la línea de comandos: `fixture 1 thru 8 + 12 - 4`, `group 2`, `all`, `odd`, `even`, `next`, `prev`.
- La selección vive en la sesión de interfaz o dentro de un mismo texto de comando.

## Poner valores
- `fixture 1 thru 4 at 70` (dimmer por defecto), `red at 100`, `pan at 30`, `fixture 1 thru 8 at 0 thru 100` (abanico), `colour orange`.
<!--ai-->- Por protocolo: `prog_set` con `fixtures` y `attrs` (valores 0–1).<!--/ai-->
- `locate`/`identify` enciende un aparato para localizarlo.

## Liberar y deshacer
- `clear` vacía el programador; puedes liberar solo ciertos aparatos o atributos.
- `blind on`: el programador deja de salir a escena para preparar un look sin que se vea; las ediciones en blind son seguras.

## Grabar
`record cue`, `record scene`, `record palette`, `record group` toman lo que haya en el programador (tema `cuelists`, `escenas`, `grupos-paletas`). Después de grabar, suele convenir `clear`.

## Masters
El programador se mezcla antes del grand master y del blackout: si todo está a oscuras, mira esos dos primero (`diagnostico`).

## Apuntar a un punto
Con cabezas móviles seleccionadas, en **Live → Posición → «Apuntar a…»** eliges un punto del escenario (arrastrando sobre el plano visto desde arriba, o con X/Z en metros), un objeto del escenario u otro aparato, y la altura del punto (un actor: 1,4 m el pecho). Las cabezas calculan su pan/tilt y los dejan en el programador; al arrastrar es un seguidor manual (follow-spot). También: herramienta «Apuntar» del 3D/2D (clic en el escenario) y la consola: `aim 1.5 0 2 height 1.4`, `fixture 1 thru 4 aim object 3`.
<!--ai-->Por protocolo: `aim {fixtures, target:{type:"point",x,y,z}|{type:"object",id}|{type:"fixture",id}, height}`.<!--/ai-->
