"""
Lumos theme — grandMA3-inspired dark professional palette.
"""
from PySide6.QtWidgets import QApplication
from PySide6.QtGui import QPalette, QColor

# ── Core palette ──────────────────────────────────────────────────────────────
C_BG          = "#0c0c0c"
C_PANEL       = "#141414"
C_PANEL2      = "#1a1a1a"
C_WIDGET      = "#1e1e1e"
C_INPUT       = "#242424"
C_BORDER      = "#2a2a2a"
C_BORDER_LT   = "#3a3a3a"
C_HANDLE      = "#1c1c1c"

C_TEXT        = "#c8c8c8"
C_TEXT_DIM    = "#666666"
C_TEXT_HDR    = "#ffffff"
C_TEXT_LABEL  = "#909090"

# ── Accents ───────────────────────────────────────────────────────────────────
C_ACCENT      = "#0097c8"   # MA3 cyan — selection / focus
C_GREEN       = "#40b840"   # GO / live
C_RED         = "#c83030"   # STOP / blackout
C_AMBER       = "#c87820"   # warning / grand master
C_BLUE        = "#2060c0"   # info

# ── Channel type colours (MA3 standard) ──────────────────────────────────────
C_DIMMER    = "#f0e0a0"    # warm white — intensity
C_COLOR     = "#e040a0"    # magenta-pink — colour
C_POSITION  = "#00d4e8"    # cyan — pan/tilt
C_BEAM      = "#ff8c00"    # orange — beam (gobo, iris, focus, zoom)
C_CONTROL   = "#60c040"    # green — control channels
C_EFFECT    = "#c080ff"    # purple — effects

# ── Fixture type colours ──────────────────────────────────────────────────────
FX_COLORS = {
    "Moving Head": C_POSITION,
    "LED PAR":     C_BEAM,
    "Dimmer":      "#808080",
    "Strobe":      C_RED,
    "Laser":       C_GREEN,
    "Fogger":      "#607080",
    "LED Strip":   C_ACCENT,
}


QSS = f"""
/* ── Global ──────────────────────────────────────────────────────────── */
* {{
    outline: none;
}}
QWidget {{
    background-color: {C_BG};
    color: {C_TEXT};
    font-family: "Segoe UI", Arial, sans-serif;
    font-size: 11px;
    selection-background-color: {C_ACCENT};
    selection-color: {C_TEXT_HDR};
}}

/* ── Splitter ─────────────────────────────────────────────────────────── */
QSplitter::handle {{
    background-color: {C_BORDER};
}}
QSplitter::handle:horizontal {{
    width: 4px;
}}
QSplitter::handle:vertical {{
    height: 4px;
}}
QSplitter::handle:hover {{
    background-color: {C_ACCENT};
}}

/* ── Scroll bars ─────────────────────────────────────────────────────── */
QScrollBar:vertical {{
    background: {C_BG};
    width: 6px;
    border: none;
    margin: 0;
}}
QScrollBar::handle:vertical {{
    background: {C_BORDER_LT};
    border-radius: 3px;
    min-height: 24px;
}}
QScrollBar::handle:vertical:hover {{
    background: {C_ACCENT};
}}
QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{ height: 0; }}
QScrollBar:horizontal {{
    background: {C_BG};
    height: 6px;
    border: none;
    margin: 0;
}}
QScrollBar::handle:horizontal {{
    background: {C_BORDER_LT};
    border-radius: 3px;
    min-width: 24px;
}}
QScrollBar::handle:horizontal:hover {{
    background: {C_ACCENT};
}}
QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {{ width: 0; }}

/* ── Push buttons ────────────────────────────────────────────────────── */
QPushButton {{
    background-color: {C_WIDGET};
    color: {C_TEXT};
    border: 1px solid {C_BORDER};
    border-radius: 3px;
    padding: 6px 14px;
    font-size: 11px;
    font-weight: 600;
    min-height: 28px;
}}
QPushButton:hover {{
    background-color: {C_INPUT};
    border-color: {C_BORDER_LT};
    color: {C_TEXT_HDR};
}}
QPushButton:pressed {{
    background-color: {C_ACCENT};
    border-color: {C_ACCENT};
    color: white;
}}
QPushButton:checked {{
    background-color: {C_ACCENT};
    border-color: {C_ACCENT};
    color: white;
}}
QPushButton:disabled {{
    color: {C_TEXT_DIM};
    border-color: {C_BORDER};
    background-color: {C_PANEL};
}}

/* Specific named buttons */
QPushButton#btn_go {{
    background-color: #1a3a1a;
    border: 1px solid #2a5a2a;
    color: #60d060;
    font-weight: 700;
    font-size: 12px;
    min-height: 36px;
}}
QPushButton#btn_go:hover {{
    background-color: #1e4e1e;
    color: #80e080;
}}
QPushButton#btn_go:pressed {{
    background-color: {C_GREEN};
    color: white;
}}
QPushButton#btn_stop {{
    background-color: #3a1a1a;
    border: 1px solid #5a2a2a;
    color: #d06060;
    font-weight: 700;
    font-size: 12px;
    min-height: 36px;
}}
QPushButton#btn_stop:hover {{
    background-color: #4e1e1e;
    color: #e08080;
}}
QPushButton#btn_stop:pressed {{
    background-color: {C_RED};
    color: white;
}}
QPushButton#btn_flash {{
    background-color: #2a2a0a;
    border: 1px solid #4a4a1a;
    color: #c0c040;
    font-weight: 700;
    min-height: 28px;
}}
QPushButton#btn_flash:pressed {{
    background-color: #c0c020;
    color: #0c0c0c;
}}
QPushButton#btn_blackout {{
    background-color: #200808;
    border: 1px solid {C_RED};
    color: {C_RED};
    font-weight: 800;
    font-size: 12px;
    letter-spacing: 1px;
    min-height: 44px;
    min-width: 120px;
    border-radius: 3px;
}}
QPushButton#btn_blackout:checked {{
    background-color: {C_RED};
    color: white;
}}
QPushButton#btn_blackout:hover:!checked {{
    background-color: #380c0c;
}}

/* ── Tab / mode bar ──────────────────────────────────────────────────── */
QTabWidget::pane {{
    border: none;
    background: {C_BG};
}}
QTabBar::tab {{
    background: {C_PANEL};
    color: {C_TEXT_DIM};
    padding: 10px 20px;
    border: none;
    border-bottom: 2px solid transparent;
    font-weight: 700;
    font-size: 11px;
    min-width: 80px;
}}
QTabBar::tab:selected {{
    background: {C_PANEL2};
    color: {C_TEXT_HDR};
    border-bottom: 2px solid {C_ACCENT};
}}
QTabBar::tab:hover:!selected {{
    color: {C_TEXT};
    background: {C_PANEL2};
}}

/* ── Tree / List / Table ─────────────────────────────────────────────── */
QTreeWidget, QListWidget, QTableWidget {{
    background-color: {C_PANEL};
    alternate-background-color: {C_PANEL2};
    border: 1px solid {C_BORDER};
    border-radius: 2px;
    gridline-color: {C_BORDER};
    show-decoration-selected: 1;
}}
QTreeWidget::item, QListWidget::item, QTableWidget::item {{
    padding: 4px 6px;
    border: none;
    min-height: 28px;
}}
QTreeWidget::item:selected, QListWidget::item:selected, QTableWidget::item:selected {{
    background-color: {C_ACCENT};
    color: white;
}}
QTreeWidget::item:hover, QListWidget::item:hover {{
    background-color: {C_INPUT};
}}
QHeaderView::section {{
    background-color: {C_PANEL};
    color: {C_TEXT_LABEL};
    padding: 6px 8px;
    border: none;
    border-right: 1px solid {C_BORDER};
    border-bottom: 1px solid {C_BORDER};
    font-weight: 700;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    min-height: 28px;
}}
QHeaderView {{
    background-color: {C_PANEL};
}}

/* ── Combo / Input ───────────────────────────────────────────────────── */
QComboBox {{
    background-color: {C_WIDGET};
    border: 1px solid {C_BORDER};
    border-radius: 3px;
    padding: 5px 8px;
    min-height: 28px;
    color: {C_TEXT};
}}
QComboBox:hover {{ border-color: {C_BORDER_LT}; }}
QComboBox:focus {{ border-color: {C_ACCENT}; }}
QComboBox::drop-down {{ border: none; width: 20px; }}
QComboBox QAbstractItemView {{
    background: {C_PANEL};
    border: 1px solid {C_BORDER};
    selection-background-color: {C_ACCENT};
    min-height: 28px;
}}
QLineEdit, QSpinBox, QDoubleSpinBox {{
    background-color: {C_WIDGET};
    border: 1px solid {C_BORDER};
    border-radius: 3px;
    padding: 5px 8px;
    color: {C_TEXT};
    min-height: 28px;
}}
QLineEdit:focus, QSpinBox:focus {{ border-color: {C_ACCENT}; }}
QSpinBox::up-button, QSpinBox::down-button {{
    background: {C_INPUT};
    border: none;
    width: 18px;
    min-height: 14px;
}}
QSpinBox::up-button:hover, QSpinBox::down-button:hover {{
    background: {C_ACCENT};
}}

/* ── Slider ──────────────────────────────────────────────────────────── */
QSlider::groove:horizontal {{
    height: 4px;
    background: {C_INPUT};
    border-radius: 2px;
}}
QSlider::handle:horizontal {{
    background: {C_ACCENT};
    width: 18px;
    height: 18px;
    margin: -7px 0;
    border-radius: 9px;
}}
QSlider::sub-page:horizontal {{
    background: {C_ACCENT};
    border-radius: 2px;
}}

/* ── Group box ───────────────────────────────────────────────────────── */
QGroupBox {{
    border: 1px solid {C_BORDER};
    border-radius: 4px;
    margin-top: 18px;
    padding: 8px 6px 6px 6px;
    font-weight: 700;
    font-size: 10px;
    color: {C_TEXT_LABEL};
    text-transform: uppercase;
    letter-spacing: 1px;
}}
QGroupBox::title {{
    subcontrol-origin: margin;
    subcontrol-position: top left;
    padding: 0 6px;
    left: 8px;
    color: {C_TEXT_LABEL};
}}

/* ── Tooltip ─────────────────────────────────────────────────────────── */
QToolTip {{
    background-color: {C_PANEL2};
    color: {C_TEXT};
    border: 1px solid {C_BORDER_LT};
    padding: 4px 8px;
    border-radius: 3px;
    font-size: 10px;
}}

/* ── Dialog ──────────────────────────────────────────────────────────── */
QDialog {{
    background-color: {C_PANEL};
}}
QDialogButtonBox QPushButton {{
    min-width: 80px;
}}

/* ── Message box ─────────────────────────────────────────────────────── */
QMessageBox {{
    background-color: {C_PANEL};
}}

/* ── Status bar ──────────────────────────────────────────────────────── */
QStatusBar {{
    background-color: {C_PANEL};
    border-top: 1px solid {C_BORDER};
    color: {C_TEXT_DIM};
    font-size: 10px;
    min-height: 22px;
}}
QStatusBar::item {{ border: none; }}
QLabel#lbl_uni {{
    color: {C_GREEN};
    font-weight: 700;
    padding: 0 8px;
    border-left: 1px solid {C_BORDER};
}}
QLabel#lbl_gm {{
    color: {C_AMBER};
    font-weight: 700;
    padding: 0 8px;
    border-left: 1px solid {C_BORDER};
}}
QLabel#lbl_artnet {{
    color: {C_ACCENT};
    font-weight: 700;
    padding: 0 8px;
    border-left: 1px solid {C_BORDER};
}}

/* ── Menu ────────────────────────────────────────────────────────────── */
QMenuBar {{
    background: {C_PANEL};
    border-bottom: 1px solid {C_BORDER};
    padding: 2px;
}}
QMenuBar::item {{ padding: 4px 10px; border-radius: 3px; }}
QMenuBar::item:selected {{ background: {C_INPUT}; color: {C_TEXT_HDR}; }}
QMenu {{
    background: {C_PANEL2};
    border: 1px solid {C_BORDER_LT};
    padding: 2px;
}}
QMenu::item {{ padding: 6px 24px 6px 12px; border-radius: 2px; min-height: 28px; }}
QMenu::item:selected {{ background: {C_ACCENT}; color: white; }}
QMenu::separator {{ height: 1px; background: {C_BORDER}; margin: 3px 0; }}
"""


def apply_theme(app: QApplication) -> None:
    app.setStyle("Fusion")
    app.setStyleSheet(QSS)
    pal = QPalette()
    pal.setColor(QPalette.ColorRole.Window,          QColor(C_BG))
    pal.setColor(QPalette.ColorRole.WindowText,      QColor(C_TEXT))
    pal.setColor(QPalette.ColorRole.Base,            QColor(C_PANEL))
    pal.setColor(QPalette.ColorRole.AlternateBase,   QColor(C_PANEL2))
    pal.setColor(QPalette.ColorRole.ToolTipBase,     QColor(C_PANEL2))
    pal.setColor(QPalette.ColorRole.ToolTipText,     QColor(C_TEXT))
    pal.setColor(QPalette.ColorRole.Text,            QColor(C_TEXT))
    pal.setColor(QPalette.ColorRole.BrightText,      QColor(C_TEXT_HDR))
    pal.setColor(QPalette.ColorRole.Button,          QColor(C_WIDGET))
    pal.setColor(QPalette.ColorRole.ButtonText,      QColor(C_TEXT))
    pal.setColor(QPalette.ColorRole.Highlight,       QColor(C_ACCENT))
    pal.setColor(QPalette.ColorRole.HighlightedText, QColor(C_TEXT_HDR))
    pal.setColor(QPalette.ColorRole.Link,            QColor(C_ACCENT))
    pal.setColor(QPalette.ColorRole.Mid,             QColor(C_BORDER))
    pal.setColor(QPalette.ColorRole.Dark,            QColor(C_PANEL))
    app.setPalette(pal)
