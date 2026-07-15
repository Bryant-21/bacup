"""Tests for NIF file hot-reload watcher behavior."""
from types import SimpleNamespace


class _FakeObserver:
    def __init__(self):
        self.scheduled = []
        self.unscheduled = []

    def start(self):
        pass

    def schedule(self, handler, path, recursive=False):
        watch = (handler, path, recursive)
        self.scheduled.append(watch)
        return watch

    def unschedule(self, watch):
        self.unscheduled.append(watch)

    def stop(self):
        pass

    def join(self, timeout=None):
        pass


def _event(path):
    return SimpleNamespace(is_directory=False, src_path=str(path))


def test_mark_saved_consumes_pending_save_event(tmp_path, monkeypatch):
    from ui.editor import nif_watcher

    monkeypatch.setattr(nif_watcher, "Observer", _FakeObserver)
    path = tmp_path / "saved.nif"
    path.write_bytes(b"before")

    watcher = nif_watcher.NifFileWatcher()
    watcher.DEBOUNCE_SEC = 0
    watcher.watch_session(str(path))

    path.write_bytes(b"after")
    watcher._handler.on_modified(_event(path))
    watcher.mark_saved(str(path))

    assert watcher.check_reload() is None


def test_mark_saved_ignores_late_events_from_same_save(tmp_path, monkeypatch):
    from ui.editor import nif_watcher

    monkeypatch.setattr(nif_watcher, "Observer", _FakeObserver)
    path = tmp_path / "saved.nif"
    path.write_bytes(b"saved")

    watcher = nif_watcher.NifFileWatcher()
    watcher.DEBOUNCE_SEC = 0
    watcher.watch_session(str(path))
    watcher.mark_saved(str(path))

    watcher._handler.on_modified(_event(path))
    assert watcher.check_reload() is None

    path.write_bytes(b"external")
    watcher._handler.on_modified(_event(path))
    assert watcher.check_reload() == str(path.resolve())
