"""Conversion Log panel — BottomDock.

Scrollable, color-coded log with level filtering.
"""
from __future__ import annotations

import logging

from imgui_bundle import imgui

_logger = logging.getLogger("toolkit.conversion")

_NS = "##conversion"

_COLORS = {
    "INFO": imgui.ImVec4(0.85, 0.85, 0.85, 1.0),
    "WARN": imgui.ImVec4(1.0, 0.9, 0.3, 1.0),
    "ERROR": imgui.ImVec4(1.0, 0.3, 0.3, 1.0),
}

_MAX_ENTRIES = 5000


class ConversionLogPanel:
    def __init__(self, workspace):
        self._workspace = workspace
        self._entries: list[tuple[str, str]] = []  # (level, message)
        self._show_info = True
        self._show_warn = True
        self._show_error = True
        self._auto_scroll = True

    def draw(self):
        if imgui.begin(f"Conversion Log{_NS}"):
            self.draw_body()
        imgui.end()

    def draw_body(self):
        # Filter buttons
        _, self._show_info = imgui.checkbox(f"INFO{_NS}_fi", self._show_info)
        imgui.same_line()
        _, self._show_warn = imgui.checkbox(f"WARN{_NS}_fw", self._show_warn)
        imgui.same_line()
        _, self._show_error = imgui.checkbox(f"ERROR{_NS}_fe", self._show_error)
        imgui.same_line()
        if imgui.button(f"Clear{_NS}"):
            self._entries.clear()

        imgui.separator()

        # Log content
        avail = imgui.get_content_region_avail()
        if imgui.begin_child(f"log_scroll{_NS}", imgui.ImVec2(0, avail.y)):
            for level, msg in self._entries:
                if level == "INFO" and not self._show_info:
                    continue
                if level == "WARN" and not self._show_warn:
                    continue
                if level == "ERROR" and not self._show_error:
                    continue

                color = _COLORS.get(level, _COLORS["INFO"])
                imgui.push_style_color(imgui.Col_.text, color)
                imgui.text_wrapped(f"[{level}] {msg}")
                imgui.pop_style_color()

            if self._auto_scroll:
                imgui.set_scroll_here_y(1.0)
        imgui.end_child()

    def handle_event(self, event: dict) -> None:
        event_type = event.get("type")
        if event_type == "log":
            self._append_entry(event.get("level", "INFO"), event.get("message", ""))
        elif event_type == "phase_start":
            self._append_entry("INFO", _phase_start_message(event.get("data", {})))
        elif event_type == "phase_complete":
            data = event.get("data", {})
            level = "ERROR" if data.get("status") == "error" else "INFO"
            self._append_entry(level, _phase_complete_message(data))

    def _append_entry(self, level: str, message: str) -> None:
        self._entries.append((level, message))
        if level == "ERROR":
            _logger.error("%s", message)
        elif level == "WARN":
            _logger.warning("%s", message)
        else:
            _logger.info("%s", message)
        # Cap entries
        if len(self._entries) > _MAX_ENTRIES:
            self._entries = self._entries[-_MAX_ENTRIES:]

    def load_from_file(self, log_path: str) -> None:
        """Load prior conversion log entries from a file."""
        import os

        if not os.path.isfile(log_path):
            return
        try:
            with open(log_path, encoding="utf-8") as f:
                for line in f:
                    line = line.rstrip()
                    if line.startswith("[INFO]"):
                        self._entries.append(("INFO", line[7:]))
                    elif line.startswith("[WARN]"):
                        self._entries.append(("WARN", line[7:]))
                    elif line.startswith("[ERROR]"):
                        self._entries.append(("ERROR", line[8:]))
        except Exception:
            pass


def _phase_start_message(data: dict) -> str:
    return f"Starting {_phase_label(data)}"


def _phase_complete_message(data: dict) -> str:
    label = _phase_label(data)
    completed = int(data.get("completed_items") or 0)
    total = int(data.get("total_items") or 0)
    item_counts = f" ({completed}/{total} items)"
    elapsed = _phase_elapsed_suffix(data)
    if data.get("status") == "error":
        error = data.get("error") or "unknown error"
        return f"Failed {label}: {error}{item_counts}{elapsed}"
    return f"Completed {label}{item_counts}{elapsed}"


def _phase_label(data: dict) -> str:
    phase = data.get("phase")
    phase_name = data.get("phase_name") or "Unnamed phase"
    if phase:
        return f"phase {phase}: {phase_name}"
    return phase_name


def _phase_elapsed_suffix(data: dict) -> str:
    elapsed_seconds = data.get("elapsed_seconds")
    if not isinstance(elapsed_seconds, (int, float)):
        return ""
    return f" in {float(elapsed_seconds):.3f}s"
