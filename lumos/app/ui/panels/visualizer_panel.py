"""
3D Stage Visualizer panel.

Hosts the VisualizerGLWidget and a scene-object tree with property editor.
Supports NDI output (requires: pip install ndi-python) and PyOpenGL
(requires: pip install PyOpenGL).
"""
from __future__ import annotations

from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QSplitter, QTreeWidget,
    QTreeWidgetItem, QLabel, QPushButton, QToolButton, QMenu,
    QSpinBox, QDoubleSpinBox, QLineEdit, QComboBox, QCheckBox,
    QGroupBox, QFormLayout, QFrame, QSizePolicy, QDialog,
    QDialogButtonBox, QColorDialog,
)
from PySide6.QtCore import Qt, Slot, QTimer
from PySide6.QtGui import QColor, QIcon, QFont, QAction

from app.engine import LumosEngine
from app.ui import theme as T
from app.visualizer.scene import (
    VisualizerScene, SceneObject, TrussObject, FixtureObject,
    StageObject, ProjectionObject, Vec3,
)
from app.visualizer.gl_widget import VisualizerGLWidget
from app.visualizer.ndi_output import NDIOutput
from app.visualizer.ndi_finder import scan_ndi_sources


# ---------------------------------------------------------------------------
# Properties dialog
# ---------------------------------------------------------------------------

class _PropertiesDialog(QDialog):
    def __init__(self, obj: SceneObject, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.obj = obj
        self.setWindowTitle(f"Properties — {obj.name}")
        self.setModal(True)
        self.setMinimumWidth(360)
        self._build()

    def _build(self) -> None:
        root = QVBoxLayout(self)
        root.setSpacing(8)

        form = QFormLayout()
        form.setSpacing(6)
        form.setLabelAlignment(Qt.AlignmentFlag.AlignRight)

        # Common fields
        self._name = QLineEdit(self.obj.name)
        form.addRow("Name:", self._name)

        # Position
        self._px = self._spin(-50, 50, self.obj.pos.x, 2)
        self._py = self._spin(-10, 20, self.obj.pos.y, 2)
        self._pz = self._spin(-50, 50, self.obj.pos.z, 2)
        ph = self._row3(self._px, self._py, self._pz)
        form.addRow("Position (x,y,z):", ph)

        # Rotation
        self._rx = self._spin(-360, 360, self.obj.rot.x, 1)
        self._ry = self._spin(-360, 360, self.obj.rot.y, 1)
        self._rz = self._spin(-360, 360, self.obj.rot.z, 1)
        rh = self._row3(self._rx, self._ry, self._rz)
        form.addRow("Rotation (x,y,z):", rh)

        # Type-specific
        if isinstance(self.obj, TrussObject):
            self._length = self._spin(0.1, 30, self.obj.length, 2)
            form.addRow("Length (m):", self._length)
            self._truss_type = QComboBox()
            self._truss_type.addItems(["square", "triangular", "ground"])
            self._truss_type.setCurrentText(self.obj.truss_type)
            form.addRow("Truss type:", self._truss_type)

        elif isinstance(self.obj, FixtureObject):
            self._beam_angle = self._spin(1, 90, self.obj.beam_angle, 1)
            form.addRow("Beam angle (°):", self._beam_angle)
            self._beam_length = self._spin(0.5, 30, self.obj.beam_length, 1)
            form.addRow("Beam length (m):", self._beam_length)

        elif isinstance(self.obj, StageObject):
            self._shape = QComboBox()
            self._shape.addItems(["box", "cylinder", "plane"])
            self._shape.setCurrentText(self.obj.shape)
            form.addRow("Shape:", self._shape)
            self._sw = self._spin(0.1, 50, self.obj.width, 2)
            self._sh = self._spin(0.1, 20, self.obj.height, 2)
            self._sd = self._spin(0.1, 50, self.obj.depth, 2)
            form.addRow("Size (w,h,d):", self._row3(self._sw, self._sh, self._sd))
            self._color_btn = QPushButton()
            r, g, b = [int(c * 255) for c in self.obj.color]
            self._obj_color = QColor(r, g, b)
            self._update_color_btn()
            self._color_btn.clicked.connect(self._pick_color)
            form.addRow("Color:", self._color_btn)

        elif isinstance(self.obj, ProjectionObject):
            self._proj_w = self._spin(0.1, 20, self.obj.width, 2)
            self._proj_h = self._spin(0.1, 20, self.obj.height, 2)
            form.addRow("Size (w,h):", self._row2(self._proj_w, self._proj_h))
            self._src_type = QComboBox()
            self._src_type.addItems(["color", "ndi"])
            self._src_type.setCurrentText(self.obj.source_type)
            form.addRow("Source:", self._src_type)
            self._ndi_src = QLineEdit(self.obj.ndi_source)
            form.addRow("NDI source:", self._ndi_src)
            self._pr = QSpinBox(); self._pr.setRange(0, 255); self._pr.setValue(self.obj.r)
            self._pg = QSpinBox(); self._pg.setRange(0, 255); self._pg.setValue(self.obj.g)
            self._pb = QSpinBox(); self._pb.setRange(0, 255); self._pb.setValue(self.obj.b)
            form.addRow("Color (R,G,B):", self._row3(self._pr, self._pg, self._pb))

        root.addLayout(form)

        bbox = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        bbox.accepted.connect(self._save)
        bbox.rejected.connect(self.reject)
        root.addWidget(bbox)

    def _spin(self, lo: float, hi: float, val: float, decimals: int) -> QDoubleSpinBox:
        s = QDoubleSpinBox()
        s.setRange(lo, hi)
        s.setDecimals(decimals)
        s.setValue(val)
        s.setSingleStep(0.1)
        return s

    def _row3(self, a: QWidget, b: QWidget, c: QWidget) -> QWidget:
        w = QWidget()
        h = QHBoxLayout(w)
        h.setContentsMargins(0, 0, 0, 0)
        h.setSpacing(4)
        for x in (a, b, c):
            h.addWidget(x)
        return w

    def _row2(self, a: QWidget, b: QWidget) -> QWidget:
        w = QWidget()
        h = QHBoxLayout(w)
        h.setContentsMargins(0, 0, 0, 0)
        h.setSpacing(4)
        h.addWidget(a); h.addWidget(b)
        return w

    def _pick_color(self) -> None:
        c = QColorDialog.getColor(self._obj_color, self)
        if c.isValid():
            self._obj_color = c
            self._update_color_btn()

    def _update_color_btn(self) -> None:
        self._color_btn.setStyleSheet(
            f"background:{self._obj_color.name()}; border:none; border-radius:3px; min-height:24px;"
        )

    def _save(self) -> None:
        self.obj.name = self._name.text().strip() or self.obj.name
        self.obj.pos = Vec3(self._px.value(), self._py.value(), self._pz.value())
        self.obj.rot = Vec3(self._rx.value(), self._ry.value(), self._rz.value())

        if isinstance(self.obj, TrussObject):
            self.obj.length = self._length.value()
            self.obj.truss_type = self._truss_type.currentText()
        elif isinstance(self.obj, FixtureObject):
            self.obj.beam_angle = self._beam_angle.value()
            self.obj.beam_length = self._beam_length.value()
        elif isinstance(self.obj, StageObject):
            self.obj.shape = self._shape.currentText()
            self.obj.width = self._sw.value()
            self.obj.height = self._sh.value()
            self.obj.depth = self._sd.value()
            c = self._obj_color
            self.obj.color = (c.redF(), c.greenF(), c.blueF())
        elif isinstance(self.obj, ProjectionObject):
            self.obj.width = self._proj_w.value()
            self.obj.height = self._proj_h.value()
            self.obj.source_type = self._src_type.currentText()
            self.obj.ndi_source = self._ndi_src.text().strip()
            self.obj.r = self._pr.value()
            self.obj.g = self._pg.value()
            self.obj.b = self._pb.value()

        self.accept()


# ---------------------------------------------------------------------------
# Main panel
# ---------------------------------------------------------------------------

class VisualizerPanel(QWidget):
    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self.scene = VisualizerScene()
        self.scene.default_stage()
        self._ndi = NDIOutput("Lumos Visualizer", self)
        self._patched_fixtures: list = []
        self._build_ui()

        # Refresh GL on every DMX tick
        engine.dmx_updated.connect(self._on_dmx_updated)

    # ------------------------------------------------------------------
    # Build
    # ------------------------------------------------------------------

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        root.addWidget(self._build_toolbar())
        root.addWidget(self._make_hline())

        splitter = QSplitter(Qt.Orientation.Horizontal)
        splitter.setHandleWidth(4)
        splitter.setStyleSheet(
            f"QSplitter::handle {{ background:{T.C_BORDER}; }}"
        )

        # Left: GL widget
        self._gl = VisualizerGLWidget(self.scene, self.engine)
        self._gl.object_selected.connect(self._on_object_selected)
        splitter.addWidget(self._gl)

        # Right: object tree + properties
        right = self._build_right_panel()
        splitter.addWidget(right)

        splitter.setStretchFactor(0, 3)
        splitter.setStretchFactor(1, 1)
        root.addWidget(splitter, 1)

        self._ndi.set_source(self._gl)
        self._refresh_tree()

    def _build_toolbar(self) -> QWidget:
        bar = QWidget()
        bar.setFixedHeight(40)
        bar.setStyleSheet(f"background:{T.C_PANEL2};")
        lay = QHBoxLayout(bar)
        lay.setContentsMargins(8, 0, 8, 0)
        lay.setSpacing(6)

        # Edit mode toggle
        self._edit_btn = QPushButton("Edit Mode")
        self._edit_btn.setCheckable(True)
        self._edit_btn.setFixedHeight(28)
        self._edit_btn.toggled.connect(self._on_edit_mode)
        self._edit_btn.setStyleSheet(
            f"QPushButton {{ background:{T.C_WIDGET}; color:{T.C_TEXT};"
            f" border:1px solid {T.C_BORDER_LT}; border-radius:3px; padding:0 10px; }}"
            f"QPushButton:checked {{ background:{T.C_ACCENT}; color:{T.C_TEXT_HDR}; border-color:{T.C_ACCENT}; }}"
        )
        lay.addWidget(self._edit_btn)

        # Add dropdown
        add_btn = QToolButton()
        add_btn.setText("＋ Add")
        add_btn.setFixedHeight(28)
        add_btn.setPopupMode(QToolButton.ToolButtonPopupMode.InstantPopup)
        add_btn.setStyleSheet(
            f"QToolButton {{ background:{T.C_WIDGET}; color:{T.C_TEXT};"
            f" border:1px solid {T.C_BORDER_LT}; border-radius:3px; padding:0 10px; }}"
            f"QToolButton::menu-indicator {{ width:0; }}"
        )
        menu = QMenu(add_btn)
        menu.addAction("Truss",              self._add_truss)
        menu.addAction("Fixture",            self._add_fixture)
        menu.addAction("Stage Element",      self._add_stage)
        menu.addAction("Projection Surface", self._add_projection)
        add_btn.setMenu(menu)
        lay.addWidget(add_btn)

        # Default stage button
        btn_default = QPushButton("Default Stage")
        btn_default.setFixedHeight(28)
        btn_default.setStyleSheet(
            f"QPushButton {{ background:{T.C_WIDGET}; color:{T.C_TEXT};"
            f" border:1px solid {T.C_BORDER_LT}; border-radius:3px; padding:0 10px; }}"
        )
        btn_default.clicked.connect(self._reset_stage)
        lay.addWidget(btn_default)

        lay.addStretch()

        # NDI section
        self._ndi_dot = QLabel("●")
        self._ndi_dot.setStyleSheet(f"color:{T.C_TEXT_DIM}; font-size:14px;")
        lay.addWidget(self._ndi_dot)

        self._ndi_lbl = QLabel("NDI: off")
        self._ndi_lbl.setStyleSheet(f"color:{T.C_TEXT_DIM}; font-size:10px;")
        lay.addWidget(self._ndi_lbl)

        self._ndi_btn = QPushButton("Start NDI")
        self._ndi_btn.setFixedHeight(28)
        self._ndi_btn.setStyleSheet(
            f"QPushButton {{ background:{T.C_WIDGET}; color:{T.C_TEXT};"
            f" border:1px solid {T.C_BORDER_LT}; border-radius:3px; padding:0 10px; }}"
        )
        self._ndi_btn.clicked.connect(self._toggle_ndi)
        lay.addWidget(self._ndi_btn)

        self._ndi.status_changed.connect(self._on_ndi_status)

        return bar

    def _build_right_panel(self) -> QWidget:
        w = QWidget()
        w.setMinimumWidth(200)
        w.setMaximumWidth(300)
        lay = QVBoxLayout(w)
        lay.setContentsMargins(4, 4, 4, 4)
        lay.setSpacing(4)

        # Object tree
        tree_lbl = QLabel("SCENE OBJECTS")
        tree_lbl.setStyleSheet(
            f"color:{T.C_TEXT_LABEL}; font-size:10px; font-weight:800; letter-spacing:1px;"
        )
        lay.addWidget(tree_lbl)

        self._tree = QTreeWidget()
        self._tree.setHeaderHidden(True)
        self._tree.setStyleSheet(
            f"QTreeWidget {{ background:{T.C_PANEL}; border:none; }}"
            f"QTreeWidget::item {{ padding:3px; }}"
            f"QTreeWidget::item:selected {{ background:{T.C_ACCENT}; color:{T.C_TEXT_HDR}; }}"
        )
        self._tree.itemSelectionChanged.connect(self._on_tree_select)
        self._tree.itemDoubleClicked.connect(self._on_tree_double_click)
        lay.addWidget(self._tree, 1)

        # Delete button
        btn_del = QPushButton("✕ Remove Selected")
        btn_del.setFixedHeight(30)
        btn_del.clicked.connect(self._remove_selected)
        lay.addWidget(btn_del)

        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.HLine)
        sep.setStyleSheet(f"color:{T.C_BORDER};")
        lay.addWidget(sep)

        # Properties label
        props_lbl = QLabel("PROPERTIES")
        props_lbl.setStyleSheet(
            f"color:{T.C_TEXT_LABEL}; font-size:10px; font-weight:800; letter-spacing:1px;"
        )
        lay.addWidget(props_lbl)

        self._props_lbl = QLabel("Double-click an object to edit")
        self._props_lbl.setStyleSheet(
            f"color:{T.C_TEXT_DIM}; font-size:10px; font-style:italic;"
        )
        self._props_lbl.setWordWrap(True)
        lay.addWidget(self._props_lbl)

        return w

    @staticmethod
    def _make_hline() -> QFrame:
        f = QFrame()
        f.setFrameShape(QFrame.Shape.HLine)
        f.setStyleSheet(f"color:{T.C_BORDER};")
        return f

    # ------------------------------------------------------------------
    # Scene actions
    # ------------------------------------------------------------------

    def _add_truss(self) -> None:
        obj = TrussObject(name="Truss", pos=Vec3(0, 4, 0), length=4.0)
        self.scene.add(obj)
        self._refresh_tree()
        self._gl.update()

    def _add_fixture(self) -> None:
        obj = FixtureObject(name="Fixture", pos=Vec3(0, 5, 0),
                            rot=Vec3(-90, 0, 0))
        self.scene.add(obj)
        self._refresh_tree()
        self._gl.update()

    def _add_stage(self) -> None:
        obj = StageObject(name="Stage Element", pos=Vec3(0, 0, 0),
                          width=4.0, height=0.1, depth=4.0)
        self.scene.add(obj)
        self._refresh_tree()
        self._gl.update()

    def _add_projection(self) -> None:
        obj = ProjectionObject(name="Projection", pos=Vec3(0, 3, -4),
                               width=3.0, height=2.0)
        self.scene.add(obj)
        self._refresh_tree()
        self._gl.update()

    def _reset_stage(self) -> None:
        self.scene.objects.clear()
        self.scene.default_stage()
        self._refresh_tree()
        self._gl.update()

    def _remove_selected(self) -> None:
        items = self._tree.selectedItems()
        for item in items:
            obj_id = item.data(0, Qt.ItemDataRole.UserRole)
            if obj_id:
                self.scene.remove(obj_id)
        self._refresh_tree()
        self._gl.update()

    # ------------------------------------------------------------------
    # Tree
    # ------------------------------------------------------------------

    def _refresh_tree(self) -> None:
        self._tree.clear()
        categories: dict[str, QTreeWidgetItem] = {}
        type_map = {
            TrussObject:      "Trusses",
            FixtureObject:    "Fixtures",
            StageObject:      "Stage",
            ProjectionObject: "Projections",
        }
        for obj in self.scene.objects:
            cat = type_map.get(type(obj), "Other")
            if cat not in categories:
                parent = QTreeWidgetItem([cat])
                parent.setExpanded(True)
                self._tree.addTopLevelItem(parent)
                categories[cat] = parent
            child = QTreeWidgetItem([obj.name])
            child.setData(0, Qt.ItemDataRole.UserRole, obj.id)
            categories[cat].addChild(child)

    def _on_tree_select(self) -> None:
        pass

    def _on_tree_double_click(self, item: QTreeWidgetItem, _col: int) -> None:
        obj_id = item.data(0, Qt.ItemDataRole.UserRole)
        if not obj_id:
            return
        obj = self.scene.get(obj_id)
        if obj:
            dlg = _PropertiesDialog(obj, self)
            if dlg.exec():
                self._refresh_tree()
                self._gl.update()

    # ------------------------------------------------------------------
    # GL selection callback
    # ------------------------------------------------------------------

    def _on_object_selected(self, obj_id: str) -> None:
        # Highlight matching tree item
        root = self._tree.invisibleRootItem()
        for i in range(root.childCount()):
            cat = root.child(i)
            for j in range(cat.childCount()):
                child = cat.child(j)
                if child.data(0, Qt.ItemDataRole.UserRole) == obj_id:
                    self._tree.setCurrentItem(child)
                    return

    # ------------------------------------------------------------------
    # Edit mode
    # ------------------------------------------------------------------

    def _on_edit_mode(self, on: bool) -> None:
        self._gl._edit_mode = on

    # ------------------------------------------------------------------
    # NDI
    # ------------------------------------------------------------------

    def _toggle_ndi(self) -> None:
        if self._ndi._active:
            self._ndi.stop()
            self._ndi_btn.setText("Start NDI")
        else:
            self._ndi.start()
            self._ndi_btn.setText("Stop NDI")

    @Slot(str)
    def _on_ndi_status(self, msg: str) -> None:
        self._ndi_lbl.setText(f"NDI: {msg}")
        active = self._ndi._active
        self._ndi_dot.setStyleSheet(
            f"color:{T.C_GREEN if active else T.C_TEXT_DIM}; font-size:14px;"
        )

    # ------------------------------------------------------------------
    # DMX tick
    # ------------------------------------------------------------------

    @Slot(int, bytes)
    def _on_dmx_updated(self, uid: int, data: bytes) -> None:
        if self.isVisible():
            self._gl.update()

    # ------------------------------------------------------------------
    # Fixture pool integration
    # ------------------------------------------------------------------

    def refresh_fixtures(self, fixtures: list) -> None:
        """Called by main window when patch changes."""
        self._patched_fixtures = fixtures

    # ------------------------------------------------------------------
    # Show save/load helpers
    # ------------------------------------------------------------------

    def to_dict(self) -> dict:
        return self.scene.to_dict()

    def from_dict(self, d: dict) -> None:
        self.scene = VisualizerScene.from_dict(d)
        self._gl.scene = self.scene
        self._refresh_tree()
        self._gl.update()
