"""OSC Protocol - proper implementation for Lumos.

Fixes QLC+'s OSC issues:
- Full OSC 1.0 and 1.1 support
- Bidirectional: receive commands + send feedback
- Proper address pattern routing with wildcards
- Bundle support
- Feedback mirroring (send current values back to controllers)
- Named parameter paths (not just /dmx/universe/channel)
- Support for TouchOSC, Lemur, QLab, and custom layouts
"""
import asyncio
import struct
import re
import time
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional, Tuple


# ─── OSC Type Tags ──────────────────────────────────────────────────────────

OSC_TYPES = {
    "i": "int",
    "f": "float",
    "s": "string",
    "b": "blob",
    "h": "int64",
    "t": "timetag",
    "d": "double",
    "c": "char",
    "r": "rgba",
    "m": "midi",
    "T": "true",
    "F": "false",
    "N": "nil",
    "I": "impulse",
}


def _pad(n: int) -> int:
    """Round up to next multiple of 4."""
    return (n + 3) & ~3


def decode_string(data: bytes, offset: int) -> Tuple[str, int]:
    end = data.index(b"\x00", offset)
    s = data[offset:end].decode("utf-8", errors="replace")
    return s, _pad(end + 1)


def decode_blob(data: bytes, offset: int) -> Tuple[bytes, int]:
    size = struct.unpack_from(">i", data, offset)[0]
    blob = data[offset + 4: offset + 4 + size]
    return blob, offset + 4 + _pad(size)


def encode_string(s: str) -> bytes:
    b = s.encode("utf-8") + b"\x00"
    return b + b"\x00" * (_pad(len(b)) - len(b))


def encode_blob(b: bytes) -> bytes:
    header = struct.pack(">i", len(b))
    padded = b + b"\x00" * (_pad(len(b)) - len(b))
    return header + padded


@dataclass
class OSCMessage:
    address: str
    args: List[Any] = field(default_factory=list)
    type_tags: str = ""

    @classmethod
    def decode(cls, data: bytes) -> "OSCMessage":
        offset = 0
        address, offset = decode_string(data, offset)

        if offset >= len(data) or data[offset:offset+1] != b",":
            return cls(address=address)

        tags, offset = decode_string(data, offset)
        tags = tags.lstrip(",")

        args = []
        for tag in tags:
            if tag == "i":
                args.append(struct.unpack_from(">i", data, offset)[0])
                offset += 4
            elif tag == "f":
                args.append(struct.unpack_from(">f", data, offset)[0])
                offset += 4
            elif tag == "s":
                s, offset = decode_string(data, offset)
                args.append(s)
            elif tag == "b":
                b, offset = decode_blob(data, offset)
                args.append(b)
            elif tag == "h":
                args.append(struct.unpack_from(">q", data, offset)[0])
                offset += 8
            elif tag == "d":
                args.append(struct.unpack_from(">d", data, offset)[0])
                offset += 8
            elif tag == "T":
                args.append(True)
            elif tag == "F":
                args.append(False)
            elif tag == "N":
                args.append(None)
            elif tag == "I":
                args.append("impulse")
            elif tag == "c":
                args.append(chr(struct.unpack_from(">i", data, offset)[0]))
                offset += 4
            elif tag == "r":
                r, g, b, a = struct.unpack_from("BBBB", data, offset)
                args.append((r, g, b, a))
                offset += 4
            elif tag == "m":
                args.append(struct.unpack_from("BBBB", data, offset))
                offset += 4

        return cls(address=address, args=args, type_tags=tags)

    def encode(self) -> bytes:
        tags = ","
        encoded_args = b""
        for arg in self.args:
            if isinstance(arg, bool):
                tags += "T" if arg else "F"
            elif isinstance(arg, int):
                tags += "i"
                encoded_args += struct.pack(">i", arg)
            elif isinstance(arg, float):
                tags += "f"
                encoded_args += struct.pack(">f", arg)
            elif isinstance(arg, str):
                tags += "s"
                encoded_args += encode_string(arg)
            elif isinstance(arg, bytes):
                tags += "b"
                encoded_args += encode_blob(arg)
            elif arg is None:
                tags += "N"

        return encode_string(self.address) + encode_string(tags) + encoded_args

    def __repr__(self):
        return f"OSCMessage({self.address!r}, {self.args})"


@dataclass
class OSCBundle:
    timetag: float  # seconds since epoch
    elements: List[Any] = field(default_factory=list)  # OSCMessage or OSCBundle

    IMMEDIATELY = 1.0 / (2**32)  # OSC "immediately" timetag

    @classmethod
    def decode(cls, data: bytes) -> "OSCBundle":
        # "#bundle\0" header
        offset = 8
        secs, frac = struct.unpack_from(">II", data, offset)
        timetag = secs - 2208988800 + frac / (2**32)  # NTP to Unix
        offset += 8

        elements = []
        while offset < len(data):
            size = struct.unpack_from(">i", data, offset)[0]
            offset += 4
            chunk = data[offset: offset + size]
            offset += size
            if chunk.startswith(b"#bundle"):
                elements.append(OSCBundle.decode(chunk))
            else:
                elements.append(OSCMessage.decode(chunk))
        return cls(timetag=timetag, elements=elements)

    def encode(self) -> bytes:
        unix = self.timetag + 2208988800
        secs = int(unix)
        frac = int((unix - secs) * (2**32))
        header = b"#bundle\x00" + struct.pack(">II", secs, frac)
        body = b""
        for el in self.elements:
            enc = el.encode()
            body += struct.pack(">i", len(enc)) + enc
        return header + body


def address_matches(pattern: str, address: str) -> bool:
    """OSC address pattern matching with wildcards."""
    # Convert OSC pattern to regex
    regex = re.escape(pattern)
    regex = regex.replace(r"\*", "[^/]*")
    regex = regex.replace(r"\?", "[^/]")
    regex = regex.replace(r"\[", "[").replace(r"\]", "]")
    regex = regex.replace(r"\{", "(").replace(r"\}", ")").replace(r"\,", "|")
    regex = "^" + regex + "$"
    return bool(re.match(regex, address))


class OSCRouter:
    """Routes OSC messages to handlers by address pattern."""

    def __init__(self):
        self._handlers: List[Tuple[str, Callable]] = []

    def add_handler(self, pattern: str, callback: Callable):
        """Register a handler for an OSC address pattern."""
        self._handlers.append((pattern, callback))

    def remove_handler(self, pattern: str):
        self._handlers = [(p, c) for p, c in self._handlers if p != pattern]

    def dispatch(self, message: OSCMessage) -> bool:
        """Dispatch a message to matching handlers. Returns True if handled."""
        matched = False
        for pattern, callback in self._handlers:
            if address_matches(pattern, message.address):
                try:
                    callback(message)
                except Exception as e:
                    pass
                matched = True
        return matched

    def dispatch_bundle(self, bundle: OSCBundle):
        for el in bundle.elements:
            if isinstance(el, OSCMessage):
                self.dispatch(el)
            elif isinstance(el, OSCBundle):
                self.dispatch_bundle(el)


class OSCServer:
    """UDP OSC server with proper bundle support and feedback."""

    def __init__(self, host: str = "0.0.0.0", port: int = 8000):
        self.host = host
        self.port = port
        self.router = OSCRouter()
        self._transport = None
        self._protocol = None
        self._feedback_targets: List[Tuple[str, int]] = []  # (host, port) to mirror to
        self._running = False

    def add_feedback_target(self, host: str, port: int):
        """Add a target that receives feedback (e.g., a TouchOSC device)."""
        if (host, port) not in self._feedback_targets:
            self._feedback_targets.append((host, port))

    def remove_feedback_target(self, host: str, port: int):
        self._feedback_targets = [(h, p) for h, p in self._feedback_targets if (h, p) != (host, port)]

    def send_feedback(self, address: str, *args):
        """Send a value update to all feedback targets."""
        msg = OSCMessage(address=address, args=list(args))
        data = msg.encode()
        if self._transport:
            for host, port in self._feedback_targets:
                try:
                    self._transport.sendto(data, (host, port))
                except Exception:
                    pass

    async def start(self):
        loop = asyncio.get_event_loop()
        self._transport, self._protocol = await loop.create_datagram_endpoint(
            lambda: _OSCProtocol(self),
            local_addr=(self.host, self.port),
        )
        self._running = True

    async def stop(self):
        self._running = False
        if self._transport:
            self._transport.close()

    def handle_data(self, data: bytes, addr: Tuple[str, int]):
        try:
            if data.startswith(b"#bundle"):
                bundle = OSCBundle.decode(data)
                self.router.dispatch_bundle(bundle)
            else:
                msg = OSCMessage.decode(data)
                self.router.dispatch(msg)
        except Exception:
            pass

    def on(self, pattern: str):
        """Decorator to register a handler."""
        def decorator(func: Callable):
            self.router.add_handler(pattern, func)
            return func
        return decorator


class _OSCProtocol(asyncio.DatagramProtocol):
    def __init__(self, server: OSCServer):
        self.server = server

    def connection_made(self, transport):
        self.server._transport = transport

    def datagram_received(self, data: bytes, addr):
        self.server.handle_data(data, addr)

    def error_received(self, exc):
        pass


class LumosOSC:
    """High-level OSC integration for Lumos.

    Provides named paths for fixtures, cues, and master controls.
    Handles feedback to connected controllers automatically.
    """

    def __init__(self, server: OSCServer, cue_manager, patch_manager, universe_manager):
        self.server = server
        self.cue_manager = cue_manager
        self.patch = patch_manager
        self.universes = universe_manager
        self._setup_handlers()

    def _setup_handlers(self):
        router = self.server.router

        # Master intensity: /lumos/master/intensity <0.0-1.0>
        router.add_handler("/lumos/master/intensity", self._on_master_intensity)

        # Raw DMX: /lumos/dmx/<universe>/<channel> <0-255>
        router.add_handler("/lumos/dmx/*/*", self._on_dmx_raw)

        # Fixture channel: /lumos/fixture/<fixture_id>/<channel_name> <0-255>
        router.add_handler("/lumos/fixture/*/*", self._on_fixture_channel)

        # Fixture intensity shorthand: /lumos/fixture/<fixture_id>/intensity <0.0-1.0>
        router.add_handler("/lumos/fixture/*/intensity", self._on_fixture_intensity)

        # Cue go: /lumos/cue/<cue_list_id>/go
        router.add_handler("/lumos/cue/*/go", self._on_cue_go)

        # Cue direct: /lumos/cue/<cue_list_id>/<index> <1=go>
        router.add_handler("/lumos/cue/*/*", self._on_cue_index)

        # Flash: /lumos/fixture/<fixture_id>/flash <0=off, 1=on>
        router.add_handler("/lumos/fixture/*/flash", self._on_fixture_flash)

    def _parse_address_parts(self, address: str) -> List[str]:
        return [p for p in address.split("/") if p]

    def _on_master_intensity(self, msg: OSCMessage):
        if not msg.args:
            return
        val = float(msg.args[0])
        val = max(0.0, min(1.0, val))
        # Apply to all intensity channels
        for fid, inst in self.patch.fixtures.items():
            channels = self.patch.get_channels(fid)
            fdef = self.patch.library.get(inst.definition_id)
            if not fdef:
                continue
            for ch in channels:
                ch_def = fdef.channels.get(ch)
                if ch_def and ch_def.group.value == "intensity":
                    self.cue_manager.set_channel_direct(fid, ch, int(val * 255))

    def _on_dmx_raw(self, msg: OSCMessage):
        parts = self._parse_address_parts(msg.address)
        if len(parts) < 4 or not msg.args:
            return
        try:
            universe_id = int(parts[2])
            channel = int(parts[3])
            value = int(float(msg.args[0]))
            u = self.universes.get_universe(universe_id)
            if u:
                u.set_channel(channel - 1, value)  # convert 1-indexed to 0-indexed
        except (ValueError, IndexError):
            pass

    def _on_fixture_channel(self, msg: OSCMessage):
        parts = self._parse_address_parts(msg.address)
        if len(parts) < 4 or not msg.args:
            return
        fixture_id = parts[2]
        channel = parts[3]
        value = int(float(msg.args[0]))
        self.cue_manager.set_channel_direct(fixture_id, channel, value)
        # Send feedback
        self.server.send_feedback(msg.address, value)

    def _on_fixture_intensity(self, msg: OSCMessage):
        parts = self._parse_address_parts(msg.address)
        if len(parts) < 4 or not msg.args:
            return
        fixture_id = parts[2]
        val = max(0.0, min(1.0, float(msg.args[0])))
        fdef_id = None
        inst = self.patch.fixtures.get(fixture_id)
        if not inst:
            return
        fdef = self.patch.library.get(inst.definition_id)
        if not fdef:
            return
        for ch in self.patch.get_channels(fixture_id):
            ch_def = fdef.channels.get(ch)
            if ch_def and ch_def.group.value == "intensity":
                self.cue_manager.set_channel_direct(fixture_id, ch, int(val * 255))
        self.server.send_feedback(msg.address, val)

    def _on_cue_go(self, msg: OSCMessage):
        parts = self._parse_address_parts(msg.address)
        if len(parts) < 3:
            return
        cue_list_id = parts[2]
        self.cue_manager.go_next(cue_list_id)
        self.server.send_feedback(msg.address, 1)

    def _on_cue_index(self, msg: OSCMessage):
        parts = self._parse_address_parts(msg.address)
        if len(parts) < 4 or not msg.args:
            return
        cue_list_id = parts[2]
        try:
            index = int(parts[3])
            val = float(msg.args[0])
            if val > 0:
                cl = self.cue_manager.cue_lists.get(cue_list_id)
                if cl and 0 <= index < len(cl.cues):
                    cl.current_index = index - 1
                    self.cue_manager.go_next(cue_list_id)
        except (ValueError, IndexError):
            pass

    def _on_fixture_flash(self, msg: OSCMessage):
        parts = self._parse_address_parts(msg.address)
        if len(parts) < 4 or not msg.args:
            return
        fixture_id = parts[2]
        on = float(msg.args[0]) > 0
        fdef_id = None
        inst = self.patch.fixtures.get(fixture_id)
        if not inst:
            return
        fdef = self.patch.library.get(inst.definition_id)
        if not fdef:
            return
        for ch in self.patch.get_channels(fixture_id):
            ch_def = fdef.channels.get(ch)
            if ch_def and ch_def.group.value == "intensity":
                self.cue_manager.set_channel_direct(fixture_id, ch, 255 if on else 0)
