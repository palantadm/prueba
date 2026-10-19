"""
Lumos main window.
Flat navigation: mode buttons at the top, no nested menus to hunt through.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QPushButton, QLabel, QSlider, QStackedWidget,
    QSizePolicy, QStatusBar, QFrame, QSpacerItem,
    QFileDialog, QMessageBox,
)
from PySide6.QtCore import Qt, QSize, QTimer, Signal, Slot
from PySide6.QtGui import QFont, QColor, QIcon, QKeySequence, QShortcut

from app.engine import LumosEngine
from app.ui.panels.simple_desk import SimpleDesk
from app.ui.panels.dmx_monitor import DMXMonitor
from app.ui.panels.patch_panel import PatchPanel
from app.ui import theme as T


# ── Mode button ───────────────────────────────────────────────────────────────

class ModeButton(QPushButton):
    """Top-level mode selector button."""

    def __init__(self, text: str, parent: Optional[QWidget] = None) -> None:
        super().__init__(text, parent)
        self.setCheckable(True)
        self.setFixedHeight(38)
        self.setMinimumWidth(90)
        self.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))
        self._update_style(False)

    def setChecked(self, checked: bool) -> None:
        super().setChecked(checked)
        self._update_style(checked)

    def _update_style(self, active: bool) -> None:
        if active:
            self.setStyleSheet(f"""
                QPushButton {{
                    background-color: {T.C_ACCENT};
                    color: white;
                    border: none;
                    border-bottom: 3px solid #42a5f5;
                    border-radius: 0;
                    padding: 0 16px;
                }}
            """)
        else:
            self.setStyleSheet(f"""
                QPushButton {{
                    background-color: transparent;
                    color: {T.C_TEXT_DIM};
                    border: none;
                    border-bottom: 3px solid transparent;
                    border-radius: 0;
                    padding: 0 16px;
                }}
                QPushButton:hover {{
                    color: {T.C_TEXT};
                    background-color: {T.C_BG_WIDGET};
                }}
            """)


# ── Grand master slider ───────────────────────────────────────────────────────

class GrandMasterWidget(QWidget):
    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        layout = QHBoxLayout(self)
        layout.setContentsMargins(8, 0, 8, 0)
        layout.setSpacing(6)

        lbl = QLabel("GM")
        lbl.setStyleSheet(f"color: {T.C_AMBER}; font-weight: 700; font-size: 10px;")
        layout.addWidget(lbl)

        self._slider = QSlider(Qt.Orientation.Horizontal)
        self._slider.setRange(0, 255)
        self._slider.setValue(255)
        self._slider.setFixedWidth(100)
        self._slider.setToolTip("Grand Master (Ctrl+G to toggle)")
        self._slider.valueChanged.connect(self._on_changed)
        layout.addWidget(self._slider)

        self._val_lbl = QLabel("100%")
        self._val_lbl.setStyleSheet(f"color: {T.C_AMBER}; font-weight: 700; font-size: 11px; min-width: 36px;")
        layout.addWidget(self._val_lbl)

        engine.grand_master_changed.connect(self._on_engine_changed)

    def _on_changed(self, v: int) -> None:
        pct = int(v / 255 * 100)
        self._val_lbl.setText(f"{pct}%")
        self.engine.grand_master = v / 255.0

    @Slot(float)
    def _on_engine_changed(self, gm: float) -> None:
        v = int(gm * 255)
        self._slider.blockSignals(True)
        self._slider.setValue(v)
        self._slider.blockSignals(False)
        self._val_lbl.setText(f"{int(gm*100)}%")


# ── Main window ───────────────────────────────────────────────────────────────

class MainWindow(QMainWindow):
    def __init__(self, engine: LumosEngine) -> None:
        super().__init__()
        self.engine = engine
        self.setWindowTitle("Lumos — Theatrical Lighting Control")
        self.setMinimumSize(1100, 660)
        self.resize(1280, 800)

        self._build_ui()
        self._build_shortcuts()
        self._select_mode(0)

        # Blackout blink timer
        self._blink = QTimer(self)
        self._blink.setInterval(500)
        self._blink.timeout.connect(self._blink_blackout)
        self._blink_state = False

        engine.blackout_changed.connect(self._on_blackout_changed)
        engine.dmx_updated.connect(self._on_dmx_tick)

    # ------------------------------------------------------------------
    # UI construction
    # ------------------------------------------------------------------

    def _build_ui(self) -> None:
        central = QWidget()
        self.setCentralWidget(central)
        root = QVBoxLayout(central)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        # ── Header bar ────────────────────────────────────────────────
        header = self._build_header()
        root.addWidget(header)

        # Thin separator
        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.HLine)
        sep.setStyleSheet(f"color: {T.C_BORDER};")
        root.addWidget(sep)

        # ── Stack ─────────────────────────────────────────────────────
        self._stack = QStackedWidget()
        root.addWidget(self._stack, 1)

        # Pages
        self._desk = SimpleDesk(self.engine)
        self._patch = PatchPanel(self.engine)
        self._monitor = DMXMonitor(self.engine)
        self._stack.addWidget(self._desk)    # index 0
        self._stack.addWidget(self._patch)   # index 1
        self._stack.addWidget(self._monitor) # index 2

        # ── Status bar ────────────────────────────────────────────────
        self._build_status()

    def _build_header(self) -> QWidget:
        bar = QWidget()
        bar.setFixedHeight(42)
        bar.setStyleSheet(f"background-color: {T.C_BG_PANEL}; border-bottom: 1px solid {T.C_BORDER};")
        layout = QHBoxLayout(bar)
        layout.setContentsMargins(8, 0, 8, 0)
        layout.setSpacing(0)

        # Logo/title
        logo = QLabel("⬡ LUMOS")
        logo.setStyleSheet(f"color: {T.C_TEXT_HDR}; font-size: 14px; font-weight: 900; letter-spacing: 3px; padding-right: 16px; border-right: 1px solid {T.C_BORDER};")
        layout.addWidget(logo)

        # Mode buttons
        modes = [("🎚  Simple Desk", 0), ("🔌  Patch", 1), ("📊  DMX Monitor", 2)]
        self._mode_btns: list[ModeButton] = []
        btn_group_widget = QWidget()
        btn_group_widget.setStyleSheet("background: transparent;")
        btn_layout = QHBoxLayout(btn_group_widget)
        btn_layout.setContentsMargins(0, 0, 0, 0)
        btn_layout.setSpacing(0)

        for label, idx in modes:
            btn = ModeButton(label)
            btn.clicked.connect(lambda checked, i=idx: self._select_mode(i))
            btn_layout.addWidget(btn)
            self._mode_btns.append(btn)

        layout.addWidget(btn_group_widget)
        layout.addStretch()

        # Grand master
        self._gm_widget = GrandMasterWidget(self.engine)
        layout.addWidget(self._gm_widget)

        # Separator
        sep2 = QFrame()
        sep2.setFrameShape(QFrame.Shape.VLine)
        sep2.setStyleSheet(f"color: {T.C_BORDER};")
        layout.addWidget(sep2)

        # Blackout button
        self._btn_blackout = QPushButton("⬛  BLACKOUT")
        self._btn_blackout.setObjectName("btn_blackout")
        self._btn_blackout.setCheckable(True)
        self._btn_blackout.setFixedHeight(34)
        self._btn_blackout.setMinimumWidth(110)
        self._btn_blackout.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))
        self._btn_blackout.toggled.connect(self._on_blackout_btn)
        layout.addWidget(self._btn_blackout)

        return bar

    def _build_status(self) -> None:
        sb = QStatusBar()
        sb.setFixedHeight(24)
        self.setStatusBar(sb)

        self._status_universes: dict[int, QLabel] = {}
        for uid in self.engine.universes:
            lbl = QLabel(f"U{uid}: 0%")
            lbl.setObjectName("status_universe")
            sb.addPermanentWidget(lbl)
            self._status_universes[uid] = lbl

        self._status_gm = QLabel("GM: 100%")
        self._status_gm.setObjectName("status_gm")
        sb.addPermanentWidget(self._status_gm)

        sb.showMessage("Ready  —  Press F1 for help")

    # ------------------------------------------------------------------
    # Shortcuts
    # ------------------------------------------------------------------

    def _build_shortcuts(self) -> None:
        QShortcut(QKeySequence("F1"), self).activated.connect(self._show_help)
        QShortcut(QKeySequence("Ctrl+B"), self).activated.connect(self._toggle_blackout)
        QShortcut(QKeySequence("1"), self).activated.connect(lambda: self._select_mode(0))
        QShortcut(QKeySequence("2"), self).activated.connect(lambda: self._select_mode(1))
        QShortcut(QKeySequence("3"), self).activated.connect(lambda: self._select_mode(2))

    # ------------------------------------------------------------------
    # Slots
    # ------------------------------------------------------------------

    def _select_mode(self, index: int) -> None:
        self._stack.setCurrentIndex(index)
        for i, btn in enumerate(self._mode_btns):
            btn.setChecked(i == index)

    def _on_blackout_btn(self, checked: bool) -> None:
        self.engine.blackout = checked

    def _toggle_blackout(self) -> None:
        self._btn_blackout.setChecked(not self._btn_blackout.isChecked())

    @Slot(bool)
    def _on_blackout_changed(self, on: bool) -> None:
        self._btn_blackout.blockSignals(True)
        self._btn_blackout.setChecked(on)
        self._btn_blackout.blockSignals(False)
        if on:
            self._blink.start()
        else:
            self._blink.stop()
            self._btn_blackout.setStyleSheet("")  # reset to QSS
            self.statusBar().showMessage("Blackout OFF")

    def _blink_blackout(self) -> None:
        self._blink_state = not self._blink_state
        color = T.C_RED if self._blink_state else "#3a0000"
        self._btn_blackout.setStyleSheet(
            f"QPushButton#btn_blackout {{ background-color: {color}; color: white; "
            f"border: 2px solid {T.C_RED}; font-weight: 700; letter-spacing: 1px; }}"
        )

    @Slot(int, bytes)
    def _on_dmx_tick(self, uid: int, data: bytes) -> None:
        if uid in self._status_universes:
            peak = max(data) if data else 0
            pct = int(peak / 255 * 100)
            self._status_universes[uid].setText(f"U{uid}: {pct}%")

    def _show_help(self) -> None:
        QMessageBox.information(
            self, "Lumos — Keyboard shortcuts",
            "1 / 2 / 3  →  Switch mode (Desk / Patch / Monitor)\n"
            "Ctrl+B       →  Toggle Blackout\n"
            "F1           →  This help\n\n"
            "Fader:  double-click = zero\n"
            "          scroll wheel = ±1  (Shift = ±10)\n"
            "          right-click = set value",
        )
