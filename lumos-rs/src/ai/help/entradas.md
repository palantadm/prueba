Lumos recibe órdenes externas por **MIDI** (Web MIDI en el navegador, MIDI nativo en la app de escritorio), **OSC** (UDP, puerto 7700 por defecto) y **Art-Net**; el timecode llega por MTC.

## Bindings
Un binding (`binding_save`) conecta una **fuente** con una **acción**:
- Fuentes: MIDI (nota, CC o program; canal 1–16 o cualquiera; puerto opcional) y OSC (dirección).
- Acciones: nivel/GO/BACK/FLASH de un executor, GO/BACK de una cuelist, grand master, blackout, canal DMX, atributo de un aparato o grupo, activar un efecto, aplicar una paleta, limpiar el programador.
- `out_min`/`out_max` escalan los valores continuos; las acciones tipo disparo actúan al cruzar 0,5.
- **Aprender (learn)**: la interfaz pone el binding en modo aprendizaje y captura el siguiente mensaje que llegue.

## MIDI
Ajustes → MIDI lista los puertos; en la app de escritorio cada puerto nativo se habilita o no (`midi_enable`, ajuste de la máquina, no del show). El feedback a LEDs y faders motorizados sale por MIDI y el feedback OSC se configura en Ajustes → OSC feedback (host y puerto).

## OSC
Direcciones del tipo `/lumos/cuelist/N/current` para el estado; las entradas se asignan con bindings o con los widgets de la consola virtual.

## Consejo
Si un control no responde: Monitor → entradas muestra los mensajes recibidos recientemente.
