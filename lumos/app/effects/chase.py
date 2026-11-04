"""
Chase effect — steps through a list of color/value states.
"""
from __future__ import annotations

from app.effects.base import Effect, EffectTarget


class ChaseEffect(Effect):
    """
    Steps through a list of channel-value states, one step per interval.

    ``steps`` is a list of lists — each inner list is channel values
    (e.g. [R, G, B]) applied to every target starting at start_channel.
    ``step_time_ms`` controls how long each step lasts.
    """

    def __init__(self) -> None:
        super().__init__()
        self.name = "Chase"
        self.steps: list[list[int]] = [
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
        ]
        self.step_time_ms: float = 500.0
        self._current_step: int = 0
        self._step_elapsed_ms: float = 0.0

    def _apply(self) -> None:
        self._step_elapsed_ms += 23.0  # dt is fixed in manager, but we track here
        # advance step when timer expires
        while self._step_elapsed_ms >= self.step_time_ms:
            self._step_elapsed_ms -= self.step_time_ms
            if self.steps:
                self._current_step = (self._current_step + 1) % len(self.steps)

    def tick(self, dt_ms: float) -> None:
        """Override to use dt_ms properly instead of fixed 23."""
        if self.enabled:
            self._elapsed_ms += dt_ms
            self._step_elapsed_ms += dt_ms
            while self._step_elapsed_ms >= self.step_time_ms:
                self._step_elapsed_ms -= self.step_time_ms
                if self.steps:
                    self._current_step = (self._current_step + 1) % len(self.steps)

    def get_values(self) -> dict[tuple[int, int], int]:
        result: dict[tuple[int, int], int] = {}
        if not self.steps or not self.targets:
            return result
        step_vals = self.steps[self._current_step % len(self.steps)]
        for target in self.targets:
            for i, val in enumerate(step_vals):
                ch = target.start_channel + i
                if ch <= 512:
                    result[(target.universe, ch)] = max(0, min(255, val))
        return result

    def to_dict(self) -> dict:
        d = super().to_dict()
        d["steps"] = [list(s) for s in self.steps]
        d["step_time_ms"] = self.step_time_ms
        return d
