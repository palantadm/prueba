Lumos es una consola de iluminación escénica: un motor en Rust (44 Hz por defecto) y una consola web que se abre desde la mesa, una tablet o el móvil en la misma red.

## Flujo de trabajo típico
1. **Patch** (espacio 4): añade aparatos eligiendo perfil, modo, universo y dirección DMX. Los perfiles vienen de la biblioteca (GDTF, QLC+, Open Fixture Library) — tema `biblioteca`.
2. **Salidas** (Ajustes → Universos): cada universo puede enviar a Art-Net, sACN, OSC, serie (Enttec) o USB — tema `salidas`.
3. **Live** (espacio 1): selecciona aparatos, ponles valores en el programador, crea **grupos** y **paletas** — temas `programador`, `grupos-paletas`.
4. **Grabar**: el programador se graba en **cues** de una cuelist, en **escenas** o en paletas — temas `cuelists`, `escenas`.
5. **Ejecutar**: GO en una cuelist, escenas y chases en los **executors** (Desk, espacio 2) o en la consola virtual — temas `executors`, `consola-virtual`.
6. **Automatizar**: macros, timelines, timecode, MIDI/OSC — temas `macros-timelines`, `entradas`.
7. **Guardar**: los shows se guardan en la carpeta de datos; hay autoguardado y copias de seguridad — tema `shows`.

## Ideas clave
- Los valores de atributo van de 0 a 100 % (0–1 internamente).
- El programador manda sobre todo lo demás hasta que lo liberas (`clear`).
- Master general y blackout actúan al final de la cadena y afectan a todo.
- Todo lo que hace la interfaz se puede hacer con la línea de comandos (`linea-de-comandos`).
- Si algo no se ilumina: tema `diagnostico`.
