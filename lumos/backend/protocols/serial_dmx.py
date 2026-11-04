"""USB/Serial DMX output.

Supports:
  Enttec Open DMX USB  — FTDI-based, raw DMX break
  Enttec DMX USB Pro   — Firmware-based, message protocol
  Generic FTDI/USB-RS485 adapters
  Art-Net USB Widgets  — Enttec protocol compatible

Auto-detects Enttec Pro vs Open by probing firmware version.
"""
import asyncio
import struct
import threading
import time
from typing import Callable, Dict, List, Optional

try:
    import serial
    import serial.tools.list_ports
    SERIAL_AVAILABLE = True
except ImportError:
    SERIAL_AVAILABLE = False


# Enttec USB Pro API labels
ENTTEC_PRO_START_MSG   = 0x7E
ENTTEC_PRO_END_MSG     = 0xE7
LABEL_GET_PARAMS       = 3
LABEL_SET_PARAMS       = 4
LABEL_RECEIVE_DMX      = 5
LABEL_SEND_DMX         = 6
LABEL_SEND_RDM         = 7
LABEL_RECEIVE_RDM      = 11
LABEL_GET_WIDGET_INFO  = 77
LABEL_GET_WIDGET_SN    = 10


def enttec_pro_message(label: int, data: bytes = b"") -> bytes:
    """Wrap data in Enttec Pro message format."""
    length = len(data)
    return (
        bytes([ENTTEC_PRO_START_MSG, label])
        + struct.pack("<H", length)
        + data
        + bytes([ENTTEC_PRO_END_MSG])
    )


def enttec_pro_dmx(data: bytes) -> bytes:
    """Build Enttec Pro DMX output message."""
    payload = b"\x00" + data[:512]  # start code 0x00 + DMX
    return enttec_pro_message(LABEL_SEND_DMX, payload)


class SerialDMXDevice:
    """Represents a detected serial DMX device."""

    def __init__(self, port: str, mode: str = "auto"):
        """
        port: serial port path (e.g. /dev/ttyUSB0, COM3)
        mode: 'enttec_pro', 'enttec_open', 'auto'
        """
        self.port = port
        self.mode = mode
        self._ser = None
        self._detected_mode = None
        self._last_dmx: Optional[bytes] = None
        self._thread: Optional[threading.Thread] = None
        self._running = False

        self.on_dmx_received: Optional[Callable[[bytes], None]] = None

    def open(self) -> bool:
        if not SERIAL_AVAILABLE:
            return False
        try:
            self._ser = serial.Serial(
                port=self.port,
                baudrate=250000,
                bytesize=serial.EIGHTBITS,
                parity=serial.PARITY_NONE,
                stopbits=serial.STOPBITS_TWO,
                timeout=0.1,
            )
            if self.mode == "auto":
                self._detected_mode = self._probe_mode()
            else:
                self._detected_mode = self.mode

            if self._detected_mode == "enttec_open":
                # Open DMX needs specific baud
                self._ser.baudrate = 250000

            self._running = True
            self._thread = threading.Thread(target=self._receive_loop, daemon=True)
            self._thread.start()
            return True
        except Exception:
            return False

    def close(self):
        self._running = False
        if self._ser:
            try:
                self._ser.close()
            except Exception:
                pass

    def _probe_mode(self) -> str:
        """Try to identify Enttec Pro by querying firmware version."""
        try:
            self._ser.write(enttec_pro_message(LABEL_GET_WIDGET_INFO))
            time.sleep(0.1)
            resp = self._ser.read(32)
            if resp and resp[0] == ENTTEC_PRO_START_MSG and resp[1] == LABEL_GET_WIDGET_INFO:
                return "enttec_pro"
        except Exception:
            pass
        return "enttec_open"

    def send_dmx(self, data: bytes):
        if not self._ser or not self._ser.is_open:
            return
        self._last_dmx = data
        if self._detected_mode == "enttec_pro":
            self._send_enttec_pro(data)
        else:
            self._send_open_dmx(data)

    def _send_enttec_pro(self, data: bytes):
        try:
            self._ser.write(enttec_pro_dmx(data))
        except Exception:
            pass

    def _send_open_dmx(self, data: bytes):
        """Send raw DMX break + MAB + data for Open DMX."""
        try:
            # Break: set BREAK condition
            self._ser.sendBreak(duration=0.001)  # 1ms break
            # MAB: mark after break (auto handled by hardware)
            time.sleep(0.000012)  # 12us MAB
            # Start code + data
            payload = b"\x00" + data[:512]
            self._ser.write(payload)
        except Exception:
            pass

    def _receive_loop(self):
        """Read incoming DMX (for widgets that support input)."""
        if not self._ser:
            return
        buf = b""
        while self._running:
            if self._detected_mode == "enttec_pro":
                chunk = self._ser.read(64)
                if chunk:
                    buf += chunk
                    # Parse Enttec Pro messages
                    while len(buf) >= 5:
                        if buf[0] != ENTTEC_PRO_START_MSG:
                            buf = buf[1:]
                            continue
                        label = buf[1]
                        length = struct.unpack_from("<H", buf, 2)[0]
                        if len(buf) < 5 + length:
                            break
                        msg_data = buf[4: 4 + length]
                        end = buf[4 + length]
                        buf = buf[5 + length:]
                        if end == ENTTEC_PRO_END_MSG and label == LABEL_RECEIVE_DMX:
                            if self.on_dmx_received and len(msg_data) > 1:
                                self.on_dmx_received(msg_data[1:])  # skip status byte
            else:
                time.sleep(0.02)

    @property
    def is_open(self) -> bool:
        return bool(self._ser and self._ser.is_open)

    @property
    def detected_mode(self) -> Optional[str]:
        return self._detected_mode


def list_serial_ports() -> List[dict]:
    """List available serial ports and identify likely DMX devices."""
    if not SERIAL_AVAILABLE:
        return []
    ports = []
    for p in serial.tools.list_ports.comports():
        likely_dmx = any(kw in (p.description or "").lower()
                         for kw in ["enttec", "ftdi", "usb serial", "dmx", "rs485"])
        ports.append({
            "port": p.device,
            "description": p.description or "",
            "manufacturer": p.manufacturer or "",
            "vid": f"{p.vid:04X}" if p.vid else None,
            "pid": f"{p.pid:04X}" if p.pid else None,
            "likely_dmx": likely_dmx,
        })
    return ports


class SerialDMXManager:
    """Manages multiple serial DMX devices."""

    def __init__(self):
        self._devices: Dict[str, SerialDMXDevice] = {}

    def open_device(self, port: str, mode: str = "auto") -> bool:
        if port in self._devices and self._devices[port].is_open:
            return True
        dev = SerialDMXDevice(port, mode)
        if dev.open():
            self._devices[port] = dev
            return True
        return False

    def close_device(self, port: str):
        if port in self._devices:
            self._devices[port].close()
            del self._devices[port]

    def send_dmx(self, port: str, data: bytes):
        dev = self._devices.get(port)
        if dev:
            dev.send_dmx(data)

    def close_all(self):
        for dev in self._devices.values():
            dev.close()
        self._devices.clear()

    @property
    def open_ports(self) -> List[str]:
        return [p for p, d in self._devices.items() if d.is_open]
