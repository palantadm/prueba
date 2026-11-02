## Para qué sirve
Desde la consola virtual puedes manejar **otros programas** (Resolume, QLab, Ableton Live…) con un botón o un fader, sin escribir rutas OSC a mano. Lumos trae una lista de acciones ya hechas para cada programa; tú solo eliges cuál y rellenas los números (capa, clip, cue…).

Un mando MIDI se asigna después a ese botón o fader como a cualquier otro widget de la consola virtual (modo «Aprender»), así que el mando maneja Resolume a través de Lumos.

## Cómo se hace
1. **Da de alta el programa** una vez: Ajustes → **Programas externos** → «Nuevo destino». Pon un nombre, elige el programa (Lumos rellena el puerto habitual), y la IP del ordenador donde corre (`127.0.0.1` si es el mismo).
2. En el programa de destino activa la entrada OSC con ese mismo puerto (cada programa lo explica abajo).
3. En la **Consola virtual** pulsa «Añadir de un programa», elige el destino, la acción de la lista y los números que pida. Aparece un botón o un fader ya configurado.
4. Para el mando MIDI: edita el widget, «Entradas» → Aprender, y mueve el control.
<!--ai-->Por protocolo: `osc_target_save` (una vez) y `vc_add_osc` con `action` «programa/acción» y `params`; `osc_send` prueba una acción sin crear botón. Usa SIEMPRE las acciones de esta lista; no inventes direcciones OSC. Si el programa no está, el usuario puede añadir el suyo con `osc_software_save`; si solo necesitas una dirección suelta, usa una acción de guion `osc`.<!--/ai-->

## Comportamiento de cada tipo
- **Botón**: envía un mensaje al pulsar (p. ej. lanzar un clip).
- **Interruptor**: envía 1 al activarlo y 0 al desactivarlo (p. ej. ocultar una capa).
- **Fader**: envía su valor mientras lo mueves; en Resolume va de 0 a 1.

## Añadir otro programa
Ajustes → Programas externos → «Programa propio»: nombre, puerto y una lista de acciones (nombre, dirección OSC y qué tipo de control es). Dentro de la dirección puedes poner huecos entre llaves, como `/ch/{canal}/mute`: Lumos pedirá ese número al añadir el botón. Una vez creado aparece junto a los programas de Lumos.

## Programas incluidos
{{OSC_CATALOG}}

## Si no responde
- Comprueba IP y puerto del destino y que el programa tiene la entrada OSC activada.
- Los dos equipos deben estar en la misma red y sin cortafuegos que bloquee UDP en ese puerto.
- OSC no devuelve confirmación: Lumos no puede saber si el programa lo recibió. Prueba con el botón «Probar» de la acción.
