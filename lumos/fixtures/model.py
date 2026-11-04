"""Fixture data model — compatible with QLC+, OFL, GDTF."""
from __future__ import annotations
from dataclasses import dataclass, field
from enum import Enum
from typing import Optional


class ChannelType(Enum):
    INTENSITY   = "intensity"
    PAN         = "pan"
    TILT        = "tilt"
    RED         = "red"
    GREEN       = "green"
    BLUE        = "blue"
    WHITE       = "white"
    AMBER       = "amber"
    UV          = "uv"
    LIME        = "lime"
    CTO         = "cto"
    CTB         = "ctb"
    COLOR_WHEEL = "color_wheel"
    GOBO_WHEEL  = "gobo_wheel"
    GOBO_ROT    = "gobo_rot"
    SHUTTER     = "shutter"
    STROBE      = "strobe"
    SPEED       = "speed"
    FOCUS       = "focus"
    ZOOM        = "zoom"
    IRIS        = "iris"
    PRISM       = "prism"
    FROST       = "frost"
    EFFECTS     = "effects"
    MACRO       = "macro"
    RESET       = "reset"
    MAINTENANCE = "maintenance"
    NOTHING     = "nothing"

    @property
    def ui_color(self) -> str:
        return {
            ChannelType.INTENSITY:   "#f0e0a0",
            ChannelType.PAN:         "#00d4e8",
            ChannelType.TILT:        "#00d4e8",
            ChannelType.RED:         "#e05050",
            ChannelType.GREEN:       "#50d050",
            ChannelType.BLUE:        "#5080ff",
            ChannelType.WHITE:       "#e8e8e8",
            ChannelType.AMBER:       "#ff9040",
            ChannelType.UV:          "#a040ff",
            ChannelType.LIME:        "#c0e040",
            ChannelType.CTO:         "#ffd070",
            ChannelType.CTB:         "#70c0ff",
            ChannelType.COLOR_WHEEL: "#e040a0",
            ChannelType.GOBO_WHEEL:  "#ff8c00",
            ChannelType.GOBO_ROT:    "#ffb040",
            ChannelType.SHUTTER:     "#60c040",
            ChannelType.STROBE:      "#60c040",
            ChannelType.SPEED:       "#80d060",
            ChannelType.FOCUS:       "#c0c0c0",
            ChannelType.ZOOM:        "#c0d0e0",
            ChannelType.IRIS:        "#a0b0c0",
            ChannelType.PRISM:       "#c080ff",
            ChannelType.FROST:       "#b0c8e0",
            ChannelType.EFFECTS:     "#c080ff",
        }.get(self, "#707070")

    @property
    def display_name(self) -> str:
        return self.value.replace("_", " ").title()


@dataclass
class Capability:
    min_val: int
    max_val: int
    label: str
    color: str = ""
    gobo_index: int = 0


@dataclass
class ChannelDef:
    name: str
    type: ChannelType
    capabilities: list[Capability] = field(default_factory=list)
    default_value: int = 0
    highlight_value: int = 255
    is_fine: bool = False
    coarse_name: str = ""
    snap: bool = False

    @property
    def description(self) -> str:
        if self.capabilities:
            return " / ".join(c.label for c in self.capabilities[:3])
        return self.type.display_name


@dataclass
class FixtureMode:
    name: str
    channels: list[str]

    @property
    def channel_count(self) -> int:
        return len(self.channels)


@dataclass
class Physical:
    weight_kg:  float = 0.0
    width_mm:   int = 0
    height_mm:  int = 0
    depth_mm:   int = 0
    power_w:    int = 0
    connector:  str = "3-pin XLR"
    bulb_type:  str = "LED"
    lumens:     int = 0
    color_temp: int = 0
    pan_max:    float = 540.0
    tilt_max:   float = 270.0
    lens_min:   float = 0.0
    lens_max:   float = 0.0


@dataclass
class FixtureDef:
    manufacturer: str
    model:        str
    fixture_type: str
    channels:     dict[str, ChannelDef]
    modes:        list[FixtureMode]
    physical:     Physical = field(default_factory=Physical)
    categories:   list[str] = field(default_factory=list)
    comment:      str = ""

    @property
    def default_mode(self) -> FixtureMode:
        return self.modes[0] if self.modes else FixtureMode("1ch", [])

    @property
    def key(self) -> str:
        return f"{self.manufacturer}/{self.model}"

    def mode_by_name(self, name: str) -> Optional[FixtureMode]:
        for m in self.modes:
            if m.name == name:
                return m
        return None

    @property
    def modes_summary(self) -> str:
        return " / ".join(f"{m.channel_count}ch" for m in self.modes)
