"""
Show panel — save/load the full show to/from a JSON file.
"""
from __future__ import annotations
import json
import datetime
from typing import Optional

from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QPushButton,
    QLabel, QFrame, QFileDialog, QMessageBox, QSizePolicy,
)
from PySide6.QtCore import Qt, Slot

from app.engine import LumosEngine, CueList, Cue
from app.ui import theme as T

_SHOW_VERSION = 1


def _make_hline() -> QFrame:
    f = QFrame()
    f.setFrameShape(QFrame.Shape.HLine)
    f.setStyleSheet(f"color: {T.C_BORDER};")
    return f


class ShowPanel(QWidget):
    """Save and load show files (JSON)."""

    def __init__(self, engine: LumosEngine, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.engine = engine
        self._patch_fixtures: list[dict] = []  # set by main_window via set_fixtures()
        self._build()

    # ── Build ─────────────────────────────────────────────────────────────────

    def _build(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(6)

        # Header
        hdr = QLabel("SHOW FILE")
        hdr.setStyleSheet(
            f"color: {T.C_TEXT_LABEL}; font-size: 10px; font-weight: 800;"
            f" letter-spacing: 2px;"
        )
        root.addWidget(hdr)
        root.addWidget(_make_hline())

        # Info card
        info = QWidget()
        info.setStyleSheet(
            f"background: {T.C_PANEL}; border: 1px solid {T.C_BORDER}; border-radius: 4px;"
        )
        info_lay = QVBoxLayout(info)
        info_lay.setContentsMargins(12, 10, 12, 10)
        info_lay.setSpacing(4)

        self._lbl_path = QLabel("No show loaded")
        self._lbl_path.setWordWrap(True)
        self._lbl_path.setStyleSheet(f"color: {T.C_TEXT}; font-size: 11px;")
        info_lay.addWidget(self._lbl_path)

        self._lbl_time = QLabel("")
        self._lbl_time.setStyleSheet(f"color: {T.C_TEXT_DIM}; font-size: 10px;")
        info_lay.addWidget(self._lbl_time)

        root.addWidget(info)
        root.addWidget(_make_hline())

        # Save / Load buttons
        btn_row = QHBoxLayout()
        btn_row.setSpacing(10)

        btn_save = QPushButton("💾  Save Show")
        btn_save.setFixedHeight(44)
        btn_save.setStyleSheet(
            f"background: #0a1a0a; border: 1px solid {T.C_GREEN}; color: {T.C_GREEN};"
            f" font-weight: 800; font-size: 13px; border-radius: 3px;"
        )
        btn_save.clicked.connect(self._save_show)
        btn_row.addWidget(btn_save)

        btn_load = QPushButton("📂  Load Show")
        btn_load.setFixedHeight(44)
        btn_load.setStyleSheet(
            f"background: #0a0a1a; border: 1px solid {T.C_ACCENT}; color: {T.C_ACCENT};"
            f" font-weight: 800; font-size: 13px; border-radius: 3px;"
        )
        btn_load.clicked.connect(self._load_show)
        btn_row.addWidget(btn_load)

        root.addLayout(btn_row)

        # Summary
        root.addWidget(_make_hline())
        self._lbl_summary = QLabel(self._build_summary())
        self._lbl_summary.setStyleSheet(
            f"color: {T.C_TEXT_DIM}; font-size: 10px;"
            f" background: {T.C_PANEL}; border: 1px solid {T.C_BORDER};"
            f" border-radius: 3px; padding: 8px;"
        )
        self._lbl_summary.setAlignment(Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignLeft)
        root.addWidget(self._lbl_summary)

        root.addStretch()

    # ── Public API ────────────────────────────────────────────────────────────

    def set_fixtures(self, fixtures: list[dict]) -> None:
        """Called when patch changes so we always save current fixture data."""
        self._patch_fixtures = fixtures
        self._lbl_summary.setText(self._build_summary())

    # ── Internal ──────────────────────────────────────────────────────────────

    def _build_summary(self) -> str:
        e = self.engine
        return (
            f"Universes: {len(e.universes)}\n"
            f"Fixtures:  {len(self._patch_fixtures)}\n"
            f"Scenes:    {len(e.scenes)}\n"
            f"Cuelists:  {len(e.cuelists)}   "
            f"({sum(len(cl.cues) for cl in e.cuelists.values())} cues total)"
        )

    def _show_to_dict(self) -> dict:
        e = self.engine
        return {
            "version": _SHOW_VERSION,
            "universes": [
                {"id": u.id, "name": u.name}
                for u in e.universes.values()
            ],
            "patch": self._patch_fixtures,
            "scenes": {
                name: list(data)
                for name, data in e.scenes.items()
            },
            "cuelists": [
                {
                    "id": cl.id,
                    "name": cl.name,
                    "cues": [
                        {
                            "number": c.number,
                            "name": c.name,
                            "universe": c.universe,
                            "data": list(c.data),
                            "fade_in": c.fade_in,
                            "fade_out": c.fade_out,
                            "follow_wait": c.follow_wait,
                        }
                        for c in cl.cues
                    ],
                }
                for cl in e.cuelists.values()
            ],
        }

    def _dict_to_show(self, d: dict) -> None:
        e = self.engine
        version = d.get("version", 1)

        # Universes
        for u_data in d.get("universes", []):
            uid = u_data["id"]
            if uid not in e.universes:
                e.add_universe(uid, u_data.get("name", f"Universe {uid}"))
            else:
                e.universes[uid].name = u_data.get("name", e.universes[uid].name)

        # Scenes
        e.scenes.clear()
        for name, data in d.get("scenes", {}).items():
            e.scenes[name] = bytearray(data)

        # Cuelists
        e.cuelists.clear()
        max_id = 0
        for cl_data in d.get("cuelists", []):
            cl = CueList(
                id=cl_data["id"],
                name=cl_data["name"],
            )
            for c_data in cl_data.get("cues", []):
                cue = Cue(
                    number=float(c_data["number"]),
                    name=c_data["name"],
                    universe=int(c_data["universe"]),
                    data=bytearray(c_data["data"]),
                    fade_in=float(c_data.get("fade_in", 3.0)),
                    fade_out=float(c_data.get("fade_out", 3.0)),
                    follow_wait=float(c_data.get("follow_wait", 0.0)),
                )
                cl.cues.append(cue)
            e.cuelists[cl.id] = cl
            max_id = max(max_id, cl.id)
        e._next_cuelist_id = max_id + 1

        # Patch fixtures (just store for UI layer to pick up)
        self._patch_fixtures = d.get("patch", [])

    # ── Slots ─────────────────────────────────────────────────────────────────

    @Slot()
    def _save_show(self) -> None:
        path, _ = QFileDialog.getSaveFileName(
            self, "Save Show", "", "Lumos Show (*.lumos);;JSON (*.json);;All files (*)"
        )
        if not path:
            return
        if not (path.endswith(".lumos") or path.endswith(".json")):
            path += ".lumos"
        try:
            data = self._show_to_dict()
            with open(path, "w", encoding="utf-8") as f:
                json.dump(data, f, indent=2)
            now = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
            self._lbl_path.setText(path)
            self._lbl_time.setText(f"Saved at {now}")
            self._lbl_summary.setText(self._build_summary())
        except Exception as exc:
            QMessageBox.critical(self, "Save failed", str(exc))

    @Slot()
    def _load_show(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "Load Show", "", "Lumos Show (*.lumos);;JSON (*.json);;All files (*)"
        )
        if not path:
            return
        try:
            with open(path, "r", encoding="utf-8") as f:
                data = json.load(f)
            self._dict_to_show(data)
            now = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
            self._lbl_path.setText(path)
            self._lbl_time.setText(f"Loaded at {now}")
            self._lbl_summary.setText(self._build_summary())
            # Notify all cuelists changed
            for cl_id in self.engine.cuelists:
                self.engine.cuelist_changed.emit(cl_id)
        except Exception as exc:
            QMessageBox.critical(self, "Load failed", str(exc))
