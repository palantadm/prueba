"""
OSC manager — drives the OSC router and optionally runs an OSC input server.

Requires python-osc: pip install python-osc
"""
from __future__ import annotations

import threading
from typing import Optional

from PySide6.QtCore import QObject, QTimer, Signal, Slot

from app.engine import LumosEngine
from .route import OSCRoute

try:
    from pythonosc.udp_client import SimpleUDPClient as _SimpleUDPClient
    _PYTHON_OSC_AVAILABLE = True
except ImportError:
    _PYTHON_OSC_AVAILABLE = False


class OSCManager(QObject):
    """Drives OSC output routes and manages an optional OSC input server."""

    # Emitted whenever a route fires: (route_id, output_value)
    route_fired = Signal(str, object)

    def __init__(self, engine: LumosEngine, parent: Optional[QObject] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self.routes: list[OSCRoute] = []

        # Lazily created per (host, port) UDP clients
        self._clients: dict[tuple[str, int], Optional[object]] = {}

        # OSC input server state
        self._input_server = None
        self._input_thread: Optional[threading.Thread] = None
        self._input_port: int = 9000
        self._input_mappings: list[dict] = []  # list of {address, universe, channel, in_min, in_max}

        # Drive tick at ~44 Hz (same cadence as the engine)
        self._timer = QTimer(self)
        self._timer.setInterval(23)
        self._timer.timeout.connect(self._tick)
        self._timer.start()

    # ------------------------------------------------------------------
    # Route management
    # ------------------------------------------------------------------

    def add_route(self, route: OSCRoute) -> None:
        self.routes.append(route)

    def remove_route(self, route_id: str) -> None:
        self.routes = [r for r in self.routes if r.id != route_id]

    def get_route(self, route_id: str) -> Optional[OSCRoute]:
        for r in self.routes:
            if r.id == route_id:
                return r
        return None

    # ------------------------------------------------------------------
    # UDP client pool
    # ------------------------------------------------------------------

    def _get_client(self, host: str, port: int) -> Optional[object]:
        """Lazily create or reuse a UDP client per (host, port) pair."""
        key = (host, port)
        if key not in self._clients:
            if not _PYTHON_OSC_AVAILABLE:
                self._clients[key] = None
            else:
                try:
                    self._clients[key] = _SimpleUDPClient(host, port)
                except Exception:
                    self._clients[key] = None
        return self._clients[key]

    # ------------------------------------------------------------------
    # Output tick
    # ------------------------------------------------------------------

    @Slot()
    def _tick(self) -> None:
        for route in self.routes:
            if not route.enabled:
                continue
            val = self.engine.get_channel(route.universe, route.channel)
            if not route.should_send(val):
                continue
            client = self._get_client(route.host, route.port)
            if client is None:
                continue
            out_val = route.transform(val)
            try:
                client.send_message(route.address, out_val)
                route._last_sent = val
                self.route_fired.emit(route.id, out_val)
            except Exception:
                pass

    # ------------------------------------------------------------------
    # Serialisation
    # ------------------------------------------------------------------

    def to_dict(self) -> list[dict]:
        return [r.to_dict() for r in self.routes]

    def from_dict(self, data: list[dict]) -> None:
        self.routes = [OSCRoute.from_dict(d) for d in data]

    # ------------------------------------------------------------------
    # OSC input server
    # ------------------------------------------------------------------

    def start_osc_server(self, port: int = 9000) -> bool:
        """Start an OSC input server on *port*. Received messages set DMX channels."""
        if not _PYTHON_OSC_AVAILABLE:
            return False
        self.stop_osc_server()
        self._input_port = port
        try:
            from pythonosc.dispatcher import Dispatcher
            from pythonosc.osc_server import BlockingOSCUDPServer

            dispatcher = Dispatcher()
            dispatcher.set_default_handler(self._handle_osc_input)

            server = BlockingOSCUDPServer(("0.0.0.0", port), dispatcher)
            self._input_server = server

            self._input_thread = threading.Thread(
                target=server.serve_forever,
                daemon=True,
                name="lumos-osc-input",
            )
            self._input_thread.start()
            return True
        except Exception:
            self._input_server = None
            self._input_thread = None
            return False

    def stop_osc_server(self) -> None:
        """Stop the OSC input server if running."""
        if self._input_server is not None:
            try:
                self._input_server.shutdown()
            except Exception:
                pass
            self._input_server = None
        if self._input_thread is not None:
            self._input_thread = None

    @property
    def input_server_running(self) -> bool:
        return self._input_server is not None and (
            self._input_thread is not None and self._input_thread.is_alive()
        )

    def add_input_mapping(
        self,
        osc_address: str,
        universe: int,
        channel: int,
        in_min: float = 0.0,
        in_max: float = 1.0,
    ) -> None:
        """Map an incoming OSC float/int message to a DMX channel."""
        self._input_mappings.append({
            "address": osc_address,
            "universe": universe,
            "channel": channel,
            "in_min": in_min,
            "in_max": in_max,
        })

    def remove_input_mapping(self, osc_address: str) -> None:
        self._input_mappings = [
            m for m in self._input_mappings if m["address"] != osc_address
        ]

    def _handle_osc_input(self, address: str, *args) -> None:
        """Default handler for incoming OSC messages; maps to DMX channels."""
        for mapping in self._input_mappings:
            if mapping["address"] != address:
                continue
            if not args:
                continue
            raw = args[0]
            try:
                v = float(raw)
            except (TypeError, ValueError):
                continue
            in_min = mapping["in_min"]
            in_max = mapping["in_max"]
            if in_max != in_min:
                t = (v - in_min) / (in_max - in_min)
            else:
                t = 0.0
            t = max(0.0, min(1.0, t))
            dmx_val = int(round(t * 255))
            # Must be called on the Qt main thread; use invokeMethod for safety
            from PySide6.QtCore import QMetaObject, Qt
            QMetaObject.invokeMethod(
                self,
                "_apply_input_value",
                Qt.ConnectionType.QueuedConnection,
                mapping["universe"],
                mapping["channel"],
                dmx_val,
            )

    @Slot(int, int, int)
    def _apply_input_value(self, universe: int, channel: int, value: int) -> None:
        self.engine.set_channel(universe, channel, value)

    # ------------------------------------------------------------------
    # Quick-send helper (used by the UI test section)
    # ------------------------------------------------------------------

    def quick_send(self, host: str, port: int, address: str, value: object) -> bool:
        """Send a single OSC message immediately; returns True on success."""
        client = self._get_client(host, port)
        if client is None:
            return False
        try:
            client.send_message(address, value)
            return True
        except Exception:
            return False
