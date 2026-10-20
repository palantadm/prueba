"""
NDI receiver: receives one NDI source and exposes its frames as an OpenGL texture.
Requires ndi-python: pip install ndi-python
Falls back gracefully if not available.
"""
from __future__ import annotations

from typing import Optional

from PySide6.QtCore import QObject, QThread, QTimer, Signal

try:
    import NDIlib as ndi
    _HAS_NDI = True
except ImportError:
    _HAS_NDI = False

try:
    import OpenGL.GL as GL
    _HAS_OPENGL = True
except ImportError:
    _HAS_OPENGL = False


class _RecvWorker(QThread):
    """Background thread that pulls frames from an NDI source."""

    frame_ready = Signal(bytes, int, int)   # data, width, height

    def __init__(self, source_name: str, parent=None) -> None:
        super().__init__(parent)
        self._source_name = source_name
        self._running = False

    def stop(self) -> None:
        self._running = False
        self.wait(3000)

    def run(self) -> None:
        if not _HAS_NDI:
            return
        ndi.initialize()

        # Find the named source
        finder = ndi.find_create_v2()
        if not finder:
            return

        import time
        deadline = time.monotonic() + 3.0
        recv_source = None
        while time.monotonic() < deadline and recv_source is None:
            ndi.find_wait_for_sources(finder, 200)
            sources = ndi.find_get_current_sources(finder)
            for s in (sources or []):
                if s.ndi_name == self._source_name:
                    recv_source = s
                    break
        ndi.find_destroy(finder)

        if recv_source is None:
            return

        settings = ndi.RecvCreateV3()
        settings.source_to_connect_to = recv_source
        settings.color_format = ndi.RECV_COLOR_FORMAT_RGBX_RGBA
        settings.bandwidth = ndi.RECV_BANDWIDTH_HIGHEST

        receiver = ndi.recv_create_v3(settings)
        if not receiver:
            return

        self._running = True
        while self._running:
            frame_type, video, audio, meta = ndi.recv_capture_v2(receiver, 100)
            if frame_type == ndi.FRAME_TYPE_VIDEO and video is not None:
                w, h = video.xres, video.yres
                data_bytes = bytes(video.data)
                self.frame_ready.emit(data_bytes, w, h)
                ndi.recv_free_video_v2(receiver, video)

        ndi.recv_destroy(receiver)
        ndi.destroy()


class NDIReceiver(QObject):
    """Receives one NDI source and makes its frames available as an OpenGL texture."""

    frame_ready = Signal()

    def __init__(self, source_name: str, parent=None) -> None:
        super().__init__(parent)
        self._source_name = source_name
        self._worker: Optional[_RecvWorker] = None
        self._latest_frame: Optional[bytes] = None
        self._frame_w: int = 0
        self._frame_h: int = 0
        self._texture_id: int = 0
        self._texture_dirty: bool = False

    # ------------------------------------------------------------------
    # Public interface
    # ------------------------------------------------------------------

    def start(self) -> bool:
        if not _HAS_NDI:
            return False
        self._worker = _RecvWorker(self._source_name, self)
        self._worker.frame_ready.connect(self._on_frame)
        self._worker.start()
        return True

    def stop(self) -> None:
        if self._worker:
            self._worker.stop()
            self._worker = None
        if _HAS_OPENGL and self._texture_id:
            try:
                GL.glDeleteTextures([self._texture_id])
            except Exception:
                pass
            self._texture_id = 0

    def get_frame_rgb(self) -> Optional[bytes]:
        return self._latest_frame

    def get_texture_id(self) -> int:
        """Return an OpenGL texture id, uploading the latest frame if needed."""
        if not _HAS_OPENGL:
            return 0
        if self._texture_dirty and self._latest_frame:
            self._upload_texture()
            self._texture_dirty = False
        return self._texture_id

    # ------------------------------------------------------------------
    # Internal
    # ------------------------------------------------------------------

    def _on_frame(self, data: bytes, w: int, h: int) -> None:
        self._latest_frame = data
        self._frame_w = w
        self._frame_h = h
        self._texture_dirty = True
        self.frame_ready.emit()

    def _upload_texture(self) -> None:
        if not self._latest_frame:
            return
        try:
            if not self._texture_id:
                tex = GL.glGenTextures(1)
                self._texture_id = int(tex) if not hasattr(tex, "__len__") else int(tex[0])

            GL.glBindTexture(GL.GL_TEXTURE_2D, self._texture_id)
            GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_MIN_FILTER, GL.GL_LINEAR)
            GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_MAG_FILTER, GL.GL_LINEAR)
            GL.glTexImage2D(
                GL.GL_TEXTURE_2D, 0, GL.GL_RGBA,
                self._frame_w, self._frame_h, 0,
                GL.GL_RGBA, GL.GL_UNSIGNED_BYTE,
                self._latest_frame,
            )
            GL.glBindTexture(GL.GL_TEXTURE_2D, 0)
        except Exception:
            pass
