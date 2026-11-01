La biblioteca reúne los **perfiles** de aparato: lista de canales por modo, atributos que controlan, rangos (capacidades: ruedas de color, gobos, shutter…), ángulo de haz, rangos de pan/tilt y modelos 3D.

## Orígenes
- **Integrados**: perfiles genéricos (dimmer, RGB par, RGBW, cabeza móvil…) que siempre están.
- **GDTF** (`.gdtf`): formato estándar; lo usan MagicQ y MA3. Incluye modelos 3D, ruedas y óptica reales.
- **QLC+** (`.qxf`).
- **Open Fixture Library** (JSON de OFL).
- Los perfiles importados se guardan dentro del show (y sus recursos en la carpeta `library` de datos).
- No se admiten los formatos nativos de MagicQ (`heads.all`, `.hed`) ni MA2 XML.

## Importar
Patch → Importar aparatos: arrastra los ficheros. El resultado indica cuántos perfiles se importaron y los errores. <!--ai-->Con el asistente, `library_import_ofl` importa desde OFL.<!--/ai-->

## Buscar y usar
<!--ai-->- `search_library` busca por fabricante, modelo y categoría; `get_profile` muestra los modos y canales.<!--/ai--><!--ui-->- En Patch → Añadir se busca por fabricante, modelo y categoría; al elegir el perfil se ven sus modos y canales.<!--/ui-->
- Un perfil puede tener varios modos con distinto número de canales: elige el que coincida con el modo configurado en el propio aparato.
- Si un aparato apunta a un perfil o modo que no existe, aparece como «perfil desconocido» y no emite.<!--ai--> `get_fixture_state` lo avisa.<!--/ai-->
- Eliminar un perfil importado deja huérfanos a los aparatos que lo usen.

## Editor de perfiles
**Patch → «Editor de perfiles…»** sirve para un aparato que no está en la biblioteca o para corregir uno propio. Puedes **crear uno nuevo** (con una plantilla RGB de 4 canales) o **duplicar uno de la biblioteca** y modificarlo; los perfiles de la biblioteca de Lumos no se pueden sobrescribir.
- **Datos:** fabricante, modelo, id (minúsculas, tipo `acme/par-led`; no se cambia después), categoría, potencia, ángulo del haz (mínimo y máximo) y rangos de pan/tilt (0 = fijo).
- **Modos:** un perfil tiene uno o varios (p. ej. 3ch, 4ch, 8ch). Se pueden añadir, duplicar y quitar; no se puede quitar un modo que esté en uso por aparatos parcheados.
- **Canales de cada modo:** canal DMX, atributo (dimmer, red, pan, colorwheel, gobo…), canal fino opcional (16 bits), valor por defecto, invertir y **rangos** (huecos de la rueda de color, gobos, zonas de shutter/estrobo; hay botones para dividir en huecos iguales).
- Al guardar se comprueba todo (canales repetidos, rangos mal formados…) y se avisa de cada error. Después el perfil sale en «Añadir» como cualquier otro, y se guarda con el show.
<!--ai-->Por protocolo: `profile_save {profile}`. El perfil usa `offset` desde 0 (canal 1 = 0).<!--/ai-->
