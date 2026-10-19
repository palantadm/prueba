"""DMX Universe management - supports multiple universes, Art-Net and sACN output."""
import asyncio
import time
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Callable
import struct
import socket


DMX_CHANNELS = 512


@dataclass
class Universe:
    id: int
    name: str = ""
    output_type: str = "none"  # artnet, sacn, none
    artnet_ip: str = "255.255.255.255"
    artnet_net: int = 0
    artnet_subnet: int = 0
    artnet_universe: int = 0
    sacn_ip: str = "239.255.0.1"
    sacn_universe: int = 1

    _data: bytearray = field(default_factory=lambda: bytearray(DMX_CHANNELS), repr=False)
    _dirty: bool = field(default=False, repr=False)
    _last_send: float = field(default=0.0, repr=False)

    def set_channel(self, channel: int, value: int):
        """Set a single channel (0-indexed). value 0-255."""
        if 0 <= channel < DMX_CHANNELS:
            old = self._data[channel]
            self._data[channel] = max(0, min(255, value))
            if old != self._data[channel]:
                self._dirty = True

    def set_channels(self, start: int, values: bytes | bytearray | list):
        """Set multiple consecutive channels starting at start (0-indexed)."""
        for i, v in enumerate(values):
            self.set_channel(start + i, int(v))

    def get_channel(self, channel: int) -> int:
        if 0 <= channel < DMX_CHANNELS:
            return self._data[channel]
        return 0

    def get_all(self) -> bytearray:
        return bytearray(self._data)

    def clear(self):
        self._data = bytearray(DMX_CHANNELS)
        self._dirty = True


class ArtNetSender:
    """Art-Net DMX output over UDP."""

    ARTNET_PORT = 6454
    ARTNET_HEADER = b"Art-Net\x00"
    OPCODE_DMX = 0x5000

    def __init__(self):
        self._sock: Optional[socket.socket] = None

    def open(self):
        self._sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self._sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        self._sock.setblocking(False)

    def close(self):
        if self._sock:
            self._sock.close()
            self._sock = None

    def send(self, universe: Universe):
        if not self._sock:
            return
        sequence = 0
        physical = 0
        port_address = (
            (universe.artnet_net & 0x7F) << 8
            | (universe.artnet_subnet & 0x0F) << 4
            | (universe.artnet_universe & 0x0F)
        )
        data = universe.get_all()
        packet = struct.pack(
            ">8sHBBBBH",
            self.ARTNET_HEADER,
            self.OPCODE_DMX,
            0,  # protocol hi
            14,  # protocol lo
            sequence,
            physical,
            port_address,
        ) + struct.pack(">H", len(data)) + bytes(data)
        # Swap opcode bytes (Art-Net is little-endian for opcode)
        packet = (
            self.ARTNET_HEADER
            + struct.pack("<H", self.OPCODE_DMX)
            + struct.pack(">BBBB", 0, 14, sequence, physical)
            + struct.pack("<H", port_address)
            + struct.pack(">H", len(data))
            + bytes(data)
        )
        try:
            self._sock.sendto(packet, (universe.artnet_ip, self.ARTNET_PORT))
        except OSError:
            pass


class SACNSender:
    """sACN (E1.31) DMX output over UDP multicast."""

    SACN_PORT = 5568

    def __init__(self):
        self._sock: Optional[socket.socket] = None
        self._sequence: Dict[int, int] = {}

    def open(self):
        self._sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
        self._sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, 64)
        self._sock.setblocking(False)

    def close(self):
        if self._sock:
            self._sock.close()
            self._sock = None

    def send(self, universe: Universe):
        if not self._sock:
            return
        u = universe.sacn_universe
        seq = self._sequence.get(u, 0)
        self._sequence[u] = (seq + 1) % 256
        data = universe.get_all()

        # Build E1.31 packet (simplified)
        cid = b'\x00' * 16
        source_name = b'Lumos\x00' + b'\x00' * 58
        priority = 100
        sync_addr = 0
        options = 0

        # PDU lengths
        prop_count = len(data) + 1
        framing_len = 0x7000 | (77 + prop_count)
        dmp_len = 0x7000 | (11 + prop_count)

        root_pdu = (
            struct.pack(">H", 0x0010)  # preamble
            + struct.pack(">H", 0x0000)  # postamble
            + b'\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00'  # ACN packet id
            + struct.pack(">H", 0x7000 | (137 + prop_count))
            + struct.pack(">I", 0x00000004)  # vector
            + cid
        )
        framing_pdu = (
            struct.pack(">H", framing_len)
            + struct.pack(">I", 0x00000002)
            + source_name
            + struct.pack(">BB", priority, 0)
            + struct.pack(">H", sync_addr)
            + struct.pack(">B", seq)
            + struct.pack(">B", options)
            + struct.pack(">H", u)
        )
        dmp_pdu = (
            struct.pack(">H", dmp_len)
            + struct.pack(">B", 0x02)
            + struct.pack(">B", 0xa1)
            + struct.pack(">H", 0x0000)
            + struct.pack(">H", 0x0001)
            + struct.pack(">H", prop_count)
            + struct.pack(">B", 0x00)
            + bytes(data)
        )
        packet = root_pdu + framing_pdu + dmp_pdu
        ip = f"239.255.{(u >> 8) & 0xFF}.{u & 0xFF}"
        try:
            self._sock.sendto(packet, (ip, self.SACN_PORT))
        except OSError:
            pass


class UniverseManager:
    """Manages all DMX universes and output drivers."""

    def __init__(self):
        self.universes: Dict[int, Universe] = {}
        self._artnet = ArtNetSender()
        self._sacn = SACNSender()
        self._running = False
        self._task: Optional[asyncio.Task] = None
        self._subscribers: List[Callable] = []

    def add_universe(self, uid: int, name: str = "", output_type: str = "none", **kwargs) -> Universe:
        u = Universe(id=uid, name=name or f"Universe {uid}", output_type=output_type, **kwargs)
        self.universes[uid] = u
        return u

    def get_universe(self, uid: int) -> Optional[Universe]:
        return self.universes.get(uid)

    def subscribe(self, callback: Callable):
        """Subscribe to DMX updates (for websocket broadcast)."""
        self._subscribers.append(callback)

    def unsubscribe(self, callback: Callable):
        if callback in self._subscribers:
            self._subscribers.remove(callback)

    async def start(self):
        self._artnet.open()
        self._sacn.open()
        self._running = True
        self._task = asyncio.create_task(self._output_loop())

    async def stop(self):
        self._running = False
        if self._task:
            self._task.cancel()
        self._artnet.close()
        self._sacn.close()

    async def _output_loop(self):
        """Send DMX at ~44Hz for dirty universes, refresh all at 1Hz."""
        while self._running:
            now = time.monotonic()
            for u in self.universes.values():
                should_send = u._dirty or (now - u._last_send > 1.0)
                if should_send:
                    if u.output_type == "artnet":
                        self._artnet.send(u)
                    elif u.output_type == "sacn":
                        self._sacn.send(u)
                    u._dirty = False
                    u._last_send = now
                    for cb in self._subscribers:
                        try:
                            await cb(u.id, u.get_all())
                        except Exception:
                            pass
            await asyncio.sleep(1 / 44)
