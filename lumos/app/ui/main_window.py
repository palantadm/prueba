"""
Lumos main window.
Flat navigation: mode buttons at the top, no nested menus.
Layout:
  ┌─────────────────────────────────────────────────────┐
  │  Header: logo | modes | GM | Blackout | Screens      │
  ├────────┬────────────────────────────┬────────────────┤
  │ Fixture│  Central stack             │  (right panel) │
  │ Pool   │  LIVE → Programmer         │                │
  │ 220px  │  DESK → SimpleDesk         │                │
  │        │  PATCH → PatchPanel        │                │
  │        │  MONITOR → DMXMonitor      │                │
  ├────────┴────────────────────────────┴────────────────┤
  │  ExecutorBank (172px fixed height)                   │
  └─────────────────────────────────────────────────────┘
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QPushButton, QLabel, QSlider, QStackedWidget,
    QFrame, QSplitter, QMessageBox, QStatusBar,
)
from PySide6.QtCore import Qt, QTimer, Signal, Slot
from PySide6.QtGui import QFont, QKeySequence, QShortcut

from app.engine import LumosEngine
from app.ui.panels.simple_desk import SimpleDesk
from app.ui.panels.dmx_monitor import DMXMonitor
from app.ui.panels.patch_panel import PatchPanel
from app.ui.panels.programmer import Programmer
from app.ui.panels.fixture_pool import FixturePool
from app.ui.panels.executor_bank import ExecutorBank
from app.ui.panels.cuelist_panel import CuelistPanel
from app.ui.panels.show_panel import ShowPanel
from app.ui.screens import ScreenManager
from app.ui import theme as T

# ─── Mode indices ────────────────────────────────────────────────────────────
_MODE_LIVE    = 0
_MODE_DESK    = 1
_MODE_PATCH   = 2
_MODE_MONITOR = 3
_MODE_CUELIST = 4
_MODE_SHOW    = 5


# ─── Mode button ─────────────────────────────────────────────────────────────

class _ModeBtn(QPushButton):
    def __init__(self, label: str, parent: Optional[QWidget] = None) -> None:
        super().__init__(label, parent)
        self.setCheckable(True)
        self.setFixedHeight(40)
        self.setMinimumWidth(100)
        self.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))
        self._paint(False)

    def setChecked(self, on: bool) -> None:
        super().setChecked(on)
        self._paint(on)

    def _paint(self, on: bool) -> None:
        if on:
            self.setStyleSheet(
                f"QPushButton {{ background: {T.C_ACCENT}; color: {T.C_TEXT_HDR};"
                f" border: none; border-bottom: 3px solid {T.C_POSITION};"
                f" border-radius: 0; padding: 0 16px; }}"
            )
        else:
            self.setStyleSheet(
                f"QPushButton {{ background: transparent; color: {T.C_TEXT_DIM};"
                f" border: none; border-bottom: 3px solid transparent;"
                f" border-radius: 0; padding: 0 16px; }}"
                f"QPushButton:hover {{ color: {T.C_TEXT}; background: {T.C_PANEL}; }}"
            )


# ─── Grand master ─────────────────────────────────────────────────────────────

class _GrandMaster(QWidget):
    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        lay = QHBoxLayout(self)
        lay.setContentsMargins(8, 0, 8, 0)
        lay.setSpacing(4)

        lbl = QLabel("GM")
        lbl.setStyleSheet(f"color: {T.C_AMBER}; font-weight: 800; font-size: 10px;")
        lay.addWidget(lbl)

        self._slider = QSlider(Qt.Orientation.Horizontal)
        self._slider.setRange(0, 255)
        self._slider.setValue(255)
        self._slider.setFixedWidth(90)
        self._slider.valueChanged.connect(self._on_slide)
        lay.addWidget(self._slider)

        self._lbl = QLabel("100%")
        self._lbl.setStyleSheet(
            f"color: {T.C_AMBER}; font-weight: 700; font-size: 11px; min-width: 34px;"
        )
        lay.addWidget(self._lbl)

        engine.grand_master_changed.connect(self._on_engine)

    def _on_slide(self, v: int) -> None:
        self._lbl.setText(f"{int(v / 255 * 100)}%")
        self.engine.grand_master = v / 255.0

    @Slot(float)
    def _on_engine(self, gm: float) -> None:
        self._slider.blockSignals(True)
        self._slider.setValue(int(gm * 255))
        self._slider.blockSignals(False)
        self._lbl.setText(f"{int(gm * 100)}%")


# ─── Main window ─────────────────────────────────────────────────────────────

class MainWindow(QMainWindow):
    def __init__(self, engine: LumosEngine) -> None:
        super().__init__()
        self.engine = engine
        self._screen_mgr = ScreenManager()
        self.setWindowTitle("Lumos  ·  Theatrical Lighting Control")
        self.setMinimumSize(1100, 680)
        self.resize(1440, 860)

        self._build_ui()
        self._build_shortcuts()
        self._select_mode(_MODE_LIVE)

        self._blink = QTimer(self)
        self._blink.setInterval(480)
        self._blink.timeout.connect(self._blink_tick)
        self._blink_state = False

        engine.blackout_changed.connect(self._on_blackout_changed)
        engine.dmx_updated.connect(self._on_dmx_tick)
        engine.scene_saved.connect(self._on_scene_saved)

    # ── Construction ─────────────────────────────────────────────────────────

    def _build_ui(self) -> None:
        central = QWidget()
        self.setCentralWidget(central)
        root = QVBoxLayout(central)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        root.addWidget(self._build_header())
        root.addWidget(self._make_hline())

        # ── Body: fixture pool | central | (placeholder) ──────────────
        body = QSplitter(Qt.Orientation.Horizontal)
        body.setChildrenCollapsible(False)
        body.setHandleWidth(4)
        body.setStyleSheet(
            f"QSplitter::handle {{ background: {T.C_BORDER}; }}"
            f"QSplitter::handle:hover {{ background: {T.C_ACCENT}; }}"
        )

        # Left: fixture pool (fixed width, not stretchable)
        self._fixture_pool = FixturePool(self.engine)
        body.addWidget(self._fixture_pool)

        # Centre: stacked panels
        self._stack = QStackedWidget()

        self._programmer = Programmer(self.engine)
        self._desk = SimpleDesk(self.engine)
        self._patch = PatchPanel(self.engine)
        self._monitor = DMXMonitor(self.engine)
        self._cuelist_panel = CuelistPanel(self.engine)
        self._show_panel = ShowPanel(self.engine)

        self._stack.addWidget(self._programmer)      # 0 = LIVE
        self._stack.addWidget(self._desk)             # 1 = DESK
        self._stack.addWidget(self._patch)            # 2 = PATCH
        self._stack.addWidget(self._monitor)          # 3 = MONITOR
        self._stack.addWidget(self._cuelist_panel)    # 4 = CUELIST
        self._stack.addWidget(self._show_panel)       # 5 = SHOW

        body.addWidget(self._stack)

        # Stretch factors: pool stays fixed, centre stretches
        body.setStretchFactor(0, 0)
        body.setStretchFactor(1, 1)

        root.addWidget(body, 1)

        # ── Footer separator ──────────────────────────────────────────
        root.addWidget(self._make_hline())

        # ── Executor bank ─────────────────────────────────────────────
        self._exec_bank = ExecutorBank(self.engine)
        root.addWidget(self._exec_bank)

        # ── Status bar ────────────────────────────────────────────────
        self._build_status()

        # ── Wire signals ──────────────────────────────────────────────
        self._fixture_pool.selection_changed.connect(self._on_selection_changed)
        self._patch.fixtures_changed.connect(self._fixture_pool.refresh_fixtures)
        self._patch.fixtures_changed.connect(self._on_fixtures_changed_for_show)
        self._programmer.scene_recorded.connect(self._on_scene_saved)
        self._cuelist_panel.go_pressed.connect(self._on_cuelist_go)
        self._cuelist_panel.back_pressed.connect(self._on_cuelist_back)

    def _build_header(self) -> QWidget:
        bar = QWidget()
        bar.setFixedHeight(44)
        bar.setStyleSheet(f"background: {T.C_PANEL};")
        lay = QHBoxLayout(bar)
        lay.setContentsMargins(10, 0, 10, 0)
        lay.setSpacing(0)

        # Logo
        logo = QLabel("⬡  LUMOS")
        logo.setStyleSheet(
            f"color: {T.C_TEXT_HDR}; font-size: 15px; font-weight: 900;"
            f" letter-spacing: 3px; padding-right: 14px;"
            f" border-right: 1px solid {T.C_BORDER};"
        )
        lay.addWidget(logo)

        # Mode buttons
        modes = [
            ("⬤  LIVE",     _MODE_LIVE),
            ("🎚  DESK",    _MODE_DESK),
            ("🔌  PATCH",   _MODE_PATCH),
            ("📊  MONITOR", _MODE_MONITOR),
            ("📋  CUES",    _MODE_CUELIST),
            ("💾  SHOW",    _MODE_SHOW),
        ]
        self._mode_btns: list[_ModeBtn] = []
        for label, idx in modes:
            btn = _ModeBtn(label)
            btn.clicked.connect(lambda _checked, i=idx: self._select_mode(i))
            lay.addWidget(btn)
            self._mode_btns.append(btn)

        lay.addStretch()

        # Screens button
        screens = ScreenManager().available_screens()
        if len(screens) > 1:
            btn_scr = QPushButton(f"⊞  Screens ({len(screens)})")
            btn_scr.setFixedHeight(32)
            btn_scr.setStyleSheet(
                f"background: {T.C_PANEL2}; color: {T.C_TEXT_LABEL};"
                f" border: 1px solid {T.C_BORDER}; border-radius: 3px;"
                f" padding: 0 10px; font-size: 10px;"
            )
            btn_scr.clicked.connect(self._open_secondary)
            lay.addWidget(btn_scr)
            lay.addSpacing(8)

        # Grand master
        self._gm = _GrandMaster(self.engine)
        lay.addWidget(self._gm)

        # Separator
        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.VLine)
        sep.setStyleSheet(f"color: {T.C_BORDER};")
        lay.addWidget(sep)
        lay.addSpacing(8)

        # Blackout
        self._btn_bo = QPushButton("⬛  BLACKOUT")
        self._btn_bo.setObjectName("btn_blackout")
        self._btn_bo.setCheckable(True)
        self._btn_bo.setFixedHeight(34)
        self._btn_bo.setMinimumWidth(110)
        self._btn_bo.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))
        self._btn_bo.toggled.connect(lambda on: setattr(self.engine, "blackout", on))
        lay.addWidget(self._btn_bo)

        return bar

    def _build_status(self) -> None:
        sb = QStatusBar()
        sb.setFixedHeight(22)
        self.setStatusBar(sb)

        self._status_u: dict[int, QLabel] = {}
        for uid in self.engine.universes:
            lbl = QLabel(f"  U{uid}: 0%  ")
            lbl.setObjectName("status_universe")
            sb.addPermanentWidget(lbl)
            self._status_u[uid] = lbl

        self._status_scene_lbl = QLabel("  No scenes  ")
        self._status_scene_lbl.setObjectName("status_scene")
        sb.addPermanentWidget(self._status_scene_lbl)

        sb.showMessage("Ready  —  F1 for shortcuts")

    @staticmethod
    def _make_hline() -> QFrame:
        f = QFrame()
        f.setFrameShape(QFrame.Shape.HLine)
        f.setStyleSheet(f"color: {T.C_BORDER};")
        return f

    # ── Shortcuts ────────────────────────────────────────────────────────────

    def _build_shortcuts(self) -> None:
        pairs = [
            ("F1",      self._show_help),
            ("Ctrl+B",  self._toggle_blackout),
            ("1",       lambda: self._select_mode(_MODE_LIVE)),
            ("2",       lambda: self._select_mode(_MODE_DESK)),
            ("3",       lambda: self._select_mode(_MODE_PATCH)),
            ("4",       lambda: self._select_mode(_MODE_MONITOR)),
            ("5",       lambda: self._select_mode(_MODE_CUELIST)),
            ("6",       lambda: self._select_mode(_MODE_SHOW)),
            ("F11",     self._toggle_fullscreen),
        ]
        for key, slot in pairs:
            QShortcut(QKeySequence(key), self).activated.connect(slot)

    # ── Mode selection ───────────────────────────────────────────────────────

    def _select_mode(self, idx: int) -> None:
        self._stack.setCurrentIndex(idx)
        for i, btn in enumerate(self._mode_btns):
            btn.setChecked(i == idx)
        # Fixture pool and executor bank only visible in LIVE/DESK modes
        self._fixture_pool.setVisible(idx in (_MODE_LIVE, _MODE_DESK))
        self._exec_bank.setVisible(idx in (_MODE_LIVE, _MODE_DESK))

    # ── Slots ────────────────────────────────────────────────────────────────

    @Slot(list)
    def _on_selection_changed(self, names: list[str]) -> None:
        # Resolve names → fixture dicts and pass to programmer
        selected = [
            fx for fx in self._patch.fixtures
            if fx.name in names
        ]
        self._programmer.set_fixtures([f.to_dict() for f in selected])

    @Slot(list)
    def _on_fixtures_changed_for_show(self, fx_dicts: list) -> None:
        # Forward current patched fixtures to show panel for save
        self._show_panel.set_fixtures(fx_dicts)

    @Slot(int)
    def _on_cuelist_go(self, cuelist_id: int) -> None:
        self.statusBar().showMessage(f"GO — cuelist {cuelist_id}", 2000)

    @Slot(int)
    def _on_cuelist_back(self, cuelist_id: int) -> None:
        self.statusBar().showMessage(f"BACK — cuelist {cuelist_id}", 2000)

    @Slot(str)
    def _on_scene_saved(self, name: str) -> None:
        self._fixture_pool.refresh_scenes()
        n = len(self.engine.scenes)
        self._status_scene_lbl.setText(f"  {n} scene{'s' if n != 1 else ''}  ")
        self.statusBar().showMessage(f"Scene '{name}' recorded", 3000)

    @Slot(bool)
    def _on_blackout_changed(self, on: bool) -> None:
        self._btn_bo.blockSignals(True)
        self._btn_bo.setChecked(on)
        self._btn_bo.blockSignals(False)
        if on:
            self._blink.start()
        else:
            self._blink.stop()
            self._btn_bo.setStyleSheet("")
            self.statusBar().showMessage("Blackout OFF", 2000)

    def _blink_tick(self) -> None:
        self._blink_state = not self._blink_state
        c = T.C_RED if self._blink_state else "#3a0000"
        self._btn_bo.setStyleSheet(
            f"QPushButton#btn_blackout {{ background: {c}; color: white;"
            f" border: 2px solid {T.C_RED}; font-weight: 700; letter-spacing: 1px; }}"
        )

    @Slot(int, bytes)
    def _on_dmx_tick(self, uid: int, data: bytes) -> None:
        if uid in self._status_u:
            pct = int(max(data) / 255 * 100) if data else 0
            self._status_u[uid].setText(f"  U{uid}: {pct}%  ")

    def _toggle_blackout(self) -> None:
        self._btn_bo.setChecked(not self._btn_bo.isChecked())

    def _toggle_fullscreen(self) -> None:
        if self.isFullScreen():
            self.showNormal()
        else:
            self.showFullScreen()

    def _open_secondary(self) -> None:
        screens = self._screen_mgr.available_screens()
        if len(screens) < 2:
            return
        # Open DMX monitor on second screen
        monitor2 = DMXMonitor(self.engine)
        self._screen_mgr.open_on_screen(1, monitor2, "Lumos — DMX Monitor")

    def _show_help(self) -> None:
        QMessageBox.information(
            self, "Lumos — Keyboard shortcuts",
            "1  →  LIVE mode (fixture pool + programmer)\n"
            "2  →  DESK mode (direct faders)\n"
            "3  →  PATCH (fixture patching)\n"
            "4  →  DMX MONITOR\n"
            "5  →  CUES (cuelist editor)\n"
            "6  →  SHOW (save / load)\n"
            "Ctrl+B  →  Toggle Blackout\n"
            "F11     →  Toggle fullscreen\n"
            "F1      →  This help\n\n"
            "Fader:  drag up/down · scroll = ±1 (Shift = ±10)\n"
            "          double-click = zero · right-click = type value\n\n"
            "Knob:   drag up/down · scroll = ±1 (Shift = ±10)\n"
            "          double-click = zero · right-click = type value",
        )

    # ── Window close ─────────────────────────────────────────────────────────

    def closeEvent(self, event) -> None:
        self._screen_mgr.close_all()
        self.engine.stop()
        super().closeEvent(event)
