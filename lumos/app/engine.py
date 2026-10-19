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
    # grand master changed (0.0 – 1.0)
    grand_master_changed = Signal(float)
    # blackout toggled
    blackout_changed = Signal(bool)

    TICK_MS = 23  # ~44 Hz

    def __init__(self, parent: Optional[QObject] = None) -> None:
        super().__init__(parent)

        self.universes: dict[int, Universe] = {}
        self._gm: float = 1.0          # grand master 0.0–1.0
        self._blackout: bool = False
        self._artnet = ArtNetOutput()

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
