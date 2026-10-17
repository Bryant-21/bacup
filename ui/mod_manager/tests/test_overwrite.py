from pathlib import Path
import os
import threading

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.overwrite import clear, copy_mod, history, restore, review, review_restore, root
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2


@pytest.fixture
def output(tmp_path):
    library = Library(tmp_path / "library", create=True)
    source = make_mo2(tmp_path)
    library.import_mo2(inspect_instance(source))
    profile = library.profiles()[0].id
    folder = root(library, profile)
    (folder / "generated" / "empty").mkdir(parents=True)
    (folder / "generated" / "output.txt").write_bytes(b"owned generated output")
    return library, profile, folder, source


def test_copy_mod_keeps_original_then_reviewed_clear_restore_reopen(output):
    library, profile, folder, source = output
    original = {str(path): path.read_bytes() for path in source.rglob("*") if path.is_file()}
    plan = review(library, profile)
    identifier = copy_mod(library, plan, "Generated output")
    row = next(row for row in library.rows(profile) if row.id == identifier)
    assert row.name == "Generated output"
    assert (Path(row.source) / "generated/output.txt").read_bytes() == b"owned generated output"
    assert (Path(row.source) / "generated/empty").is_dir()
    assert (folder / "generated/output.txt").read_bytes() == b"owned generated output"
    with pytest.raises(ManagerError, match="already has this name"):
        copy_mod(library, plan, "Generated output")
    backup = clear(library, plan)
    assert list(folder.iterdir()) == []
    reopened = Library(library.root)
    record = history(reopened, profile)[0]
    assert record["id"] == backup and record["state"] == "complete"
    restore(reopened, backup, review_restore(reopened, profile, backup))
    assert (folder / "generated/output.txt").read_bytes() == b"owned generated output"
    assert (folder / "generated/empty").is_dir()
    assert history(reopened, profile)[0]["state"] == "restored"
    assert {str(path): path.read_bytes() for path in source.rglob("*") if path.is_file()} == original


@pytest.mark.parametrize("fault", ["file", "directory", "cancelled", "active"])
def test_failed_clear_preserves_all_original_output(output, monkeypatch, fault):
    library, profile, folder, _ = output
    plan = review(library, profile)
    cancel = threading.Event()
    if fault == "file":
        (folder / "generated/output.txt").write_bytes(b"changed after review")
    elif fault == "directory":
        (folder / "new-empty").mkdir()
    elif fault == "cancelled":
        monkeypatch.setattr("creation_lib.mod_manager.overwrite._checkpoint", lambda stage: cancel.set())
    else:
        monkeypatch.setattr("creation_lib.mod_manager.runtime.active_sessions", lambda path: [{}])
    before = {str(path): path.read_bytes() for path in folder.rglob("*") if path.is_file()}
    with pytest.raises(Cancelled if fault == "cancelled" else ManagerError):
        clear(library, plan, cancel)
    assert {str(path): path.read_bytes() for path in folder.rglob("*") if path.is_file()} == before
    assert not history(library, profile)


@pytest.mark.parametrize("moment", ["journal", "placed", "restored"])
def test_interrupted_owned_rename_recovers_without_losing_output(output, monkeypatch, moment):
    library, profile, folder, _ = output
    backup = clear(library, review(library, profile)) if moment == "restored" else None
    def fail(stage):
        if stage == moment:
            raise OSError("disposable interruption")
    monkeypatch.setattr("creation_lib.mod_manager.overwrite._checkpoint", fail)
    if moment == "restored":
        plan = review_restore(library, profile, backup)
        with pytest.raises(ManagerError, match="finalization"):
            restore(library, backup, plan)
    else:
        with pytest.raises(OSError if moment == "journal" else ManagerError):
            clear(library, review(library, profile))
    reopened = Library(library.root)
    assert not list((library.root / ".overwrite-transactions").glob("*.json"))
    if moment == "placed":
        record = history(reopened, profile)[0]
        payload = library.root / ".overwrite-history" / profile / record["id"] / "payload"
        assert not list(folder.iterdir()) and (payload / "generated/output.txt").read_bytes() == b"owned generated output"
    else:
        assert (folder / "generated/output.txt").read_bytes() == b"owned generated output"


def test_restore_collision_and_changed_backup_never_merge_or_replace(output):
    library, profile, folder, _ = output
    backup = clear(library, review(library, profile))
    plan = review_restore(library, profile, backup)
    new = folder / "new.txt"
    new.write_bytes(b"new output must survive")
    with pytest.raises(ManagerError, match="must be empty"):
        restore(library, backup, plan)
    assert new.read_bytes() == b"new output must survive"
    new.unlink()
    payload = library.root / ".overwrite-history" / profile / backup / "payload"
    (payload / "generated/output.txt").write_bytes(b"changed backup")
    with pytest.raises(ManagerError, match="changed"):
        restore(library, backup, plan)
    assert not list(folder.iterdir()) and (payload / "generated/output.txt").read_bytes() == b"changed backup"


def test_copy_staged_bytes_must_match_reviewed_sha(output, monkeypatch):
    library, profile, folder, _ = output
    plan = review(library, profile)
    original = library.owned.install
    def changed(*args, **kwargs):
        path = folder / "generated/output.txt"
        before = path.stat()
        path.write_bytes(b"changed-generated-data")
        os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns))
        assert path.stat().st_size == before.st_size
        return original(*args, **kwargs)
    monkeypatch.setattr(library.owned, "install", changed)
    with pytest.raises(ManagerError, match="changed while copying"):
        copy_mod(library, plan, "Rejected output")
    assert all(row.name != "Rejected output" for row in library.rows(profile))
    assert not list((library.root / ".staging").iterdir())
