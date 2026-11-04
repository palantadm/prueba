"""
Fixture pool panel — left sidebar.
Grid of fixture buttons, click to select for programmer.
MA3-style.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QGridLayout,
    QPushButton, QLabel, QScrollArea, QFrame, QSizePolicy,
)
from PySide6.QtCore import Qt, Signal, QSize
from PySide6.QtGui import QColor, QFont

from app.engine import LumosEngine
from app.ui import theme as T

# Map fixture type → colour
_FX_COLOR = {
    "Moving Head": T.C_POSITION,
    "LED PAR":     T.C_BEAM,
    "Dimmer":      "#606060",
    "Strobe":      T.C_RED,
    "Laser":       T.C_GREEN,
    "Fogger":      "#508060",
    "LED Strip":   T.C_ACCENT,
}


class FixtureButton(QPushButton):
    """A touch-sized fixture/group button in the pool."""

    def __init__(self, name: str, number: int, fx_type: str = "", parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.fx_name   = name
        self.fx_number = number
        self.fx_type   = fx_type
        self.setCheckable(True)
        self.setMinimumSize(80, 52)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self._base_color = _FX_COLOR.get(fx_type, "#505050")
        self._update_style(False)

    def setChecked(self, checked: bool) -> None:
        super().setChecked(checked)
        self._update_style(checked)

    def _update_style(self, active: bool) -> None:
        c = self._base_color
        if active:
            self.setStyleSheet(f"""
                QPushButton {{
                    background-color: {c};
                    color: #0c0c0c;
                    border: 2px solid {QColor(c).lighter(150).name()};
                    border-radius: 3px;
                    font-weight: 800;
                    font-size: 10px;
                    padding: 4px 2px;
                    text-align: center;
                }}
            """)
        else:
            self.setStyleSheet(f"""
                QPushButton {{
                    background-color: {T.C_PANEL2};
                    color: {c};
                    border: 1px solid {T.C_BORDER};
                    border-left: 3px solid {c};
                    border-radius: 3px;
                    font-weight: 700;
                    font-size: 10px;
                    padding: 4px 2px;
                    text-align: center;
                }}
                QPushButton:hover {{
                    background-color: {T.C_INPUT};
                    border-left-color: {QColor(c).lighter(140).name()};
                }}
            """)


class FixturePool(QWidget):
    """
    Left panel: fixture buttons + group buttons.
    selection_changed emits list of selected fixture names.
    """
    selection_changed = Signal(list)   # list[str] — fixture names

    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self._buttons: list[FixtureButton] = []
        self._fixtures: list[dict] = []   # {name, type, universe, address}
        self.setFixedWidth(220)
        self._build()

    def _build(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(4, 4, 4, 4)
        root.setSpacing(4)

        # Header
        hdr = QLabel("FIXTURES")
        hdr.setStyleSheet(
            f"color: {T.C_TEXT_LABEL}; font-size: 10px; font-weight: 800;"
            f" letter-spacing: 2px; padding: 4px 6px;"
            f" border-bottom: 1px solid {T.C_BORDER};"
        )
        root.addWidget(hdr)

        # Quick select bar
        sel_bar = QHBoxLayout()
        sel_bar.setSpacing(4)
        btn_all = QPushButton("ALL")
        btn_all.setFixedHeight(28)
        btn_all.clicked.connect(self._select_all)
        sel_bar.addWidget(btn_all)

        btn_none = QPushButton("NONE")
        btn_none.setFixedHeight(28)
        btn_none.clicked.connect(self._select_none)
        sel_bar.addWidget(btn_none)

        btn_inv = QPushButton("INV")
        btn_inv.setFixedHeight(28)
        btn_inv.setToolTip("Invert selection")
        btn_inv.clicked.connect(self._invert)
        sel_bar.addWidget(btn_inv)
        root.addLayout(sel_bar)

        line = QFrame()
        line.setFrameShape(QFrame.Shape.HLine)
        line.setStyleSheet(f"color: {T.C_BORDER};")
        root.addWidget(line)

        # Grid of fixture buttons
        self._grid_widget = QWidget()
        self._grid_widget.setStyleSheet(f"background: {T.C_BG};")
        self._grid = QGridLayout(self._grid_widget)
        self._grid.setSpacing(3)
        self._grid.setContentsMargins(0, 0, 0, 0)

        scroll = QScrollArea()
        scroll.setWidget(self._grid_widget)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setStyleSheet("border: none; background: transparent;")
        root.addWidget(scroll, 1)

        # Scene pool (bottom)
        hdr2 = QLabel("SCENES")
        hdr2.setStyleSheet(
            f"color: {T.C_TEXT_LABEL}; font-size: 10px; font-weight: 800;"
            f" letter-spacing: 2px; padding: 4px 6px;"
            f" border-top: 1px solid {T.C_BORDER}; border-bottom: 1px solid {T.C_BORDER};"
        )
        root.addWidget(hdr2)

        self._scene_widget = QWidget()
        self._scene_widget.setStyleSheet(f"background: {T.C_BG};")
        self._scene_grid = QGridLayout(self._scene_widget)
        self._scene_grid.setSpacing(3)
        self._scene_grid.setContentsMargins(0, 0, 0, 0)

        scene_scroll = QScrollArea()
        scene_scroll.setWidget(self._scene_widget)
        scene_scroll.setWidgetResizable(True)
        scene_scroll.setFixedHeight(100)
        scene_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scene_scroll.setStyleSheet("border: none; background: transparent;")
        root.addWidget(scene_scroll)

        # Empty placeholder
        self._empty_lbl = QLabel("No fixtures patched.\nGo to Patch tab.")
        self._empty_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._empty_lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 10px;")
        root.addWidget(self._empty_lbl)
        self._empty_lbl.hide()

    # ── Public ────────────────────────────────────────────────────────

    def refresh_fixtures(self, fixtures: list[dict]) -> None:
        """Rebuild buttons from a list of patched fixture dicts."""
        self._fixtures = fixtures

        for btn in self._buttons:
            self._grid.removeWidget(btn)
            btn.deleteLater()
        self._buttons.clear()

        if not fixtures:
            self._empty_lbl.show()
            return
        self._empty_lbl.hide()

        cols = 2
        for i, fx in enumerate(fixtures):
            btn = FixtureButton(
                name=fx.get("name", f"FX {i+1}"),
                number=i + 1,
                fx_type=fx.get("type", ""),
            )
            row, col = divmod(i, cols)
            text = f"{fx.get('name', '')}\n{fx.get('type', '')}  U{fx.get('universe', 1)}:{fx.get('address', 1)}"
            btn.setText(text)
            btn.toggled.connect(lambda checked, b=btn: self._on_toggle(b, checked))
            self._grid.addWidget(btn, row, col)
            self._buttons.append(btn)

    def refresh_scenes(self) -> None:
        """Rebuild scene pool buttons from engine.scenes."""
        for i in reversed(range(self._scene_grid.count())):
            w = self._scene_grid.itemAt(i).widget()
            if w:
                w.deleteLater()

        cols = 2
        for i, name in enumerate(self.engine.scenes):
            btn = QPushButton(name)
            btn.setMinimumSize(80, 44)
            btn.setStyleSheet(
                f"background: {T.C_PANEL2}; color: {T.C_EFFECT};"
                f" border: 1px solid {T.C_BORDER}; border-radius: 3px;"
                f" font-weight: 700; font-size: 9px;"
            )
            btn.setToolTip(f"Recall scene: {name}")
            btn.clicked.connect(lambda checked, n=name: self.engine.recall_scene(n, 1))
            r, c = divmod(i, cols)
            self._scene_grid.addWidget(btn, r, c)

    def selected_fixtures(self) -> list[str]:
        return [btn.fx_name for btn in self._buttons if btn.isChecked()]

    # ── Slots ─────────────────────────────────────────────────────────

    def _on_toggle(self, btn: FixtureButton, checked: bool) -> None:
        self.selection_changed.emit(self.selected_fixtures())

    def _select_all(self) -> None:
        for btn in self._buttons:
            btn.setChecked(True)
        self.selection_changed.emit(self.selected_fixtures())

    def _select_none(self) -> None:
        for btn in self._buttons:
            btn.setChecked(False)
        self.selection_changed.emit([])

    def _invert(self) -> None:
        for btn in self._buttons:
            btn.setChecked(not btn.isChecked())
        self.selection_changed.emit(self.selected_fixtures())
