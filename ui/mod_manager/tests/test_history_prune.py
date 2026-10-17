from pathlib import Path
import json
import os
import subprocess
import threading

import pytest

from creation_lib.mod_manager import history_prune as prune
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.transactions import CommittedTransaction, digest
from .test_transactions import setup_owned


def fixture(tmp_path):
    library, profile, mod, replacement = setup_owned(tmp_path)
    (library.root / "mods" / mod / "empty" / "nested").mkdir(parents=True)
    with_package = Package(replacement)
    try:
        library.reinstall(library.review_owned(mod), with_package, with_package.plain_plan())
    finally:
        with_package.close()
    entry = library.history()[0]
    return library, profile, mod, entry


def test_removes_one_reviewed_backup_keeps_installed_and_quarantine_content(tmp_path):
    library, profile, mod, entry = fixture(tmp_path)
    parent = Path(entry["path"]).parent
    (parent / "uncommitted-new").mkdir()
    (parent / "uncommitted-new" / "keep.txt").write_text("keep")
    plan = prune.review(library, entry["id"])
    assert plan.files == 1 and plan.bytes == 8
    assert prune.remove(library, plan) == entry["id"]
    assert not Path(entry["path"]).exists() and not library.history()
    assert (library.root / "mods" / mod / "file.txt").read_text() == "replacement"
    assert (parent / "uncommitted-new" / "keep.txt").read_text() == "keep"
    assert library.rows(profile)[0].id == mod
    assert not list((library.root / ".history-prune").iterdir())
    with pytest.raises(ManagerError, match="no longer"):
        prune.remove(library, plan)


@pytest.mark.parametrize("change", ["contents", "added", "empty-directory", "metadata"])
def test_changed_review_preserves_every_backup(tmp_path, change):
    library, profile, mod, entry = fixture(tmp_path)
    plan = prune.review(library, entry["id"])
    path = Path(entry["path"])
    if change == "contents":
        (path / "file.txt").write_text("modified")
    elif change == "added":
        (path / "new.txt").write_text("new")
    elif change == "empty-directory":
        (path / "new empty").mkdir()
    else:
        with library._db() as db:
            db.execute("UPDATE operations SET created='changed' WHERE id=?", (entry["id"],))
    with pytest.raises(ManagerError, match="changed after review"):
        prune.remove(library, plan)
    assert path.is_dir() and len(library.history()) == 1
    assert (library.root / "mods" / mod).is_dir()


@pytest.mark.parametrize("checkpoint", ["receipt", "moved", "metadata"])
@pytest.mark.parametrize("outcome", ["cancel", "failure"])
def test_precommit_cancel_and_failures_restore_complete_backup(tmp_path, monkeypatch, checkpoint, outcome):
    library, profile, mod, entry = fixture(tmp_path)
    plan, cancel = prune.review(library, entry["id"]), threading.Event()
    def stop(name):
        if name == checkpoint:
            if outcome == "cancel":
                cancel.set()
            else:
                raise OSError("disposable backup deletion failure")
    monkeypatch.setattr(prune, "_checkpoint", stop)
    with pytest.raises(Cancelled if outcome == "cancel" else OSError):
        prune.remove(library, plan, cancel)
    assert (Path(entry["path"]) / "file.txt").read_text() == "original"
    assert (Path(entry["path"]) / "empty" / "nested").is_dir()
    assert len(library.history()) == 1 and not list((library.root / ".history-prune").iterdir())


@pytest.mark.parametrize("checkpoint", ["committed", "file-removed"])
def test_committed_interruption_finishes_on_reopen(tmp_path, monkeypatch, checkpoint):
    library, profile, mod, entry = fixture(tmp_path)
    plan = prune.review(library, entry["id"])
    def stop(name):
        if name == checkpoint:
            raise OSError("disposable committed cleanup failure")
    monkeypatch.setattr(prune, "_checkpoint", stop)
    with pytest.raises(CommittedTransaction, match="deletion committed"):
        prune.remove(library, plan)
    assert not library.history() and list((library.root / ".history-prune").glob("*.json"))
    monkeypatch.setattr(prune, "_checkpoint", lambda name: None)
    reopened = Library(library.root)
    assert "Finished a committed deletion" in reopened.recovery_report[-1]
    assert not list((library.root / ".history-prune").iterdir())
    assert (library.root / "mods" / mod / "file.txt").read_text() == "replacement"


@pytest.mark.parametrize("change", ["changed", "added", "directory", "receipt"])
def test_pending_cleanup_preserves_changed_or_unexpected_content(tmp_path, monkeypatch, change):
    library, profile, mod, entry = fixture(tmp_path)
    def stop(name):
        if name == "committed":
            raise OSError("disposable cleanup interruption")
    monkeypatch.setattr(prune, "_checkpoint", stop)
    with pytest.raises(CommittedTransaction):
        prune.remove(library, prune.review(library, entry["id"]))
    payload = library.root / ".history-prune" / entry["id"] / "payload"
    if change == "changed":
        (payload / "file.txt").write_text("changed")
    elif change == "added":
        (payload / "added.txt").write_text("preserve")
    elif change == "directory":
        (payload / "unexpected").mkdir()
    else:
        receipt = payload.parent.with_suffix(".json")
        receipt.write_text(receipt.read_text().replace('"version": 1', '"version": 999'))
    monkeypatch.setattr(prune, "_checkpoint", lambda name: None)
    with pytest.raises(ManagerError):
        prune.recover(library)
    assert payload.is_dir() and (payload / "file.txt").is_file()
    assert (library.root / "mods" / mod).is_dir()


def test_runtime_guard_and_cancel_before_receipt_are_read_only(tmp_path, monkeypatch):
    library, profile, mod, entry = fixture(tmp_path)
    plan = prune.review(library, entry["id"])
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        prune.remove(library, plan, cancel)
    monkeypatch.setattr(library, "_runtime_write_guard", lambda: (_ for _ in ()).throw(ManagerError("runtime active")))
    with pytest.raises(ManagerError, match="runtime active"):
        prune.remove(library, plan)
    assert Path(entry["path"]).is_dir() and not (library.root / ".history-prune").exists()


@pytest.mark.parametrize("move", [False, True])
def test_reopen_uncommitted_receipt_restores_intact_backup(tmp_path, move):
    library, profile, mod, entry = fixture(tmp_path)
    with library._db() as db:
        plan, operation, manifest, directories = prune._review(library, db, entry["id"])
    source, trash, journal = prune._paths(library, entry["id"])
    prune._write(journal, {"version": 1, "library": library.library_id, "operation": operation,
                           "manifest": manifest, "directories": directories, "token": plan.token})
    if move:
        trash.parent.mkdir()
        source.rename(trash)
    reopened = Library(library.root)
    assert "uncommitted recovery-backup deletion" in reopened.recovery_report[-1]
    assert (source / "file.txt").read_text() == "original" and len(reopened.history()) == 1
    assert not list((library.root / ".history-prune").iterdir())


def test_pending_receipt_path_traversal_is_rejected(tmp_path, monkeypatch):
    library, profile, mod, entry = fixture(tmp_path)
    def stop(name):
        if name == "committed":
            raise OSError("disposable interruption")
    monkeypatch.setattr(prune, "_checkpoint", stop)
    with pytest.raises(CommittedTransaction):
        prune.remove(library, prune.review(library, entry["id"]))
    journal = library.root / ".history-prune" / (entry["id"] + ".json")
    record = json.loads(journal.read_text())
    record["manifest"]["../outside.txt"] = record["manifest"].pop("file.txt")
    record["token"] = digest((record["operation"], record["manifest"], record["directories"]))
    journal.write_text(json.dumps(record))
    outside = library.root / ".history-prune" / entry["id"] / "outside.txt"
    outside.write_text("must remain")
    with pytest.raises(ManagerError, match="Invalid recovery deletion receipt"):
        prune.recover(library)
    assert outside.read_text() == "must remain"


def test_backup_nested_link_is_never_followed(tmp_path):
    library, profile, mod, entry = fixture(tmp_path)
    target = tmp_path / "disposable outside"
    target.mkdir()
    (target / "keep.txt").write_text("do not follow")
    link = Path(entry["path"]) / "linked"
    if os.name == "nt":
        subprocess.run(["cmd.exe", "/c", "mklink", "/J", str(link), str(target)], check=True, capture_output=True)
    else:
        link.symlink_to(target, target_is_directory=True)
    try:
        with pytest.raises(ManagerError, match="Links and Windows reparse"):
            prune.review(library, entry["id"])
        assert (target / "keep.txt").read_text() == "do not follow"
    finally:
        link.rmdir() if os.name == "nt" else link.unlink()


@pytest.mark.parametrize("failure", ["bound", "sync"])
def test_receipt_publication_failure_preserves_payload_and_removes_temporary(tmp_path, monkeypatch, failure):
    library, profile, mod, entry = fixture(tmp_path)
    plan = prune.review(library, entry["id"])
    if failure == "bound":
        monkeypatch.setattr(prune, "LIMIT", 32)
    else:
        def fail_sync(descriptor):
            raise OSError("disposable receipt sync failure")
        monkeypatch.setattr(os, "fsync", fail_sync)
    with pytest.raises(ManagerError if failure == "bound" else OSError):
        prune.remove(library, plan)
    assert (Path(entry["path"]) / "file.txt").read_text() == "original"
    assert len(library.history()) == 1
    base = library.root / ".history-prune"
    assert not base.exists() or not list(base.iterdir())


def test_corrupt_operation_metadata_stops_before_receipt_or_payload_move(tmp_path):
    library, profile, mod, entry = fixture(tmp_path)
    with library._db() as db:
        db.execute("UPDATE operations SET snapshot='[]' WHERE id=?", (entry["id"],))
    with pytest.raises(ManagerError, match="metadata is malformed"):
        prune.review(library, entry["id"])
    assert (Path(entry["path"]) / "file.txt").read_text() == "original"
