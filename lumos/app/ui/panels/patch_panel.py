"""
Patch panel — assign fixtures to DMX addresses.
Simple list of patched fixtures with add/remove/edit.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QTableWidget, QTableWidgetItem, QHeaderView, QComboBox,
    QSpinBox, QLineEdit, QDialog, QDialogButtonBox, QFormLayout,
    QMessageBox,
)
from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QColor

from app.engine import LumosEngine


# ── Simple fixture data model (no dependency on backend) ─────────────────────

class PatchedFixture:
    _next_id = 1

    def __init__(
        self,
        name: str,
        fixture_type: str,
        universe: int,
        address: int,
        channels: int,
        manufacturer: str = "",
    ) -> None:
        self.id = PatchedFixture._next_id
        PatchedFixture._next_id += 1
        self.name = name
        self.fixture_type = fixture_type
        self.manufacturer = manufacturer
        self.universe = universe
        self.address = address
        self.channels = channels

    @property
    def end_address(self) -> int:
        return self.address + self.channels - 1


# ── Built-in fixture types ────────────────────────────────────────────────────

BUILTIN_FIXTURES: list[dict] = [
    {"name": "Dimmer 1ch",         "type": "Dimmer",       "channels": 1,  "mfr": "Generic"},
    {"name": "PAR RGB",            "type": "LED PAR",      "channels": 3,  "mfr": "Generic"},
    {"name": "PAR RGBW",           "type": "LED PAR",      "channels": 4,  "mfr": "Generic"},
    {"name": "PAR RGBWA",          "type": "LED PAR",      "channels": 5,  "mfr": "Generic"},
    {"name": "PAR RGBWAU",         "type": "LED PAR",      "channels": 6,  "mfr": "Generic"},
    {"name": "Moving Head Spot",   "type": "Moving Head",  "channels": 8,  "mfr": "Generic"},
    {"name": "Moving Head Wash",   "type": "Moving Head",  "channels": 11, "mfr": "Generic"},
    {"name": "Moving Head 16ch",   "type": "Moving Head",  "channels": 16, "mfr": "Generic"},
    {"name": "Strobe 2ch",         "type": "Strobe",       "channels": 2,  "mfr": "Generic"},
    {"name": "Laser RGB",          "type": "Laser",        "channels": 7,  "mfr": "Generic"},
    {"name": "Smoke Machine",      "type": "Fogger",       "channels": 2,  "mfr": "Generic"},
    {"name": "LED Strip 4ch",      "type": "LED Strip",    "channels": 4,  "mfr": "Generic"},
]


# ── Add Fixture dialog ────────────────────────────────────────────────────────

class AddFixtureDialog(QDialog):
    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self.setWindowTitle("Add Fixture")
        self.setModal(True)
        self.setMinimumWidth(360)
        self._build()

    def _build(self) -> None:
        layout = QVBoxLayout(self)
        form = QFormLayout()
        form.setSpacing(8)

        # Fixture type
        self._type_combo = QComboBox()
        for fx in BUILTIN_FIXTURES:
            self._type_combo.addItem(f"{fx['name']}  ({fx['channels']}ch)", fx)
        self._type_combo.currentIndexChanged.connect(self._on_type_changed)
        form.addRow("Fixture type:", self._type_combo)

        # Custom name
        self._name_edit = QLineEdit()
        self._name_edit.setPlaceholderText("e.g. Stage Left PAR")
        form.addRow("Label:", self._name_edit)

        # Quantity
        self._qty_spin = QSpinBox()
        self._qty_spin.setRange(1, 48)
        self._qty_spin.setValue(1)
        form.addRow("Quantity:", self._qty_spin)

        # Universe
        self._uni_combo = QComboBox()
        for uid, u in self.engine.universes.items():
            self._uni_combo.addItem(f"Universe {uid} — {u.name}", uid)
        form.addRow("Universe:", self._uni_combo)

        # Start address
        self._addr_spin = QSpinBox()
        self._addr_spin.setRange(1, 512)
        self._addr_spin.setValue(1)
        form.addRow("Start address:", self._addr_spin)

        # Channels info
        self._info_lbl = QLabel()
        self._info_lbl.setStyleSheet("color: #808080; font-size: 10px;")
        form.addRow("", self._info_lbl)
        self._on_type_changed(0)

        layout.addLayout(form)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)

    def _on_type_changed(self, index: int) -> None:
        fx = self._type_combo.itemData(index)
        if fx:
            self._info_lbl.setText(
                f"{fx['channels']} channels  ·  {fx['type']}  ·  {fx['mfr']}"
            )

    def get_fixtures(self) -> list[PatchedFixture]:
        fx = self._type_combo.currentData()
        if not fx:
            return []
        uid = self._uni_combo.currentData()
        base_addr = self._addr_spin.value()
        qty = self._qty_spin.value()
        base_name = self._name_edit.text().strip() or fx["name"]
        result = []
        for i in range(qty):
            name = f"{base_name} {i+1}" if qty > 1 else base_name
            addr = base_addr + i * fx["channels"]
            if addr + fx["channels"] - 1 > 512:
                break
            result.append(PatchedFixture(
                name=name,
                fixture_type=fx["type"],
                manufacturer=fx["mfr"],
                universe=uid,
                address=addr,
                channels=fx["channels"],
            ))
        return result


# ── Patch Panel ───────────────────────────────────────────────────────────────

class PatchPanel(QWidget):
    """Shows all patched fixtures; supports add/remove."""

    fixtures_changed = Signal()

    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self.fixtures: list[PatchedFixture] = []
        self._build_ui()

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(6)

        # Toolbar
        bar = QHBoxLayout()
        btn_add = QPushButton("＋  Add Fixture")
        btn_add.setFixedHeight(28)
        btn_add.clicked.connect(self._add)
        bar.addWidget(btn_add)

        self._btn_remove = QPushButton("✕  Remove")
        self._btn_remove.setFixedHeight(28)
        self._btn_remove.setEnabled(False)
        self._btn_remove.clicked.connect(self._remove)
        bar.addWidget(self._btn_remove)

        bar.addStretch()
        self._count_lbl = QLabel("0 fixtures")
        self._count_lbl.setStyleSheet("color: #808080; font-size: 10px;")
        bar.addWidget(self._count_lbl)
        root.addLayout(bar)

        # Table
        self._table = QTableWidget(0, 6)
        self._table.setHorizontalHeaderLabels(
            ["Name", "Type", "Universe", "Addr", "Channels", "End Addr"]
        )
        self._table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        self._table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.ResizeToContents)
        self._table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self._table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self._table.setAlternatingRowColors(True)
        self._table.verticalHeader().setVisible(False)
        self._table.itemSelectionChanged.connect(self._on_selection)
        root.addWidget(self._table, 1)

    def _add(self) -> None:
        dlg = AddFixtureDialog(self.engine, self)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            for fx in dlg.get_fixtures():
                self._add_fixture(fx)

    def _add_fixture(self, fx: PatchedFixture) -> None:
        # Warn about overlap
        for existing in self.fixtures:
            if existing.universe == fx.universe:
                if not (fx.end_address < existing.address or
                        fx.address > existing.end_address):
                    QMessageBox.warning(
                        self, "Address conflict",
                        f"'{fx.name}' (U{fx.universe}:{fx.address}–{fx.end_address}) "
                        f"overlaps with '{existing.name}' ({existing.address}–{existing.end_address}).",
                    )
        self.fixtures.append(fx)
        self._refresh_table()
        self.fixtures_changed.emit()

    def _remove(self) -> None:
        rows = sorted(
            {idx.row() for idx in self._table.selectedIndexes()},
            reverse=True,
        )
        for r in rows:
            if 0 <= r < len(self.fixtures):
                self.fixtures.pop(r)
        self._refresh_table()
        self.fixtures_changed.emit()

    def _on_selection(self) -> None:
        self._btn_remove.setEnabled(bool(self._table.selectedItems()))

    def _refresh_table(self) -> None:
        self._table.setRowCount(len(self.fixtures))
        type_colors = {
            "Moving Head": "#9c6fcd",
            "LED PAR":     "#42a5f5",
            "Dimmer":      "#ffa726",
            "Strobe":      "#f44336",
            "Laser":       "#4caf50",
            "Fogger":      "#90a4ae",
            "LED Strip":   "#26c6da",
        }
        for r, fx in enumerate(self.fixtures):
            items = [
                fx.name,
                fx.fixture_type,
                f"Universe {fx.universe}",
                str(fx.address),
                str(fx.channels),
                str(fx.end_address),
            ]
            for c, txt in enumerate(items):
                item = QTableWidgetItem(txt)
                item.setForeground(QColor(
                    type_colors.get(fx.fixture_type, "#d4d4d4") if c == 1 else "#d4d4d4"
                ))
                self._table.setItem(r, c, item)

        self._count_lbl.setText(f"{len(self.fixtures)} fixture{'s' if len(self.fixtures) != 1 else ''}")
