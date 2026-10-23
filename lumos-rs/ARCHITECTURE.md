# Lumos 0.3 — architecture

Rewrite of Lumos as a **Rust engine + web console**. The Python/PySide6 prototype in `../lumos` is superseded.

```
 Browser (TypeScript/Preact/three.js) ──ws /ws──▶ Rust server (axum)
   Web MIDI ─ MidiIn ─────────────────────────────▶  engine task  (44 Hz tick, owns all state)
   3D visualiser ◀─ binary DMX frames ────────────     ├─ io::IoHub  ─▶ Art-Net · sACN · OSC · Serial/USB
   Tablets / phones (same URL on LAN)                   └─ inputs: OSC in · Art-Net in · (MTC via MidiIn)
```

Why: deterministic real-time (no GIL), one static binary, UI reachable from any device on the LAN,
real 3D (WebGL), Web MIDI for controllers, no Qt dependency.

## Contract
`src/model.rs` is the single source of truth. `cargo test` regenerates `ui/src/gen/*.ts` (ts-rs).
Messages: `ClientMsg {t:"cmd", id, cmd: Command}` → `ServerMsg::Ack`; server pushes `Show`, `Library`, `Rt`, `Shows`,
`Learned`, `Log`; binary DMX frames `[0x01][u16 BE universe][512 B]` to subscribers (`SubscribeDmx`).
Show/Library are resent whole (coalesced ≤10/s); `Rt` ≤20/s on change + 1/s heartbeat. High-rate controls
(faders, encoders) send commands without waiting for ack.

REST (small): `GET /api/ports` serial ports · `GET /api/net` local IPs · `POST /api/media` multipart upload → `{url}` ·
`GET /media/<file>` · `POST /api/library/import` multipart (.gdtf/.qxf/OFL .json → `{imported,errors}`, assets extracted to `<data>/library/<dir>/`) · `GET /library/<dir>/…` · `GET /api/show` download current show JSON · `PUT /api/show` import. Everything else is WebSocket.
Static UI is embedded (`rust-embed` of `ui/dist`), falling back to disk in debug builds.

## Engine semantics (per tick, default 44 Hz)
1. **Cuelists** (running ones): tracked state at the current cue (tracking list = cumulative changes since last block cue);
   crossfade previous→current per attribute using `fade_in` (attribute rising) / `fade_out` (falling) after `delay`;
   discrete attributes (those with `capabilities`: colorwheel/gobo/shutter…) snap at the end of `delay`.
   `Cuelist.level` scales the `dimmer` attribute. Auto-follow (`follow`) and timecode cues trigger GO.
2. **Merge cuelists**: dimmer = HTP (max); other attributes = LTP, ties broken by `priority`, then latest GO.
3. **Effects** add an offset around the current value (`hue` rotates RGB(+W/A) fixtures). Executor effect fader scales depth.
4. **Programmer** overrides everything (LTP) for the attributes it holds.
5. **Masters**: Group executors scale the group's dimmer; Grand master scales `dimmer` — and, for fixtures with no dimmer
   channel (RGB-only), scales red/green/blue/white/amber/uv/lime ("virtual dimmer"). Blackout forces those to 0.
6. **Patch**: attribute → DMX through the fixture's profile mode (8-bit or 16-bit with `fine`, `invert`).
   Attributes nobody drives use `ChannelDef.default`. Overlapping addresses are allowed (HTP per channel).
7. **External DMX** (Art-Net in) is merged HTP into the universe, then `DmxSet` overrides win.
8. Final frame → `IoHub.send(universe, frame)` for every configured universe (also when static, so receivers don't time out),
   and → binary frames for subscribers.

Persistence: shows are JSON in `<data>/shows/<name>.json`; the live show autosaves (debounced 2 s) to `<data>/autosave.json`
and is restored on start. Media uploads in `<data>/media`.

Bindings: MIDI (`MidiIn` from the browser) and OSC input → `Action` via `Show.bindings`. Learn mode broadcasts `Learned`.
Timecode: MTC decoded from `MidiIn` (`io::MtcDecoder`) or the internal clock; cues with `timecode` fire when passed.

## Ownership (parallel build)
| Area | Files |
|---|---|
| Engine, API, persistence, library, bindings, CLI | `src/engine/**`, `src/api/**`, `src/library.rs`, `src/persist.rs`, `src/main.rs` |
| Outputs/inputs (Art-Net, sACN, OSC, serial, MTC) | `src/io/**` |
| Shell, design system, Settings, Patch, Monitor, MIDI | `ui/src/App.tsx`, `ui/src/ui/**`, `ui/src/workspaces/{Settings,Patch,Monitor}*`, `ui/src/midi.ts`, `docs/DESIGN_SYSTEM.md` |
| Live, Desk, Cues, executors, widgets | `ui/src/workspaces/{Live,Desk,Cues}*`, `ui/src/widgets/**` |
| 3D visualiser + stage editor | `ui/src/workspaces/Stage3D*`, `ui/src/stage/**` |
Shared and frozen: `src/model.rs`, `ui/src/store.ts`, `ui/src/gen/**`, `ui/src/styles/tokens.css` (ask before changing; additive CSS goes in `base.css`).

## Automation: command line, macros, timelines, MSC
* **Command line** (`Command::CommandLine{text}`, `src/engine/cmdline.rs`): console-style statements separated by `;`/newline,
  English + Spanish keywords, compiled to ordinary `Command`s and executed in order; results in `Runtime.cmd_log` (last 30).
  Selection lives in the interpreter session (one text / one macro run): `fixture 1 thru 4 at 50; red at 100`.
  Selection `fixture|aparato 1 thru 10 + 15 - 4`, `group N`, `all`, `odd|even`, `next|prev`; values `at 50|full|out|half|home|0 thru 100`;
  `record cue|palette|scene|group`, `update cue`, `delete …`, `apply palette`, `go|back|pause|release|goto`, `scene N on|off|solo|at V`,
  `chase N on|off|bpm V|tap`, `macro N`, `timeline N run|stop|seek`, `blackout`, `gm`, `clear`, `park U/C at V`, `undo|redo`, `blind`.
  Syntax errors run nothing and report the column; a failing statement stops the rest. `GET /api/cmdline/help` (grammar JSON),
  `GET /api/cmdline/complete?q=` (autocomplete).
* **Macros** (`macros.rs`): steps = command-line texts with a `wait`; non-blocking on the engine clock; nesting limit 8; a failing step aborts the run.
* **Timelines** (`timeline.rs`): events fire command-line texts when the playhead passes `at`; `seek` never fires skipped events;
  `looped`; `tc_start` follows show timecode (holds while TC is stopped; jumps >2 s are seeks).
* **MSC** (`msc.rs`): SysEx `F0 7F <dev> 02 <fmt> <cmd> … F7` via `MidiIn` (browser needs Web MIDI `sysex: true`) or native MIDI. GO/STOP/RESUME/TIMED_GO/LOAD/ALL_OFF/RESET
  honouring `Settings.msc` (device id, list→cuelist map); LOAD arms the cue for the next MSC GO.
