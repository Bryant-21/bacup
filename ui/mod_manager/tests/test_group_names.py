import threading

import pytest

from creation_lib.mod_manager import group_names, organization
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
    mods = [row.id for row in library.rows(profile) if row.kind == "external"]
    organization.bulk(library, profile, mods[:2], group="Visual/Textures")
    organization.bulk(library, profile, mods[2:], group="Other")
    return library, profile, preview, source


@pytest.mark.parametrize("original,replacement", [("Visual", "Custom"), ("Visual/Textures", "Other/Textures"), ("Visual", "Visual/More")])
def test_group_and_descendants_are_profile_local_repeatable_and_undoable(target, original, replacement):
    library, profile, preview, source = target
    before = library.rows(profile)
    unchanged = library.rows(library.profiles()[1].id)
    source_files = {str(p): p.read_bytes() for p in source.rglob("*") if p.is_file()}
    plan = group_names.review(library, profile, original)
    group_names.apply(library, plan, replacement)
    library.import_mo2(preview, reconcile=True)
    after = Library(library.root).rows(profile)
    for old, new in zip(before, after, strict=True):
        expected = replacement + old.group[len(original):] if old.group == original or old.group.startswith(original + "/") else old.group
        assert new.group == expected and new.name == old.name
    assert library.rows(library.profiles()[1].id) == unchanged
    organization.undo(library, profile)
    assert library.rows(profile) == before
    assert source_files == {str(p): p.read_bytes() for p in source.rglob("*") if p.is_file()}


@pytest.mark.parametrize("replacement", ["Other", "other", "../Bad", "Bad\x00", "x" * 201, "bad\ud800", "x" * 195])
def test_collision_invalid_or_long_descendant_is_preserved(target, replacement):
    library, profile, _, _ = target
    before = library.rows(profile)
    with pytest.raises(ManagerError):
        group_names.apply(library, group_names.review(library, profile, "Visual"), replacement)
    assert library.rows(profile) == before


@pytest.mark.parametrize("outcome", ["cancelled", "failed", "stale"])
def test_group_stale_cancel_failure_preserves_changes(target, monkeypatch, outcome):
    library, profile, _, _ = target
    plan = group_names.review(library, profile, "Visual")
    cancel = threading.Event()
    def stop():
        if outcome == "failed":
            raise OSError("generated group rename failure")
        cancel.set()
    monkeypatch.setattr(group_names, "_checkpoint", stop)
    if outcome == "stale":
        with library._db() as db:
            db.execute("UPDATE entries SET priority=priority+1 WHERE profile_id=?", (profile,))
    before = library.rows(profile)
    with pytest.raises(ManagerError if outcome == "stale" else Cancelled if outcome == "cancelled" else OSError):
        group_names.apply(library, plan, "Changed", cancel)
    assert library.rows(profile) == before


def test_missing_group_library_and_recorded_runtime_guards(target, monkeypatch, tmp_path):
    library, profile, _, _ = target
    with pytest.raises(ManagerError, match="existing local"):
        group_names.review(library, profile, "Missing")
    plan = group_names.review(library, profile, "Visual")
    with pytest.raises(ManagerError, match="Library changed"):
        group_names.apply(Library(tmp_path / "other", create=True), plan, "Changed")
    monkeypatch.setattr("creation_lib.mod_manager.runtime.active_sessions", lambda _: [("fixture", {"state": "running"})])
    with pytest.raises(ManagerError, match="runtime"):
        group_names.apply(library, plan, "Changed")
