"""
Lumos 3D Stage Visualizer — OpenGL render widget.
Falls back gracefully to a plain message if PyOpenGL is not installed.
"""
from __future__ import annotations

import math
from typing import Optional, TYPE_CHECKING

from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QPainter, QColor, QFont
from PySide6.QtOpenGLWidgets import QOpenGLWidget

try:
    import OpenGL.GL as GL
    import OpenGL.GLU as GLU
    _HAS_OPENGL = True
except ImportError:
    _HAS_OPENGL = False

from app.visualizer.scene import (
    VisualizerScene, SceneObject, TrussObject, FixtureObject,
    StageObject, ProjectionObject,
)

if TYPE_CHECKING:
    from app.engine import LumosEngine


class VisualizerGLWidget(QOpenGLWidget):
    """OpenGL 3D stage visualizer widget."""

    object_selected = Signal(str)  # emits object id

    def __init__(
        self,
        scene: VisualizerScene,
        engine: "LumosEngine",
        parent=None,
    ) -> None:
        super().__init__(parent)
        self.scene = scene
        self.engine = engine

        # Camera state
        self._cam_yaw: float = 30.0
        self._cam_pitch: float = -25.0
        self._cam_dist: float = 12.0
        self._cam_target: list[float] = [0.0, 1.0, 0.0]

        self._selected_id: str = ""
        self._edit_mode: bool = False
        self._last_mouse: Optional[tuple[int, int]] = None

        self.setMinimumSize(400, 300)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)

        # NDI receiver registry: obj_id → NDIReceiver
        self._ndi_receivers: dict[str, object] = {}

    # ------------------------------------------------------------------
    # GL lifecycle
    # ------------------------------------------------------------------

    def initializeGL(self) -> None:
        if not _HAS_OPENGL:
            return
        GL.glClearColor(0.05, 0.05, 0.07, 1.0)
        GL.glEnable(GL.GL_DEPTH_TEST)
        GL.glEnable(GL.GL_BLEND)
        GL.glBlendFunc(GL.GL_SRC_ALPHA, GL.GL_ONE_MINUS_SRC_ALPHA)
        GL.glEnable(GL.GL_LIGHTING)
        GL.glEnable(GL.GL_LIGHT0)
        GL.glLightfv(GL.GL_LIGHT0, GL.GL_POSITION, [0.0, 10.0, 5.0, 1.0])
        GL.glLightfv(GL.GL_LIGHT0, GL.GL_DIFFUSE, [0.6, 0.6, 0.6, 1.0])
        GL.glLightfv(GL.GL_LIGHT0, GL.GL_AMBIENT, [0.2, 0.2, 0.2, 1.0])
        GL.glShadeModel(GL.GL_SMOOTH)
        GL.glColorMaterial(GL.GL_FRONT_AND_BACK, GL.GL_AMBIENT_AND_DIFFUSE)
        GL.glEnable(GL.GL_COLOR_MATERIAL)

    def resizeGL(self, w: int, h: int) -> None:
        if not _HAS_OPENGL:
            return
        GL.glViewport(0, 0, w, max(h, 1))
        GL.glMatrixMode(GL.GL_PROJECTION)
        GL.glLoadIdentity()
        GLU.gluPerspective(45.0, w / max(h, 1), 0.1, 200.0)
        GL.glMatrixMode(GL.GL_MODELVIEW)

    def paintGL(self) -> None:
        if not _HAS_OPENGL:
            p = QPainter(self)
            p.fillRect(self.rect(), QColor("#0c0c0c"))
            p.setPen(QColor("#606060"))
            p.setFont(QFont("Segoe UI", 12))
            p.drawText(
                self.rect(),
                Qt.AlignmentFlag.AlignCenter,
                "3D Visualizer requires PyOpenGL\npip install PyOpenGL",
            )
            p.end()
            return

        GL.glClear(GL.GL_COLOR_BUFFER_BIT | GL.GL_DEPTH_BUFFER_BIT)
        GL.glLoadIdentity()

        # Orbital camera
        yaw_r = math.radians(self._cam_yaw)
        pitch_r = math.radians(self._cam_pitch)
        eye_x = self._cam_target[0] + self._cam_dist * math.sin(yaw_r) * math.cos(pitch_r)
        eye_y = self._cam_target[1] + self._cam_dist * math.sin(-pitch_r)
        eye_z = self._cam_target[2] + self._cam_dist * math.cos(yaw_r) * math.cos(pitch_r)
        GLU.gluLookAt(
            eye_x, eye_y, eye_z,
            *self._cam_target,
            0.0, 1.0, 0.0,
        )

        if self.scene.grid_visible:
            self._draw_grid()
        if self.scene.floor_visible:
            self._draw_floor()
        self._draw_scene()

    # ------------------------------------------------------------------
    # Drawing helpers
    # ------------------------------------------------------------------

    def _draw_grid(self) -> None:
        """Draw a 20x20 grid on the XZ plane at Y=0, 1m spacing."""
        GL.glDisable(GL.GL_LIGHTING)
        GL.glColor4f(0.2, 0.2, 0.22, 0.7)
        GL.glLineWidth(1.0)
        GL.glBegin(GL.GL_LINES)
        half = 10
        for i in range(-half, half + 1):
            GL.glVertex3f(i, 0.0, -half)
            GL.glVertex3f(i, 0.0, half)
            GL.glVertex3f(-half, 0.0, i)
            GL.glVertex3f(half, 0.0, i)
        GL.glEnd()
        # Axis indicators
        GL.glLineWidth(2.0)
        GL.glBegin(GL.GL_LINES)
        GL.glColor3f(0.5, 0.1, 0.1)
        GL.glVertex3f(0, 0, 0)
        GL.glVertex3f(1, 0, 0)
        GL.glColor3f(0.1, 0.5, 0.1)
        GL.glVertex3f(0, 0, 0)
        GL.glVertex3f(0, 1, 0)
        GL.glColor3f(0.1, 0.1, 0.5)
        GL.glVertex3f(0, 0, 0)
        GL.glVertex3f(0, 0, 1)
        GL.glEnd()
        GL.glLineWidth(1.0)
        GL.glEnable(GL.GL_LIGHTING)

    def _draw_floor(self) -> None:
        """Draw a subtle floor rectangle."""
        GL.glDisable(GL.GL_LIGHTING)
        GL.glColor4f(0.08, 0.07, 0.06, 0.6)
        GL.glBegin(GL.GL_QUADS)
        GL.glVertex3f(-10.0, 0.0, -8.0)
        GL.glVertex3f( 10.0, 0.0, -8.0)
        GL.glVertex3f( 10.0, 0.0,  8.0)
        GL.glVertex3f(-10.0, 0.0,  8.0)
        GL.glEnd()
        GL.glEnable(GL.GL_LIGHTING)

    def _draw_scene(self) -> None:
        for obj in self.scene.objects:
            if not obj.visible:
                continue
            GL.glPushMatrix()
            GL.glTranslatef(obj.pos.x, obj.pos.y, obj.pos.z)
            GL.glRotatef(obj.rot.y, 0.0, 1.0, 0.0)
            GL.glRotatef(obj.rot.x, 1.0, 0.0, 0.0)
            GL.glRotatef(obj.rot.z, 0.0, 0.0, 1.0)
            GL.glScalef(obj.scale.x, obj.scale.y, obj.scale.z)

            is_selected = (obj.id == self._selected_id)

            if isinstance(obj, TrussObject):
                self._draw_truss(obj, is_selected)
            elif isinstance(obj, FixtureObject):
                self._draw_fixture(obj, is_selected)
            elif isinstance(obj, ProjectionObject):
                self._draw_projection(obj, is_selected)
            elif isinstance(obj, StageObject):
                self._draw_stage_obj(obj, is_selected)
            else:
                # Generic fallback: small cube
                self._set_color(obj.color, is_selected)
                self._draw_box(0.3, 0.3, 0.3)

            GL.glPopMatrix()

    # ------------------------------------------------------------------
    # Truss
    # ------------------------------------------------------------------

    def _draw_truss(self, obj: TrussObject, selected: bool) -> None:
        GL.glEnable(GL.GL_LIGHTING)
        self._set_color(obj.color, selected)
        half = obj.length / 2.0
        r = obj.tube_radius
        r_inner = r * 0.6  # cross-bracing radius

        quad = GLU.gluNewQuadric()
        GLU.gluQuadricNormals(quad, GLU.GLU_SMOOTH)

        if obj.truss_type == "triangular":
            offsets = [(-r * 3, 0), (r * 3, 0), (0, r * 4)]
        else:  # square / ground
            offsets = [(-r * 3, -r * 3), (r * 3, -r * 3), (r * 3, r * 3), (-r * 3, r * 3)]

        # Draw main tubes along X axis
        for dx, dy in offsets:
            GL.glPushMatrix()
            GL.glTranslatef(-half, dy, dx)
            GL.glRotatef(90, 0, 1, 0)
            GLU.gluCylinder(quad, r, r, obj.length, 8, 1)
            GL.glPopMatrix()

        # Cross-bracing as lines
        GL.glDisable(GL.GL_LIGHTING)
        GL.glLineWidth(1.0)
        GL.glColor3f(obj.color[0] * 0.7, obj.color[1] * 0.7, obj.color[2] * 0.7)
        GL.glBegin(GL.GL_LINES)
        n_braces = max(2, int(obj.length * 2))
        step = obj.length / n_braces
        for i in range(n_braces):
            x0 = -half + i * step
            x1 = -half + (i + 1) * step
            # Connect adjacent tubes with X pattern
            for j in range(len(offsets)):
                ax, ay = offsets[j]
                bx, by = offsets[(j + 1) % len(offsets)]
                GL.glVertex3f(x0, ay, ax)
                GL.glVertex3f(x1, by, bx)
                GL.glVertex3f(x0, by, bx)
                GL.glVertex3f(x1, ay, ax)
        GL.glEnd()
        GL.glEnable(GL.GL_LIGHTING)

        GLU.gluDeleteQuadric(quad)

    # ------------------------------------------------------------------
    # Fixture
    # ------------------------------------------------------------------

    def _draw_fixture(self, obj: FixtureObject, selected: bool) -> None:
        # Get DMX color for this fixture
        beam_r, beam_g, beam_b, dimmer = self._get_fixture_dmx_color(obj)

        # Draw body
        GL.glEnable(GL.GL_LIGHTING)
        self._set_color(obj.color, selected)
        self._draw_box(0.25, 0.2, 0.25)

        # Yoke
        GL.glColor3f(0.25, 0.25, 0.25)
        quad = GLU.gluNewQuadric()
        GL.glPushMatrix()
        GL.glTranslatef(0, -0.15, 0)
        GLU.gluCylinder(quad, 0.02, 0.02, 0.1, 6, 1)
        GL.glPopMatrix()
        GLU.gluDeleteQuadric(quad)

        # Beam cone (semi-transparent)
        if dimmer > 0.02:
            GL.glDisable(GL.GL_LIGHTING)
            GL.glEnable(GL.GL_BLEND)
            alpha = dimmer * 0.35
            GL.glColor4f(beam_r, beam_g, beam_b, alpha)
            GL.glBlendFunc(GL.GL_SRC_ALPHA, GL.GL_ONE)  # additive for light

            half_angle = math.radians(obj.beam_angle)
            tip_r = 0.01
            base_r = obj.beam_length * math.tan(half_angle)

            n_segments = 16
            GL.glBegin(GL.GL_TRIANGLE_FAN)
            GL.glVertex3f(0.0, -0.1, 0.0)  # tip
            for i in range(n_segments + 1):
                theta = 2.0 * math.pi * i / n_segments
                cx = base_r * math.cos(theta)
                cz = base_r * math.sin(theta)
                GL.glVertex3f(cx, -0.1 - obj.beam_length, cz)
            GL.glEnd()

            GL.glBlendFunc(GL.GL_SRC_ALPHA, GL.GL_ONE_MINUS_SRC_ALPHA)
            GL.glEnable(GL.GL_LIGHTING)

    def _get_fixture_dmx_color(
        self, obj: FixtureObject
    ) -> tuple[float, float, float, float]:
        """Return (r, g, b, dimmer) 0-1 from live DMX values if patched."""
        # Default: use the scene color
        sr, sg, sb = obj.color[0], obj.color[1], obj.color[2]
        dimmer = 0.8

        # Try to get from engine if patch_id is valid
        # We need to find the patched fixture by id; the engine holds universes.
        # The gl_widget gets a reference to engine; fixtures are looked up by
        # scanning universes.  We access raw channels via a stored patch map
        # if available via set_patch_map.
        patch_map = getattr(self, "_patch_map", {})
        pf = patch_map.get(obj.patch_id)
        if pf is not None:
            univ = pf.universe
            addr = pf.address
            ch_names = [n.lower() for n in pf.ch_names]

            def _ch(name_part: str) -> float:
                for i, n in enumerate(ch_names):
                    if name_part in n:
                        return self.engine.get_channel(univ, addr + i) / 255.0
                return 0.0

            r = _ch("red")
            g = _ch("green")
            b = _ch("blue")
            dim = _ch("dimmer") or _ch("intensity") or _ch("master")

            has_rgb = r > 0 or g > 0 or b > 0
            if has_rgb:
                sr, sg, sb = r, g, b
            if dim > 0:
                dimmer = dim
            elif has_rgb:
                dimmer = max(r, g, b)

        return sr, sg, sb, dimmer

    def set_patch_map(self, patch_map: dict) -> None:
        """Set {patch_id: PatchedFixture} for beam coloring."""
        self._patch_map = patch_map

    # ------------------------------------------------------------------
    # Stage element
    # ------------------------------------------------------------------

    def _draw_stage_obj(self, obj: StageObject, selected: bool) -> None:
        GL.glEnable(GL.GL_LIGHTING)
        self._set_color(obj.color, selected)

        if obj.shape == "cylinder":
            quad = GLU.gluNewQuadric()
            GLU.gluQuadricNormals(quad, GLU.GLU_SMOOTH)
            GL.glPushMatrix()
            GL.glTranslatef(0, -obj.height / 2, 0)
            GL.glRotatef(-90, 1, 0, 0)
            GLU.gluCylinder(quad, obj.width / 2, obj.width / 2, obj.height, 16, 2)
            GLU.gluDisk(quad, 0, obj.width / 2, 16, 1)
            GL.glTranslatef(0, 0, obj.height)
            GLU.gluDisk(quad, 0, obj.width / 2, 16, 1)
            GL.glPopMatrix()
            GLU.gluDeleteQuadric(quad)
        elif obj.shape == "plane":
            GL.glDisable(GL.GL_LIGHTING)
            alpha = 0.85
            r, g, b = obj.color[0], obj.color[1], obj.color[2]
            GL.glColor4f(r, g, b, alpha)
            hw = obj.width / 2
            hd = obj.depth / 2
            GL.glBegin(GL.GL_QUADS)
            GL.glNormal3f(0, 1, 0)
            GL.glVertex3f(-hw, 0, -hd)
            GL.glVertex3f( hw, 0, -hd)
            GL.glVertex3f( hw, 0,  hd)
            GL.glVertex3f(-hw, 0,  hd)
            GL.glEnd()
            GL.glEnable(GL.GL_LIGHTING)
        else:  # box
            self._draw_box(obj.width, obj.height, obj.depth)

    # ------------------------------------------------------------------
    # Projection surface
    # ------------------------------------------------------------------

    def _draw_projection(self, obj: ProjectionObject, selected: bool) -> None:
        GL.glDisable(GL.GL_LIGHTING)
        hw = obj.width / 2
        hh = obj.height / 2

        # Fill color
        if obj.source_type == "ndi":
            # Try to get texture from NDI receiver
            recv = self._ndi_receivers.get(obj.id)
            if recv is not None:
                tex_id = recv.get_texture_id() if hasattr(recv, "get_texture_id") else 0
                if tex_id:
                    GL.glEnable(GL.GL_TEXTURE_2D)
                    GL.glBindTexture(GL.GL_TEXTURE_2D, tex_id)
                    GL.glColor4f(1.0, 1.0, 1.0, 1.0)
                    GL.glBegin(GL.GL_QUADS)
                    GL.glTexCoord2f(0, 1); GL.glVertex3f(-hw, -hh, 0)
                    GL.glTexCoord2f(1, 1); GL.glVertex3f( hw, -hh, 0)
                    GL.glTexCoord2f(1, 0); GL.glVertex3f( hw,  hh, 0)
                    GL.glTexCoord2f(0, 0); GL.glVertex3f(-hw,  hh, 0)
                    GL.glEnd()
                    GL.glDisable(GL.GL_TEXTURE_2D)
                    GL.glBindTexture(GL.GL_TEXTURE_2D, 0)
                    GL.glEnable(GL.GL_LIGHTING)
                    return

        # Fallback: solid color or gradient
        r = obj.r / 255.0
        g = obj.g / 255.0
        b = obj.b / 255.0
        GL.glColor4f(r, g, b, 1.0)
        GL.glBegin(GL.GL_QUADS)
        GL.glVertex3f(-hw, -hh, 0)
        GL.glVertex3f( hw, -hh, 0)
        GL.glVertex3f( hw,  hh, 0)
        GL.glVertex3f(-hw,  hh, 0)
        GL.glEnd()

        # Border
        if selected:
            GL.glColor3f(0.0, 0.6, 0.8)
        else:
            GL.glColor3f(0.3, 0.3, 0.35)
        GL.glLineWidth(1.5)
        GL.glBegin(GL.GL_LINE_LOOP)
        GL.glVertex3f(-hw, -hh, 0.01)
        GL.glVertex3f( hw, -hh, 0.01)
        GL.glVertex3f( hw,  hh, 0.01)
        GL.glVertex3f(-hw,  hh, 0.01)
        GL.glEnd()
        GL.glLineWidth(1.0)
        GL.glEnable(GL.GL_LIGHTING)

    # ------------------------------------------------------------------
    # Utility: draw a box, set material color, selection highlight
    # ------------------------------------------------------------------

    def _draw_box(self, w: float, h: float, d: float) -> None:
        hw, hh, hd = w / 2, h / 2, d / 2
        faces = [
            # normal, vertices
            ([0, 0, 1],  [(-hw,-hh, hd), ( hw,-hh, hd), ( hw, hh, hd), (-hw, hh, hd)]),
            ([0, 0,-1],  [( hw,-hh,-hd), (-hw,-hh,-hd), (-hw, hh,-hd), ( hw, hh,-hd)]),
            ([0, 1, 0],  [(-hw, hh, hd), ( hw, hh, hd), ( hw, hh,-hd), (-hw, hh,-hd)]),
            ([0,-1, 0],  [(-hw,-hh,-hd), ( hw,-hh,-hd), ( hw,-hh, hd), (-hw,-hh, hd)]),
            ([1, 0, 0],  [( hw,-hh, hd), ( hw,-hh,-hd), ( hw, hh,-hd), ( hw, hh, hd)]),
            ([-1,0, 0],  [(-hw,-hh,-hd), (-hw,-hh, hd), (-hw, hh, hd), (-hw, hh,-hd)]),
        ]
        GL.glBegin(GL.GL_QUADS)
        for normal, verts in faces:
            GL.glNormal3fv(normal)
            for v in verts:
                GL.glVertex3fv(v)
        GL.glEnd()

    def _set_color(self, color: tuple, selected: bool) -> None:
        if selected:
            GL.glColor3f(0.0, 0.6, 0.9)
        else:
            if len(color) == 3:
                GL.glColor3f(color[0], color[1], color[2])
            else:
                GL.glColor4f(color[0], color[1], color[2], color[3])

    # ------------------------------------------------------------------
    # Camera / mouse controls
    # ------------------------------------------------------------------

    def mousePressEvent(self, event) -> None:
        self._last_mouse = (event.position().x(), event.position().y())

        if event.button() == Qt.MouseButton.LeftButton and not self._edit_mode:
            # Pick object by color (simplified: just track press for drag)
            pass

    def mouseMoveEvent(self, event) -> None:
        if self._last_mouse is None:
            return
        x, y = event.position().x(), event.position().y()
        dx = x - self._last_mouse[0]
        dy = y - self._last_mouse[1]
        self._last_mouse = (x, y)

        buttons = event.buttons()
        if buttons & (Qt.MouseButton.RightButton | Qt.MouseButton.MiddleButton):
            # Orbit
            self._cam_yaw -= dx * 0.5
            self._cam_pitch -= dy * 0.5
            self._cam_pitch = max(-89.0, min(89.0, self._cam_pitch))
            self.update()
        elif buttons & Qt.MouseButton.LeftButton and self._edit_mode:
            # Move selected object in XZ plane
            obj = self.scene.get(self._selected_id)
            if obj is not None:
                obj.pos.x += dx * 0.02
                obj.pos.z += dy * 0.02
                self.update()

    def mouseReleaseEvent(self, event) -> None:
        self._last_mouse = None

    def wheelEvent(self, event) -> None:
        delta = event.angleDelta().y()
        self._cam_dist -= delta * 0.01
        self._cam_dist = max(1.0, min(60.0, self._cam_dist))
        self.update()

    def reset_camera(self) -> None:
        self._cam_yaw = 30.0
        self._cam_pitch = -25.0
        self._cam_dist = 12.0
        self._cam_target = [0.0, 1.0, 0.0]
        self.update()

    def set_edit_mode(self, enabled: bool) -> None:
        self._edit_mode = enabled

    def select_object(self, obj_id: str) -> None:
        self._selected_id = obj_id
        self.update()

    def register_ndi_receiver(self, obj_id: str, receiver) -> None:
        self._ndi_receivers[obj_id] = receiver

    def unregister_ndi_receiver(self, obj_id: str) -> None:
        self._ndi_receivers.pop(obj_id, None)
