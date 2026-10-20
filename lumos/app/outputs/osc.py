"""
OSC DMX backend.

Sends each non-zero DMX channel as an OSC message on every tick.
Requires python-osc: pip install python-osc

Address pattern supports {universe}, {channel} substitution.
Default: /dmx/{universe}/{channel}   → value int 0-255

Also supports sending the full 512-byte universe as a single blob:
  /dmx/universe/{universe}  → blob of 512 bytes
"""
from __future__ import annotations

from typing import Optional

from .base import OutputBackend

try:
    from pythonosc.udp_client import SimpleUDPClient as _SimpleUDPClient
    from pythonosc.osc_bundle_builder import OscBundleBuilder as _OscBundleBuilder
    from pythonosc.osc_message_builder import OscMessageBuilder as _OscMessageBuilder
    import pythonosc.osc_bundle as _osc_bundle
    _PYTHON_OSC_AVAILABLE = True
except ImportError:
    _PYTHON_OSC_AVAILABLE = False


class OSCBackend(OutputBackend):
    """Sends DMX data via OSC (UDP) using python-osc."""

    name: str = "OSC (UDP)"

    def __init__(
        self,
        host: str = "127.0.0.1",
        port: int = 8000,
        address_pattern: str = "/dmx/{universe}/{channel}",
        mode: str = "per_channel",
        only_nonzero: bool = True,
        send_rate_limit_ms: int = 0,
    ) -> None:
        self.host = host
        self.port = port
        self.address_pattern = address_pattern
        self.mode = mode  # "per_channel" | "blob" | "both"
        self.only_nonzero = only_nonzero
        self.send_rate_limit_ms = send_rate_limit_ms

        self._client: Optional[object] = None
        self._open: bool = False
        self._last_send_ms: int = 0

    # ------------------------------------------------------------------
    # OutputBackend interface
    # ------------------------------------------------------------------

    def open(self) -> bool:
        if not _PYTHON_OSC_AVAILABLE:
            return False
        try:
            self._client = _SimpleUDPClient(self.host, self.port)
            self._open = True
            return True
        except Exception:
            self._client = None
            self._open = False
            return False

    def close(self) -> None:
        self._client = None
        self._open = False

    def send(self, universe_id: int, data: bytearray) -> None:
        if not self._open or self._client is None:
            return

        # Rate limiting
        if self.send_rate_limit_ms > 0:
            import time
            now_ms = int(time.monotonic() * 1000)
            if (now_ms - self._last_send_ms) < self.send_rate_limit_ms:
                return
            self._last_send_ms = now_ms

        try:
            if self.mode in ("per_channel", "both"):
                self._send_per_channel(universe_id, data)
            if self.mode in ("blob", "both"):
                self._send_blob(universe_id, data)
        except Exception:
            pass

    def _send_per_channel(self, universe_id: int, data: bytearray) -> None:
        """Send each channel as an individual OSC message."""
        for ch_idx, value in enumerate(data[:512]):
            if self.only_nonzero and value == 0:
                continue
            channel = ch_idx + 1  # 1-indexed
            address = self.address_pattern.format(
                universe=universe_id, channel=channel
            )
            try:
                self._client.send_message(address, value)
            except Exception:
                pass

    def _send_blob(self, universe_id: int, data: bytearray) -> None:
        """Send the full 512-byte universe as a blob."""
        address = f"/dmx/universe/{universe_id}"
        blob = bytes(data[:512]).ljust(512, b"\x00")
        try:
            self._client.send_message(address, blob)
        except Exception:
            pass

    @property
    def is_open(self) -> bool:
        return self._open

    def config_dict(self) -> dict:
        return {
            "type": "osc",
            "host": self.host,
            "port": self.port,
            "address_pattern": self.address_pattern,
            "mode": self.mode,
            "only_nonzero": self.only_nonzero,
            "send_rate_limit_ms": self.send_rate_limit_ms,
        }

    @classmethod
    def from_config(cls, cfg: dict) -> "OSCBackend":
        return cls(
            host=cfg.get("host", "127.0.0.1"),
            port=int(cfg.get("port", 8000)),
            address_pattern=cfg.get("address_pattern", "/dmx/{universe}/{channel}"),
            mode=cfg.get("mode", "per_channel"),
            only_nonzero=bool(cfg.get("only_nonzero", True)),
            send_rate_limit_ms=int(cfg.get("send_rate_limit_ms", 0)),
        )
