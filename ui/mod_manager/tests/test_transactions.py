import json
from pathlib import Path
import sqlite3
import subprocess
import sys
import threading

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.transactions import CommittedTransaction, OwnedReview
from .fixtures import make_mo2
from .test_library import digest


def setup_owned(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile = library.create_profile("Main")
    source = tmp_path / "original"
    source.mkdir()
    (source / "file.txt").write_text("original")
    package = Package(source)
    mod = library.install(package, package.plain_plan(), "Fixture", profile)
    package.close()
    replacement = tmp_path / "replacement"
    replacement.mkdir()
    (replacement / "file.txt").write_text("replacement")
    (replacement / "extra.txt").write_text("extra")
    return library, profile, mod, replacement


def test_reinstall_preserves_all_profile_customization_and_modified_payload(tmp_path):
    library, profile, mod, replacement = setup_owned(tmp_path)
    clone = library.create_profile("Clone", profile)
    library.customize(clone, mod, enabled=False, group="Custom/Sub", tags="favorite", notes="keep notes")
    owned = Path(library.rows(profile)[0].source)
    (owned / "file.txt").write_text("user edit")
    (owned / "added.ini").write_text("user data")
    review = library.review_owned(mod)
    assert len(review.profiles) == 2 and len(review.changed) == 2
    before = {identifier: library.rows(identifier)[0] for identifier in (profile, clone)}
    with_package = Package(replacement)
    try:
        assert library.reinstall(review, with_package, with_package.plain_plan()) == mod
    finally:
        with_package.close()
    assert (owned / "file.txt").read_text() == "replacement"
    assert not (owned / "added.ini").exists()
    for identifier in (profile, clone):
        current = library.rows(identifier)[0]
        assert (current.id, current.enabled, current.group, current.priority, current.tags, current.notes) == (mod, before[identifier].enabled, before[identifier].group, before[identifier].priority, "favorite", "keep notes")
    history = library.history()[0]
    assert (Path(history["path"]) / "added.ini").read_text() == "user data"
    library.restore(history["id"], library.review_owned(mod))
    assert (owned / "file.txt").read_text() == "user edit"
    assert (owned / "added.ini").read_text() == "user data"
    assert len(library.history()) == 2


def test_uninstall_restore_profile_references_and_relative_order(tmp_path):
    library, profile, mod, replacement = setup_owned(tmp_path)
    clone = library.create_profile("Clone", profile)
    library.customize(clone, mod, enabled=False, group="Folder")
    package = Package(replacement)
    other = library.install(package, package.plain_plan(), "Other", profile)
    package.close()
    review = library.review_owned(mod)
    operation = library.uninstall(review)
    assert [row.id for row in library.rows(profile)] == [other]
    assert library.rows(clone) == []
    assert not (library.root / "mods" / mod).exists()
    assert Path(library.history()[0]["path"]).is_dir()
    assert library.restore(operation) == mod
    assert [row.id for row in library.rows(profile)] == [mod, other]
    assert not library.rows(clone)[0].enabled and library.rows(clone)[0].group == "Folder"
    with pytest.raises(ManagerError, match="Review the current"):
        library.restore(operation)


def test_restore_does_not_recreate_deleted_profiles(tmp_path):
    library, profile, mod, _ = setup_owned(tmp_path)
    operation = library.uninstall(library.review_owned(mod))
    with library._db() as db:
        db.execute("DELETE FROM profiles WHERE id=?", (profile,))
    library.restore(operation)
    assert library.profiles() == []
    with library._db() as db:
        assert db.execute("SELECT COUNT(*) FROM mods").fetchone()[0] == 1
        assert db.execute("SELECT COUNT(*) FROM entries").fetchone()[0] == 0


@pytest.mark.parametrize("change", ["content", "same_size_restored_mtime", "add", "remove", "profile", "notes"])
def test_stale_owned_review_refuses_uninstall_without_changing_content(tmp_path, change):
    library, profile, mod, _ = setup_owned(tmp_path)
    review = library.review_owned(mod)
    owned = library.root / "mods" / mod
    if change in {"content", "same_size_restored_mtime"}:
        import os
        file = owned / "file.txt"
        info = file.stat()
        file.write_text("modified" if change == "same_size_restored_mtime" else "different content")
        if change == "same_size_restored_mtime":
            os.utime(file, ns=(info.st_atime_ns, info.st_mtime_ns))
    elif change == "add":
        (owned / "added.txt").write_text("user data")
    elif change == "remove":
        (owned / "file.txt").unlink()
    elif change == "profile":
        library.create_profile("New reference", profile)
    else:
        library.customize(profile, mod, notes="changed after review")
    before = digest(owned)
    with pytest.raises(ManagerError, match="changed after review"):
        library.uninstall(review)
    assert digest(owned) == before and library.rows(profile)[0].id == mod
    assert not any((library.root / ".transactions").glob("*.json"))


def test_cancelled_reinstall_and_metadata_failure_restore_original(tmp_path):
    library, profile, mod, replacement = setup_owned(tmp_path)
    owned = library.root / "mods" / mod
    before = digest(owned)
    package = Package(replacement)
    cancel = threading.Event()
    with pytest.raises(Cancelled):
        library.reinstall(library.review_owned(mod), package, package.plain_plan(), cancel, lambda *args: cancel.set())
    assert digest(owned) == before
    with library._db() as db:
        db.execute("CREATE TRIGGER reject_reinstall BEFORE UPDATE ON mods BEGIN SELECT RAISE(FAIL,'fixture failure'); END")
    with pytest.raises(sqlite3.IntegrityError, match="fixture failure"):
        library.reinstall(library.review_owned(mod), package, package.plain_plan())
    package.close()
    assert digest(owned) == before and library.rows(profile)[0].id == mod
    assert not any((library.root / ".staging").iterdir())


def test_uninstall_database_failure_restores_folder_and_references(tmp_path):
    library, profile, mod, _ = setup_owned(tmp_path)
    with library._db() as db:
        db.execute("CREATE TRIGGER reject_uninstall BEFORE DELETE ON mods BEGIN SELECT RAISE(FAIL,'fixture failure'); END")
    with pytest.raises(sqlite3.IntegrityError):
        library.uninstall(library.review_owned(mod))
    assert library.rows(profile)[0].id == mod
    assert (library.root / "mods" / mod / "file.txt").read_text() == "original"


def test_imported_sources_and_forged_owned_paths_are_never_adopted(tmp_path):
    source = make_mo2(tmp_path)
    before = digest(source)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    row = library.rows(library.profiles()[0].id)[1]
    with pytest.raises(ManagerError, match="read-only"):
        library.review_owned(row.id)
    fake = OwnedReview(row.id, row.name, "fake", (), (), 0, 0)
    with pytest.raises(ManagerError, match="read-only"):
        library.uninstall(fake)
    with library._db() as db:
        db.execute("UPDATE mods SET kind='owned' WHERE id=?", (row.id,))
    with pytest.raises(ManagerError, match="path does not match"):
        library.review_owned(row.id)
    assert digest(source) == before


def test_exclusive_library_lock_prevents_recovery_of_an_active_operation(tmp_path):
    library, _, _, _ = setup_owned(tmp_path)
    with library.owned.lock():
        with pytest.raises(ManagerError, match="Another operation"):
            Library(library.root)


def test_uncertain_post_commit_error_preserves_committed_payload(tmp_path, monkeypatch):
    from contextlib import contextmanager
    library, profile, _, replacement = setup_owned(tmp_path)
    original = library._db
    fail = True
    @contextmanager
    def failing_commit():
        nonlocal fail
        with original() as db:
            yield db
        with original() as db:
            committed = db.execute("SELECT COUNT(*) FROM operations").fetchone()[0] == 2
        if fail and committed:
            fail = False
            raise OSError("fixture error after committed database context")
    monkeypatch.setattr(library, "_db", failing_commit)
    package = Package(replacement)
    with pytest.raises(CommittedTransaction, match="change committed"):
        library.install(package, package.plain_plan(), "Committed", profile)
    package.close()
    reopened = Library(library.root)
    assert len(reopened.rows(profile)) == 2 and reopened.recovery_report
    row = next(row for row in reopened.rows(profile) if row.name == "Committed")
    assert (Path(row.source) / "file.txt").read_text() == "replacement"


@pytest.mark.parametrize("operation", ["reinstall", "uninstall"])
def test_cancellation_immediately_before_commit_rolls_back(tmp_path, monkeypatch, operation):
    library, profile, mod, replacement = setup_owned(tmp_path)
    cancel = threading.Event()
    monkeypatch.setattr(library.owned, "_checkpoint", lambda name: cancel.set() if name == "before-commit" else None)
    with pytest.raises(Cancelled):
        if operation == "uninstall":
            library.uninstall(library.review_owned(mod), cancel)
        else:
            package = Package(replacement)
            try:
                library.reinstall(library.review_owned(mod), package, package.plain_plan(), cancel)
            finally:
                package.close()
    assert library.rows(profile)[0].id == mod
    assert (library.root / "mods" / mod / "file.txt").read_text() == "original"


def test_restore_rejects_reparse_payload_without_following_it(tmp_path):
    from .test_mo2 import junction
    library, profile, mod, _ = setup_owned(tmp_path)
    operation = library.uninstall(library.review_owned(mod))
    payload = Path(library.history()[0]["path"])
    preserved = payload.with_name("preserved-payload")
    payload.rename(preserved)
    junction(payload, preserved)
    with pytest.raises(ManagerError, match="reparse"):
        library.restore(operation)
    assert library.rows(profile) == [] and (preserved / "file.txt").read_text() == "original"


def test_changed_backup_after_restore_review_is_preserved_and_refused(tmp_path):
    library, profile, mod, _ = setup_owned(tmp_path)
    operation = library.uninstall(library.review_owned(mod))
    entry, review = library.review_restore(operation)
    file = Path(entry["path"]) / "file.txt"
    file.write_text("modified backup")
    with pytest.raises(ManagerError, match="backup changed after review"):
        library.restore(operation, review, backup_token=entry["backup_token"])
    assert library.rows(profile) == [] and file.read_text() == "modified backup"


def test_owned_recovery_preserves_user_script_files_as_inert_data(tmp_path):
    library, profile, mod, _ = setup_owned(tmp_path)
    owned = library.root / "mods" / mod
    (owned / "fomod").mkdir()
    (owned / "fomod/script.cs").write_text("fixture code must stay inert")
    operation = library.uninstall(library.review_owned(mod))
    entry, review = library.review_restore(operation)
    library.restore(operation, review, backup_token=entry["backup_token"])
    assert (owned / "fomod/script.cs").read_text() == "fixture code must stay inert"
    assert library.rows(profile)[0].id == mod


def test_restore_preserves_empty_directories_and_empty_owned_mods(tmp_path):
    library, profile, mod, _ = setup_owned(tmp_path)
    owned = library.root / "mods" / mod
    (owned / "file.txt").unlink()
    (owned / "user-empty/nested").mkdir(parents=True)
    operation = library.uninstall(library.review_owned(mod))
    entry, review = library.review_restore(operation)
    library.restore(operation, review, backup_token=entry["backup_token"])
    assert (owned / "user-empty/nested").is_dir()
    assert library.rows(profile)[0].id == mod


def test_restore_detects_same_size_and_timestamp_change_during_copy(tmp_path, monkeypatch):
    import os
    library, profile, mod, _ = setup_owned(tmp_path)
    operation = library.uninstall(library.review_owned(mod))
    entry, review = library.review_restore(operation)
    copy = library._copy_plan
    def change_then_copy(sources, destination, cancel=None, progress=None):
        file = Path(entry["path"]) / "file.txt"
        info = file.stat()
        file.write_text("modified")
        os.utime(file, ns=(info.st_atime_ns, info.st_mtime_ns))
        return copy(sources, destination, cancel, progress)
    monkeypatch.setattr(library, "_copy_plan", change_then_copy)
    with pytest.raises(ManagerError, match="changed while copying"):
        library.restore(operation, review, backup_token=entry["backup_token"])
    assert not library.rows(profile)
    assert (Path(entry["path"]) / "file.txt").read_text() == "modified"


@pytest.mark.parametrize("operation", ["uninstall", "reinstall"])
def test_changed_content_after_move_rolls_back_without_losing_edits(tmp_path, monkeypatch, operation):
    library, profile, mod, replacement = setup_owned(tmp_path)
    review = library.review_owned(mod)
    def edit_moved_payload(name):
        if name == "old-moved":
            file = next((library.root / ".history").glob("*/payload/file.txt"))
            file.write_text("edit made during transaction")
    monkeypatch.setattr(library.owned, "_checkpoint", edit_moved_payload)
    with pytest.raises(ManagerError, match="changed during"):
        if operation == "uninstall":
            library.uninstall(review)
        else:
            package = Package(replacement)
            try:
                library.reinstall(review, package, package.plain_plan())
            finally:
                package.close()
    assert library.rows(profile)[0].id == mod
    assert (library.root / "mods" / mod / "file.txt").read_text() == "edit made during transaction"


@pytest.mark.parametrize("operation,checkpoint", [
    *(('install', stage) for stage in ('journal', 'prepared', 'placed', 'before-commit', 'committed')),
    *(('reinstall', stage) for stage in ('journal', 'prepared', 'old-moved', 'placed', 'before-commit', 'committed')),
    *(('uninstall', stage) for stage in ('journal', 'old-moved', 'before-commit', 'committed')),
    *(('restore', stage) for stage in ('journal', 'prepared', 'placed', 'before-commit', 'committed')),
])
def test_actual_process_termination_recovers_owned_transactions(tmp_path, operation, checkpoint):
    library, profile, mod, replacement = setup_owned(tmp_path)
    history = ""
    if operation == "install":
        library.uninstall(library.review_owned(mod))
    elif operation == "restore":
        history = library.uninstall(library.review_owned(mod))
    args = {"library": str(library.root), "operation": operation, "checkpoint": checkpoint,
            "profile": profile, "mod_id": mod, "package": str(replacement), "history": history}
    process = subprocess.run([sys.executable, "-m", "ui.mod_manager.tests.crash_worker", json.dumps(args)],
                             capture_output=True, timeout=20)
    assert process.returncode == 91, process.stderr.decode(errors="replace")
    reopened = Library(library.root)
    assert reopened.recovery_report
    assert not any((reopened.root / ".transactions").glob("*.json"))
    assert not any((reopened.root / ".staging").iterdir())
    rows = reopened.rows(profile)
    committed = checkpoint == "committed"
    if operation == "install":
        assert len(rows) == int(committed)
        if committed:
            assert (Path(rows[0].source) / "file.txt").read_text() == "replacement"
    elif operation == "reinstall":
        assert len(rows) == 1 and rows[0].id == mod
        assert (Path(rows[0].source) / "file.txt").read_text() == ("replacement" if committed else "original")
    elif operation == "uninstall":
        assert len(rows) == int(not committed)
        if rows:
            assert (Path(rows[0].source) / "file.txt").read_text() == "original"
    else:
        assert len(rows) == int(committed)
        if committed:
            assert rows[0].id == mod and (Path(rows[0].source) / "file.txt").read_text() == "original"
    assert not Library(library.root).recovery_report


@pytest.mark.parametrize("change", ["mod_id", "library", "kind", "identity", "oversized"])
def test_malformed_or_escaping_journal_leaves_user_data_untouched(tmp_path, change):
    library, profile, mod, _ = setup_owned(tmp_path)
    record = {"format": 1, "library": library.library_id, "id": "a" * 32,
              "mod_id": mod, "kind": "reinstall", "old_present": True}
    if change == "mod_id":
        record["mod_id"] = "../../outside"
    elif change in {"library", "kind"}:
        record[change] = "foreign"
    elif change == "identity":
        record["id"] = "b" * 32
    journal = library.root / ".transactions" / ("a" * 32 + ".json")
    journal.write_text(json.dumps(record) if change != "oversized" else " " * 5000)
    before = digest(library.root / "mods")
    with pytest.raises(ManagerError):
        Library(library.root)
    assert digest(library.root / "mods") == before and library.rows(profile)[0].id == mod
