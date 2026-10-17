import threading

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.organization import bulk, separator, undo
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.reference_removal import apply, review
from .fixtures import make_mo2


def setup(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    preview = inspect_instance(source)
    library.import_mo2(preview)
    profile = library.profiles()[0].id
    original = library.rows(profile)
    selected = next(row for row in original if row.kind == "external")
    return source, library, preview, profile, original, selected


def test_reviewed_bulk_reference_separator_remove_repeat_reopen_and_undo_preserve_sources(tmp_path):
    source, library, preview, profile, original, selected = setup(tmp_path)
    other = library.create_profile("Other", clone=profile)
    other_before = library.rows(other)
    group = separator(library, profile, "Generated separator")
    before = library.rows(profile)
    payloads = {str(path): path.read_bytes() for path in source.rglob("*") if path.is_file()}
    plan = review(library, profile, (selected.id, group))
    apply(library, plan)
    assert len(library.rows(profile)) == len(before) - 2
    assert library.rows(other) == other_before
    assert [row.priority for row in library.rows(profile)] == list(range(len(before) - 2))
    reopened = Library(library.root)
    reopened.import_mo2(preview, reconcile=True)
    assert selected.id not in {row.id for row in reopened.rows(profile)}
    undo(reopened, profile)
    assert reopened.rows(profile) == before
    reopened.import_mo2(preview, reconcile=True)
    assert reopened.rows(profile) == before
    assert {str(path): path.read_bytes() for path in source.rglob("*") if path.is_file()} == payloads


@pytest.mark.parametrize("fault", ["cancel-before", "cancel-after", "failure-after"])
def test_removal_cancel_or_failure_rolls_back_membership_exclusions_and_undo(tmp_path, monkeypatch, fault):
    _, library, _, profile, before, selected = setup(tmp_path)
    plan, event = review(library, profile, (selected.id,)), threading.Event()
    def checkpoint():
        if fault == "cancel-after":
            event.set()
        if fault == "failure-after":
            raise OSError("generated metadata failure")
    monkeypatch.setattr("creation_lib.mod_manager.reference_removal._checkpoint", checkpoint)
    if fault == "cancel-before":
        event.set()
    with pytest.raises((Cancelled, OSError)):
        apply(library, plan, event)
    assert library.rows(profile) == before
    with library._db() as db:
        assert db.execute("SELECT count(*) FROM reference_exclusions").fetchone()[0] == 0
        assert db.execute("SELECT count(*) FROM organization_history").fetchone()[0] == 0


@pytest.mark.parametrize("changed", ["name", "priority", "source", "membership"])
def test_stale_review_preserves_changed_metadata(tmp_path, changed):
    _, library, _, profile, _, selected = setup(tmp_path)
    plan = review(library, profile, (selected.id,))
    with library._db() as db:
        if changed == "name":
            db.execute("UPDATE mods SET name='Changed' WHERE id=?", (selected.id,))
        elif changed == "source":
            db.execute("UPDATE mods SET source='Unavailable changed fixture' WHERE id=?", (selected.id,))
        elif changed == "priority":
            db.execute("UPDATE entries SET priority=900 WHERE profile_id=? AND mod_id=?", (profile, selected.id))
        else:
            db.execute("DELETE FROM entries WHERE profile_id=? AND mod_id=?", (profile, selected.id))
    before = library.rows(profile)
    with pytest.raises(ManagerError, match="changed|no longer"):
        apply(library, plan)
    assert library.rows(profile) == before


def test_undo_preserves_earlier_exclusions_and_later_edit_blocks_stale_undo(tmp_path):
    _, library, preview, profile, _, first = setup(tmp_path)
    second = next(row for row in library.rows(profile) if row.kind == "external" and row.id != first.id)
    apply(library, review(library, profile, (first.id,)))
    apply(library, review(library, profile, (second.id,)))
    undo(library, profile)
    library.import_mo2(preview, reconcile=True)
    assert first.id not in {row.id for row in library.rows(profile)} and second.id in {row.id for row in library.rows(profile)}
    bulk(library, profile, (second.id,), enabled=False)
    with library._db() as db:
        db.execute("UPDATE entries SET group_name='Later independent edit' WHERE profile_id=? AND mod_id=?", (profile, second.id))
    with pytest.raises(ManagerError, match="changed after"):
        undo(library, profile)


def test_owned_invalid_empty_and_runtime_locked_targets_fail(tmp_path, monkeypatch):
    _, library, _, profile, before, selected = setup(tmp_path)
    for targets in ((), ("unknown",)):
        with pytest.raises(ManagerError):
            review(library, profile, targets)
    plan = review(library, profile, (selected.id,))
    monkeypatch.setattr("creation_lib.mod_manager.runtime.active_sessions", lambda root: [("generated running fixture", {})])
    with pytest.raises(ManagerError, match="runtime"):
        apply(library, plan)
    monkeypatch.undo()
    assert library.rows(profile) == before
    with library._db() as db:
        db.execute("UPDATE mods SET kind='owned' WHERE id=?", (selected.id,))
    with pytest.raises(ManagerError, match="Owned mods"):
        review(library, profile, (selected.id,))


def test_undo_does_not_restore_reference_whose_source_or_ownership_changed(tmp_path):
    _, library, _, profile, _, selected = setup(tmp_path)
    apply(library, review(library, profile, (selected.id,)))
    with library._db() as db:
        db.execute("UPDATE mods SET source='Changed shared metadata target' WHERE id=?", (selected.id,))
    before = library.rows(profile)
    with pytest.raises(ManagerError, match="source or ownership changed"):
        undo(library, profile)
    assert library.rows(profile) == before
