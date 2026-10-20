"""
OutputManager — maps universes to output backends and routes DMX frames.
"""
from __future__ import annotations

from typing import Optional, TYPE_CHECKING

from .base import OutputBackend

if TYPE_CHECKING:
    from app.engine import LumosEngine


class OutputManager:
    """
    Manages one :class:`OutputBackend` per universe.

    Usage::

        manager = OutputManager(engine)
        backend = ArtNetBackend(ip="192.168.1.100")
        manager.set_backend(1, backend)
        manager.open_all()

        # Connect to engine signal so frames are forwarded automatically
        engine.dmx_updated.connect(manager._on_dmx_updated)
    """

    def __init__(self, engine: "LumosEngine") -> None:
        self._engine = engine
        self._backends: dict[int, OutputBackend] = {}

    # ------------------------------------------------------------------
    # Backend registry
    # ------------------------------------------------------------------

    def set_backend(self, universe_id: int, backend: OutputBackend) -> None:
        """Assign *backend* to *universe_id*.  Closes any previous backend."""
        old = self._backends.get(universe_id)
        if old is not None and old.is_open:
            old.close()
        self._backends[universe_id] = backend

    def remove_backend(self, universe_id: int) -> None:
        """Remove and close the backend for *universe_id* (if any)."""
        backend = self._backends.pop(universe_id, None)
        if backend is not None and backend.is_open:
            backend.close()

    def get_backend(self, universe_id: int) -> Optional[OutputBackend]:
        """Return the backend for *universe_id*, or None."""
        return self._backends.get(universe_id)

    # ------------------------------------------------------------------
    # Bulk open / close
    # ------------------------------------------------------------------

    def open_all(self) -> dict[int, bool]:
        """Open every registered backend.  Returns {universe_id: success}."""
        results: dict[int, bool] = {}
        for uid, backend in self._backends.items():
            if not backend.is_open:
                results[uid] = backend.open()
            else:
                results[uid] = True
        return results

    def close_all(self) -> None:
        """Close every open backend."""
        for backend in self._backends.values():
            if backend.is_open:
                backend.close()

    # ------------------------------------------------------------------
    # Engine signal handler
    # ------------------------------------------------------------------

    def _on_dmx_updated(self, universe_id: int, data: bytes) -> None:
        """
        Slot compatible with LumosEngine.dmx_updated(int, bytes).

        Connect via::

            engine.dmx_updated.connect(manager._on_dmx_updated)
        """
        backend = self._backends.get(universe_id)
        if backend is not None and backend.is_open:
            backend.send(universe_id, bytearray(data))

    # ------------------------------------------------------------------
    # Serialisation
    # ------------------------------------------------------------------

    def config_dict(self) -> dict:
        """Return a JSON-serialisable representation of all backends."""
        return {
            str(uid): backend.config_dict()
            for uid, backend in self._backends.items()
        }
