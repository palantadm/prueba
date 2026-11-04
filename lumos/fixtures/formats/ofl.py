"""Open Fixture Library JSON fixture file parser."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Optional, Any

from fixtures.model import (
    FixtureDef, FixtureMode, ChannelDef, ChannelType, Capability, Physical,
)

_TYPE_MAP: dict[str, ChannelType] = {
    "Intensity":        ChannelType.INTENSITY,
    "Pan":              ChannelType.PAN,
    "Tilt":             ChannelType.TILT,
    "Red":              ChannelType.RED,
    "Green":            ChannelType.GREEN,
    "Blue":             ChannelType.BLUE,
    "White":            ChannelType.WHITE,
    "Amber":            ChannelType.AMBER,
    "UV":               ChannelType.UV,
    "Lime":             ChannelType.LIME,
    "CTO":              ChannelType.CTO,
    "CTB":              ChannelType.CTB,
    "ColorTemperature": ChannelType.CTO,
    "ColorPreset":      ChannelType.COLOR_WHEEL,
    "WheelSlot":        ChannelType.COLOR_WHEEL,
    "WheelRotation":    ChannelType.GOBO_ROT,
    "WheelShake":       ChannelType.GOBO_WHEEL,
    "Shutter":          ChannelType.SHUTTER,
    "StrobeSpeed":      ChannelType.STROBE,
    "StrobeDuration":   ChannelType.STROBE,
    "Speed":            ChannelType.SPEED,
    "Focus":            ChannelType.FOCUS,
    "Zoom":             ChannelType.ZOOM,
    "Iris":             ChannelType.IRIS,
    "IrisEffect":       ChannelType.IRIS,
    "Prism":            ChannelType.PRISM,
    "PrismRotation":    ChannelType.PRISM,
    "Frost":            ChannelType.FROST,
    "FrostIntensity":   ChannelType.FROST,
    "Effect":           ChannelType.EFFECTS,
    "EffectParameter":  ChannelType.EFFECTS,
    "Macro":            ChannelType.MACRO,
    "Maintenance":      ChannelType.MAINTENANCE,
    "NoFunction":       ChannelType.NOTHING,
    "Generic":          ChannelType.NOTHING,
}


def _cap_type_to_channel_type(cap_type: str) -> ChannelType:
    return _TYPE_MAP.get(cap_type, ChannelType.NOTHING)


def _parse_channel(name: str, data: dict[str, Any]) -> ChannelDef:
    cap_list: list[Capability] = []

    capabilities: list[dict] = data.get("capabilities", [])
    if not capabilities:
        cap = data.get("capability")
        if cap:
            capabilities = [cap]

    dominant: ChannelType = ChannelType.NOTHING
    type_votes: dict[ChannelType, int] = {}

    for cap_data in capabilities:
        cap_type = cap_data.get("type", "Generic")
        ct = _cap_type_to_channel_type(cap_type)
        type_votes[ct] = type_votes.get(ct, 0) + 1

        dmx = cap_data.get("dmxRange", [0, 255])
        mn = dmx[0] if dmx else 0
        mx = dmx[1] if len(dmx) > 1 else mn
        label = cap_data.get("comment", cap_type)
        color = ""
        colors = cap_data.get("colors", [])
        if colors:
            color = colors[0]
        cap_list.append(Capability(mn, mx, label, color))

    if type_votes:
        dominant = max(type_votes, key=lambda k: type_votes[k])

    fineChannelAliases = data.get("fineChannelAliases", [])
    is_fine = name.endswith(" fine") or name.endswith("Fine")

    snap = dominant in (ChannelType.COLOR_WHEEL, ChannelType.GOBO_WHEEL)

    return ChannelDef(
        name=name,
        type=dominant,
        capabilities=cap_list,
        is_fine=is_fine,
        snap=snap,
    )


def _parse_physical(data: dict[str, Any]) -> Physical:
    p = Physical()
    dimensions = data.get("dimensions")
    if dimensions:
        try: p.weight_kg = float(dimensions.get("weight", 0))
        except (TypeError, ValueError): pass
        try: p.width_mm  = int(dimensions.get("width",  0))
        except (TypeError, ValueError): pass
        try: p.height_mm = int(dimensions.get("height", 0))
        except (TypeError, ValueError): pass
        try: p.depth_mm  = int(dimensions.get("depth",  0))
        except (TypeError, ValueError): pass
    bulb = data.get("bulb", {}) or {}
    if bulb:
        p.bulb_type = bulb.get("type", "LED")
        try: p.lumens = int(bulb.get("lumens", 0))
        except (TypeError, ValueError): pass
        try: p.color_temp = int(bulb.get("colorTemperature", 0))
        except (TypeError, ValueError): pass
    lens = data.get("lens", {}) or {}
    degs = lens.get("degreesMinMax", [0, 0])
    if degs:
        try: p.lens_min = float(degs[0])
        except (TypeError, ValueError): pass
        try: p.lens_max = float(degs[1]) if len(degs) > 1 else p.lens_min
        except (TypeError, ValueError): pass
    focus = data.get("focus", {}) or {}
    try: p.pan_max  = float(focus.get("panMax",  540))
    except (TypeError, ValueError): pass
    try: p.tilt_max = float(focus.get("tiltMax", 270))
    except (TypeError, ValueError): pass
    power = data.get("power")
    if power:
        try: p.power_w = int(power)
        except (TypeError, ValueError): pass
    return p


def parse_ofl(path: str | Path, manufacturer: str = "") -> Optional[FixtureDef]:
    with open(str(path), encoding="utf-8") as f:
        data = json.load(f)

    mfr = manufacturer or data.get("manufacturer", {}).get("name", "Unknown")
    if isinstance(mfr, dict):
        mfr = mfr.get("name", "Unknown")
    model    = data.get("name", Path(path).stem)
    fx_type  = data.get("categories", ["Generic"])[0] if data.get("categories") else "Generic"
    cats     = data.get("categories", [fx_type])
    comment  = data.get("comment", "")

    available: dict[str, dict] = data.get("availableChannels", {})
    channels: dict[str, ChannelDef] = {}
    for ch_name, ch_data in available.items():
        channels[ch_name] = _parse_channel(ch_name, ch_data)
        for alias in ch_data.get("fineChannelAliases", []):
            fine_def = ChannelDef(
                name=alias,
                type=channels[ch_name].type,
                is_fine=True,
                coarse_name=ch_name,
            )
            channels[alias] = fine_def

    templateChannels: dict[str, dict] = data.get("templateChannels", {})
    for tmpl_name, tmpl_data in templateChannels.items():
        channels[tmpl_name] = _parse_channel(tmpl_name, tmpl_data)

    modes: list[FixtureMode] = []
    for mode_data in data.get("modes", []):
        mode_name = mode_data.get("name", "Mode")
        ch_list: list[str] = []
        for entry in mode_data.get("channels", []):
            if entry is None:
                ch_list.append("")
            elif isinstance(entry, str):
                ch_list.append(entry)
            elif isinstance(entry, dict):
                ch_list.append(entry.get("insert", ""))
            else:
                ch_list.append(str(entry))
        modes.append(FixtureMode(mode_name, ch_list))

    if not modes and channels:
        modes = [FixtureMode("default", list(channels.keys()))]

    phys_raw = data.get("physical", {})
    physical = _parse_physical(phys_raw) if phys_raw else Physical()

    return FixtureDef(
        manufacturer=mfr,
        model=model,
        fixture_type=fx_type,
        channels=channels,
        modes=modes,
        physical=physical,
        categories=cats,
        comment=comment,
    )
