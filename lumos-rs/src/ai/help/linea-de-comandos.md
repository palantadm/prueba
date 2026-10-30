La línea de comandos acepta órdenes de consola. Varias órdenes se separan con `;` o con saltos de línea y **comparten la selección** dentro del mismo texto. Las palabras clave funcionan en español e inglés, sin distinguir mayúsculas ni acentos. `# texto` es un comentario. Un error de sintaxis no ejecuta nada y avisa de la columna; si una orden falla, las siguientes no se ejecutan. El resultado de cada línea queda en el registro (`get_runtime` → `last_commands`).

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
macro 3 ; macro stop
timeline 1 run ; timeline 1 seek 30
blackout on ; gm 80 ; clear ; park 1/5 at 100 ; unpark
blind on ; undo ; redo
```

## Atributos y colores
Atributos (con alias): dimmer (dim, intensity), pan, tilt, red/rojo, green/verde, blue/azul, white/blanco, amber/ámbar, uv, lime, cto, colorwheel (rueda), gobo, gobo_rot, prism, prism_rot, frost, zoom, focus, iris, shutter (strobe), speed, effect, control. Colores con nombre: red, green, blue, white, yellow, cyan, magenta, orange, pink, purple, amber, warm, cool, black (y sus equivalentes en español).

## Consejos
- Valores de 0 a 100. `full`/`max`/`pleno` = 100, `out`/`off`/`cero` = 0.
- Después de grabar, `clear` evita que el programador siga mandando.
- Lo que no cubre la línea de comandos (parchear con dirección, ajustes, salidas) va con `send_command`.
