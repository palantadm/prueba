"""Fixture Library browser dialog — search, preview, add to patch."""
from __future__ import annotations

from typing import Optional

from PySide6.QtWidgets import (
    QDialog, QWidget, QVBoxLayout, QHBoxLayout, QSplitter,
    QTreeWidget, QTreeWidgetItem, QListWidget, QListWidgetItem,
    QLabel, QLineEdit, QPushButton, QSpinBox, QComboBox,
    QFrame, QScrollArea, QGroupBox, QGridLayout,
    QFileDialog, QProgressDialog, QMessageBox, QSizePolicy,
)
from PySide6.QtCore import Qt, Signal, QThread, QObject, Slot
from PySide6.QtGui import QFont, QColor

from app.ui import theme as T
from fixtures.library import get_library
from fixtures.model import FixtureDef, ChannelDef


# ─── Background import worker ────────────────────────────────────────────────

class _ImportWorker(QObject):
    finished = Signal(int)
    error    = Signal(str)

    def __init__(self, folder: str, fmt: str) -> None:
        super().__init__()
        self._folder = folder
        self._fmt    = fmt

    def run(self) -> None:
        try:
            lib = get_library()
            if self._fmt == "qlcplus":
                count = lib.import_qlcplus_folder(self._folder)
            else:
                count = lib.import_ofl_folder(self._folder)
            self.finished.emit(count)
        except Exception as e:
            self.error.emit(str(e))


# ─── Channel detail row ───────────────────────────────────────────────────────

class _ChannelRow(QFrame):
    def __init__(self, idx: int, ch: ChannelDef, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.setFixedHeight(26)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(4, 0, 4, 0)
        lay.setSpacing(6)

        num = QLabel(f"{idx + 1:3d}")
        num.setFixedWidth(28)
        num.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 11px; font-family: monospace;")
        lay.addWidget(num)

        color_dot = QLabel("●")
        color_dot.setFixedWidth(16)
        color_dot.setStyleSheet(f"color: {ch.type.ui_color}; font-size: 14px;")
        lay.addWidget(color_dot)

        name_lbl = QLabel(ch.name)
        name_lbl.setStyleSheet(f"color: {T.C_TEXT}; font-size: 11px;")
        lay.addWidget(name_lbl, 1)

        type_lbl = QLabel(ch.type.display_name)
        type_lbl.setFixedWidth(90)
        type_lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 10px;")
        type_lbl.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        lay.addWidget(type_lbl)


# ─── Main dialog ─────────────────────────────────────────────────────────────

class FixtureLibraryPanel(QDialog):
    """
    Signal emits list of (FixtureDef, mode_name, universe, address, quantity).
    """
    fixtures_to_add = Signal(list)

    def __init__(self, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("Fixture Library")
        self.setMinimumSize(1000, 680)
        self.resize(1200, 760)
        self.setModal(True)

        self._lib = get_library()
        self._selected: Optional[FixtureDef] = None
        self._import_thread: Optional[QThread] = None
        self._import_worker: Optional[_ImportWorker] = None

        self._build_ui()
        self._refresh_tree()

        self.setStyleSheet(
            f"QDialog {{ background: {T.C_BG}; }}"
            f"QLabel {{ color: {T.C_TEXT}; }}"
            f"QLineEdit {{ background: {T.C_PANEL2}; color: {T.C_TEXT};"
            f" border: 1px solid {T.C_BORDER}; border-radius: 3px; padding: 2px 6px; }}"
            f"QTreeWidget, QListWidget {{ background: {T.C_PANEL}; color: {T.C_TEXT};"
            f" border: 1px solid {T.C_BORDER}; outline: none; }}"
            f"QTreeWidget::item:selected, QListWidget::item:selected"
            f" {{ background: {T.C_ACCENT}; color: white; }}"
            f"QScrollBar:vertical {{ background: {T.C_PANEL2}; width: 8px; }}"
            f"QScrollBar::handle:vertical {{ background: {T.C_BORDER}; border-radius: 4px; }}"
            f"QGroupBox {{ color: {T.C_TEXT_LABEL}; border: 1px solid {T.C_BORDER};"
            f" border-radius: 4px; margin-top: 8px; padding-top: 8px; font-size: 10px; }}"
            f"QGroupBox::title {{ subcontrol-origin: margin; left: 8px; }}"
            f"QSpinBox, QComboBox {{ background: {T.C_PANEL2}; color: {T.C_TEXT};"
            f" border: 1px solid {T.C_BORDER}; border-radius: 3px; padding: 2px 4px; }}"
            f"QComboBox::drop-down {{ border: none; }}"
            f"QComboBox QAbstractItemView {{ background: {T.C_PANEL2}; color: {T.C_TEXT}; }}"
        )

    # ── Build UI ──────────────────────────────────────────────────────────────

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(6)

        # Top: search bar + import buttons
        root.addWidget(self._build_toolbar())

        # Main content: tree | fixture list | detail
        splitter = QSplitter(Qt.Orientation.Horizontal)
        splitter.setHandleWidth(4)
        splitter.setStyleSheet(f"QSplitter::handle {{ background: {T.C_BORDER}; }}")

        # Left: manufacturer tree
        self._tree = QTreeWidget()
        self._tree.setHeaderHidden(True)
        self._tree.setMinimumWidth(180)
        self._tree.setMaximumWidth(260)
        self._tree.itemSelectionChanged.connect(self._on_tree_select)
        splitter.addWidget(self._tree)

        # Centre: fixture list
        centre = QWidget()
        cl = QVBoxLayout(centre)
        cl.setContentsMargins(0, 0, 0, 0)
        cl.setSpacing(4)

        lbl_count = QLabel()
        lbl_count.setObjectName("lbl_count")
        lbl_count.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 10px; padding: 2px 4px;")
        cl.addWidget(lbl_count)
        self._lbl_count = lbl_count

        self._list = QListWidget()
        self._list.setMinimumWidth(240)
        self._list.itemSelectionChanged.connect(self._on_fixture_select)
        cl.addWidget(self._list)
        splitter.addWidget(centre)

        # Right: detail + add controls
        right = QWidget()
        rl = QVBoxLayout(right)
        rl.setContentsMargins(0, 0, 0, 0)
        rl.setSpacing(6)
        rl.addWidget(self._build_detail_panel())
        rl.addWidget(self._build_add_panel())
        splitter.addWidget(right)

        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)
        splitter.setStretchFactor(2, 2)
        root.addWidget(splitter, 1)

        # Bottom: cancel button
        root.addWidget(self._build_bottom_bar())

    def _build_toolbar(self) -> QWidget:
        bar = QWidget()
        lay = QHBoxLayout(bar)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(8)

        lbl = QLabel("Search:")
        lbl.setStyleSheet(f"color: {T.C_TEXT_LABEL}; font-size: 11px;")
        lay.addWidget(lbl)

        self._search = QLineEdit()
        self._search.setPlaceholderText("manufacturer, model, type…")
        self._search.setFixedHeight(32)
        self._search.textChanged.connect(self._on_search)
        lay.addWidget(self._search, 1)

        lay.addSpacing(12)

        btn_qlc = QPushButton("⬇  Import QLC+ folder")
        btn_qlc.setFixedHeight(32)
        btn_qlc.setStyleSheet(self._btn_style())
        btn_qlc.clicked.connect(lambda: self._import_folder("qlcplus"))
        lay.addWidget(btn_qlc)

        btn_ofl = QPushButton("⬇  Import OFL folder")
        btn_ofl.setFixedHeight(32)
        btn_ofl.setStyleSheet(self._btn_style())
        btn_ofl.clicked.connect(lambda: self._import_folder("ofl"))
        lay.addWidget(btn_ofl)

        btn_auto = QPushButton("⚡  Auto-import QLC+")
        btn_auto.setFixedHeight(32)
        btn_auto.setStyleSheet(
            f"QPushButton {{ background: {T.C_ACCENT}; color: white;"
            f" border: none; border-radius: 4px; padding: 0 12px; font-size: 11px; font-weight: 700; }}"
            f"QPushButton:hover {{ background: {T.C_ACCENT}dd; }}"
        )
        btn_auto.clicked.connect(self._auto_import)
        lay.addWidget(btn_auto)

        return bar

    def _build_detail_panel(self) -> QGroupBox:
        box = QGroupBox("Fixture Details")
        lay = QVBoxLayout(box)
        lay.setContentsMargins(8, 12, 8, 8)
        lay.setSpacing(4)

        # Info grid
        info = QWidget()
        gl = QGridLayout(info)
        gl.setContentsMargins(0, 0, 0, 0)
        gl.setSpacing(4)

        def _info_row(row: int, key: str) -> QLabel:
            kl = QLabel(key + ":")
            kl.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 10px;")
            vl = QLabel("—")
            vl.setStyleSheet(f"color: {T.C_TEXT}; font-size: 11px;")
            vl.setWordWrap(True)
            gl.addWidget(kl, row, 0)
            gl.addWidget(vl, row, 1)
            return vl

        self._d_mfr   = _info_row(0, "Manufacturer")
        self._d_model = _info_row(1, "Model")
        self._d_type  = _info_row(2, "Type")
        self._d_modes = _info_row(3, "Modes")
        self._d_phys  = _info_row(4, "Physical")
        lay.addWidget(info)

        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.HLine)
        sep.setStyleSheet(f"color: {T.C_BORDER};")
        lay.addWidget(sep)

        lbl_ch = QLabel("Channels")
        lbl_ch.setStyleSheet(f"color: {T.C_TEXT_LABEL}; font-size: 10px; font-weight: 700;")
        lay.addWidget(lbl_ch)

        # Mode selector for channel preview
        mode_row = QHBoxLayout()
        mode_row.setContentsMargins(0, 0, 0, 0)
        mode_lbl = QLabel("Mode:")
        mode_lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 10px;")
        mode_row.addWidget(mode_lbl)
        self._mode_combo = QComboBox()
        self._mode_combo.setFixedWidth(130)
        self._mode_combo.currentIndexChanged.connect(self._refresh_channels)
        mode_row.addWidget(self._mode_combo)
        mode_row.addStretch()
        lay.addLayout(mode_row)

        # Scrollable channel list
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        scroll.setMinimumHeight(180)
        self._ch_container = QWidget()
        self._ch_layout = QVBoxLayout(self._ch_container)
        self._ch_layout.setContentsMargins(0, 0, 0, 0)
        self._ch_layout.setSpacing(1)
        self._ch_layout.addStretch()
        scroll.setWidget(self._ch_container)
        lay.addWidget(scroll, 1)

        return box

    def _build_add_panel(self) -> QGroupBox:
        box = QGroupBox("Add to Patch")
        lay = QGridLayout(box)
        lay.setContentsMargins(8, 12, 8, 8)
        lay.setSpacing(6)

        def _lbl(txt: str) -> QLabel:
            l = QLabel(txt)
            l.setStyleSheet(f"color: {T.C_TEXT_LABEL}; font-size: 10px;")
            return l

        lay.addWidget(_lbl("Universe:"), 0, 0)
        self._spin_univ = QSpinBox()
        self._spin_univ.setRange(1, 8)
        self._spin_univ.setValue(1)
        self._spin_univ.setFixedWidth(60)
        lay.addWidget(self._spin_univ, 0, 1)

        lay.addWidget(_lbl("Address:"), 1, 0)
        self._spin_addr = QSpinBox()
        self._spin_addr.setRange(1, 512)
        self._spin_addr.setValue(1)
        self._spin_addr.setFixedWidth(70)
        lay.addWidget(self._spin_addr, 1, 1)

        lay.addWidget(_lbl("Quantity:"), 2, 0)
        self._spin_qty = QSpinBox()
        self._spin_qty.setRange(1, 64)
        self._spin_qty.setValue(1)
        self._spin_qty.setFixedWidth(60)
        lay.addWidget(self._spin_qty, 2, 1)

        lay.addWidget(_lbl("Mode:"), 3, 0)
        self._add_mode_combo = QComboBox()
        self._add_mode_combo.setMinimumWidth(130)
        lay.addWidget(self._add_mode_combo, 3, 1)

        self._btn_add = QPushButton("✚  Add to Patch")
        self._btn_add.setFixedHeight(40)
        self._btn_add.setEnabled(False)
        self._btn_add.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))
        self._btn_add.setStyleSheet(
            f"QPushButton {{ background: {T.C_ACCENT}; color: white;"
            f" border: none; border-radius: 4px; }}"
            f"QPushButton:hover {{ background: {T.C_ACCENT}cc; }}"
            f"QPushButton:disabled {{ background: {T.C_PANEL2}; color: {T.C_TEXT_DIM}; }}"
        )
        self._btn_add.clicked.connect(self._on_add)
        lay.addWidget(self._btn_add, 4, 0, 1, 2)

        return box

    def _build_bottom_bar(self) -> QWidget:
        bar = QWidget()
        lay = QHBoxLayout(bar)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.addStretch()
        btn_close = QPushButton("Close")
        btn_close.setFixedHeight(34)
        btn_close.setFixedWidth(90)
        btn_close.setStyleSheet(self._btn_style())
        btn_close.clicked.connect(self.reject)
        lay.addWidget(btn_close)
        return bar

    @staticmethod
    def _btn_style() -> str:
        return (
            f"QPushButton {{ background: {T.C_PANEL2}; color: {T.C_TEXT_LABEL};"
            f" border: 1px solid {T.C_BORDER}; border-radius: 4px;"
            f" padding: 0 12px; font-size: 11px; }}"
            f"QPushButton:hover {{ background: {T.C_PANEL}; color: {T.C_TEXT}; }}"
        )

    # ── Data ─────────────────────────────────────────────────────────────────

    def _refresh_tree(self) -> None:
        self._tree.clear()

        all_item = QTreeWidgetItem(["All Fixtures"])
        all_item.setData(0, Qt.ItemDataRole.UserRole, "__all__")
        all_item.setFont(0, QFont("Segoe UI", 10, QFont.Weight.Bold))
        self._tree.addTopLevelItem(all_item)

        builtin_item = QTreeWidgetItem(["Built-in"])
        builtin_item.setData(0, Qt.ItemDataRole.UserRole, "__builtin__")
        builtin_item.setFont(0, QFont("Segoe UI", 9))
        self._tree.addTopLevelItem(builtin_item)

        for mfr in self._lib.manufacturers:
            item = QTreeWidgetItem([mfr])
            item.setData(0, Qt.ItemDataRole.UserRole, mfr)
            self._tree.addTopLevelItem(item)

        self._tree.topLevelItem(0).setSelected(True)
        self._populate_list(self._lib.all_fixtures)

    def _populate_list(self, fixtures: list[FixtureDef]) -> None:
        self._list.clear()
        self._lbl_count.setText(f"{len(fixtures)} fixture{'s' if len(fixtures) != 1 else ''}")
        for fx in sorted(fixtures, key=lambda f: (f.manufacturer, f.model)):
            item = QListWidgetItem(f"{fx.manufacturer}  ·  {fx.model}")
            item.setData(Qt.ItemDataRole.UserRole, fx)
            item.setToolTip(f"{fx.fixture_type} — {fx.modes_summary}")
            self._list.addItem(item)

    def _show_fixture(self, fx: FixtureDef) -> None:
        self._selected = fx
        self._d_mfr.setText(fx.manufacturer)
        self._d_model.setText(fx.model)
        self._d_type.setText(fx.fixture_type)
        self._d_modes.setText(fx.modes_summary)

        p = fx.physical
        parts = []
        if p.weight_kg:   parts.append(f"{p.weight_kg:.1f} kg")
        if p.power_w:     parts.append(f"{p.power_w} W")
        if p.lens_max:    parts.append(f"{p.lens_min:.0f}–{p.lens_max:.0f}°")
        if p.pan_max:     parts.append(f"Pan {p.pan_max:.0f}°")
        if p.tilt_max:    parts.append(f"Tilt {p.tilt_max:.0f}°")
        self._d_phys.setText("  ".join(parts) or "—")

        # Populate mode combos
        self._mode_combo.blockSignals(True)
        self._mode_combo.clear()
        self._add_mode_combo.clear()
        for m in fx.modes:
            label = f"{m.name}  ({m.channel_count}ch)"
            self._mode_combo.addItem(label, m.name)
            self._add_mode_combo.addItem(label, m.name)
        self._mode_combo.blockSignals(False)

        self._refresh_channels()
        self._btn_add.setEnabled(True)

    def _refresh_channels(self) -> None:
        if not self._selected:
            return
        # Clear old rows (keep stretch)
        while self._ch_layout.count() > 1:
            item = self._ch_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        mode_name = self._mode_combo.currentData()
        mode = self._selected.mode_by_name(mode_name) if mode_name else self._selected.default_mode

        for i, ch_name in enumerate(mode.channels):
            ch = self._selected.channels.get(ch_name)
            if ch is None:
                ch_unknown = ChannelDef(name=ch_name or f"Ch {i+1}", type=__import__('fixtures.model', fromlist=['ChannelType']).ChannelType.NOTHING)
                ch = ch_unknown
            row = _ChannelRow(i, ch)
            self._ch_layout.insertWidget(self._ch_layout.count() - 1, row)

    def _clear_detail(self) -> None:
        self._selected = None
        self._btn_add.setEnabled(False)
        for lbl in (self._d_mfr, self._d_model, self._d_type, self._d_modes, self._d_phys):
            lbl.setText("—")
        self._mode_combo.clear()
        self._add_mode_combo.clear()
        while self._ch_layout.count() > 1:
            item = self._ch_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

    # ── Slots ─────────────────────────────────────────────────────────────────

    @Slot()
    def _on_tree_select(self) -> None:
        items = self._tree.selectedItems()
        if not items:
            return
        key = items[0].data(0, Qt.ItemDataRole.UserRole)
        query = self._search.text().strip()

        if key == "__all__":
            base = self._lib.all_fixtures
        elif key == "__builtin__":
            from fixtures.builtin import BUILTIN_FIXTURES
            base = list(BUILTIN_FIXTURES)
        else:
            base = self._lib.by_manufacturer(key)

        if query:
            q = query.lower()
            base = [f for f in base if q in f"{f.manufacturer} {f.model} {f.fixture_type}".lower()]

        self._populate_list(base)
        self._clear_detail()

    @Slot(str)
    def _on_search(self, text: str) -> None:
        q = text.strip().lower()
        items = self._tree.selectedItems()
        key = items[0].data(0, Qt.ItemDataRole.UserRole) if items else "__all__"

        if key == "__all__":
            base = self._lib.all_fixtures
        elif key == "__builtin__":
            from fixtures.builtin import BUILTIN_FIXTURES
            base = list(BUILTIN_FIXTURES)
        else:
            base = self._lib.by_manufacturer(key)

        if q:
            base = [f for f in base if q in f"{f.manufacturer} {f.model} {f.fixture_type}".lower()]

        self._populate_list(base)

    @Slot()
    def _on_fixture_select(self) -> None:
        items = self._list.selectedItems()
        if not items:
            self._clear_detail()
            return
        fx: FixtureDef = items[0].data(Qt.ItemDataRole.UserRole)
        if fx:
            self._show_fixture(fx)

    @Slot()
    def _on_add(self) -> None:
        if not self._selected:
            return
        mode_name = self._add_mode_combo.currentData() or self._selected.default_mode.name
        univ = self._spin_univ.value()
        addr = self._spin_addr.value()
        qty  = self._spin_qty.value()

        entries = []
        mode = self._selected.mode_by_name(mode_name) or self._selected.default_mode
        for i in range(qty):
            cur_addr = addr + i * mode.channel_count
            if cur_addr + mode.channel_count - 1 > 512:
                QMessageBox.warning(self, "Address overflow",
                    f"Not enough addresses in universe {univ} for all {qty} fixtures.")
                break
            entries.append((self._selected, mode_name, univ, cur_addr))

        if entries:
            self.fixtures_to_add.emit(entries)
            self.accept()

    # ── Import ────────────────────────────────────────────────────────────────

    def _import_folder(self, fmt: str) -> None:
        folder = QFileDialog.getExistingDirectory(
            self, f"Select {'QLC+' if fmt == 'qlcplus' else 'OFL'} Fixtures Folder")
        if not folder:
            return
        self._run_import(folder, fmt)

    def _auto_import(self) -> None:
        path = self._lib.autodetect_qlcplus()
        if not path:
            QMessageBox.information(self, "QLC+ not found",
                "QLC+ installation not detected. Please use 'Import QLC+ folder' to select manually.")
            return
        self._run_import(path, "qlcplus")

    def _run_import(self, folder: str, fmt: str) -> None:
        dlg = QProgressDialog(
            f"Importing fixtures from\n{folder}…", "Cancel", 0, 0, self)
        dlg.setWindowTitle("Importing…")
        dlg.setModal(True)
        dlg.setMinimumDuration(0)
        dlg.setValue(0)

        worker = _ImportWorker(folder, fmt)
        thread = QThread()
        worker.moveToThread(thread)
        thread.started.connect(worker.run)

        def on_done(count: int) -> None:
            thread.quit()
            dlg.close()
            self._refresh_tree()
            QMessageBox.information(self, "Import complete",
                f"Imported {count} fixture{'s' if count != 1 else ''} successfully.")

        def on_error(msg: str) -> None:
            thread.quit()
            dlg.close()
            QMessageBox.warning(self, "Import error", msg)

        worker.finished.connect(on_done)
        worker.error.connect(on_error)

        self._import_thread = thread
        self._import_worker = worker
        thread.start()
        dlg.exec()
