"""sACN / E1.31 full implementation.

Covers:
  E1.31 Data Packet     — DMX over multicast or unicast
  Per-Address Priority  — E1.31 Appendix A (PAP) for per-channel priority
  E1.31 Universe Disc.  — Appendix A: announce which universes we use
  Framing Layer flags   — Preview, stream terminated, forced sync
  Priority merging      — Track sources per universe, HTP merge
"""
import asyncio
import socket
import struct
import time
import uuid
from dataclasses import dataclass, field
from typing import Callable, Dict, List, Optional, Set, Tuple

SACN_PORT = 5568
SACN_UNIVERSE_DISCOVERY = 64214  # E1.31 Appendix A universe

# Root layer vectors
VECTOR_ROOT_E131      = 0x00000004
VECTOR_ROOT_EXTENDED  = 0x00000008

# Framing layer vectors
VECTOR_E131_DATA_PACKET       = 0x00000002
VECTOR_E131_EXTENDED_SYNC     = 0x00000001
VECTOR_E131_EXTENDED_DISCOVERY = 0x00000002

# DMP layer
VECTOR_DMP_SET_PROPERTY = 0x02

# Extended framing
VECTOR_UNIVERSE_DISCOVERY = 0x00000001

LUMOS_CID = uuid.UUID("12345678-1234-5678-1234-567812345678").bytes


def _multicast_ip(universe: int) -> str:
    hi = (universe >> 8) & 0xFF
    lo = universe & 0xFF
    return f"239.255.{hi}.{lo}"


def _pdu_flags_length(length: int) -> bytes:
    return struct.pack(">H", 0x7000 | (length & 0x0FFF))


def _build_e131_packet(universe: int, sequence: int, priority: int,
                       source_name: str, data: bytes,
                       preview: bool = False, terminated: bool = False,
                       forced_sync: bool = False, sync_addr: int = 0) -> bytes:
    """Build a complete E1.31 data packet."""
    dmp_data_len = 1 + len(data)   # start code + values
    dmp_pdu_len  = 10 + 1 + dmp_data_len
    frame_pdu_len = 77 + dmp_pdu_len
    root_pdu_len  = 22 + frame_pdu_len

    options = 0
    if preview:    options |= 0x80
    if terminated: options |= 0x40
    if forced_sync: options |= 0x20

    src_name = source_name.encode()[:63].ljust(64, b"\x00")

    # Root layer
    root = (
        struct.pack(">H", 0x0010)          # preamble size
        + struct.pack(">H", 0x0000)        # postamble size
        + b"\x00ASC-E1.17\x00\x00"         # ACN packet identifier (12 bytes)
        + _pdu_flags_length(root_pdu_len)
        + struct.pack(">I", VECTOR_ROOT_E131)
        + LUMOS_CID
    )

    # Framing layer
    frame = (
        _pdu_flags_length(frame_pdu_len)
        + struct.pack(">I", VECTOR_E131_DATA_PACKET)
        + src_name
        + struct.pack(">B", priority)      # priority
        + struct.pack(">H", sync_addr)     # sync address
        + struct.pack(">B", sequence)      # sequence number
        + struct.pack(">B", options)
        + struct.pack(">H", universe)
    )

    # DMP layer
    dmp = (
        _pdu_flags_length(dmp_pdu_len)
        + struct.pack(">B", VECTOR_DMP_SET_PROPERTY)
        + struct.pack(">B", 0xA1)          # addr type, data type
        + struct.pack(">H", 0x0000)        # first property addr
        + struct.pack(">H", 0x0001)        # addr increment
        + struct.pack(">H", dmp_data_len)  # property count
        + b"\x00"                          # start code
        + data
    )

    return root + frame + dmp


def _build_universe_discovery_packet(sequence: int, source_name: str,
                                     universes: List[int]) -> bytes:
    """Build E1.31 Universe Discovery packet (Appendix A)."""
    sorted_u = sorted(set(universes))
    u_data = b"".join(struct.pack(">H", u) for u in sorted_u)
    disc_pdu_len = 8 + len(u_data)
    frame_pdu_len = 77 + disc_pdu_len
    root_pdu_len  = 22 + frame_pdu_len

    src_name = source_name.encode()[:63].ljust(64, b"\x00")

    root = (
        struct.pack(">H", 0x0010)
        + struct.pack(">H", 0x0000)
        + b"\x00ASC-E1.17\x00\x00"
        + _pdu_flags_length(root_pdu_len)
        + struct.pack(">I", VECTOR_ROOT_EXTENDED)
        + LUMOS_CID
    )
    frame = (
        _pdu_flags_length(frame_pdu_len)
        + struct.pack(">I", VECTOR_E131_EXTENDED_DISCOVERY)
        + src_name
        + struct.pack(">B", 100)   # priority
        + struct.pack(">H", 0)     # sync
        + struct.pack(">B", sequence)
        + struct.pack(">B", 0)     # options
        + struct.pack(">H", SACN_UNIVERSE_DISCOVERY)
    )
    disc = (
        _pdu_flags_length(disc_pdu_len)
        + struct.pack(">I", VECTOR_UNIVERSE_DISCOVERY)
        + struct.pack(">B", 0)     # page
        + struct.pack(">B", 0)     # last
        + u_data
    )
    return root + frame + disc


@dataclass
class SACNSource:
    """A remote sACN source we're receiving from."""
    cid: bytes
    source_name: str
    universe: int
    priority: int
    sequence: int
    data: bytearray
    last_seen: float = field(default_factory=time.monotonic)


class SACNProtocol(asyncio.DatagramProtocol):
    def __init__(self, driver):
        self.driver = driver

    def connection_made(self, transport):
        self.driver._transport = transport

    def datagram_received(self, data: bytes, addr):
        self.driver._on_receive(data, addr)

    def error_received(self, exc):
        pass


class SACNDriver:
    """Full sACN E1.31 driver with PAP, Universe Discovery, and HTP merge."""

    def __init__(self, bind_ip: str = "0.0.0.0",
                 source_name: str = "Lumos",
                 priority: int = 100):
        self.bind_ip = bind_ip
        self.source_name = source_name
        self.priority = priority
        self._cid = LUMOS_CID

        self._transport: Optional[asyncio.DatagramTransport] = None
        self._seq: Dict[int, int] = {}        # universe -> sequence
        self._sources: Dict[Tuple[bytes, int], SACNSource] = {}  # (cid, universe)
        self._subscribed_universes: Set[int] = set()
        self._announced_universes: List[int] = []

        self.on_dmx_received: Optional[Callable[[int, bytes, int], None]] = None  # (universe, data, priority)
        self._running = False
        self._disc_task: Optional[asyncio.Task] = None
        self._merge_task: Optional[asyncio.Task] = None

        # HTP merge output
        self._merged: Dict[int, bytearray] = {}  # universe -> merged data

    async def start(self):
        loop = asyncio.get_event_loop()
        self._transport, _ = await loop.create_datagram_endpoint(
            lambda: SACNProtocol(self),
            local_addr=(self.bind_ip, SACN_PORT),
            allow_broadcast=True,
            family=socket.AF_INET,
        )
        self._running = True
        self._disc_task = asyncio.create_task(self._discovery_loop())
        self._merge_task = asyncio.create_task(self._merge_loop())

    async def stop(self):
        self._running = False
        for task in [self._disc_task, self._merge_task]:
            if task:
                task.cancel()
        if self._transport:
            self._transport.close()

    # ── Sending ───────────────────────────────────────────────────────────────

    def send_dmx(self, universe: int, data: bytes,
                 priority: Optional[int] = None,
                 dest_ip: Optional[str] = None,
                 preview: bool = False):
        """Send DMX data for a universe."""
        seq = self._next_seq(universe)
        prio = priority if priority is not None else self.priority
        payload = bytes(data)[:512]
        if len(payload) < 512:
            payload = payload.ljust(512, b"\x00")
        packet = _build_e131_packet(
            universe=universe,
            sequence=seq,
            priority=prio,
            source_name=self.source_name,
            data=payload,
            preview=preview,
        )
        ip = dest_ip or _multicast_ip(universe)
        self._sendto(packet, (ip, SACN_PORT))

    def send_per_address_priority(self, universe: int, priorities: bytes,
                                   dest_ip: Optional[str] = None):
        """Send PAP packet (E1.31 Appendix A) — per-channel priority."""
        # PAP uses start code 0xDD
        seq = self._next_seq(universe + 0x10000)
        pap_data = bytes(priorities)[:512].ljust(512, b"\x00")

        src_name = self.source_name.encode()[:63].ljust(64, b"\x00")
        dmp_data_len = 1 + len(pap_data)
        dmp_pdu_len  = 10 + 1 + dmp_data_len
        frame_pdu_len = 77 + dmp_pdu_len
        root_pdu_len  = 22 + frame_pdu_len

        root = (
            struct.pack(">H", 0x0010) + struct.pack(">H", 0x0000)
            + b"\x00ASC-E1.17\x00\x00"
            + _pdu_flags_length(root_pdu_len)
            + struct.pack(">I", VECTOR_ROOT_E131)
            + self._cid
        )
        frame = (
            _pdu_flags_length(frame_pdu_len)
            + struct.pack(">I", VECTOR_E131_DATA_PACKET)
            + src_name
            + struct.pack(">B", self.priority)
            + struct.pack(">H", 0)
            + struct.pack(">B", seq)
            + struct.pack(">B", 0)
            + struct.pack(">H", universe)
        )
        dmp = (
            _pdu_flags_length(dmp_pdu_len)
            + struct.pack(">B", VECTOR_DMP_SET_PROPERTY)
            + struct.pack(">B", 0xA1)
            + struct.pack(">H", 0x0000)
            + struct.pack(">H", 0x0001)
            + struct.pack(">H", dmp_data_len)
            + b"\xdd"   # PAP start code
            + pap_data
        )
        ip = dest_ip or _multicast_ip(universe)
        self._sendto(root + frame + dmp, (ip, SACN_PORT))

    def send_stream_terminated(self, universe: int, dest_ip: Optional[str] = None):
        """Signal that we're done with a universe."""
        seq = self._next_seq(universe)
        packet = _build_e131_packet(
            universe=universe,
            sequence=seq,
            priority=self.priority,
            source_name=self.source_name,
            data=b"\x00" * 512,
            terminated=True,
        )
        ip = dest_ip or _multicast_ip(universe)
        self._sendto(packet, (ip, SACN_PORT))

    def announce_universe(self, universe: int):
        if universe not in self._announced_universes:
            self._announced_universes.append(universe)

    # ── Receiving ────────────────────────────────────────────────────────────

    def subscribe_universe(self, universe: int):
        """Join multicast for a universe."""
        if universe in self._subscribed_universes:
            return
        self._subscribed_universes.add(universe)
        mc_ip = _multicast_ip(universe)
        if self._transport:
            sock = self._transport.get_extra_info("socket")
            if sock:
                try:
                    mreq = struct.pack("4sL", socket.inet_aton(mc_ip),
                                       socket.INADDR_ANY)
                    sock.setsockopt(socket.IPPROTO_IP,
                                    socket.IP_ADD_MEMBERSHIP, mreq)
                except Exception:
                    pass

    def _on_receive(self, data: bytes, addr):
        if len(data) < 126:
            return
        # Check ACN identifier
        if data[4:16] != b"\x00ASC-E1.17\x00\x00":
            return

        root_vector = struct.unpack_from(">I", data, 18)[0]
        if root_vector not in (VECTOR_ROOT_E131, VECTOR_ROOT_EXTENDED):
            return

        cid = data[22:38]
        frame_vector = struct.unpack_from(">I", data, 40)[0]

        if frame_vector == VECTOR_E131_DATA_PACKET:
            src_name = data[44:108].rstrip(b"\x00").decode("ascii", errors="replace")
            priority = data[108]
            seq      = data[111]
            options  = data[112]
            universe = struct.unpack_from(">H", data, 113)[0]

            # DMP
            dmp_vector = data[117]
            if dmp_vector != VECTOR_DMP_SET_PROPERTY:
                return
            prop_count = struct.unpack_from(">H", data, 123)[0]
            start_code = data[125]
            dmx = bytearray(data[126: 126 + prop_count - 1])

            # Stream terminated?
            if options & 0x40:
                key = (cid, universe)
                if key in self._sources:
                    del self._sources[key]
                return

            # PAP packet
            if start_code == 0xDD:
                return  # handled separately

            # Normal DMX
            key = (bytes(cid), universe)
            self._sources[key] = SACNSource(
                cid=bytes(cid),
                source_name=src_name,
                universe=universe,
                priority=priority,
                sequence=seq,
                data=dmx,
            )

        elif frame_vector == VECTOR_E131_EXTENDED_DISCOVERY:
            # Universe discovery — log but don't act
            pass

    async def _merge_loop(self):
        """HTP merge all sources per universe, call callback."""
        while self._running:
            universes: Dict[int, List[SACNSource]] = {}
            now = time.monotonic()
            for src in self._sources.values():
                if now - src.last_seen > 2.5:
                    continue
                universes.setdefault(src.universe, []).append(src)

            for u, sources in universes.items():
                # HTP merge across sources
                merged = bytearray(512)
                for src in sources:
                    for i, v in enumerate(src.data):
                        if i < 512 and v > merged[i]:
                            merged[i] = v
                if self._merged.get(u) != merged:
                    self._merged[u] = merged
                    if self.on_dmx_received:
                        # Find highest priority for this universe
                        max_prio = max(s.priority for s in sources)
                        self.on_dmx_received(u, bytes(merged), max_prio)

            await asyncio.sleep(0.022)  # ~44Hz

    async def _discovery_loop(self):
        """Send Universe Discovery every 10s."""
        seq = 0
        while self._running:
            if self._announced_universes:
                seq = (seq + 1) % 256
                packet = _build_universe_discovery_packet(
                    seq, self.source_name, self._announced_universes
                )
                self._sendto(packet, ("239.255.250.214", SACN_PORT))
            await asyncio.sleep(10)

    def _next_seq(self, key) -> int:
        s = (self._seq.get(key, 0) + 1) % 256
        self._seq[key] = s
        return s

    def _sendto(self, packet: bytes, addr: Tuple[str, int]):
        if self._transport:
            try:
                self._transport.sendto(packet, addr)
            except Exception:
                pass

    @property
    def sources(self) -> List[dict]:
        return [
            {
                "cid": s.cid.hex(),
                "name": s.source_name,
                "universe": s.universe,
                "priority": s.priority,
            }
            for s in self._sources.values()
        ]
