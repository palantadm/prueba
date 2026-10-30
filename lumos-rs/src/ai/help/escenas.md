Una **escena** es un look grabado (una foto de valores por aparato) que se puede encender y apagar con fundidos, mezclar con un nivel y fundir hacia otra escena (morph).

## Crear
- Desde el programador: `record scene 3 Amanecer` (añade `output` para grabar lo que sale ahora en lugar del programador).
- Por protocolo: `scene_record` (con `from_output`, `fixtures`, `merge`) o `scene_save` con la escena completa.

## Usar
- `scene 3 on`, `scene 3 off`, `scene 3 toggle`, `scene 3 at 60` (nivel), `scene solo 3` (apaga las demás), `scenes release`.
- En un executor de tipo escena: el fader es el nivel y el botón la enciende/apaga con `fade_in` / `fade_out` (segundos).

## Tipos (`kind`)
- `look`: mezcla todos los atributos según el nivel.
- `intensity`: solo toma el dimmer de la escena (submaster de intensidad).
- `inhibitive`: el nivel limita el dimmer de sus aparatos (submaster inhibitivo).

## Notas
- Varias escenas activas se combinan: dimmer por HTP, resto por último valor.
- Una escena tiene color de acento (`color`) para su botón.
- Borrarla es deshacible, pero apagar una escena en directo cambia lo que ve el público ahora.
