"""
Screen management — detect connected displays and open secondary windows.
"""
from __future__ import annotations
from typing import Optional

from PySide6.QtWidgets import QMainWindow, QWidget, QVBoxLayout, QApplication
from PySide6.QtCore import Qt, QRect
from PySide6.QtGui import QScreen

from app.ui import theme as T


class SecondaryWindow(QMainWindow):
    """Full-screen window on a secondary display holding any panel widget."""

    def __init__(self, screen: QScreen, title: str = "Lumos — Secondary",
                 parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setWindowFlag(Qt.WindowType.Window, True)
        self._screen = screen
        self._panel: Optional[QWidget] = None

        central = QWidget()
        self.setCentralWidget(central)
        self._layout = QVBoxLayout(central)
        self._layout.setContentsMargins(0, 0, 0, 0)
        self._layout.setSpacing(0)

        self.setGeometry(screen.geometry())
        self.setStyleSheet(f"background: {T.C_BG};")

    def set_panel(self, widget: QWidget) -> None:
        if self._panel is not None:
            self._layout.removeWidget(self._panel)
            self._panel.setParent(None)  # type: ignore[arg-type]
        self._panel = widget
        self._layout.addWidget(widget)

    def go_fullscreen(self) -> None:
        self.showFullScreen()

    def go_normal(self) -> None:
        self.showNormal()
        self.setGeometry(self._screen.geometry())


class ScreenManager:
    """Tracks available screens and manages secondary windows."""

    def __init__(self) -> None:
        self._windows: dict[int, SecondaryWindow] = {}

    def available_screens(self) -> list[QScreen]:
        return QApplication.screens()

    def secondary_screens(self) -> list[QScreen]:
        screens = self.available_screens()
        return screens[1:] if len(screens) > 1 else []

    def open_on_screen(self, screen_index: int, panel: QWidget,
                       title: str = "Lumos — Secondary") -> SecondaryWindow:
        screens = self.available_screens()
        if screen_index >= len(screens):
            screen_index = len(screens) - 1
        screen = screens[screen_index]

        if screen_index in self._windows:
            win = self._windows[screen_index]
        else:
            win = SecondaryWindow(screen, title)
            self._windows[screen_index] = win

        win.set_panel(panel)
        win.go_fullscreen()
        return win

    def close_all(self) -> None:
        for win in self._windows.values():
            win.close()
        self._windows.clear()
