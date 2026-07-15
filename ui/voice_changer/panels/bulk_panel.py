"""Bulk processing panel — apply effects to a folder of audio files."""
from __future__ import annotations

import logging
import os
import threading
from typing import Optional

import numpy as np
from imgui_bundle import imgui

from creation_lib.ui.theme.window_chrome import AsyncWorker

_log = logging.getLogger("toolkit.voice_changer.bulk")
_NS = "##voice_changer"

_SUPPORTED_EXTS = {".wav", ".xwm", ".fuz"}


def _open_folder_dialog(title: str) -> str | None:
    try:
        from creation_lib.ui.widgets.pick_folder import pick_folder
        return pick_folder(title)
    except Exception:
        return None


class BulkPanel:
    """Batch process audio files with the active effect chain."""

    def __init__(self, app):
        self._app = app
        self.window_name = f"Bulk{_NS}"

        self._input_folder: str = ""
        self._output_folder: str = ""
        self._files: list[dict] = []  # [{name, path, ext, checked, duration}]
        self._processing: bool = False
        self._progress: int = 0
        self._progress_total: int = 0
        self._progress_file: str = ""
        self._worker: Optional[threading.Thread] = None

    def _scan_folder(self):
        """Scan input folder for supported audio files."""
        self._files = []
        if not os.path.isdir(self._input_folder):
            return

        for fname in sorted(os.listdir(self._input_folder)):
            ext = os.path.splitext(fname)[1].lower()
            if ext in _SUPPORTED_EXTS:
                path = os.path.join(self._input_folder, fname)
                duration = self._get_duration(path, ext)
                self._files.append({
                    "name": fname,
                    "path": path,
                    "ext": ext[1:].upper(),
                    "checked": True,
                    "duration": duration,
                })

        self._app.log(f"Found {len(self._files)} audio files in {self._input_folder}")

    def _get_duration(self, path: str, ext: str) -> float:
        """Get audio duration in seconds (WAV only, others show 0)."""
        if ext == ".wav":
            try:
                import soundfile as sf
                info = sf.info(path)
                return info.duration
            except Exception:
                pass
        return 0.0

    def _process_files(self):
        """Process all checked files (runs in background thread)."""
        import soundfile as sf
        from ui.voice_changer.engine import process_chain
        from ui.voice_changer.format_converter import detect_format, to_wav, from_wav

        checked = [f for f in self._files if f["checked"]]
        self._progress_total = len(checked)
        self._progress = 0

        os.makedirs(self._output_folder, exist_ok=True)

        for f in checked:
            self._progress_file = f["name"]
            try:
                fmt = detect_format(f["path"])

                if fmt != "wav":
                    import tempfile
                    tmp_dir = tempfile.mkdtemp()
                    wav_path = to_wav(f["path"], output_dir=tmp_dir)
                else:
                    wav_path = f["path"]

                data, sr = sf.read(wav_path, always_2d=False)
                if data.ndim > 1:
                    data = data[:, 0]
                data = data.astype(np.float32)

                result = process_chain(data, sample_rate=sr, chain=self._app.active_chain, normalize=True)

                base = os.path.splitext(f["name"])[0]
                out_wav = os.path.join(self._output_folder, f"{base}.wav")
                sf.write(out_wav, result, sr)

                if fmt != "wav":
                    from_wav(out_wav, fmt, output_dir=self._output_folder)
                    os.remove(out_wav)  # Clean up intermediate WAV

                self._app.log(f"Processed: {f['name']}")

                # Clean up temp files
                if fmt != "wav" and os.path.isdir(tmp_dir):
                    import shutil
                    shutil.rmtree(tmp_dir, ignore_errors=True)

            except Exception as e:
                self._app.log(f"FAILED: {f['name']} — {e}")
                _log.exception("Failed to process %s", f["name"])

            self._progress += 1

        self._processing = False
        self._app.log(f"Batch complete: {self._progress}/{self._progress_total} files")

    def draw(self):
        imgui.begin(self.window_name)

        # Input folder
        imgui.text("Input Folder:")
        imgui.set_next_item_width(-80)
        _, self._input_folder = imgui.input_text("##input_dir", self._input_folder, 1024)
        imgui.same_line()
        if imgui.button("Browse##in"):
            folder = _open_folder_dialog("Select Input Folder")
            if folder:
                self._input_folder = folder
                if not self._output_folder:
                    self._output_folder = os.path.join(folder, "processed")
                self._scan_folder()

        # Output folder
        imgui.text("Output Folder:")
        imgui.set_next_item_width(-80)
        _, self._output_folder = imgui.input_text("##output_dir", self._output_folder, 1024)
        imgui.same_line()
        if imgui.button("Browse##out"):
            folder = _open_folder_dialog("Select Output Folder")
            if folder:
                self._output_folder = folder

        imgui.separator()

        # File list
        avail = imgui.get_content_region_avail()
        list_height = avail.y - 80  # Reserve for buttons + progress
        if imgui.begin_child("##file_list", imgui.ImVec2(0, list_height),
                             child_flags=imgui.ChildFlags_.borders.value):
            if imgui.begin_table("##files", 4, imgui.TableFlags_.row_bg.value | imgui.TableFlags_.borders_inner_h.value):
                imgui.table_setup_column("", imgui.TableColumnFlags_.width_fixed.value, 30)
                imgui.table_setup_column("File", imgui.TableColumnFlags_.width_stretch.value)
                imgui.table_setup_column("Format", imgui.TableColumnFlags_.width_fixed.value, 50)
                imgui.table_setup_column("Duration", imgui.TableColumnFlags_.width_fixed.value, 60)
                imgui.table_headers_row()

                for i, f in enumerate(self._files):
                    imgui.table_next_row()
                    imgui.table_next_column()
                    _, f["checked"] = imgui.checkbox(f"##chk_{i}", f["checked"])
                    imgui.table_next_column()
                    imgui.text(f["name"])
                    imgui.table_next_column()
                    imgui.text(f["ext"])
                    imgui.table_next_column()
                    if f["duration"] > 0:
                        imgui.text(f"{f['duration']:.1f}s")
                    else:
                        imgui.text_disabled("—")

                imgui.end_table()
        imgui.end_child()

        # Select/deselect all
        if imgui.button("Select All"):
            for f in self._files:
                f["checked"] = True
        imgui.same_line()
        if imgui.button("Deselect All"):
            for f in self._files:
                f["checked"] = False

        imgui.same_line()
        if self._app.active_preset_names:
            names = " + ".join(self._app.active_preset_names)
            imgui.text(f"Presets: {names}")
        else:
            imgui.text("Presets: (none)")

        # Process button
        imgui.same_line()
        checked_count = sum(1 for f in self._files if f["checked"])
        if self._processing:
            imgui.begin_disabled()
        if imgui.button(f"Process Selected ({checked_count})"):
            if checked_count > 0 and self._output_folder:
                self._processing = True
                self._worker = threading.Thread(target=self._process_files, daemon=True)
                self._worker.start()
        if self._processing:
            imgui.end_disabled()

        # Progress bar
        if self._processing and self._progress_total > 0:
            frac = self._progress / self._progress_total
            imgui.progress_bar(frac, imgui.ImVec2(-1, 0),
                               f"{self._progress}/{self._progress_total}  {self._progress_file}")

        imgui.end()

    def collect_settings(self) -> dict:
        return {
            "last_input_folder": self._input_folder,
            "last_output_folder": self._output_folder,
        }

    def restore_settings(self, last_input_folder: str = "", last_output_folder: str = ""):
        self._input_folder = last_input_folder
        self._output_folder = last_output_folder
        if self._input_folder and os.path.isdir(self._input_folder):
            self._scan_folder()
