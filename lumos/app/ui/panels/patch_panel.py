"""
Patch panel — assign fixtures to DMX addresses.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QTableWidget, QTableWidgetItem, QHeaderView, QComboBox,
    QSpinBox, QLineEdit, QDialog, QDialogButtonBox, QFormLayout,
    QMessageBox, QCheckBox, QFrame,
)
from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QColor

from app.engine import LumosEngine
from app.ui import theme as T

_FX_COLOR = {
    "Moving Head": T.C_POSITION,
    "LED PAR":     T.C_BEAM,
    "Dimmer":      "#606060",
    "Strobe":      T.C_RED,
    "Laser":       T.C_GREEN,
    "Fogger":      "#508060",
    "LED Strip":   T.C_ACCENT,
}

# Generic fixture library
BUILTIN: list[dict] = [
    {"name": "Dimmer 1ch",       "type": "Dimmer",      "channels": 1,  "mfr": "Generic",
     "ch_names": ["Intensity"]},
    {"name": "PAR RGB",          "type": "LED PAR",     "channels": 3,  "mfr": "Generic",
     "ch_names": ["Red", "Green", "Blue"]},
    {"name": "PAR RGBW",         "type": "LED PAR",     "channels": 4,  "mfr": "Generic",
     "ch_names": ["Red", "Green", "Blue", "White"]},
    {"name": "PAR RGBWAU",       "type": "LED PAR",     "channels": 6,  "mfr": "Generic",
     "ch_names": ["Red", "Green", "Blue", "White", "Amber", "UV"]},
    {"name": "Moving Head Spot", "type": "Moving Head", "channels": 8,  "mfr": "Generic",
     "ch_names": ["Pan", "Pan Fine", "Tilt", "Tilt Fine", "Color Wheel", "Gobo Wheel", "Shutter", "Dimmer"]},
    {"name": "Moving Head Wash", "type": "Moving Head", "channels": 11, "mfr": "Generic",
     "ch_names": ["Pan", "Pan Fine", "Tilt", "Tilt Fine", "Red", "Green", "Blue", "White", "Dimmer", "Zoom", "Speed"]},
    {"name": "Moving Head 16ch", "type": "Moving Head", "channels": 16, "mfr": "Generic",
     "ch_names": ["Pan", "Pan Fine", "Tilt", "Tilt Fine", "Speed", "Dimmer", "Shutter",
                  "Color Wheel", "Gobo Wheel", "Gobo Rotate", "Focus", "Zoom", "Iris", "Frost",
                  "Prism", "Control"]},
    {"name": "Strobe 2ch",       "type": "Strobe",      "channels": 2,  "mfr": "Generic",
     "ch_names": ["Intensity", "Strobe"]},
    {"name": "Laser RGB",        "type": "Laser",       "channels": 7,  "mfr": "Generic",
     "ch_names": ["Mode", "Red", "Green", "Blue", "Speed", "Pattern", "Rotation"]},
    {"name": "Smoke 2ch",        "type": "Fogger",      "channels": 2,  "mfr": "Generic",
     "ch_names": ["Intensity", "Strobe"]},
    {"name": "LED Strip 6ch",    "type": "LED Strip",   "channels": 6,  "mfr": "Generic",
     "ch_names": ["Red", "Green", "Blue", "White", "Amber", "UV"]},
    {"name": "LED Strip 7ch",    "type": "LED Strip",   "channels": 7,  "mfr": "Generic",
     "ch_names": ["Master", "Red", "Green", "Blue", "White", "Amber", "UV"]},
]


class PatchedFixture:
    _next_id = 1

    def __init__(self, name: str, fx_type: str, manufacturer: str,
                 universe: int, address: int, channels: int, ch_names: list[str]) -> None:
        self.id = PatchedFixture._next_id
        PatchedFixture._next_id += 1
        self.name = name
        self.fx_type = fx_type
        self.manufacturer = manufacturer
        self.universe = universe
        self.address = address
        self.channels = channels
        self.ch_names = ch_names

    @property
    def end_address(self) -> int:
        return self.address + self.channels - 1

    def to_dict(self) -> dict:
        return {
            "name": self.name,
            "type": self.fx_type,
            "universe": self.universe,
            "address": self.address,
            "channel_names": self.ch_names,
        }


class AddFixtureDialog(QDialog):
    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self.setWindowTitle("Add Fixture")
        self.setModal(True)
        self.setMinimumWidth(400)
        self._build()

    def _build(self) -> None:
        root = QVBoxLayout(self)
        root.setSpacing(10)

        form = QFormLayout()
        form.setSpacing(8)
        form.setLabelAlignment(Qt.AlignmentFlag.AlignRight)

        self._type_combo = QComboBox()
        for fx in BUILTIN:
            self._type_combo.addItem(f"{fx['name']}  ({fx['channels']}ch)  —  {fx['mfr']}", fx)
        self._type_combo.currentIndexChanged.connect(self._on_type)
        form.addRow("Fixture type:", self._type_combo)

        self._name_edit = QLineEdit()
        self._name_edit.setPlaceholderText("e.g. Stage Left Spot")
        form.addRow("Label:", self._name_edit)

        self._qty_spin = QSpinBox()
        self._qty_spin.setRange(1, 64)
        self._qty_spin.setValue(1)
        form.addRow("Quantity:", self._qty_spin)

        self._uni_combo = QComboBox()
        for uid, u in self.engine.universes.items():
            self._uni_combo.addItem(f"Universe {uid}  —  {u.name}", uid)
        form.addRow("Universe:", self._uni_combo)

        self._addr_spin = QSpinBox()
        self._addr_spin.setRange(1, 512)
        self._addr_spin.setValue(1)
        form.addRow("Start address:", self._addr_spin)

        self._info_lbl = QLabel()
        self._info_lbl.setStyleSheet(f"color: {T.C_TEXT_LABEL}; font-size: 10px;")
        form.addRow("", self._info_lbl)

        root.addLayout(form)

        self._btn_box = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        self._btn_box.accepted.connect(self.accept)
        self._btn_box.rejected.connect(self.reject)
        root.addWidget(self._btn_box)

        self._on_type(0)

    def _on_type(self, index: int) -> None:
        fx = self._type_combo.itemData(index)
        if fx:
            self._info_lbl.setText(
                f"{fx['channels']} ch  ·  {fx['type']}  ·  channels: "
                + ", ".join(fx.get("ch_names", [])[:6])
                + ("…" if len(fx.get("ch_names", [])) > 6 else "")
            )

    def get_fixtures(self) -> list[PatchedFixture]:
        fx = self._type_combo.currentData()
        if not fx:
            return []
        uid = self._uni_combo.currentData()
        addr = self._addr_spin.value()
        qty = self._qty_spin.value()
        base = self._name_edit.text().strip() or fx["name"]
        result = []
        for i in range(qty):
            name = f"{base} {i+1}" if qty > 1 else base
            a = addr + i * fx["channels"]
            if a + fx["channels"] - 1 > 512:
                break
            result.append(PatchedFixture(
                name=name, fx_type=fx["type"], manufacturer=fx["mfr"],
                universe=uid, address=a, channels=fx["channels"],
                ch_names=list(fx.get("ch_names", [])),
            ))
        return result


class PatchPanel(QWidget):
    fixtures_changed = Signal(list)  # list[dict]

    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self.fixtures: list[PatchedFixture] = []
        self._build_ui()

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(6)

        bar = QHBoxLayout()
        bar.setSpacing(8)

        btn_add = QPushButton("＋  Add Fixture")
        btn_add.setFixedHeight(36)
        btn_add.clicked.connect(self._add)
        bar.addWidget(btn_add)

        self._btn_remove = QPushButton("✕  Remove")
        self._btn_remove.setFixedHeight(36)
        self._btn_remove.setEnabled(False)
        self._btn_remove.clicked.connect(self._remove)
        bar.addWidget(self._btn_remove)

        bar.addStretch()

        self._count_lbl = QLabel("No fixtures")
        self._count_lbl.setStyleSheet(f"color: {T.C_TEXT_LABEL}; font-size: 10px;")
        bar.addWidget(self._count_lbl)
        root.addLayout(bar)

        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.HLine)
        sep.setStyleSheet(f"color: {T.C_BORDER};")
        root.addWidget(sep)

        self._table = QTableWidget(0, 7)
        self._table.setHorizontalHeaderLabels(
            ["#", "Name", "Type", "Manufacturer", "Universe", "Addr", "Ch"]
        )
        hh = self._table.horizontalHeader()
        hh.setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        hh.setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        hh.setSectionResizeMode(2, QHeaderView.ResizeMode.ResizeToContents)
        hh.setSectionResizeMode(3, QHeaderView.ResizeMode.ResizeToContents)
        hh.setSectionResizeMode(4, QHeaderView.ResizeMode.ResizeToContents)
        hh.setSectionResizeMode(5, QHeaderView.ResizeMode.ResizeToContents)
        hh.setSectionResizeMode(6, QHeaderView.ResizeMode.ResizeToContents)
        self._table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self._table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self._table.setAlternatingRowColors(True)
        self._table.verticalHeader().setVisible(False)
        self._table.itemSelectionChanged.connect(self._on_select)
        self._table.setMinimumRowHeight = lambda h: None  # noqa
        root.addWidget(self._table, 1)

    def _add(self) -> None:
        dlg = AddFixtureDialog(self.engine, self)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            for fx in dlg.get_fixtures():
                self._add_fx(fx)

    def _add_fx(self, fx: PatchedFixture) -> None:
        for ex in self.fixtures:
            if ex.universe == fx.universe:
                if not (fx.end_address < ex.address or fx.address > ex.end_address):
                    QMessageBox.warning(
                        self, "Address conflict",
                        f"'{fx.name}' (U{fx.universe}:{fx.address}–{fx.end_address})"
                        f" overlaps with '{ex.name}' ({ex.address}–{ex.end_address}).\n"
                        f"The fixture was added anyway; please check addresses.",
                    )
        self.fixtures.append(fx)
        self._refresh()
        self.fixtures_changed.emit([f.to_dict() for f in self.fixtures])

    def _remove(self) -> None:
        rows = sorted(
            {i.row() for i in self._table.selectedIndexes()}, reverse=True
        )
        for r in rows:
            if 0 <= r < len(self.fixtures):
                self.fixtures.pop(r)
        self._refresh()
        self.fixtures_changed.emit([f.to_dict() for f in self.fixtures])

    def _on_select(self) -> None:
        self._btn_remove.setEnabled(bool(self._table.selectedItems()))

    def _refresh(self) -> None:
        self._table.setRowCount(len(self.fixtures))
        for r, fx in enumerate(self.fixtures):
            self._table.setRowHeight(r, 32)
            vals = [
                str(fx.id), fx.name, fx.fx_type, fx.manufacturer,
                f"U{fx.universe}", str(fx.address), str(fx.channels),
            ]
            for c, txt in enumerate(vals):
                item = QTableWidgetItem(txt)
                col = _FX_COLOR.get(fx.fx_type, T.C_TEXT) if c == 2 else T.C_TEXT
                item.setForeground(QColor(col))
                self._table.setItem(r, c, item)
        n = len(self.fixtures)
        self._count_lbl.setText(f"{n} fixture{'s' if n != 1 else ''} patched")
