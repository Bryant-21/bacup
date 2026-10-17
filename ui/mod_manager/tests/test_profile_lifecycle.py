import threading

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2, make_package


def setup(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    return source, library, library.profiles()[0]


def test_delete_restore_retains_shared_mods_organization_sources_and_restart(tmp_path):
    source, library, profile = setup(tmp_path)
    before = {str(path): path.read_bytes() for path in source.glob("profiles/*/*") if path.is_file()}
    row = library.rows(profile.id)[1]
    library.customize(profile.id, row.id, enabled=False, group="Local/Textures", notes="shared note")
    rows = library.rows(profile.id)
    other = library.profiles()[1]
    other_rows = library.rows(other.id)
    library.delete_profile(profile.id)
    reopened = Library(library.root)
    assert len(reopened.profiles()) == 1 and reopened.rows(other.id) == other_rows
    assert reopened.deleted_profiles()[0]["id"] == profile.id
    assert reopened.import_mo2(inspect_instance(source), reconcile=True) == 0
    reopened.restore_profile(profile.id)
    assert reopened.rows(profile.id) == rows
    assert next(item for item in reopened.profiles() if item.id == profile.id).files == profile.files
    assert {str(path): path.read_bytes() for path in source.glob("profiles/*/*") if path.is_file()} == before


@pytest.mark.parametrize("operation", ["delete", "restore"])
def test_cancelled_profile_lifecycle_is_atomic(tmp_path, operation):
    _, library, profile = setup(tmp_path)
    cancelled = threading.Event()
    cancelled.set()
    if operation == "restore":
        library.delete_profile(profile.id)
    with pytest.raises(Cancelled):
        (library.delete_profile if operation == "delete" else library.restore_profile)(profile.id, cancelled)
    assert (any(item.id == profile.id for item in library.profiles())) == (operation == "delete")
    assert bool(library.deleted_profiles()) == (operation == "restore")


def test_delete_last_profile_does_not_remove_shared_mod_metadata(tmp_path):
    _, library, profile = setup(tmp_path)
    for item in library.profiles():
        library.delete_profile(item.id)
    assert not library.profiles()
    with library._db() as db:
        assert db.execute("SELECT COUNT(*) FROM mods").fetchone()[0] == 4
    library.restore_profile(profile.id)
    assert len(library.rows(profile.id)) == 4


def test_new_profile_can_use_existing_references_without_copying(tmp_path):
    _, library, original = setup(tmp_path)
    sources = {row.id: row.source for row in library.rows(original.id) if row.kind != "separator"}
    created = library.create_profile("Blank local profile")
    rows = library.rows(created)
    assert len(rows) == 3 and all(not row.enabled for row in rows)
    assert {row.id: row.source for row in rows} == sources
    assert not tuple((library.root / "mods").iterdir())


def test_legacy_unmanaged_rows_stay_outside_managed_lists_and_exports(tmp_path):
    source, library, profile = setup(tmp_path)
    with library._db() as db:
        db.execute("INSERT INTO mods(id,name,source,kind) VALUES ('legacy-game','Unmanaged fixture','','foreign')")
        db.execute("INSERT INTO entries VALUES (?, 'legacy-game',99,1,'')", (profile.id,))
    reopened = Library(library.root)
    reopened.import_mo2(inspect_instance(source), reconcile=True)
    assert all(row.kind != "foreign" for row in reopened.rows(profile.id))
    assert reopened.export_plan(profile.id).files



def test_restore_profile_requires_removed_mod_first_and_preserves_snapshot(tmp_path):
    _, library, profile = setup(tmp_path)
    package = Package(make_package(tmp_path))
    try:
        identifier = library.install(package, package.plain_plan(), "Generated owned fixture", profile.id)
    finally:
        package.close()
    library.customize(profile.id, identifier, enabled=False, group="Before/Restore")
    library.delete_profile(profile.id)
    backup = library.uninstall(library.review_owned(identifier))
    with pytest.raises(ManagerError, match="Restore removed mod content"):
        library.restore_profile(profile.id)
    assert library.deleted_profiles() and all(item.id != profile.id for item in library.profiles())
    entry, current = library.review_restore(backup)
    library.restore(backup, current, backup_token=entry["backup_token"])
    library.restore_profile(profile.id)
    row = next(row for row in library.rows(profile.id) if row.id == identifier)
    assert not row.enabled and row.group == "Before/Restore"
