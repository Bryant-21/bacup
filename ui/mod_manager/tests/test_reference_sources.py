import shutil
import threading

import pytest

from creation_lib.mod_manager import reference_sources as references
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2


def files(root):
    return {str(path.relative_to(root)): path.read_bytes() for path in root.rglob("*") if path.is_file()}


@pytest.fixture
def target(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    preview = inspect_instance(source)
    library.import_mo2(preview)
    profile = library.profiles()[0].id
    row = next(row for row in library.rows(profile) if row.kind == "external")
    replacement = tmp_path / "Relocated source"
    shutil.copytree(row.source, replacement)
    return library, profile, row, preview, source, replacement


def plan(target):
    library, profile, row, _, _, replacement = target
    return references.review(library, profile, row.id, str(replacement))


def test_shared_relink_repeat_import_reopen_and_guarded_undo_preserve_payloads(target):
    library, profile, row, preview, source, replacement = target
    old_files, new_files = files(source), files(replacement)
    reviewed = plan(target)
    assert reviewed.files == len(new_files) and reviewed.profiles == 2
    references.apply(library, reviewed, accept_identity=True)
    library.import_mo2(preview, reconcile=True)
    reopened = Library(library.root)
    for p in reopened.profiles():
        current = next(item for item in reopened.rows(p.id) if item.id == row.id)
        assert current.source == str(replacement) and current.name == row.name and current.version == row.version
    assert row.id in references.undoable(reopened, profile)
    references.undo(reopened, profile, row.id)
    assert row.id not in references.undoable(reopened, profile)
    assert all(next(item for item in reopened.rows(p.id) if item.id == row.id).source == row.source for p in reopened.profiles())
    assert files(source) == old_files and files(replacement) == new_files


def test_missing_old_source_is_repairable_and_undo_restores_its_metadata_without_io(target):
    library, profile, row, _, _, _ = target
    with library._db() as db:
        db.execute("UPDATE mods SET source='' WHERE id=?", (row.id,))
    references.apply(library, plan(target), accept_identity=True)
    references.undo(library, profile, row.id)
    assert next(item for item in library.rows(profile) if item.id == row.id).source == ""


@pytest.mark.parametrize("change", ["metadata", "membership", "file", "new-file", "directory"])
def test_stale_reference_or_candidate_preserves_later_changes(target, change):
    library, profile, row, _, _, replacement = target
    reviewed = plan(target)
    if change in {"metadata", "membership"}:
        with library._db() as db:
            if change == "metadata":
                db.execute("UPDATE mods SET source='later' WHERE id=?", (row.id,))
            else:
                db.execute("DELETE FROM entries WHERE profile_id=? AND mod_id=?", (profile, row.id))
    elif change == "file":
        (replacement / "meta.ini").write_bytes(b"later fixture content")
    elif change == "new-file":
        (replacement / "later.txt").write_bytes(b"new fixture")
    else:
        (replacement / "empty new directory").mkdir()
    before, candidate = library.rows(profile), files(replacement)
    with pytest.raises(ManagerError):
        references.apply(library, reviewed, accept_identity=True)
    assert library.rows(profile) == before and files(replacement) == candidate


@pytest.mark.parametrize("phase", ["apply", "undo"])
@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_relink_and_undo_cancel_failure_roll_back_metadata(target, monkeypatch, phase, outcome):
    library, profile, row, _, source, replacement = target
    reviewed = plan(target)
    if phase == "undo":
        references.apply(library, reviewed, accept_identity=True)
    before, old_files, candidate = library.rows(profile), files(source), files(replacement)
    cancel = threading.Event()
    def stop():
        if outcome == "failed":
            raise OSError("generated metadata relink failure")
        cancel.set()
    monkeypatch.setattr(references, "_checkpoint", stop)
    with pytest.raises(Cancelled if outcome == "cancelled" else OSError):
        if phase == "apply":
            references.apply(library, reviewed, accept_identity=True, cancel=cancel)
        else:
            references.undo(library, profile, row.id, cancel)
    assert library.rows(profile) == before and files(source) == old_files and files(replacement) == candidate


def test_acknowledgement_library_boundaries_owned_kind_and_runtime_guards(target, tmp_path, monkeypatch):
    library, profile, row, _, _, _ = target
    with pytest.raises(ManagerError, match="Confirm"):
        references.apply(library, plan(target))
    other = Library(tmp_path / "other", create=True)
    for folder in (library.root, library.root / "mods", library.root.parent, other.root / "mods"):
        with pytest.raises(ManagerError, match="manager-owned"):
            references.review(library, profile, row.id, str(folder))
    with library._db() as db:
        db.execute("UPDATE mods SET kind='owned' WHERE id=?", (row.id,))
    with pytest.raises(ManagerError, match="referenced source"):
        plan(target)
    with library._db() as db:
        db.execute("UPDATE mods SET kind='external' WHERE id=?", (row.id,))
    reviewed = plan(target)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.active_sessions", lambda _: [("fixture", {"state": "running"})])
    with pytest.raises(ManagerError, match="runtime"):
        references.apply(library, reviewed, accept_identity=True)


def test_nested_reparse_candidate_and_empty_folder_are_rejected(target, tmp_path):
    from .test_mo2 import junction
    library, profile, row, _, _, replacement = target
    empty = tmp_path / "empty"
    empty.mkdir()
    with pytest.raises(ManagerError, match="containing mod files"):
        references.review(library, profile, row.id, str(empty))
    junction(replacement / "nested-link", empty)
    with pytest.raises(ManagerError, match="reparse|Links"):
        plan(target)


def test_later_source_edit_or_owned_conversion_blocks_undo(target):
    library, profile, row, _, _, _ = target
    references.apply(library, plan(target), accept_identity=True)
    with library._db() as db:
        db.execute("UPDATE mods SET source='later' WHERE id=?", (row.id,))
    before = library.rows(profile)
    with pytest.raises(ManagerError, match="Undo is blocked"):
        references.undo(library, profile, row.id)
    assert library.rows(profile) == before
