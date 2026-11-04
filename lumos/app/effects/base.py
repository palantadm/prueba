"""
Base classes for the Lumos effects engine.
"""
from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Optional


@dataclass
class EffectTarget:
    universe: int
    start_channel: int  # 1-indexed
    channel_count: int  # e.g., 3 for RGB


class Effect(ABC):
    def __init__(self) -> None:
        self.name: str = "Effect"
        self.enabled: bool = False
        self.targets: list[EffectTarget] = []
        self.speed: float = 1.0     # Hz
        self.size: float = 1.0      # 0.0-1.0 amplitude
        self.offset: float = 0.0    # phase offset 0.0-1.0
        self._elapsed_ms: float = 0.0

    def tick(self, dt_ms: float) -> None:
        if self.enabled:
            self._elapsed_ms += dt_ms
            self._apply()

    @abstractmethod
    def _apply(self) -> None: ...

    @abstractmethod
    def get_values(self) -> dict[tuple[int, int], int]:
        """Returns {(universe, channel_1indexed): value}"""
        ...

    def to_dict(self) -> dict:
        return {
            "type": type(self).__name__,
            "name": self.name,
            "enabled": self.enabled,
            "speed": self.speed,
            "size": self.size,
            "offset": self.offset,
            "targets": [
                {
                    "universe": t.universe,
                    "start_channel": t.start_channel,
                    "channel_count": t.channel_count,
                }
                for t in self.targets
            ],
        }
