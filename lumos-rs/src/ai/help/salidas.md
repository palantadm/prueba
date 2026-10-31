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

## Art-Net que no funciona — método
Llama a `diagnose_outputs` (re-escanea nodos y revisa toda la cadena). Causas habituales, por orden de frecuencia:
1. **Port-address distinto**: el nodo escucha Net/Sub/Uni X y Lumos envía a Y. Los nodos que contestan a ArtPoll muestran sus puertos de salida; hay que igualarlos. Ojo con numerar desde 0 o desde 1: Lumos universo 1 con Net 0/Sub 0/Uni 0 sale como port-address 0.
2. **Subred distinta**: PC 192.168.1.x y nodo 2.x.x.x o 10.x.x.x (o máscara diferente). Pon IP fija al adaptador del PC en la red del nodo (Art-Net clásico: 2.x.x.x/255.0.0.0; 10.x.x.x/255.0.0.0).
3. **Broadcast por el adaptador equivocado** (Wi-Fi + cable, VPN, VM). Solución: IP del nodo en `host` (unicast).
4. **Firewall**: UDP 6454 entrada y salida. En Windows, permitir Lumos en red privada.
5. **Puerto ≠ 6454** o IP mal escrita; **cable/switch** (los LED del nodo parpadean al recibir datos).
6. El nodo **no contesta a ArtPoll** (algunos baratos): no es concluyente; usa `output_test` a un nivel bajo y mira si el aparato reacciona.
7. Otro programa usando el puerto 6454 (error en la entrada Art-Net o en la salida).
Arreglo: `universe_save` con la salida corregida (cambia solo lo necesario). Verificación: `output_test` (nivel fijo) + `get_dmx`, y apagar el test al final.

## sACN
Multicast `239.255.hi.lo` necesita IGMP en el switch; en Wi-Fi suele fallar → usa unicast. Universo 1–63999, prioridad 1–200 (100 típico). Cada universo sACN solo puede lo use un universo de Lumos.

## Serie / USB
El puerto debe aparecer entre los puertos detectados (driver FTDI/CH340 instalado; en Linux, grupo `dialout`/`uucp`). Enttec Open DMX y Enttec Pro/DMXking son drivers distintos: elegir el equivocado da salida muda.
