"""Central fixture library — built-ins + imported fixtures."""
from __future__ import annotations

import os
import sys
from pathlib import Path
from typing import Optional

from fixtures.model import FixtureDef
from fixtures.builtin import BUILTIN_FIXTURES


_QLC_SEARCH_PATHS_WIN = [
    r"C:\QLC+\Fixtures",
    r"C:\Program Files\QLC+\Fixtures",
    r"C:\Program Files (x86)\QLC+\Fixtures",
]
_QLC_SEARCH_PATHS_LIN = [
    "/usr/share/qlcplus/fixtures",
    "/usr/local/share/qlcplus/fixtures",
    str(Path.home() / ".qlcplus" / "fixtures"),
]


class FixtureLibrary:
    """Holds all known fixture definitions; thread-safe for reads."""

    def __init__(self) -> None:
        self._fixtures: dict[str, FixtureDef] = {}
        for f in BUILTIN_FIXTURES:
            self._fixtures[f.key] = f

    # ── Load / import ────────────────────────────────────────────────────────

    def import_qlcplus_file(self, path: str | Path) -> Optional[FixtureDef]:
        from fixtures.formats.qlcplus import parse_qlcplus
        try:
            fd = parse_qlcplus(path)
            if fd:
                self._fixtures[fd.key] = fd
            return fd
        except Exception:
            return None

    def import_qlcplus_folder(self, folder: str | Path) -> int:
        from fixtures.formats.qlcplus import parse_qlcplus
        folder = Path(folder)
        count = 0
        for qxf in folder.rglob("*.qxf"):
            try:
                fd = parse_qlcplus(qxf)
                if fd:
                    self._fixtures[fd.key] = fd
                    count += 1
            except Exception:
                continue
        return count

    def import_ofl_json(self, path: str | Path, manufacturer: str = "") -> Optional[FixtureDef]:
        from fixtures.formats.ofl import parse_ofl
        try:
            fd = parse_ofl(path, manufacturer)
            if fd:
                self._fixtures[fd.key] = fd
            return fd
        except Exception:
            return None

    def import_ofl_folder(self, folder: str | Path) -> int:
        from fixtures.formats.ofl import parse_ofl
        folder = Path(folder)
        count = 0
        for mfr_dir in folder.iterdir():
            if not mfr_dir.is_dir():
                continue
            mfr = mfr_dir.name
            for jf in mfr_dir.glob("*.json"):
                try:
                    fd = parse_ofl(jf, mfr)
                    if fd:
                        self._fixtures[fd.key] = fd
                        count += 1
                except Exception:
                    continue
        return count

    def add_fixture(self, fd: FixtureDef) -> None:
        self._fixtures[fd.key] = fd

    # ── Auto-detect QLC+ ─────────────────────────────────────────────────────

    def autodetect_qlcplus(self) -> str:
        """Return the QLC+ fixtures folder found, or '' if not found."""
        paths = _QLC_SEARCH_PATHS_WIN if sys.platform == "win32" else _QLC_SEARCH_PATHS_LIN
        for p in paths:
            if Path(p).is_dir():
                return p
        return ""

    def import_qlcplus_auto(self) -> int:
        p = self.autodetect_qlcplus()
        if p:
            return self.import_qlcplus_folder(p)
        return 0

    # ── Query ────────────────────────────────────────────────────────────────

    @property
    def all_fixtures(self) -> list[FixtureDef]:
        return list(self._fixtures.values())

    @property
    def manufacturers(self) -> list[str]:
        return sorted({f.manufacturer for f in self._fixtures.values()})

    def by_manufacturer(self, mfr: str) -> list[FixtureDef]:
        return sorted(
            [f for f in self._fixtures.values() if f.manufacturer == mfr],
            key=lambda f: f.model,
        )

    def get(self, key: str) -> Optional[FixtureDef]:
        return self._fixtures.get(key)

    def search(self, query: str) -> list[FixtureDef]:
        q = query.lower().strip()
        if not q:
            return self.all_fixtures
        results: list[FixtureDef] = []
        for f in self._fixtures.values():
            blob = f"{f.manufacturer} {f.model} {f.fixture_type} {' '.join(f.categories)}".lower()
            if q in blob:
                results.append(f)
        return sorted(results, key=lambda f: (f.manufacturer, f.model))

    def search_by_type(self, fixture_type: str) -> list[FixtureDef]:
        t = fixture_type.lower()
        return [f for f in self._fixtures.values() if t in f.fixture_type.lower()]

    @property
    def total(self) -> int:
        return len(self._fixtures)

    @property
    def builtin_count(self) -> int:
        return len(BUILTIN_FIXTURES)


# Module-level singleton
_library: Optional[FixtureLibrary] = None


def get_library() -> FixtureLibrary:
    global _library
    if _library is None:
        _library = FixtureLibrary()
    return _library
