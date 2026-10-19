"""
Vertical DMX fader widget — custom QPainter rendering.
Looks and behaves like a hardware desk fader.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import QWidget, QSizePolicy, QInputDialog
from PySide6.QtCore import Qt, Signal, QRect, QPoint, QSize
from PySide6.QtGui import (
    QPainter, QPen, QBrush, QColor, QFont,
    QLinearGradient, QPaintEvent, QMouseEvent, QWheelEvent,
)


def _value_color(v: int) -> QColor:
    """Gradient: dark blue (0) → blue (128) → warm amber (255)."""
    t = v / 255.0
    if t < 0.5:
        s = t * 2
        r = int(10 + s * 40)
        g = int(20 + s * 80)
        b = int(60 + s * 140)
    else:
        s = (t - 0.5) * 2
        r = int(50 + s * 205)
        g = int(100 + s * 67)
        b = int(200 - s * 145)
    return QColor(r, g, b)


class Fader(QWidget):
    """
    Single-channel DMX fader.
    Signals:
        value_changed(channel, value)  — channel is 1-indexed
    """

    value_changed = Signal(int, int)   # channel, value (0-255)

    _TRACK_W  = 6
    _HANDLE_W = 32
    _HANDLE_H = 14
    _PAD_TOP  = 28   # space above track for value label
    _PAD_BOT  = 20   # space below track for channel label

    def __init__(
        self,
        channel: int = 1,
        value: int = 0,
        universe: int = 1,
        label: str = "",
        parent: Optional[QWidget] = None,
    ) -> None:
        super().__init__(parent)
        self.channel  = channel
        self.universe = universe
        self._value   = max(0, min(255, value))
        self._label   = label or f"{channel:02d}"
        self._dragging = False
        self._drag_start_y = 0
        self._drag_start_v = 0

        self.setMinimumSize(44, 120)
        self.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Expanding)
        self.setToolTip(f"Ch {channel}  (double-click = zero, wheel = fine)")
        self.setCursor(Qt.CursorShape.PointingHandCursor)

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    @property
    def value(self) -> int:
        return self._value

    @value.setter
    def value(self, v: int) -> None:
        v = max(0, min(255, v))
        if v != self._value:
            self._value = v
            self.update()

    def set_value_silent(self, v: int) -> None:
        """Update display without emitting signal (used for external updates)."""
        self.value = v

    def set_label(self, text: str) -> None:
        self._label = text
        self.update()

    # ------------------------------------------------------------------
    # Geometry helpers
    # ------------------------------------------------------------------

    def _track_rect(self) -> tuple[int, int, int, int]:
        """Returns (x, top, x, bottom) of the fader track centre line."""
        cx = self.width() // 2
        top = self._PAD_TOP
        bot = self.height() - self._PAD_BOT
        return cx, top, cx, bot

    def _track_range(self) -> tuple[int, int]:
        """Pixel range for fader travel."""
        return self._PAD_TOP, self.height() - self._PAD_BOT

    def _value_to_y(self, v: int) -> int:
        top, bot = self._track_range()
        return bot - int((v / 255.0) * (bot - top))

    def _y_to_value(self, y: int) -> int:
        top, bot = self._track_range()
        v = (bot - y) / max(1, bot - top) * 255
        return max(0, min(255, int(v)))

    def sizeHint(self) -> QSize:
        return QSize(44, 160)

    # ------------------------------------------------------------------
    # Paint
    # ------------------------------------------------------------------

    def paintEvent(self, event: QPaintEvent) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w = self.width()
        h = self.height()
        cx = w // 2
        top, bot = self._track_range()
        handle_y = self._value_to_y(self._value)
        col = _value_color(self._value)

        # Background
        p.fillRect(0, 0, w, h, QColor("#1c1c1c"))

        # Track groove
        track_x = cx - self._TRACK_W // 2
        p.setBrush(QColor("#0d0d0d"))
        p.setPen(QPen(QColor("#333333"), 1))
        p.drawRoundedRect(track_x, top, self._TRACK_W, bot - top, 3, 3)

        # Fill above handle (value fill)
        if self._value > 0:
            grad = QLinearGradient(0, handle_y, 0, bot)
            grad.setColorAt(0.0, col)
            grad.setColorAt(1.0, QColor("#0a1020"))
            p.setBrush(grad)
            p.setPen(Qt.PenStyle.NoPen)
            fill_h = bot - handle_y
            if fill_h > 0:
                p.drawRoundedRect(track_x, handle_y, self._TRACK_W, fill_h, 3, 3)

        # Scale marks (every 64 steps)
        p.setPen(QPen(QColor("#2a2a2a"), 1))
        for step in range(0, 256, 64):
            my = self._value_to_y(step)
            p.drawLine(cx - 10, my, cx - self._TRACK_W // 2 - 1, my)

        # Handle
        hw = self._HANDLE_W
        hh = self._HANDLE_H
        hx = cx - hw // 2
        hy = handle_y - hh // 2

        if self._dragging:
            handle_col = col.lighter(130)
        elif self._value > 0:
            handle_col = col
        else:
            handle_col = QColor("#2a3a4a")

        p.setBrush(handle_col)
        p.setPen(QPen(handle_col.lighter(160), 1))
        p.drawRoundedRect(hx, hy, hw, hh, 3, 3)

        # Handle centre line
        p.setPen(QPen(QColor(255, 255, 255, 80), 1))
        p.drawLine(hx + 4, handle_y, hx + hw - 4, handle_y)

        # Value label (top)
        p.setPen(col if self._value > 0 else QColor("#404040"))
        font = QFont("Consolas", 10, QFont.Weight.Bold)
        p.setFont(font)
        pct = int(self._value / 255 * 100)
        p.drawText(QRect(0, 2, w, 18), Qt.AlignmentFlag.AlignHCenter, f"{pct:3d}%")

        # Channel label (bottom)
        p.setPen(QColor("#606060"))
        font2 = QFont("Segoe UI", 8)
        p.setFont(font2)
        p.drawText(QRect(0, h - 18, w, 16), Qt.AlignmentFlag.AlignHCenter, self._label)

    # ------------------------------------------------------------------
    # Mouse events
    # ------------------------------------------------------------------

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            y = int(event.position().y())
            top, bot = self._track_range()
            handle_y = self._value_to_y(self._value)
            # Click anywhere in track area starts drag
            if top <= y <= bot:
                self._dragging = True
                self._drag_start_y = y
                self._drag_start_v = self._value
                v = self._y_to_value(y)
                self._set(v)
        elif event.button() == Qt.MouseButton.RightButton:
            self._context_menu(event)

    def mouseMoveEvent(self, event: QMouseEvent) -> None:
        if self._dragging:
            y = int(event.position().y())
            v = self._y_to_value(y)
            self._set(v)

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:
        self._dragging = False
        self.update()

    def mouseDoubleClickEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            self._set(0)

    def wheelEvent(self, event: QWheelEvent) -> None:
        delta = 1 if event.angleDelta().y() > 0 else -1
        if event.modifiers() & Qt.KeyboardModifier.ShiftModifier:
            delta *= 10
        self._set(self._value + delta)

    def _set(self, v: int) -> None:
        v = max(0, min(255, v))
        if v != self._value:
            self._value = v
            self.update()
            self.value_changed.emit(self.channel, self._value)

    def _context_menu(self, event: QMouseEvent) -> None:
        from PySide6.QtWidgets import QMenu
        menu = QMenu(self)
        menu.addAction("Set to 0%",   lambda: self._set(0))
        menu.addAction("Set to 50%",  lambda: self._set(128))
        menu.addAction("Set to 100%", lambda: self._set(255))
        menu.addSeparator()
        a = menu.addAction("Enter value…")
        a.triggered.connect(self._enter_value)
        menu.exec(event.globalPosition().toPoint())

    def _enter_value(self) -> None:
        v, ok = QInputDialog.getInt(
            self, "Set channel", f"Value for channel {self.channel} (0–255):",
            self._value, 0, 255,
        )
        if ok:
            self._set(v)
