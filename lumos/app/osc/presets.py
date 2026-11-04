"""
Pre-built OSC route templates for popular apps.
Each preset is a callable that returns an OSCRoute (or list[OSCRoute]).
"""
from __future__ import annotations

from .route import OSCRoute

# ── App name constants ──────────────────────────────────────────────────────
RESOLUME_AVENUE = "Resolume Avenue/Arena"
TOUCHDESIGNER   = "Derivative TouchDesigner"
ABLETON_LIVE    = "Ableton Live (via Max4Live)"
GENERIC_FLOAT   = "Generic (float 0-1)"
GENERIC_INT     = "Generic (int 0-255)"
QLCplus         = "QLC+"
WATCHOUT        = "Dataton Watchout"


# ── Preset factory functions ────────────────────────────────────────────────

def resolume_layer_opacity(
    host: str, port: int, universe: int, channel: int, layer: int = 1
) -> OSCRoute:
    """Map one DMX channel → Resolume layer opacity."""
    return OSCRoute(
        label=f"Resolume Layer {layer} Opacity",
        universe=universe,
        channel=channel,
        host=host,
        port=port,
        address=f"/composition/layers/{layer}/video/opacity",
        out_type="float",
        out_min=0.0,
        out_max=1.0,
        mode="continuous",
    )


def resolume_clip_trigger(
    host: str, port: int, universe: int, channel: int,
    layer: int = 1, clip: int = 1
) -> OSCRoute:
    """DMX value >= 128 triggers a Resolume clip."""
    return OSCRoute(
        label=f"Resolume L{layer}C{clip} trigger",
        universe=universe,
        channel=channel,
        host=host,
        port=port,
        address=f"/composition/layers/{layer}/clips/{clip}/connect",
        out_type="int",
        out_min=0,
        out_max=1,
        mode="threshold_cross",
        threshold=128,
    )


def resolume_master_opacity(
    host: str, port: int, universe: int, channel: int
) -> OSCRoute:
    """Map one DMX channel → Resolume composition master opacity."""
    return OSCRoute(
        label="Resolume Master Opacity",
        universe=universe,
        channel=channel,
        host=host,
        port=port,
        address="/composition/video/opacity",
        out_type="float",
        out_min=0.0,
        out_max=1.0,
        mode="continuous",
    )


def touchdesigner_float(
    host: str, port: int, universe: int, channel: int,
    td_path: str = "/chan1"
) -> OSCRoute:
    """Map DMX channel → TouchDesigner CHOP channel (float 0-1)."""
    return OSCRoute(
        label=f"TD {td_path}",
        universe=universe,
        channel=channel,
        host=host,
        port=port,
        address=td_path,
        out_type="float",
        out_min=0.0,
        out_max=1.0,
        mode="continuous",
    )


def touchdesigner_int(
    host: str, port: int, universe: int, channel: int,
    td_path: str = "/chan1"
) -> OSCRoute:
    """Map DMX channel → TouchDesigner CHOP channel (int 0-255)."""
    return OSCRoute(
        label=f"TD {td_path} (int)",
        universe=universe,
        channel=channel,
        host=host,
        port=port,
        address=td_path,
        out_type="int",
        out_min=0,
        out_max=255,
        mode="continuous",
    )


def ableton_track_volume(
    host: str, port: int, universe: int, channel: int, track: int = 1
) -> OSCRoute:
    """Map DMX channel → Ableton track volume via Max4Live (0.0 - 1.0)."""
    return OSCRoute(
        label=f"Ableton Track {track} Volume",
        universe=universe,
        channel=channel,
        host=host,
        port=port,
        address=f"/live/track/set/volume/{track - 1}",
        out_type="float",
        out_min=0.0,
        out_max=1.0,
        mode="continuous",
    )


def qlcplus_channel(
    host: str, port: int, universe: int, channel: int
) -> OSCRoute:
    """Map DMX channel → QLC+ channel (0-indexed universes and channels)."""
    return OSCRoute(
        label=f"QLC+ U{universe}C{channel}",
        universe=universe,
        channel=channel,
        host=host,
        port=port,
        # QLC+ uses 0-indexed universe and channel numbers
        address=f"/dmx/universe/{universe - 1}/dmx/{channel - 1}",
        out_type="float",
        out_min=0.0,
        out_max=1.0,
        mode="continuous",
    )


def generic_float(
    host: str, port: int, universe: int, channel: int,
    address: str = "/lumos/channel"
) -> OSCRoute:
    """Generic float route (0.0 - 1.0)."""
    return OSCRoute(
        label=f"Float {address}",
        universe=universe,
        channel=channel,
        host=host,
        port=port,
        address=address,
        out_type="float",
        out_min=0.0,
        out_max=1.0,
        mode="continuous",
    )


def generic_int(
    host: str, port: int, universe: int, channel: int,
    address: str = "/lumos/channel"
) -> OSCRoute:
    """Generic int route (0 - 255)."""
    return OSCRoute(
        label=f"Int {address}",
        universe=universe,
        channel=channel,
        host=host,
        port=port,
        address=address,
        out_type="int",
        out_min=0,
        out_max=255,
        mode="continuous",
    )


def watchout_fade(
    host: str, port: int, universe: int, channel: int,
    display_name: str = "Main"
) -> OSCRoute:
    """Map DMX channel → Dataton Watchout display fade."""
    return OSCRoute(
        label=f"Watchout {display_name} Fade",
        universe=universe,
        channel=channel,
        host=host,
        port=port,
        address=f"/watchout/fade/{display_name}",
        out_type="float",
        out_min=0.0,
        out_max=1.0,
        mode="continuous",
    )


# ── Preset catalog (for UI discovery) ──────────────────────────────────────
#
# Each entry describes a preset function:
#   func        callable name in this module
#   params      extra params beyond (host, port, universe, channel)
#   description human-readable one-liner
#

PRESET_CATALOG: dict[str, list[dict]] = {
    RESOLUME_AVENUE: [
        {
            "func": "resolume_layer_opacity",
            "params": ["layer"],
            "defaults": {"layer": 1},
            "description": "Layer opacity (float 0-1)",
        },
        {
            "func": "resolume_clip_trigger",
            "params": ["layer", "clip"],
            "defaults": {"layer": 1, "clip": 1},
            "description": "Clip trigger (threshold cross)",
        },
        {
            "func": "resolume_master_opacity",
            "params": [],
            "defaults": {},
            "description": "Master composition opacity",
        },
    ],
    TOUCHDESIGNER: [
        {
            "func": "touchdesigner_float",
            "params": ["td_path"],
            "defaults": {"td_path": "/chan1"},
            "description": "Float channel (0-1)",
        },
        {
            "func": "touchdesigner_int",
            "params": ["td_path"],
            "defaults": {"td_path": "/chan1"},
            "description": "Int channel (0-255)",
        },
    ],
    ABLETON_LIVE: [
        {
            "func": "ableton_track_volume",
            "params": ["track"],
            "defaults": {"track": 1},
            "description": "Track volume (Max4Live)",
        },
    ],
    QLCplus: [
        {
            "func": "qlcplus_channel",
            "params": [],
            "defaults": {},
            "description": "DMX channel (0-indexed bridge)",
        },
    ],
    GENERIC_FLOAT: [
        {
            "func": "generic_float",
            "params": ["address"],
            "defaults": {"address": "/lumos/channel"},
            "description": "Float 0.0 – 1.0",
        },
    ],
    GENERIC_INT: [
        {
            "func": "generic_int",
            "params": ["address"],
            "defaults": {"address": "/lumos/channel"},
            "description": "Int 0 – 255",
        },
    ],
    WATCHOUT: [
        {
            "func": "watchout_fade",
            "params": ["display_name"],
            "defaults": {"display_name": "Main"},
            "description": "Display fade",
        },
    ],
}

# Map function name → callable for dynamic dispatch from UI
_PRESET_FUNCS = {
    "resolume_layer_opacity": resolume_layer_opacity,
    "resolume_clip_trigger": resolume_clip_trigger,
    "resolume_master_opacity": resolume_master_opacity,
    "touchdesigner_float": touchdesigner_float,
    "touchdesigner_int": touchdesigner_int,
    "ableton_track_volume": ableton_track_volume,
    "qlcplus_channel": qlcplus_channel,
    "generic_float": generic_float,
    "generic_int": generic_int,
    "watchout_fade": watchout_fade,
}


def create_preset(
    func_name: str,
    host: str,
    port: int,
    universe: int,
    channel: int,
    **kwargs,
) -> OSCRoute:
    """Create an OSCRoute from a preset function name and parameters."""
    fn = _PRESET_FUNCS.get(func_name)
    if fn is None:
        raise ValueError(f"Unknown preset function: {func_name!r}")
    return fn(host=host, port=port, universe=universe, channel=channel, **kwargs)
