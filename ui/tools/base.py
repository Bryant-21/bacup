"""Base class for all bulk tools."""

from __future__ import annotations

import logging
import os
import threading
import traceback
from typing import Callable

from imgui_bundle import imgui

_log = logging.getLogger("tools")


class BaseTool:
    """Base class for all tool windows in the Bulk Toolbox.

    Subclasses MUST set: name, tool_id, description, category.
    Subclasses MUST override: draw_content().
    Subclasses MAY override: initialize(), cleanup(), get_default_settings(),
                             apply_settings(), collect_settings().
    """

    name: str = ""
    tool_id: str = ""
    description: str = ""
    category: str = ""

    def __init__(self):
        self.visible = False
        self._initialized = False
        self._shared_paths: dict[str, str] = {}  # populated by workspace

        # Background processing state (read by UI thread, written by worker)
        self._running = False
        self._progress = 0.0
        self._status_msg = ""
        self._error_msg = ""
        self._cancel_requested = False
        self._worker_thread: threading.Thread | None = None

        # Results (set by worker, read by UI after completion)
        self._result_msg = ""

    # -- Tool discovery --

    @staticmethod
    def find_resource_tool(name: str, subdir: str = "") -> str:
        """Locate an executable bundled in the resource directory.

        Checks resource/<subdir>/<name>, resource/<name>, then PATH.
        Returns the path string or "" if not found.
        """
        from ui.toolkit.app_paths import get_resource_dir
        res = get_resource_dir()
        candidates = [res / subdir / name] if subdir else []
        candidates.append(res / name)
        try:
            from creation_lib.paths import get_resource_dir as get_creation_lib_resource_dir

            lib_res = get_creation_lib_resource_dir()
            if subdir:
                candidates.append(lib_res / subdir / name)
            candidates.append(lib_res / name)
        except Exception:
            pass
        for c in candidates:
            if c.is_file():
                return str(c)
        import shutil
        found = shutil.which(name)
        return found or ""

    # -- File collection --

    @staticmethod
    def collect_files(
        input_path: str,
        file_filter: Callable[[str], bool],
        include_subdirs: bool = True,
    ) -> list[tuple[str, str | None]]:
        """Collect files matching a filter from a file or directory.

        Returns list of (absolute_path, relative_dir_from_input).
        If input_path is a single file, returns [(path, None)] if it passes
        the filter, otherwise [].
        """
        ip = os.path.abspath(input_path)

        if os.path.isfile(ip):
            return [(ip, None)] if file_filter(ip) else []

        tasks = []
        if include_subdirs:
            walker = os.walk(ip)
        else:
            try:
                files = [f for f in os.listdir(ip) if os.path.isfile(os.path.join(ip, f))]
            except OSError:
                return []
            walker = [(ip, [], files)]

        for root, _dirs, files in walker:
            rel = os.path.relpath(root, ip)
            if rel == ".":
                rel = ""
            for f in files:
                path = os.path.join(root, f)
                if file_filter(path):
                    tasks.append((path, rel))

        return tasks

    # -- Lifecycle --

    def initialize(self) -> None:
        """One-time setup. Called when workspace initializes."""
        self._initialized = True

    def cleanup(self) -> None:
        """App exit. Cancel any running work."""
        self._cancel_requested = True
        if self._worker_thread and self._worker_thread.is_alive():
            self._worker_thread.join(timeout=2.0)

    # -- Settings --

    def get_default_settings(self) -> dict:
        return {}

    def apply_settings(self, settings: dict) -> None:
        pass

    def collect_settings(self) -> dict:
        return {}

    # -- UI --

    def draw_window(self) -> None:
        """Render the floating ImGui window. Only called when self.visible is True."""
        if not self.visible:
            return

        imgui.set_next_window_size(imgui.ImVec2(750, 500), imgui.Cond_.first_use_ever)
        expanded, self.visible = imgui.begin(f"{self.name}###tool_{self.tool_id}", True)
        if expanded:
            self.draw_content()

            # -- Error display --
            if self._error_msg:
                imgui.spacing()
                imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.3, 0.3, 1.0))
                imgui.text_wrapped(self._error_msg)
                imgui.pop_style_color()
                if imgui.small_button("Dismiss"):
                    self._error_msg = ""

            # -- Result display --
            if self._result_msg and not self._running:
                imgui.spacing()
                imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(0.3, 1.0, 0.3, 1.0))
                imgui.text_wrapped(self._result_msg)
                imgui.pop_style_color()

            # -- Progress bar --
            if self._running:
                imgui.spacing()
                imgui.separator()
                imgui.progress_bar(self._progress, imgui.ImVec2(-1, 0), self._status_msg)
                if imgui.button("Cancel"):
                    self._cancel_requested = True

        imgui.end()

    def draw_content(self) -> None:
        """Override in subclasses — render the tool-specific UI.

        This is called inside the imgui.begin/end block.
        """
        imgui.text("Not implemented")

    # -- Background processing --

    def _start_batch(self, target, *args, **kwargs) -> None:
        """Start a background task. `target` is a callable that receives *args, **kwargs.

        The target function should call self._on_progress() and check self._cancel_requested.
        """
        if self._running:
            return

        self._running = True
        self._progress = 0.0
        self._status_msg = "Starting..."
        self._error_msg = ""
        self._result_msg = ""
        self._cancel_requested = False

        def _wrapper():
            try:
                target(*args, **kwargs)
            except Exception as e:
                self._error_msg = f"Error: {e}\n{traceback.format_exc()}"
                _log.exception("Tool %s failed", self.tool_id)
            finally:
                self._running = False

        self._worker_thread = threading.Thread(target=_wrapper, daemon=True)
        self._worker_thread.start()

    def _on_progress(self, current: int, total: int, message: str = "") -> None:
        """Called from worker thread to update progress."""
        self._progress = current / max(total, 1)
        self._status_msg = message or f"{current}/{total}"
