## Macros
Una macro es una lista de pasos (`steps`); cada paso es un texto de la línea de comandos más una espera (`wait`, segundos). Se ejecutan sin bloquear el motor, pueden anidarse hasta 8 niveles y, si un paso falla, la macro se aborta.
- `macro_save` / `macro_remove`; lanzar con `macro 3`; parar con `macro stop`.
- Se pueden poner en un botón de la consola virtual.

## Timelines
Una línea de tiempo (`length` en segundos) tiene eventos (`at`, `text`, `label`) que lanzan líneas de comandos cuando el cursor los pasa. Un `seek` no dispara los eventos saltados. `looped` repite; `audio` muestra la onda de un fichero. `timeline 1 run [from T]`, `stop`, `seek T`.
Con `tc_start` el cursor sigue el timecode del show (se detiene si el timecode se para; saltos mayores de 2 s cuentan como seek).

## Timecode
Ajustes → Timecode: fuente `off`, `mtc` (MIDI Time Code recibido por MIDI) o `internal` (reloj interno, se arranca desde la interfaz), `fps` 24/25/29.97/30 y desfase en fotogramas. Las cues con `timecode` (`HH:MM:SS:FF`) lanzan GO al pasar.

## MSC (MIDI Show Control)
Ajustes → MSC: activa, `device_id` (0–111, 127 = todos) y asigna los números de lista MSC a cuelists de Lumos. Atiende GO, STOP, RESUME, TIMED_GO, LOAD, ALL_OFF y RESET. Necesita MIDI con SysEx (en navegador hay que permitirlo; el MIDI nativo de la app de escritorio no tiene esa limitación).
