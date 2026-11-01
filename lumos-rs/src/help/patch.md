El **patch** asigna a cada aparato un perfil (qué canales tiene), un modo (cuántos canales usa), un universo y una dirección DMX (1–512).

## Añadir aparatos
- En Patch: «Añadir», elige el perfil de la biblioteca, el modo, la cantidad, el universo y (opcional) la dirección inicial. Si no das dirección, Lumos coloca los aparatos a continuación en el primer hueco libre.
<!--ai-->- Mediante el asistente: `search_library` para encontrar el perfil, `get_profile` para ver sus modos y después el comando `fixture_add` con `profile`, `mode`, `count`, `universe`, `address` y `name`.<!--/ai--><!--ui-->- Con el asistente de IA (opcional) basta pedirlo: busca el perfil, elige el modo y lo parchea.<!--/ui-->
- Importación masiva: CSV y MVR (Patch → Importar).

## Que no se pisen
Cada aparato ocupa `dirección … dirección + canales − 1`. Si dos rangos se cruzan en el mismo universo, ambos reciben los mismos canales y se comportan mal. <!--ai-->`check_patch` lista los solapes, direcciones fuera de 1–512, perfiles desconocidos y los huecos libres de cada universo; con `add` simula un alta.<!--/ai--> Con `address: null` Lumos usa siempre huecos libres. Las altas y movimientos del asistente que pisarían canales se rechazan con una sugerencia de hueco libre. Dos aparatos con la misma dirección y tamaño (clones) son legítimos si se hacen a propósito.

## Reglas
- Cada aparato ocupa tantos canales como indique su modo (`channels`). Dos aparatos pueden solaparse en direcciones (se mezcla por HTP por canal), pero casi siempre es un error: el diagnóstico lo avisa.
- Un **universo** es lógico (`id`, nombre, salidas). Los aparatos referencian el id; hace falta que el universo exista y tenga una salida activa para que algo salga por cable.
- Puedes **cambiar de modelo** conservando los valores con «morph»: previsualiza antes cómo se reasignan los atributos.
<!--ai-->- Cambiar nombre, posición en el escenario (`pos`, metros) y rotación (`rot`, grados) se hace con `fixture_update`.<!--/ai--><!--ui-->- Nombre, posición en el escenario (metros) y rotación (grados) se cambian en el panel del aparato.<!--/ui-->
- Eliminar aparatos los quita también de grupos, cues y escenas; es deshacible.

## Atajos de numeración
Los ids de aparato son números correlativos. Los comandos de selección (`fixture 1 thru 8`) usan esos ids, no la dirección DMX.

## Localizar
El botón **Localizar** de un aparato lo enciende en blanco/pleno de forma que se vea de verdad: sube el dimmer, el color (o blanco), abre shutter y enciende la lámpara si el perfil lo necesita. Si un perfil sigue sin encender, ajusta qué valores usa en el diálogo de Localizar: puedes fijar los valores a mano o copiarlos de lo que tiene ahora un aparato que sí funciona.
