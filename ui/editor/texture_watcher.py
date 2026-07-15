"""Watch texture files for changes and reload them in the viewport."""
import logging
import os
import time

from watchdog.observers import Observer
from watchdog.events import FileSystemEventHandler

_log = logging.getLogger("nif_editor.texture_watcher")


class TextureReloadHandler(FileSystemEventHandler):
    def __init__(self, watched_paths: dict[str, str], debounce_ms=500):
        self._watched = watched_paths  # abs_path -> texture path key
        self._debounce_ms = debounce_ms
        self._pending: dict[str, float] = {}  # path -> last_event_time

    def _mark_changed(self, path: str):
        path = os.path.normpath(path)
        if path in self._watched:
            self._pending[path] = time.time()

    def on_modified(self, event):
        if event.is_directory:
            return
        self._mark_changed(event.src_path)

    def on_created(self, event):
        if event.is_directory:
            return
        self._mark_changed(event.src_path)

    def on_moved(self, event):
        if event.is_directory:
            return
        self._mark_changed(event.dest_path)


class TextureWatcher:
    def __init__(self, ctx=None, label: str = "Texture"):
        self._observer: Observer | None = None
        self._handler: TextureReloadHandler | None = None
        self._watched: dict[str, str] = {}
        self.ctx = ctx
        self._label = label

    def start(self, texture_paths: dict[str, str]):
        """Start watching texture files. texture_paths: {abs_path: path_key}"""
        self.stop()
        self._watched = {os.path.normpath(p): k for p, k in texture_paths.items()}
        if not self._watched:
            return
        self._handler = TextureReloadHandler(self._watched)
        self._observer = Observer()
        dirs = set(os.path.dirname(p) for p in self._watched)
        for d in dirs:
            if os.path.isdir(d):
                self._observer.schedule(self._handler, d, recursive=False)
        self._observer.start()

    def check_reloads(self) -> list[str]:
        """Call each frame. Returns list of paths that were reloaded."""
        if not self._handler:
            return []
        reloaded = []
        now = time.time()
        for path, event_time in list(self._handler._pending.items()):
            if (now - event_time) < 0.5:
                continue
            del self._handler._pending[path]
            if path in self._watched:
                reloaded.append(path)
                _log.info("%s changed: %s", self._label, path)
        return reloaded

    def stop(self):
        if self._observer:
            self._observer.stop()
            self._observer.join(timeout=2)
            self._observer = None
        self._handler = None
        self._watched.clear()
