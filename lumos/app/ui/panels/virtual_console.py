"""
Virtual Console — freeform canvas of programmable buttons, faders, knobs,
and XY pads that control the Lumos engine.

Architecture
------------
QGraphicsScene / QGraphicsView is used as the canvas.  Each VC widget
(VCButton, VCFader, VCKnob, VCXYPad) is a QGraphicsItem subclass.

Edit mode  : items are draggable / right-click for properties dialog.
Run mode   : items respond to mouse interactions and drive the engine.
"""
from __future__ import annotations

import math
from typing import Any, Optional

from PySide6.QtCore import (
    QPointF,
    QRectF,
    QSizeF,
    Qt,
    Signal,
    Slot,
)
from PySide6.QtGui import (
    QBrush,
    QColor,
    QFont,
    QPainter,
    QPen,
    QRadialGradient,
)
from PySide6.QtWidgets import (
    QAbstractSlider,
    QCheckBox,
    QColorDialog,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QGraphicsEllipseItem,
    QGraphicsItem,
    QGraphicsProxyWidget,
    QGraphicsRectItem,
    QGraphicsScene,
    QGraphicsView,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QSlider,
    QSpinBox,
    QToolBar,
    QVBoxLayout,
    QWidget,
)

from app.engine import LumosEngine
from app.ui import theme as T


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _qcolor(hex_str: str) -> QColor:
    return QColor(hex_str)


def _mix(a: float, b: float, t: float) -> float:
    return a + (b - a) * t


# ---------------------------------------------------------------------------
# VCButton
# ---------------------------------------------------------------------------

class VCButton(QGraphicsRectItem):
    """
    A labelled button on the virtual console.

    Action types
    ------------
    "scene_recall"  — recalls a named scene on a universe
    "channel_set"   — sets universe+channel to a fixed value
    "none"          — no action
    """

    DEFAULT_W = 100
    DEFAULT_H = 60

    def __init__(
        self,
        label: str = "Button",
        parent: QGraphicsItem | None = None,
    ) -> None:
        super().__init__(0, 0, self.DEFAULT_W, self.DEFAULT_H, parent)
        self.label: str = label
        self.bg_color: str = T.C_WIDGET
        self.text_color: str = T.C_TEXT_HDR
        self.action_type: str = "none"
        self.action_data: dict = {}
        self._pressed: bool = False
        self._edit_mode: bool = False

        self.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsSelectable, True)
        self.setAcceptHoverEvents(True)

    # -- Appearance ----------------------------------------------------------

    def paint(
        self,
        painter: QPainter,
        option,
        widget=None,
    ) -> None:
        r = self.rect()

        # Background
        bg = QColor(self.bg_color)
        if self._pressed:
            bg = bg.lighter(160)
        painter.setBrush(QBrush(bg))
        pen_color = QColor(T.C_ACCENT) if self.isSelected() else QColor(T.C_BORDER)
        painter.setPen(QPen(pen_color, 1.5))
        painter.drawRoundedRect(r, 4, 4)

        # Label
        font = QFont("Segoe UI", 9, QFont.Weight.Bold)
        painter.setFont(font)
        painter.setPen(QColor(self.text_color))
        painter.drawText(r, Qt.AlignmentFlag.AlignCenter, self.label)

        # Edit-mode selection handles
        if self._edit_mode and self.isSelected():
            handle_pen = QPen(QColor(T.C_ACCENT), 1, Qt.PenStyle.DashLine)
            painter.setPen(handle_pen)
            painter.setBrush(Qt.BrushStyle.NoBrush)
            painter.drawRect(r.adjusted(1, 1, -1, -1))

    # -- Interaction ---------------------------------------------------------

    def mousePressEvent(self, event) -> None:
        if self._edit_mode:
            super().mousePressEvent(event)
            return
        if event.button() == Qt.MouseButton.LeftButton:
            self._pressed = True
            self.update()
            self._trigger()

    def mouseReleaseEvent(self, event) -> None:
        if self._edit_mode:
            super().mouseReleaseEvent(event)
            return
        if event.button() == Qt.MouseButton.LeftButton:
            self._pressed = False
            self.update()

    def contextMenuEvent(self, event) -> None:
        if self._edit_mode:
            _open_properties(self, event.screenPos())

    def _trigger(self) -> None:
        scene = self.scene()
        if not hasattr(scene, "_engine"):
            return
        eng: LumosEngine = scene._engine  # type: ignore[attr-defined]
        if self.action_type == "scene_recall":
            uni = self.action_data.get("universe", 1)
            name = self.action_data.get("scene", "")
            if name:
                eng.recall_scene(name, uni)
        elif self.action_type == "channel_set":
            uni = self.action_data.get("universe", 1)
            ch = self.action_data.get("channel", 1)
            val = self.action_data.get("value", 255)
            eng.set_channel(uni, ch, val)

    # -- Persistence ---------------------------------------------------------

    def to_dict(self) -> dict:
        p = self.pos()
        r = self.rect()
        return {
            "type": "button",
            "x": p.x(), "y": p.y(),
            "w": r.width(), "h": r.height(),
            "label": self.label,
            "bg_color": self.bg_color,
            "text_color": self.text_color,
            "action_type": self.action_type,
            "action_data": self.action_data,
        }

    @classmethod
    def from_dict(cls, d: dict) -> "VCButton":
        btn = cls(d.get("label", "Button"))
        btn.setPos(d.get("x", 0), d.get("y", 0))
        btn.setRect(0, 0, d.get("w", cls.DEFAULT_W), d.get("h", cls.DEFAULT_H))
        btn.bg_color = d.get("bg_color", T.C_WIDGET)
        btn.text_color = d.get("text_color", T.C_TEXT_HDR)
        btn.action_type = d.get("action_type", "none")
        btn.action_data = d.get("action_data", {})
        return btn


# ---------------------------------------------------------------------------
# Vertical fader widget (used inside VCFader proxy)
# ---------------------------------------------------------------------------

class _VerticalFader(QWidget):
    value_changed = Signal(int)  # 0-255

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setFixedSize(60, 160)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(4, 4, 4, 4)
        lay.setSpacing(2)

        self._label = QLabel("Fader")
        self._label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._label.setStyleSheet(f"color: {T.C_TEXT_LABEL}; font-size: 9px;")
        lay.addWidget(self._label)

        self._slider = QSlider(Qt.Orientation.Vertical)
        self._slider.setRange(0, 255)
        self._slider.setValue(0)
        self._slider.valueChanged.connect(self.value_changed)
        lay.addWidget(self._slider, stretch=1)

        self._val_lbl = QLabel("0")
        self._val_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._val_lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 9px;")
        self._slider.valueChanged.connect(lambda v: self._val_lbl.setText(str(v)))
        lay.addWidget(self._val_lbl)

    def set_label(self, text: str) -> None:
        self._label.setText(text)

    def value(self) -> int:
        return self._slider.value()

    def set_value(self, v: int) -> None:
        self._slider.setValue(v)


# ---------------------------------------------------------------------------
# VCFader
# ---------------------------------------------------------------------------

class VCFader(QGraphicsProxyWidget):
    """
    A vertical fader that controls a universe+channel or acts as a master.
    """

    def __init__(self, parent: QGraphicsItem | None = None) -> None:
        super().__init__(parent)
        self.label: str = "Fader"
        self.action_type: str = "channel"   # "channel" | "master"
        self.action_data: dict = {"universe": 1, "channel": 1}
        self._edit_mode: bool = False

        self._widget = _VerticalFader()
        self._widget.value_changed.connect(self._on_value_changed)
        self.setWidget(self._widget)
        self.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsSelectable, True)

    def contextMenuEvent(self, event) -> None:
        if self._edit_mode:
            _open_properties(self, event.screenPos())

    @Slot(int)
    def _on_value_changed(self, v: int) -> None:
        scene = self.scene()
        if not hasattr(scene, "_engine"):
            return
        eng: LumosEngine = scene._engine  # type: ignore[attr-defined]
        if self.action_type == "channel":
            uni = self.action_data.get("universe", 1)
            ch = self.action_data.get("channel", 1)
            eng.set_channel(uni, ch, v)
        elif self.action_type == "master":
            eng.grand_master = v / 255.0

    def to_dict(self) -> dict:
        p = self.pos()
        return {
            "type": "fader",
            "x": p.x(), "y": p.y(),
            "label": self.label,
            "value": self._widget.value(),
            "action_type": self.action_type,
            "action_data": self.action_data,
        }

    @classmethod
    def from_dict(cls, d: dict) -> "VCFader":
        f = cls()
        f.setPos(d.get("x", 0), d.get("y", 0))
        f.label = d.get("label", "Fader")
        f._widget.set_label(f.label)
        f._widget.set_value(d.get("value", 0))
        f.action_type = d.get("action_type", "channel")
        f.action_data = d.get("action_data", {"universe": 1, "channel": 1})
        return f


# ---------------------------------------------------------------------------
# VCKnob
# ---------------------------------------------------------------------------

class VCKnob(QGraphicsEllipseItem):
    """
    A rotary knob.  Drag up to increase, down to decrease.
    """

    DEFAULT_SIZE = 64

    def __init__(self, parent: QGraphicsItem | None = None) -> None:
        s = self.DEFAULT_SIZE
        super().__init__(0, 0, s, s, parent)
        self.label: str = "Knob"
        self.action_type: str = "channel"
        self.action_data: dict = {"universe": 1, "channel": 1}
        self._value: float = 0.0   # 0.0-1.0
        self._edit_mode: bool = False
        self._drag_start_y: Optional[float] = None
        self._drag_start_val: float = 0.0

        self.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsSelectable, True)
        self.setAcceptHoverEvents(True)

    def value_int(self) -> int:
        return int(self._value * 255)

    def paint(self, painter: QPainter, option, widget=None) -> None:
        r = self.rect()
        cx, cy = r.center().x(), r.center().y()
        radius = r.width() / 2.0

        # Background circle
        grad = QRadialGradient(cx, cy, radius)
        grad.setColorAt(0, QColor("#2a2a2a"))
        grad.setColorAt(1, QColor("#111111"))
        painter.setBrush(QBrush(grad))
        pen_color = QColor(T.C_ACCENT) if self.isSelected() else QColor(T.C_BORDER)
        painter.setPen(QPen(pen_color, 1.5))
        painter.drawEllipse(r)

        # Arc showing value (from 225° to 225°+270°*value)
        arc_rect = r.adjusted(6, 6, -6, -6)
        pen_arc = QPen(QColor(T.C_ACCENT), 3, Qt.PenStyle.SolidLine,
                       Qt.PenCapStyle.RoundCap)
        painter.setPen(pen_arc)
        start_angle = 225 * 16  # Qt uses 1/16th degrees
        span_angle = int(-270 * 16 * self._value)
        painter.drawArc(arc_rect, start_angle, span_angle)

        # Center dot
        dot_r = 4.0
        painter.setBrush(QBrush(QColor(T.C_TEXT_HDR)))
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawEllipse(QRectF(cx - dot_r, cy - dot_r, dot_r * 2, dot_r * 2))

        # Label below
        font = QFont("Segoe UI", 7)
        painter.setFont(font)
        painter.setPen(QColor(T.C_TEXT_LABEL))
        painter.drawText(
            QRectF(r.left(), r.bottom() - 16, r.width(), 16),
            Qt.AlignmentFlag.AlignCenter,
            self.label,
        )

    def mousePressEvent(self, event) -> None:
        if self._edit_mode:
            super().mousePressEvent(event)
            return
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_start_y = event.pos().y()
            self._drag_start_val = self._value

    def mouseMoveEvent(self, event) -> None:
        if self._edit_mode or self._drag_start_y is None:
            super().mouseMoveEvent(event)
            return
        dy = self._drag_start_y - event.pos().y()
        new_val = max(0.0, min(1.0, self._drag_start_val + dy / 150.0))
        self._value = new_val
        self.update()
        self._emit_value()

    def mouseReleaseEvent(self, event) -> None:
        if self._edit_mode:
            super().mouseReleaseEvent(event)
            return
        self._drag_start_y = None

    def contextMenuEvent(self, event) -> None:
        if self._edit_mode:
            _open_properties(self, event.screenPos())

    def _emit_value(self) -> None:
        scene = self.scene()
        if not hasattr(scene, "_engine"):
            return
        eng: LumosEngine = scene._engine  # type: ignore[attr-defined]
        if self.action_type == "channel":
            uni = self.action_data.get("universe", 1)
            ch = self.action_data.get("channel", 1)
            eng.set_channel(uni, ch, self.value_int())

    def to_dict(self) -> dict:
        p = self.pos()
        r = self.rect()
        return {
            "type": "knob",
            "x": p.x(), "y": p.y(),
            "size": r.width(),
            "label": self.label,
            "value": self._value,
            "action_type": self.action_type,
            "action_data": self.action_data,
        }

    @classmethod
    def from_dict(cls, d: dict) -> "VCKnob":
        k = cls()
        k.setPos(d.get("x", 0), d.get("y", 0))
        s = d.get("size", cls.DEFAULT_SIZE)
        k.setRect(0, 0, s, s)
        k.label = d.get("label", "Knob")
        k._value = d.get("value", 0.0)
        k.action_type = d.get("action_type", "channel")
        k.action_data = d.get("action_data", {"universe": 1, "channel": 1})
        return k


# ---------------------------------------------------------------------------
# VCXYPad
# ---------------------------------------------------------------------------

class VCXYPad(QGraphicsRectItem):
    """
    2D XY controller.  Dragging the crosshair sets two channels simultaneously.
    """

    DEFAULT_W = 150
    DEFAULT_H = 150

    def __init__(self, parent: QGraphicsItem | None = None) -> None:
        super().__init__(0, 0, self.DEFAULT_W, self.DEFAULT_H, parent)
        self.label: str = "XY Pad"
        self.x_universe: int = 1
        self.x_channel: int = 1
        self.y_universe: int = 1
        self.y_channel: int = 2
        self._x_val: float = 0.5   # 0.0-1.0
        self._y_val: float = 0.5
        self._edit_mode: bool = False

        self.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsSelectable, True)
        self.setAcceptHoverEvents(True)

    def paint(self, painter: QPainter, option, widget=None) -> None:
        r = self.rect()

        # Background
        painter.setBrush(QBrush(QColor("#0e1520")))
        border_color = QColor(T.C_ACCENT) if self.isSelected() else QColor(T.C_BORDER)
        painter.setPen(QPen(border_color, 1.5))
        painter.drawRect(r)

        # Grid lines (3×3)
        grid_pen = QPen(QColor(T.C_BORDER), 0.5, Qt.PenStyle.DotLine)
        painter.setPen(grid_pen)
        for i in range(1, 3):
            xg = r.left() + r.width() * i / 3
            yg = r.top() + r.height() * i / 3
            painter.drawLine(QPointF(xg, r.top()), QPointF(xg, r.bottom()))
            painter.drawLine(QPointF(r.left(), yg), QPointF(r.right(), yg))

        # Crosshair
        cx = r.left() + self._x_val * r.width()
        cy = r.top() + (1.0 - self._y_val) * r.height()
        cross_pen = QPen(QColor(T.C_ACCENT), 1)
        painter.setPen(cross_pen)
        painter.drawLine(QPointF(cx, r.top()), QPointF(cx, r.bottom()))
        painter.drawLine(QPointF(r.left(), cy), QPointF(r.right(), cy))

        # Dot
        dot_r = 5.0
        painter.setBrush(QBrush(QColor(T.C_ACCENT)))
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawEllipse(QRectF(cx - dot_r, cy - dot_r, dot_r * 2, dot_r * 2))

        # Label
        font = QFont("Segoe UI", 7)
        painter.setFont(font)
        painter.setPen(QColor(T.C_TEXT_LABEL))
        painter.drawText(
            QRectF(r.left(), r.top() + 2, r.width(), 14),
            Qt.AlignmentFlag.AlignCenter,
            self.label,
        )

    def mousePressEvent(self, event) -> None:
        if self._edit_mode:
            super().mousePressEvent(event)
            return
        if event.button() == Qt.MouseButton.LeftButton:
            self._update_from_pos(event.pos())

    def mouseMoveEvent(self, event) -> None:
        if self._edit_mode:
            super().mouseMoveEvent(event)
            return
        if event.buttons() & Qt.MouseButton.LeftButton:
            self._update_from_pos(event.pos())

    def contextMenuEvent(self, event) -> None:
        if self._edit_mode:
            _open_properties(self, event.screenPos())

    def _update_from_pos(self, local_pos: QPointF) -> None:
        r = self.rect()
        xn = max(0.0, min(1.0, (local_pos.x() - r.left()) / r.width()))
        yn = max(0.0, min(1.0, 1.0 - (local_pos.y() - r.top()) / r.height()))
        self._x_val = xn
        self._y_val = yn
        self.update()

        scene = self.scene()
        if not hasattr(scene, "_engine"):
            return
        eng: LumosEngine = scene._engine  # type: ignore[attr-defined]
        eng.set_channel(self.x_universe, self.x_channel, int(xn * 255))
        eng.set_channel(self.y_universe, self.y_channel, int(yn * 255))

    def to_dict(self) -> dict:
        p = self.pos()
        r = self.rect()
        return {
            "type": "xypad",
            "x": p.x(), "y": p.y(),
            "w": r.width(), "h": r.height(),
            "label": self.label,
            "x_universe": self.x_universe,
            "x_channel": self.x_channel,
            "y_universe": self.y_universe,
            "y_channel": self.y_channel,
            "x_val": self._x_val,
            "y_val": self._y_val,
        }

    @classmethod
    def from_dict(cls, d: dict) -> "VCXYPad":
        pad = cls()
        pad.setPos(d.get("x", 0), d.get("y", 0))
        pad.setRect(0, 0, d.get("w", cls.DEFAULT_W), d.get("h", cls.DEFAULT_H))
        pad.label = d.get("label", "XY Pad")
        pad.x_universe = d.get("x_universe", 1)
        pad.x_channel = d.get("x_channel", 1)
        pad.y_universe = d.get("y_universe", 1)
        pad.y_channel = d.get("y_channel", 2)
        pad._x_val = d.get("x_val", 0.5)
        pad._y_val = d.get("y_val", 0.5)
        return pad


# ---------------------------------------------------------------------------
# Properties dialog
# ---------------------------------------------------------------------------

def _open_properties(item: QGraphicsItem, screen_pos) -> None:
    dlg = VCPropertiesDialog(item)
    dlg.move(int(screen_pos.x()), int(screen_pos.y()))
    dlg.exec()


class VCPropertiesDialog(QDialog):
    """Generic properties editor for any VC item."""

    def __init__(self, item: QGraphicsItem, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._item = item
        self.setWindowTitle("Item Properties")
        self.setMinimumWidth(320)

        layout = QFormLayout(self)
        layout.setContentsMargins(12, 12, 12, 12)
        layout.setSpacing(8)

        # Label (all types)
        if hasattr(item, "label"):
            self._label_edit = QLineEdit(item.label)
            self._label_edit.textChanged.connect(lambda t: setattr(item, "label", t))
            layout.addRow("Label:", self._label_edit)

        # Position
        pos = item.pos()
        self._x_spin = QSpinBox()
        self._x_spin.setRange(-10000, 10000)
        self._x_spin.setValue(int(pos.x()))
        self._x_spin.valueChanged.connect(lambda v: item.setX(v))
        layout.addRow("X:", self._x_spin)

        self._y_spin = QSpinBox()
        self._y_spin.setRange(-10000, 10000)
        self._y_spin.setValue(int(pos.y()))
        self._y_spin.valueChanged.connect(lambda v: item.setY(v))
        layout.addRow("Y:", self._y_spin)

        # Size (for rect items)
        if isinstance(item, (VCButton, VCXYPad)):
            r = item.rect()
            self._w_spin = QSpinBox()
            self._w_spin.setRange(20, 1000)
            self._w_spin.setValue(int(r.width()))
            self._w_spin.valueChanged.connect(
                lambda v: item.setRect(0, 0, v, item.rect().height())
            )
            layout.addRow("Width:", self._w_spin)

            self._h_spin = QSpinBox()
            self._h_spin.setRange(20, 1000)
            self._h_spin.setValue(int(r.height()))
            self._h_spin.valueChanged.connect(
                lambda v: item.setRect(0, 0, item.rect().width(), v)
            )
            layout.addRow("Height:", self._h_spin)

        # VCButton action
        if isinstance(item, VCButton):
            self._action_cb = QComboBox()
            for at in ("none", "scene_recall", "channel_set"):
                self._action_cb.addItem(at)
            idx = self._action_cb.findText(item.action_type)
            if idx >= 0:
                self._action_cb.setCurrentIndex(idx)
            self._action_cb.currentTextChanged.connect(
                lambda t: setattr(item, "action_type", t)
            )
            layout.addRow("Action:", self._action_cb)

        # VCXYPad channels
        if isinstance(item, VCXYPad):
            self._xu_spin = QSpinBox()
            self._xu_spin.setRange(1, 64)
            self._xu_spin.setValue(item.x_universe)
            self._xu_spin.valueChanged.connect(lambda v: setattr(item, "x_universe", v))
            layout.addRow("X Universe:", self._xu_spin)

            self._xc_spin = QSpinBox()
            self._xc_spin.setRange(1, 512)
            self._xc_spin.setValue(item.x_channel)
            self._xc_spin.valueChanged.connect(lambda v: setattr(item, "x_channel", v))
            layout.addRow("X Channel:", self._xc_spin)

            self._yu_spin = QSpinBox()
            self._yu_spin.setRange(1, 64)
            self._yu_spin.setValue(item.y_universe)
            self._yu_spin.valueChanged.connect(lambda v: setattr(item, "y_universe", v))
            layout.addRow("Y Universe:", self._yu_spin)

            self._yc_spin = QSpinBox()
            self._yc_spin.setRange(1, 512)
            self._yc_spin.setValue(item.y_channel)
            self._yc_spin.valueChanged.connect(lambda v: setattr(item, "y_channel", v))
            layout.addRow("Y Channel:", self._yc_spin)

        btns = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        btns.rejected.connect(self.accept)
        layout.addRow(btns)


# ---------------------------------------------------------------------------
# VC Scene
# ---------------------------------------------------------------------------

class VCScene(QGraphicsScene):
    """QGraphicsScene that holds a reference to the engine."""

    def __init__(self, engine: LumosEngine, parent=None) -> None:
        super().__init__(parent)
        self._engine = engine
        self.setSceneRect(0, 0, 3000, 2000)
        self.setBackgroundBrush(QBrush(QColor(T.C_BG)))

    def set_edit_mode(self, edit: bool) -> None:
        for item in self.items():
            if hasattr(item, "_edit_mode"):
                item._edit_mode = edit
            if hasattr(item, "setFlag"):
                item.setFlag(
                    QGraphicsItem.GraphicsItemFlag.ItemIsMovable, edit
                )
        self.update()

    def to_dict(self) -> list[dict]:
        result = []
        for item in self.items():
            if hasattr(item, "to_dict"):
                result.append(item.to_dict())
        return result

    def from_dict(self, data: list[dict]) -> None:
        self.clear()
        _loaders = {
            "button": VCButton.from_dict,
            "fader": VCFader.from_dict,
            "knob": VCKnob.from_dict,
            "xypad": VCXYPad.from_dict,
        }
        for d in data:
            t = d.get("type", "")
            if t in _loaders:
                item = _loaders[t](d)
                self.addItem(item)


# ---------------------------------------------------------------------------
# Virtual Console panel
# ---------------------------------------------------------------------------

class VirtualConsolePanel(QWidget):
    """
    The Virtual Console panel: toolbar + QGraphicsView canvas.
    """

    def __init__(self, engine: LumosEngine, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._engine = engine
        self._edit_mode = False

        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        # Toolbar
        toolbar = QWidget()
        toolbar.setFixedHeight(44)
        toolbar.setStyleSheet(f"background: {T.C_PANEL}; border-bottom: 1px solid {T.C_BORDER};")
        tb_lay = QHBoxLayout(toolbar)
        tb_lay.setContentsMargins(8, 4, 8, 4)
        tb_lay.setSpacing(6)

        def _tb_btn(label: str) -> QPushButton:
            b = QPushButton(label)
            b.setFixedHeight(32)
            return b

        btn_add_btn = _tb_btn("Add Button")
        btn_add_btn.clicked.connect(self._add_button)
        tb_lay.addWidget(btn_add_btn)

        btn_add_fdr = _tb_btn("Add Fader")
        btn_add_fdr.clicked.connect(self._add_fader)
        tb_lay.addWidget(btn_add_fdr)

        btn_add_knob = _tb_btn("Add Knob")
        btn_add_knob.clicked.connect(self._add_knob)
        tb_lay.addWidget(btn_add_knob)

        btn_add_xy = _tb_btn("Add XY Pad")
        btn_add_xy.clicked.connect(self._add_xypad)
        tb_lay.addWidget(btn_add_xy)

        tb_lay.addWidget(_VSep())

        self._btn_delete = _tb_btn("Delete Selected")
        self._btn_delete.clicked.connect(self._delete_selected)
        tb_lay.addWidget(self._btn_delete)

        tb_lay.addStretch()

        self._edit_btn = QPushButton("Edit Mode")
        self._edit_btn.setCheckable(True)
        self._edit_btn.setFixedHeight(32)
        self._edit_btn.toggled.connect(self._toggle_edit_mode)
        tb_lay.addWidget(self._edit_btn)

        root.addWidget(toolbar)

        # Canvas
        self._scene = VCScene(engine, self)
        self._view = QGraphicsView(self._scene)
        self._view.setRenderHint(QPainter.RenderHint.Antialiasing)
        self._view.setDragMode(QGraphicsView.DragMode.ScrollHandDrag)
        self._view.setStyleSheet(f"background: {T.C_BG}; border: none;")
        root.addWidget(self._view)

        # Default to run mode
        self._scene.set_edit_mode(False)

    # ------------------------------------------------------------------
    # Toolbar actions
    # ------------------------------------------------------------------

    def _next_pos(self) -> QPointF:
        """Place new items near the top-left, slightly offset from each other."""
        existing = len(self._scene.items())
        col = existing % 6
        row = existing // 6
        return QPointF(20 + col * 120, 20 + row * 100)

    @Slot()
    def _add_button(self) -> None:
        item = VCButton("Button")
        item.setPos(self._next_pos())
        item._edit_mode = self._edit_mode
        item.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsMovable, self._edit_mode)
        self._scene.addItem(item)

    @Slot()
    def _add_fader(self) -> None:
        item = VCFader()
        item.setPos(self._next_pos())
        item._edit_mode = self._edit_mode
        item.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsMovable, self._edit_mode)
        self._scene.addItem(item)

    @Slot()
    def _add_knob(self) -> None:
        item = VCKnob()
        item.setPos(self._next_pos())
        item._edit_mode = self._edit_mode
        item.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsMovable, self._edit_mode)
        self._scene.addItem(item)

    @Slot()
    def _add_xypad(self) -> None:
        item = VCXYPad()
        item.setPos(self._next_pos())
        item._edit_mode = self._edit_mode
        item.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsMovable, self._edit_mode)
        self._scene.addItem(item)

    @Slot()
    def _delete_selected(self) -> None:
        for item in self._scene.selectedItems():
            self._scene.removeItem(item)

    @Slot(bool)
    def _toggle_edit_mode(self, checked: bool) -> None:
        self._edit_mode = checked
        self._scene.set_edit_mode(checked)
        if checked:
            self._view.setDragMode(QGraphicsView.DragMode.RubberBandDrag)
        else:
            self._view.setDragMode(QGraphicsView.DragMode.ScrollHandDrag)

    # ------------------------------------------------------------------
    # Persistence
    # ------------------------------------------------------------------

    def to_dict(self) -> list[dict]:
        return self._scene.to_dict()

    def from_dict(self, data: list[dict]) -> None:
        self._scene.from_dict(data)
        self._scene.set_edit_mode(self._edit_mode)


# ---------------------------------------------------------------------------
# Small separator helper
# ---------------------------------------------------------------------------

class _VSep(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedSize(1, 28)
        self.setStyleSheet(f"background: {T.C_BORDER};")
