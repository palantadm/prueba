"""
Vertical DMX fader — touch-first design, MA3-inspired colour gradient.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import QWidget, QSizePolicy, QInputDialog
from PySide6.QtCore import Qt, Signal, QRect, QSize, QPoint
from PySide6.QtGui import (
    QPainter, QPen, QBrush, QColor, QFont, QLinearGradient,
    QPaintEvent, QMouseEvent, QWheelEvent,
)
from app.ui import theme as T


def _value_color(v: int) -> QColor:
    """Gradient: near-black → dark blue → cyan → white  (MA3 style)."""
    t = v / 255.0
    if t < 0.4:
        s = t / 0.4
        return QColor(int(s * 10), int(s * 80), int(30 + s * 150))
    elif t < 0.75:
        s = (t - 0.4) / 0.35
        return QColor(int(10 + s * 20), int(80 + s * 120), int(180 + s * 55))
    else:
        s = (t - 0.75) / 0.25
        r = int(30 + s * 225)
        g = int(200 + s * 55)
        b = int(235 + s * 20)
        return QColor(min(r, 255), min(g, 255), min(b, 255))


class Fader(QWidget):
    """
    Single-channel touch fader.
    value_changed(channel: int, value: int)
    """
    value_changed = Signal(int, int)

    _TRACK_W   = 6
    _HANDLE_W  = 40
    _HANDLE_H  = 18
    _LED_W     = 4
    _PAD_TOP   = 30
    _PAD_BOT   = 22

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

        self.setMinimumSize(52, 160)
        self.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Expanding)
        self.setToolTip(f"Ch {channel}  –  dbl-click=zero  wheel=±1  shift+wheel=±10")
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setAttribute(Qt.WidgetAttribute.WA_AcceptTouchEvents, True)

    # ── Public API ────────────────────────────────────────────────────

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
        self.value = v

    def set_label(self, text: str) -> None:
        self._label = text
        self.update()

    # ── Geometry ──────────────────────────────────────────────────────

    def _track_range(self) -> tuple[int, int]:
        return self._PAD_TOP, self.height() - self._PAD_BOT

    def _value_to_y(self, v: int) -> int:
        top, bot = self._track_range()
        return bot - int((v / 255.0) * (bot - top))

    def _y_to_value(self, y: int) -> int:
        top, bot = self._track_range()
        v = (bot - y) / max(1, bot - top) * 255
        return max(0, min(255, int(v)))

    def sizeHint(self) -> QSize:
        return QSize(52, 200)

    # ── Paint ─────────────────────────────────────────────────────────

    def paintEvent(self, event: QPaintEvent) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        cx = w // 2
        top, bot = self._track_range()
        handle_y = self._value_to_y(self._value)
        col = _value_color(self._value)

        # Background
        p.fillRect(0, 0, w, h, QColor(T.C_PANEL))

        # Scale ticks (every 64)
        p.setPen(QPen(QColor("#252525"), 1))
        for step in range(0, 256, 64):
            my = self._value_to_y(step)
            p.drawLine(cx - 12, my, cx - self._TRACK_W // 2 - 2, my)

        # Track groove
        tx = cx - self._TRACK_W // 2
        p.setBrush(QColor("#0a0a0a"))
        p.setPen(QPen(QColor("#1e1e1e"), 1))
        p.drawRoundedRect(tx, top, self._TRACK_W, bot - top, 3, 3)

        # Value fill
        if self._value > 0:
            fill_h = bot - handle_y
            if fill_h > 0:
                grad = QLinearGradient(0, handle_y, 0, bot)
                grad.setColorAt(0.0, col)
                grad.setColorAt(1.0, QColor(0, 10, 20))
                p.setBrush(grad)
                p.setPen(Qt.PenStyle.NoPen)
                p.drawRoundedRect(tx, handle_y, self._TRACK_W, fill_h, 3, 3)

        # LED strip (right side)
        led_x = cx + self._TRACK_W // 2 + 4
        if self._value > 0:
            led_grad = QLinearGradient(0, bot, 0, handle_y)
            led_grad.setColorAt(0.0, QColor(0, 20, 30))
            led_grad.setColorAt(1.0, col)
            p.setBrush(led_grad)
            p.setPen(Qt.PenStyle.NoPen)
            p.drawRoundedRect(led_x, handle_y, self._LED_W, bot - handle_y, 2, 2)
        else:
            p.fillRect(led_x, top, self._LED_W, bot - top, QColor("#0a0a0a"))

        # Handle
        hw, hh = self._HANDLE_W, self._HANDLE_H
        hx = cx - hw // 2
        hy = handle_y - hh // 2

        handle_col = col.lighter(120) if self._dragging else QColor("#2e3840")
        p.setBrush(handle_col)
        p.setPen(QPen(col.lighter(160) if self._value > 0 else QColor("#3a3a3a"), 1))
        p.drawRoundedRect(hx, hy, hw, hh, 4, 4)

        # Handle grip lines
        p.setPen(QPen(QColor(255, 255, 255, 50), 1))
        for dy in (-3, 0, 3):
            p.drawLine(hx + 6, handle_y + dy, hx + hw - 6, handle_y + dy)

        # Value label (top)
        p.setPen(col if self._value > 0 else QColor("#3a3a3a"))
        p.setFont(QFont("Consolas", 10, QFont.Weight.Bold))
        pct = int(self._value / 255 * 100)
        p.drawText(QRect(0, 4, w, 18), Qt.AlignmentFlag.AlignHCenter, f"{pct:3d}%")

        # Channel label (bottom)
        p.setPen(QColor(T.C_TEXT_DIM))
        p.setFont(QFont("Segoe UI", 8))
        p.drawText(QRect(0, h - 18, w, 16), Qt.AlignmentFlag.AlignHCenter, self._label)

    # ── Mouse / Touch ─────────────────────────────────────────────────

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            y = int(event.position().y())
            top, bot = self._track_range()
            if top <= y <= bot:
                self._dragging = True
                self._set(self._y_to_value(y))
        elif event.button() == Qt.MouseButton.RightButton:
            self._context_menu(event)

    def mouseMoveEvent(self, event: QMouseEvent) -> None:
        if self._dragging:
            self._set(self._y_to_value(int(event.position().y())))

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:
        self._dragging = False
        self.update()

    def mouseDoubleClickEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            self._set(0)

    def wheelEvent(self, event: QWheelEvent) -> None:
        step = 10 if event.modifiers() & Qt.KeyboardModifier.ShiftModifier else 1
        delta = step if event.angleDelta().y() > 0 else -step
        self._set(self._value + delta)

    def _set(self, v: int) -> None:
        v = max(0, min(255, v))
        if v != self._value:
            self._value = v
            self.update()
            self.value_changed.emit(self.channel, v)

    def _context_menu(self, event: QMouseEvent) -> None:
        from PySide6.QtWidgets import QMenu
        m = QMenu(self)
        m.addAction("0%   (zero)",   lambda: self._set(0))
        m.addAction("25%",           lambda: self._set(64))
        m.addAction("50%",           lambda: self._set(128))
        m.addAction("75%",           lambda: self._set(191))
        m.addAction("100% (full)",   lambda: self._set(255))
        m.addSeparator()
        m.addAction("Enter value…",  self._enter_value)
        m.exec(event.globalPosition().toPoint())

    def _enter_value(self) -> None:
        v, ok = QInputDialog.getInt(
            self, "Channel value",
            f"Ch {self.channel}  (0 – 255):",
            self._value, 0, 255,
        )
        if ok:
            self._set(v)
