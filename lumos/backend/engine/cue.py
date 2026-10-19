"""Cue, Scene, and Timeline management."""
import time
import asyncio
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Callable, Tuple
from enum import Enum


class FadeType(str, Enum):
    LINEAR = "linear"
    EASE_IN = "ease_in"
    EASE_OUT = "ease_out"
    EASE_IN_OUT = "ease_in_out"
    SNAP = "snap"


@dataclass
class ChannelValue:
    """A specific channel value in a cue."""
    fixture_id: str
    channel: str
    value: int  # 0-255


@dataclass
class Cue:
    """A single cue: a set of channel values with fade info."""
    id: str
    name: str
    values: List[ChannelValue] = field(default_factory=list)
    fade_in: float = 0.0    # seconds
    fade_out: float = 0.0   # seconds
    hold: float = 0.0       # seconds (0 = manual trigger)
    fade_type: FadeType = FadeType.LINEAR
    notes: str = ""

    def set_channel(self, fixture_id: str, channel: str, value: int):
        for cv in self.values:
            if cv.fixture_id == fixture_id and cv.channel == channel:
                cv.value = value
                return
        self.values.append(ChannelValue(fixture_id=fixture_id, channel=channel, value=value))

    def get_value(self, fixture_id: str, channel: str) -> Optional[int]:
        for cv in self.values:
            if cv.fixture_id == fixture_id and cv.channel == channel:
                return cv.value
        return None

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "fade_in": self.fade_in,
            "fade_out": self.fade_out,
            "hold": self.hold,
            "fade_type": self.fade_type,
            "notes": self.notes,
            "values": [
                {"fixture_id": cv.fixture_id, "channel": cv.channel, "value": cv.value}
                for cv in self.values
            ],
        }


@dataclass
class CueList:
    """An ordered list of cues (like a scene sequence)."""
    id: str
    name: str
    cues: List[Cue] = field(default_factory=list)
    loop: bool = False
    current_index: int = -1

    def add_cue(self, cue: Cue, position: int = -1):
        if position < 0:
            self.cues.append(cue)
        else:
            self.cues.insert(position, cue)

    def remove_cue(self, cue_id: str) -> bool:
        for i, c in enumerate(self.cues):
            if c.id == cue_id:
                self.cues.pop(i)
                if self.current_index >= len(self.cues):
                    self.current_index = len(self.cues) - 1
                return True
        return False

    def next_cue(self) -> Optional[Cue]:
        if not self.cues:
            return None
        if self.current_index < len(self.cues) - 1:
            return self.cues[self.current_index + 1]
        if self.loop:
            return self.cues[0]
        return None

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "loop": self.loop,
            "current_index": self.current_index,
            "cues": [c.to_dict() for c in self.cues],
        }


class FadeEngine:
    """Handles smooth channel fades."""

    @dataclass
    class ActiveFade:
        fixture_id: str
        channel: str
        start_val: int
        end_val: int
        start_time: float
        duration: float
        fade_type: FadeType

        def value_at(self, now: float) -> Tuple[int, bool]:
            """Returns (current_value, is_complete)."""
            if self.duration <= 0:
                return self.end_val, True
            elapsed = now - self.start_time
            if elapsed >= self.duration:
                return self.end_val, True
            t = elapsed / self.duration
            t = FadeEngine._apply_curve(t, self.fade_type)
            val = int(self.start_val + (self.end_val - self.start_val) * t)
            return val, False

    @staticmethod
    def _apply_curve(t: float, fade_type: FadeType) -> float:
        if fade_type == FadeType.EASE_IN:
            return t * t
        if fade_type == FadeType.EASE_OUT:
            return 1 - (1 - t) * (1 - t)
        if fade_type == FadeType.EASE_IN_OUT:
            return t * t * (3 - 2 * t)
        return t  # LINEAR

    def __init__(self):
        self._fades: Dict[Tuple[str, str], "FadeEngine.ActiveFade"] = {}

    def start_fade(self, fixture_id: str, channel: str,
                   start_val: int, end_val: int,
                   duration: float, fade_type: FadeType = FadeType.LINEAR):
        key = (fixture_id, channel)
        self._fades[key] = self.ActiveFade(
            fixture_id=fixture_id,
            channel=channel,
            start_val=start_val,
            end_val=end_val,
            start_time=time.monotonic(),
            duration=duration,
            fade_type=fade_type,
        )

    def tick(self) -> Tuple[List[Tuple[str, str, int]], List[Tuple[str, str]]]:
        """Returns (updates, completed) lists of (fixture_id, channel, value)."""
        now = time.monotonic()
        updates = []
        completed = []
        for key, fade in list(self._fades.items()):
            val, done = fade.value_at(now)
            updates.append((fade.fixture_id, fade.channel, val))
            if done:
                completed.append(key)
        for key in completed:
            del self._fades[key]
        return updates, completed

    def cancel(self, fixture_id: str, channel: str):
        self._fades.pop((fixture_id, channel), None)

    def cancel_fixture(self, fixture_id: str):
        keys = [k for k in self._fades if k[0] == fixture_id]
        for k in keys:
            del self._fades[k]

    @property
    def active_count(self) -> int:
        return len(self._fades)


class CueManager:
    """Manages cue lists and playback state."""

    def __init__(self, patch_manager, universe_manager):
        self.patch = patch_manager
        self.universes = universe_manager
        self.cue_lists: Dict[str, CueList] = {}
        self._live_values: Dict[Tuple[int, int], int] = {}  # (universe, ch) -> value
        self._fade_engine = FadeEngine()
        self._running = False
        self._task: Optional[asyncio.Task] = None
        self._id_counter = 1
        self._subscribers: List[Callable] = []

    def _next_id(self, prefix: str = "cue") -> str:
        fid = f"{prefix}_{self._id_counter:04d}"
        self._id_counter += 1
        return fid

    def subscribe(self, callback: Callable):
        self._subscribers.append(callback)

    def create_cue_list(self, name: str) -> CueList:
        cl = CueList(id=self._next_id("cl"), name=name)
        self.cue_lists[cl.id] = cl
        return cl

    def create_cue(self, name: str, fade_in: float = 0.0, hold: float = 0.0) -> Cue:
        return Cue(id=self._next_id("cue"), name=name, fade_in=fade_in, hold=hold)

    def set_channel_direct(self, fixture_id: str, channel: str, value: int):
        """Set a channel directly (programmer/manual override)."""
        addr = self.patch.get_channel_address(fixture_id, channel)
        if addr:
            universe_id, dmx_ch = addr
            self._live_values[(universe_id, dmx_ch)] = value
            u = self.universes.get_universe(universe_id)
            if u:
                u.set_channel(dmx_ch, value)

    def go_cue(self, cue: Cue):
        """Execute a cue, starting fades for all channels."""
        for cv in cue.values:
            addr = self.patch.get_channel_address(cv.fixture_id, cv.channel)
            if not addr:
                continue
            universe_id, dmx_ch = addr
            current = self._live_values.get((universe_id, dmx_ch), 0)
            self._fade_engine.start_fade(
                fixture_id=cv.fixture_id,
                channel=cv.channel,
                start_val=current,
                end_val=cv.value,
                duration=cue.fade_in,
                fade_type=cue.fade_type,
            )

    def go_next(self, cue_list_id: str):
        """Advance to the next cue in a list."""
        cl = self.cue_lists.get(cue_list_id)
        if not cl:
            return
        next_cue = cl.next_cue()
        if next_cue:
            cl.current_index = cl.cues.index(next_cue)
            self.go_cue(next_cue)
            for cb in self._subscribers:
                asyncio.create_task(cb("cue_go", {
                    "cue_list_id": cue_list_id,
                    "cue_id": next_cue.id,
                    "cue_name": next_cue.name,
                    "index": cl.current_index,
                }))

    def remap_fixtures(self, old_ids: List[str], new_ids: List[str],
                       mappings) -> int:
        """Update cue data after a morph. Returns number of entries updated."""
        count = 0
        old_to_new = dict(zip(old_ids, new_ids))
        channel_map = {m.source_channel: m.target_channel for m in mappings}

        for cl in self.cue_lists.values():
            for cue in cl.cues:
                new_values = []
                for cv in cue.values:
                    if cv.fixture_id in old_to_new:
                        new_ch = channel_map.get(cv.channel)
                        if new_ch:
                            new_values.append(ChannelValue(
                                fixture_id=old_to_new[cv.fixture_id],
                                channel=new_ch,
                                value=cv.value,
                            ))
                            count += 1
                    else:
                        new_values.append(cv)
                cue.values = new_values
        return count

    async def start(self):
        self._running = True
        self._task = asyncio.create_task(self._tick_loop())

    async def stop(self):
        self._running = False
        if self._task:
            self._task.cancel()

    async def _tick_loop(self):
        while self._running:
            updates, _ = self._fade_engine.tick()
            for fixture_id, channel, value in updates:
                addr = self.patch.get_channel_address(fixture_id, channel)
                if addr:
                    universe_id, dmx_ch = addr
                    self._live_values[(universe_id, dmx_ch)] = value
                    u = self.universes.get_universe(universe_id)
                    if u:
                        u.set_channel(dmx_ch, value)
            await asyncio.sleep(1 / 50)  # 50Hz tick

    def to_dict(self) -> List[dict]:
        return [cl.to_dict() for cl in self.cue_lists.values()]
