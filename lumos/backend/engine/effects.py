"""Effects Engine — applies dynamic patterns to fixture channels.

Patterns (waveforms):
  sine, cosine, square, triangle, sawtooth, reverse_saw,
  random (sample & hold), random_smooth (interpolated noise),
  pulse, step_sequence, audio_envelope (future)

Each effect runs on one or more fixture channels with:
  - Phase offset per fixture (for fan/spread effects)
  - Amplitude (depth)
  - Center value (offset)
  - Speed (Hz)
  - Width (duty cycle for square/pulse)
  - Step sequence list (for step effects)
  - One-shot mode (plays once then stops)
  - BPM sync (locks speed to MIDI clock)
"""
import asyncio
import math
import random
import time
from dataclasses import dataclass, field
from typing import Callable, Dict, List, Optional, Tuple
from enum import Enum


class Waveform(str, Enum):
    SINE           = "sine"
    COSINE         = "cosine"
    SQUARE         = "square"
    TRIANGLE       = "triangle"
    SAWTOOTH       = "sawtooth"
    REVERSE_SAW    = "reverse_saw"
    RANDOM         = "random"
    RANDOM_SMOOTH  = "random_smooth"
    PULSE          = "pulse"
    STEP_SEQUENCE  = "step_sequence"


@dataclass
class EffectChannel:
    """A single channel within an effect."""
    fixture_id: str
    channel: str
    phase: float = 0.0       # 0.0 – 1.0 (phase offset in cycle)


@dataclass
class Effect:
    """A running effect on a set of channels."""
    id: str
    name: str
    waveform: Waveform = Waveform.SINE
    channels: List[EffectChannel] = field(default_factory=list)

    speed: float = 1.0        # Hz
    amplitude: int = 127      # 0-255 peak-to-peak half
    center: int = 127         # 0-255 center value
    width: float = 0.5        # 0.0-1.0 (duty cycle for square/pulse)
    steps: List[int] = field(default_factory=lambda: [0,64,128,192,255])
    phase_spread: float = 0.0 # auto-spread phase across fixtures (0=off, 1.0=full cycle)

    bpm_sync: bool = False
    bpm_multiplier: float = 1.0  # relative to BPM (e.g. 0.5 = half time)

    one_shot: bool = False
    enabled: bool = True

    # Internal
    _start_time: float = field(default_factory=time.monotonic, repr=False)
    _random_values: Dict[str, Tuple[float, float, float]] = field(default_factory=dict, repr=False)

    def apply_phase_spread(self):
        """Distribute phase_spread evenly across channels."""
        if self.phase_spread > 0 and len(self.channels) > 1:
            spread = self.phase_spread / len(self.channels)
            for i, ch in enumerate(self.channels):
                ch.phase = spread * i

    def value_at(self, t: float, phase: float) -> int:
        """Compute effect output [0-255] at normalized time t (seconds) with phase offset."""
        period = 1.0 / max(0.001, self.speed)
        tp = ((t / period) + phase) % 1.0  # normalized phase 0-1
        raw = self._wave(tp, self.waveform)
        val = self.center + int(raw * self.amplitude)
        return max(0, min(255, val))

    def _wave(self, tp: float, waveform: Waveform) -> float:
        """Return normalized waveform value -1.0 to +1.0."""
        if waveform == Waveform.SINE:
            return math.sin(tp * 2 * math.pi)
        if waveform == Waveform.COSINE:
            return math.cos(tp * 2 * math.pi)
        if waveform == Waveform.SQUARE:
            return 1.0 if tp < self.width else -1.0
        if waveform == Waveform.TRIANGLE:
            if tp < 0.5:
                return tp * 4 - 1
            return 3 - tp * 4
        if waveform == Waveform.SAWTOOTH:
            return tp * 2 - 1
        if waveform == Waveform.REVERSE_SAW:
            return 1 - tp * 2
        if waveform == Waveform.PULSE:
            return 1.0 if tp < self.width else -1.0
        if waveform == Waveform.RANDOM:
            return self._random_sh(tp)
        if waveform == Waveform.RANDOM_SMOOTH:
            return self._random_smooth(tp)
        if waveform == Waveform.STEP_SEQUENCE:
            return self._step(tp)
        return 0.0

    def _random_sh(self, tp: float) -> float:
        """Random sample-and-hold: new random value each cycle."""
        key = "rsh"
        entry = self._random_values.get(key, (0.0, 0.0, 0.0))
        last_tp, last_val, _ = entry
        if tp < last_tp:  # cycle wrapped
            new_val = random.uniform(-1.0, 1.0)
            self._random_values[key] = (tp, new_val, 0.0)
            return new_val
        self._random_values[key] = (tp, last_val, 0.0)
        return last_val

    def _random_smooth(self, tp: float) -> float:
        """Smoothly interpolated random noise."""
        # Simple implementation using noise-like interpolation
        floor_tp = math.floor(tp * 4) / 4
        frac = (tp * 4) % 1.0
        import hashlib
        def rnd(x):
            h = int(hashlib.md5(str(x).encode()).hexdigest(), 16)
            return (h % 10000) / 5000.0 - 1.0
        v0 = rnd(floor_tp * 1000 + int(self.id.replace("eff_","") or 0))
        v1 = rnd((floor_tp + 0.25) * 1000 + int(self.id.replace("eff_","") or 0))
        t2 = frac * frac * (3 - 2 * frac)  # smoothstep
        return v0 + (v1 - v0) * t2

    def _step(self, tp: float) -> float:
        if not self.steps:
            return 0.0
        idx = int(tp * len(self.steps))
        val = self.steps[min(idx, len(self.steps)-1)]
        return (val / 127.5) - 1.0  # normalize to -1..+1

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "waveform": self.waveform,
            "speed": self.speed,
            "amplitude": self.amplitude,
            "center": self.center,
            "width": self.width,
            "steps": self.steps,
            "phase_spread": self.phase_spread,
            "bpm_sync": self.bpm_sync,
            "bpm_multiplier": self.bpm_multiplier,
            "one_shot": self.one_shot,
            "enabled": self.enabled,
            "channels": [
                {"fixture_id": c.fixture_id, "channel": c.channel, "phase": c.phase}
                for c in self.channels
            ],
        }


class EffectsEngine:
    """Runs all active effects and merges them into DMX output."""

    def __init__(self, patch_manager, universe_manager):
        self.patch = patch_manager
        self.universes = universe_manager
        self.effects: Dict[str, Effect] = {}
        self._running = False
        self._task: Optional[asyncio.Task] = None
        self._bpm: float = 120.0
        self._id_counter = 1
        self._subscribers: List[Callable] = []

    def set_bpm(self, bpm: float):
        """Called by MIDI clock handler."""
        self._bpm = max(20.0, min(300.0, bpm))

    def subscribe(self, callback: Callable):
        self._subscribers.append(callback)

    def _next_id(self) -> str:
        fid = f"eff_{self._id_counter:04d}"
        self._id_counter += 1
        return fid

    def create_effect(self, name: str, waveform: Waveform = Waveform.SINE,
                      fixture_ids: List[str] = None, channels: List[str] = None,
                      **kwargs) -> Effect:
        """Create and register an effect."""
        eff = Effect(id=self._next_id(), name=name, waveform=waveform, **kwargs)
        if fixture_ids and channels:
            for fid in fixture_ids:
                for ch in channels:
                    eff.channels.append(EffectChannel(fixture_id=fid, channel=ch))
            if eff.phase_spread > 0:
                eff.apply_phase_spread()
        self.effects[eff.id] = eff
        return eff

    def remove_effect(self, effect_id: str) -> bool:
        if effect_id in self.effects:
            del self.effects[effect_id]
            return True
        return False

    async def start(self):
        self._running = True
        self._task = asyncio.create_task(self._tick_loop())

    async def stop(self):
        self._running = False
        if self._task:
            self._task.cancel()

    async def _tick_loop(self):
        while self._running:
            t = time.monotonic()
            updates: List[Tuple[str, str, int]] = []

            for eff in list(self.effects.values()):
                if not eff.enabled:
                    continue
                elapsed = t - eff._start_time

                # BPM sync
                speed = eff.speed
                if eff.bpm_sync:
                    speed = (self._bpm / 60.0) * eff.bpm_multiplier

                # Temporarily override speed for value_at
                orig_speed = eff.speed
                eff.speed = speed

                for ch_entry in eff.channels:
                    val = eff.value_at(elapsed, ch_entry.phase)
                    updates.append((ch_entry.fixture_id, ch_entry.channel, val))

                eff.speed = orig_speed

                # One-shot: disable after one cycle
                if eff.one_shot:
                    period = 1.0 / max(0.001, speed)
                    if elapsed >= period:
                        eff.enabled = False

            # Apply to universe
            for fixture_id, channel, value in updates:
                addr = self.patch.get_channel_address(fixture_id, channel)
                if addr:
                    uid, dmx_ch = addr
                    u = self.universes.get_universe(uid)
                    if u:
                        u.set_channel(dmx_ch, value)

            # Notify subscribers
            if updates and self._subscribers:
                for cb in self._subscribers:
                    try:
                        await cb("effects_tick", len(updates))
                    except Exception:
                        pass

            await asyncio.sleep(1 / 50)  # 50Hz

    def to_dict(self) -> List[dict]:
        return [e.to_dict() for e in self.effects.values()]
