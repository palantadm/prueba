"""
Chamsys MagicDMX backend.

  ChamsysMagicDMXBackend — MagicDMX Basic (1 universe) and Full (2 universes)
  Both use the Enttec Pro framed message protocol over CDC serial.

  MagicDMX Basic:  one CDC port, 250 000 baud, Enttec Pro frame format.
  MagicDMX Full:   two CDC ports (A/B), 250 000 baud per port.
                   Specify port as "COM3-A" / "COM3-B" or "/dev/ttyACM0-A".
                   The suffix selects which of the two serial devices to open.
"""
from __future__ import annotations

from typing import Optional

from .base import OutputBackend

try:
    import serial          # type: ignore
    _SERIAL_AVAILABLE = True
except ImportError:
    serial = None          # type: ignore
    _SERIAL_AVAILABLE = False

_LABEL_DMX_TX = 0x06
_BAUDRATE      = 250_000


class ChamsysMagicDMXBackend(OutputBackend):
    """Chamsys MagicDMX Basic/Full — CDC serial, Enttec Pro framing at 250 kBaud."""

    name: str = "Chamsys MagicDMX"

    def __init__(self, port: str = "/dev/ttyACM0",
                 mode: str = "basic",
                 universe_slot: int = 0) -> None:
        """
        port          — serial port path.  For Full models append -A or -B to
                        the base name (e.g. "/dev/ttyACM0-A"); the suffix is
                        stripped and the next port index used automatically.
        mode          — "basic" or "full"
        universe_slot — 0 (first/only) or 1 (second, Full only)
        """
        self.port = port
        self.mode = mode
        self.universe_slot = universe_slot
        self._ser: Optional[object] = None

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _resolve_port(self) -> str:
        """Strip -A/-B suffix and offset port number for Full slot 1."""
        port = self.port
        slot_override: Optional[int] = None
        for suffix, slot in (("-A", 0), ("-B", 1)):
            if port.upper().endswith(suffix):
                port = port[: -len(suffix)]
                slot_override = slot
                break
        slot = slot_override if slot_override is not None else self.universe_slot
        if slot == 0 or self.mode == "basic":
            return port
        # Full mode, second universe: increment the trailing digit/number
        import re
        m = re.search(r"(\d+)$", port)
        if m:
            n = int(m.group(1))
            return port[: m.start()] + str(n + 1)
        return port

    # ------------------------------------------------------------------
    # OutputBackend interface
    # ------------------------------------------------------------------

    def open(self) -> bool:
        if not _SERIAL_AVAILABLE:
            return False
        try:
            ser = serial.Serial(
                port=self._resolve_port(),
                baudrate=_BAUDRATE,
                stopbits=serial.STOPBITS_TWO,
                bytesize=serial.EIGHTBITS,
                parity=serial.PARITY_NONE,
                timeout=0,
            )
            self._ser = ser
            return True
        except Exception:
            self._ser = None
            return False

    def close(self) -> None:
        if self._ser is not None:
            try:
                self._ser.close()       # type: ignore[attr-defined]
            except Exception:
                pass
            self._ser = None

    def send(self, universe_id: int, data: bytearray) -> None:
        if self._ser is None:
            return
        frame   = bytes(data[:512]).ljust(512, b"\x00")
        payload = b"\x00" + frame      # start code + 512 bytes
        length  = len(payload)         # 513
        packet  = (
            bytes([0x7E, _LABEL_DMX_TX, length & 0xFF, (length >> 8) & 0xFF])
            + payload
            + bytes([0xE7])
        )
        try:
            self._ser.write(packet)    # type: ignore[attr-defined]
        except Exception:
            pass

    @property
    def is_open(self) -> bool:
        return self._ser is not None

    def config_dict(self) -> dict:
        return {
            "type":          "chamsys_magicdmx",
            "port":          self.port,
            "mode":          self.mode,
            "universe_slot": self.universe_slot,
        }

    @classmethod
    def from_config(cls, cfg: dict) -> "ChamsysMagicDMXBackend":
        return cls(
            port=cfg.get("port", "/dev/ttyACM0"),
            mode=cfg.get("mode", "basic"),
            universe_slot=int(cfg.get("universe_slot", 0)),
        )
