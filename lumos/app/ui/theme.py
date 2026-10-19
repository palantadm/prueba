"""
Lumos dark theme — professional theatrical look.
Colors reference:  Daslight / grandMA2 dark UI palette.
"""
from PySide6.QtWidgets import QApplication
from PySide6.QtGui import QPalette, QColor
from PySide6.QtCore import Qt

# ── Color tokens ──────────────────────────────────────────────────────────────
C_BG_DARK   = "#141414"
C_BG        = "#1e1e1e"
C_BG_PANEL  = "#252526"
C_BG_WIDGET = "#2d2d30"
C_BG_INPUT  = "#3c3c3c"
C_BORDER    = "#464647"
C_BORDER_LT = "#585858"

C_TEXT      = "#d4d4d4"
C_TEXT_DIM  = "#808080"
C_TEXT_HDR  = "#ffffff"

C_ACCENT    = "#0078d4"   # blue  — selection, focus
C_GREEN     = "#4caf50"   # green — live / go
C_RED       = "#f44336"   # red   — stop / blackout
C_AMBER     = "#ffa726"   # amber — warning / highlight
C_PURPLE    = "#9c6fcd"   # purple — effects / programmed

# Fader colours (value-dependent gradient in code)
C_FADER_ZERO   = "#1a2a3a"
C_FADER_LOW    = "#1a4a7a"
C_FADER_MID    = "#1565c0"
C_FADER_HIGH   = "#42a5f5"
C_FADER_FULL   = "#ffa726"   # warm amber at 100% (matches tungsten look)
C_FADER_TRACK  = "#111111"
C_FADER_HANDLE = "#37474f"


QSS = f"""
/* ── Global ──────────────────────────────────────────────────────────── */
QWidget {{
    background-color: {C_BG};
    color: {C_TEXT};
    font-family: "Segoe UI", "SF Pro", Arial, sans-serif;
    font-size: 11px;
    selection-background-color: {C_ACCENT};
    selection-color: {C_TEXT_HDR};
}}

/* ── Main window ─────────────────────────────────────────────────────── */
QMainWindow {{
    background-color: {C_BG_DARK};
}}
QMainWindow::separator {{
    background: {C_BORDER};
    width: 3px;
    height: 3px;
}}

/* ── Menu bar ────────────────────────────────────────────────────────── */
QMenuBar {{
    background-color: {C_BG_PANEL};
    border-bottom: 1px solid {C_BORDER};
    padding: 2px;
}}
QMenuBar::item {{
    padding: 4px 8px;
    border-radius: 3px;
}}
QMenuBar::item:selected {{
    background-color: {C_BG_INPUT};
}}
QMenu {{
    background-color: {C_BG_PANEL};
    border: 1px solid {C_BORDER};
    padding: 2px;
}}
QMenu::item {{
    padding: 5px 24px 5px 12px;
    border-radius: 3px;
}}
QMenu::item:selected {{
    background-color: {C_ACCENT};
    color: {C_TEXT_HDR};
}}
QMenu::separator {{
    height: 1px;
    background: {C_BORDER};
    margin: 3px 0;
}}

/* ── Toolbar ─────────────────────────────────────────────────────────── */
QToolBar {{
    background-color: {C_BG_PANEL};
    border-bottom: 1px solid {C_BORDER};
    spacing: 4px;
    padding: 4px 8px;
}}
QToolBar::separator {{
    background: {C_BORDER};
    width: 1px;
    margin: 4px 4px;
}}
QToolButton {{
    background-color: {C_BG_WIDGET};
    border: 1px solid {C_BORDER};
    border-radius: 4px;
    padding: 5px 10px;
    font-weight: 600;
}}
QToolButton:hover {{
    background-color: {C_BG_INPUT};
    border-color: {C_BORDER_LT};
}}
QToolButton:pressed {{
    background-color: {C_ACCENT};
    border-color: {C_ACCENT};
    color: white;
}}
QToolButton:checked {{
    background-color: {C_RED};
    border-color: {C_RED};
    color: white;
}}

/* ── Dock widgets ────────────────────────────────────────────────────── */
QDockWidget {{
    titlebar-close-icon: url(none);
    font-weight: 700;
    font-size: 11px;
    color: {C_TEXT_DIM};
}}
QDockWidget::title {{
    background-color: {C_BG_PANEL};
    padding: 5px 8px;
    border-bottom: 1px solid {C_BORDER};
    text-align: left;
}}
QDockWidget::close-button, QDockWidget::float-button {{
    border: none;
    background: transparent;
    padding: 2px;
}}

/* ── Tab widget ──────────────────────────────────────────────────────── */
QTabWidget::pane {{
    border: none;
    background-color: {C_BG};
}}
QTabBar {{
    background-color: {C_BG_PANEL};
}}
QTabBar::tab {{
    background-color: {C_BG_PANEL};
    color: {C_TEXT_DIM};
    padding: 7px 16px;
    border-right: 1px solid {C_BORDER};
    border-bottom: 2px solid transparent;
    font-size: 11px;
    font-weight: 600;
    min-width: 80px;
}}
QTabBar::tab:selected {{
    color: {C_TEXT_HDR};
    border-bottom: 2px solid {C_ACCENT};
    background-color: {C_BG};
}}
QTabBar::tab:hover:!selected {{
    color: {C_TEXT};
    background-color: {C_BG_WIDGET};
}}

/* ── Push buttons ────────────────────────────────────────────────────── */
QPushButton {{
    background-color: {C_BG_WIDGET};
    border: 1px solid {C_BORDER};
    border-radius: 4px;
    padding: 5px 14px;
    font-weight: 600;
    min-height: 24px;
}}
QPushButton:hover {{
    background-color: {C_BG_INPUT};
    border-color: {C_BORDER_LT};
}}
QPushButton:pressed {{
    background-color: {C_ACCENT};
    border-color: {C_ACCENT};
    color: white;
}}
QPushButton:disabled {{
    color: {C_TEXT_DIM};
    border-color: {C_BORDER};
}}
QPushButton#btn_go {{
    background-color: #1b5e20;
    border-color: #2e7d32;
    color: #a5d6a7;
    font-size: 13px;
    font-weight: 700;
    min-width: 60px;
}}
QPushButton#btn_go:hover {{
    background-color: #2e7d32;
    color: white;
}}
QPushButton#btn_stop {{
    background-color: #b71c1c;
    border-color: #c62828;
    color: #ffcdd2;
    font-size: 13px;
    font-weight: 700;
}}
QPushButton#btn_stop:hover {{
    background-color: #c62828;
    color: white;
}}
QPushButton#btn_blackout {{
    background-color: #212121;
    border: 2px solid {C_RED};
    color: {C_RED};
    font-weight: 700;
    letter-spacing: 1px;
}}
QPushButton#btn_blackout:checked {{
    background-color: {C_RED};
    color: white;
}}
QPushButton#btn_blackout:hover {{
    background-color: #3a0000;
}}

/* ── Sliders ─────────────────────────────────────────────────────────── */
QSlider::groove:horizontal {{
    height: 4px;
    background: {C_BG_INPUT};
    border-radius: 2px;
}}
QSlider::handle:horizontal {{
    background: {C_ACCENT};
    width: 16px;
    height: 16px;
    margin: -6px 0;
    border-radius: 8px;
}}
QSlider::sub-page:horizontal {{
    background: {C_ACCENT};
    border-radius: 2px;
}}

/* ── Scroll bars ─────────────────────────────────────────────────────── */
QScrollBar:vertical {{
    background: {C_BG_DARK};
    width: 8px;
    border: none;
}}
QScrollBar::handle:vertical {{
    background: {C_BORDER};
    border-radius: 4px;
    min-height: 20px;
}}
QScrollBar::handle:vertical:hover {{
    background: {C_BORDER_LT};
}}
QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{
    height: 0;
}}
QScrollBar:horizontal {{
    background: {C_BG_DARK};
    height: 8px;
    border: none;
}}
QScrollBar::handle:horizontal {{
    background: {C_BORDER};
    border-radius: 4px;
    min-width: 20px;
}}
QScrollBar::handle:horizontal:hover {{
    background: {C_BORDER_LT};
}}
QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {{
    width: 0;
}}

/* ── Tree / List views ───────────────────────────────────────────────── */
QTreeWidget, QListWidget, QTableWidget {{
    background-color: {C_BG_WIDGET};
    alternate-background-color: {C_BG_PANEL};
    border: 1px solid {C_BORDER};
    border-radius: 4px;
    gridline-color: {C_BORDER};
}}
QTreeWidget::item, QListWidget::item, QTableWidget::item {{
    padding: 3px 6px;
    border: none;
}}
QTreeWidget::item:selected, QListWidget::item:selected, QTableWidget::item:selected {{
    background-color: {C_ACCENT};
    color: white;
}}
QHeaderView::section {{
    background-color: {C_BG_PANEL};
    color: {C_TEXT_DIM};
    padding: 4px 8px;
    border: none;
    border-right: 1px solid {C_BORDER};
    border-bottom: 1px solid {C_BORDER};
    font-weight: 600;
    font-size: 10px;
    text-transform: uppercase;
}}

/* ── Combo box ───────────────────────────────────────────────────────── */
QComboBox {{
    background-color: {C_BG_WIDGET};
    border: 1px solid {C_BORDER};
    border-radius: 4px;
    padding: 4px 8px;
    min-height: 22px;
}}
QComboBox:hover {{
    border-color: {C_BORDER_LT};
}}
QComboBox::drop-down {{
    border: none;
    width: 20px;
}}
QComboBox QAbstractItemView {{
    background-color: {C_BG_PANEL};
    border: 1px solid {C_BORDER};
    selection-background-color: {C_ACCENT};
}}

/* ── Line edits ──────────────────────────────────────────────────────── */
QLineEdit, QSpinBox {{
    background-color: {C_BG_WIDGET};
    border: 1px solid {C_BORDER};
    border-radius: 4px;
    padding: 4px 8px;
    color: {C_TEXT};
}}
QLineEdit:focus, QSpinBox:focus {{
    border-color: {C_ACCENT};
}}
QSpinBox::up-button, QSpinBox::down-button {{
    background-color: {C_BG_INPUT};
    border: none;
    width: 16px;
}}

/* ── Group box ───────────────────────────────────────────────────────── */
QGroupBox {{
    border: 1px solid {C_BORDER};
    border-radius: 5px;
    margin-top: 16px;
    padding: 8px;
    font-weight: 700;
    font-size: 10px;
    color: {C_TEXT_DIM};
    text-transform: uppercase;
    letter-spacing: 1px;
}}
QGroupBox::title {{
    subcontrol-origin: margin;
    subcontrol-position: top left;
    padding: 0 6px;
    left: 10px;
}}

/* ── Status bar ──────────────────────────────────────────────────────── */
QStatusBar {{
    background-color: {C_BG_PANEL};
    border-top: 1px solid {C_BORDER};
    color: {C_TEXT_DIM};
    font-size: 10px;
}}
QStatusBar::item {{
    border: none;
}}
QLabel#status_universe {{
    color: {C_GREEN};
    font-weight: 700;
    padding: 2px 8px;
    border-left: 1px solid {C_BORDER};
}}
QLabel#status_gm {{
    color: {C_AMBER};
    font-weight: 700;
    padding: 2px 8px;
    border-left: 1px solid {C_BORDER};
}}
"""


def apply_theme(app: QApplication) -> None:
    app.setStyle("Fusion")
    app.setStyleSheet(QSS)
    pal = QPalette()
    pal.setColor(QPalette.ColorRole.Window,          QColor(C_BG))
    pal.setColor(QPalette.ColorRole.WindowText,      QColor(C_TEXT))
    pal.setColor(QPalette.ColorRole.Base,            QColor(C_BG_WIDGET))
    pal.setColor(QPalette.ColorRole.AlternateBase,   QColor(C_BG_PANEL))
    pal.setColor(QPalette.ColorRole.ToolTipBase,     QColor(C_BG_PANEL))
    pal.setColor(QPalette.ColorRole.ToolTipText,     QColor(C_TEXT))
    pal.setColor(QPalette.ColorRole.Text,            QColor(C_TEXT))
    pal.setColor(QPalette.ColorRole.Button,          QColor(C_BG_WIDGET))
    pal.setColor(QPalette.ColorRole.ButtonText,      QColor(C_TEXT))
    pal.setColor(QPalette.ColorRole.Highlight,       QColor(C_ACCENT))
    pal.setColor(QPalette.ColorRole.HighlightedText, QColor(C_TEXT_HDR))
    app.setPalette(pal)
