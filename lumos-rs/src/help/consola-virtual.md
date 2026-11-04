La **consola virtual** (espacio 9) son páginas de widgets táctiles que se guardan en el show.

- Cada página tiene `cols` × `rows`; cada widget tiene `kind`, `label`, posición (`x`, `y`) y tamaño (`w`, `h`) en la rejilla y una `action`.
- Widgets: botones, faders, knobs, pads XY, y widgets de lista de cues y velocidad (speed dial).
- Las acciones pueden lanzar cuelists, escenas, macros, ajustar masters, atributos, etc.
- Se pueden asignar a MIDI/OSC (aprender) con feedback a LEDs y faders motorizados.
- Funciona bien en tablet: abre la misma dirección de red del servidor (menú Ayuda → «Dirección para tablet o móvil…» en la app de escritorio).

<!--ai-->Para modificarla desde el asistente: `get_show` con `section: "vconsole"` muestra las páginas y widgets; `send_command` con `vc_save` guarda la consola completa (envía siempre la estructura entera, no un cambio parcial). Pulsar, mover o soltar widgets sin editar la consola se hace con `vc_press`, `vc_value` y `vc_xy`.<!--/ai-->
