## Grupos
Un grupo es una lista con nombre de aparatos (`id`, `name`, `fixtures`).
- Crear desde la selección: `fixture 1 thru 4; record group 5 Frontales`.
<!--ai-->- Por protocolo: `group_save` con el grupo completo; `group_remove` para borrar.<!--/ai-->
- Usarlos: `group 5 at 70`, o como selección base para paletas y executors de grupo.

## Paletas
Una paleta guarda valores reutilizables de un tipo: **intensidad, posición, color, haz** o **todo** (`kind`). Se aplican a cualquier aparato que tenga esos atributos.
- Grabar: selecciona, pon valores y `record palette color 3 Ámbar` (el tipo filtra qué atributos se guardan).
- Aplicar: `group 1 apply palette 3`.
- Valores universales (`values`) más ajustes por aparato (`per_fixture`), que tienen prioridad.
- **Enlaces**: cuando una cue o escena se graba con paletas aplicadas, guarda una referencia (`refs`); si luego editas la paleta, todas las cues que la usan cambian. Para congelar los valores graba sin referencias.
- Editar o borrar paletas: desde el panel de paletas.<!--ai--> Por protocolo: `palette_save` / `palette_remove`.<!--/ai-->

## Consejo
Para looks reutilizables: crea primero grupos (frontales, contras, fondo) y paletas de color y posición; después graba cues combinándolas. Cambiar un color en la paleta lo actualiza en todo el show.
