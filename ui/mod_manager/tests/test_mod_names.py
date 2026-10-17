import threading
from pathlib import Path

import pytest

from creation_lib.mod_manager import mod_names
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2


@pytest.fixture
def title(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    preview = inspect_instance(source)
    library.import_mo2(preview)
    profile = library.profiles()[0].id
    row = next(row for row in library.rows(profile) if row.kind == "external")
    return library, profile, row, preview


def test_title_is_shared_without_source_io_and_repeat_import_preserves_alias(title):
    library, profile, row, preview = title
    originals = {str(path): path.read_bytes() for path in Path(preview.root).rglob("*") if path.is_file()}
    mod_names.apply(library, mod_names.review(library, profile, row.id), "My ### display alias 🌿")
    library.import_mo2(preview)
    assert all(next(mod for mod in library.rows(item.id) if mod.id == row.id).name == "My ### display alias 🌿" for item in library.profiles())
    assert next(mod for mod in library.rows(profile) if mod.id == row.id).source == row.source
    assert originals == {str(path): path.read_bytes() for path in Path(preview.root).rglob("*") if path.is_file()}


@pytest.mark.parametrize("name", ["", "   ", "x" * 201, "newline\nname", "bad\ud800"])
def test_invalid_display_title_never_changes_metadata(title, name):
    library, profile, row, _ = title
    with pytest.raises(ManagerError):
        mod_names.apply(library, mod_names.review(library, profile, row.id), name)
    assert next(mod for mod in library.rows(profile) if mod.id == row.id) == row


@pytest.mark.parametrize("change", ["title", "source", "membership"])
def test_stale_title_target_is_not_overwritten(title, change):
    library, profile, row, _ = title
    plan = mod_names.review(library, profile, row.id)
    with library._db() as db:
        if change == "membership":
            db.execute("DELETE FROM entries WHERE profile_id=? AND mod_id=?", (profile, row.id))
        else:
            db.execute(f"UPDATE mods SET {'name' if change == 'title' else 'source'}='changed' WHERE id=?", (row.id,))
    before = library.rows(profile)
    with pytest.raises(ManagerError):
        mod_names.apply(library, plan, "Unexpected replacement")
    assert library.rows(profile) == before


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_title_write_rolls_back_before_commit(title, monkeypatch, outcome):
    library, profile, row, _ = title
    cancel = threading.Event()
    def stop():
        if outcome == "failed":
            raise OSError("disposable metadata failure")
        cancel.set()
    monkeypatch.setattr(mod_names, "_checkpoint", stop)
    with pytest.raises(Cancelled if outcome == "cancelled" else OSError):
        mod_names.apply(library, mod_names.review(library, profile, row.id), "Never committed", cancel)
    assert next(mod for mod in library.rows(profile) if mod.id == row.id) == row


def test_separator_missing_profile_and_recorded_runtime_guard(title, monkeypatch):
    library, profile, row, _ = title
    separator = next(mod.id for mod in library.rows(profile) if mod.kind == "separator")
    for profile_id, mod_id in (("missing", row.id), (profile, separator)):
        with pytest.raises(ManagerError, match="Select an owned"):
            mod_names.review(library, profile_id, mod_id)
    plan = mod_names.review(library, profile, row.id)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.active_sessions", lambda root: [("fixture", {"state": "running"})])
    with pytest.raises(ManagerError, match="recorded runtime"):
        mod_names.apply(library, plan, "No runtime edits")
