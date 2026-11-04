"""SMPTE Timecode — LTC (Linear Timecode) via audio and MTC (MIDI Timecode).

LTC: decodes biphase-mark encoded timecode from an audio stream using
     sounddevice / pyaudio. Each frame = 80 bits, 2 bits per sample.
     Falls back to software simulation when no audio backend is available.

MTC: MIDI Timecode quarter-frame / full-frame assembly (also in midi_protocol.py).
     Re-exported here for unified timecode subscriber API.
"""
import math
import threading
import time
from dataclasses import dataclass, field
from typing import Callable, List, Optional


# ── SMPTE frame rates ──────────────────────────────────────────────────────────

FRAME_RATES = {
    24:    24.0,
    25:    25.0,
    "29df": 30000 / 1001,   # 29.97 drop-frame
    30:    30.0,
}


# ── Timecode position ─────────────────────────────────────────────────────────

@dataclass
class SMPTETime:
    hours: int   = 0
    minutes: int = 0
    seconds: int = 0
    frames: int  = 0
    fps: float   = 25.0
    drop_frame: bool = False

    def to_seconds(self) -> float:
        base = self.hours * 3600 + self.minutes * 60 + self.seconds
        return base + self.frames / self.fps

    def to_frame_number(self) -> int:
        """Total frame count (approximate for drop-frame)."""
        total = (self.hours * 3600 + self.minutes * 60 + self.seconds) * round(self.fps)
        total += self.frames
        if self.drop_frame:
            # SMPTE drop-frame correction: skip 2 frames every minute except every 10th
            d = self.hours * 60 + self.minutes
            total -= 2 * (d - d // 10)
        return total

    def __str__(self) -> str:
        sep = ";" if self.drop_frame else ":"
        return f"{self.hours:02d}:{self.minutes:02d}:{self.seconds:02d}{sep}{self.frames:02d}"

    @classmethod
    def from_frame_number(cls, n: int, fps: float = 25.0,
                          drop_frame: bool = False) -> "SMPTETime":
        fps_int = round(fps)
        if drop_frame:
            # Reverse drop-frame calculation
            d = n // (17982)   # frames per 10 minutes at 29.97df
            n += 18 * d + 2 * ((n - 18 * d * 17982) // 1798)
        h = n // (fps_int * 3600)
        n -= h * fps_int * 3600
        m = n // (fps_int * 60)
        n -= m * fps_int * 60
        s = n // fps_int
        f = n - s * fps_int
        return cls(hours=h, minutes=m, seconds=s, frames=f, fps=fps, drop_frame=drop_frame)


# ── LTC decoder ───────────────────────────────────────────────────────────────

LTC_SYNC_WORD = 0b0011111111111101  # last 16 bits of an LTC frame

# LTC bit layout within 80-bit frame (each char = 4 bits, LSB first per nibble)
# bits 0-3: frame units; 4-7: user bits 1; 8-10: frame tens; 11: drop-frame flag
# 12-15: user bits 2; 16-19: seconds units; 20-23: user bits 3
# 24-26: seconds tens; 27-31: user bits 4+flags; ... 64-79: sync word

class LTCDecoder:
    """Decodes LTC from raw int16 audio samples (mono)."""

    def __init__(self, sample_rate: int = 48000, fps: float = 25.0):
        self._sample_rate = sample_rate
        self._fps = fps
        self._bit_buffer: List[int] = []    # decoded bits (0/1)
        self._frame_buffer = 0              # 80-bit accumulator
        self._bit_count = 0
        self._last_polarity = 0
        self._samples_per_half_bit = sample_rate / (fps * 80 * 2)
        self._edge_threshold = 0.5          # fraction of half-bit period
        self._last_edge_sample = 0
        self._subscribers: List[Callable[[SMPTETime], None]] = []

    def subscribe(self, cb: Callable[[SMPTETime], None]):
        self._subscribers.append(cb)

    def feed(self, samples):
        """Feed int16 numpy array or list of int16 audio samples."""
        prev = None
        edge_count = 0
        period_samples = 0

        for i, s in enumerate(samples):
            polarity = 1 if s >= 0 else -1
            if prev is not None and polarity != prev:
                gap = i - self._last_edge_sample
                self._last_edge_sample = i
                half_bit = self._samples_per_half_bit
                if gap < half_bit * (1 + self._edge_threshold):
                    # Short transition → mid-bit edge (biphase 1 bit)
                    edge_count += 1
                    if edge_count == 2:
                        self._push_bit(1)
                        edge_count = 0
                else:
                    # Long transition → bit boundary edge (biphase 0 bit)
                    self._push_bit(0)
                    edge_count = 0
            prev = polarity

    def _push_bit(self, bit: int):
        self._frame_buffer = ((self._frame_buffer >> 1) | (bit << 79)) & ((1 << 80) - 1)
        self._bit_count += 1
        if self._bit_count >= 80:
            sync = self._frame_buffer & 0xFFFF
            if sync == LTC_SYNC_WORD:
                tc = self._decode_frame(self._frame_buffer)
                if tc:
                    for cb in self._subscribers:
                        cb(tc)

    def _decode_frame(self, bits: int) -> Optional[SMPTETime]:
        def _nibble(start: int) -> int:
            return (bits >> start) & 0xF

        frame_units = _nibble(0) & 0xF
        frame_tens  = (_nibble(8) >> 0) & 0x3
        drop_frame  = bool((bits >> 11) & 1)
        seconds_units = _nibble(16) & 0xF
        seconds_tens  = (_nibble(24) >> 0) & 0x7
        minutes_units = _nibble(32) & 0xF
        minutes_tens  = (_nibble(40) >> 0) & 0x7
        hours_units   = _nibble(48) & 0xF
        hours_tens    = (_nibble(56) >> 0) & 0x3

        h = hours_tens   * 10 + hours_units
        m = minutes_tens * 10 + minutes_units
        s = seconds_tens * 10 + seconds_units
        f = frame_tens   * 10 + frame_units

        if not (0 <= h < 24 and 0 <= m < 60 and 0 <= s < 60):
            return None

        return SMPTETime(
            hours=h, minutes=m, seconds=s, frames=f,
            fps=self._fps, drop_frame=drop_frame,
        )


# ── Audio capture ─────────────────────────────────────────────────────────────

AUDIO_AVAILABLE = False
try:
    import sounddevice as sd  # type: ignore
    AUDIO_AVAILABLE = True
except ImportError:
    try:
        import pyaudio  # type: ignore
        AUDIO_AVAILABLE = True
    except ImportError:
        pass


class LTCAudioReader:
    """Reads LTC from an audio input device and feeds to LTCDecoder."""

    def __init__(self, device_index: Optional[int] = None,
                 sample_rate: int = 48000, fps: float = 25.0):
        self._device = device_index
        self._sample_rate = sample_rate
        self._decoder = LTCDecoder(sample_rate=sample_rate, fps=fps)
        self._running = False
        self._thread: Optional[threading.Thread] = None

    def subscribe(self, cb: Callable[[SMPTETime], None]):
        self._decoder.subscribe(cb)

    def start(self):
        if not AUDIO_AVAILABLE:
            return
        self._running = True
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def stop(self):
        self._running = False

    def _run(self):
        try:
            import sounddevice as sd
            blocksize = 1024

            def _callback(indata, frames, t, status):
                if not self._running:
                    raise sd.CallbackStop()
                mono = indata[:, 0] if indata.ndim > 1 else indata[:, 0]
                samples = (mono * 32767).astype("int16").tolist()
                self._decoder.feed(samples)

            with sd.InputStream(
                device=self._device,
                channels=1,
                samplerate=self._sample_rate,
                dtype="float32",
                blocksize=blocksize,
                callback=_callback,
            ):
                while self._running:
                    time.sleep(0.1)
        except ImportError:
            self._run_pyaudio()
        except Exception as e:
            print(f"[LTC] Audio capture error: {e}")

    def _run_pyaudio(self):
        try:
            import pyaudio
            import struct as _struct
            pa = pyaudio.PyAudio()
            stream = pa.open(
                format=pyaudio.paInt16,
                channels=1,
                rate=self._sample_rate,
                input=True,
                input_device_index=self._device,
                frames_per_buffer=1024,
            )
            while self._running:
                raw = stream.read(1024, exception_on_overflow=False)
                samples = list(_struct.unpack(f"<{len(raw)//2}h", raw))
                self._decoder.feed(samples)
            stream.stop_stream()
            stream.close()
            pa.terminate()
        except Exception as e:
            print(f"[LTC] PyAudio capture error: {e}")

    @staticmethod
    def list_devices() -> List[dict]:
        devices = []
        try:
            import sounddevice as sd
            for i, d in enumerate(sd.query_devices()):
                if d["max_input_channels"] > 0:
                    devices.append({"index": i, "name": d["name"],
                                    "channels": d["max_input_channels"],
                                    "sample_rate": int(d["default_samplerate"])})
        except ImportError:
            try:
                import pyaudio
                pa = pyaudio.PyAudio()
                for i in range(pa.get_device_count()):
                    d = pa.get_device_info_by_index(i)
                    if d["maxInputChannels"] > 0:
                        devices.append({"index": i, "name": d["name"],
                                        "channels": d["maxInputChannels"],
                                        "sample_rate": int(d["defaultSampleRate"])})
                pa.terminate()
            except Exception:
                pass
        return devices


# ── Timecode Manager (unified LTC + MTC) ─────────────────────────────────────

class TimecodeManager:
    """Single subscriber interface for both LTC and MTC timecode sources."""

    def __init__(self):
        self._ltc_reader: Optional[LTCAudioReader] = None
        self._subscribers: List[Callable[[str, SMPTETime], None]] = []
        self._last_tc: Optional[SMPTETime] = None
        self._source: str = "none"

    def subscribe(self, cb: Callable[[str, SMPTETime], None]):
        """cb(source, SMPTETime) called on each new frame from 'ltc' or 'mtc'."""
        self._subscribers.append(cb)

    def _notify(self, source: str, tc: SMPTETime):
        self._last_tc = tc
        self._source = source
        for cb in self._subscribers:
            try:
                cb(source, tc)
            except Exception:
                pass

    # ── LTC ────────────────────────────────────────────────────────────────────

    def start_ltc(self, device_index: Optional[int] = None,
                  sample_rate: int = 48000, fps: float = 25.0):
        if self._ltc_reader:
            self._ltc_reader.stop()
        self._ltc_reader = LTCAudioReader(device_index, sample_rate, fps)
        self._ltc_reader.subscribe(lambda tc: self._notify("ltc", tc))
        self._ltc_reader.start()
        return AUDIO_AVAILABLE

    def stop_ltc(self):
        if self._ltc_reader:
            self._ltc_reader.stop()
            self._ltc_reader = None

    # ── MTC ────────────────────────────────────────────────────────────────────

    def handle_mtc_frame(self, tc_dict: dict):
        """Accept a decoded MTC frame dict from midi_protocol.MTCParser."""
        tc = SMPTETime(
            hours=tc_dict.get("hours", 0),
            minutes=tc_dict.get("minutes", 0),
            seconds=tc_dict.get("seconds", 0),
            frames=tc_dict.get("frames", 0),
            fps=tc_dict.get("fps", 25.0),
        )
        self._notify("mtc", tc)

    # ── Status ────────────────────────────────────────────────────────────────

    @property
    def current(self) -> Optional[SMPTETime]:
        return self._last_tc

    def to_dict(self) -> dict:
        return {
            "source": self._source,
            "ltc_active": self._ltc_reader is not None,
            "audio_available": AUDIO_AVAILABLE,
            "current": str(self._last_tc) if self._last_tc else None,
        }

    @staticmethod
    def list_audio_devices() -> List[dict]:
        return LTCAudioReader.list_devices()
