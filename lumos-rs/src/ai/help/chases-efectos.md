## Chases
Un chase es una lista de pasos que se repite al ritmo de un tempo.
- Cada paso tiene `values` (por aparato) o una `scene`, `hold` (tiempos de pulso) y `fade` (0 = salto seco, 1 = fundido continuo).
- Propiedades: `bpm`, `dir` (forward, backward, bounce, random) y opcionalmente un `speed_master`.
- Control: `chase 1 on|off`, `chase 1 bpm 128`, `chase 1 tap`, `next`, `prev`. Por protocolo `chase_save` / `chase_remove`.

## Efectos
Un efecto es una forma de onda que se suma al valor actual de un atributo: `attr` (cualquier atributo, o `hue` para arcoíris en aparatos RGB), `shape` (sine, triangle, saw, square, random, pulse, ramp; las 2D circle y eight mueven `attr` y `attr2`, p. ej. pan y tilt), `rate` (ciclos por segundo) o `bpm`, `depth` (amplitud 0–1), `phase_spread` (grados repartidos entre los aparatos para ondas que viajan) y `fixtures`.
- `effect_save` / `effect_remove`; se activan desde un executor de efecto o la interfaz de efectos en Live. El fader del executor es la profundidad.

## Speed masters
Un `speed_master` tiene un `bpm` (20–240) que pueden seguir varios chases y efectos; un executor de velocidad lo ajusta y su botón marca el tempo (tap).

## Consejos
- Para un barrido de color: efecto `hue` con `phase_spread` 360 y `rate` bajo.
- Para un movimiento: efecto `circle` con `attr` pan y `attr2` tilt.
