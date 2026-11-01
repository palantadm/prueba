Un **universo** (512 canales) puede enviarse a varias **salidas** a la vez. Se configuran en Ajustes → Universos.

## Tipos de salida
- **Art-Net**: `host` (vacío = broadcast), puerto (6454 por defecto), net/subnet/universe del port-address.
- **sACN (E1.31)**: número de universo, prioridad y unicast opcional (por defecto multicast `239.255.hi.lo`).
- **OSC**: host, puerto, patrón con `{universe}` y `{channel}`, modo por canal, blob o ambos.
- **Serie**: Enttec Open DMX o Enttec Pro / DMXking (elige el puerto serie).
- **USB (uDMX)** solo si el binario se compiló con la característica `usb`.

Cada salida se puede activar o desactivar (`enabled`). El motor envía también cuando la trama no cambia, para que los receptores no pierdan la señal.

## Comprobar
- Monitor (espacio 6): estado de cada salida (OK/Error con mensaje) y valores DMX por universo.
- **Test de salida** (Ajustes → Universos y salidas): pone todos los canales a un nivel, hace un chase de canal o fija un canal; **sustituye a la salida normal** hasta apagarlo. Es una acción en directo.
- Descubrimiento de nodos Art-Net: botón **Escanear** (Ajustes → Universos y salidas).
- Canales aparcados (`park U/C at V`) fijan un valor ignorando todo; se liberan con `unpark`.

## Entrada Art-Net
Ajustes → Art-Net in: se mapea un port-address recibido a un universo de Lumos y se mezcla por HTP.

## Art-Net que no funciona — método
<!--ai-->Llama a `diagnose_outputs` (re-escanea nodos y revisa toda la cadena). Causas habituales, por orden de frecuencia:<!--/ai--><!--ui-->Revisa en este orden (Ajustes → Universos y salidas y Monitor):<!--/ui-->
1. **Port-address distinto**: el nodo escucha Net/Sub/Uni X y Lumos envía a Y. Los nodos que contestan a ArtPoll muestran sus puertos de salida; hay que igualarlos. Ojo con numerar desde 0 o desde 1: Lumos universo 1 con Net 0/Sub 0/Uni 0 sale como port-address 0.
2. **Subred distinta**: PC 192.168.1.x y nodo 2.x.x.x o 10.x.x.x (o máscara diferente). Pon IP fija al adaptador del PC en la red del nodo (Art-Net clásico: 2.x.x.x/255.0.0.0; 10.x.x.x/255.0.0.0).
3. **Broadcast por el adaptador equivocado** (Wi-Fi + cable, VPN, VM). Solución: IP del nodo en `host` (unicast).
4. **Firewall**: UDP 6454 entrada y salida. En Windows, permitir Lumos en red privada.
5. **Puerto ≠ 6454** o IP mal escrita; **cable/switch** (los LED del nodo parpadean al recibir datos).
6. El nodo **no contesta a ArtPoll** (algunos baratos): no es concluyente; usa el Test de salida a un nivel bajo y mira si el aparato reacciona.
7. Otro programa usando el puerto 6454 (error en la entrada Art-Net o en la salida).
<!--ai-->Arreglo: `universe_save` con la salida corregida (cambia solo lo necesario). Verificación: `output_test` (nivel fijo) + `get_dmx`, y apagar el test al final.<!--/ai--><!--ui-->Arreglo: corrige la salida en Ajustes → Universos y salidas. Verificación: Test de salida (nivel fijo) y Monitor; apaga el test al final.<!--/ui-->

## RDM (configurar los aparatos desde la consola)
RDM va por la misma línea DMX y deja hablar con los aparatos: ver quién hay, cambiar su **dirección**, su **nombre** y su **modo**, y hacerlos parpadear para localizarlos. Aquí funciona a través de nodos Art-Net con RDM.
- Ajustes → **RDM** → **Escanear**. Aparece una tabla con cada aparato (fabricante, modelo, dirección, modo, canales) y a cuál del patch corresponde.
- «Repetir el descubrimiento» hace que los nodos vuelvan a buscar en la línea; es más lento, úsalo si falta alguno.
- **Identificar** hace parpadear el aparato físico; vuelve a pulsar para apagarlo.
- Cambia la dirección o el modo escribiendo en la tabla: el valor mostrado es el que el aparato confirma tras el cambio, no el que pediste.
- Si cambias el modo, el número de canales cambia: revisa el patch para que no se pisen.
- Un aparato que deja de contestar se marca «Sin respuesta» tras tres intentos; el resto sigue funcionando.
- Hace falta una salida Art-Net hacia el nodo, y que el nodo y los aparatos soporten RDM.
<!--ai-->- Por protocolo: `rdm_scan`, `rdm_refresh`, `rdm_identify`, `rdm_set_address`, `rdm_set_label`, `rdm_set_personality`, `rdm_clear`. El resultado se lee en `get_runtime` → `rdm` (devices con uid, address, personality, footprint, fixture = id del patch que coincide, error, status). Tras `rdm_scan` espera unos segundos (`wait`) y vuelve a leer. Si cambias dirección o modo, ajusta también el patch con `fixture_update` y comprueba solapes con `check_patch`.<!--/ai-->

## sACN
Multicast `239.255.hi.lo` necesita IGMP en el switch; en Wi-Fi suele fallar → usa unicast. Universo 1–63999, prioridad 1–200 (100 típico). Cada universo sACN solo puede lo use un universo de Lumos.

## Serie / USB
El puerto debe aparecer entre los puertos detectados (driver FTDI/CH340 instalado; en Linux, grupo `dialout`/`uucp`). Enttec Open DMX y Enttec Pro/DMXking son drivers distintos: elegir el equivocado da salida muda.
