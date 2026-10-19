"""FastAPI REST + WebSocket routes for Lumos."""
import asyncio
import json
from typing import Any, Dict, List, Optional
from fastapi import APIRouter, HTTPException, WebSocket, WebSocketDisconnect
from pydantic import BaseModel

from ..engine.fixture import FixtureInstance, FixtureLibrary, PatchManager
from ..engine.universe import UniverseManager
from ..engine.cue import CueManager, Cue, CueList
from ..engine.morph import MorphEngine, MorphPreview


router = APIRouter()

# Will be injected by main app
library: FixtureLibrary = None
patch: PatchManager = None
universes: UniverseManager = None
cues: CueManager = None
morph: MorphEngine = None

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
        _ws_clients.remove(ws)


# ─── WebSocket ───────────────────────────────────────────────────────────────

@router.websocket("/ws")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    _ws_clients.append(websocket)
    # Send initial state
    await websocket.send_text(json.dumps({
        "event": "init",
        "data": {
            "universes": [
                {"id": u.id, "name": u.name, "output_type": u.output_type}
                for u in universes.universes.values()
            ],
            "fixtures": patch.to_dict(),
            "cue_lists": cues.to_dict(),
        }
    }))
    try:
        while True:
            raw = await websocket.receive_text()
            msg = json.loads(raw)
            await _handle_ws_message(msg)
    except WebSocketDisconnect:
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


@router.get("/universes")
def get_universes():
    return [
        {
            "id": u.id,
            "name": u.name,
            "output_type": u.output_type,
            "artnet_ip": u.artnet_ip,
            "sacn_ip": u.sacn_ip,
            "sacn_universe": u.sacn_universe,
        }
        for u in universes.universes.values()
    ]


@router.post("/universes/{uid}")
def create_universe(uid: int, body: UniverseCreate):
    if uid in universes.universes:
        raise HTTPException(400, "Universe already exists")
    u = universes.add_universe(uid, **body.model_dump())
    return {"id": u.id, "name": u.name}


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
        "feedback_targets": osc_server_ref._feedback_targets,
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
