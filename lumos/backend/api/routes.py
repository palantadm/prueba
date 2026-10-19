"""FastAPI REST + WebSocket routes for Lumos."""
import asyncio
import json
from typing import Any, Dict, List, Optional
from fastapi import APIRouter, HTTPException, WebSocket, WebSocketDisconnect
from fastapi.responses import Response
from pydantic import BaseModel

from ..engine.fixture import FixtureInstance, FixtureLibrary, PatchManager
from ..engine.universe import UniverseManager, UniverseOutput
from ..engine.cue import CueManager, Cue, CueList
from ..engine.morph import MorphEngine, MorphPreview
from ..engine.effects import EffectsEngine, Waveform, EffectChannel
from ..engine.groups import GroupManager


router = APIRouter()

# Injected by main app
library: FixtureLibrary = None
patch: PatchManager = None
universes: UniverseManager = None
cues: CueManager = None
morph: MorphEngine = None
effects: EffectsEngine = None
groups: GroupManager = None
show_manager = None
midi_driver = None

_ws_clients: List[WebSocket] = []


async def broadcast(event: str, data: Any):
    msg = json.dumps({"event": event, "data": data})
    dead = []
    for ws in _ws_clients:
        try:
            await ws.send_text(msg)
        except Exception:
            dead.append(ws)
    for ws in dead:
        if ws in _ws_clients:
            _ws_clients.remove(ws)


# ─── WebSocket ───────────────────────────────────────────────────────────────

@router.websocket("/ws")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    _ws_clients.append(websocket)
    await websocket.send_text(json.dumps({
        "event": "init",
        "data": {
            "universes": universes.to_dict(),
            "fixtures": patch.to_dict(),
            "cue_lists": cues.to_dict(),
            "effects": effects.to_dict() if effects else [],
            "groups": groups.to_dict() if groups else {},
        }
    }))
    try:
        while True:
            raw = await websocket.receive_text()
            msg = json.loads(raw)
            await _handle_ws_message(msg)
    except WebSocketDisconnect:
        if websocket in _ws_clients:
            _ws_clients.remove(websocket)


async def _handle_ws_message(msg: dict):
    action = msg.get("action")
    data = msg.get("data", {})

    if action == "set_channel":
        cues.set_channel_direct(data["fixture_id"], data["channel"], data["value"])
        await broadcast("channel_update", data)

    elif action == "cue_go":
        cues.go_next(data["cue_list_id"])

    elif action == "cue_go_index":
        cl = cues.cue_lists.get(data["cue_list_id"])
        if cl:
            idx = data.get("index", 0)
            if 0 <= idx < len(cl.cues):
                cl.current_index = idx - 1
                cues.go_next(data["cue_list_id"])

    elif action == "set_grand_master" and groups:
        groups.grand_master = float(data.get("value", 1.0))
        await broadcast("grand_master", {"value": groups.grand_master})

    elif action == "blackout" and groups:
        groups.blackout = bool(data.get("active", False))
        await broadcast("blackout", {"active": groups.blackout})

    elif action == "set_submaster" and groups:
        groups.set_submaster(data["group_id"], float(data.get("level", 1.0)))
        await broadcast("submaster_update", data)


# ─── Universe ────────────────────────────────────────────────────────────────

class UniverseCreate(BaseModel):
    name: str = ""
    output_type: str = "none"
    artnet_ip: str = "255.255.255.255"
    artnet_net: int = 0
    artnet_subnet: int = 0
    artnet_universe: int = 0
    sacn_ip: str = "239.255.0.1"
    sacn_universe: int = 1
    sacn_priority: int = 100


class OutputAdd(BaseModel):
    type: str
    artnet_ip: str = "255.255.255.255"
    artnet_net: int = 0
    artnet_subnet: int = 0
    artnet_universe: int = 0
    sacn_universe: int = 1
    sacn_dest_ip: Optional[str] = None
    sacn_priority: int = 100
    kinet_ip: str = "255.255.255.255"
    kinet_port: int = 1
    kinet_version: int = 2
    serial_port: str = ""
    serial_mode: str = "auto"


@router.get("/universes")
def get_universes():
    return universes.to_dict()


@router.post("/universes/{uid}")
def create_universe(uid: int, body: UniverseCreate):
    if uid in universes.universes:
        raise HTTPException(400, "Universe already exists")
    u = universes.add_universe(uid, **body.model_dump())
    return u.to_dict()


@router.get("/universes/{uid}/dmx")
def get_universe_dmx(uid: int):
    u = universes.get_universe(uid)
    if not u:
        raise HTTPException(404, "Universe not found")
    return {"universe": uid, "data": list(u.get_all())}


@router.post("/universes/{uid}/dmx/{channel}")
def set_dmx_channel(uid: int, channel: int, value: int):
    u = universes.get_universe(uid)
    if not u:
        raise HTTPException(404)
    u.set_channel(channel - 1, value)
    return {"ok": True}


@router.post("/universes/{uid}/outputs")
async def add_universe_output(uid: int, body: OutputAdd):
    u = universes.get_universe(uid)
    if not u:
        raise HTTPException(404)
    out = UniverseOutput(**body.model_dump())
    universes.add_output(uid, out)
    await broadcast("universe_updated", universes.to_dict())
    return u.to_dict()


@router.delete("/universes/{uid}/outputs/{idx}")
async def remove_universe_output(uid: int, idx: int):
    u = universes.get_universe(uid)
    if not u:
        raise HTTPException(404)
    universes.remove_output(uid, idx)
    await broadcast("universe_updated", universes.to_dict())
    return {"ok": True}


# ─── Protocol status ─────────────────────────────────────────────────────────

@router.get("/protocols/artnet/nodes")
def artnet_nodes():
    drv = universes.artnet_driver
    if not drv:
        return []
    return drv.nodes

@router.get("/protocols/sacn/sources")
def sacn_sources():
    drv = universes.sacn_driver
    if not drv:
        return []
    return drv.sources

@router.get("/protocols/kinet/devices")
def kinet_devices():
    drv = universes.kinet_driver
    if not drv:
        return []
    return drv.devices

@router.get("/protocols/serial/ports")
def serial_ports():
    from ..protocols.serial_dmx import list_serial_ports
    return list_serial_ports()

@router.post("/protocols/serial/open")
def serial_open(port: str, mode: str = "auto"):
    mgr = universes.serial_manager
    if not mgr:
        from ..protocols.serial_dmx import SerialDMXManager
        universes._serial_mgr = SerialDMXManager()
        mgr = universes.serial_manager
    ok = mgr.open_device(port, mode)
    return {"ok": ok, "port": port}

@router.post("/protocols/artnet/poll")
async def artnet_poll():
    drv = universes.artnet_driver
    if not drv:
        raise HTTPException(503, "Art-Net driver not running")
    drv.send_poll()
    return {"ok": True}


# ─── Fixture Library ─────────────────────────────────────────────────────────

@router.get("/library")
def get_library(q: Optional[str] = None):
    if q:
        return library.search(q)
    return library.to_dict()


@router.get("/library/{manufacturer}/{model}")
def get_fixture_def(manufacturer: str, model: str):
    fdef = library.get(f"{manufacturer}::{model}")
    if not fdef:
        raise HTTPException(404)
    return {
        "id": fdef.id,
        "manufacturer": fdef.manufacturer,
        "model": fdef.model,
        "type": fdef.fixture_type,
        "channels": {
            name: {
                "name": ch.name,
                "group": ch.group,
                "default": ch.default,
                "capabilities": [
                    {"min": c.min_val, "max": c.max_val, "name": c.name, "color": c.color}
                    for c in ch.capabilities
                ],
            }
            for name, ch in fdef.channels.items()
        },
        "modes": [{"name": m.name, "channels": m.channels} for m in fdef.modes],
    }


# ─── Patch ───────────────────────────────────────────────────────────────────

class PatchFixtureRequest(BaseModel):
    definition_id: str
    name: str
    universe: int
    address: int
    mode: str = ""
    group: str = "Default"


@router.get("/patch")
def get_patch():
    return patch.to_dict()


@router.post("/patch")
async def add_fixture(body: PatchFixtureRequest):
    inst = patch.patch(
        definition_id=body.definition_id,
        name=body.name,
        universe=body.universe,
        address=body.address,
        mode=body.mode,
        group=body.group,
    )
    if not inst:
        raise HTTPException(400, f"Definition '{body.definition_id}' not found in library")
    channels = patch.get_channels(inst.id)
    conflicts = patch.check_conflicts(inst.universe, inst.address, len(channels), inst.id)
    result = {**patch.to_dict()[-1], "conflicts": conflicts}
    await broadcast("patch_updated", patch.to_dict())
    return result


@router.delete("/patch/{fixture_id}")
async def remove_fixture(fixture_id: str):
    if not patch.unpatch(fixture_id):
        raise HTTPException(404)
    await broadcast("patch_updated", patch.to_dict())
    return {"ok": True}


@router.patch("/patch/{fixture_id}")
async def update_fixture(fixture_id: str, body: dict):
    inst = patch.fixtures.get(fixture_id)
    if not inst:
        raise HTTPException(404)
    for k, v in body.items():
        if hasattr(inst, k) and k not in ("id",):
            setattr(inst, k, v)
    await broadcast("patch_updated", patch.to_dict())
    return {"ok": True}


# ─── Channels (direct control) ───────────────────────────────────────────────

@router.post("/fixture/{fixture_id}/channel/{channel}")
async def set_fixture_channel(fixture_id: str, channel: str, value: int):
    cues.set_channel_direct(fixture_id, channel, value)
    await broadcast("channel_update", {
        "fixture_id": fixture_id,
        "channel": channel,
        "value": value,
    })
    return {"ok": True}


# ─── Cues ────────────────────────────────────────────────────────────────────

class CueListCreate(BaseModel):
    name: str


class CueCreate(BaseModel):
    name: str
    fade_in: float = 0.0
    hold: float = 0.0
    fade_type: str = "linear"


class CueValueSet(BaseModel):
    fixture_id: str
    channel: str
    value: int


@router.get("/cuelists")
def get_cue_lists():
    return cues.to_dict()


@router.post("/cuelists")
def create_cue_list(body: CueListCreate):
    cl = cues.create_cue_list(body.name)
    return cl.to_dict()


@router.post("/cuelists/{cl_id}/cues")
def add_cue(cl_id: str, body: CueCreate):
    cl = cues.cue_lists.get(cl_id)
    if not cl:
        raise HTTPException(404)
    cue = cues.create_cue(body.name, body.fade_in, body.hold)
    cl.add_cue(cue)
    return cue.to_dict()


@router.post("/cuelists/{cl_id}/cues/{cue_id}/values")
def set_cue_value(cl_id: str, cue_id: str, body: CueValueSet):
    cl = cues.cue_lists.get(cl_id)
    if not cl:
        raise HTTPException(404)
    for cue in cl.cues:
        if cue.id == cue_id:
            cue.set_channel(body.fixture_id, body.channel, body.value)
            return {"ok": True}
    raise HTTPException(404)


@router.post("/cuelists/{cl_id}/go")
async def cue_go(cl_id: str):
    cues.go_next(cl_id)
    await broadcast("cue_go", {"cue_list_id": cl_id})
    return {"ok": True}


@router.post("/cuelists/{cl_id}/go/{index}")
async def cue_go_index(cl_id: str, index: int):
    cl = cues.cue_lists.get(cl_id)
    if not cl:
        raise HTTPException(404)
    if not 0 <= index < len(cl.cues):
        raise HTTPException(400, "Index out of range")
    cl.current_index = index - 1
    cues.go_next(cl_id)
    await broadcast("cue_go", {"cue_list_id": cl_id, "index": index})
    return {"ok": True}


# ─── Effects ─────────────────────────────────────────────────────────────────

class EffectCreate(BaseModel):
    name: str
    waveform: str = "sine"
    fixture_ids: List[str] = []
    channels: List[str] = []
    speed: float = 1.0
    amplitude: int = 127
    center: int = 127
    width: float = 0.5
    steps: List[int] = [0, 64, 128, 192, 255]
    phase_spread: float = 0.0
    bpm_sync: bool = False
    bpm_multiplier: float = 1.0
    one_shot: bool = False


class EffectUpdate(BaseModel):
    speed: Optional[float] = None
    amplitude: Optional[int] = None
    center: Optional[int] = None
    width: Optional[float] = None
    waveform: Optional[str] = None
    phase_spread: Optional[float] = None
    bpm_sync: Optional[bool] = None
    bpm_multiplier: Optional[float] = None
    enabled: Optional[bool] = None
    steps: Optional[List[int]] = None


@router.get("/effects")
def get_effects():
    if not effects:
        return []
    return effects.to_dict()


@router.post("/effects")
async def create_effect(body: EffectCreate):
    if not effects:
        raise HTTPException(503, "Effects engine not ready")
    eff = effects.create_effect(
        name=body.name,
        waveform=Waveform(body.waveform),
        fixture_ids=body.fixture_ids,
        channels=body.channels,
        speed=body.speed,
        amplitude=body.amplitude,
        center=body.center,
        width=body.width,
        steps=body.steps,
        phase_spread=body.phase_spread,
        bpm_sync=body.bpm_sync,
        bpm_multiplier=body.bpm_multiplier,
        one_shot=body.one_shot,
    )
    await broadcast("effect_created", eff.to_dict())
    return eff.to_dict()


@router.patch("/effects/{eff_id}")
async def update_effect(eff_id: str, body: EffectUpdate):
    if not effects:
        raise HTTPException(503)
    eff = effects.effects.get(eff_id)
    if not eff:
        raise HTTPException(404)
    data = body.model_dump(exclude_none=True)
    if "waveform" in data:
        data["waveform"] = Waveform(data["waveform"])
    for k, v in data.items():
        setattr(eff, k, v)
    await broadcast("effect_updated", eff.to_dict())
    return eff.to_dict()


@router.delete("/effects/{eff_id}")
async def delete_effect(eff_id: str):
    if not effects:
        raise HTTPException(503)
    if not effects.remove_effect(eff_id):
        raise HTTPException(404)
    await broadcast("effect_deleted", {"id": eff_id})
    return {"ok": True}


@router.post("/effects/{eff_id}/enable")
async def enable_effect(eff_id: str, enabled: bool = True):
    if not effects:
        raise HTTPException(503)
    eff = effects.effects.get(eff_id)
    if not eff:
        raise HTTPException(404)
    eff.enabled = enabled
    return {"ok": True, "enabled": enabled}


@router.post("/effects/bpm")
def set_effects_bpm(bpm: float):
    if effects:
        effects.set_bpm(bpm)
    return {"ok": True, "bpm": bpm}


# ─── Groups ──────────────────────────────────────────────────────────────────

class GroupCreate(BaseModel):
    name: str
    fixture_ids: List[str] = []
    color: str = "#4488ff"


class GroupFixtureUpdate(BaseModel):
    fixture_id: str


@router.get("/groups")
def get_groups():
    if not groups:
        return {}
    return groups.to_dict()


@router.post("/groups")
async def create_group(body: GroupCreate):
    if not groups:
        raise HTTPException(503)
    g = groups.create_group(body.name, body.fixture_ids, body.color)
    await broadcast("groups_updated", groups.to_dict())
    return g.to_dict()


@router.delete("/groups/{group_id}")
async def delete_group(group_id: str):
    if not groups or not groups.remove_group(group_id):
        raise HTTPException(404)
    await broadcast("groups_updated", groups.to_dict())
    return {"ok": True}


@router.post("/groups/{group_id}/fixtures")
async def add_fixture_to_group(group_id: str, body: GroupFixtureUpdate):
    if not groups:
        raise HTTPException(503)
    if not groups.add_fixture_to_group(group_id, body.fixture_id):
        raise HTTPException(404)
    await broadcast("groups_updated", groups.to_dict())
    return {"ok": True}


@router.delete("/groups/{group_id}/fixtures/{fixture_id}")
async def remove_fixture_from_group(group_id: str, fixture_id: str):
    if not groups:
        raise HTTPException(503)
    groups.remove_fixture_from_group(group_id, fixture_id)
    await broadcast("groups_updated", groups.to_dict())
    return {"ok": True}


@router.post("/groups/{group_id}/submaster")
async def set_submaster(group_id: str, level: float):
    if not groups:
        raise HTTPException(503)
    groups.set_submaster(group_id, level)
    await broadcast("submaster_update", {"group_id": group_id, "level": level})
    return {"ok": True}


@router.post("/groups/{group_id}/flash")
async def flash_submaster(group_id: str, active: bool = True):
    if not groups:
        raise HTTPException(503)
    groups.flash_submaster(group_id, active)
    return {"ok": True}


@router.post("/grandmaster")
async def set_grand_master(value: float):
    if groups:
        groups.grand_master = value
        await broadcast("grand_master", {"value": groups.grand_master})
    return {"ok": True, "value": value}


@router.post("/blackout")
async def set_blackout(active: bool):
    if groups:
        groups.blackout = active
        await broadcast("blackout", {"active": groups.blackout})
    return {"ok": True, "active": active}


# ─── MIDI ─────────────────────────────────────────────────────────────────────

class MIDIBindingCreate(BaseModel):
    midi_type: str
    midi_channel: int = 0
    number: int = -1
    action: str
    target: str
    channel: str = ""
    scale: float = 1.0


@router.get("/midi/ports")
def midi_ports():
    if not midi_driver:
        return {"in": [], "out": []}
    return midi_driver.refresh_ports()


@router.post("/midi/open_input")
def midi_open_input(port_name: str):
    if not midi_driver:
        raise HTTPException(503)
    ok = midi_driver.open_input(port_name)
    return {"ok": ok}


@router.post("/midi/open_output")
def midi_open_output(port_name: str):
    if not midi_driver:
        raise HTTPException(503)
    ok = midi_driver.open_output(port_name)
    return {"ok": ok}


@router.get("/midi/bindings")
def get_midi_bindings():
    if not midi_driver:
        return []
    return midi_driver.bindings_to_dict()


@router.post("/midi/bindings")
def add_midi_binding(body: MIDIBindingCreate):
    if not midi_driver:
        raise HTTPException(503)
    from ..protocols.midi_protocol import MIDIBinding
    b = MIDIBinding(**body.model_dump())
    midi_driver.add_binding(b)
    return {"ok": True, "count": len(midi_driver.bindings)}


@router.delete("/midi/bindings/{idx}")
def remove_midi_binding(idx: int):
    if not midi_driver:
        raise HTTPException(503)
    midi_driver.remove_binding(idx)
    return {"ok": True}


# ─── Morph ───────────────────────────────────────────────────────────────────

class MorphRequest(BaseModel):
    source_fixture_ids: List[str]
    target_definition_id: str
    target_mode: str = ""


@router.post("/morph/preview")
def morph_preview(body: MorphRequest):
    preview = morph.preview_morph(
        body.source_fixture_ids,
        body.target_definition_id,
        body.target_mode,
    )
    if not preview:
        raise HTTPException(400, "Could not compute morph preview")
    return {
        "source_fixture_ids": preview.source_fixture_ids,
        "target_definition_id": preview.target_definition_id,
        "target_mode": preview.target_mode,
        "coverage": preview.coverage,
        "warnings": preview.warnings,
        "unmapped_source": preview.unmapped_source,
        "unmapped_target": preview.unmapped_target,
        "channel_mappings": [
            {
                "source": m.source_channel,
                "target": m.target_channel,
                "source_group": m.source_group,
                "target_group": m.target_group,
                "confidence": round(m.confidence, 2),
            }
            for m in preview.channel_mappings
        ],
    }


@router.post("/morph/apply")
async def morph_apply(body: MorphRequest):
    preview = morph.preview_morph(
        body.source_fixture_ids,
        body.target_definition_id,
        body.target_mode,
    )
    if not preview:
        raise HTTPException(400, "Could not compute morph preview")
    result = morph.apply_morph(preview, cues)
    await broadcast("patch_updated", patch.to_dict())
    return {
        "original_ids": result.original_fixture_ids,
        "new_ids": result.new_fixture_ids,
        "cue_updates": result.cue_updates,
        "mappings_applied": len(result.channel_mappings),
    }


# ─── OSC Config ──────────────────────────────────────────────────────────────

osc_server_ref = None


class OSCTargetRequest(BaseModel):
    host: str
    port: int


@router.get("/osc/status")
def osc_status():
    if not osc_server_ref:
        return {"running": False}
    return {
        "running": osc_server_ref._running,
        "host": osc_server_ref.host,
        "port": osc_server_ref.port,
        "feedback_targets": list(osc_server_ref._feedback_targets),
    }


@router.post("/osc/feedback/add")
def osc_add_feedback(body: OSCTargetRequest):
    if osc_server_ref:
        osc_server_ref.add_feedback_target(body.host, body.port)
    return {"ok": True}


@router.post("/osc/feedback/remove")
def osc_remove_feedback(body: OSCTargetRequest):
    if osc_server_ref:
        osc_server_ref.remove_feedback_target(body.host, body.port)
    return {"ok": True}


# ─── Show ─────────────────────────────────────────────────────────────────────

class ShowSaveRequest(BaseModel):
    path: str


class ShowLoadRequest(BaseModel):
    path: str


@router.get("/show")
def show_state():
    if not show_manager:
        return {}
    return show_manager.to_dict()


@router.post("/show/save")
async def show_save(body: ShowSaveRequest):
    if not show_manager:
        raise HTTPException(503)
    ok = show_manager.save(body.path)
    if not ok:
        raise HTTPException(500, "Save failed")
    return {"ok": True, "path": body.path}


@router.post("/show/load")
async def show_load(body: ShowLoadRequest):
    if not show_manager:
        raise HTTPException(503)
    ok = show_manager.load(body.path)
    if not ok:
        raise HTTPException(500, "Load failed")
    await broadcast("show_loaded", {"path": body.path})
    await broadcast("patch_updated", patch.to_dict())
    return {"ok": True, "path": body.path}


# ── Timecode ──────────────────────────────────────────────────────────────────

timecode = None


@router.get("/timecode")
def timecode_status():
    if not timecode:
        return {"source": "none", "current": None}
    return timecode.to_dict()


@router.get("/timecode/devices")
def timecode_devices():
    from ..protocols.timecode import TimecodeManager
    return TimecodeManager.list_audio_devices()


class LTCStartRequest(BaseModel):
    device_index: Optional[int] = None
    sample_rate: int = 48000
    fps: float = 25.0


@router.post("/timecode/ltc/start")
def timecode_ltc_start(body: LTCStartRequest):
    if not timecode:
        raise HTTPException(503)
    ok = timecode.start_ltc(body.device_index, body.sample_rate, body.fps)
    return {"ok": ok, "audio_available": ok}


@router.post("/timecode/ltc/stop")
def timecode_ltc_stop():
    if not timecode:
        raise HTTPException(503)
    timecode.stop_ltc()
    return {"ok": True}


# ── RDM ───────────────────────────────────────────────────────────────────────

_rdm_controller = None


def _get_rdm():
    global _rdm_controller
    if _rdm_controller is None:
        from ..protocols.rdm import RDMController
        _rdm_controller = RDMController()
    return _rdm_controller


class RDMAttachRequest(BaseModel):
    port: str


class RDMSetAddressRequest(BaseModel):
    uid: str
    address: int


class RDMSetPersonalityRequest(BaseModel):
    uid: str
    personality: int


class RDMIdentifyRequest(BaseModel):
    uid: str
    on: bool = True


def _parse_uid(uid_str: str):
    parts = uid_str.split(":")
    if len(parts) != 2:
        raise HTTPException(400, "UID must be MFRID:DEVID (hex)")
    try:
        return (int(parts[0], 16), int(parts[1], 16))
    except ValueError:
        raise HTTPException(400, "Invalid UID hex")


@router.post("/rdm/attach")
def rdm_attach(body: RDMAttachRequest):
    rdm = _get_rdm()
    if not universes or not universes.serial_manager:
        raise HTTPException(503, "Serial manager not available")
    rdm.attach_serial(universes.serial_manager, body.port)
    return {"ok": True, "port": body.port}


@router.post("/rdm/discover")
def rdm_discover():
    import concurrent.futures
    rdm = _get_rdm()
    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as ex:
        future = ex.submit(rdm.discover)
        devices = future.result(timeout=30)
    return [d.to_dict() for d in devices]


@router.get("/rdm/devices")
def rdm_devices():
    rdm = _get_rdm()
    return [d.to_dict() for d in rdm.devices]


@router.post("/rdm/address")
def rdm_set_address(body: RDMSetAddressRequest):
    rdm = _get_rdm()
    uid = _parse_uid(body.uid)
    ok = rdm.set_dmx_address(uid, body.address)
    return {"ok": ok}


@router.post("/rdm/personality")
def rdm_set_personality(body: RDMSetPersonalityRequest):
    rdm = _get_rdm()
    uid = _parse_uid(body.uid)
    ok = rdm.set_personality(uid, body.personality)
    return {"ok": ok}


@router.post("/rdm/identify")
def rdm_identify(body: RDMIdentifyRequest):
    rdm = _get_rdm()
    uid = _parse_uid(body.uid)
    ok = rdm.identify_device(uid, body.on)
    return {"ok": ok}


# ── Technical Rider PDF ───────────────────────────────────────────────────────

class RiderRequest(BaseModel):
    show_name: str = "Show"
    venue: str = ""
    date: str = ""
    notes: str = ""
    page_size: str = "A4"   # A4 | Letter


@router.post("/show/rider")
async def generate_rider(body: RiderRequest):
    """Generate and download the technical rider as a PDF."""
    if not patch or not universes:
        raise HTTPException(503, "Engine not ready")
    try:
        from ..engine.rider import generate_rider_pdf, REPORTLAB_AVAILABLE
        if not REPORTLAB_AVAILABLE:
            raise HTTPException(503, "reportlab not installed: pip install reportlab")
        from reportlab.lib.pagesizes import A4, letter
        page_size = letter if body.page_size.lower() == "letter" else A4
        pdf_bytes = generate_rider_pdf(
            patch=patch,
            universes=universes,
            cues=cues,
            effects=effects,
            groups=groups,
            show_name=body.show_name,
            venue=body.venue,
            date_str=body.date,
            notes=body.notes,
            page_size=page_size,
        )
        safe_name = "".join(c if c.isalnum() or c in "-_ " else "_" for c in body.show_name).strip()
        filename = f"lumos_rider_{safe_name or 'show'}.pdf"
        return Response(
            content=pdf_bytes,
            media_type="application/pdf",
            headers={"Content-Disposition": f'attachment; filename="{filename}"'},
        )
    except ImportError as e:
        raise HTTPException(503, str(e))
    except Exception as e:
        raise HTTPException(500, f"PDF generation failed: {e}")
