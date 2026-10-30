Un **universo** (512 canales) puede enviarse a varias **salidas** a la vez. Se configuran en Ajustes → Universos (`universe_save`).

## Tipos de salida
- **Art-Net**: `host` (vacío = broadcast), puerto (6454 por defecto), net/subnet/universe del port-address.
- **sACN (E1.31)**: número de universo, prioridad y unicast opcional (por defecto multicast `239.255.hi.lo`).
- **OSC**: host, puerto, patrón con `{universe}` y `{channel}`, modo por canal, blob o ambos.
- **Serie**: Enttec Open DMX o Enttec Pro / DMXking (elige el puerto serie).
- **USB (uDMX)** solo si el binario se compiló con la característica `usb`.

Cada salida se puede activar o desactivar (`enabled`). El motor envía también cuando la trama no cambia, para que los receptores no pierdan la señal.

## Comprobar
- Monitor (espacio 6): estado de cada salida (OK/Error con mensaje) y valores DMX por universo.
- `output_test`: pone todos los canales a un nivel, hace un chase de canal o fija un canal; **sustituye a la salida normal** hasta apagarlo. Es una acción en directo.
- Descubrimiento de nodos Art-Net: `nodes_scan`.
- Canales aparcados (`park U/C at V`) fijan un valor ignorando todo; se liberan con `unpark`.

## Entrada Art-Net
Ajustes → Art-Net in: se mapea un port-address recibido a un universo de Lumos y se mezcla por HTP.
