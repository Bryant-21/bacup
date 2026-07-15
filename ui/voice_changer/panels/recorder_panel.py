"""Recorder panel — record/load audio, dual waveform, apply effects, save."""
from __future__ import annotations

import logging
import os
import time
from typing import Optional

import numpy as np
from imgui_bundle import imgui, implot

from creation_lib.ui.theme.window_chrome import AsyncWorker

_log = logging.getLogger("toolkit.voice_changer.recorder")
_NS = "##voice_changer"


def _open_file_dialog(title: str, filetypes=None, save: bool = False) -> str | None:
    """Open a native file dialog."""
    try:
        from creation_lib.ui.widgets.pick_folder import pick_file, pick_save_file
        if save:
            return pick_save_file(
                title,
                filetypes or [("WAV files", "*.wav")],
                default_ext=".wav",
            )
        return pick_file(
            title,
            filetypes or [("WAV files", "*.wav"), ("All files", "*.*")],
        )
    except Exception:
        _log.warning("File dialog not available")
        return None


class RecorderPanel:
    """Record or load audio, preview original vs processed, save result."""

    def __init__(self, app):
        self._app = app
        self.window_name = f"Recorder{_NS}"

        # Recording state
        self._recording: bool = False
        self._rec_start_time: float = 0.0
        self._rec_stream = None
        self._rec_frames: list[np.ndarray] = []

        # Input devices
        self._input_devices: list[str] = []
        self._input_device_ids: list = []
        self._in_device_idx: int = 0

        # Output devices
        self._output_devices: list[str] = []
        self._output_device_ids: list = []
        self._out_device_idx: int = 0

        # Original audio
        self._orig_data: Optional[np.ndarray] = None
        self._orig_samples: Optional[np.ndarray] = None  # downsampled for waveform
        self._orig_duration: float = 0.0

        # Processed audio
        self._proc_data: Optional[np.ndarray] = None
        self._proc_samples: Optional[np.ndarray] = None
        self._proc_duration: float = 0.0

        # Playback state (original)
        self._orig_playing: bool = False
        self._orig_play_pos: float = 0.0
        self._orig_play_start_time: float = 0.0
        self._orig_play_start_offset: float = 0.0

        # Playback state (processed)
        self._proc_playing: bool = False
        self._proc_play_pos: float = 0.0
        self._proc_play_start_time: float = 0.0
        self._proc_play_start_offset: float = 0.0

        # Volume
        self._volume: float = 1.0

        # End trim (milliseconds to cut from end of recording)
        self._end_trim_ms: float = 0.0

        # Async processing
        self._worker: Optional[AsyncWorker] = None
        self._apply_and_play: bool = False

        self._enumerate_devices()

    def _enumerate_devices(self):
        """Enumerate audio devices, filtering to WASAPI only (best quality on Windows)."""
        try:
            import sounddevice as sd
            devices = sd.query_devices()
            self._input_devices = []
            self._input_device_ids = []
            self._output_devices = []
            self._output_device_ids = []

            # Find WASAPI host API index; fall back to showing all if not found
            wasapi_api = None
            for api_idx in range(sd.query_hostapis.__wrapped__() if hasattr(sd.query_hostapis, '__wrapped__') else 99):
                try:
                    api = sd.query_hostapis(api_idx)
                    if "WASAPI" in api["name"]:
                        wasapi_api = api_idx
                        break
                except Exception:
                    break

            for i, d in enumerate(devices):
                # Filter to WASAPI devices only (avoids MME/DirectSound/WDM-KS duplicates)
                if wasapi_api is not None and d.get("hostapi") != wasapi_api:
                    continue
                if d["max_input_channels"] > 0:
                    self._input_devices.append(d["name"])
                    self._input_device_ids.append(i)
                if d["max_output_channels"] > 0:
                    self._output_devices.append(d["name"])
                    self._output_device_ids.append(i)
        except Exception:
            self._input_devices = ["Default"]
            self._input_device_ids = [None]
            self._output_devices = ["Default"]
            self._output_device_ids = [None]

    def _load_audio(self, path: str):
        """Load a WAV file as the original audio."""
        try:
            import soundfile as sf
            data, sr = sf.read(path, always_2d=False)
            if data.ndim > 1:
                data = data[:, 0]
            self._orig_data = data.astype(np.float32)
            step = max(1, len(data) // 2000)
            self._orig_samples = data[::step].astype(np.float32)
            self._app.sample_rate = sr
            self._orig_duration = len(data) / sr
            self._orig_play_pos = 0.0
            self._proc_data = None
            self._proc_samples = None
            self._proc_duration = 0.0
            self._app.log(f"Loaded: {os.path.basename(path)} ({sr}Hz, {self._orig_duration:.1f}s)")
        except Exception as e:
            self._app.log(f"Failed to load audio: {e}")

    def _set_processed(self, data: np.ndarray):
        """Set the processed audio result."""
        self._proc_data = data
        step = max(1, len(data) // 2000)
        self._proc_samples = data[::step].astype(np.float32)
        self._proc_duration = len(data) / self._app.sample_rate
        self._proc_play_pos = 0.0

    def _play(self, which: str, from_pos: float = -1.0):
        """Start playback. which: 'orig' or 'proc'."""
        data = self._orig_data if which == "orig" else self._proc_data
        if data is None:
            return
        try:
            import sounddevice as sd
            sd.stop()

            if which == "orig":
                if from_pos >= 0:
                    self._orig_play_pos = max(0.0, min(from_pos, 1.0))
                start = int(self._orig_play_pos * len(data))
                self._orig_playing = True
                self._proc_playing = False
                self._orig_play_start_offset = self._orig_play_pos * self._orig_duration
                self._orig_play_start_time = time.time()
            else:
                if from_pos >= 0:
                    self._proc_play_pos = max(0.0, min(from_pos, 1.0))
                start = int(self._proc_play_pos * len(data))
                self._proc_playing = True
                self._orig_playing = False
                self._proc_play_start_offset = self._proc_play_pos * self._proc_duration
                self._proc_play_start_time = time.time()

            remaining = data[start:]
            if len(remaining) == 0:
                return
            play_data = remaining * self._volume if self._volume < 1.0 else remaining

            sd_device = None
            if self._out_device_idx < len(self._output_device_ids):
                sd_device = self._output_device_ids[self._out_device_idx]
            sd.play(play_data, self._app.sample_rate, device=sd_device)
        except Exception as e:
            _log.exception("Playback failed: %s", e)
            self._orig_playing = False
            self._proc_playing = False

    def _stop(self, which: str):
        try:
            import sounddevice as sd
            sd.stop()
        except Exception:
            pass
        if which == "orig":
            self._orig_playing = False
            self._orig_play_pos = 0.0
        else:
            self._proc_playing = False
            self._proc_play_pos = 0.0

    def _start_recording(self):
        try:
            import sounddevice as sd
            self._rec_frames = []
            self._rec_start_time = time.time()
            self._recording = True
            dev_idx = (
                self._input_device_ids[self._in_device_idx]
                if self._in_device_idx < len(self._input_device_ids)
                else None
            )

            # Use the device's default sample rate (44100 may not be supported)
            dev_info = sd.query_devices(dev_idx, 'input') if dev_idx is not None else sd.query_devices(kind='input')
            native_sr = int(dev_info['default_samplerate'])
            self._app.sample_rate = native_sr

            def callback(indata, frames, t, status):
                self._rec_frames.append(indata.copy())

            self._rec_stream = sd.InputStream(
                samplerate=native_sr,
                channels=1,
                dtype="float32",
                device=dev_idx,
                callback=callback,
            )
            self._rec_stream.start()
            self._app.log("Recording started...")
        except Exception as e:
            self._recording = False
            self._app.log(f"Recording failed: {e}")

    def _stop_recording(self):
        if self._rec_stream:
            self._rec_stream.stop()
            self._rec_stream.close()
            self._rec_stream = None
        self._recording = False
        if self._rec_frames:
            data = np.concatenate(self._rec_frames, axis=0).flatten()
            # Trim end to remove click artifacts
            if self._end_trim_ms > 0:
                trim_samples = int(self._end_trim_ms / 1000.0 * self._app.sample_rate)
                if trim_samples < len(data):
                    data = data[:-trim_samples]
            self._orig_data = data.astype(np.float32)
            step = max(1, len(data) // 2000)
            self._orig_samples = data[::step].astype(np.float32)
            self._orig_duration = len(data) / self._app.sample_rate
            self._orig_play_pos = 0.0
            self._proc_data = None
            self._proc_samples = None
            self._app.log(f"Recorded {self._orig_duration:.1f}s of audio")
            self._auto_save(data, "rec")

    def draw(self):
        imgui.begin(self.window_name)

        # -- Controls row --
        if self._recording:
            if imgui.button("Stop Recording"):
                self._stop_recording()
            imgui.same_line()
            elapsed = time.time() - self._rec_start_time
            imgui.text_colored(imgui.ImVec4(1, 0.3, 0.3, 1), f"REC {elapsed:.1f}s")
        else:
            if imgui.button("Record"):
                self._start_recording()

        imgui.same_line()
        if imgui.button("Load File"):
            path = _open_file_dialog("Load Audio File")
            if path:
                self._load_audio(path)

        # End trim slider
        imgui.same_line()
        imgui.set_next_item_width(120)
        _, self._end_trim_ms = imgui.slider_float(
            "End trim##vc", self._end_trim_ms, 0.0, 500.0, "%.0f ms"
        )
        if imgui.is_item_hovered():
            imgui.set_item_tooltip("Trim milliseconds from end of recording to remove click artifacts")

        # Input device
        imgui.same_line()
        imgui.set_next_item_width(350)
        if self._input_devices:
            changed, self._in_device_idx = imgui.combo(
                "Input##vc_in", self._in_device_idx, self._input_devices
            )

        imgui.separator()

        # -- Original waveform --
        self._update_playback_pos("orig")
        imgui.text("Original")
        self._draw_transport("orig")
        self._draw_waveform("orig", self._orig_samples, self._orig_play_pos, self._orig_duration)

        imgui.spacing()
        imgui.separator()
        imgui.spacing()

        # -- Processed waveform --
        self._update_playback_pos("proc")
        imgui.text("Processed")
        self._draw_transport("proc")
        self._draw_waveform("proc", self._proc_samples, self._proc_play_pos, self._proc_duration)

        imgui.separator()

        # Poll worker
        if self._worker and self._worker.done:
            if self._worker.error:
                self._app.log(f"Processing failed: {self._worker.error}")
            else:
                self._set_processed(self._worker.result)
                self._app.log("Effect applied successfully")
                self._auto_save(self._worker.result, "proc")
                if getattr(self, "_apply_and_play", False):
                    self._play("proc", from_pos=0.0)
                    self._apply_and_play = False
            self._worker = None

        imgui.same_line()
        if self._proc_data is None:
            imgui.begin_disabled()
        if imgui.button("Save Processed WAV"):
            path = _open_file_dialog("Save Processed Audio", save=True)
            if path and self._proc_data is not None:
                import soundfile as sf
                sf.write(path, self._proc_data, self._app.sample_rate)
                self._app.log(f"Saved: {path}")
        if self._proc_data is None:
            imgui.end_disabled()

        # Volume + output device
        imgui.same_line()
        imgui.set_next_item_width(100)
        _, self._volume = imgui.slider_float("Volume##vc", self._volume, 0.0, 1.0)
        imgui.same_line()
        imgui.set_next_item_width(350)
        if self._output_devices:
            _, self._out_device_idx = imgui.combo(
                "Output##vc_out", self._out_device_idx, self._output_devices
            )

        imgui.end()

    def _update_playback_pos(self, which: str):
        """Update play position based on elapsed time."""
        if which == "orig" and self._orig_playing:
            elapsed = time.time() - self._orig_play_start_time
            current = self._orig_play_start_offset + elapsed
            self._orig_play_pos = min(current / self._orig_duration, 1.0) if self._orig_duration > 0 else 0.0
            if self._orig_play_pos >= 1.0:
                self._orig_playing = False
                self._orig_play_pos = 0.0
        elif which == "proc" and self._proc_playing:
            elapsed = time.time() - self._proc_play_start_time
            current = self._proc_play_start_offset + elapsed
            self._proc_play_pos = min(current / self._proc_duration, 1.0) if self._proc_duration > 0 else 0.0
            if self._proc_play_pos >= 1.0:
                self._proc_playing = False
                self._proc_play_pos = 0.0

    def _draw_transport(self, which: str):
        """Draw play/stop controls and time display."""
        playing = self._orig_playing if which == "orig" else self._proc_playing
        pos = self._orig_play_pos if which == "orig" else self._proc_play_pos
        duration = self._orig_duration if which == "orig" else self._proc_duration
        processing = self._worker is not None and not self._worker.done

        if which == "orig":
            has_data = self._orig_data is not None
        else:
            has_data = self._orig_data is not None  # can always apply if original exists

        if not has_data or (which == "proc" and processing):
            imgui.begin_disabled()

        if playing:
            if imgui.button(f"Pause##{which}"):
                try:
                    import sounddevice as sd
                    sd.stop()
                except Exception:
                    pass
                if which == "orig":
                    self._orig_playing = False
                else:
                    self._proc_playing = False
        else:
            label = "Applying...##proc" if (which == "proc" and processing) else f"Play##{which}"
            if imgui.button(label):
                if which == "orig":
                    self._play(which, from_pos=pos)
                else:
                    self._apply_and_play = True
                    self._start_apply()

        imgui.same_line()
        if imgui.button(f"Stop##{which}"):
            self._stop(which)

        imgui.same_line()
        current_sec = pos * duration
        imgui.text(f"{current_sec:.1f}s / {duration:.1f}s")

        if not has_data or (which == "proc" and processing):
            imgui.end_disabled()

    def _draw_waveform(self, which: str, samples, play_pos: float, duration: float):
        """Draw waveform with implot and draggable playhead."""
        waveform_height = 80
        if samples is not None and len(samples) > 0:
            implot.push_style_var(implot.StyleVar_.plot_padding.value, imgui.ImVec2(0, 0))
            flags = (
                implot.Flags_.no_legend.value
                | implot.Flags_.no_mouse_text.value
                | implot.Flags_.no_title.value
            )
            if implot.begin_plot(f"##waveform_{which}", size=imgui.ImVec2(-1, waveform_height), flags=flags):
                implot.setup_axes(
                    "", "",
                    implot.AxisFlags_.no_decorations.value,
                    implot.AxisFlags_.no_decorations.value,
                )
                implot.setup_axes_limits(0, len(samples), -1.0, 1.0, implot.Cond_.always.value)
                implot.plot_shaded(f"##wave_{which}", samples, 0.0)

                # Draggable playhead
                if duration > 0:
                    playhead_x = play_pos * len(samples)
                    color = imgui.ImVec4(1.0, 0.9, 0.2, 1.0)
                    changed, new_x, _, _, _ = implot.drag_line_x(0, playhead_x, color, 2.0)
                    if changed:
                        new_pos = max(0.0, min(new_x / len(samples), 1.0))
                        if which == "orig":
                            self._orig_play_pos = new_pos
                        else:
                            self._proc_play_pos = new_pos
                        if (which == "orig" and self._orig_playing) or (which == "proc" and self._proc_playing):
                            self._play(which, from_pos=new_pos)

                implot.end_plot()
            implot.pop_style_var()
        else:
            imgui.dummy(imgui.ImVec2(0, waveform_height))

    def _start_apply(self):
        """Kick off async effect processing."""
        if self._orig_data is not None:
            self._worker = AsyncWorker(
                target_fn=self._app.process_audio,
                args=(self._orig_data.copy(), self._app.sample_rate),
            )
            self._worker.start()
            self._app.log("Applying effect chain...")

    def _auto_save(self, audio: np.ndarray, prefix: str):
        """Auto-save audio and refresh the recordings panel."""
        from ui.voice_changer.panels.recordings_panel import RecordingsPanel
        path = RecordingsPanel.auto_save(audio, self._app.sample_rate, prefix)
        if path:
            self._app.log(f"Auto-saved: {os.path.basename(path)}")
            if self._app.recordings_panel:
                self._app.recordings_panel._refresh()

    def collect_settings(self) -> dict:
        return {
            "input_device": self._in_device_idx,
            "output_device": self._out_device_idx,
            "volume": self._volume,
            "end_trim_ms": self._end_trim_ms,
        }

    def restore_settings(self, input_device: int = 0, output_device: int = 0,
                         volume: float = 1.0, end_trim_ms: float = 0.0):
        self._in_device_idx = input_device
        self._out_device_idx = output_device
        self._volume = volume
        self._end_trim_ms = end_trim_ms
