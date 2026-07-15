"""NIF file browser panel — shows NIF-like files in the active NIF's folder.

Tabbed alongside Scene Tree in the left-top dock. Right-click any NIF-like file
to attach it to the current main NIF via auto-detect connect point logic.
"""
from __future__ import annotations

import logging
import os

from imgui_bundle import imgui
from ui.editor.nif_file_types import is_nif_like_path

_log = logging.getLogger("nif_editor.file_browser")


class NifFileBrowserPanel:
    """Left-dock panel showing .nif file tree for the open NIF's directory."""

    def __init__(self, app):
        self.app = app
        self.window_name = "File Browser"  # renamed to "File Browser##nif" by workspace
        self._visible = True
        self._last_root_dir: str | None = None

    # ------------------------------------------------------------------
    # Public helpers (tested without ImGui)
    # ------------------------------------------------------------------

    def _scan_dir(self, dir_path: str) -> list[tuple[bool, str, str]]:
        """Return sorted (is_dir, name, abs_path) entries for dir_path.

        Only includes NIF-like files (case-insensitive) and subdirectories.
        Sorted: directories first, then files; alphabetical case-insensitive
        within each group.  Returns [] on any OS error.
        """
        try:
            entries = list(os.scandir(dir_path))
        except OSError:
            return []

        result = []
        for e in entries:
            if e.is_dir(follow_symlinks=False):
                result.append((True, e.name, e.path))
            elif is_nif_like_path(e.name):
                result.append((False, e.name, e.path))

        result.sort(key=lambda t: (not t[0], t[1].lower()))
        return result

    def _is_attached(self, abs_path: str) -> bool:
        """Return True if abs_path is currently open as any session."""
        norm = os.path.normcase(os.path.normpath(abs_path))
        for session in self.app.registry.all_sessions():
            if os.path.normcase(os.path.normpath(session.file_path)) == norm:
                return True
        return False

    # ------------------------------------------------------------------
    # ImGui rendering
    # ------------------------------------------------------------------

    def draw(self) -> None:
        if not self._visible:
            return
        expanded, opened = imgui.begin(self.window_name, True)
        if not opened:
            self._visible = False
            imgui.end()
            return
        if expanded:
            self._draw_content()
        imgui.end()

    def _draw_content(self) -> None:
        try:
            session = self.app.registry.get_session("main")
        except KeyError:
            imgui.text_disabled("Open a NIF to browse its folder")
            return

        root_dir = os.path.dirname(os.path.normpath(session.file_path))
        self._last_root_dir = root_dir
        folder_name = os.path.basename(root_dir) or root_dir

        flags = imgui.TreeNodeFlags_.default_open
        if imgui.tree_node_ex(folder_name, flags):
            self._draw_dir(root_dir, depth=0)
            imgui.tree_pop()

    def _draw_dir(self, dir_path: str, depth: int) -> None:
        """Recursively render directory contents up to depth 3."""
        if depth >= 3:
            return

        for is_dir, name, abs_path in self._scan_dir(dir_path):
            if is_dir:
                if imgui.tree_node(name):
                    self._draw_dir(abs_path, depth + 1)
                    imgui.tree_pop()
            else:
                attached = self._is_attached(abs_path)
                label = f"{name} [attached]" if attached else name

                if attached:
                    imgui.push_style_color(
                        imgui.Col_.text,
                        imgui.ImVec4(1.0, 0.85, 0.0, 1.0),  # yellow
                    )

                imgui.selectable(label, False)

                if attached:
                    imgui.pop_style_color()

                # Right-click context menu
                if imgui.begin_popup_context_item(f"##ctx_{abs_path}"):
                    if not attached:
                        if imgui.menu_item("Attach", "", False)[0]:
                            try:
                                self.app.attach_nif_auto(abs_path)
                            except Exception as e:
                                self.app.status_text = f"Attach error: {e}"
                        if imgui.menu_item("Bash", "", False)[0]:
                            try:
                                self.app.bash_nif(abs_path)
                            except Exception as e:
                                self.app.status_text = f"Bash error: {e}"
                    else:
                        if imgui.menu_item("Detach", "", False)[0]:
                            norm = os.path.normcase(os.path.normpath(abs_path))
                            for s in self.app.registry.all_sessions():
                                if os.path.normcase(os.path.normpath(s.file_path)) == norm:
                                    try:
                                        self.app.detach_nif(s.nif_id)
                                    except Exception as e:
                                        self.app.status_text = f"Detach error: {e}"
                                    break
                    imgui.end_popup()
