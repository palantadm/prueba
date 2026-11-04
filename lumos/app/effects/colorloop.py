"""
Color loop effect — smooth HSV hue rotation.
"""
from __future__ import annotations

import colorsys
import math

from app.effects.base import Effect, EffectTarget


class ColorLoopEffect(Effect):
    """
    Continuously rotates through the HSV color wheel.

    - speed (Hz): full hue rotation per second when speed=1.0
    - size (0.0-1.0): saturation (0 = white/desaturated, 1 = full color)
    - Assumes each target has at least 3 channels (R, G, B).
    """

    def __init__(self) -> None:
        super().__init__()
        self.name = "Color Loop"

    def _apply(self) -> None:
        # State is computed on-the-fly in get_values from _elapsed_ms
        pass

    def get_values(self) -> dict[tuple[int, int], int]:
        result: dict[tuple[int, int], int] = {}
        if not self.targets:
            return result

        elapsed_s = self._elapsed_ms / 1000.0
        hue = (self.speed * elapsed_s + self.offset) % 1.0
        sat = max(0.0, min(1.0, self.size))
        r, g, b = colorsys.hsv_to_rgb(hue, sat, 1.0)

        r_val = int(r * 255)
        g_val = int(g * 255)
        b_val = int(b * 255)

        for target in self.targets:
            if target.channel_count >= 1:
                result[(target.universe, target.start_channel)] = r_val
            if target.channel_count >= 2:
                result[(target.universe, target.start_channel + 1)] = g_val
            if target.channel_count >= 3:
                result[(target.universe, target.start_channel + 2)] = b_val

        return result
