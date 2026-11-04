"""
Executor bank — horizontal row of executor faders at the bottom of the window.
Supports pages of executors (like Hog4 / MA).
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QHBoxLayout, QVBoxLayout, QPushButton,
    QLabel, QScrollArea, QFrame, QSizePolicy,
)
from PySide6.QtCore import Qt, Signal

from app.engine import LumosEngine
from app.ui.widgets.executor_fader import ExecutorFader
from app.ui import theme as T

_EXECS_PER_PAGE = 10


class ExecutorBank(QWidget):
    """
    Fixed-height bank of executor faders with page navigation.
    Executors can fire scenes from the engine.
    """

    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self._page = 0
        self._execs: list[ExecutorFader] = []
        self.setFixedHeight(172)
        self.setStyleSheet(f"background-color: {T.C_PANEL2}; border-top: 1px solid {T.C_BORDER};")
        self._build()
        self._load_page(0)

    def _build(self) -> None:
        root = QHBoxLayout(self)
        root.setContentsMargins(6, 4, 6, 4)
        root.setSpacing(4)

        # Page nav (left)
        nav = QVBoxLayout()
        nav.setSpacing(4)
        self._page_lbl = QLabel("PG 1")
        self._page_lbl.setStyleSheet(
            f"color: {T.C_ACCENT}; font-weight: 800; font-size: 10px; letter-spacing: 1px;"
        )
        self._page_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        nav.addWidget(self._page_lbl)

        btn_prev = QPushButton("▲")
        btn_prev.setFixedSize(36, 36)
        btn_prev.setToolTip("Previous page")
        btn_prev.clicked.connect(self._prev_page)
        nav.addWidget(btn_prev)

        btn_next = QPushButton("▼")
        btn_next.setFixedSize(36, 36)
        btn_next.setToolTip("Next page")
        btn_next.clicked.connect(self._next_page)
        nav.addWidget(btn_next)

        nav.addStretch()
        nav_w = QWidget()
        nav_w.setLayout(nav)
        nav_w.setFixedWidth(44)
        root.addWidget(nav_w)

        # Separator
        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.VLine)
        sep.setStyleSheet(f"color: {T.C_BORDER};")
        root.addWidget(sep)

        # Executor faders container (scrollable)
        self._fader_row = QWidget()
        self._fader_row.setStyleSheet(f"background: {T.C_PANEL2};")
        self._fader_layout = QHBoxLayout(self._fader_row)
        self._fader_layout.setContentsMargins(0, 0, 0, 0)
        self._fader_layout.setSpacing(3)

        scroll = QScrollArea()
        scroll.setWidget(self._fader_row)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setStyleSheet("border: none;")
        root.addWidget(scroll, 1)

        # Separator
        sep2 = QFrame()
        sep2.setFrameShape(QFrame.Shape.VLine)
        sep2.setStyleSheet(f"color: {T.C_BORDER};")
        root.addWidget(sep2)

        # Master GO / STOP / CLR (right)
        ctrl = QVBoxLayout()
        ctrl.setSpacing(4)

        btn_go_all = QPushButton("▶  GO")
        btn_go_all.setObjectName("btn_go")
        btn_go_all.setFixedSize(72, 44)
        btn_go_all.setToolTip("GO all assigned executors")
        ctrl.addWidget(btn_go_all)

        btn_stop_all = QPushButton("■  STOP")
        btn_stop_all.setObjectName("btn_stop")
        btn_stop_all.setFixedSize(72, 36)
        btn_stop_all.setToolTip("STOP all executors")
        ctrl.addWidget(btn_stop_all)

        btn_clr = QPushButton("CLR")
        btn_clr.setFixedSize(72, 28)
        btn_clr.setStyleSheet(
            f"background: #1a1a00; border: 1px solid #3a3a10;"
            f" color: #c0c040; font-weight: 700;"
        )
        btn_clr.setToolTip("Clear programmer")
        ctrl.addWidget(btn_clr)

        ctrl.addStretch()
        ctrl_w = QWidget()
        ctrl_w.setLayout(ctrl)
        ctrl_w.setFixedWidth(80)
        root.addWidget(ctrl_w)

    def _load_page(self, page: int) -> None:
        # Clear old faders
        for ex in self._execs:
            self._fader_layout.removeWidget(ex)
            ex.deleteLater()
        self._execs.clear()

        for i in range(_EXECS_PER_PAGE):
            ex_num = page * _EXECS_PER_PAGE + i + 1
            ex = ExecutorFader(ex_num)
            ex.go_pressed.connect(lambda n=ex_num: self._on_go(n))
            ex.stop_pressed.connect(lambda n=ex_num: self._on_stop(n))
            ex.flash_pressed.connect(lambda held, n=ex_num: self._on_flash(n, held))
            ex.level_changed.connect(lambda v, n=ex_num: self._on_level(n, v))
            self._fader_layout.addWidget(ex)
            self._execs.append(ex)

        self._fader_layout.addStretch()
        self._page = page
        self._page_lbl.setText(f"PG {page + 1}")

    # ── Engine callbacks ──────────────────────────────────────────────

    def _on_go(self, ex_num: int) -> None:
        idx = ex_num - 1 - self._page * _EXECS_PER_PAGE
        if 0 <= idx < len(self._execs):
            name = self._execs[idx]._assigned
            if name and name in self.engine.scenes:
                self.engine.recall_scene(name, 1)

    def _on_stop(self, ex_num: int) -> None:
        pass  # TODO: playback stop

    def _on_flash(self, ex_num: int, held: bool) -> None:
        pass  # level already handled in ExecutorFader

    def _on_level(self, ex_num: int, value: int) -> None:
        pass  # TODO: scale executor output

    def _prev_page(self) -> None:
        if self._page > 0:
            self._load_page(self._page - 1)

    def _next_page(self) -> None:
        self._load_page(self._page + 1)

    def assign_scene(self, ex_index: int, scene_name: str) -> None:
        """Assign a scene name to executor slot (0-indexed within current page)."""
        if 0 <= ex_index < len(self._execs):
            self._execs[ex_index].assign(scene_name)
