"""
Generic FTDI-based Open DMX backends.

  FTDIRawBackend    — any FT232R dongle (raw serial break, identical to Enttec Open)
  DMXkingBackend    — DMXking ultraDMX Micro (also FTDI serial break)

Compatible with virtually all cheap USB-DMX dongles that expose an FT232R:
  DM-X, YN-DMX512, Eurolite USB-DMX512 PRO adapter, DMXIS, etc.
"""
from __future__ import annotations

from .enttec import EnttecOpenBackend


class FTDIRawBackend(EnttecOpenBackend):
    """
    Generic FTDI FT232R open DMX dongle.

    Protocol is identical to the Enttec Open DMX USB: raw serial DMX with a
    software-generated break at 250 000 baud.  This class is a named alias
    so the UI can present it under a recognisable label.
    """

    name: str = "FTDI Raw (Open DMX)"

    def config_dict(self) -> dict:
        return {
            "type":     "ftdi_raw",
            "port":     self.port,
            "baudrate": self.baudrate,
        }

    @classmethod
    def from_config(cls, cfg: dict) -> "FTDIRawBackend":
        return cls(
            port=cfg.get("port", "/dev/ttyUSB0"),
            baudrate=int(cfg.get("baudrate", 250_000)),
        )


class DMXkingBackend(EnttecOpenBackend):
    """
    DMXking ultraDMX Micro / uDMX (serial variant).

    Uses the same raw serial break protocol as the Enttec Open DMX USB.
    The DMXking devices expose an FT232R and accept standard DMX framing.
    """

    name: str = "DMXking ultraDMX"

    def config_dict(self) -> dict:
        return {
            "type":     "dmxking",
            "port":     self.port,
            "baudrate": self.baudrate,
        }

    @classmethod
    def from_config(cls, cfg: dict) -> "DMXkingBackend":
        return cls(
            port=cfg.get("port", "/dev/ttyUSB0"),
            baudrate=int(cfg.get("baudrate", 250_000)),
        )
