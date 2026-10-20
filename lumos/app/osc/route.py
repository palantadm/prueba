from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional
import uuid


@dataclass
class OSCRoute:
    """Maps a DMX channel (or range) to a custom OSC message."""

    id: str = field(default_factory=lambda: str(uuid.uuid4())[:8])
    label: str = ""
    enabled: bool = True

    # Source (DMX)
    universe: int = 1
    channel: int = 1        # 1-512

    # Destination (OSC)
    host: str = "127.0.0.1"
    port: int = 8000
    address: str = "/lumos/channel"

    # Value transform: maps DMX [in_min, in_max] → [out_min, out_max]
    in_min: int = 0
    in_max: int = 255
    out_min: float = 0.0
    out_max: float = 1.0
    out_type: str = "float"     # "float" | "int" | "bool" | "string"
    out_string_on: str = "on"   # value when bool/threshold → True
    out_string_off: str = "off"

    # Trigger
    mode: str = "continuous"   # "continuous" | "on_change" | "threshold_cross"
    threshold: int = 128       # for threshold_cross mode

    # Private state (not serialised)
    _last_sent: Optional[int] = field(default=None, repr=False, compare=False)
    _above_threshold: bool = field(default=False, repr=False, compare=False)

    # ------------------------------------------------------------------
    # Core logic
    # ------------------------------------------------------------------

    def transform(self, dmx_value: int) -> float | int | bool | str:
        """Map DMX 0-255 to output value according to out_type and range."""
        if self.in_max == self.in_min:
            t = 0.0
        else:
            t = (dmx_value - self.in_min) / (self.in_max - self.in_min)
        t = max(0.0, min(1.0, t))

        if self.out_type == "bool":
            return dmx_value >= self.threshold
        if self.out_type == "string":
            return self.out_string_on if dmx_value >= self.threshold else self.out_string_off

        raw = self.out_min + t * (self.out_max - self.out_min)
        return int(round(raw)) if self.out_type == "int" else float(raw)

    def should_send(self, dmx_value: int) -> bool:
        """Decide whether to fire given the current trigger mode."""
        if not self.enabled:
            return False
        if self.mode == "continuous":
            return True
        if self.mode == "on_change":
            return dmx_value != self._last_sent
        if self.mode == "threshold_cross":
            was_above = self._above_threshold
            now_above = dmx_value >= self.threshold
            self._above_threshold = now_above
            return now_above != was_above
        return True

    # ------------------------------------------------------------------
    # Serialisation
    # ------------------------------------------------------------------

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "label": self.label,
            "enabled": self.enabled,
            "universe": self.universe,
            "channel": self.channel,
            "host": self.host,
            "port": self.port,
            "address": self.address,
            "in_min": self.in_min,
            "in_max": self.in_max,
            "out_min": self.out_min,
            "out_max": self.out_max,
            "out_type": self.out_type,
            "out_string_on": self.out_string_on,
            "out_string_off": self.out_string_off,
            "mode": self.mode,
            "threshold": self.threshold,
        }

    @classmethod
    def from_dict(cls, d: dict) -> "OSCRoute":
        return cls(
            id=d.get("id", str(uuid.uuid4())[:8]),
            label=d.get("label", ""),
            enabled=bool(d.get("enabled", True)),
            universe=int(d.get("universe", 1)),
            channel=int(d.get("channel", 1)),
            host=d.get("host", "127.0.0.1"),
            port=int(d.get("port", 8000)),
            address=d.get("address", "/lumos/channel"),
            in_min=int(d.get("in_min", 0)),
            in_max=int(d.get("in_max", 255)),
            out_min=float(d.get("out_min", 0.0)),
            out_max=float(d.get("out_max", 1.0)),
            out_type=d.get("out_type", "float"),
            out_string_on=d.get("out_string_on", "on"),
            out_string_off=d.get("out_string_off", "off"),
            mode=d.get("mode", "continuous"),
            threshold=int(d.get("threshold", 128)),
        )
