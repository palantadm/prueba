"""DMX Universe management — pluggable output backends per universe.

Each universe can have one or more output drivers attached:
  artnet   — Art-Net 4 via ArtNetDriver
  sacn     — sACN E1.31 via SACNDriver
  kinet    — KiNET via KiNETDriver
  serial   — Enttec/USB DMX via SerialDMXManager
  (multiple outputs per universe are supported simultaneously)
"""
import asyncio
import time
from dataclasses import dataclass, field
from typing import Callable, Dict, List, Optional, Tuple


DMX_CHANNELS = 512


@dataclass
class UniverseOutput:
    """One output destination attached to a universe."""
    type: str          # artnet | sacn | kinet | serial
    # Art-Net
    artnet_ip: str = "255.255.255.255"
    artnet_net: int = 0
    artnet_subnet: int = 0
    artnet_universe: int = 0
    # sACN
    sacn_universe: int = 1
    sacn_dest_ip: Optional[str] = None  # None = multicast
    sacn_priority: int = 100
    # KiNET
    kinet_ip: str = "255.255.255.255"
    kinet_port: int = 1
    kinet_version: int = 2
    # Serial
    serial_port: str = "/dev/ttyUSB0"
    serial_mode: str = "auto"

    def to_dict(self) -> dict:
        return {k: v for k, v in self.__dict__.items() if v is not None}


@dataclass
class Universe:
    id: int
    name: str = ""
    outputs: List[UniverseOutput] = field(default_factory=list)

    _data: bytearray = field(default_factory=lambda: bytearray(DMX_CHANNELS), repr=False)
    _dirty: bool = field(default=False, repr=False)
    _last_send: float = field(default=0.0, repr=False)

    def set_channel(self, channel: int, value: int):
        if 0 <= channel < DMX_CHANNELS:
            clamped = max(0, min(255, value))
            if self._data[channel] != clamped:
                self._data[channel] = clamped
                self._dirty = True

    def set_channels(self, start: int, values):
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

    def add_output(self, out: UniverseOutput):
        self.outputs.append(out)

    def remove_output(self, idx: int):
        if 0 <= idx < len(self.outputs):
            self.outputs.pop(idx)

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "outputs": [o.to_dict() for o in self.outputs],
        }


class UniverseManager:
    """Manages all DMX universes and routes data to output drivers."""

    def __init__(self):
        self.universes: Dict[int, Universe] = {}
        self._running = False
        self._task: Optional[asyncio.Task] = None
        self._subscribers: List[Callable] = []

        # Lazily-initialised protocol drivers
        self._artnet = None
        self._sacn = None
        self._kinet = None
        self._serial_mgr = None

    # ── Driver init ───────────────────────────────────────────────────────────

    def _ensure_artnet(self):
        if self._artnet is None:
            from ..protocols.artnet import ArtNetDriver
            self._artnet = ArtNetDriver()

    def _ensure_sacn(self):
        if self._sacn is None:
            from ..protocols.sacn import SACNDriver
            self._sacn = SACNDriver()

    def _ensure_kinet(self):
        if self._kinet is None:
            from ..protocols.kinet import KiNETDriver
            self._kinet = KiNETDriver()

    def _ensure_serial(self):
        if self._serial_mgr is None:
            from ..protocols.serial_dmx import SerialDMXManager
            self._serial_mgr = SerialDMXManager()

    # ── Universe CRUD ─────────────────────────────────────────────────────────

    def add_universe(self, uid: int, name: str = "",
                     output_type: str = "none", **kwargs) -> Universe:
        u = Universe(id=uid, name=name or f"Universe {uid}")
        # Legacy single-output support
        if output_type and output_type != "none":
            out_kwargs = {k: v for k, v in kwargs.items()
                         if k.startswith(output_type.split("_")[0])}
            out = UniverseOutput(type=output_type, **out_kwargs)
            u.outputs.append(out)
        self.universes[uid] = u
        return u

    def get_universe(self, uid: int) -> Optional[Universe]:
        return self.universes.get(uid)

    def add_output(self, uid: int, out: UniverseOutput):
        u = self.universes.get(uid)
        if u:
            u.add_output(out)
            if out.type == "sacn":
                self._ensure_sacn()
                if self._sacn:
                    self._sacn.announce_universe(out.sacn_universe)

    def remove_output(self, uid: int, idx: int):
        u = self.universes.get(uid)
        if u:
            u.remove_output(idx)

    # ── Subscriptions ─────────────────────────────────────────────────────────

    def subscribe(self, callback: Callable):
        self._subscribers.append(callback)

    def unsubscribe(self, callback: Callable):
        if callback in self._subscribers:
            self._subscribers.remove(callback)

    # ── Lifecycle ─────────────────────────────────────────────────────────────

    async def start(self):
        self._running = True
        # Start network drivers that need async startup
        try:
            self._ensure_artnet()
            await self._artnet.start()
        except Exception as e:
            print(f"[UniverseManager] Art-Net start failed: {e}")
            self._artnet = None
        try:
            self._ensure_sacn()
            await self._sacn.start()
        except Exception as e:
            print(f"[UniverseManager] sACN start failed: {e}")
            self._sacn = None
        try:
            self._ensure_kinet()
            await self._kinet.start()
        except Exception as e:
            print(f"[UniverseManager] KiNET start failed: {e}")
            self._kinet = None
        self._task = asyncio.create_task(self._output_loop())

    async def stop(self):
        self._running = False
        if self._task:
            self._task.cancel()
        if self._artnet:
            await self._artnet.stop()
        if self._sacn:
            await self._sacn.stop()
        if self._kinet:
            await self._kinet.stop()
        if self._serial_mgr:
            self._serial_mgr.close_all()

    # ── Output loop ───────────────────────────────────────────────────────────

    async def _output_loop(self):
        while self._running:
            now = time.monotonic()
            for u in self.universes.values():
                should_send = u._dirty or (now - u._last_send > 1.0)
                if not should_send:
                    continue

                dmx = u.get_all()
                for out in u.outputs:
                    self._send_output(out, dmx)

                u._dirty = False
                u._last_send = now

                for cb in self._subscribers:
                    try:
                        await cb(u.id, dmx)
                    except Exception:
                        pass

            await asyncio.sleep(1 / 44)

    def _send_output(self, out: UniverseOutput, dmx: bytearray):
        try:
            if out.type == "artnet" and self._artnet:
                port_addr = (
                    (out.artnet_net & 0x7F) << 8
                    | (out.artnet_subnet & 0x0F) << 4
                    | (out.artnet_universe & 0x0F)
                )
                self._artnet.send_dmx(out.artnet_ip, port_addr, bytes(dmx))

            elif out.type == "sacn" and self._sacn:
                self._sacn.send_dmx(
                    universe=out.sacn_universe,
                    data=bytes(dmx),
                    priority=out.sacn_priority,
                    dest_ip=out.sacn_dest_ip,
                )

            elif out.type == "kinet" and self._kinet:
                if out.kinet_version == 1:
                    self._kinet.send_portout_v1(out.kinet_ip, out.kinet_port, bytes(dmx))
                else:
                    self._kinet.send_dmxout_v2(out.kinet_ip, out.kinet_port, bytes(dmx))

            elif out.type == "serial":
                self._ensure_serial()
                if self._serial_mgr:
                    if not self._serial_mgr.open_ports or out.serial_port not in self._serial_mgr.open_ports:
                        self._serial_mgr.open_device(out.serial_port, out.serial_mode)
                    self._serial_mgr.send_dmx(out.serial_port, bytes(dmx))
        except Exception as e:
            print(f"[UniverseManager] Output error ({out.type}): {e}")

    # ── Protocol driver access (for routes/API) ───────────────────────────────

    @property
    def artnet_driver(self):
        return self._artnet

    @property
    def sacn_driver(self):
        return self._sacn

    @property
    def kinet_driver(self):
        return self._kinet

    @property
    def serial_manager(self):
        return self._serial_mgr

    def to_dict(self) -> List[dict]:
        return [u.to_dict() for u in self.universes.values()]
