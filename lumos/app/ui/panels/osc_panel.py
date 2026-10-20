"""
OSC Mapper Panel — configure OSC output routes and the OSC input server.

Layout:
  Left  — route list + preset loader + quick-send test section
  Right — full route editor with live value preview
"""
from __future__ import annotations

from typing import Optional

from PySide6.QtCore import Qt, QTimer, Slot
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (
    QButtonGroup,
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
    QMenu,
    QProgressBar,
    QPushButton,
    QRadioButton,
    QScrollArea,
    QSizePolicy,
    QSpinBox,
    QSplitter,
    QVBoxLayout,
    QWidget,
)

from app.osc.manager import OSCManager
from app.osc.route import OSCRoute
from app.osc import presets as P
from app.ui import theme as T


# ---------------------------------------------------------------------------
# Activity dot widget — flashes green on each OSC send
# ---------------------------------------------------------------------------

class _ActivityDot(QLabel):
    """Small coloured dot that briefly flashes on activity."""

    _OFF_COLOR  = T.C_BORDER_LT
    _ON_COLOR   = T.C_GREEN

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setFixedSize(8, 8)
        self._active = False
        self._set_color(self._OFF_COLOR)
        self._fade_timer = QTimer(self)
        self._fade_timer.setSingleShot(True)
        self._fade_timer.setInterval(180)
        self._fade_timer.timeout.connect(self._fade)

    def _set_color(self, color: str) -> None:
        self.setStyleSheet(
            f"background:{color}; border-radius:4px; min-width:8px; min-height:8px;"
        )

    def flash(self) -> None:
        self._set_color(self._ON_COLOR)
        self._fade_timer.start()

    @Slot()
    def _fade(self) -> None:
        self._set_color(self._OFF_COLOR)


# ---------------------------------------------------------------------------
# Route list item (with activity dot)
# ---------------------------------------------------------------------------

class _RouteItem(QListWidgetItem):
    def __init__(self, route: OSCRoute) -> None:
        super().__init__()
        self.route_id = route.id
        self.refresh(route)

    def refresh(self, route: OSCRoute) -> None:
        prefix = "" if route.enabled else "○ "
        self.setText(f"{prefix}{route.label or route.address}")
        self.setToolTip(
            f"U{route.universe} Ch{route.channel} → "
            f"{route.host}:{route.port}{route.address}"
        )
        color = QColor(T.C_TEXT) if route.enabled else QColor(T.C_TEXT_DIM)
        self.setForeground(color)


# ---------------------------------------------------------------------------
# Preset selection dialog
# ---------------------------------------------------------------------------

class _PresetDialog(QDialog):
    """Pick an app + route type from the preset catalog."""

    def __init__(
        self,
        host: str,
        port: int,
        universe: int,
        channel: int,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self.setWindowTitle("Insert Preset Route")
        self.setMinimumWidth(400)
        self._host = host
        self._port = port
        self._universe = universe
        self._channel = channel
        self._result_route: Optional[OSCRoute] = None

        layout = QVBoxLayout(self)

        # App selector
        app_row = QHBoxLayout()
        app_row.addWidget(QLabel("App:"))
        self._app_combo = QComboBox()
        for app_name in P.PRESET_CATALOG:
            self._app_combo.addItem(app_name)
        app_row.addWidget(self._app_combo, 1)
        layout.addLayout(app_row)

        # Route type selector
        type_row = QHBoxLayout()
        type_row.addWidget(QLabel("Route:"))
        self._type_combo = QComboBox()
        type_row.addWidget(self._type_combo, 1)
        layout.addLayout(type_row)

        # Extra params section (simple key=value form)
        self._params_group = QGroupBox("Parameters")
        self._params_layout = QFormLayout()
        self._params_group.setLayout(self._params_layout)
        layout.addWidget(self._params_group)

        # DMX source row
        src_row = QHBoxLayout()
        src_row.addWidget(QLabel("Universe:"))
        self._uni_spin = QSpinBox()
        self._uni_spin.setRange(1, 64)
        self._uni_spin.setValue(universe)
        src_row.addWidget(self._uni_spin)
        src_row.addWidget(QLabel("Channel:"))
        self._ch_spin = QSpinBox()
        self._ch_spin.setRange(1, 512)
        self._ch_spin.setValue(channel)
        src_row.addWidget(self._ch_spin)
        layout.addLayout(src_row)

        btns = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        btns.accepted.connect(self._accept)
        btns.rejected.connect(self.reject)
        layout.addWidget(btns)

        self._app_combo.currentTextChanged.connect(self._populate_types)
        self._type_combo.currentIndexChanged.connect(self._populate_params)
        self._populate_types(self._app_combo.currentText())

    def _populate_types(self, app_name: str) -> None:
        self._type_combo.clear()
        entries = P.PRESET_CATALOG.get(app_name, [])
        for entry in entries:
            self._type_combo.addItem(entry["description"], userData=entry)

    def _populate_params(self) -> None:
        # Clear previous param widgets
        while self._params_layout.rowCount() > 0:
            self._params_layout.removeRow(0)
        self._param_widgets: dict[str, QWidget] = {}

        entry = self._type_combo.currentData()
        if entry is None:
            return
        defaults = entry.get("defaults", {})
        for param_name in entry.get("params", []):
            default_val = defaults.get(param_name, "")
            if isinstance(default_val, int):
                widget: QWidget = QSpinBox()
                widget.setRange(0, 9999)
                widget.setValue(default_val)
            elif isinstance(default_val, float):
                widget = QDoubleSpinBox()
                widget.setDecimals(3)
                widget.setValue(default_val)
            else:
                widget = QLineEdit(str(default_val))
            self._params_layout.addRow(param_name, widget)
            self._param_widgets[param_name] = widget

        self._params_group.setVisible(bool(entry.get("params")))

    def _accept(self) -> None:
        entry = self._type_combo.currentData()
        if entry is None:
            self.reject()
            return
        func_name = entry["func"]
        kwargs: dict = {}
        for param_name, widget in getattr(self, "_param_widgets", {}).items():
            if isinstance(widget, QSpinBox):
                kwargs[param_name] = widget.value()
            elif isinstance(widget, QDoubleSpinBox):
                kwargs[param_name] = widget.value()
            else:
                # QLineEdit — try int, then float, then string
                text = widget.text().strip()
                try:
                    kwargs[param_name] = int(text)
                except ValueError:
                    try:
                        kwargs[param_name] = float(text)
                    except ValueError:
                        kwargs[param_name] = text
        try:
            self._result_route = P.create_preset(
                func_name,
                host=self._host,
                port=self._port,
                universe=self._uni_spin.value(),
                channel=self._ch_spin.value(),
                **kwargs,
            )
            self.accept()
        except Exception as exc:
            from PySide6.QtWidgets import QMessageBox
            QMessageBox.warning(self, "Preset Error", str(exc))

    @property
    def result_route(self) -> Optional[OSCRoute]:
        return self._result_route


# ---------------------------------------------------------------------------
# Route editor (right pane)
# ---------------------------------------------------------------------------

class _RouteEditor(QWidget):
    """Edit one OSCRoute in place; changes are applied immediately."""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._route: Optional[OSCRoute] = None
        self._updating = False

        root = QVBoxLayout(self)
        root.setContentsMargins(12, 8, 12, 8)
        root.setSpacing(6)

        # Header row: label + enabled checkbox
        hdr = QHBoxLayout()
        hdr.addWidget(QLabel("Label:"))
        self._lbl_edit = QLineEdit()
        self._lbl_edit.setPlaceholderText("Route label")
        hdr.addWidget(self._lbl_edit, 1)
        hdr.addWidget(QLabel("Enabled:"))
        self._enabled_chk = QCheckBox()
        self._enabled_chk.setChecked(True)
        hdr.addWidget(self._enabled_chk)
        root.addLayout(hdr)

        # DMX SOURCE
        src_grp = QGroupBox("DMX Source")
        src_lay = QHBoxLayout(src_grp)
        src_lay.addWidget(QLabel("Universe:"))
        self._uni_spin = QSpinBox()
        self._uni_spin.setRange(1, 64)
        src_lay.addWidget(self._uni_spin)
        src_lay.addSpacing(12)
        src_lay.addWidget(QLabel("Channel:"))
        self._ch_spin = QSpinBox()
        self._ch_spin.setRange(1, 512)
        src_lay.addWidget(self._ch_spin)
        src_lay.addStretch()
        root.addWidget(src_grp)

        # OSC DESTINATION
        dst_grp = QGroupBox("OSC Destination")
        dst_lay = QFormLayout(dst_grp)
        self._host_edit = QLineEdit("127.0.0.1")
        self._port_spin = QSpinBox()
        self._port_spin.setRange(1, 65535)
        self._port_spin.setValue(8000)
        self._addr_edit = QLineEdit("/lumos/channel")
        dst_lay.addRow("Host:", self._host_edit)
        dst_lay.addRow("Port:", self._port_spin)
        dst_lay.addRow("Address:", self._addr_edit)
        root.addWidget(dst_grp)

        # VALUE TRANSFORM
        tx_grp = QGroupBox("Value Transform")
        tx_lay = QFormLayout(tx_grp)
        type_row = QHBoxLayout()
        self._type_combo = QComboBox()
        for t in ("float", "int", "bool", "string"):
            self._type_combo.addItem(t)
        type_row.addWidget(self._type_combo)
        tx_lay.addRow("Type:", type_row)

        in_row = QHBoxLayout()
        self._in_min_spin = QSpinBox()
        self._in_min_spin.setRange(0, 255)
        self._in_max_spin = QSpinBox()
        self._in_max_spin.setRange(0, 255)
        self._in_max_spin.setValue(255)
        in_row.addWidget(self._in_min_spin)
        in_row.addWidget(QLabel("–"))
        in_row.addWidget(self._in_max_spin)
        tx_lay.addRow("In:", in_row)

        out_row = QHBoxLayout()
        self._out_min_spin = QDoubleSpinBox()
        self._out_min_spin.setRange(-99999, 99999)
        self._out_min_spin.setDecimals(3)
        self._out_max_spin = QDoubleSpinBox()
        self._out_max_spin.setRange(-99999, 99999)
        self._out_max_spin.setDecimals(3)
        self._out_max_spin.setValue(1.0)
        out_row.addWidget(self._out_min_spin)
        out_row.addWidget(QLabel("–"))
        out_row.addWidget(self._out_max_spin)
        tx_lay.addRow("Out:", out_row)

        str_row = QHBoxLayout()
        self._on_edit = QLineEdit("on")
        self._off_edit = QLineEdit("off")
        str_row.addWidget(QLabel("On:"))
        str_row.addWidget(self._on_edit)
        str_row.addWidget(QLabel("Off:"))
        str_row.addWidget(self._off_edit)
        tx_lay.addRow("String:", str_row)
        root.addWidget(tx_grp)

        # TRIGGER MODE
        trig_grp = QGroupBox("Trigger Mode")
        trig_lay = QVBoxLayout(trig_grp)
        mode_row = QHBoxLayout()
        self._mode_group = QButtonGroup(self)
        for i, (key, label) in enumerate([
            ("continuous", "Continuous"),
            ("on_change", "On Change"),
            ("threshold_cross", "Threshold"),
        ]):
            rb = QRadioButton(label)
            rb.setProperty("mode_key", key)
            if i == 0:
                rb.setChecked(True)
            self._mode_group.addButton(rb, i)
            mode_row.addWidget(rb)
        trig_lay.addLayout(mode_row)

        thr_row = QHBoxLayout()
        thr_row.addWidget(QLabel("Threshold:"))
        self._threshold_spin = QSpinBox()
        self._threshold_spin.setRange(0, 255)
        self._threshold_spin.setValue(128)
        thr_row.addWidget(self._threshold_spin)
        thr_row.addStretch()
        trig_lay.addLayout(thr_row)
        root.addWidget(trig_grp)

        # LIVE VALUE preview
        live_grp = QGroupBox("Live Value")
        live_lay = QHBoxLayout(live_grp)
        self._live_bar = QProgressBar()
        self._live_bar.setRange(0, 255)
        self._live_bar.setValue(0)
        self._live_bar.setFixedHeight(12)
        self._live_bar.setTextVisible(False)
        self._live_bar.setStyleSheet(
            f"QProgressBar {{ border: 1px solid {T.C_BORDER}; border-radius:2px; "
            f"background:{T.C_WIDGET}; }}"
            f"QProgressBar::chunk {{ background:{T.C_ACCENT}; border-radius:2px; }}"
        )
        live_lay.addWidget(self._live_bar, 1)
        self._live_label = QLabel("0 → 0.000")
        self._live_label.setStyleSheet(f"color:{T.C_TEXT_DIM}; font-size:10px;")
        live_lay.addWidget(self._live_label)
        self._fired_dot = _ActivityDot()
        live_lay.addWidget(self._fired_dot)
        root.addWidget(live_grp)

        root.addStretch()

        # Connect all change signals
        self._lbl_edit.textChanged.connect(self._on_edit)
        self._enabled_chk.toggled.connect(self._on_edit)
        self._uni_spin.valueChanged.connect(self._on_edit)
        self._ch_spin.valueChanged.connect(self._on_edit)
        self._host_edit.textChanged.connect(self._on_edit)
        self._port_spin.valueChanged.connect(self._on_edit)
        self._addr_edit.textChanged.connect(self._on_edit)
        self._type_combo.currentTextChanged.connect(self._on_edit)
        self._in_min_spin.valueChanged.connect(self._on_edit)
        self._in_max_spin.valueChanged.connect(self._on_edit)
        self._out_min_spin.valueChanged.connect(self._on_edit)
        self._out_max_spin.valueChanged.connect(self._on_edit)
        self._on_edit.connect(self._off_edit)
        self._off_edit.textChanged.connect(self._on_edit)
        self._mode_group.idToggled.connect(self._on_edit)
        self._threshold_spin.valueChanged.connect(self._on_edit)

        self.setEnabled(False)

    # ------------------------------------------------------------------

    @Slot()
    def _on_edit(self, *_) -> None:
        if self._updating or self._route is None:
            return
        r = self._route
        r.label = self._lbl_edit.text()
        r.enabled = self._enabled_chk.isChecked()
        r.universe = self._uni_spin.value()
        r.channel = self._ch_spin.value()
        r.host = self._host_edit.text()
        r.port = self._port_spin.value()
        r.address = self._addr_edit.text()
        r.out_type = self._type_combo.currentText()
        r.in_min = self._in_min_spin.value()
        r.in_max = self._in_max_spin.value()
        r.out_min = self._out_min_spin.value()
        r.out_max = self._out_max_spin.value()
        r.out_string_on = self._on_edit_widget().text()
        r.out_string_off = self._off_edit.text()
        btn = self._mode_group.checkedButton()
        if btn is not None:
            r.mode = btn.property("mode_key")
        r.threshold = self._threshold_spin.value()

    def _on_edit_widget(self) -> QLineEdit:
        """Return the 'on' string QLineEdit (avoid name collision with signal)."""
        return self._on_edit

    def load(self, route: OSCRoute) -> None:
        self._route = None
        self._updating = True
        self.setEnabled(True)

        self._lbl_edit.setText(route.label)
        self._enabled_chk.setChecked(route.enabled)
        self._uni_spin.setValue(route.universe)
        self._ch_spin.setValue(route.channel)
        self._host_edit.setText(route.host)
        self._port_spin.setValue(route.port)
        self._addr_edit.setText(route.address)

        idx = self._type_combo.findText(route.out_type)
        if idx >= 0:
            self._type_combo.setCurrentIndex(idx)

        self._in_min_spin.setValue(route.in_min)
        self._in_max_spin.setValue(route.in_max)
        self._out_min_spin.setValue(route.out_min)
        self._out_max_spin.setValue(route.out_max)
        self._on_edit.setText(route.out_string_on)
        self._off_edit.setText(route.out_string_off)

        for btn in self._mode_group.buttons():
            if btn.property("mode_key") == route.mode:
                btn.setChecked(True)
                break
        self._threshold_spin.setValue(route.threshold)

        self._updating = False
        self._route = route

    def update_live(self, dmx_val: int, out_val: object) -> None:
        self._live_bar.setValue(dmx_val)
        self._live_label.setText(f"{dmx_val} → {out_val}")
        self._fired_dot.flash()

    def clear(self) -> None:
        self._route = None
        self.setEnabled(False)


# ---------------------------------------------------------------------------
# Main OSC panel
# ---------------------------------------------------------------------------

class OSCPanel(QWidget):
    """Full OSC mapper UI — routes, editor, input server, quick-send."""

    def __init__(
        self,
        osc_manager: OSCManager,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._mgr = osc_manager
        self._selected_route_id: Optional[str] = None
        self._dots: dict[str, _ActivityDot] = {}

        # Connect manager signal for live activity
        self._mgr.route_fired.connect(self._on_route_fired)

        self._build_ui()

        # Refresh live values periodically
        self._live_timer = QTimer(self)
        self._live_timer.setInterval(100)
        self._live_timer.timeout.connect(self._refresh_live)
        self._live_timer.start()

    # ------------------------------------------------------------------
    # UI construction
    # ------------------------------------------------------------------

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        # ── Header bar ──────────────────────────────────────────────
        header = QWidget()
        header.setFixedHeight(36)
        header.setStyleSheet(
            f"background:{T.C_PANEL}; border-bottom:1px solid {T.C_BORDER};"
        )
        hlay = QHBoxLayout(header)
        hlay.setContentsMargins(12, 0, 12, 0)
        title_lbl = QLabel("OSC MAPPER")
        title_lbl.setStyleSheet(
            f"color:{T.C_TEXT_HDR}; font-weight:700; font-size:12px; letter-spacing:1px;"
        )
        hlay.addWidget(title_lbl)
        hlay.addStretch()

        # Input server toggle in header
        self._srv_dot = _ActivityDot()
        self._srv_dot.setToolTip("OSC input server status")
        hlay.addWidget(self._srv_dot)
        self._srv_btn = QPushButton("Input Server")
        self._srv_btn.setCheckable(True)
        self._srv_btn.setFixedHeight(24)
        self._srv_btn.toggled.connect(self._toggle_input_server)
        hlay.addWidget(self._srv_btn)
        hlay.addWidget(QLabel("port:"))
        self._srv_port_spin = QSpinBox()
        self._srv_port_spin.setRange(1, 65535)
        self._srv_port_spin.setValue(9000)
        self._srv_port_spin.setFixedWidth(70)
        hlay.addWidget(self._srv_port_spin)
        root.addWidget(header)

        # ── Main splitter ────────────────────────────────────────────
        splitter = QSplitter(Qt.Orientation.Horizontal)
        splitter.setHandleWidth(4)
        root.addWidget(splitter, 1)

        # Left column
        left = QWidget()
        left.setMinimumWidth(180)
        left.setMaximumWidth(280)
        left_lay = QVBoxLayout(left)
        left_lay.setContentsMargins(8, 8, 4, 8)
        left_lay.setSpacing(4)

        lbl = QLabel("Routes")
        lbl.setStyleSheet(
            f"color:{T.C_TEXT_LABEL}; font-size:10px; font-weight:700; "
            f"letter-spacing:1px; text-transform:uppercase;"
        )
        left_lay.addWidget(lbl)

        self._route_list = QListWidget()
        self._route_list.currentItemChanged.connect(self._on_route_selected)
        left_lay.addWidget(self._route_list, 1)

        btn_row = QHBoxLayout()
        self._add_btn = QPushButton("＋ Add")
        self._add_btn.setFixedHeight(26)
        self._add_btn.clicked.connect(self._add_route)
        self._remove_btn = QPushButton("✕ Remove")
        self._remove_btn.setFixedHeight(26)
        self._remove_btn.clicked.connect(self._remove_route)
        btn_row.addWidget(self._add_btn)
        btn_row.addWidget(self._remove_btn)
        left_lay.addLayout(btn_row)

        self._preset_btn = QPushButton("Preset ▼")
        self._preset_btn.setFixedHeight(26)
        self._preset_btn.clicked.connect(self._show_preset_menu)
        left_lay.addWidget(self._preset_btn)

        # Quick Send section
        qs_grp = QGroupBox("Quick Send")
        qs_lay = QFormLayout(qs_grp)
        self._qs_host = QLineEdit("127.0.0.1")
        self._qs_port = QSpinBox()
        self._qs_port.setRange(1, 65535)
        self._qs_port.setValue(8000)
        self._qs_addr = QLineEdit("/test/msg")
        self._qs_value = QLineEdit("0.5")
        qs_lay.addRow("Host:", self._qs_host)
        qs_lay.addRow("Port:", self._qs_port)
        qs_lay.addRow("Address:", self._qs_addr)
        qs_lay.addRow("Value:", self._qs_value)
        self._qs_btn = QPushButton("Send")
        self._qs_btn.setFixedHeight(26)
        self._qs_btn.clicked.connect(self._quick_send)
        qs_lay.addRow("", self._qs_btn)
        self._qs_status = QLabel("")
        self._qs_status.setStyleSheet(f"color:{T.C_TEXT_DIM}; font-size:10px;")
        qs_lay.addRow("", self._qs_status)
        left_lay.addWidget(qs_grp)

        # Input mappings section
        inp_grp = QGroupBox("Input Mappings")
        inp_lay = QFormLayout(inp_grp)
        self._inp_addr_edit = QLineEdit("/lumos/input")
        self._inp_uni_spin = QSpinBox()
        self._inp_uni_spin.setRange(1, 64)
        self._inp_ch_spin = QSpinBox()
        self._inp_ch_spin.setRange(1, 512)
        inp_lay.addRow("OSC Address:", self._inp_addr_edit)
        inp_lay.addRow("Universe:", self._inp_uni_spin)
        inp_lay.addRow("Channel:", self._inp_ch_spin)
        inp_add_btn = QPushButton("Add Mapping")
        inp_add_btn.setFixedHeight(26)
        inp_add_btn.clicked.connect(self._add_input_mapping)
        inp_lay.addRow("", inp_add_btn)
        left_lay.addWidget(inp_grp)

        splitter.addWidget(left)

        # Right column — editor
        right_scroll = QScrollArea()
        right_scroll.setWidgetResizable(True)
        right_scroll.setFrameShape(right_scroll.Shape.NoFrame)
        self._editor = _RouteEditor()
        right_scroll.setWidget(self._editor)
        splitter.addWidget(right_scroll)

        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)

    # ------------------------------------------------------------------
    # Slots & helpers
    # ------------------------------------------------------------------

    @Slot(QListWidgetItem, QListWidgetItem)
    def _on_route_selected(
        self,
        current: Optional[QListWidgetItem],
        previous: Optional[QListWidgetItem],
    ) -> None:
        if current is None:
            self._editor.clear()
            self._selected_route_id = None
            return
        route_id = current.data(Qt.ItemDataRole.UserRole)
        route = self._mgr.get_route(route_id)
        if route is None:
            self._editor.clear()
            self._selected_route_id = None
            return
        self._selected_route_id = route_id
        self._editor.load(route)

    def _add_route(self) -> None:
        route = OSCRoute(label="New Route")
        self._mgr.add_route(route)
        self._rebuild_list()
        # Select the new item
        for i in range(self._route_list.count()):
            item = self._route_list.item(i)
            if item and item.data(Qt.ItemDataRole.UserRole) == route.id:
                self._route_list.setCurrentItem(item)
                break

    def _remove_route(self) -> None:
        item = self._route_list.currentItem()
        if item is None:
            return
        route_id = item.data(Qt.ItemDataRole.UserRole)
        self._mgr.remove_route(route_id)
        self._editor.clear()
        self._selected_route_id = None
        self._rebuild_list()

    def _show_preset_menu(self) -> None:
        """Show the preset dialog and insert the resulting route."""
        # Determine sensible defaults from first universe/channel or current route
        host, port, universe, channel = "127.0.0.1", 8000, 1, 1
        if self._selected_route_id:
            r = self._mgr.get_route(self._selected_route_id)
            if r:
                host, port, universe, channel = r.host, r.port, r.universe, r.channel

        dlg = _PresetDialog(host, port, universe, channel, self)
        if dlg.exec() == QDialog.DialogCode.Accepted and dlg.result_route is not None:
            self._mgr.add_route(dlg.result_route)
            self._rebuild_list()

    def _rebuild_list(self) -> None:
        self._route_list.clear()
        for route in self._mgr.routes:
            item = _RouteItem(route)
            item.setData(Qt.ItemDataRole.UserRole, route.id)
            self._route_list.addItem(item)
            # Restore selection
            if route.id == self._selected_route_id:
                self._route_list.setCurrentItem(item)

    @Slot()
    def _quick_send(self) -> None:
        host = self._qs_host.text().strip()
        port = self._qs_port.value()
        address = self._qs_addr.text().strip()
        raw = self._qs_value.text().strip()
        # Parse value
        value: object
        try:
            value = int(raw)
        except ValueError:
            try:
                value = float(raw)
            except ValueError:
                value = raw

        ok = self._mgr.quick_send(host, port, address, value)
        if ok:
            self._qs_status.setStyleSheet(f"color:{T.C_GREEN}; font-size:10px;")
            self._qs_status.setText("Sent OK")
        else:
            self._qs_status.setStyleSheet(f"color:{T.C_RED}; font-size:10px;")
            self._qs_status.setText("Failed (python-osc not installed?)")

    @Slot(bool)
    def _toggle_input_server(self, enabled: bool) -> None:
        if enabled:
            port = self._srv_port_spin.value()
            ok = self._mgr.start_osc_server(port)
            if ok:
                self._srv_dot._set_color(T.C_GREEN)
                self._srv_btn.setText(f"Input Server ● {port}")
            else:
                self._srv_btn.setChecked(False)
                self._srv_dot._set_color(T.C_RED)
                self._srv_btn.setText("Input Server (failed)")
        else:
            self._mgr.stop_osc_server()
            self._srv_dot._set_color(T.C_BORDER_LT)
            self._srv_btn.setText("Input Server")

    @Slot()
    def _add_input_mapping(self) -> None:
        addr = self._inp_addr_edit.text().strip()
        uni = self._inp_uni_spin.value()
        ch = self._inp_ch_spin.value()
        if addr:
            self._mgr.add_input_mapping(addr, uni, ch)

    @Slot(str, object)
    def _on_route_fired(self, route_id: str, out_val: object) -> None:
        """Flash the list item for the fired route and update the editor if selected."""
        # Refresh the corresponding list item text
        for i in range(self._route_list.count()):
            item = self._route_list.item(i)
            if item and item.data(Qt.ItemDataRole.UserRole) == route_id:
                route = self._mgr.get_route(route_id)
                if route:
                    cast_item = item  # already a _RouteItem but QListWidget casts it
                    # Update editor live if this is the selected route
                    if route_id == self._selected_route_id:
                        dmx = self._mgr.engine.get_channel(route.universe, route.channel)
                        self._editor.update_live(dmx, out_val)
                break

    @Slot()
    def _refresh_live(self) -> None:
        """Periodically update the live bar in the editor."""
        if self._selected_route_id is None:
            return
        route = self._mgr.get_route(self._selected_route_id)
        if route is None:
            return
        dmx = self._mgr.engine.get_channel(route.universe, route.channel)
        out_val = route.transform(dmx)
        self._editor._live_bar.setValue(dmx)
        self._editor._live_label.setText(f"{dmx} → {out_val}")

        # Also keep route list labels up-to-date after edits
        for i in range(self._route_list.count()):
            item = self._route_list.item(i)
            if item and item.data(Qt.ItemDataRole.UserRole) == route.id:
                r = self._mgr.get_route(route.id)
                if r:
                    prefix = "" if r.enabled else "○ "
                    item.setText(f"{prefix}{r.label or r.address}")
                break
