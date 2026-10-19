"""Fixture definitions and patch management.

Supports importing QLC+ .qxf fixture files and our own format.
"""
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple
from enum import Enum
import json
import os


class ChannelGroup(str, Enum):
    INTENSITY = "intensity"
    COLOR = "color"
    GOBO = "gobo"
    PRISM = "prism"
    SHUTTER = "shutter"
    SPEED = "speed"
    PAN = "pan"
    TILT = "tilt"
    FOCUS = "focus"
    ZOOM = "zoom"
    IRIS = "iris"
    COLOR_WHEEL = "color_wheel"
    EFFECT = "effect"
    MAINTENANCE = "maintenance"
    OTHER = "other"


@dataclass
class ChannelCapability:
    min_val: int
    max_val: int
    name: str
    color: Optional[str] = None
    preset: Optional[str] = None


@dataclass
class ChannelDefinition:
    name: str
    group: ChannelGroup = ChannelGroup.OTHER
    default: int = 0
    capabilities: List[ChannelCapability] = field(default_factory=list)
    is_fine: bool = False      # 16-bit fine channel
    coarse_name: Optional[str] = None  # name of the 8-bit pair


@dataclass
class FixtureMode:
    name: str
    channels: List[str]  # ordered list of channel names


@dataclass
class FixtureDefinition:
    """A fixture type definition (from library)."""
    manufacturer: str
    model: str
    fixture_type: str  # Moving Head, LED Bar, PAR, etc.
    channels: Dict[str, ChannelDefinition] = field(default_factory=dict)
    modes: List[FixtureMode] = field(default_factory=list)
    source_file: str = ""

    @property
    def id(self) -> str:
        return f"{self.manufacturer}::{self.model}"

    def get_mode(self, name: str) -> Optional[FixtureMode]:
        for m in self.modes:
            if m.name == name:
                return m
        return self.modes[0] if self.modes else None

    def channel_count(self, mode_name: str = "") -> int:
        mode = self.get_mode(mode_name) if mode_name else (self.modes[0] if self.modes else None)
        return len(mode.channels) if mode else len(self.channels)


@dataclass
class FixtureInstance:
    """A patched fixture in the show."""
    id: str               # unique instance id
    definition_id: str    # references FixtureDefinition.id
    name: str
    universe: int
    address: int          # 1-512 (DMX address, 1-indexed)
    mode: str             # mode name
    group: str = "Default"
    position: Tuple[float, float, float] = (0.0, 0.0, 0.0)  # x, y, z stage coords
    notes: str = ""

    @property
    def end_address(self) -> int:
        return self.address  # will be computed by patch manager


class FixtureLibrary:
    """In-memory library of fixture definitions."""

    def __init__(self):
        self._defs: Dict[str, FixtureDefinition] = {}

    def add(self, fdef: FixtureDefinition):
        self._defs[fdef.id] = fdef

    def get(self, fid: str) -> Optional[FixtureDefinition]:
        return self._defs.get(fid)

    def list_all(self) -> List[FixtureDefinition]:
        return list(self._defs.values())

    def search(self, query: str) -> List[FixtureDefinition]:
        q = query.lower()
        return [f for f in self._defs.values()
                if q in f.manufacturer.lower() or q in f.model.lower() or q in f.fixture_type.lower()]

    def load_from_qxf(self, path: str) -> Optional[FixtureDefinition]:
        """Load a QLC+ .qxf fixture file."""
        try:
            tree = ET.parse(path)
            root = tree.getroot()
        except Exception:
            return None

        manufacturer = root.findtext("Manufacturer", "")
        model = root.findtext("Model", "")
        fixture_type = root.findtext("Type", "Other")

        channels: Dict[str, ChannelDefinition] = {}
        for ch_el in root.findall("Channel"):
            ch_name = ch_el.get("Name", "")
            group_el = ch_el.find("Group")
            group_str = group_el.text.lower().replace(" ", "_") if group_el is not None else "other"
            try:
                group = ChannelGroup(group_str)
            except ValueError:
                group = ChannelGroup.OTHER

            caps = []
            for cap_el in ch_el.findall("Capability"):
                try:
                    caps.append(ChannelCapability(
                        min_val=int(cap_el.get("Min", 0)),
                        max_val=int(cap_el.get("Max", 255)),
                        name=cap_el.text or "",
                        color=cap_el.get("Color"),
                        preset=cap_el.get("Preset"),
                    ))
                except Exception:
                    pass

            channels[ch_name] = ChannelDefinition(
                name=ch_name,
                group=group,
                capabilities=caps,
            )

        modes = []
        for mode_el in root.findall("Mode"):
            mode_name = mode_el.get("Name", "")
            mode_channels = [
                ch_el.get("Name", "")
                for ch_el in sorted(mode_el.findall("Channel"),
                                    key=lambda e: int(e.get("Number", 0)))
            ]
            modes.append(FixtureMode(name=mode_name, channels=mode_channels))

        fdef = FixtureDefinition(
            manufacturer=manufacturer,
            model=model,
            fixture_type=fixture_type,
            channels=channels,
            modes=modes,
            source_file=path,
        )
        self._defs[fdef.id] = fdef
        return fdef

    def load_directory(self, directory: str):
        """Recursively load all .qxf files from a directory."""
        count = 0
        for dirpath, _, filenames in os.walk(directory):
            for filename in filenames:
                if filename.endswith(".qxf"):
                    path = os.path.join(dirpath, filename)
                    fdef = self.load_from_qxf(path)
                    if fdef:
                        count += 1
        return count

    def to_dict(self) -> List[dict]:
        result = []
        for fdef in self._defs.values():
            result.append({
                "id": fdef.id,
                "manufacturer": fdef.manufacturer,
                "model": fdef.model,
                "type": fdef.fixture_type,
                "modes": [{"name": m.name, "channels": len(m.channels)} for m in fdef.modes],
            })
        return result


class PatchManager:
    """Manages patched fixture instances in the show."""

    def __init__(self, library: FixtureLibrary):
        self.library = library
        self.fixtures: Dict[str, FixtureInstance] = {}
        self._id_counter = 1

    def _next_id(self) -> str:
        fid = f"fix_{self._id_counter:04d}"
        self._id_counter += 1
        return fid

    def patch(self, definition_id: str, name: str, universe: int, address: int,
              mode: str = "", group: str = "Default") -> Optional[FixtureInstance]:
        """Add a fixture to the patch."""
        fdef = self.library.get(definition_id)
        if not fdef:
            return None
        fid = self._next_id()
        inst = FixtureInstance(
            id=fid,
            definition_id=definition_id,
            name=name,
            universe=universe,
            address=address,
            mode=mode or (fdef.modes[0].name if fdef.modes else ""),
            group=group,
        )
        self.fixtures[fid] = inst
        return inst

    def unpatch(self, fixture_id: str) -> bool:
        if fixture_id in self.fixtures:
            del self.fixtures[fixture_id]
            return True
        return False

    def get_channels(self, fixture_id: str) -> List[str]:
        """Return ordered channel names for a fixture's mode."""
        inst = self.fixtures.get(fixture_id)
        if not inst:
            return []
        fdef = self.library.get(inst.definition_id)
        if not fdef:
            return []
        mode = fdef.get_mode(inst.mode)
        return mode.channels if mode else list(fdef.channels.keys())

    def get_channel_address(self, fixture_id: str, channel_name: str) -> Optional[Tuple[int, int]]:
        """Return (universe, dmx_address_0indexed) for a channel."""
        inst = self.fixtures.get(fixture_id)
        if not inst:
            return None
        channels = self.get_channels(fixture_id)
        try:
            offset = channels.index(channel_name)
            return (inst.universe, inst.address - 1 + offset)
        except ValueError:
            return None

    def check_conflicts(self, universe: int, address: int, count: int, exclude_id: str = "") -> List[str]:
        """Find fixtures that overlap with a given patch range."""
        conflicts = []
        for fid, inst in self.fixtures.items():
            if fid == exclude_id:
                continue
            if inst.universe != universe:
                continue
            inst_count = len(self.get_channels(fid))
            inst_end = inst.address + inst_count - 1
            proposed_end = address + count - 1
            if address <= inst_end and proposed_end >= inst.address:
                conflicts.append(fid)
        return conflicts

    def to_dict(self) -> List[dict]:
        result = []
        for inst in self.fixtures.values():
            channels = self.get_channels(inst.id)
            result.append({
                "id": inst.id,
                "definition_id": inst.definition_id,
                "name": inst.name,
                "universe": inst.universe,
                "address": inst.address,
                "end_address": inst.address + len(channels) - 1,
                "mode": inst.mode,
                "group": inst.group,
                "channel_count": len(channels),
                "channels": channels,
                "position": list(inst.position),
                "notes": inst.notes,
            })
        return result
