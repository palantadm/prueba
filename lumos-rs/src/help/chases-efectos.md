## Chases
Un chase es una lista de pasos que se repite al ritmo de un tempo.
- Cada paso tiene `values` (por aparato) o una `scene`, `hold` (tiempos de pulso) y `fade` (0 = salto seco, 1 = fundido continuo).
- Propiedades: `bpm`, `dir` (forward, backward, bounce, random) y opcionalmente un `speed_master`.
- Control: `chase 1 on|off`, `chase 1 bpm 128`, `chase 1 tap`, `next`, `prev`.

## Efectos
Un efecto es una forma de onda que se suma al valor actual de un atributo: `attr` (cualquier atributo, o `hue` para arcoíris en aparatos RGB), `shape` (sine, triangle, saw, square, random, pulse, ramp; las 2D circle y eight mueven `attr` y `attr2`, p. ej. pan y tilt), `rate` (ciclos por segundo) o `bpm`, `depth` (amplitud 0–1), `phase_spread` (grados repartidos entre los aparatos para ondas que viajan) y `fixtures`.
- Los efectos se crean y borran en la interfaz de efectos de Live; se activan desde un executor de efecto o ahí mismo. El fader del executor es la profundidad.<!--ai--> Por protocolo: `effect_save` / `effect_remove`.<!--/ai-->

## Speed masters
Un `speed_master` tiene un `bpm` (20–240) que pueden seguir varios chases y efectos; un executor de velocidad lo ajusta y su botón marca el tempo (tap).

## Consejos

## Ajustes avanzados del efecto
Como en ChamSys o GrandMA, cada efecto tiene opciones para decidir **quién** participa y **cómo** se reparte la fase. En el editor del efecto, sección «Avanzado»:
- **Pares / impares**, **desde–hasta** (posiciones de la lista de aparatos del efecto) y **uno de cada N**: el efecto solo toca esos aparatos.
- **Orden de la fase**: normal, inverso, aleatorio (siempre el mismo), **del centro afuera** o **de los lados al centro**.
- **Grupos** (A B A B…) y **bloques** (N aparatos seguidos) comparten la fase; **alas** = 2 hace la onda simétrica.
- **Dirección**: adelante, atrás o ida y vuelta. **Desfase** global en grados.
- **Rango**: *relativo* (se mueve alrededor del valor actual, con un desplazamiento opcional) o *absoluto* (oscila entre un mínimo y un máximo sin importar el valor actual: lo habitual para efectos de intensidad que enciendan y apaguen).
- **Ancho** del ciclo en ondas cuadrada y pulso.
<!--ai-->Por protocolo, todo esto va en el objeto `adv` de `effect_save` (mira `get_help comando effect_save`).<!--/ai-->

- Para un barrido de color: efecto `hue` con `phase_spread` 360 y `rate` bajo.
- Para un movimiento: efecto `circle` con `attr` pan y `attr2` tilt.

## Mapas de píxeles
Una fila o rejilla de aparatos RGB (barras LED, tubos de píxel, una pared de pares) se trata como los píxeles de una pantalla pequeña y se pinta con un patrón en movimiento. **Live → Píxeles → «+ Nuevo mapa de píxeles»** usa la selección **en orden de cableado**.
- **Patrones:** arcoíris, degradado, onda, barra que corre (con ancho y suavidad), ruido, estrobo y color fijo.
- **Controles:** píxeles por fila (rejilla), zigzag si las filas alternas están cableadas al revés, velocidad en ciclos por segundo (negativa = al revés), repeticiones por el mapa, dirección en grados (0 = izquierda→derecha, 90 = arriba→abajo), colores A y B y nivel.
- **Prioridad:** actúa por encima de cuelists y escenas y **por debajo del programador** (lo que toques a mano manda). Se arranca y para con el botón ON/OFF o desde la consola: `pixel 1 on`, `pixel 1 off`.
- Con «Mantener también el dimmer al máximo» los aparatos RGB con canal de dimmer se encienden solos; sin esa opción, el dimmer lo decides tú.
<!--ai-->Por protocolo: `pixel_save {map}`, `pixel_run {id,on}`, `pixel_remove {id}`.<!--/ai-->
