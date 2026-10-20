"""
Lumos 3D Stage Visualizer — Scene data model.
"""
from __future__ import annotations

import uuid
from dataclasses import dataclass, field
from typing import Optional


# ---------------------------------------------------------------------------
# Math primitives
# ---------------------------------------------------------------------------

@dataclass
class Vec3:
    x: float = 0.0
    y: float = 0.0
    z: float = 0.0

    def __iter__(self):
        return iter((self.x, self.y, self.z))

    def to_dict(self) -> dict:
        return {"x": self.x, "y": self.y, "z": self.z}

    @classmethod
    def from_dict(cls, d: dict) -> "Vec3":
        return cls(x=d.get("x", 0.0), y=d.get("y", 0.0), z=d.get("z", 0.0))


# ---------------------------------------------------------------------------
# Base scene object
# ---------------------------------------------------------------------------

@dataclass
class SceneObject:
    id: str = field(default_factory=lambda: str(uuid.uuid4()))
    name: str = "Object"
    pos: Vec3 = field(default_factory=Vec3)
    rot: Vec3 = field(default_factory=Vec3)
    scale: Vec3 = field(default_factory=lambda: Vec3(1.0, 1.0, 1.0))
    color: tuple = field(default_factory=lambda: (0.6, 0.6, 0.6))
    visible: bool = True

    def to_dict(self) -> dict:
        return {
            "__type__": type(self).__name__,
            "id": self.id,
            "name": self.name,
            "pos": self.pos.to_dict(),
            "rot": self.rot.to_dict(),
            "scale": self.scale.to_dict(),
            "color": list(self.color),
            "visible": self.visible,
        }

    @classmethod
    def from_dict(cls, d: dict) -> "SceneObject":
        obj = cls.__new__(cls)
        obj.id = d.get("id", str(uuid.uuid4()))
        obj.name = d.get("name", "Object")
        obj.pos = Vec3.from_dict(d.get("pos", {}))
        obj.rot = Vec3.from_dict(d.get("rot", {}))
        obj.scale = Vec3.from_dict(d.get("scale", {"x": 1, "y": 1, "z": 1}))
        c = d.get("color", [0.6, 0.6, 0.6])
        obj.color = tuple(c)
        obj.visible = d.get("visible", True)
        return obj


# ---------------------------------------------------------------------------
# Truss
# ---------------------------------------------------------------------------

@dataclass
class TrussObject(SceneObject):
    length: float = 3.0
    truss_type: str = "square"   # "square" | "triangular" | "ground"
    tube_radius: float = 0.025

    def to_dict(self) -> dict:
        d = super().to_dict()
        d.update({
            "length": self.length,
            "truss_type": self.truss_type,
            "tube_radius": self.tube_radius,
        })
        return d

    @classmethod
    def from_dict(cls, d: dict) -> "TrussObject":
        obj = super().from_dict(d)
        obj.length = d.get("length", 3.0)
        obj.truss_type = d.get("truss_type", "square")
        obj.tube_radius = d.get("tube_radius", 0.025)
        return obj


# ---------------------------------------------------------------------------
# Fixture
# ---------------------------------------------------------------------------

@dataclass
class FixtureObject(SceneObject):
    patch_id: int = -1
    beam_angle: float = 20.0
    beam_length: float = 8.0
    fixture_type: str = "spot"   # "spot" | "wash" | "par" | "strip"

    def to_dict(self) -> dict:
        d = super().to_dict()
        d.update({
            "patch_id": self.patch_id,
            "beam_angle": self.beam_angle,
            "beam_length": self.beam_length,
            "fixture_type": self.fixture_type,
        })
        return d

    @classmethod
    def from_dict(cls, d: dict) -> "FixtureObject":
        obj = super().from_dict(d)
        obj.patch_id = d.get("patch_id", -1)
        obj.beam_angle = d.get("beam_angle", 20.0)
        obj.beam_length = d.get("beam_length", 8.0)
        obj.fixture_type = d.get("fixture_type", "spot")
        return obj


# ---------------------------------------------------------------------------
# Stage element
# ---------------------------------------------------------------------------

@dataclass
class StageObject(SceneObject):
    shape: str = "box"   # "box" | "cylinder" | "plane"
    width: float = 1.0
    height: float = 1.0
    depth: float = 1.0

    def to_dict(self) -> dict:
        d = super().to_dict()
        d.update({
            "shape": self.shape,
            "width": self.width,
            "height": self.height,
            "depth": self.depth,
        })
        return d

    @classmethod
    def from_dict(cls, d: dict) -> "StageObject":
        obj = super().from_dict(d)
        obj.shape = d.get("shape", "box")
        obj.width = d.get("width", 1.0)
        obj.height = d.get("height", 1.0)
        obj.depth = d.get("depth", 1.0)
        return obj


# ---------------------------------------------------------------------------
# Projection / LED panel surface
# ---------------------------------------------------------------------------

@dataclass
class ProjectionObject(SceneObject):
    width: float = 2.0
    height: float = 1.5
    source_type: str = "color"   # "color" | "ndi" | "gradient"
    ndi_source: str = ""
    r: int = 255
    g: int = 255
    b: int = 255

    def to_dict(self) -> dict:
        d = super().to_dict()
        d.update({
            "width": self.width,
            "height": self.height,
            "source_type": self.source_type,
            "ndi_source": self.ndi_source,
            "r": self.r,
            "g": self.g,
            "b": self.b,
        })
        return d

    @classmethod
    def from_dict(cls, d: dict) -> "ProjectionObject":
        obj = super().from_dict(d)
        obj.width = d.get("width", 2.0)
        obj.height = d.get("height", 1.5)
        obj.source_type = d.get("source_type", "color")
        obj.ndi_source = d.get("ndi_source", "")
        obj.r = d.get("r", 255)
        obj.g = d.get("g", 255)
        obj.b = d.get("b", 255)
        return obj


# ---------------------------------------------------------------------------
# Type registry for deserialization
# ---------------------------------------------------------------------------

_TYPE_MAP = {
    "TrussObject": TrussObject,
    "FixtureObject": FixtureObject,
    "StageObject": StageObject,
    "ProjectionObject": ProjectionObject,
    "SceneObject": SceneObject,
}


def _obj_from_dict(d: dict) -> SceneObject:
    type_name = d.get("__type__", "SceneObject")
    cls = _TYPE_MAP.get(type_name, SceneObject)
    return cls.from_dict(d)


# ---------------------------------------------------------------------------
# Scene container
# ---------------------------------------------------------------------------

class VisualizerScene:
    def __init__(self) -> None:
        self.objects: list[SceneObject] = []
        self.ambient: float = 0.15
        self.floor_visible: bool = True
        self.grid_visible: bool = True

    def add(self, obj: SceneObject) -> None:
        self.objects.append(obj)

    def remove(self, obj_id: str) -> None:
        self.objects = [o for o in self.objects if o.id != obj_id]

    def get(self, obj_id: str) -> Optional[SceneObject]:
        for o in self.objects:
            if o.id == obj_id:
                return o
        return None

    def to_dict(self) -> dict:
        return {
            "ambient": self.ambient,
            "floor_visible": self.floor_visible,
            "grid_visible": self.grid_visible,
            "objects": [o.to_dict() for o in self.objects],
        }

    @classmethod
    def from_dict(cls, d: dict) -> "VisualizerScene":
        scene = cls()
        scene.ambient = d.get("ambient", 0.15)
        scene.floor_visible = d.get("floor_visible", True)
        scene.grid_visible = d.get("grid_visible", True)
        for od in d.get("objects", []):
            scene.objects.append(_obj_from_dict(od))
        return scene

    def default_stage(self) -> None:
        """Populate with a typical small-venue stage layout."""
        self.objects.clear()

        # Floor plane (12m x 8m)
        floor = StageObject(
            name="Stage Floor",
            pos=Vec3(0, 0, 0),
            color=(0.22, 0.18, 0.15),
            shape="plane",
            width=12.0,
            height=0.05,
            depth=8.0,
        )
        self.add(floor)

        # Upstage back wall
        back_wall = StageObject(
            name="Back Wall",
            pos=Vec3(0, 2.5, -4.0),
            color=(0.1, 0.1, 0.12),
            shape="box",
            width=12.0,
            height=5.0,
            depth=0.2,
        )
        self.add(back_wall)

        # Front truss (6m span, 5m high, across upstage)
        front_truss = TrussObject(
            name="Front Truss",
            pos=Vec3(0, 5.0, 2.5),
            rot=Vec3(0, 90, 0),
            color=(0.4, 0.4, 0.4),
            length=8.0,
            truss_type="square",
        )
        self.add(front_truss)

        # Back truss
        back_truss = TrussObject(
            name="Back Truss",
            pos=Vec3(0, 5.0, -2.5),
            rot=Vec3(0, 90, 0),
            color=(0.4, 0.4, 0.4),
            length=8.0,
            truss_type="square",
        )
        self.add(back_truss)

        # Left side truss
        left_truss = TrussObject(
            name="Left Side Truss",
            pos=Vec3(-4.0, 5.0, 0),
            rot=Vec3(0, 0, 0),
            color=(0.4, 0.4, 0.4),
            length=6.0,
            truss_type="square",
        )
        self.add(left_truss)

        # Right side truss
        right_truss = TrussObject(
            name="Right Side Truss",
            pos=Vec3(4.0, 5.0, 0),
            rot=Vec3(0, 0, 0),
            color=(0.4, 0.4, 0.4),
            length=6.0,
            truss_type="square",
        )
        self.add(right_truss)

        # PAR fixtures on front truss
        fixture_positions = [-3.0, -1.5, 0.0, 1.5, 3.0]
        for i, xp in enumerate(fixture_positions):
            fx = FixtureObject(
                name=f"Front PAR {i+1}",
                pos=Vec3(xp, 5.0, 2.5),
                rot=Vec3(-45, 0, 0),
                color=(1.0, 0.8, 0.2),
                beam_angle=25.0,
                beam_length=7.0,
                fixture_type="par",
            )
            self.add(fx)

        # Wash fixtures on back truss
        for i, xp in enumerate([-2.0, 0.0, 2.0]):
            fx = FixtureObject(
                name=f"Back Wash {i+1}",
                pos=Vec3(xp, 5.0, -2.5),
                rot=Vec3(-30, 0, 0),
                color=(0.2, 0.5, 1.0),
                beam_angle=40.0,
                beam_length=6.0,
                fixture_type="wash",
            )
            self.add(fx)

        # LED projection surface (upstage screen)
        screen = ProjectionObject(
            name="Upstage Screen",
            pos=Vec3(0, 2.5, -3.8),
            color=(0.9, 0.9, 0.9),
            width=6.0,
            height=3.0,
            source_type="color",
            r=30, g=30, b=60,
        )
        self.add(screen)
