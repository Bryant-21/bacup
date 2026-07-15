"""Recordings panel — auto-saved recording history (last 10)."""
from __future__ import annotations

import logging
import os
import time
from pathlib import Path

import numpy as np
from imgui_bundle import imgui

from app.paths import get_app_root as _get_app_root

_log = logging.getLogger("toolkit.voice_changer.recordings")
_NS = "##voice_changer"

_RECORDINGS_DIR = _get_app_root() / "output"
_MAX_RECORDINGS = 10


class RecordingsPanel:
    """Right dock panel showing auto-saved recordings with load controls."""

    def __init__(self, app):
        self._app = app
        self.window_name = f"Recordings{_NS}"
        self._recordings: list[dict] = []
        self._refresh()

    def _refresh(self):
        """Scan the recordings directory and build the list (newest first)."""
        self._recordings.clear()
        if not _RECORDINGS_DIR.is_dir():
            return
        wavs = sorted(_RECORDINGS_DIR.glob("*.wav"), key=lambda p: p.stat().st_mtime, reverse=True)
        for p in wavs[:_MAX_RECORDINGS]:
            stat = p.stat()
            self._recordings.append({
                "path": str(p),
                "name": p.stem,
                "size_kb": stat.st_size / 1024,
                "mtime": stat.st_mtime,
            })

    @staticmethod
    def auto_save(audio: np.ndarray, sample_rate: int, prefix: str = "rec") -> str | None:
        """Save audio to the recordings directory, pruning old files beyond limit.

        Returns the saved file path, or None on failure.
        """
        try:
            import soundfile as sf
            _RECORDINGS_DIR.mkdir(parents=True, exist_ok=True)

            timestamp = time.strftime("%Y%m%d_%H%M%S")
            filename = f"{prefix}_{timestamp}.wav"
            path = _RECORDINGS_DIR / filename
            sf.write(str(path), audio, sample_rate)

            # Prune oldest beyond limit
            wavs = sorted(_RECORDINGS_DIR.glob("*.wav"), key=lambda p: p.stat().st_mtime)
            while len(wavs) > _MAX_RECORDINGS:
                oldest = wavs.pop(0)
                try:
                    oldest.unlink()
                except OSError:
                    pass

            return str(path)
        except Exception as e:
            _log.error("Auto-save failed: %s", e)
            return None

    def draw(self):
        imgui.begin(self.window_name)

        imgui.text("Recent Recordings")
        imgui.same_line()
        if imgui.small_button("Refresh"):
            self._refresh()
        imgui.separator()

        if not self._recordings:
            imgui.text_disabled("No recordings yet")
            imgui.text_disabled("Record or process audio —")
            imgui.text_disabled("it auto-saves here.")
        else:
            for i, rec in enumerate(self._recordings):
                # Time label
                age = time.time() - rec["mtime"]
                if age < 60:
                    time_str = f"{int(age)}s ago"
                elif age < 3600:
                    time_str = f"{int(age / 60)}m ago"
                elif age < 86400:
                    time_str = f"{int(age / 3600)}h ago"
                else:
                    time_str = time.strftime("%m/%d %H:%M", time.localtime(rec["mtime"]))

                # Entry
                imgui.push_id(i)
                if imgui.button("Load"):
                    self._load_recording(rec["path"])
                imgui.same_line()
                if imgui.small_button("X"):
                    self._delete_recording(rec["path"])
                imgui.same_line()
                imgui.text(rec["name"])
                imgui.same_line()
                imgui.text_disabled(f"({rec['size_kb']:.0f}KB, {time_str})")
                imgui.pop_id()

        imgui.end()

    def _delete_recording(self, path: str):
        """Delete a recording file and refresh the list."""
        try:
            Path(path).unlink()
            self._app.log(f"Deleted recording: {os.path.basename(path)}")
        except OSError as e:
            _log.error("Delete failed: %s", e)
        self._refresh()

    def _load_recording(self, path: str):
        """Load a recording into the recorder panel."""
        if self._app.recorder_panel:
            self._app.recorder_panel._load_audio(path)
            self._app.log(f"Loaded recording: {os.path.basename(path)}")
