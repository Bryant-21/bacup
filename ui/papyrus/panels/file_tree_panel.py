"""FileTreePanel — script file browser for the Papyrus editor.

Shows mods/*/Scripts/Source/User/ trees plus any extra_roots (e.g. game Data/Scripts/Source/User).
Uses watchdog to rescan on file system changes.
"""
from __future__ import annotations

import logging
import os
from pathlib import Path
from typing import Callable, Optional

from imgui_bundle import imgui

_log = logging.getLogger("toolkit.papyrus.filetree")


class FileTreePanel:
    """Left-dock panel showing .psc file tree."""

    def __init__(self, app, project_root: str, extra_roots: list | None = None):
        """
        Args:
            app: PapyrusEditorApp instance (for open_file callback).
            project_root: Absolute path to project root.
            extra_roots: Additional root directories to show in the tree.
                Each entry is either a plain ``str`` path or a
                ``(label, path)`` tuple for an explicit display label.
        """
        self._app = app
        self._project_root = Path(project_root)
        self._extra_roots: list[str | tuple[str, str]] = list(extra_roots or [])
        self.window_name = "Files##papyrus"
        self._visible = True
        self._tree_cache: list[tuple[str, str]] = []  # (display_label, abs_path)
        self._observer = None
        self._needs_rescan = True
        self._expanded_state: dict[str, bool] = {}   # abs_path -> is_open
        self._state_initialized: set[str] = set()    # paths whose initial state was applied

    def start_watching(self):
        """Start watchdog for tree rescan on file changes."""
        roots = self._get_roots()
        if not roots:
            return
        try:
            from watchdog.observers import Observer
            from watchdog.events import FileSystemEventHandler

            class _Handler(FileSystemEventHandler):
                def __init__(self, panel):
                    self._panel = panel
                def _check(self, event):
                    if not event.is_directory and event.src_path.endswith(".psc"):
                        self._panel._needs_rescan = True
                def on_created(self, event): self._check(event)
                def on_deleted(self, event): self._check(event)
                def on_moved(self, event): self._check(event)

            self._observer = Observer()
            for root in roots:
                if os.path.isdir(root):
                    self._observer.schedule(_Handler(self), root, recursive=True)
            self._observer.start()
        except Exception as e:
            _log.warning("FileTreePanel: cannot start watcher: %s", e)

    def stop_watching(self):
        if self._observer:
            self._observer.stop()
            self._observer.join(timeout=1.0)
            self._observer = None

    def draw(self):
        if not self._visible:
            return
        expanded, opened = imgui.begin(self.window_name, True)
        if not opened:
            self._visible = False
            imgui.end()
            return
        if expanded:
            if self._needs_rescan:
                self._rescan()
                self._needs_rescan = False
            self._draw_tree()
        imgui.end()

    def _get_roots(self) -> list[tuple[str, str]]:
        """Return (label, abs_path) pairs to show in the tree."""
        roots: list[tuple[str, str]] = []
        seen: set[str] = set()

        mods = self._project_root / "mods"
        if mods.is_dir():
            for mod_dir in sorted(mods.iterdir()):
                scripts_dir = mod_dir / "Scripts" / "Source" / "User"
                if scripts_dir.is_dir():
                    norm = os.path.normpath(str(scripts_dir))
                    if norm not in seen:
                        roots.append((mod_dir.name, norm))
                        seen.add(norm)

        # User-configured extra roots (plain paths or (label, path) tuples)
        for entry in self._extra_roots:
            if isinstance(entry, tuple):
                label, path = entry
            else:
                label = None
                path = entry
            norm = os.path.normpath(path)
            if os.path.isdir(norm) and norm not in seen:
                if label is None:
                    parts = Path(norm).parts
                    label = "/".join(parts[-3:]) if len(parts) >= 3 else os.path.basename(norm)
                roots.append((label, norm))
                seen.add(norm)
        return roots

    def get_tree_state(self) -> dict:
        """Return a serialisable snapshot of expanded/collapsed state."""
        return dict(self._expanded_state)

    def set_tree_state(self, state: dict) -> None:
        """Restore expanded/collapsed state (called before first draw)."""
        self._expanded_state = dict(state)
        self._state_initialized.clear()

    def _rescan(self):
        """Rebuild the tree cache from the file system."""
        self._tree_cache = list(self._get_roots())
        # Allow set_next_item_open to re-apply saved state for new/changed nodes
        self._state_initialized.clear()
        _log.debug("FileTreePanel: rescanned, %d roots", len(self._tree_cache))

    def _draw_tree(self):
        for root_label, root_path in self._tree_cache:
            if root_path not in self._state_initialized:
                imgui.set_next_item_open(
                    self._expanded_state.get(root_path, False),
                    imgui.Cond_.once,
                )
                self._state_initialized.add(root_path)
            is_open = imgui.tree_node_ex(root_label, 0)
            self._expanded_state[root_path] = is_open
            if is_open:
                self._draw_dir(root_path, depth=0)
                imgui.tree_pop()

    def _draw_dir(self, dir_path: str, depth: int):
        try:
            entries = sorted(os.scandir(dir_path), key=lambda e: (not e.is_dir(), e.name.lower()))
        except OSError:
            return
        for entry in entries:
            if entry.is_dir():
                key = os.path.normpath(entry.path)
                if key not in self._state_initialized:
                    imgui.set_next_item_open(
                        self._expanded_state.get(key, False),
                        imgui.Cond_.once,
                    )
                    self._state_initialized.add(key)
                is_open = imgui.tree_node(entry.name + "/")
                self._expanded_state[key] = is_open
                if is_open:
                    self._draw_dir(entry.path, depth + 1)
                    imgui.tree_pop()
            elif entry.name.endswith(".psc"):
                path = os.path.normpath(entry.path)
                buf = self._app.open_files.get(path)
                dirty_marker = " \u25cf" if buf and buf.dirty else ""
                label = entry.name + dirty_marker + f"##{path}"
                selected = self._app.active_path == path
                if imgui.selectable(label, selected)[0]:
                    self._app.open_file(path)
                    self._app.active_path = path
