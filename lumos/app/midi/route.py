"""
MIDIRoute — maps an incoming MIDI message to a Lumos action.
"""
from __future__ import annotations

import uuid
from dataclasses import dataclass, field
from typing import Optional


@dataclass
class MIDIRoute:
    """One MIDI → Lumos mapping."""

    id: str = field(default_factory=lambda: str(uuid.uuid4())[:8])
    label: str = ""
    enabled: bool = True

    # ── MIDI source ──────────────────────────────────────────────────────────
    port_name: str = ""          # "" = any open port
    msg_type: str = "control_change"  # "note_on" | "note_off" | "control_change" | "program_change"
    midi_channel: int = 0        # 0 = any, 1-16 = specific
    note_or_cc: int = 0          # note number (0-127) or CC number (0-127)

    # ── Action ───────────────────────────────────────────────────────────────
    action: str = "dmx_channel"
    # Actions:
    #   "dmx_channel"       — map value → DMX channel
    #   "scene_activate"    — activate a scene by name
    #   "cue_go"            — GO on a cuelist
    #   "cue_back"          — BACK on a cuelist
    #   "blackout_toggle"   — toggle blackout
    #   "grand_master"      — set grand master level

    # DMX target (for "dmx_channel" and "grand_master")
    universe: int = 1
    dmx_channel: int = 1         # 1-512

    # Scene target
    scene_name: str = ""

    # Cuelist target
    cuelist_id: int = 1

    # ── Value mapping ────────────────────────────────────────────────────────
    # MIDI value [in_min, in_max] → output [out_min, out_max]
    in_min: int = 0
    in_max: int = 127
    out_min: int = 0
    out_max: int = 255

    # For note_on/off: trigger on note-on only (velocity > 0 → action)
    note_trigger: str = "on_press"  # "on_press" | "on_release" | "both"

    # ── Private runtime state ────────────────────────────────────────────────
    _last_value: Optional[int] = field(default=None, repr=False, compare=False)

    # ------------------------------------------------------------------

    def matches(self, msg_type: str, midi_channel: int, note_or_cc: int, port_name: str) -> bool:
        if not self.enabled:
            return False
        if self.msg_type != msg_type:
            return False
        if self.midi_channel != 0 and self.midi_channel != midi_channel:
            return False
        if self.note_or_cc != note_or_cc:
            return False
        if self.port_name and self.port_name != port_name:
            return False
        return True

    def map_value(self, midi_value: int) -> int:
        """Map MIDI value [in_min, in_max] → int [out_min, out_max]."""
        span_in = self.in_max - self.in_min
        if span_in == 0:
            return self.out_min
        t = max(0.0, min(1.0, (midi_value - self.in_min) / span_in))
        return int(round(self.out_min + t * (self.out_max - self.out_min)))

    # ── Serialisation ────────────────────────────────────────────────────────

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "label": self.label,
            "enabled": self.enabled,
            "port_name": self.port_name,
            "msg_type": self.msg_type,
            "midi_channel": self.midi_channel,
            "note_or_cc": self.note_or_cc,
            "action": self.action,
            "universe": self.universe,
            "dmx_channel": self.dmx_channel,
            "scene_name": self.scene_name,
            "cuelist_id": self.cuelist_id,
            "in_min": self.in_min,
            "in_max": self.in_max,
            "out_min": self.out_min,
            "out_max": self.out_max,
            "note_trigger": self.note_trigger,
        }

    @classmethod
    def from_dict(cls, d: dict) -> "MIDIRoute":
        return cls(
            id=d.get("id", str(uuid.uuid4())[:8]),
            label=d.get("label", ""),
            enabled=bool(d.get("enabled", True)),
            port_name=d.get("port_name", ""),
            msg_type=d.get("msg_type", "control_change"),
            midi_channel=int(d.get("midi_channel", 0)),
            note_or_cc=int(d.get("note_or_cc", 0)),
            action=d.get("action", "dmx_channel"),
            universe=int(d.get("universe", 1)),
            dmx_channel=int(d.get("dmx_channel", 1)),
            scene_name=d.get("scene_name", ""),
            cuelist_id=int(d.get("cuelist_id", 1)),
            in_min=int(d.get("in_min", 0)),
            in_max=int(d.get("in_max", 127)),
            out_min=int(d.get("out_min", 0)),
            out_max=int(d.get("out_max", 255)),
            note_trigger=d.get("note_trigger", "on_press"),
        )
