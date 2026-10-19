"""Lumos — native PySide6 desktop application entry point."""
from __future__ import annotations

import sys
from pathlib import Path

# Ensure project root (lumos/) is on sys.path so 'backend' and 'app' resolve
_ROOT = Path(__file__).parent.parent
if str(_ROOT) not in sys.path:
    sys.path.insert(0, str(_ROOT))

from PySide6.QtWidgets import QApplication
from PySide6.QtCore import Qt


def main() -> None:
    app = QApplication(sys.argv)
    app.setApplicationName("Lumos")
    app.setApplicationDisplayName("Lumos — Theatrical Lighting")
    app.setOrganizationName("Lumos")

    from app.ui.theme import apply_theme
    apply_theme(app)

    from app.engine import LumosEngine
    from app.ui.main_window import MainWindow

    engine = LumosEngine()
    window = MainWindow(engine)
    window.show()
    engine.start()

    ret = app.exec()
    engine.stop()
    sys.exit(ret)


if __name__ == "__main__":
    main()
