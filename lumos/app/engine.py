"""
Lumos Engine — Qt-native version.

Owns all DMX universes and drives the output loop via QTimer.
No asyncio: everything runs on the Qt event loop.
"""
from __future__ import annotations

import socket
import struct
import time
from dataclasses import dataclass, field
from typing import Callable, Optional

from PySide6.QtCore import QObject, QTimer, Signal, Slot


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------

@dataclass
class Universe:
    id: int
    name: str
    data: bytearray = field(default_factory=lambda: bytearray(512))
    _dirty: bool = False

    def set_channel(self, ch: int, value: int) -> None:
        """ch is 1-indexed."""
        if 1 <= ch <= 512:
            v = max(0, min(255, value))
            if self.data[ch - 1] != v:
                self.data[ch - 1] = v
                self._dirty = True

    def set_all(self, data: bytes | bytearray) -> None:
        new = bytearray(data[:512]).ljust(512, b"\x00")
        if self.data != new:
            self.data[:] = new
            self._dirty = True

    def zero(self) -> None:
        if any(self.data):
            self.data[:] = bytearray(512)
            self._dirty = True


# ---------------------------------------------------------------------------
# Cue / CueList data model
# ---------------------------------------------------------------------------

@dataclass
class Cue:
    number: float           # 1.0, 2.0, 2.5, etc.
    name: str
    universe: int
    data: bytearray         # 512-byte snapshot
    fade_in: float = 3.0    # seconds
    fade_out: float = 3.0
    follow_wait: float = 0.0  # 0 = manual, >0 = auto-follow after N seconds


@dataclass
class CueList:
    id: int
    name: str
    cues: list = field(default_factory=list)  # list[Cue]
    _current_idx: int = field(default=-1, repr=False)

    @property
    def current_cue(self) -> Optional[Cue]:
        if 0 <= self._current_idx < len(self.cues):
            return self.cues[self._current_idx]
        return None

    @property
    def next_cue(self) -> Optional[Cue]:
        if self._current_idx + 1 < len(self.cues):
            return self.cues[self._current_idx + 1]
        return None


# ---------------------------------------------------------------------------
# Art-Net output (pure UDP, no driver required)
# ---------------------------------------------------------------------------

class ArtNetOutput:
    _HEADER = b"Art-Net\x00\x00\x50\x00\x0e"  # id + OpDmx + ver

    def __init__(self, ip: str = "255.255.255.255", port: int = 6454):
        self.ip = ip
        self.port = port
        self._sock: Optional[socket.socket] = None

    def open(self) -> None:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        sock.setblocking(False)
        self._sock = sock

    def close(self) -> None:
        if self._sock:
            self._sock.close()
            self._sock = None

    def send(self, universe_id: int, data: bytearray) -> None:
        if not self._sock:
            return
        net = (universe_id >> 8) & 0xFF
        sub = universe_id & 0xFF
        packet = (
            self._HEADER
            + bytes([0, sub, net])     # sequence=0, physical=0, universe LE
            + struct.pack(">H", 512)   # length big-endian
            + bytes(data[:512]).ljust(512, b"\x00")
        )
        try:
            self._sock.sendto(packet, (self.ip, self.port))
        except Exception:
            pass


# ---------------------------------------------------------------------------
# Engine
# ---------------------------------------------------------------------------

class LumosEngine(QObject):
    """Central engine.  All UI panels talk to this object."""

    # universe_id, full 512-byte snapshot
    dmx_updated = Signal(int, bytes)
    grand_master_changed = Signal(float)
    blackout_changed = Signal(bool)
    scene_saved = Signal(str)  # scene name

    # Cuelist signals
    cuelist_changed = Signal(int)         # cuelist id
    cue_activated = Signal(int, float)    # cuelist_id, cue_number

    TICK_MS = 23  # ~44 Hz

    def __init__(self, parent: Optional[QObject] = None) -> None:
        super().__init__(parent)

        self.universes: dict[int, Universe] = {}
        self.scenes: dict[str, bytearray] = {}
        self._gm: float = 1.0
        self._blackout: bool = False
        self._artnet = ArtNetOutput()

        # Cuelist state
        self.cuelists: dict[int, CueList] = {}
        self._next_cuelist_id: int = 1

        # Fade engine: universe_id → {from_data, to_data, elapsed_ms, duration_ms}
        self._fades: dict[int, dict] = {}

        # Auto-follow timers: cuelist_id → remaining ms
        self._follow_timers: dict[int, float] = {}

        self._timer = QTimer(self)
        self._timer.setInterval(self.TICK_MS)
        self._timer.timeout.connect(self._tick)

        # Add two universes by default
        self.add_universe(1, "Universe 1")
        self.add_universe(2, "Universe 2")

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    def start(self) -> None:
        self._artnet.open()
        self._timer.start()

    def stop(self) -> None:
        self._timer.stop()
        self._artnet.close()

    # ------------------------------------------------------------------
    # Universe management
    # ------------------------------------------------------------------

    def add_universe(self, uid: int, name: str = "") -> Universe:
        u = Universe(id=uid, name=name or f"Universe {uid}")
        self.universes[uid] = u
        return u

    def remove_universe(self, uid: int) -> None:
        self.universes.pop(uid, None)

    # ------------------------------------------------------------------
    # Channel control
    # ------------------------------------------------------------------

    def set_channel(self, universe: int, channel: int, value: int) -> None:
        if universe in self.universes:
            self.universes[universe].set_channel(channel, value)

    def set_channels(self, universe: int, start: int, values: list[int]) -> None:
        if universe in self.universes:
            u = self.universes[universe]
            for i, v in enumerate(values):
                u.set_channel(start + i, v)

    def get_channel(self, universe: int, channel: int) -> int:
        if universe in self.universes and 1 <= channel <= 512:
            return self.universes[universe].data[channel - 1]
        return 0

    def zero_universe(self, uid: int) -> None:
        if uid in self.universes:
            self.universes[uid].zero()

    # ------------------------------------------------------------------
    # Scenes
    # ------------------------------------------------------------------

    def save_scene(self, name: str, universe: int) -> None:
        if universe in self.universes:
            self.scenes[name] = bytearray(self.universes[universe].data)
            self.scene_saved.emit(name)

    def recall_scene(self, name: str, universe: int) -> None:
        if name in self.scenes and universe in self.universes:
            self.universes[universe].set_all(self.scenes[name])
            self.universes[universe]._dirty = True

    def delete_scene(self, name: str) -> None:
        self.scenes.pop(name, None)

    # ------------------------------------------------------------------
    # Cuelist management
    # ------------------------------------------------------------------

    def create_cuelist(self, name: str) -> CueList:
        cl = CueList(id=self._next_cuelist_id, name=name)
        self.cuelists[self._next_cuelist_id] = cl
        self._next_cuelist_id += 1
        self.cuelist_changed.emit(cl.id)
        return cl

    def delete_cuelist(self, cuelist_id: int) -> None:
        if cuelist_id in self.cuelists:
            del self.cuelists[cuelist_id]
            self._follow_timers.pop(cuelist_id, None)
            self.cuelist_changed.emit(cuelist_id)

    def record_cue(
        self,
        cuelist_id: int,
        cue_number: float,
        name: str,
        universe: int,
        fade_in: float = 3.0,
        fade_out: float = 3.0,
    ) -> Optional[Cue]:
        if cuelist_id not in self.cuelists:
            return None
        if universe not in self.universes:
            return None
        cl = self.cuelists[cuelist_id]
        snapshot = bytearray(self.universes[universe].data)
        cue = Cue(
            number=cue_number,
            name=name,
            universe=universe,
            data=snapshot,
            fade_in=fade_in,
            fade_out=fade_out,
        )
        # Insert in sorted order by cue number
        inserted = False
        for i, existing in enumerate(cl.cues):
            if cue_number < existing.number:
                cl.cues.insert(i, cue)
                inserted = True
                break
            elif cue_number == existing.number:
                cl.cues[i] = cue  # replace
                inserted = True
                break
        if not inserted:
            cl.cues.append(cue)
        self.cuelist_changed.emit(cuelist_id)
        return cue

    def go(self, cuelist_id: int) -> None:
        """Advance to next cue and start fade."""
        if cuelist_id not in self.cuelists:
            return
        cl = self.cuelists[cuelist_id]
        if not cl.cues:
            return
        new_idx = cl._current_idx + 1
        if new_idx >= len(cl.cues):
            new_idx = 0  # wrap around
        self._activate_cue(cl, new_idx)

    def back(self, cuelist_id: int) -> None:
        """Go to previous cue."""
        if cuelist_id not in self.cuelists:
            return
        cl = self.cuelists[cuelist_id]
        if not cl.cues:
            return
        new_idx = cl._current_idx - 1
        if new_idx < 0:
            new_idx = len(cl.cues) - 1
        self._activate_cue(cl, new_idx)

    def stop_fade(self, cuelist_id: int) -> None:
        """Snap current fade to target immediately."""
        if cuelist_id not in self.cuelists:
            return
        cl = self.cuelists[cuelist_id]
        cue = cl.current_cue
        if cue is None:
            return
        uid = cue.universe
        if uid in self._fades:
            # Snap to target
            if uid in self.universes:
                self.universes[uid].set_all(self._fades[uid]["to_data"])
            del self._fades[uid]
        self._follow_timers.pop(cuelist_id, None)

    def _activate_cue(self, cl: CueList, idx: int) -> None:
        cl._current_idx = idx
        cue = cl.cues[idx]
        uid = cue.universe
        if uid in self.universes:
            from_data = bytearray(self.universes[uid].data)
            duration_ms = int(cue.fade_in * 1000)
            self._start_fade(uid, from_data, cue.data, duration_ms)
        self.cue_activated.emit(cl.id, cue.number)
        self.cuelist_changed.emit(cl.id)
        # Set up auto-follow if applicable
        if cue.follow_wait > 0:
            self._follow_timers[cl.id] = cue.follow_wait * 1000  # ms
        else:
            self._follow_timers.pop(cl.id, None)

    # ------------------------------------------------------------------
    # Fade engine
    # ------------------------------------------------------------------

    def _start_fade(
        self, universe: int, from_data: bytearray, to_data: bytearray, duration_ms: int
    ) -> None:
        if duration_ms <= 0:
            if universe in self.universes:
                self.universes[universe].set_all(to_data)
            return
        self._fades[universe] = {
            "from_data": bytearray(from_data),
            "to_data": bytearray(to_data),
            "elapsed_ms": 0,
            "duration_ms": duration_ms,
        }

    # ------------------------------------------------------------------
    # Grand master / blackout
    # ------------------------------------------------------------------

    @property
    def grand_master(self) -> float:
        return self._gm

    @grand_master.setter
    def grand_master(self, value: float) -> None:
        self._gm = max(0.0, min(1.0, value))
        for u in self.universes.values():
            u._dirty = True
        self.grand_master_changed.emit(self._gm)

    @property
    def blackout(self) -> bool:
        return self._blackout

    @blackout.setter
    def blackout(self, on: bool) -> None:
        self._blackout = on
        for u in self.universes.values():
            u._dirty = True
        self.blackout_changed.emit(on)

    def toggle_blackout(self) -> None:
        self.blackout = not self._blackout

    # ------------------------------------------------------------------
    # Internal tick
    # ------------------------------------------------------------------

    @Slot()
    def _tick(self) -> None:
        # Process fades
        completed_fades = []
        for uid, fade in self._fades.items():
            fade["elapsed_ms"] += self.TICK_MS
            t = min(1.0, fade["elapsed_ms"] / fade["duration_ms"])
            # Linear interpolation
            if uid in self.universes:
                interp = bytearray(
                    int(a + (b - a) * t)
                    for a, b in zip(fade["from_data"], fade["to_data"])
                )
                self.universes[uid].set_all(interp)
            if t >= 1.0:
                completed_fades.append(uid)
        for uid in completed_fades:
            del self._fades[uid]

        # Process auto-follow timers
        completed_follows = []
        for cl_id, remaining_ms in self._follow_timers.items():
            remaining_ms -= self.TICK_MS
            if remaining_ms <= 0:
                completed_follows.append(cl_id)
            else:
                self._follow_timers[cl_id] = remaining_ms
        for cl_id in completed_follows:
            del self._follow_timers[cl_id]
            self.go(cl_id)

        # Output loop
        for uid, u in self.universes.items():
            if u._dirty:
                if self._blackout:
                    out = bytearray(512)
                elif self._gm < 1.0:
                    out = bytearray(int(b * self._gm) for b in u.data)
                else:
                    out = u.data
                self._artnet.send(uid, out)
                self.dmx_updated.emit(uid, bytes(out))
                u._dirty = False
