# Lumos 0.3 — Rust engine + web console

Professional theatrical lighting control. One binary serves the engine and the UI; open it from the desk,
a tablet or a phone on the same network.

```bash
./scripts/build.sh                                  # UI (Vite/Preact/three.js) + release binary
./target/release/lumos --port 8080 --bind 0.0.0.0 --data ./data
```

Dev: `cargo run -- --port 8080` and `cd ui && npm run dev` (proxies `/ws`, `/api`, `/media` to :8080).
Tests: `cargo test` (engine + I/O, regenerates `ui/src/gen`), `node scripts/e2e.mjs target/release/lumos` (boots the binary, drives WS/REST).

**Features** — Live programmer (groups, palettes, colour/position/beam, effects) · Desk · cuelists with tracking,
fades, follow and timecode · executors · fixture library + Open Fixture Library import · unlimited universes, each routed to
any number of outputs (Art-Net, sACN, OSC, Enttec Open/Pro/DMXking serial, uDMX with `--features usb`) · inputs: Web MIDI,
OSC, Art-Net, MTC with learn-mode bindings · 3D visualiser with trusses, decor and projection/LED screens
(image, video, URL, webcam, screen capture for NDI viewers) · DMX monitor.

See `ARCHITECTURE.md` (contract, engine semantics) and `docs/DESIGN_SYSTEM.md`.
