"""KiNET protocol — Philips Color Kinetics lighting.

Supports:
  KiNET v1 (Port Out)   — simple DMX-like output
  KiNET v2 (DMXOUT)     — full DMX512 output
  Discovery via KiNET Sync packets
"""
import asyncio
import socket
import struct
from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple

KINET_PORT = 6038

# KiNET magic header
KINET_MAGIC = 0x0401DC4A

# Packet types
KINET_TYPE_PORTOUT   = 0x0108  # v1
KINET_TYPE_DMXOUT    = 0x0101  # v2
KINET_TYPE_SYNC      = 0x0100
KINET_TYPE_DISCOVERY = 0x0200


@dataclass
class KiNETDevice:
    ip: str
    mac: bytes
    device_type: int
    num_ports: int


class KiNETProtocol(asyncio.DatagramProtocol):
    def __init__(self, driver):
        self.driver = driver

    def connection_made(self, transport):
        self.driver._transport = transport

    def datagram_received(self, data, addr):
        self.driver._on_receive(data, addr)

    def error_received(self, exc):
        pass


class KiNETDriver:
    """KiNET v1+v2 output driver for Philips Color Kinetics."""

    def __init__(self, bind_ip: str = "0.0.0.0"):
        self.bind_ip = bind_ip
        self._transport: Optional[asyncio.DatagramTransport] = None
        self._seq = 0
        self._devices: Dict[str, KiNETDevice] = {}
        self._running = False

    async def start(self):
        loop = asyncio.get_event_loop()
        self._transport, _ = await loop.create_datagram_endpoint(
            lambda: KiNETProtocol(self),
            local_addr=(self.bind_ip, KINET_PORT),
            allow_broadcast=True,
        )
        self._running = True

    async def stop(self):
        self._running = False
        if self._transport:
            self._transport.close()

    def _next_seq(self) -> int:
        self._seq = (self._seq + 1) & 0xFFFFFFFF
        return self._seq

    def send_portout_v1(self, dest_ip: str, port: int, data: bytes):
        """KiNET v1 Port Out — up to 100 bytes per port."""
        payload = data[:100].ljust(100, b"\x00")
        seq = self._next_seq()
        packet = (
            struct.pack(">I", KINET_MAGIC)
            + struct.pack(">H", 1)            # version
            + struct.pack(">H", KINET_TYPE_PORTOUT)
            + struct.pack(">I", seq)
            + struct.pack(">B", 0)            # flags
            + struct.pack(">B", port & 0xFF)  # port
            + struct.pack(">H", len(payload))
            + payload
        )
        self._sendto(packet, (dest_ip, KINET_PORT))

    def send_dmxout_v2(self, dest_ip: str, port: int, data: bytes,
                       universe: int = 0):
        """KiNET v2 DMXOUT — full 512-byte DMX output."""
        payload = data[:512].ljust(512, b"\x00")
        seq = self._next_seq()
        packet = (
            struct.pack(">I", KINET_MAGIC)
            + struct.pack(">H", 2)            # version
            + struct.pack(">H", KINET_TYPE_DMXOUT)
            + struct.pack(">I", seq)
            + struct.pack(">H", 0)            # flags
            + struct.pack(">H", universe & 0x01FF)
            + struct.pack(">H", port & 0xFFFF)
            + struct.pack(">H", len(payload))
            + payload
        )
        self._sendto(packet, (dest_ip, KINET_PORT))

    def send_sync(self, dest_ip: str = "255.255.255.255"):
        """Send KiNET sync to all devices."""
        seq = self._next_seq()
        packet = (
            struct.pack(">I", KINET_MAGIC)
            + struct.pack(">H", 1)
            + struct.pack(">H", KINET_TYPE_SYNC)
            + struct.pack(">I", seq)
            + b"\x00" * 4
        )
        self._sendto(packet, (dest_ip, KINET_PORT))

    def _on_receive(self, data: bytes, addr: Tuple[str, int]):
        if len(data) < 8:
            return
        magic = struct.unpack_from(">I", data, 0)[0]
        if magic != KINET_MAGIC:
            return
        pkt_type = struct.unpack_from(">H", data, 6)[0]
        if pkt_type == KINET_TYPE_DISCOVERY:
            ip = addr[0]
            mac = data[12:18] if len(data) > 18 else b"\x00" * 6
            dev_type = struct.unpack_from(">H", data, 18)[0] if len(data) > 20 else 0
            self._devices[ip] = KiNETDevice(ip=ip, mac=mac, device_type=dev_type, num_ports=1)

    def _sendto(self, packet: bytes, addr: Tuple[str, int]):
        if self._transport:
            try:
                self._transport.sendto(packet, addr)
            except Exception:
                pass

    @property
    def devices(self) -> List[dict]:
        return [
            {"ip": d.ip, "mac": d.mac.hex(":"), "type": d.device_type}
            for d in self._devices.values()
        ]
