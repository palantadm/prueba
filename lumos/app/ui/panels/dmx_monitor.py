"""
DMX Monitor — real-time view of all 512 channels in a universe.
Colour-coded cells like QLC+/Daslight.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QComboBox,
)
from PySide6.QtCore import Qt, QRect, QSize, Signal
from PySide6.QtGui import (
    QPainter, QPen, QColor, QFont, QPaintEvent, QMouseEvent,
)

from app.engine import LumosEngine


_CELL_W = 42
_CELL_H = 36
_COLS   = 16   # 16 columns × 32 rows = 512


def _cell_bg(v: int) -> QColor:
    """Black (0) → dark blue → mid blue → cyan-white (255)."""
    if v == 0:
        return QColor("#141414")
    t = v / 255.0
    if t < 0.33:
        s = t / 0.33
        return QColor(int(10 + s * 10), int(15 + s * 40), int(20 + s * 100))
    elif t < 0.66:
        s = (t - 0.33) / 0.33
        return QColor(int(20 + s * 20), int(55 + s * 80), int(120 + s * 80))
    else:
        s = (t - 0.66) / 0.34
        return QColor(int(40 + s * 215), int(135 + s * 120), int(200 + s * 55))


def _cell_fg(v: int) -> QColor:
    return QColor("#ffffff") if v > 100 else QColor("#808080")


class _DMXGrid(QWidget):
    channel_clicked = Signal(int, int)   # channel (1-indexed), value

    def __init__(self, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self._data = bytearray(512)
        self._font_ch  = QFont("Consolas", 7)
        self._font_val = QFont("Consolas", 10, QFont.Weight.Bold)
        self.setMouseTracking(True)
        self._hover = -1

    def update_universe(self, data: bytes) -> None:
        self._data = bytearray(data[:512])
        self.update()

    def sizeHint(self) -> QSize:
        rows = 512 // _COLS
        return QSize(_COLS * _CELL_W, rows * _CELL_H)

    def paintEvent(self, event: QPaintEvent) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing, False)
        for i in range(512):
            v = self._data[i]
            col = i // _COLS
            row = i % _COLS if False else i // _COLS  # recompute correctly
            col = i % _COLS
            row = i // _COLS
            x = col * _CELL_W
            y = row * _CELL_H
            bg = _cell_bg(v)
            if i == self._hover:
                bg = bg.lighter(150)
            p.fillRect(x, y, _CELL_W - 1, _CELL_H - 1, bg)
            fg = _cell_fg(v)
            p.setPen(fg)
            p.setFont(self._font_ch)
            p.drawText(QRect(x, y + 1, _CELL_W - 1, 12),
                       Qt.AlignmentFlag.AlignHCenter, str(i + 1))
            p.setFont(self._font_val)
            p.drawText(QRect(x, y + 14, _CELL_W - 1, 20),
                       Qt.AlignmentFlag.AlignHCenter, str(v))
        # Grid lines
        p.setPen(QPen(QColor("#0a0a0a"), 1))
        for c in range(_COLS + 1):
            p.drawLine(c * _CELL_W, 0, c * _CELL_W, (512 // _COLS) * _CELL_H)

    def mouseMoveEvent(self, event: QMouseEvent) -> None:
        x = int(event.position().x())
        y = int(event.position().y())
        col = x // _CELL_W
        row = y // _CELL_H
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
        lbl = QLabel("Universe")
        lbl.setStyleSheet("color: #808080; font-size: 10px; font-weight: 700;")
        bar.addWidget(lbl)

        self._combo = QComboBox()
        self._combo.setFixedWidth(160)
        for uid, u in self.engine.universes.items():
            self._combo.addItem(f"Universe {uid} — {u.name}", uid)
        self._combo.currentIndexChanged.connect(self._on_universe_changed)
        bar.addWidget(self._combo)

        bar.addStretch()
        # Legend
        for val, txt in [(0, "0"), (64, "64"), (128, "128"), (192, "192"), (255, "255")]:
            c = _cell_bg(val)
            dot = QLabel(f"  {txt}")
            dot.setStyleSheet(
                f"background-color: {c.name()}; color: {'white' if val > 100 else '#888'};"
                f" padding: 2px 4px; border-radius: 3px; font-size: 10px;"
            )
            bar.addWidget(dot)

        root.addLayout(bar)

        from PySide6.QtWidgets import QScrollArea, QFrame
        line = QFrame()
        line.setFrameShape(QFrame.Shape.HLine)
        line.setStyleSheet("color: #464647;")
        root.addWidget(line)

        self._grid = _DMXGrid()
        scroll = QScrollArea()
        scroll.setWidget(self._grid)
        scroll.setWidgetResizable(False)
        scroll.setStyleSheet("border: none; background: #141414;")
        root.addWidget(scroll, 1)

    def _on_dmx_updated(self, uid: int, data: bytes) -> None:
        if uid == self._universe:
            self._grid.update_universe(data)

    def _on_universe_changed(self, index: int) -> None:
        uid = self._combo.itemData(index)
        if uid is not None:
            self._universe = uid
