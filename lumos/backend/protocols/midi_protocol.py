"""Full MIDI protocol implementation using mido.

Features:
  - Note On/Off → cue triggers, flash, bump
  - Control Change (CC) → channel/submaster control
  - NRPN (CC 98/99/6/38) → high-res fixture control
  - Program Change → cue list selection
  - SysEx → custom Lumos commands
  - MIDI Timecode (MTC) → full-frame + quarter-frame sync
  - MIDI Clock → BPM sync for effects engine
  - Output: send feedback, sync with controller surfaces (Behringer X-Touch, etc.)
"""
import asyncio
import threading
import time
from dataclasses import dataclass
from typing import Callable, Dict, List, Optional, Tuple

try:
    import mido
    MIDO_AVAILABLE = True
except ImportError:
    MIDO_AVAILABLE = False


# ── MTC timecode parser ──────────────────────────────────────────────────────

class MTCParser:
    """Parses MIDI Timecode (MTC) quarter-frame and full-frame messages."""

    RATES = [24, 25, 29.97, 30]

    def __init__(self, callback: Callable[[float, int], None]):
        """callback(timecode_seconds, framerate_index)"""
        self.callback = callback
        self._qf: List[int] = [0] * 8
        self._last_qf_idx = -1
        self._full_frame_pending = False
        self._direction = 1  # +1 forward, -1 reverse
        self._last_tc = 0.0
        self._last_time = time.monotonic()
        self._running = False

    def handle_quarter_frame(self, data_byte: int):
        piece = (data_byte >> 4) & 0x0F
        value = data_byte & 0x0F

        if piece == 7:
            # Check direction
            if self._last_qf_idx >= 0:
                self._direction = 1 if piece > self._last_qf_idx else -1
        self._last_qf_idx = piece
        self._qf[piece] = value

        # Full frame assembled after 8 quarter frames
        if piece == 7:
            frames  = self._qf[0] | ((self._qf[1] & 0x01) << 4)
            seconds = self._qf[2] | ((self._qf[3] & 0x03) << 4)
            minutes = self._qf[4] | ((self._qf[5] & 0x03) << 4)
            hours   = self._qf[6] | ((self._qf[7] & 0x01) << 4)
            rate_idx = (self._qf[7] >> 1) & 0x03
            fps = self.RATES[rate_idx]
            tc = hours * 3600 + minutes * 60 + seconds + frames / fps
            self._last_tc = tc
            self._last_time = time.monotonic()
            if self.callback:
                self.callback(tc, rate_idx)

    def handle_full_frame(self, data: bytes):
        if len(data) < 10:
            return
        # F0 7F 7F 01 01 hh mm ss ff F7
        if data[3] != 0x01 or data[4] != 0x01:
            return
        hours   = data[5] & 0x1F
        rate_idx = (data[5] >> 5) & 0x03
        minutes = data[6] & 0x3F
        seconds = data[7] & 0x3F
        frames  = data[8] & 0x1F
        fps = self.RATES[rate_idx]
        tc = hours * 3600 + minutes * 60 + seconds + frames / fps
        self._last_tc = tc
        if self.callback:
            self.callback(tc, rate_idx)

    @property
    def current_tc(self) -> float:
        """Extrapolated current timecode."""
        now = time.monotonic()
        elapsed = now - self._last_time
        return self._last_tc + elapsed * self._direction


# ── NRPN state machine ────────────────────────────────────────────────────────

class NRPNParser:
    """Parse NRPN (Non-Registered Parameter Number) messages."""

    def __init__(self, callback: Callable[[int, int, int], None]):
        """callback(channel, parameter, value)"""
        self.callback = callback
        self._nrpn_msb: Dict[int, int] = {}  # midi_channel -> MSB
        self._nrpn_lsb: Dict[int, int] = {}
        self._data_msb: Dict[int, int] = {}

    def handle_cc(self, midi_channel: int, cc: int, value: int):
        ch = midi_channel
        if cc == 99:
            self._nrpn_msb[ch] = value
        elif cc == 98:
            self._nrpn_lsb[ch] = value
        elif cc == 6:
            self._data_msb[ch] = value
        elif cc == 38:
            msb = self._nrpn_msb.get(ch, 0)
            lsb = self._nrpn_lsb.get(ch, 0)
            parameter = (msb << 7) | lsb
            hi = self._data_msb.get(ch, 0)
            lo = value
            val_14bit = (hi << 7) | lo
            if self.callback:
                self.callback(ch, parameter, val_14bit)


# ── MIDI Clock ────────────────────────────────────────────────────────────────

class MIDIClockTracker:
    """Track MIDI clock pulses to compute BPM."""

    PPQN = 24  # MIDI clock: 24 pulses per quarter note

    def __init__(self, callback: Callable[[float], None]):
        """callback(bpm)"""
        self.callback = callback
        self._pulse_times: List[float] = []
        self._max_history = 24

    def pulse(self):
        now = time.monotonic()
        self._pulse_times.append(now)
        if len(self._pulse_times) > self._max_history:
            self._pulse_times.pop(0)
        if len(self._pulse_times) >= 2:
            intervals = [
                self._pulse_times[i+1] - self._pulse_times[i]
                for i in range(len(self._pulse_times) - 1)
            ]
            avg = sum(intervals) / len(intervals)
            bpm = 60.0 / (avg * self.PPQN)
            if self.callback:
                self.callback(bpm)


# ── Binding map ───────────────────────────────────────────────────────────────

@dataclass
class MIDIBinding:
    """Maps a MIDI event to a Lumos action."""
    midi_type: str       # note_on, note_off, control_change, program_change
    midi_channel: int    # 1-16, 0=any
    number: int          # note/cc/program number, -1=any
    action: str          # cue_go, set_channel, submaster, flash, group_intensity, macro
    target: str          # cue_list_id, fixture_id, group_id, macro_id
    channel: str = ""    # fixture channel name (for set_channel)
    scale: float = 1.0   # value scaling factor


class MIDIDriver:
    """Full MIDI I/O driver."""

    def __init__(self):
        self.bindings: List[MIDIBinding] = []
        self.mtc = MTCParser(self._on_mtc)
        self.nrpn = NRPNParser(self._on_nrpn)
        self.clock = MIDIClockTracker(self._on_clock)

        self.on_timecode: Optional[Callable[[float, int], None]] = None
        self.on_bpm: Optional[Callable[[float], None]] = None
        self.on_action: Optional[Callable[[str, str, float], None]] = None  # (action, target, value)

        self._in_port = None
        self._out_port = None
        self._thread: Optional[threading.Thread] = None
        self._running = False
        self._available_ports: Dict[str, List[str]] = {"in": [], "out": []}

    def refresh_ports(self) -> Dict[str, List[str]]:
        if not MIDO_AVAILABLE:
            return {"in": [], "out": [], "error": "mido not installed"}
        try:
            self._available_ports = {
                "in": mido.get_input_names(),
                "out": mido.get_output_names(),
            }
        except Exception as e:
            self._available_ports = {"in": [], "out": [], "error": str(e)}
        return self._available_ports

    def open_input(self, port_name: str) -> bool:
        if not MIDO_AVAILABLE:
            return False
        try:
            if self._in_port:
                self._in_port.close()
            self._in_port = mido.open_input(port_name)
            self._running = True
            self._thread = threading.Thread(target=self._receive_loop, daemon=True)
            self._thread.start()
            return True
        except Exception:
            return False

    def open_output(self, port_name: str) -> bool:
        if not MIDO_AVAILABLE:
            return False
        try:
            if self._out_port:
                self._out_port.close()
            self._out_port = mido.open_output(port_name)
            return True
        except Exception:
            return False

    def close(self):
        self._running = False
        if self._in_port:
            try:
                self._in_port.close()
            except Exception:
                pass
        if self._out_port:
            try:
                self._out_port.close()
            except Exception:
                pass

    def _receive_loop(self):
        if not self._in_port:
            return
        for msg in self._in_port:
            if not self._running:
                break
            self._dispatch(msg)

    def _dispatch(self, msg):
        t = msg.type

        # MTC quarter frame
        if t == "quarter_frame":
            self.mtc.handle_quarter_frame(msg.frame_type << 4 | msg.frame_value)
            return

        # MIDI clock
        if t == "clock":
            self.clock.pulse()
            return

        # SysEx
        if t == "sysex":
            data = bytes(msg.data)
            # MTC full frame: F0 7F 7F 01 01 ...
            if data[:4] == b"\x7f\x7f\x01\x01":
                self.mtc.handle_full_frame(b"\xf0\x7f" + data + b"\xf7")
            return

        # CC — including NRPN
        if t == "control_change":
            ch = msg.channel + 1
            self.nrpn.handle_cc(ch, msg.control, msg.value)
            self._check_bindings("control_change", ch, msg.control, msg.value / 127.0)
            return

        # Note
        if t in ("note_on", "note_off"):
            ch = msg.channel + 1
            vel = msg.velocity / 127.0 if t == "note_on" else 0.0
            self._check_bindings(t, ch, msg.note, vel)
            # Also trigger note_off via note_on with velocity=0
            if t == "note_on" and msg.velocity == 0:
                self._check_bindings("note_off", ch, msg.note, 0.0)
            return

        # Program change
        if t == "program_change":
            ch = msg.channel + 1
            self._check_bindings("program_change", ch, msg.program, 1.0)

    def _check_bindings(self, midi_type: str, midi_ch: int, number: int, value: float):
        for b in self.bindings:
            if b.midi_type != midi_type:
                continue
            if b.midi_channel not in (0, midi_ch):
                continue
            if b.number not in (-1, number):
                continue
            scaled = value * b.scale
            if self.on_action:
                self.on_action(b.action, b.target, scaled)

    def _on_mtc(self, tc: float, rate_idx: int):
        if self.on_timecode:
            self.on_timecode(tc, rate_idx)

    def _on_nrpn(self, channel: int, parameter: int, value: int):
        # NRPN parameter → fixture channel (convention: param = fixture_index * 16 + ch_index)
        if self.on_action:
            self.on_action("nrpn", f"{channel}/{parameter}", value / 16383.0)

    def _on_clock(self, bpm: float):
        if self.on_bpm:
            self.on_bpm(bpm)

    # ── Sending ───────────────────────────────────────────────────────────────

    def send_note(self, channel: int, note: int, velocity: int = 127):
        if self._out_port and MIDO_AVAILABLE:
            try:
                msg = mido.Message("note_on", channel=channel-1, note=note, velocity=velocity)
                self._out_port.send(msg)
            except Exception:
                pass

    def send_cc(self, channel: int, control: int, value: int):
        if self._out_port and MIDO_AVAILABLE:
            try:
                msg = mido.Message("control_change", channel=channel-1, control=control, value=value)
                self._out_port.send(msg)
            except Exception:
                pass

    def send_feedback_note(self, channel: int, note: int, on: bool):
        self.send_note(channel, note, 127 if on else 0)

    def send_feedback_cc(self, channel: int, control: int, value: float):
        """Send CC feedback (0.0-1.0 → 0-127)."""
        self.send_cc(channel, control, int(value * 127))

    def add_binding(self, binding: MIDIBinding):
        self.bindings.append(binding)

    def remove_binding(self, idx: int):
        if 0 <= idx < len(self.bindings):
            self.bindings.pop(idx)

    def bindings_to_dict(self) -> List[dict]:
        return [
            {
                "midi_type": b.midi_type,
                "midi_channel": b.midi_channel,
                "number": b.number,
                "action": b.action,
                "target": b.target,
                "channel": b.channel,
                "scale": b.scale,
            }
            for b in self.bindings
        ]
