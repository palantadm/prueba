"""
Simple Desk — direct DMX channel control.
48 faders visible at once, scrollable to 512.
Like the grandMA / Daslight direct control view.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QGridLayout,
    QScrollArea, QPushButton, QLabel, QComboBox,
    QFrame, QSizePolicy,
)
from PySide6.QtCore import Qt, QSize
from PySide6.QtGui import QColor

from app.engine import LumosEngine
from app.ui.widgets.fader import Fader


class SimpleDesk(QWidget):
    """
    Direct DMX fader bank.  24 faders × 2 rows = 48 channels per page.
    Page buttons jump 48 channels at a time.
    """

    FADERS_PER_ROW = 24
    ROWS = 2
    PAGE_SIZE = FADERS_PER_ROW * ROWS   # 48

    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self._universe = 1
        self._page = 0           # 0 = channels 1–48, 1 = 49–96, …
        self._faders: list[Fader] = []
        self._build_ui()
        self._build_faders()
        self._connect_engine()

    # ------------------------------------------------------------------
    # UI construction
    # ------------------------------------------------------------------

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(6)

        # ── Top bar ────────────────────────────────────────────────────
        bar = QHBoxLayout()
        bar.setSpacing(8)

        lbl = QLabel("Universe")
        lbl.setStyleSheet("color: #808080; font-size: 10px; font-weight: 700;")
        bar.addWidget(lbl)

        self._uni_combo = QComboBox()
        self._uni_combo.setFixedWidth(110)
        self._refresh_universes()
        self._uni_combo.currentIndexChanged.connect(self._on_universe_changed)
        bar.addWidget(self._uni_combo)

        bar.addSpacing(16)

        # Page navigation
        lbl2 = QLabel("Page")
        lbl2.setStyleSheet("color: #808080; font-size: 10px; font-weight: 700;")
        bar.addWidget(lbl2)

        self._btn_prev = QPushButton("◀")
        self._btn_prev.setFixedSize(28, 24)
        self._btn_prev.clicked.connect(self._prev_page)
        bar.addWidget(self._btn_prev)

        self._page_lbl = QLabel("1–48")
        self._page_lbl.setStyleSheet("color: #d4d4d4; font-weight: 700; min-width: 50px;")
        self._page_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        bar.addWidget(self._page_lbl)

        self._btn_next = QPushButton("▶")
        self._btn_next.setFixedSize(28, 24)
        self._btn_next.clicked.connect(self._next_page)
        bar.addWidget(self._btn_next)

        bar.addStretch()

        # Utility buttons
        btn_zero = QPushButton("⬛ All to Zero")
        btn_zero.setFixedHeight(24)
        btn_zero.clicked.connect(self._all_to_zero)
        bar.addWidget(btn_zero)

        btn_full = QPushButton("⬜ All to Full")
        btn_full.setFixedHeight(24)
        btn_full.clicked.connect(self._all_to_full)
        bar.addWidget(btn_full)

        root.addLayout(bar)

        # Divider
        line = QFrame()
        line.setFrameShape(QFrame.Shape.HLine)
        line.setStyleSheet("color: #464647;")
        root.addWidget(line)

        # ── Fader grid ─────────────────────────────────────────────────
        self._fader_container = QWidget()
        self._fader_container.setStyleSheet("background: #1a1a1a;")
        self._grid = QGridLayout(self._fader_container)
        self._grid.setSpacing(2)
        self._grid.setContentsMargins(4, 4, 4, 4)

        scroll = QScrollArea()
        scroll.setWidget(self._fader_container)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setStyleSheet("border: none;")
        root.addWidget(scroll, 1)

    def _build_faders(self) -> None:
        # Clear existing faders
        for f in self._faders:
            self._grid.removeWidget(f)
            f.deleteLater()
        self._faders.clear()

        start_ch = self._page * self.PAGE_SIZE + 1
        for i in range(self.PAGE_SIZE):
            ch = start_ch + i
            if ch > 512:
                break
            row = i // self.FADERS_PER_ROW
            col = i % self.FADERS_PER_ROW
            v = self.engine.get_channel(self._universe, ch)
            fader = Fader(channel=ch, value=v, universe=self._universe, parent=self._fader_container)
            fader.value_changed.connect(self._on_fader_moved)
            self._grid.addWidget(fader, row, col)
            self._faders.append(fader)

        # Update page label
        end_ch = min(start_ch + self.PAGE_SIZE - 1, 512)
        self._page_lbl.setText(f"{start_ch}–{end_ch}")
        self._btn_prev.setEnabled(self._page > 0)
        self._btn_next.setEnabled(end_ch < 512)

    def _connect_engine(self) -> None:
        self.engine.dmx_updated.connect(self._on_dmx_updated)

    # ------------------------------------------------------------------
    # Slots
    # ------------------------------------------------------------------

    def _on_fader_moved(self, channel: int, value: int) -> None:
        self.engine.set_channel(self._universe, channel, value)

    def _on_dmx_updated(self, uid: int, data: bytes) -> None:
        if uid != self._universe:
            return
        start_ch = self._page * self.PAGE_SIZE + 1
        for fader in self._faders:
            fader.set_value_silent(data[fader.channel - 1])

    def _on_universe_changed(self, index: int) -> None:
        uid = self._uni_combo.itemData(index)
        if uid is not None:
            self._universe = uid
            self._build_faders()

    def _prev_page(self) -> None:
        if self._page > 0:
            self._page -= 1
            self._build_faders()

    def _next_page(self) -> None:
        start = (self._page + 1) * self.PAGE_SIZE + 1
        if start <= 512:
            self._page += 1
            self._build_faders()

    def _all_to_zero(self) -> None:
        self.engine.zero_universe(self._universe)

    def _all_to_full(self) -> None:
        start = self._page * self.PAGE_SIZE + 1
        for fader in self._faders:
            fader.set_value_silent(255)
            self.engine.set_channel(self._universe, fader.channel, 255)

    def _refresh_universes(self) -> None:
        self._uni_combo.clear()
        for uid, u in self.engine.universes.items():
            self._uni_combo.addItem(f"Universe {uid} — {u.name}", uid)
