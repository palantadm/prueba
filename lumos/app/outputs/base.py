"""
Base class for DMX output backends.
"""
from __future__ import annotations

from abc import ABC, abstractmethod


class OutputBackend(ABC):
    """Abstract base for all output backends (Art-Net, sACN, Enttec, …)."""

    name: str = "Base"

    @abstractmethod
    def open(self) -> bool:
        """Open/connect the backend.  Returns True on success."""
        ...

    @abstractmethod
    def close(self) -> None:
        """Close/disconnect the backend."""
        ...

    @abstractmethod
    def send(self, universe_id: int, data: bytearray) -> None:
        """Send 512-byte DMX frame for *universe_id*."""
        ...

    @property
    @abstractmethod
    def is_open(self) -> bool:
        """True if the backend is currently open and ready."""
        ...

    @abstractmethod
    def config_dict(self) -> dict:
        """Return a JSON-serialisable dict with all config parameters."""
        ...

    @classmethod
    @abstractmethod
    def from_config(cls, cfg: dict) -> "OutputBackend":
        """Reconstruct a backend instance from a config_dict."""
        ...
