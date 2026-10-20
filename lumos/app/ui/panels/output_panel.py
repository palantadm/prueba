"""
Output Configuration panel.

Shows one card per universe, lets the user pick a backend (Art-Net, sACN,
Enttec Open DMX, Enttec Pro) and configure its parameters, then apply it
to the OutputManager.
"""
from __future__ import annotations

from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QComboBox, QSpinBox, QLineEdit, QFrame, QScrollArea,
    QStackedWidget, QSizePolicy,
)
from PySide6.QtCore import Qt
from PySide6.QtGui import QColor

from app.engine import LumosEngine
from app.ui import theme as T
from app.outputs import (
    OutputBackend,
    ArtNetBackend,
    SACNBackend,
    EnttecOpenBackend,
    EnttecProBackend,
    UDMXBackend,
    ChamsysMagicDMXBackend,
    FTDIRawBackend,
    DMXkingBackend,
    OSCBackend,
    OutputManager,
)

# Backend options in combo order (index must match _apply / _load_existing)
_BACKEND_LABELS = [
    "None",                    # 0
    "Art-Net (UDP)",           # 1
    "sACN E1.31",              # 2
    "Enttec Open DMX USB",     # 3
    "Enttec Pro (USB)",        # 4
    "uDMX (USB HID)",          # 5  — requires: pip install pyusb
    "Chamsys MagicDMX",        # 6
    "DMXking ultraDMX",        # 7
    "FTDI Raw (Open DMX)",     # 8
    "OSC (UDP)",               # 9  — requires: pip install python-osc
]

_STATUS_STYLE = {
    "none":  f"background:{T.C_TEXT_DIM};  border-radius:5px; min-width:10px; min-height:10px; max-width:10px; max-height:10px;",
    "ok":    f"background:{T.C_GREEN};     border-radius:5px; min-width:10px; min-height:10px; max-width:10px; max-height:10px;",
    "error": f"background:{T.C_RED};       border-radius:5px; min-width:10px; min-height:10px; max-width:10px; max-height:10px;",
}


# ---------------------------------------------------------------------------
# Per-universe card
# ---------------------------------------------------------------------------

class _UniverseCard(QFrame):
    """Card widget for one universe row."""

    def __init__(self, universe_id: int, manager: OutputManager, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self._uid = universe_id
        self._manager = manager
        self._build_ui()

    # ------------------------------------------------------------------
    # Build
    # ------------------------------------------------------------------

    def _build_ui(self) -> None:
        self.setFrameShape(QFrame.Shape.StyledPanel)
        self.setStyleSheet(
            f"QFrame {{ background:{T.C_PANEL}; border:1px solid {T.C_BORDER};"
            f" border-radius:4px; padding:6px; }}"
        )

        outer = QVBoxLayout(self)
        outer.setContentsMargins(8, 8, 8, 8)
        outer.setSpacing(6)

        # --- Top row: label + status dot + combo + apply button ---
        top = QHBoxLayout()
        top.setSpacing(8)

        uni_lbl = QLabel(f"Universe {self._uid}")
        uni_lbl.setStyleSheet(
            f"color:{T.C_TEXT_HDR}; font-weight:bold; font-size:12px;"
        )
        top.addWidget(uni_lbl)

        top.addStretch(1)

        # Status dot
        self._status_dot = QLabel()
        self._status_dot.setFixedSize(10, 10)
        self._set_status("none")
        top.addWidget(self._status_dot)

        # Backend selector
        self._combo = QComboBox()
        self._combo.addItems(_BACKEND_LABELS)
        self._combo.setFixedWidth(160)
        self._combo.setStyleSheet(
            f"QComboBox {{ background:{T.C_WIDGET}; color:{T.C_TEXT};"
            f" border:1px solid {T.C_BORDER_LT}; border-radius:3px; padding:3px 6px; }}"
            f"QComboBox QAbstractItemView {{ background:{T.C_WIDGET}; color:{T.C_TEXT};"
            f" selection-background-color:{T.C_ACCENT}; }}"
        )
        self._combo.currentIndexChanged.connect(self._on_type_changed)
        top.addWidget(self._combo)

        # Apply button
        self._apply_btn = QPushButton("Apply")
        self._apply_btn.setFixedWidth(60)
        self._apply_btn.setStyleSheet(
            f"QPushButton {{ background:{T.C_ACCENT}; color:{T.C_TEXT_HDR};"
            f" border:none; border-radius:3px; padding:4px 10px; font-weight:bold; }}"
            f"QPushButton:hover {{ background:#00aadf; }}"
            f"QPushButton:pressed {{ background:#0077a0; }}"
        )
        self._apply_btn.clicked.connect(self._apply)
        top.addWidget(self._apply_btn)

        outer.addLayout(top)

        # --- Settings stack (one page per backend type) ---
        self._stack = QStackedWidget()
        self._stack.setStyleSheet("background:transparent;")

        # Page 0: None
        self._stack.addWidget(self._make_none_page())
        # Page 1: Art-Net
        self._artnet_ip, self._artnet_port, page1 = self._make_artnet_page()
        self._stack.addWidget(page1)
        # Page 2: sACN
        self._sacn_ip, self._sacn_port, self._sacn_pri, self._sacn_src, page2 = self._make_sacn_page()
        self._stack.addWidget(page2)
        # Page 3: Enttec Open
        self._open_port, page3 = self._make_serial_page("Open DMX port:", "/dev/ttyUSB0")
        self._stack.addWidget(page3)
        # Page 4: Enttec Pro
        self._pro_port, page4 = self._make_serial_page("Pro port:", "/dev/ttyUSB0")
        self._stack.addWidget(page4)
        # Page 5: uDMX
        self._stack.addWidget(self._make_udmx_page())
        # Page 6: Chamsys MagicDMX
        self._chamsys_port, self._chamsys_mode, page6 = self._make_chamsys_page()
        self._stack.addWidget(page6)
        # Page 7: DMXking
        self._dmxking_port, page7 = self._make_serial_page("DMXking port:", "/dev/ttyUSB0")
        self._stack.addWidget(page7)
        # Page 8: FTDI Raw
        self._ftdi_port, page8 = self._make_serial_page("FTDI port:", "/dev/ttyUSB0")
        self._stack.addWidget(page8)
        # Page 9: OSC
        self._osc_host, self._osc_port_spin, self._osc_pattern, self._osc_mode, page9 = self._make_osc_page()
        self._stack.addWidget(page9)

        outer.addWidget(self._stack)

        # Populate from existing backend if one is set
        self._load_existing()

    # ------------------------------------------------------------------
    # Page builders
    # ------------------------------------------------------------------

    def _field_style(self) -> str:
        return (
            f"background:{T.C_INPUT}; color:{T.C_TEXT}; border:1px solid {T.C_BORDER_LT};"
            f" border-radius:3px; padding:3px 6px;"
        )

    def _label_style(self) -> str:
        return f"color:{T.C_TEXT_LABEL}; font-size:11px;"

    def _make_none_page(self) -> QWidget:
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        lbl = QLabel("No output — universe will be silent.")
        lbl.setStyleSheet(f"color:{T.C_TEXT_DIM}; font-style:italic;")
        lay.addWidget(lbl)
        return w

    def _make_artnet_page(self):
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(12)

        lay.addWidget(QLabel("IP:"))
        ip = QLineEdit("255.255.255.255")
        ip.setFixedWidth(160)
        ip.setStyleSheet(self._field_style())
        ip.setPlaceholderText("255.255.255.255")
        lay.addWidget(ip)

        lay.addWidget(QLabel("Port:"))
        port = QSpinBox()
        port.setRange(1, 65535)
        port.setValue(6454)
        port.setFixedWidth(80)
        port.setStyleSheet(
            f"QSpinBox {{ {self._field_style()} }}"
        )
        lay.addWidget(port)
        lay.addStretch(1)

        for lbl in w.findChildren(QLabel):
            lbl.setStyleSheet(self._label_style())

        return ip, port, w

    def _make_sacn_page(self):
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(12)

        lbl_ip = QLabel("IP (empty=multicast):")
        lbl_ip.setStyleSheet(self._label_style())
        lay.addWidget(lbl_ip)
        ip = QLineEdit("")
        ip.setFixedWidth(140)
        ip.setStyleSheet(self._field_style())
        ip.setPlaceholderText("empty = multicast")
        lay.addWidget(ip)

        lbl_pri = QLabel("Priority:")
        lbl_pri.setStyleSheet(self._label_style())
        lay.addWidget(lbl_pri)
        pri = QSpinBox()
        pri.setRange(1, 200)
        pri.setValue(100)
        pri.setFixedWidth(65)
        pri.setStyleSheet(f"QSpinBox {{ {self._field_style()} }}")
        lay.addWidget(pri)

        lbl_src = QLabel("Source:")
        lbl_src.setStyleSheet(self._label_style())
        lay.addWidget(lbl_src)
        src = QLineEdit("Lumos")
        src.setFixedWidth(100)
        src.setStyleSheet(self._field_style())
        lay.addWidget(src)

        lbl_port = QLabel("Port:")
        lbl_port.setStyleSheet(self._label_style())
        lay.addWidget(lbl_port)
        port = QSpinBox()
        port.setRange(1, 65535)
        port.setValue(5568)
        port.setFixedWidth(80)
        port.setStyleSheet(f"QSpinBox {{ {self._field_style()} }}")
        lay.addWidget(port)

        lay.addStretch(1)
        return ip, port, pri, src, w

    def _make_serial_page(self, label_text: str, default_port: str = "/dev/ttyUSB0"):
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(12)

        lbl = QLabel(label_text)
        lbl.setStyleSheet(self._label_style())
        lay.addWidget(lbl)

        port = QLineEdit(default_port)
        port.setFixedWidth(200)
        port.setStyleSheet(self._field_style())
        port.setPlaceholderText("/dev/ttyUSB0 or COM3")
        lay.addWidget(port)
        lay.addStretch(1)
        return port, w

    def _make_udmx_page(self) -> QWidget:
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        lbl = QLabel("uDMX — USB HID device (VID 0x16C0, PID 0x05DC).  Requires: pip install pyusb")
        lbl.setStyleSheet(f"color:{T.C_TEXT_LABEL}; font-style:italic; font-size:11px;")
        lay.addWidget(lbl)
        return w

    def _make_chamsys_page(self):
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(12)

        lay.addWidget(self._lbl("Port:"))
        port = QLineEdit("/dev/ttyACM0")
        port.setFixedWidth(180)
        port.setStyleSheet(self._field_style())
        port.setPlaceholderText("/dev/ttyACM0-A or COM4-B")
        lay.addWidget(port)

        lay.addWidget(self._lbl("Model:"))
        mode = QComboBox()
        mode.addItems(["Basic (1 universe)", "Full (2 universes)"])
        mode.setFixedWidth(160)
        mode.setStyleSheet(
            f"QComboBox {{ background:{T.C_WIDGET}; color:{T.C_TEXT};"
            f" border:1px solid {T.C_BORDER_LT}; border-radius:3px; padding:2px 6px; }}"
        )
        lay.addWidget(mode)
        lay.addStretch(1)
        return port, mode, w

    def _make_osc_page(self):
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(10)

        lay.addWidget(self._lbl("Host:"))
        host = QLineEdit("127.0.0.1")
        host.setFixedWidth(130)
        host.setStyleSheet(self._field_style())
        lay.addWidget(host)

        lay.addWidget(self._lbl("Port:"))
        port = QSpinBox(); port.setRange(1, 65535); port.setValue(8000)
        port.setFixedWidth(75)
        port.setStyleSheet(f"QSpinBox {{ {self._field_style()} }}")
        lay.addWidget(port)

        lay.addWidget(self._lbl("Pattern:"))
        pattern = QLineEdit("/dmx/{universe}/{channel}")
        pattern.setFixedWidth(190)
        pattern.setStyleSheet(self._field_style())
        lay.addWidget(pattern)

        lay.addWidget(self._lbl("Mode:"))
        mode = QComboBox()
        mode.addItems(["per_channel", "blob", "both"])
        mode.setFixedWidth(100)
        mode.setStyleSheet(
            f"QComboBox {{ background:{T.C_WIDGET}; color:{T.C_TEXT};"
            f" border:1px solid {T.C_BORDER_LT}; border-radius:3px; padding:2px 6px; }}"
        )
        lay.addWidget(mode)
        lay.addStretch(1)
        return host, port, pattern, mode, w

    def _lbl(self, text: str) -> QLabel:
        l = QLabel(text)
        l.setStyleSheet(self._label_style())
        return l

    # ------------------------------------------------------------------
    # Logic
    # ------------------------------------------------------------------

    def _set_status(self, state: str) -> None:
        self._status_dot.setStyleSheet(_STATUS_STYLE.get(state, _STATUS_STYLE["none"]))

    def _on_type_changed(self, index: int) -> None:
        self._stack.setCurrentIndex(index)
        if index == 0:
            self._set_status("none")

    def _load_existing(self) -> None:
        """Populate UI from any already-configured backend."""
        backend = self._manager.get_backend(self._uid)
        if backend is None:
            self._combo.setCurrentIndex(0)
            return
        if isinstance(backend, ArtNetBackend):
            self._combo.setCurrentIndex(1)
            self._artnet_ip.setText(backend.ip)
            self._artnet_port.setValue(backend.port)
        elif isinstance(backend, SACNBackend):
            self._combo.setCurrentIndex(2)
            self._sacn_ip.setText(backend.ip)
            self._sacn_port.setValue(backend.port)
            self._sacn_pri.setValue(backend.priority)
            self._sacn_src.setText(backend.source_name)
        elif isinstance(backend, DMXkingBackend):
            self._combo.setCurrentIndex(7)
            self._dmxking_port.setText(backend.port)
        elif isinstance(backend, FTDIRawBackend):
            self._combo.setCurrentIndex(8)
            self._ftdi_port.setText(backend.port)
        elif isinstance(backend, EnttecOpenBackend):
            self._combo.setCurrentIndex(3)
            self._open_port.setText(backend.port)
        elif isinstance(backend, EnttecProBackend):
            self._combo.setCurrentIndex(4)
            self._pro_port.setText(backend.port)
        elif isinstance(backend, UDMXBackend):
            self._combo.setCurrentIndex(5)
        elif isinstance(backend, ChamsysMagicDMXBackend):
            self._combo.setCurrentIndex(6)
            self._chamsys_port.setText(backend.port)
            self._chamsys_mode.setCurrentIndex(0 if backend.mode == "basic" else 1)
        elif isinstance(backend, OSCBackend):
            self._combo.setCurrentIndex(9)
            self._osc_host.setText(backend.host)
            self._osc_port_spin.setValue(backend.port)
            self._osc_pattern.setText(backend.address_pattern)
            self._osc_mode.setCurrentText(backend.mode)
        self._set_status("ok" if backend.is_open else "error")

    def _apply(self) -> None:
        """Build the backend from current UI state and open it."""
        idx = self._combo.currentIndex()

        if idx == 0:
            # None — remove any existing backend
            self._manager.remove_backend(self._uid)
            self._set_status("none")
            return

        if idx == 1:
            backend: OutputBackend = ArtNetBackend(
                ip=self._artnet_ip.text().strip() or "255.255.255.255",
                port=self._artnet_port.value(),
            )
        elif idx == 2:
            backend = SACNBackend(
                ip=self._sacn_ip.text().strip(),
                port=self._sacn_port.value(),
                priority=self._sacn_pri.value(),
                source_name=self._sacn_src.text().strip() or "Lumos",
            )
        elif idx == 3:
            backend = EnttecOpenBackend(
                port=self._open_port.text().strip() or "/dev/ttyUSB0",
            )
        elif idx == 4:
            backend = EnttecProBackend(
                port=self._pro_port.text().strip() or "/dev/ttyUSB0",
            )
        elif idx == 5:
            backend = UDMXBackend()
        elif idx == 6:
            mode_str = "full" if self._chamsys_mode.currentIndex() == 1 else "basic"
            backend = ChamsysMagicDMXBackend(
                port=self._chamsys_port.text().strip() or "/dev/ttyACM0",
                mode=mode_str,
            )
        elif idx == 7:
            backend = DMXkingBackend(
                port=self._dmxking_port.text().strip() or "/dev/ttyUSB0",
            )
        elif idx == 8:
            backend = FTDIRawBackend(
                port=self._ftdi_port.text().strip() or "/dev/ttyUSB0",
            )
        elif idx == 9:
            backend = OSCBackend(
                host=self._osc_host.text().strip() or "127.0.0.1",
                port=self._osc_port_spin.value(),
                address_pattern=self._osc_pattern.text().strip() or "/dmx/{universe}/{channel}",
                mode=self._osc_mode.currentText(),
            )
        else:
            return

        self._manager.set_backend(self._uid, backend)
        success = backend.open()
        self._set_status("ok" if success else "error")


# ---------------------------------------------------------------------------
# Main panel
# ---------------------------------------------------------------------------

class OutputPanel(QWidget):
    """
    Output Configuration panel — top-level widget.

    Usage::

        panel = OutputPanel(engine)
        splitter.addWidget(panel)
    """

    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self._engine = engine
        self._manager = OutputManager(engine)
        # Forward engine DMX signal to manager
        engine.dmx_updated.connect(self._manager._on_dmx_updated)
        self._cards: dict[int, _UniverseCard] = {}
        self._build_ui()
        self._populate_universes()

    # ------------------------------------------------------------------
    # Build
    # ------------------------------------------------------------------

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        # ── Header bar ────────────────────────────────────────────────
        header = QFrame()
        header.setFixedHeight(36)
        header.setStyleSheet(
            f"background:{T.C_PANEL2}; border-bottom:1px solid {T.C_BORDER};"
        )
        h_lay = QHBoxLayout(header)
        h_lay.setContentsMargins(12, 0, 12, 0)

        title = QLabel("OUTPUT CONFIGURATION")
        title.setStyleSheet(
            f"color:{T.C_TEXT_HDR}; font-weight:bold; font-size:12px;"
            f" letter-spacing:1px; background:transparent; border:none;"
        )
        h_lay.addWidget(title)
        h_lay.addStretch(1)

        root.addWidget(header)

        # ── Scroll area for universe cards ────────────────────────────
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setStyleSheet(
            f"QScrollArea {{ background:{T.C_BG}; border:none; }}"
        )

        self._cards_widget = QWidget()
        self._cards_widget.setStyleSheet(f"background:{T.C_BG};")
        self._cards_layout = QVBoxLayout(self._cards_widget)
        self._cards_layout.setContentsMargins(12, 12, 12, 12)
        self._cards_layout.setSpacing(8)
        self._cards_layout.addStretch(1)

        scroll.setWidget(self._cards_widget)
        root.addWidget(scroll, 1)

        # ── Bottom bar: Add Universe ───────────────────────────────────
        bottom = QFrame()
        bottom.setFixedHeight(40)
        bottom.setStyleSheet(
            f"background:{T.C_PANEL2}; border-top:1px solid {T.C_BORDER};"
        )
        b_lay = QHBoxLayout(bottom)
        b_lay.setContentsMargins(12, 0, 12, 0)

        add_btn = QPushButton("+ Add Universe")
        add_btn.setStyleSheet(
            f"QPushButton {{ background:{T.C_WIDGET}; color:{T.C_TEXT};"
            f" border:1px solid {T.C_BORDER_LT}; border-radius:3px; padding:4px 12px; }}"
            f"QPushButton:hover {{ border-color:{T.C_ACCENT}; color:{T.C_TEXT_HDR}; }}"
        )
        add_btn.clicked.connect(self._add_universe)
        b_lay.addWidget(add_btn)
        b_lay.addStretch(1)

        root.addWidget(bottom)

    # ------------------------------------------------------------------
    # Universe management
    # ------------------------------------------------------------------

    def _populate_universes(self) -> None:
        """Add cards for all universes currently in the engine."""
        for uid in sorted(self._engine.universes.keys()):
            self._add_card(uid)

    def _add_card(self, universe_id: int) -> None:
        if universe_id in self._cards:
            return
        card = _UniverseCard(universe_id, self._manager, self._cards_widget)
        self._cards[universe_id] = card
        # Insert before the trailing stretch
        stretch_idx = self._cards_layout.count() - 1
        self._cards_layout.insertWidget(stretch_idx, card)

    def _add_universe(self) -> None:
        """Add a new universe (next available ID) to the engine and panel."""
        existing = set(self._engine.universes.keys())
        uid = 1
        while uid in existing:
            uid += 1
        self._engine.add_universe(uid, f"Universe {uid}")
        self._add_card(uid)

    # ------------------------------------------------------------------
    # Public helpers
    # ------------------------------------------------------------------

    @property
    def manager(self) -> OutputManager:
        """The OutputManager instance driving this panel."""
        return self._manager
