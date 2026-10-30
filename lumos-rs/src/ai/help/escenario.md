## Visualizador 3D (espacio 5)
Muestra los aparatos con haces volumétricos, proyección de gobos, sombras y modelos 3D de GDTF. Necesita aceleración gráfica (WebGL). La posición (`pos`, metros: x derecha del escenario, y arriba, z hacia el público) y la rotación (`rot`, grados) de cada aparato se editan en Patch o en el propio visualizador.

## Escenario
El escenario (`stage_save`) tiene tamaño (ancho, fondo y altura de parrilla, metros) y objetos: truss, tarima, pared, columna, telón, árbol, caja y pantallas. Las pantallas pueden ser de proyección o LED y mostrar color, imagen, vídeo, URL, webcam o captura de pantalla (`media`).

## Planta 2D (espacio 8)
Vista en planta, alzado frontal o lateral con huellas de haz en el suelo y haces de móviles; se puede arrastrar para apuntar (cambia pan/tilt en el programador).

## Usar con el asistente
`get_show` con `section: "stage"` lista los objetos. Cambiar posiciones de aparatos: `fixture_update`. Cambiar el escenario: `stage_save` con el escenario completo.
