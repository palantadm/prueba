La línea de comandos acepta órdenes de consola. Varias órdenes se separan con `;` o con saltos de línea y **comparten la selección** dentro del mismo texto. Las palabras clave funcionan en español e inglés, sin distinguir mayúsculas ni acentos. `# texto` es un comentario. Un error de sintaxis no ejecuta nada y avisa de la columna; si una orden falla, las siguientes no se ejecutan. El resultado de cada línea queda en el registro de la consola.

## Gramática
```cmd
fixture 1 thru 10 + 15 - 4        # selección (aparato|fx|aparato)
group 2 ; all ; none ; odd ; even ; next ; prev
1 thru 8                          # rango suelto
fixture 1 thru 4 at 50            # dimmer por defecto; también @ y en
dimmer 1 thru 8 at 70
red at 100 ; pan at 30
fixture 1 thru 8 at 0 thru 100    # abanico
fixture 3 colour red              # colour|color + nombre, o: colour 100 40 0 (porcentajes)
fixture 4 at full ; at out ; at half ; at home
fixture 4 locate
record cue 3 name Entrada
record cue 5 cuelist 2 name Fondo replace
record scene 2 Azul output
record palette color 3 Ámbar
fixture 1 thru 4 ; record group 5 Frontales
update cue 3
delete cue 4 ; delete scene 2
group 1 apply palette 3
go ; back ; pause ; release
cuelist 2 ; go
goto cue 10
scene 1 on ; scene 3 at 60 ; scene solo 2 ; scenes release
chase 1 on ; chase 2 bpm 128
effect 5 on ; effect 5 off                        # efectos guardados
exec 5 go ; exec 5 back ; exec 5 flash on ; exec 5 at 80   # executors
macro 3 ; macro stop
timeline 1 run ; timeline 1 seek 30
blackout on ; gm 80 ; clear ; park 1/5 at 100 ; unpark
blind on ; undo ; redo
script 1 go ; script 1 goto 12 ; script follow on     # Guion (script|guion|escaleta)
osc 192.168.1.20:53000 /cue/1/start                   # OSC a otra app; args: 12 int, 0.5 float, true/false, texto
midi note 1 60 100 ; midi cc 2 7 64 ; midi pc 1 12    # canal 1-16, número, valor
http get http://127.0.0.1:8080/status                 # http get|post|put|patch|delete <url> [cuerpo]
```

## Atributos y colores
Atributos (con alias): dimmer (dim, intensity), pan, tilt, red/rojo, green/verde, blue/azul, white/blanco, amber/ámbar, uv, lime, cto, colorwheel (rueda), gobo, gobo_rot, prism, prism_rot, frost, zoom, focus, iris, shutter (strobe), speed, effect, control. Colores con nombre: red, green, blue, white, yellow, cyan, magenta, orange, pink, purple, amber, warm, cool, black (y sus equivalentes en español).

## Consejos
- Valores de 0 a 100. `full`/`max`/`pleno` = 100, `out`/`off`/`cero` = 0.
- Después de grabar, `clear` evita que el programador siga mandando.
<!--ai-->- La línea de comandos **no parchea** ni configura universos, salidas, ajustes o perfiles: eso va con `send_command` (`fixture_add`, `universe_save`, `settings_save`…). Para grabar un cue complejo con tiempos, `cue_save`/`cuelist_save` por `send_command`.<!--/ai--><!--ui-->- La línea de comandos **no parchea** ni configura universos, salidas, ajustes o perfiles: eso se hace en Patch y en Ajustes.<!--/ui-->
- Un aparato sin seleccionar (`red at 100` sin selección) da error: selecciona antes (`fixture 1 thru 4`, `group 2`, `all`).
