"""Fixture groups, submasters, grand master and blackout.

Hierarchy:
  Grand Master (0–100%) → scales ALL output
  Blackout              → overrides everything to 0 (except flash)
  Submaster (per group) → scales only that group's intensity channels
  Group                 → named set of fixture IDs
"""
import asyncio
from dataclasses import dataclass, field
from typing import Callable, Dict, List, Optional, Set


@dataclass
class FixtureGroup:
    id: str
    name: str
    fixture_ids: List[str] = field(default_factory=list)
    color: str = "#4488ff"  # UI indicator color

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "fixture_ids": list(self.fixture_ids),
            "color": self.color,
        }


@dataclass
class Submaster:
    group_id: str
    level: float = 1.0     # 0.0 – 1.0
    flash: bool = False    # momentary full-on (ignores blackout)

    def to_dict(self) -> dict:
        return {
            "group_id": self.group_id,
            "level": round(self.level, 4),
            "flash": self.flash,
        }


class GroupManager:
    """Manages fixture groups and submasters.

    Call `scale_for_fixture(fixture_id)` to get the combined scale factor
    (grand master × submaster) that should be applied to intensity channels.
    Non-intensity channels are unaffected by submasters but still obey
    grand master and blackout.
    """

    def __init__(self):
        self.groups: Dict[str, FixtureGroup] = {}
        self.submasters: Dict[str, Submaster] = {}  # group_id → Submaster
        self._grand_master: float = 1.0             # 0.0 – 1.0
        self._blackout: bool = False
        self._id_counter = 1
        self._subscribers: List[Callable] = []

        # Reverse index: fixture_id → set of group_ids
        self._fixture_groups: Dict[str, Set[str]] = {}

    # ── Grand master / blackout ───────────────────────────────────────────────

    @property
    def grand_master(self) -> float:
        return self._grand_master

    @grand_master.setter
    def grand_master(self, value: float):
        self._grand_master = max(0.0, min(1.0, value))
        self._notify()

    @property
    def blackout(self) -> bool:
        return self._blackout

    @blackout.setter
    def blackout(self, value: bool):
        self._blackout = bool(value)
        self._notify()

    # ── Groups ────────────────────────────────────────────────────────────────

    def create_group(self, name: str, fixture_ids: List[str] = None,
                     color: str = "#4488ff") -> FixtureGroup:
        gid = f"grp_{self._id_counter:04d}"
        self._id_counter += 1
        g = FixtureGroup(id=gid, name=name, fixture_ids=list(fixture_ids or []), color=color)
        self.groups[gid] = g
        self.submasters[gid] = Submaster(group_id=gid)
        for fid in g.fixture_ids:
            self._fixture_groups.setdefault(fid, set()).add(gid)
        return g

    def remove_group(self, group_id: str) -> bool:
        g = self.groups.pop(group_id, None)
        if not g:
            return False
        self.submasters.pop(group_id, None)
        for fid in g.fixture_ids:
            self._fixture_groups.get(fid, set()).discard(group_id)
        return True

    def add_fixture_to_group(self, group_id: str, fixture_id: str) -> bool:
        g = self.groups.get(group_id)
        if not g:
            return False
        if fixture_id not in g.fixture_ids:
            g.fixture_ids.append(fixture_id)
            self._fixture_groups.setdefault(fixture_id, set()).add(group_id)
        return True

    def remove_fixture_from_group(self, group_id: str, fixture_id: str) -> bool:
        g = self.groups.get(group_id)
        if not g:
            return False
        if fixture_id in g.fixture_ids:
            g.fixture_ids.remove(fixture_id)
            self._fixture_groups.get(fixture_id, set()).discard(group_id)
        return True

    # ── Submasters ────────────────────────────────────────────────────────────

    def set_submaster(self, group_id: str, level: float):
        sm = self.submasters.get(group_id)
        if sm:
            sm.level = max(0.0, min(1.0, level))
            self._notify()

    def flash_submaster(self, group_id: str, active: bool):
        sm = self.submasters.get(group_id)
        if sm:
            sm.flash = active
            self._notify()

    # ── Scaling ───────────────────────────────────────────────────────────────

    def scale_for_fixture(self, fixture_id: str) -> float:
        """Combined intensity scale for a fixture.

        Returns 0.0 when blackout is active (unless a submaster is flashing).
        Otherwise: grand_master × min(submaster levels).
        """
        # Flash overrides blackout
        group_ids = self._fixture_groups.get(fixture_id, set())
        if self._blackout:
            for gid in group_ids:
                sm = self.submasters.get(gid)
                if sm and sm.flash:
                    return 1.0
            return 0.0

        scale = self._grand_master
        if group_ids:
            lowest_sm = min(
                (self.submasters[gid].level for gid in group_ids if gid in self.submasters),
                default=1.0,
            )
            # Flash overrides submaster level
            for gid in group_ids:
                sm = self.submasters.get(gid)
                if sm and sm.flash:
                    lowest_sm = 1.0
                    break
            scale *= lowest_sm
        return scale

    def apply_to_value(self, fixture_id: str, value: int, is_intensity: bool) -> int:
        """Scale a channel value. Only intensity channels get submaster scaling."""
        if is_intensity:
            return max(0, min(255, int(value * self.scale_for_fixture(fixture_id))))
        if self._blackout:
            group_ids = self._fixture_groups.get(fixture_id, set())
            for gid in group_ids:
                sm = self.submasters.get(gid)
                if sm and sm.flash:
                    return value
            return 0
        return max(0, min(255, int(value * self._grand_master)))

    # ── Subscriptions ─────────────────────────────────────────────────────────

    def subscribe(self, callback: Callable):
        self._subscribers.append(callback)

    def _notify(self):
        for cb in self._subscribers:
            try:
                cb()
            except Exception:
                pass

    # ── Serialization ─────────────────────────────────────────────────────────

    def to_dict(self) -> dict:
        return {
            "grand_master": round(self._grand_master, 4),
            "blackout": self._blackout,
            "groups": [g.to_dict() for g in self.groups.values()],
            "submasters": {gid: sm.to_dict() for gid, sm in self.submasters.items()},
        }

    def from_dict(self, data: dict):
        self._grand_master = float(data.get("grand_master", 1.0))
        self._blackout = bool(data.get("blackout", False))
        self.groups.clear()
        self.submasters.clear()
        self._fixture_groups.clear()
        for gd in data.get("groups", []):
            g = FixtureGroup(**{k: v for k, v in gd.items()})
            self.groups[g.id] = g
            for fid in g.fixture_ids:
                self._fixture_groups.setdefault(fid, set()).add(g.id)
        for gid, smd in data.get("submasters", {}).items():
            self.submasters[gid] = Submaster(**smd)
