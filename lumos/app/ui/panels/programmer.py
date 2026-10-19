"""
Programmer panel — shows selected fixture channels for live editing.
Central panel when in DESK mode with fixtures selected.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QGridLayout,
    QPushButton, QLabel, QScrollArea, QFrame, QSizePolicy,
    QInputDialog,
)
from PySide6.QtCore import Qt, Signal, QSize, QRect, QPoint
from PySide6.QtGui import (
    QPainter, QPen, QColor, QFont, QLinearGradient,
    QPaintEvent, QMouseEvent, QWheelEvent,
)

from app.engine import LumosEngine
from app.ui import theme as T

# Channel group definitions
_GROUPS = [
    ("INTENSITY", ["Intensity", "Dimmer", "Master"], T.C_DIMMER),
    ("COLOUR",    ["Red", "Green", "Blue", "White", "Amber", "UV", "Color Wheel"], T.C_COLOR),
    ("POSITION",  ["Pan", "Pan Fine", "Tilt", "Tilt Fine"], T.C_POSITION),
    ("BEAM",      ["Shutter", "Gobo Wheel", "Iris", "Focus", "Zoom", "Frost"], T.C_BEAM),
    ("CONTROL",   ["Speed", "Mode", "Reset", "Macro"], T.C_CONTROL),
]


class _ParamKnob(QWidget):
    """Round encoder-style control for a single channel."""
    value_changed = Signal(int)   # 0-255

    def __init__(self, name: str, value: int = 0, color: str = "#808080",
                 parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.param_name = name
        self._value = value
        self._color = color
        self._drag_y: Optional[int] = None
        self._drag_v: int = 0
        self.setFixedSize(80, 90)
        self.setCursor(Qt.CursorShape.SizeVerCursor)
        self.setToolTip(f"{name}: {value}  (drag up/down or scroll)")

    @property
    def value(self) -> int:
        return self._value

    @value.setter
    def value(self, v: int) -> None:
        v = max(0, min(255, v))
        if v != self._value:
            self._value = v
            self.update()

    def paintEvent(self, event: QPaintEvent) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        cx, cy = w // 2, 38

        # Knob background
        r = 28
        p.setBrush(QColor(T.C_PANEL2))
        p.setPen(QPen(QColor(T.C_BORDER), 1))
        p.drawEllipse(cx - r, cy - r, r * 2, r * 2)

        # Arc fill (from -135° to position)
        import math
        angle_start = 225   # degrees, Qt units (anti-clockwise from 3 o'clock)
        span = int(self._value / 255.0 * 270)
        p.setPen(QPen(QColor(self._color), 4, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
        p.setBrush(Qt.BrushStyle.NoBrush)
        # Qt drawArc: startAngle in 1/16 degree, counter-clockwise
        p.drawArc(cx - r + 4, cy - r + 4, (r - 4) * 2, (r - 4) * 2,
                  int(angle_start * 16), int(-span * 16))

        # Indicator line
        angle_deg = angle_start - span
        rad = math.radians(angle_deg)
        ix = cx + int((r - 8) * math.cos(rad))
        iy = cy - int((r - 8) * math.sin(rad))
        p.setPen(QPen(QColor(self._color), 2, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
        p.drawLine(cx, cy, ix, iy)

        # Value text inside knob
        p.setPen(QColor(T.C_TEXT))
        p.setFont(QFont("Consolas", 9, QFont.Weight.Bold))
        p.drawText(QRect(cx - r, cy - 10, r * 2, 20),
                   Qt.AlignmentFlag.AlignHCenter | Qt.AlignmentFlag.AlignVCenter,
                   str(self._value))

        # Name label below
        p.setPen(QColor(self._color))
        p.setFont(QFont("Segoe UI", 8, QFont.Weight.Bold))
        p.drawText(QRect(0, cy + r + 2, w, 16),
                   Qt.AlignmentFlag.AlignHCenter, self.param_name[:10])

    def mousePressEvent(self, e: QMouseEvent) -> None:
        if e.button() == Qt.MouseButton.LeftButton:
            self._drag_y = int(e.position().y())
            self._drag_v = self._value
        elif e.button() == Qt.MouseButton.RightButton:
            v, ok = QInputDialog.getInt(None, self.param_name,
                                        f"{self.param_name} (0–255):",
                                        self._value, 0, 255)
            if ok:
                self._set(v)

    def mouseMoveEvent(self, e: QMouseEvent) -> None:
        if self._drag_y is not None:
            dy = self._drag_y - int(e.position().y())
            self._set(self._drag_v + dy * 2)

    def mouseReleaseEvent(self, e: QMouseEvent) -> None:
        self._drag_y = None

    def mouseDoubleClickEvent(self, e: QMouseEvent) -> None:
        self._set(0)

    def wheelEvent(self, e: QWheelEvent) -> None:
        step = 10 if e.modifiers() & Qt.KeyboardModifier.ShiftModifier else 1
        delta = step if e.angleDelta().y() > 0 else -step
        self._set(self._value + delta)

    def _set(self, v: int) -> None:
        v = max(0, min(255, v))
        if v != self._value:
            self._value = v
            self.update()
            self.value_changed.emit(v)


class Programmer(QWidget):
    """
    Shows channels for currently selected fixtures, grouped by type.
    Also has Record Scene and Clear buttons.
    """
    scene_recorded = Signal(str)

    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self._selected_fixtures: list[dict] = []
        self._knobs: dict[str, _ParamKnob] = {}
        self._universe = 1
        self._build()

    def _build(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(6)

        # Toolbar
        bar = QHBoxLayout()
        bar.setSpacing(8)

        self._selection_lbl = QLabel("No fixtures selected")
        self._selection_lbl.setStyleSheet(
            f"color: {T.C_TEXT_DIM}; font-size: 10px; font-weight: 700;"
        )
        bar.addWidget(self._selection_lbl)

        bar.addStretch()

        btn_rec = QPushButton("⬤  RECORD SCENE")
        btn_rec.setStyleSheet(
            f"background: #1a0a1a; border: 1px solid {T.C_EFFECT}; color: {T.C_EFFECT};"
            f" font-weight: 700; min-height: 32px; padding: 0 14px; border-radius: 3px;"
        )
        btn_rec.clicked.connect(self._record_scene)
        bar.addWidget(btn_rec)

        btn_clr = QPushButton("✕  CLEAR")
        btn_clr.setStyleSheet(
            f"background: #1a0a0a; border: 1px solid {T.C_RED}; color: {T.C_RED};"
            f" font-weight: 700; min-height: 32px; padding: 0 14px; border-radius: 3px;"
        )
        btn_clr.clicked.connect(self._clear_programmer)
        bar.addWidget(btn_clr)

        root.addLayout(bar)

        line = QFrame()
        line.setFrameShape(QFrame.Shape.HLine)
        line.setStyleSheet(f"color: {T.C_BORDER};")
        root.addWidget(line)

        # Channel groups scroll area
        self._groups_widget = QWidget()
        self._groups_widget.setStyleSheet(f"background: {T.C_BG};")
        self._groups_layout = QVBoxLayout(self._groups_widget)
        self._groups_layout.setSpacing(8)
        self._groups_layout.setContentsMargins(0, 0, 0, 0)
        self._groups_layout.addStretch()

        scroll = QScrollArea()
        scroll.setWidget(self._groups_widget)
        scroll.setWidgetResizable(True)
        scroll.setStyleSheet("border: none;")
        root.addWidget(scroll, 1)

        # Empty state
        self._empty_lbl = QLabel(
            "Select fixtures from the pool on the left\nto edit their channels here."
        )
        self._empty_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._empty_lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 12px;")
        root.addWidget(self._empty_lbl)

    # ── Public ────────────────────────────────────────────────────────

    def set_fixtures(self, fixtures: list[dict]) -> None:
        """
        fixtures: list of {name, type, universe, address, channel_names: [str]}
        """
        self._selected_fixtures = fixtures
        self._rebuild_groups()

    # ── Internal ──────────────────────────────────────────────────────

    def _rebuild_groups(self) -> None:
        self._knobs.clear()
        # Remove all old group widgets
        while self._groups_layout.count() > 1:
            item = self._groups_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        if not self._selected_fixtures:
            self._selection_lbl.setText("No fixtures selected")
            self._empty_lbl.show()
            return

        self._empty_lbl.hide()
        count = len(self._selected_fixtures)
        names = ", ".join(f["name"] for f in self._selected_fixtures[:3])
        if count > 3:
            names += f" +{count - 3} more"
        self._selection_lbl.setText(f"{count} fixture{'s' if count > 1 else ''}: {names}")

        # Collect all channel names from selected fixtures
        all_channels: set[str] = set()
        for fx in self._selected_fixtures:
            all_channels.update(fx.get("channel_names", []))

        # Build group sections
        for grp_name, ch_names, color in _GROUPS:
            matching = [ch for ch in ch_names if ch in all_channels]
            if not matching:
                continue

            grp_w = QWidget()
            grp_w.setStyleSheet(
                f"background: {T.C_PANEL}; border-radius: 4px;"
                f" border-left: 3px solid {color};"
            )
            grp_l = QVBoxLayout(grp_w)
            grp_l.setContentsMargins(8, 6, 8, 8)
            grp_l.setSpacing(4)

            hdr = QLabel(grp_name)
            hdr.setStyleSheet(
                f"color: {color}; font-size: 10px; font-weight: 800;"
                f" letter-spacing: 1px; border: none; background: transparent;"
            )
            grp_l.addWidget(hdr)

            knob_row = QHBoxLayout()
            knob_row.setSpacing(6)
            knob_row.addStretch()

            for ch_name in matching:
                # Get current value from engine
                v = self._get_channel_value(ch_name)
                knob = _ParamKnob(ch_name, v, color)
                knob.value_changed.connect(
                    lambda val, cn=ch_name: self._on_knob_changed(cn, val)
                )
                knob_row.insertWidget(knob_row.count() - 1, knob)
                self._knobs[ch_name] = knob

            knob_row.addStretch()
            grp_l.addLayout(knob_row)
            self._groups_layout.insertWidget(self._groups_layout.count() - 1, grp_w)

        # Channels not in any known group
        leftover = [
            ch for ch in all_channels
            if not any(ch in grp[1] for grp in _GROUPS)
        ]
        if leftover:
            grp_w = QWidget()
            grp_w.setStyleSheet(
                f"background: {T.C_PANEL}; border-radius: 4px;"
                f" border-left: 3px solid {T.C_TEXT_DIM};"
            )
            grp_l = QVBoxLayout(grp_w)
            grp_l.setContentsMargins(8, 6, 8, 8)
            grp_l.setSpacing(4)
            hdr = QLabel("OTHER")
            hdr.setStyleSheet(
                f"color: {T.C_TEXT_DIM}; font-size: 10px; font-weight: 800;"
                f" letter-spacing: 1px; border: none; background: transparent;"
            )
            grp_l.addWidget(hdr)
            knob_row = QHBoxLayout()
            knob_row.setSpacing(6)
            knob_row.addStretch()
            for ch_name in sorted(leftover):
                v = self._get_channel_value(ch_name)
                knob = _ParamKnob(ch_name, v, T.C_TEXT_DIM)
                knob.value_changed.connect(
                    lambda val, cn=ch_name: self._on_knob_changed(cn, val)
                )
                knob_row.insertWidget(knob_row.count() - 1, knob)
                self._knobs[ch_name] = knob
            knob_row.addStretch()
            grp_l.addLayout(knob_row)
            self._groups_layout.insertWidget(self._groups_layout.count() - 1, grp_w)

    def _get_channel_value(self, ch_name: str) -> int:
        if not self._selected_fixtures:
            return 0
        fx = self._selected_fixtures[0]
        ch_names = fx.get("channel_names", [])
        if ch_name in ch_names:
            idx = ch_names.index(ch_name)
            addr = fx.get("address", 1) + idx
            uni = fx.get("universe", 1)
            return self.engine.get_channel(uni, addr)
        return 0

    def _on_knob_changed(self, ch_name: str, value: int) -> None:
        for fx in self._selected_fixtures:
            ch_names = fx.get("channel_names", [])
            if ch_name in ch_names:
                idx = ch_names.index(ch_name)
                addr = fx.get("address", 1) + idx
                uni = fx.get("universe", 1)
                self.engine.set_channel(uni, addr, value)

    def _record_scene(self) -> None:
        name, ok = QInputDialog.getText(
            self, "Record Scene", "Scene name:", text=f"Scene {len(self.engine.scenes) + 1}"
        )
        if ok and name.strip():
            self.engine.save_scene(name.strip(), 1)
            self.scene_recorded.emit(name.strip())

    def _clear_programmer(self) -> None:
        for fx in self._selected_fixtures:
            uni = fx.get("universe", 1)
            ch_names = fx.get("channel_names", [])
            for i in range(len(ch_names)):
                self.engine.set_channel(uni, fx.get("address", 1) + i, 0)
        for knob in self._knobs.values():
            knob.value = 0
