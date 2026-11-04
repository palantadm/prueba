"""
Executor fader — one execution lane with level fader + GO/STOP/FLASH.
Used inside the ExecutorBank at the bottom of the main window.
"""
from __future__ import annotations
from typing import Optional, Callable

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QPushButton,
    QLabel, QSizePolicy, QInputDialog,
)
from PySide6.QtCore import Qt, Signal, QRect, QSize, QTimer
from PySide6.QtGui import (
    QPainter, QPen, QColor, QFont, QLinearGradient,
    QPaintEvent, QMouseEvent, QWheelEvent,
)
from app.ui import theme as T


# ── Compact vertical fader for executor ──────────────────────────────────────

class _ExecSlider(QWidget):
    value_changed = Signal(int)   # 0-255

    _TRACK_W  = 5
    _HANDLE_W = 36
    _HANDLE_H = 14
    _PAD      = 6

    def __init__(self, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self._value = 255
        self._drag  = False
        self.setMinimumHeight(60)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setCursor(Qt.CursorShape.PointingHandCursor)

    @property
    def value(self) -> int:
        return self._value

    @value.setter
    def value(self, v: int) -> None:
        v = max(0, min(255, v))
        if v != self._value:
            self._value = v
            self.update()

    def _track_range(self) -> tuple[int, int]:
        return self._PAD, self.height() - self._PAD

    def _value_to_y(self, v: int) -> int:
        top, bot = self._track_range()
        return bot - int((v / 255.0) * (bot - top))

    def _y_to_value(self, y: int) -> int:
        top, bot = self._track_range()
        return max(0, min(255, int((bot - y) / max(1, bot - top) * 255)))

    def paintEvent(self, event: QPaintEvent) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        cx = w // 2
        top, bot = self._track_range()
        hy = self._value_to_y(self._value)
        pct = self._value / 255.0

        # bg
        p.fillRect(0, 0, w, h, QColor(T.C_PANEL2))

        # track
        tx = cx - self._TRACK_W // 2
        p.setBrush(QColor("#090909"))
        p.setPen(QPen(QColor("#1a1a1a"), 1))
        p.drawRoundedRect(tx, top, self._TRACK_W, bot - top, 2, 2)

        # fill
        if pct > 0:
            fill_h = bot - hy
            r = int(pct * 255)
            g = int(min(200 + pct * 55, 255))
            b = int(min(pct * 100, 100))
            c = QColor(r, g, b)
            p.setBrush(c)
            p.setPen(Qt.PenStyle.NoPen)
            p.drawRoundedRect(tx, hy, self._TRACK_W, fill_h, 2, 2)

        # handle
        hx = cx - self._HANDLE_W // 2
        hy2 = hy - self._HANDLE_H // 2
        p.setBrush(QColor("#383838") if not self._drag else QColor(T.C_ACCENT))
        p.setPen(QPen(QColor("#505050"), 1))
        p.drawRoundedRect(hx, hy2, self._HANDLE_W, self._HANDLE_H, 3, 3)
        p.setPen(QPen(QColor(255, 255, 255, 40), 1))
        p.drawLine(hx + 5, hy, hx + self._HANDLE_W - 5, hy)

    def mousePressEvent(self, e: QMouseEvent) -> None:
        if e.button() == Qt.MouseButton.LeftButton:
            y = int(e.position().y())
            top, bot = self._track_range()
            if top <= y <= bot:
                self._drag = True
                self._emit(self._y_to_value(y))

    def mouseMoveEvent(self, e: QMouseEvent) -> None:
        if self._drag:
            self._emit(self._y_to_value(int(e.position().y())))

    def mouseReleaseEvent(self, e: QMouseEvent) -> None:
        self._drag = False
        self.update()

    def mouseDoubleClickEvent(self, e: QMouseEvent) -> None:
        self._emit(255)

    def wheelEvent(self, e: QWheelEvent) -> None:
        step = 10 if e.modifiers() & Qt.KeyboardModifier.ShiftModifier else 1
        delta = step if e.angleDelta().y() > 0 else -step
        self._emit(self._value + delta)

    def _emit(self, v: int) -> None:
        v = max(0, min(255, v))
        if v != self._value:
            self._value = v
            self.update()
            self.value_changed.emit(v)


# ── Executor fader ────────────────────────────────────────────────────────────

class ExecutorFader(QWidget):
    """
    One execution slot: label + fader + GO + STOP + FLASH.
    go_pressed, stop_pressed, flash_pressed(bool), level_changed(int 0-255)
    """
    go_pressed    = Signal()
    stop_pressed  = Signal()
    flash_pressed = Signal(bool)   # True=down, False=up
    level_changed = Signal(int)    # 0-255

    def __init__(self, number: int = 1, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.number = number
        self._assigned: str = ""   # scene/cue name assigned
        self._flash_held = False
        self._saved_level = 255
        self.setFixedWidth(80)
        self.setMinimumHeight(160)
        self._build()

    # ── Build ─────────────────────────────────────────────────────────

    def _build(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(3, 4, 3, 4)
        root.setSpacing(2)

        # Label
        self._lbl = QLabel(f"EX {self.number}")
        self._lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._lbl.setWordWrap(True)
        self._lbl.setStyleSheet(
            f"color: {T.C_TEXT_DIM}; font-size: 9px; font-weight: 700;"
            f" background: {T.C_PANEL}; padding: 2px; border-radius: 2px;"
            f" max-height: 28px;"
        )
        self._lbl.mouseDoubleClickEvent = lambda e: self._edit_label()
        root.addWidget(self._lbl)

        # Fader
        self._slider = _ExecSlider()
        self._slider.value_changed.connect(self._on_level)
        root.addWidget(self._slider, 1)

        # GO button
        btn_go = QPushButton("▶  GO")
        btn_go.setObjectName("btn_go")
        btn_go.setFixedHeight(36)
        btn_go.clicked.connect(self.go_pressed)
        root.addWidget(btn_go)

        # STOP button
        btn_stop = QPushButton("■  STOP")
        btn_stop.setObjectName("btn_stop")
        btn_stop.setFixedHeight(32)
        btn_stop.clicked.connect(self.stop_pressed)
        root.addWidget(btn_stop)

        # FLASH button (hold = full)
        btn_flash = QPushButton("⚡ FLASH")
        btn_flash.setObjectName("btn_flash")
        btn_flash.setFixedHeight(28)
        btn_flash.pressed.connect(self._flash_down)
        btn_flash.released.connect(self._flash_up)
        root.addWidget(btn_flash)

    # ── Public ────────────────────────────────────────────────────────

    def assign(self, name: str) -> None:
        self._assigned = name
        self._lbl.setText(name or f"EX {self.number}")
        color = T.C_ACCENT if name else T.C_TEXT_DIM
        self._lbl.setStyleSheet(
            f"color: {color}; font-size: 9px; font-weight: 700;"
            f" background: {T.C_PANEL}; padding: 2px; border-radius: 2px; max-height: 28px;"
        )

    @property
    def level(self) -> int:
        return self._slider.value

    @level.setter
    def level(self, v: int) -> None:
        self._slider.value = v

    # ── Slots ─────────────────────────────────────────────────────────

    def _on_level(self, v: int) -> None:
        self.level_changed.emit(v)

    def _flash_down(self) -> None:
        self._saved_level = self._slider.value
        self._slider.value = 255
        self.level_changed.emit(255)
        self.flash_pressed.emit(True)

    def _flash_up(self) -> None:
        self._slider.value = self._saved_level
        self.level_changed.emit(self._saved_level)
        self.flash_pressed.emit(False)

    def _edit_label(self) -> None:
        text, ok = QInputDialog.getText(
            self, "Rename executor", "Name:", text=self._assigned or f"EX {self.number}"
        )
        if ok:
            self.assign(text.strip())
