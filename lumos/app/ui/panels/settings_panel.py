"""
SettingsPanel — unified configuration hub (Universes, MIDI, OSC, Timecode,
Network, General).

Inspired by QLC+ I/O Manager, MagicQ DMX I/O, and grandMA3 Network menu:
  • Per-universe output protocol assignment (table view)
  • MIDI ports + route editor with learn mode
  • OSC input server + route editor
  • Timecode source (LTC/MTC/Internal)
  • Network (Art-Net / sACN) global settings
  • General engine options
"""
from __future__ import annotations

import threading
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QHBoxLayout, QVBoxLayout, QSplitter,
    QListWidget, QListWidgetItem, QStackedWidget,
    QLabel, QPushButton, QGroupBox, QTableWidget, QTableWidgetItem,
    QHeaderView, QCheckBox, QSpinBox, QComboBox, QLineEdit,
    QDialog, QDialogButtonBox, QFormLayout, QMessageBox,
    QFrame, QScrollArea, QSizePolicy, QAbstractItemView,
    QRadioButton, QButtonGroup, QDoubleSpinBox, QTextEdit,
)
from PySide6.QtCore import Qt, Signal, Slot, QTimer
from PySide6.QtGui import QFont, QColor

from app.engine import LumosEngine
from app.outputs.manager import OutputManager
from app.outputs.base import OutputBackend
from app.midi import MIDIManager, MIDIRoute
from app.osc import OSCManager, OSCRoute
from app.ui import theme as T

try:
    from app.outputs.artnet import ArtNetBackend
    _HAS_ARTNET = True
except ImportError:
    _HAS_ARTNET = False

try:
    from app.outputs.sacn import SACNBackend
    _HAS_SACN = True
except ImportError:
    _HAS_SACN = False

try:
    from app.outputs.enttec import EnttecOpenBackend, EnttecProBackend
    _HAS_ENTTEC = True
except ImportError:
    _HAS_ENTTEC = False

try:
    from app.outputs.udmx import UDMXBackend
    _HAS_UDMX = True
except ImportError:
    _HAS_UDMX = False

try:
    from app.outputs.chamsys import ChamsysBackend
    _HAS_CHAMSYS = True
except ImportError:
    _HAS_CHAMSYS = False

try:
    from app.outputs.ftdi_raw import FTDIRawBackend
    _HAS_FTDI = True
except ImportError:
    _HAS_FTDI = False

try:
    from app.outputs.osc import OSCBackend
    _HAS_OSC_OUT = True
except ImportError:
    _HAS_OSC_OUT = False


# ── Helpers ─────────────────────────────────────────────────────────────────

def _hline() -> QFrame:
    f = QFrame()
    f.setFrameShape(QFrame.Shape.HLine)
    f.setStyleSheet(f"color: {T.C_BORDER};")
    return f


def _section_label(text: str) -> QLabel:
    lbl = QLabel(text)
    lbl.setStyleSheet(
        f"color: {T.C_ACCENT}; font-weight: 700; font-size: 11px;"
        f" letter-spacing: 1px; padding: 6px 0 2px 0;"
    )
    return lbl


def _action_btn(label: str, color: str = T.C_ACCENT) -> QPushButton:
    btn = QPushButton(label)
    btn.setFixedHeight(28)
    btn.setStyleSheet(
        f"QPushButton {{ background: {color}; color: white; border: none;"
        f" border-radius: 3px; padding: 0 12px; font-size: 11px; }}"
        f"QPushButton:hover {{ background: {T.C_POSITION}; }}"
        f"QPushButton:disabled {{ background: {T.C_BORDER}; color: {T.C_TEXT_DIM}; }}"
    )
    return btn


def _table_style() -> str:
    return (
        f"QTableWidget {{ background: {T.C_PANEL}; gridline-color: {T.C_BORDER};"
        f" color: {T.C_TEXT}; selection-background-color: {T.C_ACCENT};"
        f" font-size: 12px; border: 1px solid {T.C_BORDER}; }}"
        f"QTableWidget::item {{ padding: 4px 8px; }}"
        f"QHeaderView::section {{ background: {T.C_PANEL2}; color: {T.C_TEXT_DIM};"
        f" font-size: 11px; font-weight: 600; padding: 4px 8px;"
        f" border: none; border-bottom: 1px solid {T.C_BORDER}; }}"
    )


# ── Universe backend config dialog ──────────────────────────────────────────

_BACKEND_NAMES = [
    ("None (disabled)", None),
    ("Art-Net (UDP)", "artnet"),
    ("sACN / E1.31 (UDP)", "sacn"),
    ("Enttec Open DMX (USB)", "enttec_open"),
    ("Enttec DMX Pro (USB)", "enttec_pro"),
    ("uDMX (USB HID)", "udmx"),
    ("Chamsys MagicCable (USB)", "chamsys"),
    ("DMXking ultraDMX (USB)", "dmxking"),
    ("FTDI Raw (USB)", "ftdi_raw"),
    ("OSC (UDP out)", "osc_out"),
]


class _BackendDialog(QDialog):
    """Configure an output backend for one universe."""

    def __init__(self, universe_id: int, current: Optional[OutputBackend],
                 parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.setWindowTitle(f"Configure Universe {universe_id}")
        self.setMinimumWidth(420)
        self._universe_id = universe_id
        self._result_backend: Optional[OutputBackend] = None

        lay = QVBoxLayout(self)
        lay.setSpacing(8)

        # Protocol selector
        form = QFormLayout()
        self._proto = QComboBox()
        for name, _ in _BACKEND_NAMES:
            self._proto.addItem(name)
        self._proto.setCurrentIndex(0)
        form.addRow("Protocol:", self._proto)
        lay.addLayout(form)

        lay.addWidget(_hline())

        # Per-protocol stacked config
        self._stack = QStackedWidget()
        self._stack.addWidget(self.__make_none_page())
        self._stack.addWidget(self._make_artnet_page())
        self._stack.addWidget(self._make_sacn_page())
        self._stack.addWidget(self._make_enttec_open_page())
        self._stack.addWidget(self._make_enttec_pro_page())
        self._stack.addWidget(self._make_udmx_page())
        self._stack.addWidget(self._make_chamsys_page())
        self._stack.addWidget(self._make_dmxking_page())
        self._stack.addWidget(self._make_ftdi_page())
        self._stack.addWidget(self._make_osc_out_page())
        lay.addWidget(self._stack)

        self._proto.currentIndexChanged.connect(self._stack.setCurrentIndex)

        lay.addStretch()
        btns = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        btns.accepted.connect(self._on_ok)
        btns.rejected.connect(self.reject)
        lay.addWidget(btns)

        if current is not None:
            self._load_existing(current)

    # Page builders -----------------------------------------------------------

    def __make_none_page(self) -> QWidget:
        w = QWidget()
        lbl = QLabel("No output — universe is disabled.")
        lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; padding: 8px;")
        QVBoxLayout(w).addWidget(lbl)
        return w

    def _make_artnet_page(self) -> QWidget:
        w = QWidget()
        f = QFormLayout(w)
        self._an_ip = QLineEdit("255.255.255.255")
        self._an_port = QSpinBox()
        self._an_port.setRange(1, 65535)
        self._an_port.setValue(6454)
        self._an_universe = QSpinBox()
        self._an_universe.setRange(0, 32767)
        self._an_universe.setValue(0)
        f.addRow("IP / Broadcast:", self._an_ip)
        f.addRow("Port:", self._an_port)
        f.addRow("Art-Net Universe:", self._an_universe)
        return w

    def _make_sacn_page(self) -> QWidget:
        w = QWidget()
        f = QFormLayout(w)
        self._sacn_ip = QLineEdit("")
        self._sacn_ip.setPlaceholderText("Leave blank for multicast")
        self._sacn_priority = QSpinBox()
        self._sacn_priority.setRange(0, 200)
        self._sacn_priority.setValue(100)
        self._sacn_universe = QSpinBox()
        self._sacn_universe.setRange(1, 63999)
        self._sacn_universe.setValue(1)
        f.addRow("Unicast IP (opt):", self._sacn_ip)
        f.addRow("sACN Universe:", self._sacn_universe)
        f.addRow("Priority:", self._sacn_priority)
        return w

    def _make_enttec_open_page(self) -> QWidget:
        w = QWidget()
        f = QFormLayout(w)
        self._eo_port = QLineEdit("")
        self._eo_port.setPlaceholderText("/dev/ttyUSB0  or  COM3")
        f.addRow("Serial port:", self._eo_port)
        return w

    def _make_enttec_pro_page(self) -> QWidget:
        w = QWidget()
        f = QFormLayout(w)
        self._ep_port = QLineEdit("")
        self._ep_port.setPlaceholderText("/dev/ttyUSB0  or  COM3")
        f.addRow("Serial port:", self._ep_port)
        return w

    def _make_udmx_page(self) -> QWidget:
        w = QWidget()
        lbl = QLabel("uDMX device is auto-detected via USB HID.\nNo configuration required.")
        lbl.setWordWrap(True)
        lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; padding: 8px;")
        QVBoxLayout(w).addWidget(lbl)
        return w

    def _make_chamsys_page(self) -> QWidget:
        w = QWidget()
        f = QFormLayout(w)
        self._cs_port = QLineEdit("")
        self._cs_port.setPlaceholderText("/dev/ttyUSB0  or  COM3")
        f.addRow("Serial port:", self._cs_port)
        return w

    def _make_dmxking_page(self) -> QWidget:
        w = QWidget()
        f = QFormLayout(w)
        self._dk_port = QLineEdit("")
        self._dk_port.setPlaceholderText("/dev/ttyUSB0  or  COM3")
        f.addRow("Serial port:", self._dk_port)
        return w

    def _make_ftdi_page(self) -> QWidget:
        w = QWidget()
        f = QFormLayout(w)
        self._ftdi_sn = QLineEdit("")
        self._ftdi_sn.setPlaceholderText("Leave blank for first device")
        f.addRow("FTDI Serial #:", self._ftdi_sn)
        return w

    def _make_osc_out_page(self) -> QWidget:
        w = QWidget()
        f = QFormLayout(w)
        self._osc_host = QLineEdit("127.0.0.1")
        self._osc_port_out = QSpinBox()
        self._osc_port_out.setRange(1, 65535)
        self._osc_port_out.setValue(8000)
        self._osc_pattern = QLineEdit("/dmx/{universe}/{channel}")
        self._osc_mode = QComboBox()
        self._osc_mode.addItems(["per_channel", "blob", "both"])
        f.addRow("Host:", self._osc_host)
        f.addRow("Port:", self._osc_port_out)
        f.addRow("Address pattern:", self._osc_pattern)
        f.addRow("Send mode:", self._osc_mode)
        return w

    # Load existing -----------------------------------------------------------

    def _load_existing(self, backend: OutputBackend) -> None:
        name = type(backend).__name__
        if name == "ArtNetBackend":
            self._proto.setCurrentIndex(1)
            cfg = backend.config_dict() if hasattr(backend, "config_dict") else {}
            self._an_ip.setText(cfg.get("ip", "255.255.255.255"))
            self._an_port.setValue(cfg.get("port", 6454))
            self._an_universe.setValue(cfg.get("artnet_universe", 0))
        elif name == "SACNBackend":
            self._proto.setCurrentIndex(2)
            cfg = backend.config_dict() if hasattr(backend, "config_dict") else {}
            self._sacn_ip.setText(cfg.get("ip", ""))
            self._sacn_universe.setValue(cfg.get("sacn_universe", 1))
            self._sacn_priority.setValue(cfg.get("priority", 100))
        elif name == "EnttecOpenBackend":
            self._proto.setCurrentIndex(3)
        elif name == "EnttecProBackend":
            self._proto.setCurrentIndex(4)
        elif name == "UDMXBackend":
            self._proto.setCurrentIndex(5)
        elif name == "ChamsysBackend":
            self._proto.setCurrentIndex(6)
        elif name in ("DMXkingBackend", "FTDIRawBackend"):
            self._proto.setCurrentIndex(7 if name == "DMXkingBackend" else 8)
        elif name == "OSCBackend":
            self._proto.setCurrentIndex(9)
            cfg = backend.config_dict() if hasattr(backend, "config_dict") else {}
            self._osc_host.setText(cfg.get("host", "127.0.0.1"))
            self._osc_port_out.setValue(cfg.get("port", 8000))
            self._osc_pattern.setText(cfg.get("address_pattern", "/dmx/{universe}/{channel}"))
            mode_idx = self._osc_mode.findText(cfg.get("mode", "per_channel"))
            if mode_idx >= 0:
                self._osc_mode.setCurrentIndex(mode_idx)

    # Build result backend ----------------------------------------------------

    def _on_ok(self) -> None:
        idx = self._proto.currentIndex()
        try:
            self._result_backend = self._build_backend(idx)
        except Exception as e:
            QMessageBox.warning(self, "Configuration error", str(e))
            return
        self.accept()

    def _build_backend(self, idx: int) -> Optional[OutputBackend]:
        if idx == 0:
            return None
        elif idx == 1:
            if not _HAS_ARTNET:
                raise RuntimeError("ArtNetBackend not available — check dependencies")
            return ArtNetBackend(
                ip=self._an_ip.text().strip(),
                port=self._an_port.value(),
                artnet_universe=self._an_universe.value(),
            )
        elif idx == 2:
            if not _HAS_SACN:
                raise RuntimeError("SACNBackend not available — check dependencies")
            return SACNBackend(
                ip=self._sacn_ip.text().strip() or None,
                sacn_universe=self._sacn_universe.value(),
                priority=self._sacn_priority.value(),
            )
        elif idx == 3:
            if not _HAS_ENTTEC:
                raise RuntimeError("EnttecOpenBackend not available")
            return EnttecOpenBackend(port=self._eo_port.text().strip())
        elif idx == 4:
            if not _HAS_ENTTEC:
                raise RuntimeError("EnttecProBackend not available")
            return EnttecProBackend(port=self._ep_port.text().strip())
        elif idx == 5:
            if not _HAS_UDMX:
                raise RuntimeError("UDMXBackend not available")
            return UDMXBackend()
        elif idx == 6:
            if not _HAS_CHAMSYS:
                raise RuntimeError("ChamsysBackend not available")
            return ChamsysBackend(port=self._cs_port.text().strip())
        elif idx == 7:
            return None  # DMXking placeholder
        elif idx == 8:
            if not _HAS_FTDI:
                raise RuntimeError("FTDIRawBackend not available")
            return FTDIRawBackend(serial_number=self._ftdi_sn.text().strip() or None)
        elif idx == 9:
            if not _HAS_OSC_OUT:
                raise RuntimeError("OSCBackend not available")
            return OSCBackend(
                host=self._osc_host.text().strip(),
                port=self._osc_port_out.value(),
                address_pattern=self._osc_pattern.text().strip(),
                mode=self._osc_mode.currentText(),
            )
        return None

    def result_backend(self) -> Optional[OutputBackend]:
        return self._result_backend


# ── MIDI route editor dialog ─────────────────────────────────────────────────

class _MIDIRouteDialog(QDialog):
    """Add or edit a MIDIRoute."""

    def __init__(self, route: Optional[MIDIRoute] = None,
                 available_ports: Optional[list[str]] = None,
                 parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("MIDI Route" if route is None else "Edit MIDI Route")
        self.setMinimumWidth(400)
        self._route = MIDIRoute() if route is None else route

        lay = QVBoxLayout(self)
        form = QFormLayout()

        self._label = QLineEdit(self._route.label)
        form.addRow("Label:", self._label)

        self._enabled = QCheckBox()
        self._enabled.setChecked(self._route.enabled)
        form.addRow("Enabled:", self._enabled)

        self._port = QComboBox()
        self._port.addItem("Any port", "")
        for p in (available_ports or []):
            self._port.addItem(p, p)
        cur = self._route.port_name
        idx = self._port.findData(cur)
        self._port.setCurrentIndex(idx if idx >= 0 else 0)
        form.addRow("MIDI port:", self._port)

        self._msg_type = QComboBox()
        self._msg_type.addItems(["control_change", "note_on", "note_off", "program_change"])
        self._msg_type.setCurrentText(self._route.msg_type)
        form.addRow("Message type:", self._msg_type)

        self._channel = QSpinBox()
        self._channel.setRange(0, 16)
        self._channel.setValue(self._route.midi_channel)
        self._channel.setSpecialValueText("Any")
        form.addRow("Channel (0=Any):", self._channel)

        self._note_cc = QSpinBox()
        self._note_cc.setRange(0, 127)
        self._note_cc.setValue(self._route.note_or_cc)
        form.addRow("Note/CC number:", self._note_cc)

        lay.addLayout(form)
        lay.addWidget(_section_label("ACTION"))

        form2 = QFormLayout()
        self._action = QComboBox()
        self._action.addItems([
            "dmx_channel", "grand_master", "blackout_toggle",
            "scene_activate", "cue_go", "cue_back",
        ])
        self._action.setCurrentText(self._route.action)
        form2.addRow("Action:", self._action)

        self._universe = QSpinBox()
        self._universe.setRange(1, 128)
        self._universe.setValue(self._route.universe)
        form2.addRow("Universe:", self._universe)

        self._dmx_ch = QSpinBox()
        self._dmx_ch.setRange(1, 512)
        self._dmx_ch.setValue(self._route.dmx_channel)
        form2.addRow("DMX channel:", self._dmx_ch)

        self._scene = QLineEdit(self._route.scene_name)
        form2.addRow("Scene name:", self._scene)

        self._cuelist = QSpinBox()
        self._cuelist.setRange(1, 9999)
        self._cuelist.setValue(self._route.cuelist_id)
        form2.addRow("Cuelist ID:", self._cuelist)

        lay.addLayout(form2)
        lay.addWidget(_section_label("VALUE MAPPING"))

        form3 = QFormLayout()
        self._in_min = QSpinBox()
        self._in_min.setRange(0, 127)
        self._in_min.setValue(self._route.in_min)
        self._in_max = QSpinBox()
        self._in_max.setRange(0, 127)
        self._in_max.setValue(self._route.in_max)
        self._out_min = QSpinBox()
        self._out_min.setRange(0, 255)
        self._out_min.setValue(self._route.out_min)
        self._out_max = QSpinBox()
        self._out_max.setRange(0, 255)
        self._out_max.setValue(self._route.out_max)
        form3.addRow("In min:", self._in_min)
        form3.addRow("In max:", self._in_max)
        form3.addRow("Out min:", self._out_min)
        form3.addRow("Out max:", self._out_max)

        self._note_trig = QComboBox()
        self._note_trig.addItems(["on_press", "on_release", "both"])
        self._note_trig.setCurrentText(self._route.note_trigger)
        form3.addRow("Note trigger:", self._note_trig)

        lay.addLayout(form3)

        btns = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        btns.accepted.connect(self._on_ok)
        btns.rejected.connect(self.reject)
        lay.addWidget(btns)

    def _on_ok(self) -> None:
        r = self._route
        r.label = self._label.text().strip()
        r.enabled = self._enabled.isChecked()
        r.port_name = self._port.currentData() or ""
        r.msg_type = self._msg_type.currentText()
        r.midi_channel = self._channel.value()
        r.note_or_cc = self._note_cc.value()
        r.action = self._action.currentText()
        r.universe = self._universe.value()
        r.dmx_channel = self._dmx_ch.value()
        r.scene_name = self._scene.text().strip()
        r.cuelist_id = self._cuelist.value()
        r.in_min = self._in_min.value()
        r.in_max = self._in_max.value()
        r.out_min = self._out_min.value()
        r.out_max = self._out_max.value()
        r.note_trigger = self._note_trig.currentText()
        self.accept()

    def result_route(self) -> MIDIRoute:
        return self._route


# ── Learn dialog ─────────────────────────────────────────────────────────────

class _LearnDialog(QDialog):
    """Show 'waiting for MIDI' and close on input or timeout."""

    learned = Signal(str, str, int, int)   # port_name, msg_type, channel, number

    def __init__(self, midi_manager: MIDIManager,
                 parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("MIDI Learn")
        self.setFixedSize(320, 160)
        self._mgr = midi_manager
        self._done = False

        lay = QVBoxLayout(self)
        lbl = QLabel("Play a note or move a knob on your\nMIDI controller...")
        lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        lbl.setStyleSheet(f"color: {T.C_TEXT}; font-size: 13px; padding: 16px;")
        lay.addWidget(lbl)

        self._status = QLabel("Waiting…")
        self._status.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._status.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 11px;")
        lay.addWidget(self._status)

        btn_cancel = QPushButton("Cancel")
        btn_cancel.clicked.connect(self._cancel)
        lay.addWidget(btn_cancel)

        midi_manager.start_learn(self._on_learn)

        self._timeout = QTimer(self)
        self._timeout.setSingleShot(True)
        self._timeout.setInterval(15000)
        self._timeout.timeout.connect(self._cancel)
        self._timeout.start()

    def _on_learn(self, port_name: str, msg_type: str, channel: int, number: int) -> None:
        if self._done:
            return
        self._done = True
        self._timeout.stop()
        self._status.setText(f"{msg_type}  ch{channel}  #{number}  @ {port_name or 'any'}")
        self.learned.emit(port_name, msg_type, channel, number)
        QTimer.singleShot(600, self.accept)

    def _cancel(self) -> None:
        self._mgr.stop_learn()
        self.reject()


# ── OSC Route editor dialog ──────────────────────────────────────────────────

class _OSCRouteDialog(QDialog):
    """Add or edit one OSCRoute (DMX source → OSC output)."""

    def __init__(self, route: Optional[OSCRoute] = None,
                 parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("OSC Route")
        self.setMinimumWidth(400)
        self._route = OSCRoute() if route is None else route

        lay = QVBoxLayout(self)
        form = QFormLayout()

        self._label = QLineEdit(self._route.label)
        form.addRow("Label:", self._label)
        self._enabled = QCheckBox()
        self._enabled.setChecked(self._route.enabled)
        form.addRow("Enabled:", self._enabled)

        lay.addLayout(form)
        lay.addWidget(_section_label("DMX SOURCE"))
        form2 = QFormLayout()
        self._universe = QSpinBox()
        self._universe.setRange(1, 128)
        self._universe.setValue(self._route.universe)
        form2.addRow("Universe:", self._universe)
        self._channel = QSpinBox()
        self._channel.setRange(1, 512)
        self._channel.setValue(self._route.channel)
        form2.addRow("Channel:", self._channel)
        self._in_min = QSpinBox()
        self._in_min.setRange(0, 255)
        self._in_min.setValue(self._route.in_min)
        self._in_max = QSpinBox()
        self._in_max.setRange(0, 255)
        self._in_max.setValue(self._route.in_max)
        form2.addRow("In min (DMX):", self._in_min)
        form2.addRow("In max (DMX):", self._in_max)
        lay.addLayout(form2)

        lay.addWidget(_section_label("OSC DESTINATION"))
        form3 = QFormLayout()
        self._host = QLineEdit(self._route.host)
        form3.addRow("Host:", self._host)
        self._port = QSpinBox()
        self._port.setRange(1, 65535)
        self._port.setValue(self._route.port)
        form3.addRow("Port:", self._port)
        self._addr = QLineEdit(self._route.address)
        form3.addRow("OSC address:", self._addr)
        self._out_type = QComboBox()
        self._out_type.addItems(["float", "int", "bool", "string"])
        self._out_type.setCurrentText(self._route.out_type)
        form3.addRow("Value type:", self._out_type)
        self._out_min = QDoubleSpinBox()
        self._out_min.setRange(-9999, 9999)
        self._out_min.setDecimals(3)
        self._out_min.setValue(self._route.out_min)
        self._out_max = QDoubleSpinBox()
        self._out_max.setRange(-9999, 9999)
        self._out_max.setDecimals(3)
        self._out_max.setValue(self._route.out_max)
        form3.addRow("Out min:", self._out_min)
        form3.addRow("Out max:", self._out_max)
        self._mode = QComboBox()
        self._mode.addItems(["continuous", "on_change", "threshold_cross"])
        self._mode.setCurrentText(self._route.mode)
        form3.addRow("Trigger mode:", self._mode)
        self._threshold = QSpinBox()
        self._threshold.setRange(0, 255)
        self._threshold.setValue(self._route.threshold)
        form3.addRow("Threshold:", self._threshold)
        lay.addLayout(form3)

        btns = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        btns.accepted.connect(self._on_ok)
        btns.rejected.connect(self.reject)
        lay.addWidget(btns)

    def _on_ok(self) -> None:
        r = self._route
        r.label = self._label.text().strip()
        r.enabled = self._enabled.isChecked()
        r.universe = self._universe.value()
        r.channel = self._channel.value()
        r.in_min = self._in_min.value()
        r.in_max = self._in_max.value()
        r.host = self._host.text().strip()
        r.port = self._port.value()
        r.address = self._addr.text().strip()
        r.out_type = self._out_type.currentText()
        r.out_min = self._out_min.value()
        r.out_max = self._out_max.value()
        r.mode = self._mode.currentText()
        r.threshold = self._threshold.value()
        self.accept()

    def result_route(self) -> OSCRoute:
        return self._route


# ── Category pages ───────────────────────────────────────────────────────────

class _UniversesPage(QWidget):
    """Per-universe output protocol assignment (MagicQ DMX I/O style)."""

    def __init__(self, engine: LumosEngine, output_manager: OutputManager,
                 parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self._engine = engine
        self._om = output_manager
        self._build_ui()
        self._refresh()

    def _build_ui(self) -> None:
        lay = QVBoxLayout(self)
        lay.setContentsMargins(16, 12, 16, 12)
        lay.setSpacing(8)

        lay.addWidget(_section_label("UNIVERSES  ·  OUTPUT PROTOCOL ASSIGNMENT"))
        lbl = QLabel(
            "Each logical universe can be routed to an independent output protocol. "
            "Double-click a row to configure its output."
        )
        lbl.setWordWrap(True)
        lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 11px;")
        lay.addWidget(lbl)

        self._table = QTableWidget(0, 5)
        self._table.setHorizontalHeaderLabels(["#", "Name", "Protocol", "Config summary", "Status"])
        self._table.horizontalHeader().setSectionResizeMode(3, QHeaderView.ResizeMode.Stretch)
        self._table.horizontalHeader().setStretchLastSection(False)
        self._table.setColumnWidth(0, 50)
        self._table.setColumnWidth(1, 130)
        self._table.setColumnWidth(2, 140)
        self._table.setColumnWidth(4, 90)
        self._table.verticalHeader().setVisible(False)
        self._table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        self._table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self._table.setAlternatingRowColors(True)
        self._table.setStyleSheet(_table_style())
        self._table.doubleClicked.connect(self._configure_selected)
        lay.addWidget(self._table, 1)

        btn_row = QHBoxLayout()
        btn_row.setSpacing(6)
        self._btn_add = _action_btn("＋  Add Universe")
        self._btn_add.clicked.connect(self._add_universe)
        self._btn_cfg = _action_btn("⚙  Configure Output")
        self._btn_cfg.clicked.connect(self._configure_selected)
        self._btn_open = _action_btn("▶  Open", T.C_GREEN if hasattr(T, "C_GREEN") else "#2e7d32")
        self._btn_open.clicked.connect(self._open_selected)
        self._btn_close = _action_btn("■  Close", T.C_AMBER)
        self._btn_close.clicked.connect(self._close_selected)
        self._btn_rem = _action_btn("✕  Remove", T.C_RED)
        self._btn_rem.clicked.connect(self._remove_selected)
        for b in [self._btn_add, self._btn_cfg, self._btn_open, self._btn_close, self._btn_rem]:
            btn_row.addWidget(b)
        btn_row.addStretch()
        lay.addLayout(btn_row)

    def _refresh(self) -> None:
        uids = sorted(self._engine.universes)
        self._table.setRowCount(len(uids))
        for row, uid in enumerate(uids):
            backend = self._om.get_backend(uid)
            proto = type(backend).__name__.replace("Backend", "") if backend else "None"
            cfg = backend.config_dict() if (backend and hasattr(backend, "config_dict")) else {}
            summary = _cfg_summary(cfg)
            status = "Open" if (backend and backend.is_open) else "Closed"
            status_color = "#4caf50" if status == "Open" else T.C_TEXT_DIM

            self._table.setItem(row, 0, QTableWidgetItem(str(uid)))
            self._table.setItem(row, 1, QTableWidgetItem(f"Universe {uid}"))
            self._table.setItem(row, 2, QTableWidgetItem(proto))
            self._table.setItem(row, 3, QTableWidgetItem(summary))
            status_item = QTableWidgetItem(status)
            status_item.setForeground(QColor(status_color))
            self._table.setItem(row, 4, status_item)

        for col in [0, 1, 2, 4]:
            self._table.resizeColumnToContents(col)

    def _selected_uid(self) -> Optional[int]:
        rows = self._table.selectionModel().selectedRows()
        if not rows:
            return None
        return int(self._table.item(rows[0].row(), 0).text())

    def _add_universe(self) -> None:
        existing = set(self._engine.universes)
        uid = 1
        while uid in existing:
            uid += 1
        self._engine.add_universe(uid)
        self._refresh()

    def _configure_selected(self) -> None:
        uid = self._selected_uid()
        if uid is None:
            return
        current = self._om.get_backend(uid)
        dlg = _BackendDialog(uid, current, self)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            backend = dlg.result_backend()
            if backend is None:
                self._om.remove_backend(uid)
            else:
                self._om.set_backend(uid, backend)
            self._refresh()

    def _open_selected(self) -> None:
        uid = self._selected_uid()
        if uid is None:
            return
        backend = self._om.get_backend(uid)
        if backend:
            ok = backend.open()
            if not ok:
                QMessageBox.warning(self, "Open failed",
                                    f"Could not open backend for Universe {uid}.")
        self._refresh()

    def _close_selected(self) -> None:
        uid = self._selected_uid()
        if uid is None:
            return
        backend = self._om.get_backend(uid)
        if backend and backend.is_open:
            backend.close()
        self._refresh()

    def _remove_selected(self) -> None:
        uid = self._selected_uid()
        if uid is None:
            return
        if QMessageBox.question(
            self, "Remove universe",
            f"Remove Universe {uid} and close its output?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
        ) == QMessageBox.StandardButton.Yes:
            self._om.remove_backend(uid)
            self._engine.remove_universe(uid)
            self._refresh()


def _cfg_summary(cfg: dict) -> str:
    if not cfg:
        return ""
    parts = []
    for k in ("ip", "host", "port", "serial_number", "sacn_universe", "artnet_universe"):
        if k in cfg and cfg[k]:
            parts.append(f"{k}={cfg[k]}")
    return "  ".join(parts[:3])


class _MIDIPage(QWidget):
    """MIDI port management + route editor with learn mode."""

    def __init__(self, midi_manager: MIDIManager, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self._mgr = midi_manager
        self._build_ui()
        self._refresh_ports()
        self._refresh_routes()

        self._port_refresh = QTimer(self)
        self._port_refresh.setInterval(2000)
        self._port_refresh.timeout.connect(self._refresh_ports)
        self._port_refresh.start()

    def _build_ui(self) -> None:
        lay = QVBoxLayout(self)
        lay.setContentsMargins(16, 12, 16, 12)
        lay.setSpacing(8)

        lay.addWidget(_section_label("MIDI INPUT PORTS"))
        lbl = QLabel(
            "Check the ports to connect. Routes are applied to all connected ports "
            "unless a specific port is assigned."
        )
        lbl.setWordWrap(True)
        lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 11px;")
        lay.addWidget(lbl)

        ports_row = QHBoxLayout()
        self._port_list = QListWidget()
        self._port_list.setMaximumHeight(100)
        self._port_list.setStyleSheet(
            f"QListWidget {{ background: {T.C_PANEL}; color: {T.C_TEXT};"
            f" border: 1px solid {T.C_BORDER}; font-size: 12px; }}"
        )
        ports_row.addWidget(self._port_list, 1)
        btn_refresh = _action_btn("⟳ Refresh")
        btn_refresh.clicked.connect(self._refresh_ports)
        btn_refresh.setFixedWidth(90)
        ports_row.addWidget(btn_refresh)
        lay.addLayout(ports_row)

        self._port_list.itemChanged.connect(self._on_port_toggled)

        lay.addWidget(_hline())
        lay.addWidget(_section_label("MIDI ROUTES"))

        self._route_table = QTableWidget(0, 6)
        self._route_table.setHorizontalHeaderLabels(
            ["Label", "Type", "Ch", "CC/Note", "Action", "Target"]
        )
        self._route_table.horizontalHeader().setStretchLastSection(True)
        self._route_table.verticalHeader().setVisible(False)
        self._route_table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        self._route_table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self._route_table.setAlternatingRowColors(True)
        self._route_table.setStyleSheet(_table_style())
        self._route_table.doubleClicked.connect(self._edit_selected_route)
        lay.addWidget(self._route_table, 1)

        btn_row = QHBoxLayout()
        btn_row.setSpacing(6)
        self._btn_add_route = _action_btn("＋  Add Route")
        self._btn_add_route.clicked.connect(self._add_route)
        self._btn_learn = _action_btn("🎹  Learn…", T.C_AMBER)
        self._btn_learn.clicked.connect(self._learn_route)
        self._btn_edit = _action_btn("✏  Edit")
        self._btn_edit.clicked.connect(self._edit_selected_route)
        self._btn_del = _action_btn("✕  Remove", T.C_RED)
        self._btn_del.clicked.connect(self._remove_selected_route)
        for b in [self._btn_add_route, self._btn_learn, self._btn_edit, self._btn_del]:
            btn_row.addWidget(b)
        btn_row.addStretch()
        lay.addLayout(btn_row)

        self._mgr.route_fired.connect(self._on_route_fired)

    def _refresh_ports(self) -> None:
        available = MIDIManager.available_ports()
        connected = set(self._mgr.open_port_names)

        self._port_list.blockSignals(True)
        self._port_list.clear()
        for name in available:
            item = QListWidgetItem(name)
            item.setFlags(item.flags() | Qt.ItemFlag.ItemIsUserCheckable)
            item.setCheckState(
                Qt.CheckState.Checked if name in connected else Qt.CheckState.Unchecked
            )
            self._port_list.addItem(item)

        if not available:
            item = QListWidgetItem("No MIDI ports detected")
            item.setFlags(Qt.ItemFlag.NoItemFlags)
            item.setForeground(QColor(T.C_TEXT_DIM))
            self._port_list.addItem(item)
        self._port_list.blockSignals(False)

    def _on_port_toggled(self, item: QListWidgetItem) -> None:
        name = item.text()
        if item.checkState() == Qt.CheckState.Checked:
            ok = self._mgr.open_port(name)
            if not ok:
                item.setCheckState(Qt.CheckState.Unchecked)
        else:
            self._mgr.close_port(name)

    def _refresh_routes(self) -> None:
        routes = self._mgr.routes
        self._route_table.setRowCount(len(routes))
        for row, r in enumerate(routes):
            target = _route_target_summary(r)
            ch_str = str(r.midi_channel) if r.midi_channel else "Any"
            self._route_table.setItem(row, 0, QTableWidgetItem(r.label or r.id))
            self._route_table.setItem(row, 1, QTableWidgetItem(r.msg_type))
            self._route_table.setItem(row, 2, QTableWidgetItem(ch_str))
            self._route_table.setItem(row, 3, QTableWidgetItem(str(r.note_or_cc)))
            self._route_table.setItem(row, 4, QTableWidgetItem(r.action))
            self._route_table.setItem(row, 5, QTableWidgetItem(target))
            if not r.enabled:
                for col in range(6):
                    item = self._route_table.item(row, col)
                    if item:
                        item.setForeground(QColor(T.C_TEXT_DIM))

    def _selected_route_idx(self) -> int:
        rows = self._route_table.selectionModel().selectedRows()
        return rows[0].row() if rows else -1

    def _add_route(self) -> None:
        dlg = _MIDIRouteDialog(
            available_ports=MIDIManager.available_ports(), parent=self
        )
        if dlg.exec() == QDialog.DialogCode.Accepted:
            self._mgr.add_route(dlg.result_route())
            self._refresh_routes()

    def _learn_route(self) -> None:
        if not self._mgr.open_port_names:
            QMessageBox.information(self, "MIDI Learn",
                                    "Connect at least one MIDI port first.")
            return
        dlg = _LearnDialog(self._mgr, parent=self)
        dlg.learned.connect(self._on_learned)
        dlg.exec()

    @Slot(str, str, int, int)
    def _on_learned(self, port_name: str, msg_type: str, channel: int, number: int) -> None:
        route = MIDIRoute(
            label=f"Learned {msg_type} ch{channel} #{number}",
            port_name=port_name,
            msg_type=msg_type,
            midi_channel=channel,
            note_or_cc=number,
        )
        edit_dlg = _MIDIRouteDialog(route, MIDIManager.available_ports(), self)
        if edit_dlg.exec() == QDialog.DialogCode.Accepted:
            self._mgr.add_route(edit_dlg.result_route())
            self._refresh_routes()

    def _edit_selected_route(self) -> None:
        idx = self._selected_route_idx()
        if idx < 0:
            return
        route = self._mgr.routes[idx]
        dlg = _MIDIRouteDialog(route, MIDIManager.available_ports(), self)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            self._refresh_routes()

    def _remove_selected_route(self) -> None:
        idx = self._selected_route_idx()
        if idx < 0:
            return
        self._mgr.remove_route(self._mgr.routes[idx].id)
        self._refresh_routes()

    @Slot(str, int, int)
    def _on_route_fired(self, route_id: str, midi_val: int, out_val: int) -> None:
        routes = self._mgr.routes
        for row, r in enumerate(routes):
            if r.id == route_id:
                item = self._route_table.item(row, 0)
                if item:
                    item.setBackground(QColor(T.C_ACCENT))
                    QTimer.singleShot(200, lambda r=row: self._clear_fire(r))
                break

    def _clear_fire(self, row: int) -> None:
        item = self._route_table.item(row, 0)
        if item:
            item.setBackground(QColor("transparent"))


def _route_target_summary(r: MIDIRoute) -> str:
    if r.action == "dmx_channel":
        return f"U{r.universe}/ch{r.dmx_channel}"
    if r.action in ("cue_go", "cue_back"):
        return f"cuelist {r.cuelist_id}"
    if r.action == "scene_activate":
        return r.scene_name or "—"
    return "—"


class _OSCPage(QWidget):
    """OSC input server configuration + route editor."""

    def __init__(self, osc_manager: OSCManager, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self._mgr = osc_manager
        self._build_ui()
        self._refresh_routes()

    def _build_ui(self) -> None:
        lay = QVBoxLayout(self)
        lay.setContentsMargins(16, 12, 16, 12)
        lay.setSpacing(8)

        # Input server group
        grp = QGroupBox("OSC Input Server")
        grp.setStyleSheet(
            f"QGroupBox {{ color: {T.C_TEXT}; font-weight: 600; font-size: 11px;"
            f" border: 1px solid {T.C_BORDER}; border-radius: 4px; margin-top: 6px; }}"
            f"QGroupBox::title {{ subcontrol-origin: margin; left: 10px;"
            f" padding: 0 4px; color: {T.C_ACCENT}; }}"
        )
        g_lay = QHBoxLayout(grp)
        g_lay.addWidget(QLabel("Listen port:"))
        self._srv_port = QSpinBox()
        self._srv_port.setRange(1, 65535)
        self._srv_port.setValue(7700)
        self._srv_port.setFixedWidth(90)
        g_lay.addWidget(self._srv_port)

        self._btn_start = _action_btn("▶  Start")
        self._btn_start.clicked.connect(self._start_server)
        self._btn_stop = _action_btn("■  Stop", T.C_RED)
        self._btn_stop.clicked.connect(self._stop_server)
        g_lay.addWidget(self._btn_start)
        g_lay.addWidget(self._btn_stop)

        self._srv_status = QLabel("Not running")
        self._srv_status.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 11px;")
        g_lay.addWidget(self._srv_status)
        g_lay.addStretch()
        lay.addWidget(grp)

        lay.addWidget(_section_label("OSC ROUTES"))
        lbl = QLabel("Map incoming OSC messages to Lumos actions.")
        lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 11px;")
        lay.addWidget(lbl)

        self._route_table = QTableWidget(0, 5)
        self._route_table.setHorizontalHeaderLabels(
            ["Label", "DMX Source", "OSC Destination", "Mode", "Status"]
        )
        self._route_table.horizontalHeader().setStretchLastSection(True)
        self._route_table.verticalHeader().setVisible(False)
        self._route_table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        self._route_table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self._route_table.setAlternatingRowColors(True)
        self._route_table.setStyleSheet(_table_style())
        self._route_table.doubleClicked.connect(self._edit_selected)
        lay.addWidget(self._route_table, 1)

        btn_row = QHBoxLayout()
        btn_row.setSpacing(6)
        self._btn_add = _action_btn("＋  Add Route")
        self._btn_add.clicked.connect(self._add_route)
        self._btn_edit = _action_btn("✏  Edit")
        self._btn_edit.clicked.connect(self._edit_selected)
        self._btn_del = _action_btn("✕  Remove", T.C_RED)
        self._btn_del.clicked.connect(self._remove_selected)
        for b in [self._btn_add, self._btn_edit, self._btn_del]:
            btn_row.addWidget(b)
        btn_row.addStretch()
        lay.addLayout(btn_row)

        # Test send group
        test_grp = QGroupBox("Test Send")
        test_grp.setStyleSheet(grp.styleSheet())
        t_lay = QHBoxLayout(test_grp)
        t_lay.addWidget(QLabel("Host:"))
        self._test_host = QLineEdit("127.0.0.1")
        self._test_host.setFixedWidth(130)
        t_lay.addWidget(self._test_host)
        t_lay.addWidget(QLabel("Port:"))
        self._test_port_spin = QSpinBox()
        self._test_port_spin.setRange(1, 65535)
        self._test_port_spin.setValue(7700)
        self._test_port_spin.setFixedWidth(80)
        t_lay.addWidget(self._test_port_spin)
        t_lay.addWidget(QLabel("Address:"))
        self._test_addr = QLineEdit("/test")
        self._test_addr.setFixedWidth(120)
        t_lay.addWidget(self._test_addr)
        t_lay.addWidget(QLabel("Value:"))
        self._test_val = QLineEdit("0.5")
        self._test_val.setFixedWidth(60)
        t_lay.addWidget(self._test_val)
        btn_send = _action_btn("Send")
        btn_send.clicked.connect(self._test_send)
        t_lay.addWidget(btn_send)
        self._test_result = QLabel("")
        self._test_result.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 11px;")
        t_lay.addWidget(self._test_result)
        t_lay.addStretch()
        lay.addWidget(test_grp)

        self._mgr.route_fired.connect(self._on_route_fired)

    def _start_server(self) -> None:
        port = self._srv_port.value()
        ok = self._mgr.start_osc_server(port)
        if ok:
            self._srv_status.setText(f"Running on :{port}")
            self._srv_status.setStyleSheet("color: #4caf50; font-size: 11px;")
        else:
            self._srv_status.setText("Failed to start")
            self._srv_status.setStyleSheet(f"color: {T.C_RED}; font-size: 11px;")

    def _stop_server(self) -> None:
        self._mgr.stop_osc_server()
        self._srv_status.setText("Stopped")
        self._srv_status.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 11px;")

    def _refresh_routes(self) -> None:
        routes = self._mgr.routes
        self._route_table.setRowCount(len(routes))
        for row, r in enumerate(routes):
            source = f"U{r.universe}/ch{r.channel}"
            dest = f"{r.host}:{r.port}  {r.address}"
            self._route_table.setItem(row, 0, QTableWidgetItem(r.label or r.id))
            self._route_table.setItem(row, 1, QTableWidgetItem(source))
            self._route_table.setItem(row, 2, QTableWidgetItem(dest))
            self._route_table.setItem(row, 3, QTableWidgetItem(r.mode))
            enabled_item = QTableWidgetItem("✓" if r.enabled else "✗")
            enabled_item.setForeground(
                QColor("#4caf50") if r.enabled else QColor(T.C_TEXT_DIM)
            )
            self._route_table.setItem(row, 4, enabled_item)

    def _selected_row(self) -> int:
        rows = self._route_table.selectionModel().selectedRows()
        return rows[0].row() if rows else -1

    def _add_route(self) -> None:
        dlg = _OSCRouteDialog(parent=self)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            self._mgr.add_route(dlg.result_route())
            self._refresh_routes()

    def _edit_selected(self) -> None:
        idx = self._selected_row()
        if idx < 0:
            return
        route = self._mgr.routes[idx]
        dlg = _OSCRouteDialog(route, parent=self)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            self._refresh_routes()

    def _remove_selected(self) -> None:
        idx = self._selected_row()
        if idx < 0:
            return
        self._mgr.remove_route(self._mgr.routes[idx].id)
        self._refresh_routes()

    def _test_send(self) -> None:
        try:
            val = float(self._test_val.text())
        except ValueError:
            val = 0.0
        ok = self._mgr.quick_send(
            self._test_host.text().strip(),
            self._test_port_spin.value(),
            self._test_addr.text().strip(),
            val,
        )
        self._test_result.setText("Sent ✓" if ok else "Failed ✗")
        self._test_result.setStyleSheet(
            f"color: {'#4caf50' if ok else T.C_RED}; font-size: 11px;"
        )

    @Slot(str, object)
    def _on_route_fired(self, route_id: str, value) -> None:
        routes = self._mgr.routes
        for row, r in enumerate(routes):
            if r.id == route_id:
                item = self._route_table.item(row, 0)
                if item:
                    item.setBackground(QColor(T.C_ACCENT))
                    QTimer.singleShot(200, lambda r=row: self._clear_fire(r))
                break

    def _clear_fire(self, row: int) -> None:
        item = self._route_table.item(row, 0)
        if item:
            item.setBackground(QColor("transparent"))


class _TimecodePageWidget(QWidget):
    """Timecode source configuration."""

    def __init__(self, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(16, 12, 16, 12)
        lay.setSpacing(10)

        lay.addWidget(_section_label("TIMECODE SOURCE"))
        lbl = QLabel(
            "Select the timecode source. LTC arrives via audio input. "
            "MTC arrives via MIDI. Internal uses the engine's own clock."
        )
        lbl.setWordWrap(True)
        lbl.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 11px;")
        lay.addWidget(lbl)

        self._ltc = QRadioButton("LTC  (Linear Timecode — audio input)")
        self._mtc = QRadioButton("MTC  (MIDI Timecode — MIDI input)")
        self._internal = QRadioButton("Internal  (software clock)")
        self._internal.setChecked(True)
        for rb in [self._ltc, self._mtc, self._internal]:
            rb.setStyleSheet(f"color: {T.C_TEXT}; font-size: 12px;")
            lay.addWidget(rb)

        lay.addWidget(_hline())
        form = QFormLayout()

        self._fps = QComboBox()
        self._fps.addItems(["23.976", "24", "25", "29.97", "30"])
        self._fps.setCurrentText("25")
        form.addRow("Frame rate:", self._fps)

        self._offset = QDoubleSpinBox()
        self._offset.setRange(-3600.0, 3600.0)
        self._offset.setDecimals(3)
        self._offset.setSuffix("  s")
        form.addRow("Offset:", self._offset)

        self._ltc_audio_dev = QComboBox()
        self._ltc_audio_dev.addItem("Default audio input", "")
        form.addRow("LTC audio device:", self._ltc_audio_dev)

        self._mtc_port = QComboBox()
        self._mtc_port.addItem("None", "")
        form.addRow("MTC MIDI port:", self._mtc_port)

        lay.addLayout(form)

        status_row = QHBoxLayout()
        self._tc_status = QLabel("00:00:00:00")
        self._tc_status.setStyleSheet(
            "font-family: monospace; font-size: 20px; font-weight: 700;"
            f" color: {T.C_AMBER}; padding: 8px;"
        )
        status_row.addWidget(self._tc_status)
        status_row.addStretch()
        lay.addLayout(status_row)
        lay.addStretch()


class _NetworkPage(QWidget):
    """Global Art-Net and sACN network settings."""

    def __init__(self, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(16, 12, 16, 12)
        lay.setSpacing(10)

        grp_style = (
            f"QGroupBox {{ color: {T.C_TEXT}; font-weight: 600; font-size: 11px;"
            f" border: 1px solid {T.C_BORDER}; border-radius: 4px; margin-top: 6px; }}"
            f"QGroupBox::title {{ subcontrol-origin: margin; left: 10px;"
            f" padding: 0 4px; color: {T.C_ACCENT}; }}"
        )

        # Art-Net
        artnet_grp = QGroupBox("Art-Net")
        artnet_grp.setStyleSheet(grp_style)
        af = QFormLayout(artnet_grp)
        self._an_iface = QLineEdit("")
        self._an_iface.setPlaceholderText("0.0.0.0  (all interfaces)")
        af.addRow("Bind interface:", self._an_iface)
        self._an_reply_ip = QLineEdit("")
        self._an_reply_ip.setPlaceholderText("Auto-detect from interface")
        af.addRow("ArtPollReply IP:", self._an_reply_ip)
        self._an_short = QLineEdit("Lumos")
        af.addRow("Short name:", self._an_short)
        self._an_long = QLineEdit("Lumos Lighting Control")
        af.addRow("Long name:", self._an_long)
        lay.addWidget(artnet_grp)

        # sACN
        sacn_grp = QGroupBox("sACN / E1.31")
        sacn_grp.setStyleSheet(grp_style)
        sf = QFormLayout(sacn_grp)
        self._sacn_iface = QLineEdit("")
        self._sacn_iface.setPlaceholderText("0.0.0.0  (all interfaces)")
        sf.addRow("Bind interface:", self._sacn_iface)
        self._sacn_source = QLineEdit("Lumos")
        sf.addRow("Source name:", self._sacn_source)
        self._sacn_priority = QSpinBox()
        self._sacn_priority.setRange(0, 200)
        self._sacn_priority.setValue(100)
        sf.addRow("Default priority:", self._sacn_priority)
        self._sacn_multicast = QCheckBox("Use multicast (default)")
        self._sacn_multicast.setChecked(True)
        self._sacn_multicast.setStyleSheet(f"color: {T.C_TEXT};")
        sf.addRow("", self._sacn_multicast)
        lay.addWidget(sacn_grp)
        lay.addStretch()


class _GeneralPage(QWidget):
    """General engine and application settings."""

    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self._engine = engine
        lay = QVBoxLayout(self)
        lay.setContentsMargins(16, 12, 16, 12)
        lay.setSpacing(10)

        lay.addWidget(_section_label("ENGINE"))
        form = QFormLayout()

        self._tick_rate = QSpinBox()
        self._tick_rate.setRange(1, 200)
        self._tick_rate.setValue(44)
        self._tick_rate.setSuffix("  Hz")
        form.addRow("DMX update rate:", self._tick_rate)

        self._fade_resolution = QSpinBox()
        self._fade_resolution.setRange(1, 100)
        self._fade_resolution.setValue(44)
        self._fade_resolution.setSuffix("  Hz")
        form.addRow("Fade resolution:", self._fade_resolution)

        lay.addLayout(form)

        lay.addWidget(_section_label("APPEARANCE"))
        form2 = QFormLayout()
        self._theme = QComboBox()
        self._theme.addItems(["Dark (default)", "Very dark", "Light"])
        form2.addRow("Theme:", self._theme)
        lay.addLayout(form2)

        lay.addWidget(_section_label("STARTUP"))
        form3 = QFormLayout()
        self._auto_load = QCheckBox("Load last show on startup")
        self._auto_load.setChecked(True)
        self._auto_load.setStyleSheet(f"color: {T.C_TEXT};")
        form3.addRow("", self._auto_load)
        lay.addLayout(form3)

        lay.addWidget(_hline())

        # About
        about = QLabel(
            "<b>Lumos</b> — Professional Theatrical Lighting Control<br>"
            "Architecture inspired by QLC+, Chamsys MagicQ, and grandMA3<br>"
            "<span style='color:#666;'>GPL-3.0 license</span>"
        )
        about.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 11px; padding: 8px;")
        about.setWordWrap(True)
        lay.addWidget(about)
        lay.addStretch()


# ── Main settings panel ──────────────────────────────────────────────────────

_CATEGORIES = [
    ("🔌  Universes", "Universes"),
    ("🎹  MIDI",      "MIDI"),
    ("🎵  OSC",       "OSC"),
    ("⏱  Timecode",  "Timecode"),
    ("🌐  Network",   "Network"),
    ("⚙  General",  "General"),
]


class SettingsPanel(QWidget):
    """
    Unified settings hub.

    Left: category list.
    Right: page for the selected category.
    """

    def __init__(
        self,
        engine: LumosEngine,
        output_manager: OutputManager,
        osc_manager: OSCManager,
        midi_manager: MIDIManager,
        parent: Optional[QWidget] = None,
    ) -> None:
        super().__init__(parent)
        self._engine = engine
        self._om = output_manager
        self._osc = osc_manager
        self._midi = midi_manager
        self._build_ui()

    def _build_ui(self) -> None:
        root = QHBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        # ── Left sidebar ──────────────────────────────────────────────────
        sidebar = QWidget()
        sidebar.setFixedWidth(180)
        sidebar.setStyleSheet(f"background: {T.C_PANEL};")
        s_lay = QVBoxLayout(sidebar)
        s_lay.setContentsMargins(0, 0, 0, 0)
        s_lay.setSpacing(0)

        title = QLabel("  SETTINGS")
        title.setFixedHeight(44)
        title.setStyleSheet(
            f"background: {T.C_PANEL2}; color: {T.C_TEXT_DIM};"
            f" font-size: 11px; font-weight: 700; letter-spacing: 2px;"
            f" border-bottom: 1px solid {T.C_BORDER};"
        )
        s_lay.addWidget(title)

        self._cat_list = QListWidget()
        self._cat_list.setStyleSheet(
            f"QListWidget {{ background: {T.C_PANEL}; border: none; padding: 4px 0; }}"
            f"QListWidget::item {{ color: {T.C_TEXT_DIM}; padding: 10px 16px;"
            f" font-size: 12px; border: none; }}"
            f"QListWidget::item:selected {{ background: {T.C_ACCENT};"
            f" color: {T.C_TEXT_HDR}; }}"
            f"QListWidget::item:hover:!selected {{ background: {T.C_PANEL2};"
            f" color: {T.C_TEXT}; }}"
        )
        for label, _ in _CATEGORIES:
            item = QListWidgetItem(label)
            self._cat_list.addItem(item)
        self._cat_list.setCurrentRow(0)
        self._cat_list.currentRowChanged.connect(self._on_category_changed)
        s_lay.addWidget(self._cat_list, 1)

        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.VLine)
        sep.setStyleSheet(f"color: {T.C_BORDER};")

        # ── Right content area ────────────────────────────────────────────
        self._pages = QStackedWidget()

        self._page_universes = _UniversesPage(self._engine, self._om)
        self._page_midi = _MIDIPage(self._midi)
        self._page_osc = _OSCPage(self._osc)
        self._page_timecode = _TimecodePageWidget()
        self._page_network = _NetworkPage()
        self._page_general = _GeneralPage(self._engine)

        for page in [
            self._page_universes, self._page_midi, self._page_osc,
            self._page_timecode, self._page_network, self._page_general,
        ]:
            self._pages.addWidget(page)

        root.addWidget(sidebar)
        root.addWidget(sep)
        root.addWidget(self._pages, 1)

    @Slot(int)
    def _on_category_changed(self, idx: int) -> None:
        self._pages.setCurrentIndex(idx)
