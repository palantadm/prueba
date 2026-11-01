## «No sale luz» — lista de comprobación
<!--ai-->Usa `get_fixture_state` con los ids afectados: indica en `notes` las causas probables. Orden recomendado:<!--/ai--><!--ui-->Mira el **Monitor**: estado de cada salida (OK/Error con mensaje) y valores DMX por universo. Orden recomendado:<!--/ui-->
1. **Blackout** activo (`blackout off`) o **grand master** a 0 (`gm 100`).
2. **Dimmer a 0**: nadie lo sube (programador vacío, cuelist parada, escena apagada).
3. **Blind** activo: el programador no sale a escena (`blind off`).
4. **Perfil o modo que no existe** en la biblioteca: el aparato no emite.
<!--ai-->5. **Universo sin salida activa** o con error: usa `diagnose_outputs` (revisa Art-Net, sACN, serie, nodos, IP del equipo y da el arreglo). Detalle en el tema `salidas`.<!--/ai--><!--ui-->5. **Universo sin salida activa** o con error: Ajustes → Universos y salidas (Estado). Para Art-Net/sACN/serie mira el tema «Universos y salidas».<!--/ui-->
6. **Direcciones solapadas** o fuera de rango (dirección + canales > 512).
7. **Canales aparcados** (`park`) que fijan un valor; `unpark`.
8. **Test de salida** activo sustituye la salida normal.
9. En el propio aparato: dirección y modo físicos distintos de los del patch.

## Otros problemas
- *Colores raros*: modo de canales del aparato distinto del parcheado; compara el modo del patch con el display del aparato.
- *Un móvil no se mueve*: pan/tilt en el programador o cue; atributos con rangos invertidos (`invert` en el perfil); canal de control/lámpara.
- *Los fundidos no se ven*: tiempos de la cue a 0, o el programador ocupando el atributo.
- *MIDI/OSC no responde*: Monitor → entradas; revisar puerto, canal y número del binding.
- *Latencia o saltos*: `tick_hz` en Ajustes (44 es el máximo útil del DMX512 con tramas completas) y red cableada para Art-Net/sACN.

<!--ai-->- *Localizar no enciende un móvil/cabeza*: el perfil necesita más que el dimmer (color/blanco, lámpara, shutter abierto). Mira `get_show` sección `locate` y `get_profile`; ajusta con `locate_save {profile, attrs}` o captura los valores de un aparato bien configurado con `locate_capture {fixture}`.<!--/ai--><!--ui-->- *Localizar no enciende un móvil/cabeza*: el perfil necesita más que el dimmer (color/blanco, lámpara, shutter abierto). En Patch → Localizar puedes ajustar qué valores se usan para ese perfil, o copiarlos de un aparato que ya funciona.<!--/ui-->
<!--ai-->- *El guion no avanza / no lanza*: tema `guion`; `get_runtime` → `scripts` indica bloque, última marca y siguiente.<!--/ai--><!--ui-->- *El guion no avanza / no lanza*: mira el tema «Guion»; la barra inferior del espacio Guion muestra la última marca lanzada y la siguiente.<!--/ui-->
