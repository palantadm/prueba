"""
Art-Net ArtDmx UDP output backend.
"""
from __future__ import annotations

import socket
import struct
from typing import Optional

from .base import OutputBackend


class ArtNetBackend(OutputBackend):
    """Sends Art-Net ArtDmx packets via UDP broadcast/unicast."""

    name: str = "Art-Net (UDP)"

    # OpDmx header: "Art-Net\0" + OpCode(0x5000 LE) + ProtVer(0x000e BE)
    _HEADER = b"Art-Net\x00\x00\x50\x00\x0e"

    def __init__(self, ip: str = "255.255.255.255", port: int = 6454) -> None:
        self.ip = ip
        self.port = port
        self._sock: Optional[socket.socket] = None
        self._sequence: int = 0

    # ------------------------------------------------------------------
    # OutputBackend interface
    # ------------------------------------------------------------------

    def open(self) -> bool:
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            sock.setblocking(False)
            self._sock = sock
            return True
        except OSError:
            self._sock = None
            return False

    def close(self) -> None:
        if self._sock:
            self._sock.close()
            self._sock = None

    def send(self, universe_id: int, data: bytearray) -> None:
        if not self._sock:
            return
        self._sequence = (self._sequence + 1) & 0xFF
        # Universe: low byte first (little-endian) as per Art-Net spec
        uni_lo = universe_id & 0xFF
        uni_hi = (universe_id >> 8) & 0xFF
        frame = bytes(data[:512]).ljust(512, b"\x00")
        packet = (
            self._HEADER
            + bytes([self._sequence, 0, uni_lo, uni_hi])  # seq, physical, uni LE
            + struct.pack(">H", 512)                       # length BE
            + frame
        )
        try:
            self._sock.sendto(packet, (self.ip, self.port))
        except Exception:
            pass

    @property
    def is_open(self) -> bool:
        return self._sock is not None

    def config_dict(self) -> dict:
        return {
            "type": "artnet",
            "ip": self.ip,
            "port": self.port,
        }

    @classmethod
    def from_config(cls, cfg: dict) -> "ArtNetBackend":
        return cls(
            ip=cfg.get("ip", "255.255.255.255"),
            port=int(cfg.get("port", 6454)),
        )
