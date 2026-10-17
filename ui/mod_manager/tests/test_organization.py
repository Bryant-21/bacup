import threading

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.organization import bulk, separator, undo
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2


def test_bulk_move_group_enable_undo_reopen_and_source_immutability(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    profile = library.profiles()[0]
    before = library.rows(profile.id)
    sources = {str(path): path.read_bytes() for path in source.rglob("*") if path.is_file()}
    selected = [row.id for row in before if row.kind != "separator"][:2]
    bulk(library, profile.id, selected, enabled=False, group="Custom/Textures", priority=2)
    after = library.rows(profile.id)
    assert all(not row.enabled and row.group == "Custom/Textures" for row in after if row.id in selected)
    assert [row.id for row in after if row.id in selected] == selected
    undo(Library(library.root), profile.id)
    assert library.rows(profile.id) == before
    assert {str(path): path.read_bytes() for path in source.rglob("*") if path.is_file()} == sources


def test_cancel_and_stale_undo_preserve_later_changes(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile = library.create_profile("Default")
    group_id = separator(library, profile, "Visual", "Visual")
    before = library.rows(profile)
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        bulk(library, profile, [group_id], group="Changed", cancel=cancel)
    assert library.rows(profile) == before
    separator(library, profile, "Later", "Later")
    with library._db() as db:
        db.execute("UPDATE entries SET group_name='External edit' WHERE profile_id=? AND mod_id=?", (profile, group_id))
    with pytest.raises(ManagerError, match="changed after"):
        undo(library, profile)
    assert library.rows(profile)[0].group == "External edit"


def test_separator_and_single_checkbox_edits_have_recoverable_undo(tmp_path):
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(make_mo2(tmp_path)))
    profile = library.profiles()[0]
    before = library.rows(profile.id)
    row = next(row for row in before if row.kind != "separator")
    library.customize(profile.id, row.id, enabled=not row.enabled)
    undo(library, profile.id)
    assert library.rows(profile.id) == before
    separator(library, profile.id, "Textures", "Visual/Textures")
    undo(library, profile.id)
    assert library.rows(profile.id) == before
