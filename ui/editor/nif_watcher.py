"""Watch NIF files for external changes and reload the scene.

Used by the AI terminal panel: when an AI CLI tool saves the NIF via nif-tools MCP,
the watcher detects the change and reloads the 3D viewport with an undo point.

Supports multiple NIF sessions — each session's file path is watched independently,
with directory-level deduplication (one Observer schedule per unique directory).
"""
import logging
import os
import time

from pathlib import Path
from watchdog.observers import Observer
from watchdog.events import FileSystemEventHandler

_log = logging.getLogger("nif_editor.nif_watcher")


class _MultiNifHandler(FileSystemEventHandler):
    """Watchdog handler that tracks modification times for watched NIF files."""

    def __init__(self):
        self._watched_files: set[str] = set()  # normalized paths
        self._last_event_times: dict[str, float] = {}  # path → timestamp
        self._ignored_signatures: dict[str, tuple[int, int]] = {}

    @staticmethod
    def _file_signature(path: str) -> tuple[int, int] | None:
        try:
            stat = os.stat(path)
        except OSError:
            return None
        return (stat.st_mtime_ns, stat.st_size)

    def add_file(self, path: str):
        self._watched_files.add(os.path.normpath(os.path.abspath(path)))

    def remove_file(self, path: str):
        norm = os.path.normpath(os.path.abspath(path))
        self._watched_files.discard(norm)
        self._last_event_times.pop(norm, None)
        self._ignored_signatures.pop(norm, None)

    def mark_saved(self, path: str):
        norm = os.path.normpath(os.path.abspath(path))
        signature = self._file_signature(norm)
        if signature is not None:
            self._ignored_signatures[norm] = signature
        self._last_event_times.pop(norm, None)

    def on_modified(self, event):
        if event.is_directory:
            return
        norm = os.path.normpath(os.path.abspath(event.src_path))
        if norm in self._watched_files:
            signature = self._file_signature(norm)
            ignored_signature = self._ignored_signatures.get(norm)
            if ignored_signature is not None:
                if signature == ignored_signature:
                    self._last_event_times.pop(norm, None)
                    return
                self._ignored_signatures.pop(norm, None)
            self._last_event_times[norm] = time.time()


class NifFileWatcher:
    """Watches NIF files for external saves and reloads the editor scene.

    Supports multiple sessions — call watch_session() for each NIF file,
    unwatch_session() when detaching. check_reload() returns changed paths.
    """

    DEBOUNCE_SEC = 0.5

    def __init__(self):
        self._observer: Observer | None = None
        self._handler: _MultiNifHandler | None = None
        self._watches: dict[str, object] = {}  # dir_path → ObservedWatch
        self._started = False

    def _ensure_observer(self):
        """Create and start the observer if not already running."""
        if not self._started:
            self._handler = _MultiNifHandler()
            self._observer = Observer()
            self._observer.start()
            self._started = True

    def start_watching(self, path: str):
        """Legacy single-file API — delegates to watch_session."""
        self.watch_session(path)

    def watch_session(self, session_file_path: str):
        """Add a file to the watch set."""
        self._ensure_observer()
        path = os.path.normpath(os.path.abspath(session_file_path))
        dir_path = str(Path(path).parent)

        self._handler.add_file(path)

        if dir_path not in self._watches:
            watch = self._observer.schedule(self._handler, dir_path, recursive=False)
            self._watches[dir_path] = watch
            _log.info("Watching directory for NIF changes: %s", dir_path)

        _log.info("Watching NIF file: %s", path)

    def mark_saved(self, session_file_path: str):
        """Treat the current on-disk state as editor-owned, not external."""
        if self._handler:
            self._handler.mark_saved(session_file_path)

    def unwatch_session(self, session_file_path: str, registry=None):
        """Remove a file from the watch set.

        Only removes directory watch if no other sessions use that directory.
        """
        path = os.path.normpath(os.path.abspath(session_file_path))
        dir_path = str(Path(path).parent)

        if self._handler:
            self._handler.remove_file(path)

        # Check if any other session still needs this directory
        other_dirs = set()
        if registry:
            other_dirs = {
                str(Path(os.path.normpath(os.path.abspath(s.file_path))).parent)
                for s in registry.all_sessions()
                if os.path.normpath(os.path.abspath(s.file_path)) != path
            }

        if dir_path not in other_dirs and dir_path in self._watches:
            if self._observer:
                self._observer.unschedule(self._watches.pop(dir_path))
            _log.info("Unwatched directory: %s", dir_path)

    def stop_watching(self):
        """Stop all watching."""
        if self._observer:
            self._observer.stop()
            self._observer.join(timeout=2)
            self._observer = None
        self._handler = None
        self._watches.clear()
        self._started = False

    def check_reload(self) -> str | None:
        """Call each frame. Returns the first changed file path if detected."""
        if not self._handler:
            return None

        now = time.time()
        for path, event_time in list(self._handler._last_event_times.items()):
            elapsed = now - event_time
            if elapsed >= self.DEBOUNCE_SEC:
                # Consume the event
                del self._handler._last_event_times[path]
                return path

        return None
