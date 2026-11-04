"""
Wave effect — sine wave spreading across multiple fixtures.
"""
from __future__ import annotations

import math

from app.effects.base import Effect, EffectTarget


class WaveEffect(Effect):
    """
    A sine wave that propagates across a row of fixtures/targets.

    Each target in the list is phase-shifted relative to its neighbours by
    ``1.0 / len(targets)``, creating a travelling wave (snake/chase effect).

    Formula per target i:
        phase_i = 2π × speed × elapsed_s + offset×2π + i × (2π / N)
        value_i = base + (sin(phase_i) × 0.5 + 0.5) × size × 255

    ``intensity_offset`` selects which channel within each target block is
    controlled (0 = first channel).
    """

    def __init__(self) -> None:
        super().__init__()
        self.name = "Wave"
        self.base: int = 0
        self.intensity_offset: int = 0

    def _apply(self) -> None:
        pass

    def get_values(self) -> dict[tuple[int, int], int]:
        result: dict[tuple[int, int], int] = {}
        n = len(self.targets)
        if n == 0:
            return result

        elapsed_s = self._elapsed_ms / 1000.0
        spread = 2.0 * math.pi / n if n > 1 else 0.0

        for i, target in enumerate(self.targets):
            phase = (
                2.0 * math.pi * self.speed * elapsed_s
                + self.offset * 2.0 * math.pi
                + i * spread
            )
            amp = (math.sin(phase) * 0.5 + 0.5) * self.size * 255.0
            value = max(0, min(255, int(self.base + amp)))
            ch = target.start_channel + self.intensity_offset
            if ch <= 512:
                result[(target.universe, ch)] = value

        return result

    def to_dict(self) -> dict:
        d = super().to_dict()
        d["base"] = self.base
        d["intensity_offset"] = self.intensity_offset
        return d
