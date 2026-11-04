"""
Pulse effect — sinusoidal dimmer oscillation.
"""
from __future__ import annotations

import math

from app.effects.base import Effect, EffectTarget


class PulseEffect(Effect):
    """
    Applies a sine-wave intensity oscillation to a single channel per target.

    Formula:
        value = base + (sin(2π × speed × elapsed_s + offset×2π) × 0.5 + 0.5) × size × 255

    ``intensity_offset`` selects which channel within the target's block is
    used as the intensity channel (0 = first channel, i.e. start_channel).
    """

    def __init__(self) -> None:
        super().__init__()
        self.name = "Pulse"
        self.base: int = 0           # base intensity (0-255)
        self.intensity_offset: int = 0  # channel offset within target block

    def _apply(self) -> None:
        # Computed on-the-fly in get_values
        pass

    def get_values(self) -> dict[tuple[int, int], int]:
        result: dict[tuple[int, int], int] = {}
        if not self.targets:
            return result

        elapsed_s = self._elapsed_ms / 1000.0
        phase = 2.0 * math.pi * self.speed * elapsed_s + self.offset * 2.0 * math.pi
        amp = (math.sin(phase) * 0.5 + 0.5) * self.size * 255.0
        value = max(0, min(255, int(self.base + amp)))

        for target in self.targets:
            ch = target.start_channel + self.intensity_offset
            if ch <= 512:
                result[(target.universe, ch)] = value

        return result

    def to_dict(self) -> dict:
        d = super().to_dict()
        d["base"] = self.base
        d["intensity_offset"] = self.intensity_offset
        return d
