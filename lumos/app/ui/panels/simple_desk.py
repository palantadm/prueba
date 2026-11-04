"""
Simple Desk — direct DMX channel faders, touch-optimised.
Groups of 8 faders, 48 per page.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QScrollArea,
    QPushButton, QLabel, QComboBox, QFrame, QSizePolicy,
)
from PySide6.QtCore import Qt, Slot

from app.engine import LumosEngine
from app.ui.widgets.fader import Fader
from app.ui import theme as T

_PER_ROW  = 24
_ROWS     = 2
_PAGE_SZ  = _PER_ROW * _ROWS   # 48
_GROUP_SZ = 8                   # visual separator every 8 faders


class SimpleDesk(QWidget):
    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self._universe = 1
        self._page = 0
        self._faders: list[Fader] = []
        self._build_ui()
        self._build_faders()
        engine.dmx_updated.connect(self._on_dmx_updated)

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 6)
        root.setSpacing(6)

        # ── Top bar ────────────────────────────────────────────────────
        bar = QHBoxLayout()
        bar.setSpacing(8)

        lbl_u = QLabel("UNIVERSE")
        lbl_u.setStyleSheet(
            f"color: {T.C_TEXT_LABEL}; font-size: 10px; font-weight: 800; letter-spacing: 1px;"
        )
        bar.addWidget(lbl_u)

        self._uni_combo = QComboBox()
        self._uni_combo.setFixedWidth(160)
        self._refresh_universes()
        self._uni_combo.currentIndexChanged.connect(self._on_universe_changed)
        bar.addWidget(self._uni_combo)

        bar.addSpacing(20)

        # Page nav
        lbl_pg = QLabel("PAGE")
        lbl_pg.setStyleSheet(
            f"color: {T.C_TEXT_LABEL}; font-size: 10px; font-weight: 800; letter-spacing: 1px;"
        )
        bar.addWidget(lbl_pg)

        self._btn_prev = QPushButton("◀")
        self._btn_prev.setFixedSize(44, 36)
        self._btn_prev.clicked.connect(self._prev_page)
        bar.addWidget(self._btn_prev)

        self._page_lbl = QLabel("1 – 48")
        self._page_lbl.setStyleSheet(
            f"color: {T.C_TEXT_HDR}; font-weight: 700; min-width: 60px;"
        )
        self._page_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        bar.addWidget(self._page_lbl)

        self._btn_next = QPushButton("▶")
        self._btn_next.setFixedSize(44, 36)
        self._btn_next.clicked.connect(self._next_page)
        bar.addWidget(self._btn_next)

        bar.addStretch()

        btn_zero = QPushButton("⬛  All to Zero")
        btn_zero.setFixedHeight(36)
        btn_zero.clicked.connect(self._all_zero)
        bar.addWidget(btn_zero)

        btn_full = QPushButton("⬜  All to Full")
        btn_full.setFixedHeight(36)
        btn_full.clicked.connect(self._all_full)
        bar.addWidget(btn_full)

        root.addLayout(bar)

        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.HLine)
        sep.setStyleSheet(f"color: {T.C_BORDER};")
        root.addWidget(sep)

        # ── Fader grid ─────────────────────────────────────────────────
        self._fader_container = QWidget()
        self._fader_container.setStyleSheet(f"background: {T.C_BG};")
        self._fader_h_layout = QHBoxLayout(self._fader_container)
        self._fader_h_layout.setContentsMargins(2, 4, 2, 4)
        self._fader_h_layout.setSpacing(0)

        scroll = QScrollArea()
        scroll.setWidget(self._fader_container)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        scroll.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setStyleSheet("border: none;")
        root.addWidget(scroll, 1)

    def _build_faders(self) -> None:
        for f in self._faders:
            f.deleteLater()
        self._faders.clear()

        while self._fader_h_layout.count():
            item = self._fader_h_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        start_ch = self._page * _PAGE_SZ + 1
        group_widget: Optional[QWidget] = None
        group_layout: Optional[QHBoxLayout] = None
        group_count = 0

        for i in range(_PAGE_SZ):
            ch = start_ch + i
            if ch > 512:
                break

            if i % _GROUP_SZ == 0:
                # New group block with subtle separator
                group_widget = QWidget()
                group_widget.setStyleSheet(
                    f"background: {T.C_PANEL}; border-radius: 3px; margin: 1px;"
                )
                group_layout = QHBoxLayout(group_widget)
                group_layout.setContentsMargins(3, 2, 3, 2)
                group_layout.setSpacing(2)
                self._fader_h_layout.addWidget(group_widget)

                # Group number label at top
                grp_lbl_w = QWidget()
                grp_lbl_l = QVBoxLayout(grp_lbl_w)
                grp_lbl_l.setContentsMargins(0, 0, 0, 0)
                grp_lbl_l.setSpacing(0)
                g_lbl = QLabel(f"{ch}–{min(ch + _GROUP_SZ - 1, 512)}")
                g_lbl.setStyleSheet(
                    f"color: {T.C_TEXT_LABEL}; font-size: 8px; font-weight: 700;"
                    f" letter-spacing: 1px; padding: 0 2px; border: none; background: transparent;"
                )
                g_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
                grp_lbl_l.addWidget(g_lbl)
                grp_lbl_l.addStretch()
                if group_layout:
                    group_layout.addWidget(grp_lbl_w)

            v = self.engine.get_channel(self._universe, ch)
            fader = Fader(channel=ch, value=v, universe=self._universe,
                          parent=group_widget)
            fader.value_changed.connect(self._on_fader_moved)
            if group_layout:
                group_layout.addWidget(fader)
            self._faders.append(fader)

        self._fader_h_layout.addStretch()

        end_ch = min(start_ch + _PAGE_SZ - 1, 512)
        self._page_lbl.setText(f"{start_ch} – {end_ch}")
        self._btn_prev.setEnabled(self._page > 0)
        self._btn_next.setEnabled(end_ch < 512)

    # ── Slots ─────────────────────────────────────────────────────────

    def _on_fader_moved(self, channel: int, value: int) -> None:
        self.engine.set_channel(self._universe, channel, value)

    @Slot(int, bytes)
    def _on_dmx_updated(self, uid: int, data: bytes) -> None:
        if uid != self._universe:
            return
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
        if (self._page + 1) * _PAGE_SZ < 512:
            self._page += 1
            self._build_faders()

    def _all_zero(self) -> None:
        self.engine.zero_universe(self._universe)

    def _all_full(self) -> None:
        start = self._page * _PAGE_SZ + 1
        for fader in self._faders:
            fader.set_value_silent(255)
            self.engine.set_channel(self._universe, fader.channel, 255)

    def _refresh_universes(self) -> None:
        self._uni_combo.clear()
        for uid, u in self.engine.universes.items():
            self._uni_combo.addItem(f"Universe {uid}  —  {u.name}", uid)
