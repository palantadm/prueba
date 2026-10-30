## «No sale luz» — lista de comprobación
Usa `get_fixture_state` con los ids afectados: indica en `notes` las causas probables. Orden recomendado:
1. **Blackout** activo (`blackout off`) o **grand master** a 0 (`gm 100`).
2. **Dimmer a 0**: nadie lo sube (programador vacío, cuelist parada, escena apagada).
3. **Blind** activo: el programador no sale a escena (`blind off`).
4. **Perfil o modo que no existe** en la biblioteca: el aparato no emite.
5. **Universo sin salida activa** o con error (Monitor → salidas; Ajustes → Universos). Art-Net: ¿IP/host y red correctos? sACN: ¿multicast permitido? Serie: ¿puerto correcto?
6. **Direcciones solapadas** o fuera de rango (dirección + canales > 512).
7. **Canales aparcados** (`park`) que fijan un valor; `unpark`.
8. **Test de salida** activo (`output_test`) sustituye la salida normal.
9. En el propio aparato: dirección y modo físicos distintos de los del patch.

## Otros problemas
- *Colores raros*: modo de canales del aparato distinto del parcheado; `get_profile` y comparar con el display del aparato.
- *Un móvil no se mueve*: pan/tilt en el programador o cue; atributos con rangos invertidos (`invert` en el perfil); canal de control/lámpara.
- *Los fundidos no se ven*: tiempos de la cue a 0, o el programador ocupando el atributo.
- *MIDI/OSC no responde*: Monitor → entradas; revisar puerto, canal y número del binding.
- *Latencia o saltos*: `tick_hz` en Ajustes (44 es el máximo útil del DMX512 con tramas completas) y red cableada para Art-Net/sACN.
