## Para qué sirve
El **seguimiento** hace que un grupo de cabezas móviles apunte solo a una persona o a un objeto que lleva un tracker, como un foco de seguimiento sin operador. Lumos recibe las posiciones por **PosiStageNet (PSN)**, el protocolo abierto de seguimiento.

- **BlackTrax** puede enviar PSN (actívalo en su configuración de salida). **Zactrack**, **Stage Precision**, **Kinesys** y otros también.
- Las posiciones llegan en metros. Lumos las convierte a las coordenadas del escenario y calcula el pan y tilt de cada cabezal con su posición y rotación del patch.

## Puesta en marcha
1. Coloca los cabezales en el **escenario** (posición y rotación correctas): sin eso no pueden apuntar bien.
2. Ajustes → **Seguimiento** → activa la entrada PSN. Por defecto escucha el grupo `236.10.10.10`, puerto `56565`. Si tienes varias tarjetas de red, indica la IP de la que recibe el seguimiento.
3. Verás la lista de **trackers** con su posición en directo. Si no aparece ninguno, mira la red (misma red, multicast permitido).
4. Crea un grupo con los cabezales y, en la misma página, añade un **seguimiento**: tracker, grupo, altura, suavizado e intensidad.
<!--ai-->Por protocolo: `settings_save` (con `settings.psn`), `follow_save`, `follow_set`, `follow_remove`. Los trackers presentes están en `get_runtime` → `psn.trackers` (id, nombre, posición ya en coordenadas del escenario, `active`) y el estado de cada seguimiento en `psn.follows`.<!--/ai-->

## Ajustes que importan
- **Altura**: metros por encima del punto del tracker a los que apunta. Con el tracker en el cinturón, 0,3; en el suelo, 1,5.
- **Suavizado**: milisegundos. Con 0 sigue cada muestra (tiembla si el seguimiento es ruidoso); 200–500 da un movimiento natural.
- **Intensidad**: si la pones, el dimmer sube mientras el tracker está presente y se apaga si se pierde. Si la dejas vacía, no se toca el dimmer (lo controlas con cues o ejecutores).
- Un tracker que deja de enviar más de un segundo se considera perdido: los cabezales se quedan donde estaban, sin volver al centro.

## Ejes y calibración
Cada sistema numera los ejes a su manera. Por defecto, X es derecha, Y es hacia delante y Z es arriba. En Ajustes → Seguimiento puedes intercambiar ejes, invertirlos, sumar un desplazamiento (el origen del seguimiento rara vez coincide con el del escenario) y cambiar la escala.

Prueba: sitúa a una persona en un punto conocido del escenario y comprueba que sus coordenadas en la lista son las esperadas; corrige ejes y desplazamiento hasta que coincidan.

## Si no funciona
- La lista de trackers está vacía: comprueba que PSN está activado en el sistema de seguimiento, la misma red, y que no hay un cortafuegos bloqueando UDP 56565.
- El cabezal apunta a otro lado: revisa la posición y rotación del aparato en el escenario y los ejes.
- El movimiento tiembla: sube el suavizado.
- Un seguimiento activado manda sobre el pan y el tilt de cues y programador (también cuando el tracker se pierde, porque mantiene la última posición); desactívalo para volver a controlarlos a mano.
