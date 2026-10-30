La biblioteca reúne los **perfiles** de aparato: lista de canales por modo, atributos que controlan, rangos (capacidades: ruedas de color, gobos, shutter…), ángulo de haz, rangos de pan/tilt y modelos 3D.

## Orígenes
- **Integrados**: perfiles genéricos (dimmer, RGB par, RGBW, cabeza móvil…) que siempre están.
- **GDTF** (`.gdtf`): formato estándar; lo usan MagicQ y MA3. Incluye modelos 3D, ruedas y óptica reales.
- **QLC+** (`.qxf`).
- **Open Fixture Library** (JSON de OFL).
- Los perfiles importados se guardan dentro del show (y sus recursos en la carpeta `library` de datos).
- No se admiten los formatos nativos de MagicQ (`heads.all`, `.hed`) ni MA2 XML.

## Importar
Patch → Importar aparatos: arrastra los ficheros. El resultado indica cuántos perfiles se importaron y los errores. Con el asistente, `library_import_ofl` importa desde OFL.

## Buscar y usar
- `search_library` busca por fabricante, modelo y categoría; `get_profile` muestra los modos y canales.
- Un perfil puede tener varios modos con distinto número de canales: elige el que coincida con el modo configurado en el propio aparato.
- Si un aparato apunta a un perfil o modo que no existe, aparece como «perfil desconocido» y no emite: en `get_fixture_state` se avisa.
- Eliminar un perfil importado (`library_remove`) deja huérfanos a los aparatos que lo usen.
