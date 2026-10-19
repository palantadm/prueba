"""Lumos — Theatrical Lighting Control System."""
import asyncio
import os
import sys
from contextlib import asynccontextmanager
from pathlib import Path

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles

from .engine.universe import UniverseManager
from .engine.fixture import FixtureLibrary, PatchManager
from .engine.cue import CueManager
from .engine.morph import MorphEngine
from .engine.effects import EffectsEngine
from .engine.groups import GroupManager
from .engine.show import ShowManager
from .protocols.osc import OSCServer, LumosOSC
from .protocols.midi_protocol import MIDIDriver
from .api import routes

BASE_DIR = Path(__file__).parent.parent


@asynccontextmanager
async def lifespan(app: FastAPI):
    # ── Fixture library ────────────────────────────────────────────────────
    lib = FixtureLibrary()
    library_dir = BASE_DIR / "fixtures" / "library"
    if library_dir.exists():
        count = lib.load_directory(str(library_dir))
        print(f"[Lumos] Loaded {count} fixture definitions from library")
    _add_generic_fixtures(lib)

    # ── Engine components ──────────────────────────────────────────────────
    pm = PatchManager(lib)
    um = UniverseManager()
    um.add_universe(1, "Main Stage")
    um.add_universe(2, "Back Truss")

    cm = CueManager(pm, um)
    me = MorphEngine(pm)
    ee = EffectsEngine(pm, um)
    gm = GroupManager()

    # Wire effects engine to broadcast ticks
    async def effects_broadcast(event: str, count: int):
        pass  # could broadcast DMX snapshot here if needed

    ee.subscribe(effects_broadcast)

    # Wire grand master changes to dirty universes
    def gm_changed():
        for u in um.universes.values():
            u._dirty = True

    gm.subscribe(gm_changed)

    # ── MIDI ───────────────────────────────────────────────────────────────
    midi = MIDIDriver()

    def on_midi_action(action: str, target: str, value: float):
        if action == "set_channel":
            parts = target.split("/")
            if len(parts) == 2:
                cm.set_channel_direct(parts[0], parts[1], int(value * 255))
        elif action == "cue_go":
            cm.go_next(target)
        elif action == "nrpn":
            pass  # custom NRPN handling

    def on_midi_bpm(bpm: float):
        ee.set_bpm(bpm)

    midi.on_action = on_midi_action
    midi.on_bpm = on_midi_bpm

    # ── OSC ────────────────────────────────────────────────────────────────
    osc_port = int(os.getenv("LUMOS_OSC_PORT", "8001"))
    osc = OSCServer(host="0.0.0.0", port=osc_port)
    LumosOSC(osc, cm, pm, um)

    # ── Show manager ───────────────────────────────────────────────────────
    sm = ShowManager(um, pm, cm, ee, gm, midi, osc)
    autosave_path = os.getenv("LUMOS_AUTOSAVE", str(BASE_DIR / "shows" / "autosave.json"))
    sm.set_autosave(autosave_path)

    # ── WebSocket broadcast callbacks ──────────────────────────────────────
    async def dmx_broadcast(uid: int, data: bytearray):
        await routes.broadcast("dmx_update", {"universe": uid, "data": list(data[:64])})

    um.subscribe(dmx_broadcast)

    async def cue_event(event: str, data: dict):
        await routes.broadcast(event, data)

    cm.subscribe(cue_event)

    # ── Inject into routes ─────────────────────────────────────────────────
    routes.library = lib
    routes.patch = pm
    routes.universes = um
    routes.cues = cm
    routes.morph = me
    routes.effects = ee
    routes.groups = gm
    routes.show_manager = sm
    routes.midi_driver = midi
    routes.osc_server_ref = osc

    # ── Start all services ─────────────────────────────────────────────────
    await um.start()
    await cm.start()
    await ee.start()

    try:
        await osc.start()
        print(f"[Lumos] OSC server listening on UDP :{osc_port}")
    except Exception as e:
        print(f"[Lumos] OSC server failed to start: {e}")

    print("[Lumos] Engine started — ready")
    yield

    # ── Shutdown ───────────────────────────────────────────────────────────
    sm.autosave()
    await ee.stop()
    await cm.stop()
    await um.stop()
    try:
        await osc.stop()
    except Exception:
        pass
    midi.close()
    print("[Lumos] Engine stopped")


def _add_generic_fixtures(lib: FixtureLibrary):
    from .engine.fixture import (
        FixtureDefinition, ChannelDefinition, ChannelGroup,
        FixtureMode, ChannelCapability,
    )

    def _ch(name, group, caps=None, is_fine=False, coarse=None):
        return ChannelDefinition(
            name=name, group=group,
            capabilities=caps or [],
            is_fine=is_fine,
            coarse_name=coarse or "",
        )

    lib.add(FixtureDefinition(
        manufacturer="Generic", model="Dimmer", fixture_type="Dimmer",
        channels={"Intensity": _ch("Intensity", ChannelGroup.INTENSITY,
                                    [ChannelCapability(0, 255, "Dimmer")])},
        modes=[FixtureMode("1ch", ["Intensity"])],
    ))

    lib.add(FixtureDefinition(
        manufacturer="Generic", model="RGB PAR", fixture_type="Color Changer",
        channels={
            "Red":   _ch("Red",   ChannelGroup.COLOR),
            "Green": _ch("Green", ChannelGroup.COLOR),
            "Blue":  _ch("Blue",  ChannelGroup.COLOR),
        },
        modes=[FixtureMode("3ch", ["Red", "Green", "Blue"])],
    ))

    lib.add(FixtureDefinition(
        manufacturer="Generic", model="RGBW PAR", fixture_type="Color Changer",
        channels={
            "Red":   _ch("Red",   ChannelGroup.COLOR),
            "Green": _ch("Green", ChannelGroup.COLOR),
            "Blue":  _ch("Blue",  ChannelGroup.COLOR),
            "White": _ch("White", ChannelGroup.COLOR),
        },
        modes=[FixtureMode("4ch", ["Red", "Green", "Blue", "White"])],
    ))

    lib.add(FixtureDefinition(
        manufacturer="Generic", model="Moving Head Spot", fixture_type="Moving Head",
        channels={
            "Pan":       _ch("Pan",       ChannelGroup.PAN),
            "Pan Fine":  _ch("Pan Fine",  ChannelGroup.PAN,  is_fine=True, coarse="Pan"),
            "Tilt":      _ch("Tilt",      ChannelGroup.TILT),
            "Tilt Fine": _ch("Tilt Fine", ChannelGroup.TILT, is_fine=True, coarse="Tilt"),
            "Color Wheel": _ch("Color Wheel", ChannelGroup.COLOR_WHEEL, [
                ChannelCapability(0,7,"White"), ChannelCapability(8,15,"Red"),
                ChannelCapability(16,23,"Yellow"), ChannelCapability(24,31,"Green"),
                ChannelCapability(32,39,"Blue"), ChannelCapability(40,47,"Magenta"),
                ChannelCapability(48,55,"Cyan"),
            ]),
            "Gobo Wheel": _ch("Gobo Wheel", ChannelGroup.GOBO, [
                ChannelCapability(0,7,"Open"), ChannelCapability(8,15,"Gobo 1"),
                ChannelCapability(16,23,"Gobo 2"), ChannelCapability(24,31,"Gobo 3"),
                ChannelCapability(32,39,"Gobo 4"), ChannelCapability(40,47,"Gobo 5"),
                ChannelCapability(48,255,"Gobo Spin"),
            ]),
            "Shutter": _ch("Shutter", ChannelGroup.SHUTTER, [
                ChannelCapability(0,31,"Closed"), ChannelCapability(32,63,"Open"),
                ChannelCapability(64,95,"Strobe Slow"), ChannelCapability(96,127,"Strobe Fast"),
                ChannelCapability(128,255,"Open"),
            ]),
            "Dimmer": _ch("Dimmer", ChannelGroup.INTENSITY),
            "Focus":  _ch("Focus",  ChannelGroup.FOCUS),
        },
        modes=[FixtureMode("8ch", [
            "Pan","Pan Fine","Tilt","Tilt Fine",
            "Color Wheel","Gobo Wheel","Shutter","Dimmer",
        ])],
    ))

    lib.add(FixtureDefinition(
        manufacturer="Generic", model="Moving Head Wash", fixture_type="Moving Head",
        channels={
            "Pan":       _ch("Pan",       ChannelGroup.PAN),
            "Pan Fine":  _ch("Pan Fine",  ChannelGroup.PAN,  is_fine=True, coarse="Pan"),
            "Tilt":      _ch("Tilt",      ChannelGroup.TILT),
            "Tilt Fine": _ch("Tilt Fine", ChannelGroup.TILT, is_fine=True, coarse="Tilt"),
            "Red":    _ch("Red",   ChannelGroup.COLOR),
            "Green":  _ch("Green", ChannelGroup.COLOR),
            "Blue":   _ch("Blue",  ChannelGroup.COLOR),
            "White":  _ch("White", ChannelGroup.COLOR),
            "Dimmer": _ch("Dimmer", ChannelGroup.INTENSITY),
            "Zoom":   _ch("Zoom",   ChannelGroup.ZOOM),
            "Speed":  _ch("Speed",  ChannelGroup.SPEED),
        },
        modes=[FixtureMode("11ch", [
            "Pan","Pan Fine","Tilt","Tilt Fine",
            "Red","Green","Blue","White","Dimmer","Zoom","Speed",
        ])],
    ))

    lib.add(FixtureDefinition(
        manufacturer="Generic", model="Strobe", fixture_type="Strobe",
        channels={
            "Intensity": _ch("Intensity", ChannelGroup.INTENSITY),
            "Strobe":    _ch("Strobe",    ChannelGroup.SHUTTER, [
                ChannelCapability(0,9,"Off"), ChannelCapability(10,255,"Strobe 1-25Hz"),
            ]),
        },
        modes=[FixtureMode("2ch", ["Intensity","Strobe"])],
    ))

    lib.add(FixtureDefinition(
        manufacturer="Generic", model="LED Strip", fixture_type="Color Changer",
        channels={
            "Red":    _ch("Red",    ChannelGroup.COLOR),
            "Green":  _ch("Green",  ChannelGroup.COLOR),
            "Blue":   _ch("Blue",   ChannelGroup.COLOR),
            "White":  _ch("White",  ChannelGroup.COLOR),
            "Amber":  _ch("Amber",  ChannelGroup.COLOR),
            "UV":     _ch("UV",     ChannelGroup.COLOR),
            "Master": _ch("Master", ChannelGroup.INTENSITY),
        },
        modes=[
            FixtureMode("6ch", ["Red","Green","Blue","White","Amber","UV"]),
            FixtureMode("7ch", ["Master","Red","Green","Blue","White","Amber","UV"]),
        ],
    ))


def create_app() -> FastAPI:
    app = FastAPI(
        title="Lumos",
        description="Theatrical Lighting Control System",
        version="0.2.0",
        lifespan=lifespan,
    )
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_methods=["*"],
        allow_headers=["*"],
    )
    app.include_router(routes.router, prefix="/api")

    frontend_dir = BASE_DIR / "frontend"
    public_dir = frontend_dir / "public"
    if (frontend_dir / "dist").exists():
        app.mount("/", StaticFiles(directory=str(frontend_dir / "dist"), html=True), name="static")
    elif public_dir.exists():
        app.mount("/", StaticFiles(directory=str(public_dir), html=True), name="static")

    return app


app = create_app()
