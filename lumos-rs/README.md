# Lumos 0.3 — Rust engine + web console

Professional theatrical lighting control. One binary serves the engine and the UI; open it from the desk,
a tablet or a phone on the same network.

## Instalar (aplicación de escritorio)

Los instaladores se generan solos en GitHub (pestaña **Actions → "Lumos desktop installers" → última ejecución → Artifacts**):
`lumos-Windows` (`.exe`/`.msi`), `lumos-macOS` (`.dmg`, universal Intel + Apple Silicon), `lumos-Linux` (`.deb`/`.AppImage`).
Con una etiqueta `v*` (p. ej. `git tag v0.3.0 && git push --tags`) se publican en **Releases**.
Los instaladores no están firmados: Windows mostrará SmartScreen («Más información → Ejecutar de todos modos») y macOS pedirá
clic derecho → Abrir la primera vez.

La app abre su propia ventana, guarda shows/medios/librerías en `Documentos/Lumos`, usa MIDI nativo (sin depender del navegador)
y sirve la misma consola en red (menú Ayuda → «Dirección para tablet o móvil…») para controlarla desde una tablet.
Si no se ve el 3D: la ventana necesita aceleración gráfica (WebGL); el 2D y el resto funcionan igualmente.

Build local: `cd desktop && npm ci && npx tauri build` (Linux: `libwebkit2gtk-4.1-dev libasound2-dev libgtk-3-dev librsvg2-dev libxdo-dev patchelf`).

## Servidor sin ventana / desarrollo

```bash
./scripts/build.sh                                  # UI (Vite/Preact/three.js) + release binary
./target/release/lumos --port 8080 --bind 0.0.0.0 --data ./data
```

Dev: `cargo run -- --port 8080` and `cd ui && npm run dev` (proxies `/ws`, `/api`, `/media` to :8080).
Tests: `cargo test` (engine + I/O, regenerates `ui/src/gen`), `node scripts/e2e.mjs target/release/lumos` (boots the binary, drives WS/REST).

**Features** — Live programmer (groups, palettes, colour/position/beam, effects) · Desk · cuelists with tracking,
fades, follow and timecode · executors · **fixture libraries: GDTF (MagicQ/MA3/GDTF-Share, with real 3D models, wheels, optics),
QLC+ `.qxf`, Open Fixture Library JSON** (Patch → Import fixtures) · unlimited universes, each routed to any number of outputs
(Art-Net, sACN, OSC, Enttec Open/Pro/DMXking serial, uDMX with `--features usb`) · inputs: Web MIDI, OSC, Art-Net, MTC ·
**Virtual Console**: pages of buttons, sliders, knobs, XY pads, cue-list and speed-dial widgets mapped to MIDI/OSC/touch with
MIDI-LED / motor-fader / OSC feedback and learn mode · **3D visualiser** (PBR fixtures, volumetric beams, gobo projection, shadows,
GDTF models, trusses/decor/LED & projection screens) · **2D plan/elevation visualiser** (moving-head beams and floor footprints,
drag-to-aim) · DMX monitor.

Library note: MagicQ and MA3 both exchange fixtures as GDTF. MagicQ's native `heads.all`/`.hed` and MA2's XML are not supported.

See `ARCHITECTURE.md` (contract, engine semantics) and `docs/DESIGN_SYSTEM.md`.
