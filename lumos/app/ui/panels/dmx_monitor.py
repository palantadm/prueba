"""
DMX Monitor — real-time 512-channel grid.
Touch-optimised cell size, MA3-style colour mapping.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QComboBox,
    QScrollArea, QFrame,
)
from PySide6.QtCore import Qt, QRect, QSize, Signal, Slot
from PySide6.QtGui import (
    QPainter, QPen, QColor, QFont, QPaintEvent, QMouseEvent,
    QWheelEvent,
)

from app.engine import LumosEngine
from app.ui import theme as T

_CELL_W = 48
_CELL_H = 40
_COLS   = 16


def _cell_bg(v: int) -> QColor:
    if v == 0:
        return QColor("#101010")
    t = v / 255.0
    if t < 0.3:
        s = t / 0.3
        return QColor(int(s * 5), int(s * 40), int(10 + s * 100))
    elif t < 0.6:
        s = (t - 0.3) / 0.3
        return QColor(int(5 + s * 10), int(40 + s * 110), int(110 + s * 80))
    elif t < 0.85:
        s = (t - 0.6) / 0.25
        return QColor(int(15 + s * 60), int(150 + s * 60), int(190 + s * 40))
    else:
        s = (t - 0.85) / 0.15
        return QColor(int(75 + s * 180), int(210 + s * 45), min(230 + int(s * 25), 255))


def _cell_fg(v: int) -> QColor:
    return QColor("#ffffff") if v > 80 else QColor("#505050")


class _DMXGrid(QWidget):
    channel_clicked = Signal(int, int)  # channel (1-indexed), value

    def __init__(self, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self._data   = bytearray(512)
        self._hover  = -1
        self._scale  = 1.0
        self._font_ch  = QFont("Consolas", 8)
        self._font_val = QFont("Consolas", 11, QFont.Weight.Bold)
        self.setMouseTracking(True)

    def update_universe(self, data: bytes) -> None:
        self._data = bytearray(data[:512])
        self.update()

    def sizeHint(self) -> QSize:
        rows = (512 + _COLS - 1) // _COLS
        cw = int(_CELL_W * self._scale)
        ch = int(_CELL_H * self._scale)
        return QSize(_COLS * cw, rows * ch)

    def paintEvent(self, event: QPaintEvent) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing, False)
        cw = int(_CELL_W * self._scale)
        ch_h = int(_CELL_H * self._scale)

        for i in range(512):
            v = self._data[i]
            col = i % _COLS
            row = i // _COLS
            x = col * cw
            y = row * ch_h

            bg = _cell_bg(v)
            if i == self._hover:
                bg = bg.lighter(170)

            p.fillRect(x, y, cw - 1, ch_h - 1, bg)

            fg = _cell_fg(v)
            p.setPen(fg)
            p.setFont(self._font_ch)
            p.drawText(QRect(x, y + 1, cw - 1, 14),
                       Qt.AlignmentFlag.AlignHCenter, str(i + 1))
            p.setFont(self._font_val)
            p.drawText(QRect(x, y + 14, cw - 1, ch_h - 14 - 2),
                       Qt.AlignmentFlag.AlignHCenter | Qt.AlignmentFlag.AlignVCenter,
                       str(v))

        # Grid lines
        p.setPen(QPen(QColor("#080808"), 1))
        rows = (512 + _COLS - 1) // _COLS
        for c in range(_COLS + 1):
            p.drawLine(c * cw, 0, c * cw, rows * ch_h)
        for r in range(rows + 1):
            p.drawLine(0, r * ch_h, _COLS * cw, r * ch_h)

    def mouseMoveEvent(self, event: QMouseEvent) -> None:
        cw = int(_CELL_W * self._scale)
        ch = int(_CELL_H * self._scale)
        x, y = int(event.position().x()), int(event.position().y())
        col, row = x // cw, y // ch
        i = row * _COLS + col
        hover = i if 0 <= i < 512 else -1
        if hover != self._hover:
            self._hover = hover
            self.update()

    def leaveEvent(self, event) -> None:
        self._hover = -1
        self.update()

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if 0 <= self._hover < 512:
            self.channel_clicked.emit(self._hover + 1, self._data[self._hover])

    def wheelEvent(self, event: QWheelEvent) -> None:
        if event.modifiers() & Qt.KeyboardModifier.ControlModifier:
            factor = 1.1 if event.angleDelta().y() > 0 else 0.9
            self._scale = max(0.5, min(2.5, self._scale * factor))
            self.updateGeometry()
            self.update()
        else:
            super().wheelEvent(event)


class DMXMonitor(QWidget):
    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self._universe = 1
        self._build_ui()
        engine.dmx_updated.connect(self._on_dmx_updated)

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(6)

        bar = QHBoxLayout()
        lbl = QLabel("UNIVERSE")
        lbl.setStyleSheet(
            f"color: {T.C_TEXT_LABEL}; font-size: 10px; font-weight: 800; letter-spacing: 1px;"
        )
        bar.addWidget(lbl)

        self._combo = QComboBox()
        self._combo.setFixedWidth(180)
        for uid, u in self.engine.universes.items():
            self._combo.addItem(f"Universe {uid}  —  {u.name}", uid)
        self._combo.currentIndexChanged.connect(self._on_universe_changed)
        bar.addWidget(self._combo)

        bar.addStretch()

        hint = QLabel("Ctrl+Scroll to zoom")
        hint.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 10px;")
        bar.addWidget(hint)

        # Colour legend
        for val, label in [(0, "0"), (64, "64"), (128, "128"), (192, "192"), (255, "255")]:
            c = _cell_bg(val)
            dot = QLabel(f" {label} ")
            dot.setStyleSheet(
                f"background: {c.name()};"
                f" color: {'white' if val > 80 else '#606060'};"
                f" padding: 3px 6px; border-radius: 3px; font-size: 10px; font-weight: 700;"
            )
            bar.addWidget(dot)

        root.addLayout(bar)

        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.HLine)
        sep.setStyleSheet(f"color: {T.C_BORDER};")
        root.addWidget(sep)

        self._grid = _DMXGrid()

        scroll = QScrollArea()
        scroll.setWidget(self._grid)
        scroll.setWidgetResizable(False)
        scroll.setStyleSheet(f"border: none; background: {T.C_BG};")
        root.addWidget(scroll, 1)

    @Slot(int, bytes)
    def _on_dmx_updated(self, uid: int, data: bytes) -> None:
        if uid == self._universe:
            self._grid.update_universe(data)

    def _on_universe_changed(self, index: int) -> None:
        uid = self._combo.itemData(index)
        if uid is not None:
            self._universe = uid
