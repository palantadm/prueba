"""
sACN E1.31 output backend (multicast or unicast UDP).
"""
from __future__ import annotations

import socket
import struct
import uuid
from typing import Optional

from .base import OutputBackend


class SACNBackend(OutputBackend):
    """
    Sends sACN (E1.31) data packets via UDP.

    If *ip* is empty the backend uses multicast (239.255.X.Y).
    If *ip* is set it uses unicast to that address.
    """

    name: str = "sACN E1.31"

    # E1.31 vectors
    _VECTOR_ROOT_E131_DATA = 0x00000004
    _VECTOR_E131_DATA_PACKET = 0x00000002
    _VECTOR_DMP_SET_PROPERTY = 0x02

    # Total packet length for 512 channels (per E1.31 spec)
    _PACKET_LEN = 638

    def __init__(
        self,
        ip: str = "",
        port: int = 5568,
        priority: int = 100,
        source_name: str = "Lumos",
    ) -> None:
        self.ip = ip
        self.port = port
        self.priority = priority
        self.source_name = source_name
        # Random CID (Component Identifier) — 16-byte UUID
        self._cid: bytes = uuid.uuid4().bytes
        self._sequence: int = 0
        self._sock: Optional[socket.socket] = None

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    def _multicast_addr(self, universe_id: int) -> str:
        """Map universe to 239.255.X.Y multicast address."""
        hi = (universe_id >> 8) & 0xFF
        lo = universe_id & 0xFF
        return f"239.255.{hi}.{lo}"

    def _build_packet(self, universe_id: int, data: bytearray) -> bytes:
        """Build a complete E1.31 data packet for 512 channels."""
        L = self._PACKET_LEN  # 638

        # --- DMP PDU ---
        # Flags+Len covers DMP layer itself
        dmp_content_len = 1 + 1 + 2 + 2 + 2 + 1 + 512  # = 521
        dmp_flags_len = 0x7000 | (dmp_content_len + 2)   # +2 for flags/len field = 523

        dmp = struct.pack(
            ">HBBHHHb",
            dmp_flags_len,            # Flags+Len (2)
            self._VECTOR_DMP_SET_PROPERTY,  # Vector (1) = 0x02
            0xA1,                     # Addr/Data Type (1)
            0x0000,                   # First Property Addr (2)
            0x0001,                   # Address Increment (2)
            0x0201,                   # Property count = 513 (start code + 512) (2)
            0x00,                     # Start code (1)
        )
        dmp += bytes(data[:512]).ljust(512, b"\x00")

        # --- Framing PDU ---
        src_name = self.source_name.encode("utf-8")[:63].ljust(64, b"\x00")
        self._sequence = (self._sequence + 1) & 0xFF

        framing_content_len = 4 + 64 + 1 + 2 + 1 + 1 + 2 + len(dmp)
        framing_flags_len = 0x7000 | (framing_content_len + 2)

        framing = struct.pack(">I", self._VECTOR_E131_DATA_PACKET)  # Vector (4)
        framing += src_name                                          # Source Name (64)
        framing += struct.pack(
            ">BHBBHbb",
            self.priority,   # Priority (1)
            0x0000,          # Synchronisation Address (2)
            self._sequence,  # Sequence Number (1)
            0x00,            # Options (1)
            universe_id,     # Universe (2)
            0, 0,            # padding to align (unused but keeps struct clean)
        )
        # Redo — struct.pack with mixed types needs care; build manually
        framing = (
            struct.pack(">I", self._VECTOR_E131_DATA_PACKET)
            + src_name
            + bytes([self.priority])
            + struct.pack(">H", 0x0000)       # Synchronization
            + bytes([self._sequence])
            + bytes([0x00])                   # Options
            + struct.pack(">H", universe_id)
            + dmp
        )
        # Recalculate framing length now we have the actual bytes
        framing_payload_len = len(framing) + 2  # +2 for flags/len
        framing_flags_len = 0x7000 | framing_payload_len

        framing_pdu = struct.pack(">H", framing_flags_len) + framing

        # --- Root PDU ---
        root_vector = struct.pack(">I", self._VECTOR_ROOT_E131_DATA)
        root_payload = root_vector + self._cid + framing_pdu
        root_payload_len = len(root_payload) + 2  # +2 for flags/len
        root_flags_len = 0x7000 | root_payload_len
        root_pdu = struct.pack(">H", root_flags_len) + root_payload

        # --- Preamble ---
        preamble = (
            struct.pack(">H", 0x0010)        # Preamble Size
            + struct.pack(">H", 0x0000)      # Postamble Size
            + b"ASC-E1.17\x00\x00\x00"       # ACN Packet Identifier (12 bytes)
        )

        return preamble + root_pdu

    # ------------------------------------------------------------------
    # OutputBackend interface
    # ------------------------------------------------------------------

    def open(self) -> bool:
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            if not self.ip:
                # Multicast: set TTL
                sock.setsockopt(
                    socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, 1
                )
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
        packet = self._build_packet(universe_id, data)
        dest = self.ip if self.ip else self._multicast_addr(universe_id)
        try:
            self._sock.sendto(packet, (dest, self.port))
        except Exception:
            pass

    @property
    def is_open(self) -> bool:
        return self._sock is not None

    def config_dict(self) -> dict:
        return {
            "type": "sacn",
            "ip": self.ip,
            "port": self.port,
            "priority": self.priority,
            "source_name": self.source_name,
        }

    @classmethod
    def from_config(cls, cfg: dict) -> "SACNBackend":
        return cls(
            ip=cfg.get("ip", ""),
            port=int(cfg.get("port", 5568)),
            priority=int(cfg.get("priority", 100)),
            source_name=cfg.get("source_name", "Lumos"),
        )
