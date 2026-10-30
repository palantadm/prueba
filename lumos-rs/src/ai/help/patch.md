El **patch** asigna a cada aparato un perfil (qué canales tiene), un modo (cuántos canales usa), un universo y una dirección DMX (1–512).

## Añadir aparatos
- En Patch: «Añadir», elige el perfil de la biblioteca, el modo, la cantidad, el universo y (opcional) la dirección inicial. Si no das dirección, Lumos coloca los aparatos a continuación en el primer hueco libre.
- Mediante el asistente: `search_library` para encontrar el perfil, `get_profile` para ver sus modos y después el comando `fixture_add` con `profile`, `mode`, `count`, `universe`, `address` y `name`.
- Importación masiva: CSV y MVR (Patch → Importar).

## Reglas
- Cada aparato ocupa tantos canales como indique su modo (`channels`). Dos aparatos pueden solaparse en direcciones (se mezcla por HTP por canal), pero casi siempre es un error: el diagnóstico lo avisa.
- Un **universo** es lógico (`id`, nombre, salidas). Los aparatos referencian el id; hace falta que el universo exista y tenga una salida activa para que algo salga por cable.
- Puedes **cambiar de modelo** conservando los valores con «morph» (`fixture_morph`): previsualiza antes cómo se reasignan los atributos.
- Cambiar nombre, posición en el escenario (`pos`, metros) y rotación (`rot`, grados) se hace con `fixture_update`.
- Eliminar aparatos (`fixture_remove`) los quita también de grupos, cues y escenas; es deshacible.

## Atajos de numeración
Los ids de aparato son números correlativos. Los comandos de selección (`fixture 1 thru 8`) usan esos ids, no la dirección DMX.
