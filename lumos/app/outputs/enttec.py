"""
Enttec DMX USB backends.

  EnttecOpenBackend — Enttec Open DMX USB (raw serial, break signalling)
  EnttecProBackend  — Enttec DMX USB Pro (framed message protocol)
"""
from __future__ import annotations

import time
from typing import Optional

from .base import OutputBackend

try:
    import serial  # type: ignore
    _SERIAL_AVAILABLE = True
except ImportError:
    serial = None  # type: ignore
    _SERIAL_AVAILABLE = False


# ---------------------------------------------------------------------------
# Enttec Open DMX USB
# ---------------------------------------------------------------------------

class EnttecOpenBackend(OutputBackend):
    """
    Enttec Open DMX USB — raw serial DMX with break signalling.

    Requires pyserial.  The Open DMX relies on the host CPU to generate
    the DMX break, so timing accuracy depends on the OS scheduler.
    """

    name: str = "Enttec Open DMX"

    def __init__(self, port: str = "/dev/ttyUSB0", baudrate: int = 250000) -> None:
        self.port = port
        self.baudrate = baudrate
        self._ser: Optional[object] = None  # serial.Serial

    def open(self) -> bool:
        if not _SERIAL_AVAILABLE:
            return False
        try:
            ser = serial.Serial(
                port=self.port,
                baudrate=self.baudrate,
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
                self._ser.close()  # type: ignore[attr-defined]
            except Exception:
                pass
            self._ser = None

    def send(self, universe_id: int, data: bytearray) -> None:
        if self._ser is None:
            return
        frame = bytes(data[:512]).ljust(512, b"\x00")
        try:
            ser = self._ser  # type: ignore[attr-defined]
            # Generate DMX break (>88 µs low, mark-after-break ≥8 µs)
            if hasattr(ser, "sendBreak"):
                ser.sendBreak(duration=0.0001)  # 100 µs
            else:
                ser.break_condition = True
                time.sleep(0.0001)   # 100 µs break
                ser.break_condition = False
                time.sleep(0.000012)  # 12 µs mark-after-break
            # Start code (0x00) + 512 channel bytes
            ser.write(b"\x00" + frame)
        except Exception:
            pass

    @property
    def is_open(self) -> bool:
        return self._ser is not None

    def config_dict(self) -> dict:
        return {
            "type": "enttec_open",
            "port": self.port,
            "baudrate": self.baudrate,
        }

    @classmethod
    def from_config(cls, cfg: dict) -> "EnttecOpenBackend":
        return cls(
            port=cfg.get("port", "/dev/ttyUSB0"),
            baudrate=int(cfg.get("baudrate", 250000)),
        )


# ---------------------------------------------------------------------------
# Enttec DMX USB Pro
# ---------------------------------------------------------------------------

class EnttecProBackend(OutputBackend):
    """
    Enttec DMX USB Pro — proprietary framed message protocol over serial.

    The Pro handles the DMX break and timing internally; the host just sends
    a framed message at standard serial baud rates.
    """

    name: str = "Enttec Pro"

    # Message label for "Send DMX Packet Request"
    _LABEL_DMX_TX = 0x06

    def __init__(self, port: str = "/dev/ttyUSB0", baudrate: int = 57600) -> None:
        self.port = port
        self.baudrate = baudrate
        self._ser: Optional[object] = None  # serial.Serial

    def open(self) -> bool:
        if not _SERIAL_AVAILABLE:
            return False
        try:
            ser = serial.Serial(
                port=self.port,
                baudrate=self.baudrate,
                stopbits=serial.STOPBITS_ONE,
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
                self._ser.close()  # type: ignore[attr-defined]
            except Exception:
                pass
            self._ser = None

    def send(self, universe_id: int, data: bytearray) -> None:
        if self._ser is None:
            return
        frame = bytes(data[:512]).ljust(512, b"\x00")
        # Payload = start code (0x00) + 512 data bytes = 513 bytes
        payload = b"\x00" + frame
        length = len(payload)  # 513
        length_lsb = length & 0xFF
        length_msb = (length >> 8) & 0xFF
        packet = (
            bytes([0x7E, self._LABEL_DMX_TX, length_lsb, length_msb])
            + payload
            + bytes([0xE7])
        )
        try:
            self._ser.write(packet)  # type: ignore[attr-defined]
        except Exception:
            pass

    @property
    def is_open(self) -> bool:
        return self._ser is not None

    def config_dict(self) -> dict:
        return {
            "type": "enttec_pro",
            "port": self.port,
            "baudrate": self.baudrate,
        }

    @classmethod
    def from_config(cls, cfg: dict) -> "EnttecProBackend":
        return cls(
            port=cfg.get("port", "/dev/ttyUSB0"),
            baudrate=int(cfg.get("baudrate", 57600)),
        )
