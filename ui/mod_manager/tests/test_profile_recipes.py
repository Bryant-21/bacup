import json
import threading

import pytest

from creation_lib.mod_manager import profile_recipes as recipes
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2


@pytest.fixture
def libraries(tmp_path):
    result = []
    for name in ("origin", "destination"):
        directory = tmp_path / name
        directory.mkdir()
        source = make_mo2(directory)
        library = Library(directory / "library", create=True)
        library.import_mo2(inspect_instance(source))
        result.append((library, library.profiles()[0].id, source))
    return result


def prepared(libraries, cross=False):
    source, profile, _ = libraries[0]
    recipe = recipes.read(recipes.export(source, profile))
    target, template, _ = libraries[int(cross)]
    mapping = recipes.suggestions(target, template, recipe)
    if cross:
        assert not mapping  # Equal names across libraries never constitute identity proof.
        names = {row.name: row.id for row in target.rows(template) if row.kind != "separator"}
        mapping = {entry.id: names[entry.name] for entry in recipe.entries if entry.kind != "separator"}
    return target, template, recipe, mapping


@pytest.mark.parametrize("cross", [False, True])
def test_round_trip_preserves_order_activation_groups_and_own_game_context_without_payload_io(libraries, cross, monkeypatch):
    from creation_lib.mod_manager.package import Package
    target, template, recipe, mapping = prepared(libraries, cross)
    source, original, folder = libraries[0]
    originals = {str(path): path.read_bytes() for path in folder.rglob("*") if path.is_file()}
    monkeypatch.setattr(Package, "__init__", lambda *a, **kw: pytest.fail("Recipe must not open or scan mod packages"))
    plan = recipes.review(target, template, recipe, mapping, "Transferred")
    assert plan.recipe is recipe
    assert plan.mapping == tuple(sorted(mapping.items()))
    identifier, created = recipes.apply(target, plan)
    assert created and identifier != template and len(target.profiles()) == 3
    profile, rows, _ = target.profile_snapshot(identifier)
    assert [(row.name, row.kind == "separator", row.enabled, row.group) for row in rows] == [
        (entry.name, entry.kind == "separator", entry.enabled, entry.group) for entry in recipe.entries]
    assert [row.priority for row in rows] == list(range(len(rows)))
    assert profile.source == "" and profile.files["_modbox_instance_id"] == target.profile_snapshot(template)[0].files["_modbox_instance_id"]
    assert dict(recipe.files)["plugins.txt"] == profile.files["plugins.txt"]
    assert source.profile_snapshot(original)[0].name == recipe.name
    assert originals == {str(path): path.read_bytes() for path in folder.rglob("*") if path.is_file()}
    encoded = open(recipe.path, "rb").read()
    assert str(folder).encode() not in encoded and b"credentials" not in encoded and b"_modbox_instance_id" not in encoded


def test_repeated_transfer_preserves_local_edits_and_removed_profiles(libraries):
    library, template, recipe, mapping = prepared(libraries)
    target, _ = recipes.apply(library, recipes.review(library, template, recipe, mapping, "Transferred"))
    mod = next(row for row in library.rows(target) if row.kind != "separator")
    library.customize(target, mod.id, enabled=False, group="My overrides")
    with library._db() as db:
        db.execute("UPDATE profiles SET name='My local title' WHERE id=?", (target,))
    before = library.profile_snapshot(target)
    plan = recipes.review(library, template, recipe, mapping, "Another requested title")
    assert plan.existing and recipes.apply(library, plan) == (target, False)
    assert library.profile_snapshot(target) == before
    assert recipes.suggestions(library, target, recipe) == mapping
    library.delete_profile(target)
    with pytest.raises(ManagerError, match="removed"):
        recipes.review(library, template, recipe, mapping, "Removed must stay removed")
    assert not any(profile.id == target for profile in library.profiles())


@pytest.mark.parametrize("change", ["recipe", "template", "entry", "mod", "instance", "destination"])
def test_review_invalidated_by_file_template_mapping_instance_or_destination_changes(libraries, change):
    library, template, recipe, mapping = prepared(libraries)
    plan = recipes.review(library, template, recipe, mapping, "Transferred")
    if change == "recipe":
        with open(recipe.path, "a", encoding="utf-8") as stream:
            stream.write(" ")  # Content hash catches semantically identical rewrites, too.
    else:
        with library._db() as db:
            if change == "template":
                db.execute("UPDATE profiles SET name='Changed template' WHERE id=?", (template,))
            elif change == "entry":
                db.execute("UPDATE entries SET enabled=0 WHERE profile_id=?", (template,))
            elif change == "mod":
                db.execute("UPDATE mods SET source='different source' WHERE id=?", (next(iter(mapping.values())),))
            elif change == "instance":
                db.execute("UPDATE instances SET configuration='{}'")
            else:
                db.execute("INSERT INTO profiles(id,name) VALUES (?,'Occupied')", (plan.target,))
    before = library.profiles()
    with pytest.raises(ManagerError, match="changed|occupied"):
        recipes.apply(library, plan)
    assert library.profiles() == before


@pytest.mark.parametrize("change", ["missing", "duplicate", "unknown", "foreign-game", "wrong-game-template"])
def test_mapping_requires_explicit_distinct_available_same_game_mods(libraries, change):
    library, template, recipe, mapping = prepared(libraries, True)
    source = next(iter(mapping))
    if change == "missing":
        mapping.pop(source)
    elif change == "duplicate":
        mapping = dict.fromkeys(mapping, next(iter(mapping.values())))
    elif change == "unknown":
        mapping[source] = "missing"
    else:
        with library._db() as db:
            if change == "foreign-game":
                db.execute("UPDATE mods SET nexus_game='skyrimspecialedition' WHERE id=?", (mapping[source],))
            else:
                files = library.profile_snapshot(template)[0].files
                files["_modbox_game"] = "Skyrim"
                db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), template))
    before = library.profiles()
    with pytest.raises(ManagerError):
        recipes.review(library, template, recipe, mapping, "Transferred")
    assert library.profiles() == before


@pytest.mark.parametrize("field,value", [
    ("version", True), ("game", "Unknown Game"), ("library", "../unsafe"),
    ("entries", [{"id": "bad"}]), ("files", {"../outside.ini": "[General]"}),
    ("files", {"_modbox_instance_id": "foreign"}), ("files", {"plugins.txt": None}),
    ("files", {"Fallout4.ini": "malformed INI"}), ("minimal_inis", ["unexpected.ini"]),
    ("name", "invalid\ud800"), ("files", {"plugins.txt": "invalid\ud800"}),
])
def test_strict_recipe_format_rejects_unsafe_or_unknown_metadata(libraries, field, value):
    _, _, recipe, _ = prepared(libraries)
    with open(recipe.path, encoding="utf-8") as stream:
        data = json.load(stream)
    data[field] = value
    with open(recipe.path, "w", encoding="utf-8") as stream:
        json.dump(data, stream)
    with pytest.raises(ManagerError):
        recipes.read(recipe.path)


@pytest.mark.parametrize("mutation", ["duplicate", "kind", "activation", "group"])
def test_recipe_entry_shapes_and_windows_groups_are_checked(libraries, mutation):
    _, _, recipe, _ = prepared(libraries)
    with open(recipe.path, encoding="utf-8") as stream:
        data = json.load(stream)
    if mutation == "duplicate":
        data["entries"].append(data["entries"][-1])
    else:
        entry = data["entries"][-1]
        entry[{"kind": "kind", "activation": "enabled", "group": "group"}[mutation]] = {"kind": [], "activation": 1, "group": "../escape"}[mutation]
    with open(recipe.path, "w", encoding="utf-8") as stream:
        json.dump(data, stream)
    with pytest.raises(ManagerError):
        recipes.read(recipe.path)


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_cancelled_or_failed_export_and_import_leave_no_partial_metadata(libraries, monkeypatch, outcome):
    library, template, recipe, mapping = prepared(libraries)
    plan = recipes.review(library, template, recipe, mapping, "Transferred")
    before, rows = library.profiles(), library.rows(template)
    cancel = threading.Event()
    def stop(stage):
        if outcome == "failed":
            raise OSError("disposable recipe failure")
        cancel.set()
    monkeypatch.setattr(recipes, "_checkpoint", stop)
    with pytest.raises(Cancelled if outcome == "cancelled" else OSError):
        recipes.apply(library, plan, cancel)
    assert library.profiles() == before and library.rows(template) == rows
    cancel.clear()
    exports = list((library.root / "exports").iterdir())
    with pytest.raises(Cancelled if outcome == "cancelled" else OSError):
        recipes.export(library, template, cancel)
    assert list((library.root / "exports").iterdir()) == exports
    assert not any(path.name.startswith("recipe-") for path in (library.root / ".staging").iterdir())
    assert Library(library.root).profiles() == before


def test_bounds_duplicate_json_keys_runtime_guard_and_cancelled_read(libraries, monkeypatch):
    library, template, recipe, mapping = prepared(libraries)
    plan = recipes.review(library, template, recipe, mapping, "Transferred")
    with monkeypatch.context() as patch:
        patch.setattr("creation_lib.mod_manager.runtime.active_sessions", lambda root: [("fixture", {"state": "running"})])
        with pytest.raises(ManagerError, match="recorded runtime"):
            recipes.apply(library, plan)
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        recipes.read(recipe.path, cancel)
    with monkeypatch.context() as patch:
        patch.setattr(recipes, "MAX_BYTES", 32)
        with pytest.raises(ManagerError, match="64 MiB"):
            recipes.read(recipe.path)
    with monkeypatch.context() as patch:
        patch.setattr(recipes, "MAX_ENTRIES", 2)
        with pytest.raises(ManagerError, match="ordered entries"):
            recipes.read(recipe.path)
    with open(recipe.path, "w", encoding="utf-8") as stream:
        stream.write('{"format":"first","format":"second"}')
    with pytest.raises(ManagerError, match="duplicate keys"):
        recipes.read(recipe.path)


def test_export_never_replaces_an_existing_owned_export(libraries, monkeypatch):
    from types import SimpleNamespace
    library, profile, _ = libraries[0]
    path = recipes.export(library, profile)
    before = path.read_bytes()
    monkeypatch.setattr(recipes.uuid, "uuid4", lambda: SimpleNamespace(hex=path.name.split(".")[0]))
    with pytest.raises(OSError):
        recipes.export(library, profile)
    assert path.read_bytes() == before
    assert not any(item.name.startswith("recipe-") for item in (library.root / ".staging").iterdir())
