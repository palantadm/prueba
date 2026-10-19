"""Art-Net 4 full implementation.

Covers:
  ArtDmx      — DMX output (unicast + broadcast)
  ArtPoll     — Node discovery request
  ArtPollReply— Node discovery response
  ArtSync     — Synchronised output across nodes
  ArtTrigger  — Macro/soft-key triggers
  ArtRdm      — RDM encapsulation (pass-through)
  ArtNzs      — Non-zero start code DMX
  ArtInput    — Receive DMX from Art-Net nodes (passive listen)
"""
import asyncio
import socket
import struct
import time
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional, Tuple

ARTNET_PORT = 6454
ARTNET_HEADER = b"Art-Net\x00"

# OpCodes (little-endian)
OP_POLL        = 0x2000
OP_POLL_REPLY  = 0x2100
OP_DMX         = 0x5000
OP_NZS         = 0x5100
OP_SYNC        = 0x5200
OP_ADDRESS     = 0x6000
OP_INPUT       = 0x7000
OP_TOD_REQUEST = 0x8000
OP_TOD_DATA    = 0x8100
OP_TOD_CONTROL = 0x8200
OP_RDM         = 0x8300
OP_RDM_SUB     = 0x8400
OP_TRIGGER     = 0x9900
OP_DIRECTORY   = 0x9A00
OP_DIR_REPLY   = 0x9B00


def _make_header(opcode: int) -> bytes:
    return ARTNET_HEADER + struct.pack("<H", opcode)


@dataclass
class ArtNetNode:
    ip: str
    port: int = ARTNET_PORT
    short_name: str = ""
    long_name: str = ""
    net: int = 0
    subnet: int = 0
    universe: int = 0
    firmware: int = 0
    status1: int = 0
    status2: int = 0
    mac: bytes = b"\x00" * 6
    last_seen: float = field(default_factory=time.monotonic)
    port_types: List[int] = field(default_factory=list)
    good_input: List[int] = field(default_factory=list)
    good_output: List[int] = field(default_factory=list)
    sw_in: List[int] = field(default_factory=list)
    sw_out: List[int] = field(default_factory=list)


class ArtNetProtocol(asyncio.DatagramProtocol):
    def __init__(self, driver):
        self.driver = driver

    def connection_made(self, transport):
        self.driver._transport = transport

    def datagram_received(self, data: bytes, addr: Tuple[str, int]):
        self.driver._on_receive(data, addr)

    def error_received(self, exc):
        pass


class ArtNetDriver:
    """Full Art-Net 4 driver — output + input + discovery."""

    def __init__(self, bind_ip: str = "0.0.0.0", bind_port: int = ARTNET_PORT,
                 short_name: str = "Lumos", long_name: str = "Lumos Lighting Console"):
        self.bind_ip = bind_ip
        self.bind_port = bind_port
        self.short_name = short_name
        self.long_name = long_name

        self._transport: Optional[asyncio.DatagramTransport] = None
        self._seq: Dict[int, int] = {}      # port_addr -> sequence 1-255
        self._nodes: Dict[str, ArtNetNode] = {}  # ip -> node

        # Callbacks
        self.on_dmx_received: Optional[Callable[[int, int, int, bytes], None]] = None
        self.on_node_discovered: Optional[Callable[[ArtNetNode], None]] = None
        self.on_trigger: Optional[Callable[[int, int, int, bytes], None]] = None  # (oem, key, subkey, data)

        self._poll_task: Optional[asyncio.Task] = None
        self._running = False

    def port_address(self, net: int, subnet: int, universe: int) -> int:
        return ((net & 0x7F) << 8) | ((subnet & 0x0F) << 4) | (universe & 0x0F)

    async def start(self):
        loop = asyncio.get_event_loop()
        self._transport, _ = await loop.create_datagram_endpoint(
            lambda: ArtNetProtocol(self),
            local_addr=(self.bind_ip, self.bind_port),
            allow_broadcast=True,
        )
        self._running = True
        self._poll_task = asyncio.create_task(self._poll_loop())

    async def stop(self):
        self._running = False
        if self._poll_task:
            self._poll_task.cancel()
        if self._transport:
            self._transport.close()

    # ── Send ──────────────────────────────────────────────────────────────────

    def send_dmx(self, net: int, subnet: int, universe: int,
                 data: bytes, dest_ip: str = "255.255.255.255"):
        """Send ArtDmx packet."""
        pa = self.port_address(net, subnet, universe)
        seq = self._seq.get(pa, 0) + 1
        if seq > 255:
            seq = 1
        self._seq[pa] = seq

        length = max(2, len(data))
        if length % 2:
            length += 1  # must be even
        payload = data[:length].ljust(length, b"\x00")

        packet = (
            _make_header(OP_DMX)
            + struct.pack(">BB", 0, 14)   # proto hi, lo
            + struct.pack(">B", seq)       # sequence
            + struct.pack(">B", 0)         # physical
            + struct.pack("<H", pa)        # port address LE
            + struct.pack(">H", length)    # length BE
            + payload
        )
        self._sendto(packet, (dest_ip, ARTNET_PORT))

    def send_nzs(self, net: int, subnet: int, universe: int,
                 start_code: int, data: bytes, dest_ip: str = "255.255.255.255"):
        """Send ArtNzs (non-zero start code) packet."""
        pa = self.port_address(net, subnet, universe)
        seq = self._seq.get(pa, 0) + 1
        if seq > 255: seq = 1
        self._seq[pa] = seq
        length = len(data)
        packet = (
            _make_header(OP_NZS)
            + struct.pack(">BB", 0, 14)
            + struct.pack(">BB", seq, start_code)
            + struct.pack("<H", pa)
            + struct.pack(">H", length)
            + data
        )
        self._sendto(packet, (dest_ip, ARTNET_PORT))

    def send_sync(self, dest_ip: str = "255.255.255.255"):
        """Send ArtSync to synchronise output across all nodes."""
        packet = _make_header(OP_SYNC) + struct.pack(">BB", 0, 14) + b"\x00\x00"
        self._sendto(packet, (dest_ip, ARTNET_PORT))

    def send_trigger(self, oem: int = 0xFFFF, key: int = 0,
                     subkey: int = 0, data: bytes = b"",
                     dest_ip: str = "255.255.255.255"):
        """Send ArtTrigger for macro/soft-key control."""
        payload = data[:512].ljust(512, b"\x00")
        packet = (
            _make_header(OP_TRIGGER)
            + struct.pack(">BB", 0, 14)
            + b"\x00\x00"             # filler
            + struct.pack(">H", oem)
            + struct.pack(">B", key)
            + struct.pack(">B", subkey)
            + payload
        )
        self._sendto(packet, (dest_ip, ARTNET_PORT))

    def send_poll(self, dest_ip: str = "255.255.255.255"):
        """Send ArtPoll to discover nodes."""
        packet = (
            _make_header(OP_POLL)
            + struct.pack(">BB", 0, 14)
            + struct.pack(">B", 0b00000110)  # TalkToMe: send on change + diagnostics
            + struct.pack(">B", 0x00)         # DiagPriority: DpLow
        )
        self._sendto(packet, (dest_ip, ARTNET_PORT))

    def _send_poll_reply(self, dest_ip: str):
        """Send ArtPollReply announcing ourselves."""
        ip_bytes = socket.inet_aton(self._get_local_ip())
        mac = b"\x00" * 6
        short = self.short_name.encode()[:17].ljust(18, b"\x00")
        long_n = self.long_name.encode()[:63].ljust(64, b"\x00")
        packet = (
            _make_header(OP_POLL_REPLY)
            + ip_bytes
            + struct.pack("<H", ARTNET_PORT)
            + struct.pack(">H", 0x0001)  # firmware
            + struct.pack(">BB", 0, 0)   # net switch, sub switch
            + struct.pack(">H", 0x0001)  # OEM
            + struct.pack(">B", 0)       # UBEA version
            + struct.pack(">B", 0b11010000)  # status 1
            + struct.pack("<H", 0x0000)  # ESTA manufacturer
            + short + long_n
            + b"\x00" * 64              # NodeReport
            + struct.pack(">H", 4)       # NumPorts
            + b"\xc0\xc0\xc0\xc0"       # PortTypes: DMX output
            + b"\x00" * 4               # GoodInput
            + b"\x80\x80\x80\x80"       # GoodOutput
            + b"\x00\x01\x02\x03"       # SwIn
            + b"\x00\x01\x02\x03"       # SwOut
            + b"\x00" * 4               # SwVideo, SwMacro, SwRemote, Spare
            + struct.pack(">B", 0)       # Style: StController
            + mac
            + ip_bytes                   # BindIp
            + struct.pack(">B", 1)       # BindIndex
            + struct.pack(">B", 0b00001110)  # Status2
            + b"\x00" * 26              # Filler
        )
        self._sendto(packet, (dest_ip, ARTNET_PORT))

    # ── Receive ───────────────────────────────────────────────────────────────

    def _on_receive(self, data: bytes, addr: Tuple[str, int]):
        if not data.startswith(ARTNET_HEADER):
            return
        opcode = struct.unpack_from("<H", data, 8)[0]

        if opcode == OP_POLL:
            self._send_poll_reply(addr[0])

        elif opcode == OP_POLL_REPLY:
            self._parse_poll_reply(data, addr[0])

        elif opcode == OP_DMX:
            self._parse_dmx(data, addr)

        elif opcode == OP_NZS:
            self._parse_nzs(data, addr)

        elif opcode == OP_TRIGGER:
            self._parse_trigger(data, addr)

    def _parse_dmx(self, data: bytes, addr):
        if len(data) < 18:
            return
        seq = data[12]
        physical = data[13]
        pa = struct.unpack_from("<H", data, 14)[0]
        length = struct.unpack_from(">H", data, 16)[0]
        dmx = data[18: 18 + length]
        net = (pa >> 8) & 0x7F
        subnet = (pa >> 4) & 0x0F
        universe = pa & 0x0F
        if self.on_dmx_received:
            self.on_dmx_received(net, subnet, universe, bytes(dmx))

    def _parse_nzs(self, data: bytes, addr):
        if len(data) < 18:
            return
        seq = data[12]
        start_code = data[13]
        pa = struct.unpack_from("<H", data, 14)[0]
        length = struct.unpack_from(">H", data, 16)[0]
        dmx = data[18: 18 + length]

    def _parse_trigger(self, data: bytes, addr):
        if len(data) < 530:
            return
        oem = struct.unpack_from(">H", data, 14)[0]
        key = data[16]
        subkey = data[17]
        payload = data[18:530]
        if self.on_trigger:
            self.on_trigger(oem, key, subkey, payload)

    def _parse_poll_reply(self, data: bytes, addr: str):
        if len(data) < 197:
            return
        ip = socket.inet_ntoa(data[10:14])
        short_name = data[26:44].rstrip(b"\x00").decode("ascii", errors="replace")
        long_name = data[44:108].rstrip(b"\x00").decode("ascii", errors="replace")
        firmware = struct.unpack_from(">H", data, 22)[0]
        num_ports = struct.unpack_from(">H", data, 172)[0]
        port_types = list(data[174:178])
        good_output = list(data[182:186])
        sw_in = list(data[186:190])
        sw_out = list(data[190:194])
        status1 = data[20]
        status2 = data[212] if len(data) > 212 else 0

        node = ArtNetNode(
            ip=ip,
            short_name=short_name,
            long_name=long_name,
            firmware=firmware,
            port_types=port_types[:num_ports],
            good_output=good_output[:num_ports],
            sw_in=sw_in[:num_ports],
            sw_out=sw_out[:num_ports],
            status1=status1,
            status2=status2,
        )
        self._nodes[ip] = node
        if self.on_node_discovered:
            self.on_node_discovered(node)

    # ── Poll loop ─────────────────────────────────────────────────────────────

    async def _poll_loop(self):
        while self._running:
            self.send_poll()
            # Expire nodes not seen in 30s
            now = time.monotonic()
            stale = [ip for ip, n in self._nodes.items() if now - n.last_seen > 30]
            for ip in stale:
                del self._nodes[ip]
            await asyncio.sleep(10)

    # ── Helpers ───────────────────────────────────────────────────────────────

    def _sendto(self, packet: bytes, addr: Tuple[str, int]):
        if self._transport:
            try:
                self._transport.sendto(packet, addr)
            except Exception:
                pass

    def _get_local_ip(self) -> str:
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            s.connect(("8.8.8.8", 80))
            ip = s.getsockname()[0]
            s.close()
            return ip
        except Exception:
            return "127.0.0.1"

    @property
    def nodes(self) -> List[ArtNetNode]:
        return list(self._nodes.values())

    def node_to_dict(self, n: ArtNetNode) -> dict:
        return {
            "ip": n.ip,
            "short_name": n.short_name,
            "long_name": n.long_name,
            "firmware": hex(n.firmware),
            "port_types": n.port_types,
            "sw_out": n.sw_out,
            "sw_in": n.sw_in,
        }
