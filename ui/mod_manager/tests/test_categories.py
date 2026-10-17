import json
import threading

import pytest

from creation_lib.mod_manager.categories import assign, inventory, remove, rename, save
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2


@pytest.fixture
def categorized(tmp_path):
    source = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    profile = library.profiles()[0].id
    return library, profile, source


def test_shared_assignment_nested_rename_ids_removal_and_reopen(categorized):
    library, profile, source = categorized
    original = (source / "mods" / "Base" / "meta.ini").read_bytes()
    row = next(item for item in library.rows(profile) if item.name == "Base")
    save(library, "Visual/Textures", 123)
    save(library, "Visual", 0)
    assign(library, profile, [row.id], "Visual/Textures")
    _, expected = inventory(library)
    rename(library, "Visual", "Appearance", expected)
    items, expected = inventory(library)
    assert next(item for item in items if item.name == "Appearance/Textures").nexus_id == 123
    assert all(next(item for item in library.rows(profile.id) if item.id == row.id).category == "Appearance/Textures" for profile in library.profiles())
    with pytest.raises(ManagerError, match="assigned mods"):
        remove(library, "Appearance", expected)
    assign(library, profile, [row.id], "")
    remove(library, "Appearance/Textures", inventory(library)[1])
    remove(library, "Appearance", inventory(library)[1])
    assert all(not item.name.startswith("Appearance") for item in inventory(Library(library.root))[0])
    assert (source / "mods" / "Base" / "meta.ini").read_bytes() == original


def test_unicode_casefold_update_preserves_one_canonical_category(categorized):
    library, _, _ = categorized
    save(library, "Straße/Ästhetik", 123)
    save(library, "STRASSE/ÄSTHETIK", 456)
    values = [item for item in inventory(library)[0] if item.name.casefold() == "strasse/ästhetik"]
    assert len(values) == 1 and values[0].name == "Straße/Ästhetik" and values[0].nexus_id == 456


def test_multiple_memberships_filter_primary_order_shared_rename_and_repeat(categorized):
    from creation_lib.mod_manager.controller import ManagerController
    library, profile, source = categorized
    row = next(item for item in library.rows(profile) if item.name == "Base")
    original = (source / "mods/Base/meta.ini").read_bytes()
    assign(library, profile, [row.id], ["Visual/Textures", "Gameplay/Quests", "VISUAL/TEXTURES"])
    manager = ManagerController()
    try:
        manager.library, manager.profile_id = library, profile
        manager.refresh()
        assert manager.rows_by_id[row.id].categories == ("Visual/Textures", "Gameplay/Quests")
        assert manager.rows_by_id[row.id].category == "Visual/Textures"
        assert manager.filtered(category="gameplay")[0].id == row.id
        assert manager.filtered("Gameplay/Quests")[0].id == row.id
        assert row.id not in {item.id for item in manager.filtered(category="")}
    finally:
        manager.cleanup()
    rename(library, "Gameplay/Quests", "Roleplay/Quests", inventory(library)[1])
    for current in library.profiles():
        assert next(item for item in library.rows(current.id) if item.id == row.id).categories == ("Visual/Textures", "Roleplay/Quests")
    with pytest.raises(ManagerError, match="assigned mods"):
        remove(library, "Roleplay/Quests", inventory(library)[1])
    library.import_mo2(inspect_instance(source))
    assert next(item for item in Library(library.root).rows(profile) if item.id == row.id).categories == ("Visual/Textures", "Roleplay/Quests")
    assert (source / "mods/Base/meta.ini").read_bytes() == original


def test_mo2_category_id_list_resolves_all_names_without_payload_scan(tmp_path):
    source = make_mo2(tmp_path)
    source.joinpath("categories.dat").write_text("1|Visuals||0\n2|Gameplay||0\n3|Utilities||0\n")
    metadata = source / "mods/Base/meta.ini"
    metadata.write_text("[General]\ncategory=2,1,2,3\n")
    before = metadata.read_bytes()
    preview = inspect_instance(source)
    imported = next(item for item in preview.profiles[0].mods if item.name == "Base")
    assert imported.category == "Gameplay" and imported.categories == ("Gameplay", "Visuals", "Utilities")
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(preview)
    assert next(item for item in library.rows(library.profiles()[0].id) if item.name == "Base").categories == imported.categories
    assert metadata.read_bytes() == before


@pytest.mark.parametrize("values", [["safe", "../unsafe"], ["safe"] * 33, {"safe": True}])
def test_invalid_multiple_assignments_roll_back(categorized, values):
    library, profile, _ = categorized
    row = library.rows(profile)[0]
    before = inventory(library)
    with pytest.raises(ManagerError):
        assign(library, profile, [row.id], values)
    assert inventory(library) == before


def test_owned_uninstall_restore_preserves_multiple_and_legacy_primary_categories(tmp_path):
    from .test_transactions import setup_owned
    library, profile, mod, _ = setup_owned(tmp_path)
    assign(library, profile, [mod], ["Visual/Textures", "Gameplay"])
    operation = library.uninstall(library.review_owned(mod))
    library.restore(operation)
    assert library.rows(profile)[0].categories == ("Visual/Textures", "Gameplay")
    operation = library.uninstall(library.review_owned(mod))
    with library._db() as db:
        snapshot = json.loads(db.execute("SELECT snapshot FROM operations WHERE id=?", (operation,)).fetchone()[0])
        del snapshot["mod"]["categories_json"]
        db.execute("UPDATE operations SET snapshot=? WHERE id=?", (json.dumps(snapshot), operation))
    library.restore(operation)
    assert library.rows(profile)[0].categories == ("Visual/Textures",)


@pytest.mark.parametrize("fault", ["stale", "collision", "cancelled", "traversal", "foreign-mod"])
def test_category_invalid_stale_cancelled_changes_are_atomic(categorized, fault):
    library, profile, _ = categorized
    save(library, "Visual/Textures")
    expected = inventory(library)[1]
    cancel = threading.Event()
    if fault == "stale":
        save(library, "Later")
    elif fault == "collision":
        save(library, "Other/Textures")
        expected = inventory(library)[1]
    elif fault == "cancelled":
        cancel.set()
    before = inventory(library)
    with pytest.raises(Cancelled if fault == "cancelled" else ManagerError):
        if fault == "traversal":
            save(library, "../unsafe")
        elif fault == "foreign-mod":
            assign(library, profile, ["missing"], "Visual")
        else:
            rename(library, "Visual/Textures", "Other/Textures", expected, cancel)
    assert inventory(library) == before
