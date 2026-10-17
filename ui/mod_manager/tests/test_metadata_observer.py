import threading

import pytest

from creation_lib.mod_manager import metadata_observer as observe
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2


@pytest.fixture
def observed(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    path = source / "mods" / "Base" / "meta.ini"
    return library, source, path


def test_persistent_changes_repeat_import_stale_dismiss_and_return_to_baseline(observed):
    library, source, path = observed
    before = library.rows(library.profiles()[0].id)
    original = path.read_bytes()
    assert observe.scan(library).count == 0
    path.write_bytes(original + b"\nversion=changed\n")
    changed = observe.scan(library)
    notice = next(n for n in changed.notices if n.path == str(path))
    assert notice.change == "changed" and str(path) in changed.changed_paths
    library.import_mo2(inspect_instance(source), reconcile=True)
    reopened = Library(library.root)
    assert observe.scan(reopened).count == 1 and reopened.rows(library.profiles()[0].id) == before
    path.write_bytes(original + b"\nnotes=changed again\n")
    with pytest.raises(ManagerError, match="changed again"):
        observe.dismiss(reopened, notice)
    latest = observe.scan(reopened).notices[0]
    observe.dismiss(reopened, latest)
    assert observe.scan(reopened).count == 0
    assert path.read_bytes() == original + b"\nnotes=changed again\n"


def test_missing_categories_oversized_cancel_and_relink_skip(observed):
    library, source, path = observed
    initial = observe.scan(library)
    mapping = source / "categories.dat"
    mapping.unlink()
    removed = observe.scan(library)
    assert any(n.kind == "categories" and n.change == "removed" for n in removed.notices)
    path.write_bytes(b"x" * (observe.FILE_LIMIT + 1))
    assert any(n.issue and n.change == "unavailable" for n in observe.scan(library).notices)
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        observe.scan(library, cancel=cancel)
    old = next(t for t in initial.targets if t.path == str(path))
    with library._db() as db:
        db.execute("UPDATE mods SET source=? WHERE id=?", (str(source / "other"), old.owner_id))
    path.write_bytes(b"outside later reference")
    assert not observe.scan(library, watched=(old,)).changed_paths


def test_rotating_batch_bound_and_strong_digest_same_size_time(observed, monkeypatch):
    import os
    library, _, path = observed
    monkeypatch.setattr(observe, "BATCH_FILES", 1)
    index = None
    for _ in range(4):
        index = observe.scan(library, index.targets if index else None, index.cursor if index else 0, index.visited if index else 0)
    assert index.visited == len(index.targets) == 4
    target = next(t for t in index.targets if t.path == str(path))
    before, stamp = observe.fingerprint(target)[0], path.stat()
    data = path.read_bytes()
    path.write_bytes(data.replace(b"fixture", b"changed"))
    os.utime(path, ns=(stamp.st_atime_ns, stamp.st_mtime_ns))
    assert observe.fingerprint(target)[0] != before

