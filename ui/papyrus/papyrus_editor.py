"""PapyrusEditorApp — owns editor buffers, file watchers, and LSP coordination.

File watching uses watchdog. One watchdog.Observer per unique parent directory
(per spec: deduplicated by parent dir, per-file filtering).
"""
from __future__ import annotations

import logging
import os
import threading
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

from ui.papyrus.papyrus_syntax_editor import PapyrusSyntaxEditor, SetViewAtLineMode

_log = logging.getLogger("toolkit.papyrus.editor")


def _is_base_game_path(path: str) -> bool:
    """Return True for base game source files that must not be edited."""
    normalized = path.replace('\\', '/').lower()
    return '/scripts/source/base/' in normalized


@dataclass
class EditorBuffer:
    path: str
    text: str                           # current in-editor content
    editor: "PapyrusSyntaxEditor"
    diagnostics: list = field(default_factory=list)  # list[Diagnostic] from LspService
    dirty: bool = False                 # unsaved changes flag
    external_changed: bool = False      # set by watchdog, prompts reload
    # watchdog internals — managed by PapyrusEditorApp
    _last_editor_text: str = field(default="", repr=False)  # to detect edits
    _known_mtime: float = field(default=0.0, repr=False)    # mtime at last open/save


class _FileEventHandler:
    """watchdog event handler that filters to a specific set of filenames."""

    def __init__(self, app: "PapyrusEditorApp"):
        self._app = app

    def on_modified(self, event):
        if event.is_directory:
            return
        path = os.path.normpath(event.src_path)
        buf = self._app.open_files.get(path)
        if buf is None:
            return
        try:
            current_mtime = os.path.getmtime(path)
        except OSError:
            return
        if current_mtime <= buf._known_mtime:
            return  # spurious event — file hasn't actually changed since we opened/saved it
        buf._known_mtime = current_mtime
        buf.external_changed = True
        _log.debug("External change detected: %s", path)

    def dispatch(self, event):
        """Required by watchdog Observer — route events."""
        from watchdog.events import FileModifiedEvent
        if isinstance(event, FileModifiedEvent):
            self.on_modified(event)


class PapyrusEditorApp:
    """Owns open file buffers and coordinates with LspService."""

    def __init__(self, lsp):
        """
        Args:
            lsp: LspService instance (already started).
        """
        self.open_files: dict[str, EditorBuffer] = {}  # path -> EditorBuffer
        self.active_path: str | None = None
        self.lsp = lsp

        # watchdog: one Observer per parent directory
        self._observers: dict[str, object] = {}   # parent_dir -> watchdog.Observer
        self._event_handler = _FileEventHandler(self)

    def open_file(self, path: str):
        """Load a .psc file into the editor. No-op if already open."""
        path = os.path.normpath(path)
        if path in self.open_files:
            self.active_path = path
            return
        if not os.path.exists(path):
            _log.warning("File not found: %s", path)
            return

        try:
            text = Path(path).read_text(encoding="utf-8", errors="replace")
        except OSError as e:
            _log.error("Cannot open %s: %s", path, e)
            return

        editor = PapyrusSyntaxEditor()
        editor.set_text(text)
        if _is_base_game_path(path):
            editor.set_read_only_enabled(True)

        mtime = os.path.getmtime(path)
        buf = EditorBuffer(path=path, text=text, editor=editor, _last_editor_text=text,
                           _known_mtime=mtime)
        self.open_files[path] = buf
        self.active_path = path

        # Start file watcher for parent dir
        self._ensure_observer(os.path.dirname(path))

        # Submit initial parse
        from ui.papyrus.papyrus_lsp_service import LspRequest, PARSE_RESOLVE
        self.lsp.submit(LspRequest(PARSE_RESOLVE, path=path, text=text))
        _log.info("Opened file: %s", path)

    def close_file(self, path: str):
        """Remove a buffer and stop its watcher if no other files share the dir."""
        path = os.path.normpath(path)
        if path not in self.open_files:
            return
        del self.open_files[path]

        if self.active_path == path:
            self.active_path = next(iter(self.open_files), None)

        # Stop observer if no open files share the same parent dir
        parent = os.path.dirname(path)
        still_watching = any(
            os.path.dirname(p) == parent for p in self.open_files
        )
        if not still_watching and parent in self._observers:
            obs = self._observers.pop(parent)
            obs.stop()
            obs.join(timeout=1.0)

        _log.info("Closed file: %s", path)

    def save_file(self, path: str):
        """Write buffer to disk, clear dirty flag, re-submit parse."""
        path = os.path.normpath(path)
        buf = self.open_files.get(path)
        if buf is None:
            return
        try:
            Path(path).write_text(buf.text, encoding="utf-8")
            buf.dirty = False
            buf.external_changed = False
            buf._known_mtime = os.path.getmtime(path)
            _log.info("Saved file: %s", path)
        except OSError as e:
            _log.error("Cannot save %s: %s", path, e)
            return

        from ui.papyrus.papyrus_lsp_service import LspRequest, PARSE_RESOLVE
        self.lsp.submit(LspRequest(PARSE_RESOLVE, path=path, text=buf.text))

    def update(self):
        """Called each frame. Poll LSP results and sync editor state."""
        # 1. Poll diagnostics
        diag_map = self.lsp.poll_diagnostics()
        for path, diags in diag_map.items():
            if path in self.open_files:
                self.open_files[path].diagnostics = diags

        # 2. Poll definition result
        def_result = self.lsp.poll_definition()
        if def_result is not None:
            if def_result.path:
                self.open_file(def_result.path)
                target_path = os.path.normpath(def_result.path)
            else:
                target_path = self.active_path
            if target_path and target_path in self.open_files:
                buf = self.open_files[target_path]
                buf.editor.set_view_at_line(
                    def_result.line,
                    SetViewAtLineMode.if_not_visible,
                )

    def _ensure_observer(self, parent_dir: str):
        """Start a watchdog Observer for parent_dir if one isn't running."""
        parent_dir = os.path.normpath(parent_dir)
        if parent_dir in self._observers:
            return
        try:
            from watchdog.observers import Observer
            from watchdog.events import FileSystemEventHandler

            class _Handler(FileSystemEventHandler):
                def __init__(self, handler):
                    self._h = handler
                def on_modified(self, event):
                    self._h.on_modified(event)

            obs = Observer()
            obs.schedule(_Handler(self._event_handler), parent_dir, recursive=False)
            obs.start()
            self._observers[parent_dir] = obs
        except Exception as e:
            _log.warning("Cannot start file watcher for %s: %s", parent_dir, e)

    def cleanup(self):
        """Stop all file watchers."""
        for obs in self._observers.values():
            obs.stop()
        for obs in self._observers.values():
            obs.join(timeout=1.0)
        self._observers.clear()
