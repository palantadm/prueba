"""
MIDIManager — listens to MIDI input ports and fires DMX/scene/cue actions.

Requires mido + python-rtmidi:
    pip install mido python-rtmidi

Falls back gracefully if not installed.
"""
from __future__ import annotations

import threading
from typing import Optional

from PySide6.QtCore import QObject, Signal, Slot, QMetaObject, Qt

from app.engine import LumosEngine
from .route import MIDIRoute

try:
    import mido
    _MIDO_AVAILABLE = True
except ImportError:
    _MIDO_AVAILABLE = False


class MIDIManager(QObject):
    """
    Listens to one or more MIDI input ports, dispatches routes to engine actions.

    Ports are opened with mido callbacks on daemon threads.
    Callbacks post to the Qt main thread via invokeMethod.
    """

    # Emitted for every matched route: (route_id, midi_value, out_value)
    route_fired = Signal(str, int, int)
    # Emitted for every raw MIDI message: (port_name, msg_type, channel, number, value)
    raw_message = Signal(str, str, int, int, int)
    # Emitted when available port list changes
    ports_changed = Signal(list)

    def __init__(self, engine: LumosEngine, parent: Optional[QObject] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self.routes: list[MIDIRoute] = []
        self._open_ports: dict[str, object] = {}   # name → mido port
        self._lock = threading.Lock()
        self._learning = False
        self._learn_callback = None   # callable(port_name, msg_type, channel, number)

    # ------------------------------------------------------------------
    # Port management
    # ------------------------------------------------------------------

    @staticmethod
    def available_ports() -> list[str]:
        if not _MIDO_AVAILABLE:
            return []
        try:
            return list(mido.get_input_names())
        except Exception:
            return []

    def open_port(self, name: str) -> bool:
        if not _MIDO_AVAILABLE:
            return False
        if name in self._open_ports:
            return True
        try:
            port = mido.open_input(
                name,
                callback=lambda msg, pn=name: self._on_midi_message(pn, msg),
            )
            with self._lock:
                self._open_ports[name] = port
            return True
        except Exception:
            return False

    def close_port(self, name: str) -> None:
        with self._lock:
            port = self._open_ports.pop(name, None)
        if port is not None:
            try:
                port.close()
            except Exception:
                pass

    def close_all(self) -> None:
        with self._lock:
            names = list(self._open_ports.keys())
        for name in names:
            self.close_port(name)

    @property
    def open_port_names(self) -> list[str]:
        with self._lock:
            return list(self._open_ports.keys())

    # ------------------------------------------------------------------
    # Route management
    # ------------------------------------------------------------------

    def add_route(self, route: MIDIRoute) -> None:
        self.routes.append(route)

    def remove_route(self, route_id: str) -> None:
        self.routes = [r for r in self.routes if r.id != route_id]

    def get_route(self, route_id: str) -> Optional[MIDIRoute]:
        return next((r for r in self.routes if r.id == route_id), None)

    # ------------------------------------------------------------------
    # Learn mode
    # ------------------------------------------------------------------

    def start_learn(self, callback) -> None:
        """Start learn mode. callback(port_name, msg_type, channel, number) fires once."""
        self._learn_callback = callback
        self._learning = True

    def stop_learn(self) -> None:
        self._learning = False
        self._learn_callback = None

    # ------------------------------------------------------------------
    # MIDI callback (called from mido background thread)
    # ------------------------------------------------------------------

    def _on_midi_message(self, port_name: str, msg) -> None:
        """Called by mido on a background thread for each incoming message."""
        msg_type = msg.type
        channel = getattr(msg, "channel", 0) + 1  # mido is 0-indexed, we use 1-indexed

        if msg_type == "control_change":
            number = msg.control
            value = msg.value
        elif msg_type in ("note_on", "note_off"):
            number = msg.note
            value = msg.velocity
        elif msg_type == "program_change":
            number = msg.program
            value = 127
        else:
            return

        # Emit raw signal (queued to main thread)
        QMetaObject.invokeMethod(
            self, "_emit_raw",
            Qt.ConnectionType.QueuedConnection,
            port_name, msg_type, channel, number, value,
        )

        # Learn mode
        if self._learning and msg_type in ("control_change", "note_on", "program_change"):
            if not (msg_type == "note_on" and value == 0):
                QMetaObject.invokeMethod(
                    self, "_fire_learn",
                    Qt.ConnectionType.QueuedConnection,
                    port_name, msg_type, channel, number,
                )
            return

        # Route dispatch
        for route in self.routes:
            if route.matches(msg_type, channel, number, port_name):
                # note_on with velocity=0 is a note_off in MIDI spec
                effective_type = msg_type
                if msg_type == "note_on" and value == 0:
                    effective_type = "note_off"

                if effective_type == "note_off" and route.note_trigger == "on_press":
                    continue
                if effective_type == "note_on" and route.note_trigger == "on_release":
                    continue

                out_val = route.map_value(value)
                route._last_value = value

                QMetaObject.invokeMethod(
                    self, "_dispatch_route",
                    Qt.ConnectionType.QueuedConnection,
                    route.id, value, out_val,
                )

    @Slot(str, str, int, int, int)
    def _emit_raw(self, port_name: str, msg_type: str, channel: int, number: int, value: int) -> None:
        self.raw_message.emit(port_name, msg_type, channel, number, value)

    @Slot(str, str, int, int)
    def _fire_learn(self, port_name: str, msg_type: str, channel: int, number: int) -> None:
        cb = self._learn_callback
        self._learning = False
        self._learn_callback = None
        if cb:
            cb(port_name, msg_type, channel, number)

    @Slot(str, int, int)
    def _dispatch_route(self, route_id: str, midi_value: int, out_val: int) -> None:
        route = self.get_route(route_id)
        if route is None:
            return

        action = route.action
        if action == "dmx_channel":
            self.engine.set_channel(route.universe, route.dmx_channel, out_val)
        elif action == "grand_master":
            self.engine.grand_master = out_val / 255.0
        elif action == "blackout_toggle":
            if midi_value > 0:
                self.engine.blackout = not self.engine.blackout
        elif action == "scene_activate":
            if midi_value > 0 and route.scene_name in self.engine.scenes:
                self.engine.activate_scene(route.scene_name)
        elif action == "cue_go":
            if midi_value > 0:
                self.engine.go(route.cuelist_id)
        elif action == "cue_back":
            if midi_value > 0:
                self.engine.back(route.cuelist_id)

        self.route_fired.emit(route_id, midi_value, out_val)

    # ------------------------------------------------------------------
    # Serialisation
    # ------------------------------------------------------------------

    def to_dict(self) -> dict:
        return {
            "open_ports": self.open_port_names,
            "routes": [r.to_dict() for r in self.routes],
        }

    def from_dict(self, data: dict) -> None:
        self.close_all()
        self.routes = [MIDIRoute.from_dict(d) for d in data.get("routes", [])]
        for name in data.get("open_ports", []):
            self.open_port(name)
