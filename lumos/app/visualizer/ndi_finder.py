"""
NDI network source scanner.
Requires ndi-python: pip install ndi-python
Falls back gracefully to an empty list if not available.
"""
from __future__ import annotations

import time

try:
    import NDIlib as ndi
    _HAS_NDI = True
except ImportError:
    _HAS_NDI = False


def scan_ndi_sources(timeout_ms: int = 2000) -> list[str]:
    """
    Return a list of NDI source names visible on the local network.
    Blocks for up to timeout_ms milliseconds.
    Returns an empty list if NDI is not available.
    """
    if not _HAS_NDI:
        return []

    try:
        ndi.initialize()
        finder = ndi.find_create_v2()
        if not finder:
            return []

        # Wait and collect sources
        deadline = time.monotonic() + timeout_ms / 1000.0
        while time.monotonic() < deadline:
            ndi.find_wait_for_sources(finder, 100)  # 100 ms poll

        sources = ndi.find_get_current_sources(finder)
        names = [src.ndi_name for src in (sources or [])]
        ndi.find_destroy(finder)
        return names
    except Exception:
        return []
