"""QLC+ .qxf XML fixture file parser."""
from __future__ import annotations

import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Optional

from fixtures.model import (
    FixtureDef, FixtureMode, ChannelDef, ChannelType, Capability, Physical,
)

_GROUP_MAP: dict[str, ChannelType] = {
    "Intensity":    ChannelType.INTENSITY,
    "Pan":          ChannelType.PAN,
    "Tilt":         ChannelType.TILT,
    "Colour":       ChannelType.COLOR_WHEEL,
    "Gobo":         ChannelType.GOBO_WHEEL,
    "Shutter":      ChannelType.SHUTTER,
    "Speed":        ChannelType.SPEED,
    "Prism":        ChannelType.PRISM,
    "Beam":         ChannelType.ZOOM,
    "Effect":       ChannelType.EFFECTS,
    "Maintenance":  ChannelType.MAINTENANCE,
    "Nothing":      ChannelType.NOTHING,
}

_NAME_HINTS: list[tuple[str, ChannelType]] = [
    ("red",         ChannelType.RED),
    ("green",       ChannelType.GREEN),
    ("blue",        ChannelType.BLUE),
    ("white",       ChannelType.WHITE),
    ("amber",       ChannelType.AMBER),
    (" uv",         ChannelType.UV),
    ("ultraviolet", ChannelType.UV),
    ("lime",        ChannelType.LIME),
    ("cto",         ChannelType.CTO),
    ("ctb",         ChannelType.CTB),
    ("warm white",  ChannelType.CTO),
    ("cool white",  ChannelType.CTB),
    ("dimmer",      ChannelType.INTENSITY),
    ("master",      ChannelType.INTENSITY),
    ("intensity",   ChannelType.INTENSITY),
    ("pan fine",    ChannelType.PAN),
    ("tilt fine",   ChannelType.TILT),
    ("focus",       ChannelType.FOCUS),
    ("zoom",        ChannelType.ZOOM),
    ("iris",        ChannelType.IRIS),
    ("frost",       ChannelType.FROST),
    ("prism",       ChannelType.PRISM),
    ("strobe",      ChannelType.STROBE),
    ("speed",       ChannelType.SPEED),
    ("gobo rot",    ChannelType.GOBO_ROT),
    ("gobo",        ChannelType.GOBO_WHEEL),
    ("color wheel", ChannelType.COLOR_WHEEL),
    ("colour wheel",ChannelType.COLOR_WHEEL),
    ("effect",      ChannelType.EFFECTS),
    ("macro",       ChannelType.MACRO),
    ("reset",       ChannelType.RESET),
    ("maintenance", ChannelType.MAINTENANCE),
    ("shutter",     ChannelType.SHUTTER),
]


def _infer_type(group: str, name: str) -> ChannelType:
    base = _GROUP_MAP.get(group, ChannelType.NOTHING)
    lower = name.lower()
    for hint, ct in _NAME_HINTS:
        if hint in lower:
            return ct
    if base == ChannelType.INTENSITY and "colour" in lower:
        return ChannelType.COLOR_WHEEL
    return base


def _parse_channel(el: ET.Element) -> ChannelDef:
    name = el.get("Name", "Unknown")
    group_el = el.find("Group")
    group = group_el.text if group_el is not None and group_el.text else "Nothing"
    byte_val = int(group_el.get("Byte", "0")) if group_el is not None else 0
    is_fine = byte_val == 1

    ch_type = _infer_type(group, name)
    if group == "Pan":
        ch_type = ChannelType.PAN
    elif group == "Tilt":
        ch_type = ChannelType.TILT

    caps: list[Capability] = []
    for cap_el in el.findall("Capability"):
        try:
            mn = int(cap_el.get("Min", "0"))
            mx = int(cap_el.get("Max", "255"))
            label = cap_el.text or ""
            color = cap_el.get("Color", cap_el.get("Color1", ""))
            caps.append(Capability(mn, mx, label.strip(), color))
        except (ValueError, TypeError):
            continue

    return ChannelDef(
        name=name,
        type=ch_type,
        capabilities=caps,
        is_fine=is_fine,
        snap=(group in ("Colour", "Gobo")),
    )


def _parse_physical(el: ET.Element) -> Physical:
    p = Physical()
    bulb = el.find("Bulb")
    dims = el.find("Dimensions")
    lens = el.find("Lens")
    focus = el.find("Focus")
    tech = el.find("Technical")

    if bulb is not None:
        p.bulb_type = bulb.get("Type", "LED")
        try: p.lumens = int(float(bulb.get("Lumens", "0")))
        except ValueError: pass
        try: p.color_temp = int(float(bulb.get("ColourTemperature", "0")))
        except ValueError: pass
    if dims is not None:
        try: p.weight_kg = float(dims.get("Weight", "0"))
        except ValueError: pass
        try: p.width_mm  = int(dims.get("Width", "0"))
        except ValueError: pass
        try: p.height_mm = int(dims.get("Height", "0"))
        except ValueError: pass
        try: p.depth_mm  = int(dims.get("Depth", "0"))
        except ValueError: pass
    if lens is not None:
        try: p.lens_min = float(lens.get("DegreesMin", "0"))
        except ValueError: pass
        try: p.lens_max = float(lens.get("DegreesMax", "0"))
        except ValueError: pass
    if focus is not None:
        try: p.pan_max  = float(focus.get("PanMax", "540"))
        except ValueError: pass
        try: p.tilt_max = float(focus.get("TiltMax", "270"))
        except ValueError: pass
    if tech is not None:
        try: p.power_w = int(float(tech.get("PowerConsumption", "0")))
        except ValueError: pass
        p.connector = tech.get("DmxConnector", "3-pin XLR")
    return p


def _parse_tree(root: ET.Element) -> Optional[FixtureDef]:
    ns = {"": ""}
    manufacturer = root.findtext("Manufacturer") or "Unknown"
    model        = root.findtext("Model") or "Unknown"
    fx_type      = root.findtext("Type") or "Generic"

    channels: dict[str, ChannelDef] = {}
    for ch_el in root.findall("Channel"):
        name = ch_el.get("Name", "")
        if name:
            channels[name] = _parse_channel(ch_el)

    modes: list[FixtureMode] = []
    for mode_el in root.findall("Mode"):
        mode_name = mode_el.get("Name", "Mode")
        ch_names: list[str] = []
        for ch_ref in mode_el.findall("Channel"):
            ch_names.append(ch_ref.text or "")
        modes.append(FixtureMode(mode_name, ch_names))

    if not modes and channels:
        modes = [FixtureMode("1ch", list(channels.keys()))]

    phys_el = root.find("Physical")
    physical = _parse_physical(phys_el) if phys_el is not None else Physical()

    return FixtureDef(
        manufacturer=manufacturer,
        model=model,
        fixture_type=fx_type,
        channels=channels,
        modes=modes,
        physical=physical,
        categories=[fx_type],
    )


def parse_qlcplus(path: str | Path) -> Optional[FixtureDef]:
    try:
        tree = ET.parse(str(path))
        root = tree.getroot()
        tag = root.tag
        if "}" in tag:
            tag = tag.split("}")[1]
        if tag != "FixtureDefinition":
            return None
        # Strip namespace from all elements
        for el in root.iter():
            if "}" in el.tag:
                el.tag = el.tag.split("}")[1]
        return _parse_tree(root)
    except ET.ParseError:
        return None


def parse_qlcplus_string(xml_str: str) -> Optional[FixtureDef]:
    try:
        root = ET.fromstring(xml_str)
        for el in root.iter():
            if "}" in el.tag:
                el.tag = el.tag.split("}")[1]
        return _parse_tree(root)
    except ET.ParseError:
        return None
