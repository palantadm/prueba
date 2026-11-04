"""RDM (Remote Device Management) — ANSI E1.20 over DMX512-A.

Supports:
  - Discovery: DISC_UNIQUE_BRANCH / DISC_MUTE / DISC_UN_MUTE binary tree walk
  - GET/SET parameter messages (PIDs)
  - Status messages, sensor data, DMX personality query
  - Works over Enttec Pro (label 0x02 = send RDM, label 0x04 = receive RDM)
    or any serial port that exposes the break generation needed for RDM
"""
import struct
import threading
import time
from dataclasses import dataclass, field
from typing import Callable, Dict, List, Optional, Tuple


# ── RDM Constants ──────────────────────────────────────────────────────────────

START_CODE_RDM    = 0xCC
SUB_START_CODE    = 0x01
BROADCAST_UID     = (0xFFFF, 0xFFFFFFFF)
MANUFACTURER_BROADCAST_MASK = 0xFFFFFFFF

# Command classes
CC_DISC_COMMAND          = 0x10
CC_DISC_COMMAND_RESPONSE = 0x11
CC_GET_COMMAND           = 0x20
CC_GET_COMMAND_RESPONSE  = 0x21
CC_SET_COMMAND           = 0x30
CC_SET_COMMAND_RESPONSE  = 0x31

# Response types
RESPONSE_TYPE_ACK           = 0x00
RESPONSE_TYPE_ACK_TIMER     = 0x01
RESPONSE_TYPE_NACK_REASON   = 0x02
RESPONSE_TYPE_ACK_OVERFLOW  = 0x03

# PIDs (subset of commonly used)
PID_DISC_UNIQUE_BRANCH      = 0x0001
PID_DISC_MUTE               = 0x0002
PID_DISC_UN_MUTE            = 0x0003
PID_PROXIED_DEVICES         = 0x0010
PID_COMMS_STATUS            = 0x0015
PID_STATUS_MESSAGES         = 0x0030
PID_SUPPORTED_PARAMETERS    = 0x0050
PID_PARAMETER_DESCRIPTION   = 0x0051
PID_DEVICE_INFO             = 0x0060
PID_PRODUCT_DETAIL_ID_LIST  = 0x0070
PID_DEVICE_MODEL_DESCRIPTION= 0x0080
PID_MANUFACTURER_LABEL      = 0x0081
PID_DEVICE_LABEL            = 0x0082
PID_FACTORY_DEFAULTS        = 0x0090
PID_SOFTWARE_VERSION_LABEL  = 0x00C0
PID_BOOT_SOFTWARE_VERSION   = 0x00C1
PID_DMX_PERSONALITY         = 0x00E0
PID_DMX_PERSONALITY_DESC    = 0x00E1
PID_DMX_START_ADDRESS       = 0x00F0
PID_SLOT_INFO               = 0x0120
PID_SLOT_DESCRIPTION        = 0x0121
PID_DEFAULT_SLOT_VALUE      = 0x0122
PID_SENSOR_DEFINITION       = 0x0200
PID_SENSOR_VALUE            = 0x0201
PID_RECORD_SENSORS          = 0x0202
PID_DEVICE_HOURS            = 0x0400
PID_LAMP_HOURS              = 0x0401
PID_LAMP_STRIKES            = 0x0402
PID_LAMP_STATE              = 0x0403
PID_LAMP_ON_MODE            = 0x0404
PID_DEVICE_POWER_CYCLES     = 0x0405
PID_DISPLAY_INVERT          = 0x0500
PID_DISPLAY_LEVEL           = 0x0501
PID_PAN_INVERT              = 0x0600
PID_TILT_INVERT             = 0x0601
PID_PAN_TILT_SWAP           = 0x0602
PID_REAL_TIME_CLOCK         = 0x0603
PID_IDENTIFY_DEVICE         = 0x1000
PID_RESET_DEVICE            = 0x1001
PID_POWER_STATE             = 0x1010
PID_PERFORM_SELFTEST        = 0x1020
PID_SELF_TEST_DESCRIPTION   = 0x1021
PID_CAPTURE_PRESET          = 0x1030
PID_PRESET_PLAYBACK         = 0x1031


# ── Data structures ────────────────────────────────────────────────────────────

@dataclass
class RDMDevice:
    """Discovered RDM device."""
    uid: Tuple[int, int]             # (manufacturer_id, device_id)
    label: str = ""
    manufacturer: str = ""
    model: str = ""
    dmx_address: int = 1
    dmx_personality: int = 1
    personality_count: int = 1
    footprint: int = 1
    software_version: str = ""
    muted: bool = False
    supported_pids: List[int] = field(default_factory=list)

    @property
    def uid_str(self) -> str:
        return f"{self.uid[0]:04X}:{self.uid[1]:08X}"

    def to_dict(self) -> dict:
        return {
            "uid": self.uid_str,
            "label": self.label,
            "manufacturer": self.manufacturer,
            "model": self.model,
            "dmx_address": self.dmx_address,
            "dmx_personality": self.dmx_personality,
            "personality_count": self.personality_count,
            "footprint": self.footprint,
            "software_version": self.software_version,
        }


# ── Packet builder / parser ────────────────────────────────────────────────────

CONTROLLER_UID = (0x7FF0, 0x00000001)  # Lumos controller UID


def _rdm_checksum(data: bytes) -> int:
    return sum(data) & 0xFFFF


def build_rdm_packet(
    dest_uid: Tuple[int, int],
    command_class: int,
    pid: int,
    transaction: int = 0,
    port_or_response: int = 1,
    sub_device: int = 0,
    parameter_data: bytes = b"",
) -> bytes:
    src_mfr, src_dev = CONTROLLER_UID
    dst_mfr, dst_dev = dest_uid
    msg_len = 24 + len(parameter_data)  # without checksum

    header = struct.pack(
        ">BB"          # START_CODE, SUB_START_CODE
        "B"            # message_length
        "HI"           # dest UID
        "HI"           # src UID
        "B"            # transaction_number
        "B"            # port_id / response_type
        "B"            # message_count
        "H"            # sub_device
        "B"            # command_class
        "H"            # pid
        "B",           # parameter_data_length
        START_CODE_RDM, SUB_START_CODE,
        msg_len,
        dst_mfr, dst_dev,
        src_mfr, src_dev,
        transaction & 0xFF,
        port_or_response,
        0,             # message_count (controller always 0)
        sub_device,
        command_class,
        pid,
        len(parameter_data),
    )
    body = header + parameter_data
    checksum = _rdm_checksum(body)
    return body + struct.pack(">H", checksum)


def parse_rdm_packet(data: bytes) -> Optional[dict]:
    if len(data) < 26:
        return None
    if data[0] != START_CODE_RDM or data[1] != SUB_START_CODE:
        return None
    msg_len = data[2]
    if len(data) < msg_len + 2:
        return None
    body = data[:msg_len]
    checksum = struct.unpack(">H", data[msg_len:msg_len+2])[0]
    if _rdm_checksum(body) != checksum:
        return None
    dst_mfr, dst_dev = struct.unpack(">HI", body[3:9])
    src_mfr, src_dev = struct.unpack(">HI", body[9:15])
    return {
        "dest_uid": (dst_mfr, dst_dev),
        "src_uid": (src_mfr, src_dev),
        "transaction": body[15],
        "response_type": body[16],
        "message_count": body[17],
        "sub_device": struct.unpack(">H", body[18:20])[0],
        "command_class": body[20],
        "pid": struct.unpack(">H", body[21:23])[0],
        "parameter_data": body[24:msg_len],
    }


# ── Discovery ──────────────────────────────────────────────────────────────────

def build_disc_unique_branch(lower: Tuple[int, int], upper: Tuple[int, int]) -> bytes:
    lo_mfr, lo_dev = lower
    hi_mfr, hi_dev = upper
    pd = struct.pack(">HIHI", lo_mfr, lo_dev, hi_mfr, hi_dev)
    return build_rdm_packet(
        dest_uid=BROADCAST_UID,
        command_class=CC_DISC_COMMAND,
        pid=PID_DISC_UNIQUE_BRANCH,
        parameter_data=pd,
    )


def build_disc_mute(dest_uid: Tuple[int, int]) -> bytes:
    return build_rdm_packet(
        dest_uid=dest_uid,
        command_class=CC_DISC_COMMAND,
        pid=PID_DISC_MUTE,
    )


def build_disc_un_mute() -> bytes:
    return build_rdm_packet(
        dest_uid=BROADCAST_UID,
        command_class=CC_DISC_COMMAND,
        pid=PID_DISC_UN_MUTE,
    )


def decode_disc_response(data: bytes) -> Optional[Tuple[int, int]]:
    """Decode a DISC_UNIQUE_BRANCH response (preamble + 24 bytes encoded UID)."""
    # Strip preamble (0xFE bytes) and separator (0xAA)
    idx = 0
    while idx < len(data) and data[idx] == 0xFE:
        idx += 1
    if idx >= len(data) or data[idx] != 0xAA:
        return None
    idx += 1
    # 12 encoded bytes follow (6 bytes, each repeated with mask)
    if len(data) - idx < 12:
        return None
    encoded = data[idx:idx+12]
    decoded = bytes(encoded[i*2] & encoded[i*2+1] for i in range(6))
    mfr = struct.unpack(">H", decoded[0:2])[0]
    dev = struct.unpack(">I", decoded[2:6])[0]
    return (mfr, dev)


# ── GET helpers ───────────────────────────────────────────────────────────────

def build_get(dest_uid: Tuple[int, int], pid: int,
              parameter_data: bytes = b"", transaction: int = 0) -> bytes:
    return build_rdm_packet(
        dest_uid=dest_uid,
        command_class=CC_GET_COMMAND,
        pid=pid,
        transaction=transaction,
        parameter_data=parameter_data,
    )


def build_set(dest_uid: Tuple[int, int], pid: int,
              parameter_data: bytes, transaction: int = 0) -> bytes:
    return build_rdm_packet(
        dest_uid=dest_uid,
        command_class=CC_SET_COMMAND,
        pid=pid,
        transaction=transaction,
        parameter_data=parameter_data,
    )


# ── RDM Controller ────────────────────────────────────────────────────────────

class RDMController:
    """High-level RDM controller using a SerialDMXManager port."""

    def __init__(self):
        self._serial = None
        self._port: Optional[str] = None
        self._devices: Dict[str, RDMDevice] = {}
        self._transaction = 0
        self._lock = threading.Lock()
        self._on_device_found: Optional[Callable[[RDMDevice], None]] = None

    def attach_serial(self, serial_mgr, port: str):
        self._serial = serial_mgr
        self._port = port

    def on_device_found(self, cb: Callable[[RDMDevice], None]):
        self._on_device_found = cb

    @property
    def devices(self) -> List[RDMDevice]:
        return list(self._devices.values())

    def _next_tx(self) -> int:
        self._transaction = (self._transaction + 1) & 0xFF
        return self._transaction

    def _send_and_receive(self, pkt: bytes, timeout: float = 0.05) -> Optional[bytes]:
        if not self._serial or not self._port:
            return None
        try:
            self._serial.send_rdm(self._port, pkt)
            time.sleep(timeout)
            return self._serial.read_rdm(self._port)
        except Exception:
            return None

    # ── Discovery ──────────────────────────────────────────────────────────────

    def discover(self) -> List[RDMDevice]:
        """Full binary-tree discovery walk. Returns list of found devices."""
        with self._lock:
            self._devices.clear()
            # Un-mute all first
            self._send_and_receive(build_disc_un_mute(), timeout=0.1)
            lower = (0x0000, 0x00000000)
            upper = (0xFFFF, 0xFFFFFFFF)
            self._discover_range(lower, upper)
        return self.devices

    def _discover_range(self, lower: Tuple[int, int], upper: Tuple[int, int],
                        depth: int = 0):
        if depth > 48:
            return
        pkt = build_disc_unique_branch(lower, upper)
        raw = self._send_and_receive(pkt, timeout=0.02)
        if not raw:
            return

        uid = decode_disc_response(raw)
        if uid is None:
            # Collision: split range
            lo_int = (lower[0] << 32) | lower[1]
            hi_int = (upper[0] << 32) | upper[1]
            if lo_int >= hi_int:
                return
            mid_int = (lo_int + hi_int) // 2
            mid_mfr = (mid_int >> 32) & 0xFFFF
            mid_dev = mid_int & 0xFFFFFFFF
            self._discover_range(lower, (mid_mfr, mid_dev), depth + 1)
            self._discover_range((mid_mfr, mid_dev + 1), upper, depth + 1)
            return

        # Found a single device — mute it and continue
        mute_pkt = build_disc_mute(uid)
        self._send_and_receive(mute_pkt, timeout=0.02)

        dev = RDMDevice(uid=uid)
        self._query_device(dev)
        uid_str = dev.uid_str
        self._devices[uid_str] = dev
        if self._on_device_found:
            self._on_device_found(dev)

        # Continue searching same range (there may be more)
        lo_int = (lower[0] << 32) | lower[1]
        hi_int = (upper[0] << 32) | upper[1]
        uid_int = (uid[0] << 32) | uid[1]
        if uid_int > lo_int:
            self._discover_range(lower, (uid[0], uid[1] - 1) if uid[1] > 0
                                 else (uid[0] - 1, 0xFFFFFFFF), depth + 1)
        if uid_int < hi_int:
            self._discover_range((uid[0], uid[1] + 1) if uid[1] < 0xFFFFFFFF
                                 else (uid[0] + 1, 0), upper, depth + 1)

    # ── Queries ────────────────────────────────────────────────────────────────

    def _query_device(self, dev: RDMDevice):
        uid = dev.uid
        tx = self._next_tx()

        # DEVICE_INFO
        raw = self._send_and_receive(build_get(uid, PID_DEVICE_INFO, transaction=tx))
        if raw:
            pkt = parse_rdm_packet(raw)
            if pkt and pkt["pid"] == PID_DEVICE_INFO and len(pkt["parameter_data"]) >= 19:
                pd = pkt["parameter_data"]
                dev.footprint = struct.unpack(">H", pd[8:10])[0]
                dev.dmx_personality = pd[10]
                dev.personality_count = pd[11]
                dev.dmx_address = struct.unpack(">H", pd[12:14])[0]

        # MANUFACTURER_LABEL
        raw = self._send_and_receive(build_get(uid, PID_MANUFACTURER_LABEL, transaction=self._next_tx()))
        if raw:
            pkt = parse_rdm_packet(raw)
            if pkt and pkt["pid"] == PID_MANUFACTURER_LABEL:
                dev.manufacturer = pkt["parameter_data"].decode("ascii", errors="replace")

        # DEVICE_MODEL_DESCRIPTION
        raw = self._send_and_receive(build_get(uid, PID_DEVICE_MODEL_DESCRIPTION, transaction=self._next_tx()))
        if raw:
            pkt = parse_rdm_packet(raw)
            if pkt and pkt["pid"] == PID_DEVICE_MODEL_DESCRIPTION:
                dev.model = pkt["parameter_data"].decode("ascii", errors="replace")

        # DEVICE_LABEL
        raw = self._send_and_receive(build_get(uid, PID_DEVICE_LABEL, transaction=self._next_tx()))
        if raw:
            pkt = parse_rdm_packet(raw)
            if pkt and pkt["pid"] == PID_DEVICE_LABEL:
                dev.label = pkt["parameter_data"].decode("ascii", errors="replace")

        # SOFTWARE_VERSION_LABEL
        raw = self._send_and_receive(build_get(uid, PID_SOFTWARE_VERSION_LABEL, transaction=self._next_tx()))
        if raw:
            pkt = parse_rdm_packet(raw)
            if pkt and pkt["pid"] == PID_SOFTWARE_VERSION_LABEL:
                dev.software_version = pkt["parameter_data"].decode("ascii", errors="replace")

        # SUPPORTED_PARAMETERS
        raw = self._send_and_receive(build_get(uid, PID_SUPPORTED_PARAMETERS, transaction=self._next_tx()))
        if raw:
            pkt = parse_rdm_packet(raw)
            if pkt and pkt["pid"] == PID_SUPPORTED_PARAMETERS:
                pd = pkt["parameter_data"]
                dev.supported_pids = list(struct.unpack(f">{len(pd)//2}H", pd))

    def get_dmx_address(self, uid: Tuple[int, int]) -> Optional[int]:
        raw = self._send_and_receive(build_get(uid, PID_DMX_START_ADDRESS, transaction=self._next_tx()))
        if raw:
            pkt = parse_rdm_packet(raw)
            if pkt and pkt["pid"] == PID_DMX_START_ADDRESS:
                return struct.unpack(">H", pkt["parameter_data"])[0]
        return None

    def set_dmx_address(self, uid: Tuple[int, int], address: int) -> bool:
        pd = struct.pack(">H", address)
        raw = self._send_and_receive(build_set(uid, PID_DMX_START_ADDRESS, pd, transaction=self._next_tx()))
        if raw:
            pkt = parse_rdm_packet(raw)
            return pkt is not None and pkt.get("response_type") == RESPONSE_TYPE_ACK
        return False

    def identify_device(self, uid: Tuple[int, int], on: bool = True) -> bool:
        pd = struct.pack("B", 1 if on else 0)
        raw = self._send_and_receive(build_set(uid, PID_IDENTIFY_DEVICE, pd, transaction=self._next_tx()))
        if raw:
            pkt = parse_rdm_packet(raw)
            return pkt is not None and pkt.get("response_type") == RESPONSE_TYPE_ACK
        return False

    def set_personality(self, uid: Tuple[int, int], personality: int) -> bool:
        pd = struct.pack("B", personality)
        raw = self._send_and_receive(build_set(uid, PID_DMX_PERSONALITY, pd, transaction=self._next_tx()))
        if raw:
            pkt = parse_rdm_packet(raw)
            return pkt is not None and pkt.get("response_type") == RESPONSE_TYPE_ACK
        return False

    def get_sensor_value(self, uid: Tuple[int, int], sensor: int) -> Optional[dict]:
        pd = struct.pack("B", sensor)
        raw = self._send_and_receive(build_get(uid, PID_SENSOR_VALUE, pd, transaction=self._next_tx()))
        if raw:
            pkt = parse_rdm_packet(raw)
            if pkt and pkt["pid"] == PID_SENSOR_VALUE and len(pkt["parameter_data"]) >= 9:
                d = pkt["parameter_data"]
                return {
                    "sensor": d[0],
                    "value": struct.unpack(">h", d[1:3])[0],
                    "lowest": struct.unpack(">h", d[3:5])[0],
                    "highest": struct.unpack(">h", d[5:7])[0],
                    "recorded": struct.unpack(">h", d[7:9])[0],
                }
        return None
