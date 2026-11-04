"""Show file — save and load all Lumos state as a single JSON document.

A show captures:
  - Universes (ids, names, output config)
  - Patch (fixture instances)
  - Cue lists + cues + values
  - Effects
  - Groups / submasters / grand master
  - MIDI bindings
  - OSC feedback targets
  - Protocol outputs (Art-Net IPs, sACN universes, KiNET devices, serial ports)

Version field allows future migrations.
"""
import json
import os
import time
from pathlib import Path
from typing import Any, Optional


SHOW_VERSION = "1.0"


class ShowManager:
    """Serialises and deserialises the entire show state."""

    def __init__(self, universes, patch, cues, effects, groups,
                 midi_driver=None, osc_server=None):
        self.universes = universes
        self.patch = patch
        self.cues = cues
        self.effects = effects
        self.groups = groups
        self.midi_driver = midi_driver
        self.osc_server = osc_server

        self.current_path: Optional[str] = None
        self._autosave_path: Optional[str] = None

    # ── Serialisation ─────────────────────────────────────────────────────────

    def to_dict(self) -> dict:
        data: dict[str, Any] = {
            "version": SHOW_VERSION,
            "saved_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "universes": [u.to_dict() for u in self.universes.universes.values()],
            "patch": self.patch.to_dict(),
            "cue_lists": self.cues.to_dict(),
            "effects": self.effects.to_dict(),
            "groups": self.groups.to_dict(),
        }

        if self.midi_driver:
            data["midi_bindings"] = self.midi_driver.bindings_to_dict()
            data["midi_in_port"] = getattr(self.midi_driver._in_port, "name", "")
            data["midi_out_port"] = getattr(self.midi_driver._out_port, "name", "")

        if self.osc_server:
            data["osc_feedback_targets"] = list(self.osc_server._feedback_targets)
            data["osc_port"] = self.osc_server.port

        return data

    # ── Deserialisation ───────────────────────────────────────────────────────

    def from_dict(self, data: dict):
        """Restore state from a previously saved dict.
        Does NOT restart async tasks — caller must do that.
        """
        from ..engine.universe import Universe

        # Universes
        from ..engine.universe import Universe, UniverseOutput
        self.universes.universes.clear()
        for ud in data.get("universes", []):
            uid = ud["id"]
            u = Universe(id=uid, name=ud.get("name", f"Universe {uid}"))
            for od in ud.get("outputs", []):
                u.outputs.append(UniverseOutput(**od))
            self.universes.universes[uid] = u

        # Patch: rebuild using library entries already loaded
        self.patch.fixtures.clear()
        for fd in data.get("patch", []):
            fid = fd.get("id")
            def_id = fd.get("definition_id", "")
            inst = self.patch.patch(
                definition_id=def_id,
                name=fd.get("name", ""),
                universe=fd.get("universe", 1),
                address=fd.get("address", 1),
                mode=fd.get("mode", ""),
                group=fd.get("group", "Default"),
            )
            if inst and fid:
                inst.id = fid  # preserve original IDs for cue references

        # Cue lists
        from ..engine.cue import CueList, Cue
        self.cues.cue_lists.clear()
        for cld in data.get("cue_lists", []):
            cl = CueList(
                id=cld["id"],
                name=cld["name"],
                loop=cld.get("loop", False),
            )
            for cd in cld.get("cues", []):
                cue = Cue(
                    id=cd["id"],
                    name=cd["name"],
                    fade_in=cd.get("fade_in", 0.0),
                    hold=cd.get("hold", 0.0),
                    fade_type=cd.get("fade_type", "linear"),
                )
                for fid, ch_vals in cd.get("channels", {}).items():
                    for ch, val in ch_vals.items():
                        cue.set_channel(fid, ch, val)
                cl.add_cue(cue)
            self.cues.cue_lists[cl.id] = cl

        # Effects
        from ..engine.effects import Effect, EffectChannel, Waveform
        self.effects.effects.clear()
        for ed in data.get("effects", []):
            channels = [
                EffectChannel(
                    fixture_id=c["fixture_id"],
                    channel=c["channel"],
                    phase=c.get("phase", 0.0),
                )
                for c in ed.get("channels", [])
            ]
            eff = Effect(
                id=ed["id"],
                name=ed["name"],
                waveform=Waveform(ed.get("waveform", "sine")),
                channels=channels,
                speed=ed.get("speed", 1.0),
                amplitude=ed.get("amplitude", 127),
                center=ed.get("center", 127),
                width=ed.get("width", 0.5),
                steps=ed.get("steps", [0, 64, 128, 192, 255]),
                phase_spread=ed.get("phase_spread", 0.0),
                bpm_sync=ed.get("bpm_sync", False),
                bpm_multiplier=ed.get("bpm_multiplier", 1.0),
                one_shot=ed.get("one_shot", False),
                enabled=ed.get("enabled", True),
            )
            self.effects.effects[eff.id] = eff

        # Groups
        self.groups.from_dict(data.get("groups", {}))

        # MIDI bindings
        if self.midi_driver:
            from ..protocols.midi_protocol import MIDIBinding
            self.midi_driver.bindings.clear()
            for bd in data.get("midi_bindings", []):
                self.midi_driver.bindings.append(MIDIBinding(**bd))

        # OSC
        if self.osc_server:
            self.osc_server._feedback_targets = [
                tuple(t) for t in data.get("osc_feedback_targets", [])
            ]

    # ── File I/O ──────────────────────────────────────────────────────────────

    def save(self, path: str) -> bool:
        try:
            data = self.to_dict()
            Path(path).parent.mkdir(parents=True, exist_ok=True)
            tmp = path + ".tmp"
            with open(tmp, "w", encoding="utf-8") as f:
                json.dump(data, f, indent=2)
            os.replace(tmp, path)
            self.current_path = path
            return True
        except Exception as e:
            print(f"[Show] save failed: {e}")
            return False

    def load(self, path: str) -> bool:
        try:
            with open(path, "r", encoding="utf-8") as f:
                data = json.load(f)
            self.from_dict(data)
            self.current_path = path
            return True
        except Exception as e:
            print(f"[Show] load failed: {e}")
            return False

    def set_autosave(self, path: str):
        self._autosave_path = path

    def autosave(self):
        if self._autosave_path:
            self.save(self._autosave_path)
