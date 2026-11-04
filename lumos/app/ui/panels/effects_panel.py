"""
Effects Panel — manage and configure lighting effects.

Left sidebar: list of effects + add/remove controls.
Right area: effect editor (common params + type-specific).
Bottom: targets management.
"""
from __future__ import annotations

from typing import Optional

from PySide6.QtCore import Qt, Slot
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QDoubleSpinBox,
    QFormLayout,
    QGroupBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QSpinBox,
    QSplitter,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from app.effects import (
    ChaseEffect,
    ColorLoopEffect,
    Effect,
    EffectTarget,
    EffectsManager,
    PulseEffect,
    WaveEffect,
)
from app.ui import theme as T


# ---------------------------------------------------------------------------
# Add-target dialog
# ---------------------------------------------------------------------------

class AddTargetDialog(QDialog):
    """Dialog for picking universe / start channel / channel count."""

    def __init__(self, universes: list[int], parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("Add Target")
        self.setMinimumWidth(300)

        layout = QFormLayout(self)
        layout.setContentsMargins(12, 12, 12, 12)
        layout.setSpacing(8)

        self._uni_cb = QComboBox()
        for u in universes:
            self._uni_cb.addItem(f"Universe {u}", u)
        layout.addRow("Universe:", self._uni_cb)

        self._start_spin = QSpinBox()
        self._start_spin.setRange(1, 512)
        self._start_spin.setValue(1)
        layout.addRow("Start Channel:", self._start_spin)

        self._count_spin = QSpinBox()
        self._count_spin.setRange(1, 512)
        self._count_spin.setValue(3)
        layout.addRow("Channel Count:", self._count_spin)

        btns = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        btns.accepted.connect(self.accept)
        btns.rejected.connect(self.reject)
        layout.addRow(btns)

    def target(self) -> EffectTarget:
        return EffectTarget(
            universe=self._uni_cb.currentData(),
            start_channel=self._start_spin.value(),
            channel_count=self._count_spin.value(),
        )


# ---------------------------------------------------------------------------
# Chase step editor widget
# ---------------------------------------------------------------------------

class ChaseStepsEditor(QWidget):
    """A simple table editor for Chase steps (one row = one step, columns = channels)."""

    MAX_CHANNELS = 8

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._effect: Optional[ChaseEffect] = None

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(4)

        # Step time
        row = QHBoxLayout()
        row.addWidget(QLabel("Step time (ms):"))
        self._step_time = QDoubleSpinBox()
        self._step_time.setRange(50, 30000)
        self._step_time.setValue(500)
        self._step_time.setSingleStep(50)
        self._step_time.valueChanged.connect(self._on_step_time_changed)
        row.addWidget(self._step_time)
        row.addStretch()
        layout.addLayout(row)

        # Table
        self._table = QTableWidget(0, self.MAX_CHANNELS)
        self._table.setHorizontalHeaderLabels(
            [f"Ch {i+1}" for i in range(self.MAX_CHANNELS)]
        )
        self._table.verticalHeader().setDefaultSectionSize(28)
        self._table.horizontalHeader().setDefaultSectionSize(52)
        self._table.setFixedHeight(160)
        self._table.cellChanged.connect(self._on_cell_changed)
        layout.addWidget(self._table)

        # Buttons
        btn_row = QHBoxLayout()
        btn_add = QPushButton("+ Step")
        btn_add.setFixedHeight(28)
        btn_add.clicked.connect(self._add_step)
        btn_remove = QPushButton("- Step")
        btn_remove.setFixedHeight(28)
        btn_remove.clicked.connect(self._remove_step)
        btn_row.addWidget(btn_add)
        btn_row.addWidget(btn_remove)
        btn_row.addStretch()
        layout.addLayout(btn_row)

    def set_effect(self, effect: ChaseEffect) -> None:
        self._effect = effect
        self._step_time.blockSignals(True)
        self._step_time.setValue(effect.step_time_ms)
        self._step_time.blockSignals(False)
        self._refresh_table()

    def _refresh_table(self) -> None:
        if not self._effect:
            return
        self._table.blockSignals(True)
        self._table.setRowCount(len(self._effect.steps))
        for r, step in enumerate(self._effect.steps):
            for c in range(self.MAX_CHANNELS):
                val = step[c] if c < len(step) else 0
                item = QTableWidgetItem(str(val))
                item.setTextAlignment(Qt.AlignmentFlag.AlignCenter)
                self._table.setItem(r, c, item)
        self._table.blockSignals(False)

    def _on_step_time_changed(self, v: float) -> None:
        if self._effect:
            self._effect.step_time_ms = v

    def _on_cell_changed(self, row: int, col: int) -> None:
        if not self._effect:
            return
        item = self._table.item(row, col)
        if item is None:
            return
        try:
            val = max(0, min(255, int(item.text())))
        except ValueError:
            val = 0
        while len(self._effect.steps) <= row:
            self._effect.steps.append([0] * self.MAX_CHANNELS)
        while len(self._effect.steps[row]) <= col:
            self._effect.steps[row].append(0)
        self._effect.steps[row][col] = val

    def _add_step(self) -> None:
        if not self._effect:
            return
        self._effect.steps.append([0] * self.MAX_CHANNELS)
        self._refresh_table()

    def _remove_step(self) -> None:
        if not self._effect:
            return
        row = self._table.currentRow()
        if row < 0:
            row = len(self._effect.steps) - 1
        if 0 <= row < len(self._effect.steps):
            del self._effect.steps[row]
        self._refresh_table()


# ---------------------------------------------------------------------------
# Effect editor panel
# ---------------------------------------------------------------------------

class EffectEditor(QWidget):
    """Right-side editor; adapts its layout to the selected effect type."""

    def __init__(self, manager: EffectsManager, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._manager = manager
        self._effect: Optional[Effect] = None

        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(8)

        # -- Common params ---------------------------------------------------
        common_grp = QGroupBox("Common")
        common_form = QFormLayout(common_grp)
        common_form.setContentsMargins(8, 16, 8, 8)
        common_form.setSpacing(6)

        self._name_edit = QLineEdit()
        self._name_edit.textChanged.connect(self._on_name_changed)
        common_form.addRow("Name:", self._name_edit)

        self._enabled_cb = QCheckBox("Enabled")
        self._enabled_cb.toggled.connect(self._on_enabled_changed)
        common_form.addRow("", self._enabled_cb)

        self._speed_spin = QDoubleSpinBox()
        self._speed_spin.setRange(0.1, 10.0)
        self._speed_spin.setSingleStep(0.1)
        self._speed_spin.setDecimals(2)
        self._speed_spin.setSuffix(" Hz")
        self._speed_spin.valueChanged.connect(self._on_speed_changed)
        common_form.addRow("Speed:", self._speed_spin)

        self._size_spin = QDoubleSpinBox()
        self._size_spin.setRange(0.0, 1.0)
        self._size_spin.setSingleStep(0.05)
        self._size_spin.setDecimals(2)
        self._size_spin.valueChanged.connect(self._on_size_changed)
        common_form.addRow("Size:", self._size_spin)

        self._offset_spin = QDoubleSpinBox()
        self._offset_spin.setRange(0.0, 1.0)
        self._offset_spin.setSingleStep(0.05)
        self._offset_spin.setDecimals(2)
        self._offset_spin.valueChanged.connect(self._on_offset_changed)
        common_form.addRow("Offset:", self._offset_spin)

        root.addWidget(common_grp)

        # -- Type-specific params --------------------------------------------
        self._type_grp = QGroupBox("Parameters")
        self._type_layout = QVBoxLayout(self._type_grp)
        self._type_layout.setContentsMargins(8, 16, 8, 8)
        root.addWidget(self._type_grp)

        # Chase steps editor
        self._chase_editor = ChaseStepsEditor()
        self._type_layout.addWidget(self._chase_editor)
        self._chase_editor.hide()

        # Pulse / Wave base spin
        base_row = QHBoxLayout()
        base_row.addWidget(QLabel("Base intensity:"))
        self._base_spin = QSpinBox()
        self._base_spin.setRange(0, 255)
        self._base_spin.valueChanged.connect(self._on_base_changed)
        base_row.addWidget(self._base_spin)
        base_row.addStretch()
        self._base_widget = QWidget()
        self._base_widget.setLayout(base_row)
        self._type_layout.addWidget(self._base_widget)
        self._base_widget.hide()

        # Color loop: hint label
        self._colorloop_label = QLabel(
            "Rotates through the full HSV hue range.\n"
            "Speed = full rotation / sec.  Size = saturation."
        )
        self._colorloop_label.setWordWrap(True)
        self._colorloop_label.setStyleSheet(f"color: {T.C_TEXT_DIM};")
        self._type_layout.addWidget(self._colorloop_label)
        self._colorloop_label.hide()

        # -- Targets ---------------------------------------------------------
        tgt_grp = QGroupBox("Targets")
        tgt_lay = QVBoxLayout(tgt_grp)
        tgt_lay.setContentsMargins(8, 16, 8, 8)
        tgt_lay.setSpacing(4)

        self._target_list = QListWidget()
        self._target_list.setFixedHeight(100)
        tgt_lay.addWidget(self._target_list)

        tgt_btn_row = QHBoxLayout()
        self._btn_add_tgt = QPushButton("+ Add Target")
        self._btn_add_tgt.setFixedHeight(28)
        self._btn_add_tgt.clicked.connect(self._add_target)
        self._btn_remove_tgt = QPushButton("- Remove")
        self._btn_remove_tgt.setFixedHeight(28)
        self._btn_remove_tgt.clicked.connect(self._remove_target)
        tgt_btn_row.addWidget(self._btn_add_tgt)
        tgt_btn_row.addWidget(self._btn_remove_tgt)
        tgt_btn_row.addStretch()
        tgt_lay.addLayout(tgt_btn_row)

        root.addWidget(tgt_grp)
        root.addStretch()

        self.setEnabled(False)

    # ------------------------------------------------------------------
    # Load effect into editor
    # ------------------------------------------------------------------

    def load_effect(self, effect: Optional[Effect]) -> None:
        self._effect = effect
        self.setEnabled(effect is not None)
        if effect is None:
            return

        # Block signals while populating
        for w in (self._speed_spin, self._size_spin, self._offset_spin,
                  self._name_edit, self._enabled_cb, self._base_spin):
            w.blockSignals(True)

        self._name_edit.setText(effect.name)
        self._enabled_cb.setChecked(effect.enabled)
        self._speed_spin.setValue(effect.speed)
        self._size_spin.setValue(effect.size)
        self._offset_spin.setValue(effect.offset)

        for w in (self._speed_spin, self._size_spin, self._offset_spin,
                  self._name_edit, self._enabled_cb, self._base_spin):
            w.blockSignals(False)

        # Type-specific
        self._chase_editor.hide()
        self._base_widget.hide()
        self._colorloop_label.hide()

        if isinstance(effect, ChaseEffect):
            self._type_grp.setTitle("Chase Steps")
            self._chase_editor.set_effect(effect)
            self._chase_editor.show()
        elif isinstance(effect, ColorLoopEffect):
            self._type_grp.setTitle("Color Loop")
            self._colorloop_label.show()
        elif isinstance(effect, (PulseEffect, WaveEffect)):
            self._type_grp.setTitle("Parameters")
            self._base_spin.blockSignals(True)
            self._base_spin.setValue(effect.base)
            self._base_spin.blockSignals(False)
            self._base_widget.show()
        else:
            self._type_grp.setTitle("Parameters")

        # Targets
        self._refresh_targets()

    def _refresh_targets(self) -> None:
        self._target_list.clear()
        if not self._effect:
            return
        for t in self._effect.targets:
            self._target_list.addItem(
                f"Uni {t.universe}  Ch {t.start_channel}–{t.start_channel + t.channel_count - 1}"
            )

    # ------------------------------------------------------------------
    # Slots
    # ------------------------------------------------------------------

    def _on_name_changed(self, text: str) -> None:
        if self._effect:
            self._effect.name = text

    def _on_enabled_changed(self, checked: bool) -> None:
        if self._effect:
            self._effect.enabled = checked

    def _on_speed_changed(self, v: float) -> None:
        if self._effect:
            self._effect.speed = v

    def _on_size_changed(self, v: float) -> None:
        if self._effect:
            self._effect.size = v

    def _on_offset_changed(self, v: float) -> None:
        if self._effect:
            self._effect.offset = v

    def _on_base_changed(self, v: int) -> None:
        if self._effect and isinstance(self._effect, (PulseEffect, WaveEffect)):
            self._effect.base = v

    def _add_target(self) -> None:
        if not self._effect:
            return
        universes = list(self._manager.engine.universes.keys())
        dlg = AddTargetDialog(universes, self)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            self._effect.targets.append(dlg.target())
            self._refresh_targets()

    def _remove_target(self) -> None:
        if not self._effect:
            return
        row = self._target_list.currentRow()
        if 0 <= row < len(self._effect.targets):
            del self._effect.targets[row]
            self._refresh_targets()


# ---------------------------------------------------------------------------
# Effects Panel (top-level widget)
# ---------------------------------------------------------------------------

_EFFECT_TYPES = {
    "Chase": ChaseEffect,
    "Color Loop": ColorLoopEffect,
    "Pulse": PulseEffect,
    "Wave": WaveEffect,
}


class EffectsPanel(QWidget):
    """
    Main effects panel.  Place it in the main window's tab bar.

    Left side: list of effects + add/remove toolbar.
    Right side: :class:`EffectEditor` that edits the selected effect.
    """

    def __init__(self, manager: EffectsManager, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._manager = manager

        root = QHBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        splitter = QSplitter(Qt.Orientation.Horizontal)
        root.addWidget(splitter)

        # ── Left sidebar ────────────────────────────────────────────────────
        sidebar = QWidget()
        sidebar.setMinimumWidth(180)
        sidebar.setMaximumWidth(260)
        sidebar_lay = QVBoxLayout(sidebar)
        sidebar_lay.setContentsMargins(8, 8, 8, 8)
        sidebar_lay.setSpacing(6)

        lbl = QLabel("Effects")
        lbl.setStyleSheet(
            f"color: {T.C_TEXT_LABEL}; font-size: 10px; font-weight: 700; "
            "text-transform: uppercase; letter-spacing: 1px;"
        )
        sidebar_lay.addWidget(lbl)

        self._list = QListWidget()
        self._list.currentRowChanged.connect(self._on_selection_changed)
        sidebar_lay.addWidget(self._list)

        # Type selector + add button
        self._type_cb = QComboBox()
        for name in _EFFECT_TYPES:
            self._type_cb.addItem(name)
        sidebar_lay.addWidget(self._type_cb)

        btn_add = QPushButton("＋ Add Effect")
        btn_add.setFixedHeight(36)
        btn_add.clicked.connect(self._add_effect)
        sidebar_lay.addWidget(btn_add)

        btn_remove = QPushButton("✕ Remove")
        btn_remove.setFixedHeight(36)
        btn_remove.clicked.connect(self._remove_effect)
        sidebar_lay.addWidget(btn_remove)

        splitter.addWidget(sidebar)

        # ── Right editor ────────────────────────────────────────────────────
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(scroll.Shape.NoFrame)
        scroll.setStyleSheet(f"background: {T.C_PANEL};")

        self._editor = EffectEditor(manager)
        scroll.setWidget(self._editor)
        splitter.addWidget(scroll)

        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)

    # ------------------------------------------------------------------
    # Slots
    # ------------------------------------------------------------------

    @Slot()
    def _add_effect(self) -> None:
        type_name = self._type_cb.currentText()
        cls = _EFFECT_TYPES.get(type_name, ChaseEffect)
        fx = cls()
        fx.name = f"{type_name} {len(self._manager.effects()) + 1}"
        self._manager.add_effect(fx)
        item = QListWidgetItem(fx.name)
        item.setData(Qt.ItemDataRole.UserRole, fx)
        self._list.addItem(item)
        self._list.setCurrentRow(self._list.count() - 1)

    @Slot()
    def _remove_effect(self) -> None:
        row = self._list.currentRow()
        if row < 0:
            return
        item = self._list.takeItem(row)
        fx = item.data(Qt.ItemDataRole.UserRole)
        self._manager.remove_effect(fx)
        self._editor.load_effect(None)

    @Slot(int)
    def _on_selection_changed(self, row: int) -> None:
        if row < 0:
            self._editor.load_effect(None)
            return
        item = self._list.item(row)
        if item is None:
            self._editor.load_effect(None)
            return
        fx = item.data(Qt.ItemDataRole.UserRole)
        self._editor.load_effect(fx)
        # Keep list label in sync (name may have changed)
        item.setText(fx.name)
