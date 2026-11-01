## Macros
Una macro es una lista de pasos (`steps`); cada paso es un texto de la línea de comandos más una espera (`wait`, segundos). Se ejecutan sin bloquear el motor, pueden anidarse hasta 8 niveles y, si un paso falla, la macro se aborta.
- Las macros se crean y borran en su editor; se lanzan con `macro 3` y se paran con `macro stop`.<!--ai--> Por protocolo: `macro_save` / `macro_remove`.<!--/ai-->
- Se pueden poner en un botón de la consola virtual.

## Timelines
Una línea de tiempo (`length` en segundos) tiene eventos (`at`, `text`, `label`) que lanzan líneas de comandos cuando el cursor los pasa. Un `seek` no dispara los eventos saltados. `looped` repite; `audio` muestra la onda de un fichero. `timeline 1 run [from T]`, `stop`, `seek T`.
Con `tc_start` el cursor sigue el timecode del show (se detiene si el timecode se para; saltos mayores de 2 s cuentan como seek).

## Timecode
Ajustes → Timecode: fuente `off`, `mtc` (MIDI Time Code recibido por MIDI) o `internal` (reloj interno, se arranca desde la interfaz), `fps` 24/25/29.97/30 y desfase en fotogramas. Las cues con `timecode` (`HH:MM:SS:FF`) lanzan GO al pasar.

## MSC (MIDI Show Control)
Ajustes → MSC: activa, `device_id` (0–111, 127 = todos) y asigna los números de lista MSC a cuelists de Lumos. Atiende GO, STOP, RESUME, TIMED_GO, LOAD, ALL_OFF y RESET. Necesita MIDI con SysEx (en navegador hay que permitirlo; el MIDI nativo de la app de escritorio no tiene esa limitación).

## Programación (calendario y reloj astronómico)
**Ajustes → Programación** ejecuta una línea de consola a una hora, al **amanecer** o al **atardecer**, en los días de la semana que elijas: luces de entrada al caer el sol, una escena todos los días laborables a las 8:00, un apagado general a medianoche…
- **Ubicación:** para amanecer y atardecer hay que dar latitud y longitud (o «Usar mi ubicación»). Se calcula con el algoritmo solar de la NOAA, con precisión de un par de minutos. En latitudes polares puede no haber amanecer ese día y la regla no se ejecuta.
- **Reglas:** nombre, cuándo (hora, amanecer ± minutos o atardecer ± minutos), días, y la línea de consola a ejecutar (`scene 4 on`, `go cuelist 2`, `macro 3`, `blackout off`, `sound "aviso.wav"`). Cada regla se ejecuta **una vez** en su minuto. La lista muestra cuándo será la próxima.
- Lo hace el **motor**, no la pantalla: funciona con la interfaz cerrada, pero **Lumos tiene que estar en marcha** y el equipo con la hora correcta. Si el equipo estaba apagado a esa hora, no se recupera después. Los fallos de una regla quedan en el registro de la consola.
<!--ai-->Por protocolo: `schedule_save {rule}` / `schedule_remove {id}`; la ubicación va en `settings_save` → `location {lat, lon}`.<!--/ai-->
