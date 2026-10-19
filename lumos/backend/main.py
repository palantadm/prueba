"""Lumos - Theatrical Lighting Control System
Main entry point.
"""
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
from .protocols.osc import OSCServer, LumosOSC
from .api import routes

BASE_DIR = Path(__file__).parent.parent


@asynccontextmanager
async def lifespan(app: FastAPI):
    # ── Initialize engine ──────────────────────────────────────────────────
    lib = FixtureLibrary()
    library_dir = BASE_DIR / "fixtures" / "library"
    if library_dir.exists():
        count = lib.load_directory(str(library_dir))
        print(f"[Lumos] Loaded {count} fixture definitions")

    # Add some built-in generic fixtures for demo
    _add_generic_fixtures(lib)

    pm = PatchManager(lib)
    um = UniverseManager()
    um.add_universe(1, "Main Stage", output_type="none")
    um.add_universe(2, "Back Truss", output_type="none")

    cm = CueManager(pm, um)
    me = MorphEngine(pm)

    # Wire up WebSocket DMX broadcast
    async def dmx_broadcast(uid: int, data: bytearray):
        await routes.broadcast("dmx_update", {"universe": uid, "data": list(data[:64])})

    um.subscribe(dmx_broadcast)

    # Wire up cue event broadcast
    async def cue_event(event: str, data: dict):
        await routes.broadcast(event, data)

    cm.subscribe(cue_event)

    # ── OSC Server ─────────────────────────────────────────────────────────
    osc_port = int(os.getenv("LUMOS_OSC_PORT", "8000"))
    osc = OSCServer(host="0.0.0.0", port=osc_port)
    LumosOSC(osc, cm, pm, um)

    # ── Inject into routes ─────────────────────────────────────────────────
    routes.library = lib
    routes.patch = pm
    routes.universes = um
    routes.cues = cm
    routes.morph = me
    routes.osc_server_ref = osc

    # ── Start services ─────────────────────────────────────────────────────
    await um.start()
    await cm.start()
    try:
        await osc.start()
        print(f"[Lumos] OSC server listening on UDP :{osc_port}")
    except Exception as e:
        print(f"[Lumos] OSC server failed to start: {e}")

    print("[Lumos] Engine started")
    yield

    # ── Shutdown ───────────────────────────────────────────────────────────
    await cm.stop()
    await um.stop()
    await osc.stop()
    print("[Lumos] Engine stopped")


def _add_generic_fixtures(lib: FixtureLibrary):
    """Add built-in generic fixture types."""
    from .engine.fixture import FixtureDefinition, ChannelDefinition, ChannelGroup, FixtureMode, ChannelCapability

    # Generic Dimmer
    dimmer = FixtureDefinition(
        manufacturer="Generic",
        model="Dimmer",
        fixture_type="Dimmer",
        channels={
            "Intensity": ChannelDefinition(
                name="Intensity",
                group=ChannelGroup.INTENSITY,
                capabilities=[
                    ChannelCapability(0, 255, "Dimmer"),
                ],
            )
        },
        modes=[FixtureMode(name="1ch", channels=["Intensity"])],
    )
    lib.add(dimmer)

    # Generic RGB LED PAR
    rgb_par = FixtureDefinition(
        manufacturer="Generic",
        model="RGB PAR",
        fixture_type="Color Changer",
        channels={
            "Red": ChannelDefinition("Red", ChannelGroup.COLOR),
            "Green": ChannelDefinition("Green", ChannelGroup.COLOR),
            "Blue": ChannelDefinition("Blue", ChannelGroup.COLOR),
        },
        modes=[FixtureMode(name="3ch", channels=["Red", "Green", "Blue"])],
    )
    lib.add(rgb_par)

    # Generic RGBW LED PAR
    rgbw_par = FixtureDefinition(
        manufacturer="Generic",
        model="RGBW PAR",
        fixture_type="Color Changer",
        channels={
            "Red": ChannelDefinition("Red", ChannelGroup.COLOR),
            "Green": ChannelDefinition("Green", ChannelGroup.COLOR),
            "Blue": ChannelDefinition("Blue", ChannelGroup.COLOR),
            "White": ChannelDefinition("White", ChannelGroup.COLOR),
        },
        modes=[FixtureMode(name="4ch", channels=["Red", "Green", "Blue", "White"])],
    )
    lib.add(rgbw_par)

    # Generic Moving Head (Pan/Tilt + Color + Gobo)
    mh = FixtureDefinition(
        manufacturer="Generic",
        model="Moving Head Spot",
        fixture_type="Moving Head",
        channels={
            "Pan": ChannelDefinition("Pan", ChannelGroup.PAN),
            "Pan Fine": ChannelDefinition("Pan Fine", ChannelGroup.PAN, is_fine=True, coarse_name="Pan"),
            "Tilt": ChannelDefinition("Tilt", ChannelGroup.TILT),
            "Tilt Fine": ChannelDefinition("Tilt Fine", ChannelGroup.TILT, is_fine=True, coarse_name="Tilt"),
            "Color Wheel": ChannelDefinition("Color Wheel", ChannelGroup.COLOR_WHEEL,
                capabilities=[
                    ChannelCapability(0, 7, "White"),
                    ChannelCapability(8, 15, "Red"),
                    ChannelCapability(16, 23, "Yellow"),
                    ChannelCapability(24, 31, "Green"),
                    ChannelCapability(32, 39, "Blue"),
                    ChannelCapability(40, 47, "Magenta"),
                    ChannelCapability(48, 55, "Cyan"),
                ]),
            "Gobo Wheel": ChannelDefinition("Gobo Wheel", ChannelGroup.GOBO,
                capabilities=[
                    ChannelCapability(0, 7, "Open"),
                    ChannelCapability(8, 15, "Gobo 1"),
                    ChannelCapability(16, 23, "Gobo 2"),
                    ChannelCapability(24, 31, "Gobo 3"),
                    ChannelCapability(32, 39, "Gobo 4"),
                    ChannelCapability(40, 47, "Gobo 5"),
                    ChannelCapability(48, 255, "Gobo Spin"),
                ]),
            "Shutter": ChannelDefinition("Shutter", ChannelGroup.SHUTTER,
                capabilities=[
                    ChannelCapability(0, 31, "Closed"),
                    ChannelCapability(32, 63, "Open"),
                    ChannelCapability(64, 95, "Strobe Slow"),
                    ChannelCapability(96, 127, "Strobe Fast"),
                    ChannelCapability(128, 255, "Open"),
                ]),
            "Dimmer": ChannelDefinition("Dimmer", ChannelGroup.INTENSITY),
            "Focus": ChannelDefinition("Focus", ChannelGroup.FOCUS),
        },
        modes=[FixtureMode(name="8ch", channels=[
            "Pan", "Pan Fine", "Tilt", "Tilt Fine",
            "Color Wheel", "Gobo Wheel", "Shutter", "Dimmer",
        ])],
    )
    lib.add(mh)

    # Generic Moving Head Wash
    mh_wash = FixtureDefinition(
        manufacturer="Generic",
        model="Moving Head Wash",
        fixture_type="Moving Head",
        channels={
            "Pan": ChannelDefinition("Pan", ChannelGroup.PAN),
            "Pan Fine": ChannelDefinition("Pan Fine", ChannelGroup.PAN, is_fine=True),
            "Tilt": ChannelDefinition("Tilt", ChannelGroup.TILT),
            "Tilt Fine": ChannelDefinition("Tilt Fine", ChannelGroup.TILT, is_fine=True),
            "Red": ChannelDefinition("Red", ChannelGroup.COLOR),
            "Green": ChannelDefinition("Green", ChannelGroup.COLOR),
            "Blue": ChannelDefinition("Blue", ChannelGroup.COLOR),
            "White": ChannelDefinition("White", ChannelGroup.COLOR),
            "Dimmer": ChannelDefinition("Dimmer", ChannelGroup.INTENSITY),
            "Zoom": ChannelDefinition("Zoom", ChannelGroup.ZOOM),
            "Speed": ChannelDefinition("Speed", ChannelGroup.SPEED),
        },
        modes=[
            FixtureMode(name="11ch", channels=[
                "Pan", "Pan Fine", "Tilt", "Tilt Fine",
                "Red", "Green", "Blue", "White",
                "Dimmer", "Zoom", "Speed",
            ]),
        ],
    )
    lib.add(mh_wash)


def create_app() -> FastAPI:
    app = FastAPI(
        title="Lumos",
        description="Theatrical Lighting Control System",
        version="0.1.0",
        lifespan=lifespan,
    )
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_methods=["*"],
        allow_headers=["*"],
    )
    app.include_router(routes.router, prefix="/api")

    # Serve frontend
    frontend_dir = BASE_DIR / "frontend"
    if (frontend_dir / "dist").exists():
        app.mount("/", StaticFiles(directory=str(frontend_dir / "dist"), html=True), name="static")
    elif (frontend_dir / "public").exists():
        app.mount("/", StaticFiles(directory=str(frontend_dir / "public"), html=True), name="static")

    return app


app = create_app()
