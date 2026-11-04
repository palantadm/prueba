"""
Cuelist panel — create/manage cuelists, record cues, transport controls.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QComboBox, QTableWidget, QTableWidgetItem, QHeaderView,
    QDoubleSpinBox, QLineEdit, QFrame, QSizePolicy,
    QAbstractItemView,
)
from PySide6.QtCore import Qt, Signal, Slot
from PySide6.QtGui import QColor, QFont, QBrush

from app.engine import LumosEngine, CueList, Cue
from app.ui import theme as T


def _make_hline() -> QFrame:
    f = QFrame()
    f.setFrameShape(QFrame.Shape.HLine)
    f.setStyleSheet(f"color: {T.C_BORDER};")
    return f


class CuelistPanel(QWidget):
    """Full cuelist editing and transport panel."""

    go_pressed = Signal(int)    # cuelist_id
    back_pressed = Signal(int)  # cuelist_id

    # Column indices
    _COL_NUM    = 0
    _COL_NAME   = 1
    _COL_FADEIN = 2
    _COL_FADEOUT= 3
    _COL_FOLLOW = 4

    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self._current_cl_id: Optional[int] = None
        self._updating_table = False
        self._build()
        self._refresh_cuelist_combo()

        engine.cuelist_changed.connect(self._on_cuelist_changed)
        engine.cue_activated.connect(self._on_cue_activated)

    # ── Build UI ─────────────────────────────────────────────────────────────

    def _build(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(6)

        # ── Top bar: cuelist management ────────────────────────────────────
        top = QHBoxLayout()
        top.setSpacing(6)

        lbl = QLabel("CUELIST")
        lbl.setStyleSheet(
            f"color: {T.C_TEXT_LABEL}; font-size: 10px; font-weight: 800;"
            f" letter-spacing: 1px;"
        )
        top.addWidget(lbl)

        self._combo = QComboBox()
        self._combo.setMinimumWidth(160)
        self._combo.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self._combo.currentIndexChanged.connect(self._on_combo_changed)
        top.addWidget(self._combo)

        btn_new = QPushButton("＋ New")
        btn_new.setFixedHeight(36)
        btn_new.setStyleSheet(
            f"background: {T.C_PANEL}; border: 1px solid {T.C_BORDER}; color: {T.C_TEXT};"
            f" font-weight: 700; border-radius: 3px; padding: 0 10px;"
        )
        btn_new.clicked.connect(self._new_cuelist)
        top.addWidget(btn_new)

        btn_del = QPushButton("✕ Delete")
        btn_del.setFixedHeight(36)
        btn_del.setStyleSheet(
            f"background: {T.C_PANEL}; border: 1px solid {T.C_RED}; color: {T.C_RED};"
            f" font-weight: 700; border-radius: 3px; padding: 0 10px;"
        )
        btn_del.clicked.connect(self._delete_cuelist)
        top.addWidget(btn_del)

        root.addLayout(top)
        root.addWidget(_make_hline())

        # ── Cue table ──────────────────────────────────────────────────────
        self._table = QTableWidget()
        self._table.setColumnCount(5)
        self._table.setHorizontalHeaderLabels(["Cue #", "Name", "Fade In", "Fade Out", "Follow"])
        self._table.setAlternatingRowColors(True)
        self._table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self._table.setEditTriggers(QAbstractItemView.EditTrigger.DoubleClicked)
        self._table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        self._table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        self._table.horizontalHeader().setSectionResizeMode(2, QHeaderView.ResizeMode.ResizeToContents)
        self._table.horizontalHeader().setSectionResizeMode(3, QHeaderView.ResizeMode.ResizeToContents)
        self._table.horizontalHeader().setSectionResizeMode(4, QHeaderView.ResizeMode.ResizeToContents)
        self._table.setMinimumHeight(200)
        self._table.itemChanged.connect(self._on_item_changed)
        root.addWidget(self._table, 1)

        # ── Transport bar ──────────────────────────────────────────────────
        root.addWidget(_make_hline())
        transport = QHBoxLayout()
        transport.setSpacing(8)

        self._btn_back = QPushButton("◀  BACK")
        self._btn_back.setFixedHeight(44)
        self._btn_back.setStyleSheet(
            f"background: {T.C_PANEL}; border: 1px solid {T.C_BORDER_LT};"
            f" color: {T.C_TEXT_LABEL}; font-weight: 800; font-size: 12px;"
            f" border-radius: 3px;"
        )
        self._btn_back.clicked.connect(self._on_back)
        transport.addWidget(self._btn_back)

        self._btn_go = QPushButton("▶  GO")
        self._btn_go.setObjectName("btn_go")
        self._btn_go.setFixedHeight(44)
        self._btn_go.clicked.connect(self._on_go)
        transport.addWidget(self._btn_go, 2)

        self._btn_stop = QPushButton("■  STOP")
        self._btn_stop.setObjectName("btn_stop")
        self._btn_stop.setFixedHeight(44)
        self._btn_stop.clicked.connect(self._on_stop)
        transport.addWidget(self._btn_stop)

        root.addLayout(transport)

        # ── Record cue bar ─────────────────────────────────────────────────
        root.addWidget(_make_hline())
        rec_row = QHBoxLayout()
        rec_row.setSpacing(6)

        rec_lbl = QLabel("Cue#")
        rec_lbl.setStyleSheet(f"color: {T.C_TEXT_LABEL}; font-size: 10px;")
        rec_row.addWidget(rec_lbl)

        self._spin_cue_num = QDoubleSpinBox()
        self._spin_cue_num.setRange(0.0, 9999.0)
        self._spin_cue_num.setSingleStep(1.0)
        self._spin_cue_num.setDecimals(1)
        self._spin_cue_num.setValue(1.0)
        self._spin_cue_num.setFixedWidth(70)
        self._spin_cue_num.setFixedHeight(36)
        rec_row.addWidget(self._spin_cue_num)

        name_lbl = QLabel("Name")
        name_lbl.setStyleSheet(f"color: {T.C_TEXT_LABEL}; font-size: 10px;")
        rec_row.addWidget(name_lbl)

        self._edit_cue_name = QLineEdit()
        self._edit_cue_name.setPlaceholderText("Cue name…")
        self._edit_cue_name.setFixedHeight(36)
        rec_row.addWidget(self._edit_cue_name, 1)

        uni_lbl = QLabel("Uni")
        uni_lbl.setStyleSheet(f"color: {T.C_TEXT_LABEL}; font-size: 10px;")
        rec_row.addWidget(uni_lbl)

        self._spin_uni = QDoubleSpinBox()
        self._spin_uni.setRange(1.0, 64.0)
        self._spin_uni.setSingleStep(1.0)
        self._spin_uni.setDecimals(0)
        self._spin_uni.setValue(1.0)
        self._spin_uni.setFixedWidth(56)
        self._spin_uni.setFixedHeight(36)
        rec_row.addWidget(self._spin_uni)

        btn_rec = QPushButton("⬤  REC CUE")
        btn_rec.setFixedHeight(36)
        btn_rec.setStyleSheet(
            f"background: #1a0a1a; border: 1px solid {T.C_EFFECT}; color: {T.C_EFFECT};"
            f" font-weight: 700; border-radius: 3px; padding: 0 12px;"
        )
        btn_rec.clicked.connect(self._record_cue)
        rec_row.addWidget(btn_rec)

        root.addLayout(rec_row)

        # ── Status label ───────────────────────────────────────────────────
        self._status_lbl = QLabel("No cuelist selected")
        self._status_lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 10px;")
        root.addWidget(self._status_lbl)

    # ── Cuelist combo ────────────────────────────────────────────────────────

    def _refresh_cuelist_combo(self) -> None:
        self._combo.blockSignals(True)
        self._combo.clear()
        for cl in self.engine.cuelists.values():
            self._combo.addItem(cl.name, cl.id)
        if self._current_cl_id is not None and self._current_cl_id in self.engine.cuelists:
            idx = list(self.engine.cuelists.keys()).index(self._current_cl_id)
            self._combo.setCurrentIndex(idx)
        elif self._combo.count() > 0:
            self._current_cl_id = self._combo.itemData(0)
        else:
            self._current_cl_id = None
        self._combo.blockSignals(False)
        self._refresh_table()

    # ── Table ────────────────────────────────────────────────────────────────

    def _refresh_table(self) -> None:
        self._updating_table = True
        self._table.setRowCount(0)
        if self._current_cl_id is None or self._current_cl_id not in self.engine.cuelists:
            self._updating_table = False
            self._status_lbl.setText("No cuelist selected")
            return
        cl = self.engine.cuelists[self._current_cl_id]
        self._table.setRowCount(len(cl.cues))
        for row, cue in enumerate(cl.cues):
            self._set_row(row, cue, cl)

        # Auto-advance cue number suggestion
        if cl.cues:
            last = cl.cues[-1].number
            next_num = round(last + 1.0, 1)
        else:
            next_num = 1.0
        self._spin_cue_num.setValue(next_num)

        cur = cl.current_cue
        self._status_lbl.setText(
            f"{len(cl.cues)} cues — current: {cur.number} '{cur.name}'" if cur
            else f"{len(cl.cues)} cues — press GO to start"
        )
        self._updating_table = False
        self._highlight_current_row(cl)

    def _set_row(self, row: int, cue: Cue, cl: CueList) -> None:
        num_item = QTableWidgetItem(f"{cue.number:.1f}")
        num_item.setFlags(num_item.flags() & ~Qt.ItemFlag.ItemIsEditable)
        num_item.setTextAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        self._table.setItem(row, self._COL_NUM, num_item)

        name_item = QTableWidgetItem(cue.name)
        self._table.setItem(row, self._COL_NAME, name_item)

        fi_item = QTableWidgetItem(f"{cue.fade_in:.1f}")
        self._table.setItem(row, self._COL_FADEIN, fi_item)

        fo_item = QTableWidgetItem(f"{cue.fade_out:.1f}")
        self._table.setItem(row, self._COL_FADEOUT, fo_item)

        fw_item = QTableWidgetItem(f"{cue.follow_wait:.1f}" if cue.follow_wait > 0 else "—")
        self._table.setItem(row, self._COL_FOLLOW, fw_item)

    def _highlight_current_row(self, cl: CueList) -> None:
        accent = QColor(T.C_ACCENT)
        accent.setAlpha(60)
        dim = QColor(T.C_TEXT_DIM)
        for row in range(self._table.rowCount()):
            is_current = row == cl._current_idx
            for col in range(self._table.columnCount()):
                item = self._table.item(row, col)
                if item:
                    if is_current:
                        item.setBackground(QBrush(accent))
                        item.setForeground(QBrush(QColor(T.C_TEXT_HDR)))
                    else:
                        item.setBackground(QBrush(QColor(T.C_PANEL if row % 2 == 0 else T.C_PANEL2)))
                        item.setForeground(QBrush(QColor(T.C_TEXT)))

    # ── Slots ────────────────────────────────────────────────────────────────

    @Slot(int)
    def _on_combo_changed(self, idx: int) -> None:
        if idx < 0:
            self._current_cl_id = None
        else:
            self._current_cl_id = self._combo.itemData(idx)
        self._refresh_table()

    @Slot(int)
    def _on_cuelist_changed(self, cuelist_id: int) -> None:
        self._refresh_cuelist_combo()

    @Slot(int, float)
    def _on_cue_activated(self, cuelist_id: int, cue_number: float) -> None:
        if cuelist_id == self._current_cl_id and cuelist_id in self.engine.cuelists:
            cl = self.engine.cuelists[cuelist_id]
            self._highlight_current_row(cl)
            cur = cl.current_cue
            self._status_lbl.setText(
                f"CUE {cue_number:.1f} — {cur.name}" if cur else ""
            )
            # Scroll to current row
            if cl._current_idx >= 0:
                self._table.scrollToItem(self._table.item(cl._current_idx, 0))

    @Slot()
    def _on_go(self) -> None:
        if self._current_cl_id is not None:
            self.engine.go(self._current_cl_id)
            self.go_pressed.emit(self._current_cl_id)

    @Slot()
    def _on_back(self) -> None:
        if self._current_cl_id is not None:
            self.engine.back(self._current_cl_id)
            self.back_pressed.emit(self._current_cl_id)

    @Slot()
    def _on_stop(self) -> None:
        if self._current_cl_id is not None:
            self.engine.stop_fade(self._current_cl_id)

    @Slot()
    def _new_cuelist(self) -> None:
        n = len(self.engine.cuelists) + 1
        cl = self.engine.create_cuelist(f"Cuelist {n}")
        self._current_cl_id = cl.id
        self._refresh_cuelist_combo()

    @Slot()
    def _delete_cuelist(self) -> None:
        if self._current_cl_id is not None:
            self.engine.delete_cuelist(self._current_cl_id)
            self._current_cl_id = None
            self._refresh_cuelist_combo()

    @Slot()
    def _record_cue(self) -> None:
        if self._current_cl_id is None:
            # Auto-create a cuelist if none exists
            cl = self.engine.create_cuelist("Cuelist 1")
            self._current_cl_id = cl.id

        cue_number = self._spin_cue_num.value()
        cue_name = self._edit_cue_name.text().strip() or f"Cue {cue_number:.1f}"
        universe = int(self._spin_uni.value())

        self.engine.record_cue(
            self._current_cl_id,
            cue_number,
            cue_name,
            universe,
        )
        self._edit_cue_name.clear()

    @Slot(QTableWidgetItem)
    def _on_item_changed(self, item: QTableWidgetItem) -> None:
        if self._updating_table:
            return
        if self._current_cl_id is None or self._current_cl_id not in self.engine.cuelists:
            return
        cl = self.engine.cuelists[self._current_cl_id]
        row = item.row()
        col = item.column()
        if row >= len(cl.cues):
            return
        cue = cl.cues[row]
        text = item.text().strip()
        try:
            if col == self._COL_NAME:
                cue.name = text
            elif col == self._COL_FADEIN:
                cue.fade_in = max(0.0, float(text))
                item.setText(f"{cue.fade_in:.1f}")
            elif col == self._COL_FADEOUT:
                cue.fade_out = max(0.0, float(text))
                item.setText(f"{cue.fade_out:.1f}")
            elif col == self._COL_FOLLOW:
                if text in ("—", "-", ""):
                    cue.follow_wait = 0.0
                    item.setText("—")
                else:
                    cue.follow_wait = max(0.0, float(text))
                    item.setText(f"{cue.follow_wait:.1f}")
        except ValueError:
            # Restore original value
            self._refresh_table()
