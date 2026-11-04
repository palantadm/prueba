"""
NDI output: renders the visualizer to an offscreen buffer and broadcasts as NDI.
Requires ndi-python: pip install ndi-python
Falls back gracefully if not available.
"""
from __future__ import annotations

try:
    import NDIlib as ndi
    _HAS_NDI = True
except ImportError:
    _HAS_NDI = False

import numpy as np

from PySide6.QtCore import QObject, QTimer, Signal
from PySide6.QtGui import QImage
from PySide6.QtOpenGLWidgets import QOpenGLWidget


class NDIOutput(QObject):
    """Sends the GL widget's rendered frames over NDI."""

    status_changed = Signal(str)

    def __init__(
        self,
        source_name: str = "Lumos Visualizer",
        parent: QObject | None = None,
    ) -> None:
        super().__init__(parent)
        self._source_name = source_name
        self._send_instance = None
        self._active = False
        self._width = 1280
        self._height = 720
        self._fps = 30
        self._timer = QTimer(self)
        self._timer.timeout.connect(self._send_frame)
        self._gl_widget: QOpenGLWidget | None = None

    # ------------------------------------------------------------------
    # Properties
    # ------------------------------------------------------------------

    @property
    def available(self) -> bool:
        return _HAS_NDI

    @property
    def active(self) -> bool:
        return self._active

    @property
    def source_name(self) -> str:
        return self._source_name

    @source_name.setter
    def source_name(self, value: str) -> None:
        self._source_name = value

    def set_resolution(self, width: int, height: int) -> None:
        self._width = width
        self._height = height

    def set_fps(self, fps: int) -> None:
        self._fps = max(1, fps)
        if self._active:
            self._timer.setInterval(1000 // self._fps)

    def set_source(self, gl_widget: QOpenGLWidget) -> None:
        self._gl_widget = gl_widget

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    def start(self) -> bool:
        if not _HAS_NDI:
            self.status_changed.emit("NDI not available — run: pip install ndi-python")
            return False

        ndi.initialize()
        settings = ndi.SendCreate()
        settings.ndi_name = self._source_name

        self._send_instance = ndi.send_create(settings)
        if not self._send_instance:
            self.status_changed.emit("NDI: failed to create sender")
            return False

        self._active = True
        self._timer.start(1000 // self._fps)
        self.status_changed.emit(f"NDI active: '{self._source_name}'")
        return True

    def stop(self) -> None:
        self._timer.stop()
        if _HAS_NDI and self._send_instance:
            ndi.send_destroy(self._send_instance)
            ndi.destroy()
            self._send_instance = None
        self._active = False
        self.status_changed.emit("NDI stopped")

    # ------------------------------------------------------------------
    # Frame send
    # ------------------------------------------------------------------

    def _send_frame(self) -> None:
        if not self._gl_widget or not self._send_instance:
            return

        try:
            img: QImage = self._gl_widget.grabFramebuffer()
            img = img.scaled(self._width, self._height)
            img = img.convertToFormat(QImage.Format.Format_RGBA8888)

            bits = img.bits()
            data = np.frombuffer(bytes(bits), dtype=np.uint8).copy()
            data = data.reshape((self._height, self._width, 4))

            # NDI expects BGRX; QImage gives RGBA
            bgrx = np.zeros_like(data)
            bgrx[:, :, 0] = data[:, :, 2]   # B
            bgrx[:, :, 1] = data[:, :, 1]   # G
            bgrx[:, :, 2] = data[:, :, 0]   # R
            bgrx[:, :, 3] = 255

            frame = ndi.VideoFrameV2()
            frame.data = bgrx.flatten().tobytes()
            frame.FourCC = ndi.FOURCC_VIDEO_TYPE_BGRX
            frame.xres = self._width
            frame.yres = self._height
            frame.frame_rate_N = self._fps * 1000
            frame.frame_rate_D = 1000

            ndi.send_send_video_v2(self._send_instance, frame)
        except Exception:
            pass
