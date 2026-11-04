"""
uDMX backend — Anyma/Wirbel USB HID DMX interface.

These cheap USB dongles (VID 0x16C0, PID 0x05DC) use libusb control transfers
instead of a serial port.  Requires pyusb:  pip install pyusb

Compatible hardware:
  - Anyma uDMX
  - Wirbel uDMX clones
  - Most cheap eBay/AliExpress "USB DMX" HID sticks
"""
from __future__ import annotations

from typing import Optional

from .base import OutputBackend

try:
    import usb.core    # type: ignore
    import usb.util    # type: ignore
    _USB_AVAILABLE = True
except ImportError:
    usb = None         # type: ignore
    _USB_AVAILABLE = False

_VENDOR_ID  = 0x16C0
_PRODUCT_ID = 0x05DC

_CMD_SET_SINGLE_CHANNEL  = 0x01
_CMD_SET_CHANNEL_RANGE   = 0x02

_REQUEST_TYPE = 0x40   # bmRequestType: vendor, device, host→device


class UDMXBackend(OutputBackend):
    """uDMX (Anyma/Wirbel) — USB HID via libusb control transfers."""

    name: str = "uDMX (USB HID)"

    def __init__(self, universe_offset: int = 0) -> None:
        """
        universe_offset: added to wIndex in the control transfer.
        Always 0 for single-universe devices.
        """
        self.universe_offset = universe_offset
        self._dev: Optional[object] = None  # usb.core.Device

    def open(self) -> bool:
        if not _USB_AVAILABLE:
            return False
        try:
            dev = usb.core.find(idVendor=_VENDOR_ID, idProduct=_PRODUCT_ID)
            if dev is None:
                return False
            if dev.is_kernel_driver_active(0):
                dev.detach_kernel_driver(0)
            dev.set_configuration()
            self._dev = dev
            return True
        except Exception:
            self._dev = None
            return False

    def close(self) -> None:
        if self._dev is not None:
            try:
                usb.util.dispose_resources(self._dev)   # type: ignore[attr-defined]
            except Exception:
                pass
            self._dev = None

    def send(self, universe_id: int, data: bytearray) -> None:
        if self._dev is None:
            return
        frame = bytes(data[:512]).ljust(512, b"\x00")
        try:
            self._dev.ctrl_transfer(   # type: ignore[attr-defined]
                _REQUEST_TYPE,
                _CMD_SET_CHANNEL_RANGE,
                wValue=len(frame),                           # channel count
                wIndex=self.universe_offset,                 # start channel (0-based)
                data_or_wLength=frame,
            )
        except Exception:
            pass

    @property
    def is_open(self) -> bool:
        return self._dev is not None

    def config_dict(self) -> dict:
        return {"type": "udmx", "universe_offset": self.universe_offset}

    @classmethod
    def from_config(cls, cfg: dict) -> "UDMXBackend":
        return cls(universe_offset=int(cfg.get("universe_offset", 0)))
