import threading

import pytest

from creation_lib.mod_manager import row_edit, organization
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2


@pytest.fixture
def target(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    preview = inspect_instance(source)
    library.import_mo2(preview)
    profile = library.profiles()[0].id
    row = next(row for row in library.rows(profile) if row.kind == "external")
    with library._db() as db:
        db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=123,nexus_file_id=456 WHERE id=?", (row.id,))
    return library, profile, row.id, preview, source


@pytest.mark.parametrize("field,value,attribute,expected", [("notes", "first\nsecond", "notes", "first\nsecond"), ("version", " 2.4-local ", "version", "2.4-local"), ("category", "Textures\nVisual/Other\ntextures", "categories", ("Textures", "Visual/Other")), ("nexus", "789", "nexus_mod_id", 789)])
def test_shared_fields_persist_repeat_import_reopen_without_source_writes(target, field, value, attribute, expected):
    library, profile, mod, preview, source = target
    original = {str(p): p.read_bytes() for p in source.rglob("*") if p.is_file()}
    row_edit.apply(library, row_edit.review(library, profile, mod, field), value)
    library.import_mo2(preview, reconcile=True)
    reopened = Library(library.root)
    assert all(getattr(next(row for row in reopened.rows(p.id) if row.id == mod), attribute) == expected for p in reopened.profiles())
    if field == "nexus":
        assert next(row for row in reopened.rows(profile) if row.id == mod).nexus_file_id == 0
    assert original == {str(p): p.read_bytes() for p in source.rglob("*") if p.is_file()}


def test_priority_is_profile_local_and_undoable(target):
    library, profile, mod, _, _ = target
    before = library.rows(profile)
    other = library.profiles()[1].id
    unchanged = library.rows(other)
    row_edit.apply(library, row_edit.review(library, profile, mod, "priority"), str(len(before) - 1))
    assert library.rows(profile)[-1].id == mod and library.rows(other) == unchanged
    organization.undo(library, profile)
    assert library.rows(profile) == before


@pytest.mark.parametrize("field,value", [("version", "x" * 201), ("notes", "x" * 16385), ("notes", "bad\x00"), ("version", "bad\ud800"), ("category", "../bad"), ("category", "\n".join(str(i) for i in range(33))), ("priority", "9999"), ("priority", "-1"), ("nexus", "x"), ("nexus", "99999999999999999999"), ("nexus", "2147483648")])
def test_invalid_field_never_changes_local_state(target, field, value):
    library, profile, mod, _, _ = target
    before = library.rows(profile)
    with pytest.raises(ManagerError):
        row_edit.apply(library, row_edit.review(library, profile, mod, field), value)
    assert library.rows(profile) == before


@pytest.mark.parametrize("change", ["notes", "source", "membership", "priority"])
def test_stale_review_preserves_new_metadata_or_order(target, change):
    library, profile, mod, _, _ = target
    plan = row_edit.review(library, profile, mod, "priority" if change == "priority" else "notes")
    with library._db() as db:
        if change == "membership":
            db.execute("DELETE FROM entries WHERE profile_id=? AND mod_id=?", (profile, mod))
        elif change == "priority":
            db.execute("UPDATE entries SET priority=priority+1 WHERE profile_id=?", (profile,))
        else:
            db.execute(f"UPDATE mods SET {change}='later' WHERE id=?", (mod,))
    before = library.rows(profile)
    with pytest.raises(ManagerError):
        row_edit.apply(library, plan, "0" if change == "priority" else "unexpected")
    assert library.rows(profile) == before


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_field_cancel_failure_roll_back(target, monkeypatch, outcome):
    library, profile, mod, _, _ = target
    before, cancel = library.rows(profile), threading.Event()
    def stop():
        if outcome == "failed":
            raise OSError("generated field failure")
        cancel.set()
    monkeypatch.setattr(row_edit, "_checkpoint", stop)
    with pytest.raises(Cancelled if outcome == "cancelled" else OSError):
        row_edit.apply(library, row_edit.review(library, profile, mod, "notes"), "unexpected", cancel)
    assert library.rows(profile) == before


def test_readonly_unknown_game_and_runtime_guards(target, monkeypatch):
    library, profile, mod, _, _ = target
    with pytest.raises(ManagerError, match="read-only"):
        row_edit.review(library, profile, mod, "status")
    with library._db() as db:
        db.execute("UPDATE mods SET nexus_game='' WHERE id=?", (mod,))
    plan = row_edit.review(library, profile, mod, "nexus")
    with pytest.raises(ManagerError, match="game context"):
        row_edit.apply(library, plan, "88")
    monkeypatch.setattr("creation_lib.mod_manager.runtime.active_sessions", lambda _: [("fixture", {"state": "running"})])
    with pytest.raises(ManagerError, match="runtime"):
        row_edit.apply(library, plan, "0")
