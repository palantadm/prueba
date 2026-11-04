## Guardado
- El show vivo se **autoguarda** a los pocos segundos de cada cambio (`autosave.json`) y se restaura al arrancar.
- «Guardar» escribe un fichero con nombre en la carpeta `shows` de los datos; «Abrir» carga otro (¡sustituye el show actual!); «Renombrar» lo renombra.
- Los datos viven en la carpeta que se pasa con `--data` (en la app de escritorio, `Documentos/Lumos`).

## Copias de seguridad
«Copia de seguridad» hace una copia manual; hay copias automáticas. «Restaurar» recupera una (sustituye el show actual: acción destructiva).

## Importar / exportar
- Descargar el show actual: `GET /api/show`; importar: `PUT /api/show`.
- Patch: importar CSV y MVR; informes (PDF/CSV) en el espacio de Patch.
- **Plano de escenario (stage plot)**: informe PDF con la planta y el alzado del rig (símbolos por tipo de aparato —cabeza móvil, color/LED, convencional— con su ID, elementos de escenario, rejilla de 1 m, público) y la lista de aparatos con universo, dirección y altura. Se saca de los informes del espacio de Patch. Lo dibuja con las posiciones del escenario: colócalas antes en el espacio 3D o 2D.
- **Con ChamSys MagicQ / MagicVis**: intercambia el rig por **MVR** (exporta de Lumos e importa en MagicQ, o al revés) y los perfiles por GDTF; las posiciones del plano viajan en el MVR.
- Perfiles de aparato: ver tema `biblioteca`.

## Deshacer
Los cambios en los datos del show (aparatos, grupos, paletas, escenas, cuelists, ajustes…) se pueden deshacer y rehacer (hasta 100 pasos). Lo que cambia el estado en directo (GO, blackout, master, valores del programador) no tiene deshacer. Cargar un show o restaurar una copia reinicia el estado de reproducción.

## Fusionar partes de otro show
**Show → Fusionar desde otro show…**: eliges un show guardado y qué traer: patch, grupos, paletas, escenas, cuelists, chases, efectos, macros, timelines, guiones o los ajustes de Localizar. Antes de fusionar ves qué se va a añadir y los avisos. Es **un solo paso de deshacer** (Ctrl+Z).
- Los números que chocan se **renumeran** y lo que apunta a ellos los sigue: los valores de los aparatos, los enlaces a paletas, las escenas de los pasos de un chase, los master de velocidad y los enlaces de las marcas del guion a cuelists.
- Si no fusionas el patch, solo valen los aparatos que tengan el **mismo número y el mismo perfil** en ambos shows; los demás valores se descartan (se avisa).
- Los aparatos fusionados que caigan en direcciones ocupadas se **recolocan** en el primer hueco libre (revisa el patch).
- **No se reescriben** las macros, timelines ni acciones del guion que llaman a cues, escenas o grupos por número (por ejemplo `go cuelist 2`): si hubo renumeraciones, se avisa para que las revises.
<!--ai-->Por protocolo: `show_merge {name, parts[]}`.<!--/ai-->

## Intercambio con otros programas
Menú del show → Datos:
- **MVR** (estándar): el rig con posiciones y GDTF. Lo leen grandMA3, ChamSys MagicQ, Vectorworks, Capture, Depence y otros.
- **CSV del patch**: lista de aparatos para hojas de cálculo.
- **QLC+** (`.qxw`): exporta e importa el patch, las escenas y los chasers. Las cuelists salen como chasers de escenas completas (el tracking se resuelve antes). Al importar, los aparatos se emparejan con la biblioteca por fabricante y modelo; los que no coinciden se listan para que importes su definición. «Reemplazar» sustituye aparatos, escenas y chases actuales.
- **GDTF** y definiciones de QLC+ (`.qxf`): perfiles de aparato desde Patch → Importar.
- Los formatos propios de ChamSys (`.shw`), grandMA (`.show`/XML) y Freestyler no se pueden leer ni escribir: son cerrados. Para pasar el rig usa MVR. Si necesitas más, envía un archivo de ejemplo exportado por ese programa y se puede estudiar.
<!--ai-->Estos intercambios se hacen desde la interfaz (no hay comando de protocolo): indica al usuario dónde está el menú.<!--/ai-->
