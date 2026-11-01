## Guardado
- El show vivo se **autoguarda** a los pocos segundos de cada cambio (`autosave.json`) y se restaura al arrancar.
- «Guardar» escribe un fichero con nombre en la carpeta `shows` de los datos; «Abrir» carga otro (¡sustituye el show actual!); «Renombrar» lo renombra.
- Los datos viven en la carpeta que se pasa con `--data` (en la app de escritorio, `Documentos/Lumos`).

## Copias de seguridad
«Copia de seguridad» hace una copia manual; hay copias automáticas. «Restaurar» recupera una (sustituye el show actual: acción destructiva).

## Importar / exportar
- Descargar el show actual: `GET /api/show`; importar: `PUT /api/show`.
- Patch: importar CSV y MVR; informes (PDF/CSV) en el espacio de Patch.
- Perfiles de aparato: ver tema `biblioteca`.

## Deshacer
Los cambios en los datos del show (aparatos, grupos, paletas, escenas, cuelists, ajustes…) se pueden deshacer y rehacer (hasta 100 pasos). Lo que cambia el estado en directo (GO, blackout, master, valores del programador) no tiene deshacer. Cargar un show o restaurar una copia reinicia el estado de reproducción.
