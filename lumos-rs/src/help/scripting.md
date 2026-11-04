## Para qué sirve
Los **scripts Lua** automatizan lo que los macros no pueden: condiciones, bucles, cálculos, leer el estado de la consola y reaccionar. Se escriben en Ajustes → **Scripts (Lua)**.

Un script se puede lanzar:
- a mano (botón Ejecutar),
- desde un **botón de la consola virtual** (acción «Script Lua»),
- desde una **marca de cue del guion** o un macro (acción Lua),
- **al cargar el show** (casilla «Al abrir el show»),
- **periódicamente** (cada N segundos, con «Activo»),
- o programado desde otro script con `lumos.after` y `lumos.every`.

## Cómo corre
El script se ejecuta **dentro del motor y de una vez**: no puede esperar (no hay `sleep`). Para hacer algo más tarde usa `lumos.after(segundos, "código")`. Si tarda más de **250 ms** se detiene con un error (por ejemplo un bucle infinito); la memoria se limita a 32 MB. No tiene acceso a archivos, al sistema ni a la red, salvo lo que ofrezca `lumos`.

## Lo que ofrece `lumos`
- `lumos.cmd("go cuelist 1; blackout off")` → ejecuta líneas de la consola. Devuelve `true` o `false, "mensaje"`. Con `osc`, `midi` y `http` de la consola se habla con otros programas.
- `lumos.send({type="grand_master", level=0.5})` → manda un comando interno como tabla (el asistente de IA conoce la lista completa). Devuelve `true` o `false, "mensaje"`.
- `lumos.fixtures()` → lista de aparatos `{id, name, profile, universe, address}`.
- `lumos.attr(aparato, "dimmer")` → valor actual 0–1 o `nil`.
- `lumos.grandmaster()`, `lumos.blackout()`, `lumos.exec_level(id)`, `lumos.cue(cuelist)` (número del cue actual o `nil`).
- `lumos.time()` (segundos desde que arrancó) y `lumos.date()` (`{year, month, day, hour, min, sec, wday}`, wday 1 = lunes).
- `lumos.get("clave")` / `lumos.set("clave", valor)` → memoria que **se conserva entre ejecuciones** (números, textos, booleanos y tablas sencillas). `lumos.set("clave", nil)` la borra. No se guarda en el show.
- `lumos.after(segundos, "código o id")`, `lumos.every(segundos, "código o id")` (mínimo 0,1 s) y `lumos.cancel()`.
- `lumos.osc("host", puerto, "/dirección", argumentos…)` → mensaje OSC directo.
- `print(…)` y `lumos.log(…)` → escriben en el registro del script.

## Ejemplos
Apagón automático a las 23:00 (script periódico, cada 30 s):
```lua
local d = lumos.date()
if d.hour == 23 and d.min == 0 and not lumos.get("hecho") then
  lumos.cmd("blackout on")
  lumos.set("hecho", true)
elseif d.min ~= 0 then
  lumos.set("hecho", nil)
end
```
Parpadeo de tres golpes con temporizadores:
```lua
for i = 0, 2 do
  lumos.after(i * 1.0, "lumos.cmd('blackout on')")
  lumos.after(i * 1.0 + 0.5, "lumos.cmd('blackout off')")
end
```
Subir un grupo según la hora:
```lua
local nivel = lumos.date().hour >= 20 and 30 or 80
lumos.cmd("group 1 at " .. nivel)
```

## Consejos
- Los errores salen en el registro con el número de línea. Usa `print` para depurar.
- Un script que llama a otro (por un macro, por ejemplo) se corta a los 4 niveles.
- Los comandos se ejecutan **en el momento** de la llamada, así que después puedes leer su efecto con `lumos.attr`.
<!--ai-->Por protocolo: `lua_save`, `lua_run`, `lua_eval`, `lua_remove`, `lua_stop`; el registro y los errores están en `get_runtime` → `lua.log`. Al probar código nuevo usa `lua_eval` y lee el registro antes de guardarlo. Acción de marca de guion: `{"type":"lua","id":N}`; acción de consola virtual: `{"type":"lua","script":N}`.<!--/ai-->
