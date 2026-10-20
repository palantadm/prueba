"""
EffectsManager — drives all active effects on a QTimer and writes
values into the LumosEngine.
"""
from __future__ import annotations

from PySide6.QtCore import QObject, QTimer, Slot

from app.effects.base import Effect
from app.engine import LumosEngine


class EffectsManager(QObject):
    """
    Owns a collection of :class:`Effect` instances and ticks them at ~44 Hz,
    writing the resulting channel values into the engine.
    """

    TICK_MS = 23  # ~44 Hz, matches engine rate

    def __init__(self, engine: LumosEngine, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self._effects: list[Effect] = []

        self._timer = QTimer(self)
        self._timer.setInterval(self.TICK_MS)
        self._timer.timeout.connect(self._tick)
        self._timer.start()

    # ------------------------------------------------------------------
    # Effect management
    # ------------------------------------------------------------------

    def add_effect(self, effect: Effect) -> None:
        if effect not in self._effects:
            self._effects.append(effect)

    def remove_effect(self, effect: Effect) -> None:
        try:
            self._effects.remove(effect)
        except ValueError:
            pass

    def effects(self) -> list[Effect]:
        return list(self._effects)

    # ------------------------------------------------------------------
    # Tick
    # ------------------------------------------------------------------

    @Slot()
    def _tick(self) -> None:
        for fx in self._effects:
            if fx.enabled:
                fx.tick(float(self.TICK_MS))
                vals = fx.get_values()
                for (univ, ch), v in vals.items():
                    self.engine.set_channel(univ, ch, v)

    # ------------------------------------------------------------------
    # Persistence
    # ------------------------------------------------------------------

    def config_dict(self) -> list[dict]:
        """Serialise all effects to a list of dicts for show save/load."""
        return [fx.to_dict() for fx in self._effects]
