from dataclasses import replace
from pathlib import Path
import threading

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.organization import empty_mod
from creation_lib.mod_manager.paths import ManagerError, Cancelled
from creation_lib.mod_manager.plugin_state import Catalog, Plugin, default_order, inventory, inspect
from creation_lib.mod_manager.plugin_order import move
from creation_lib.mod_manager.transactions import OwnedTransactions
from .fixtures import make_mo2


def test_empty_mod_insertion_reopen_and_duplicate_rollback(tmp_path):
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(make_mo2(tmp_path)))
    profile = library.profiles()[0].id
    before = library.rows(profile)
    anchor = before[0]
    identifier = empty_mod(library, profile, "My empty mod", anchor.id)
    rows = Library(library.root).rows(profile)
    created = next(row for row in rows if row.id == identifier)
    assert rows[1] == created and created.group == anchor.group and created.enabled
    assert created.kind == "owned" and Path(created.source).is_dir()
    assert not list(Path(created.source).iterdir())
    for name, after in (("my EMPTY mod", ""), ("Stale insertion", "missing-id")):
        with pytest.raises(ManagerError):
            empty_mod(library, profile, name, after)
    assert Library(library.root).rows(profile) == rows
    assert list((library.root / "mods").iterdir()) == [Path(created.source)]


@pytest.mark.parametrize("phase", ["prepared", "placed", "before-commit"])
def test_empty_mod_failure_restores_folder_and_profile(tmp_path, monkeypatch, phase):
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(make_mo2(tmp_path)))
    profile = library.profiles()[0].id
    before = library.rows(profile)
    def fail(self, name):
        if name == phase:
            raise OSError("injected empty-mod failure")
    monkeypatch.setattr(OwnedTransactions, "_checkpoint", fail)
    with pytest.raises(OSError, match="injected"):
        empty_mod(library, profile, "Never published", before[0].id)
    assert Library(library.root).rows(profile) == before
    assert not list((library.root / "mods").iterdir())


def test_empty_mod_cancellation_and_cli_share_backend(tmp_path):
    from creation_lib.mod_manager.agent_launch import empty_mod as cli_empty
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(make_mo2(tmp_path)))
    profile = library.profiles()[0]
    cancelled = threading.Event()
    cancelled.set()
    with pytest.raises(Cancelled):
        empty_mod(library, profile.id, "Cancelled", cancel=cancelled)
    result = cli_empty(library.root, profile.name, "CLI folder")
    assert any(row.id == result["mod_id"] and Path(row.source).is_dir() for row in library.rows(profile.id))


def catalog():
    def p(name, rank, masters=(), esm=False):
        return Plugin(name, "fixture/" + name, "fixture", "", True, rank, header={"flags":{"esm":esm}}, masters=masters, library_priority=rank)
    return Catalog("profile", "signature", "fo4", (
        p("Patch.esp", 0, ("Master.esp",)), p("Unrelated.esp", 1),
        p("Master.esp", 2), p("Base.esm", 3, esm=True),
    ))


def test_library_order_stable_dependencies_flags_and_invalid_moves():
    source = catalog()
    order = default_order(source)
    assert order == ["Base.esm", "Unrelated.esp", "Master.esp", "Patch.esp"]
    with pytest.raises(ManagerError, match="depends on"):
        move(source, order, {"Patch.esp"}, 1, ())
    disabled = replace(source, plugins=tuple(replace(p, enabled=False) for p in source.plugins))
    with pytest.raises(ManagerError, match="depends on"):
        move(disabled, order, {"Patch.esp"}, 1, ())
    with pytest.raises(ManagerError, match="Master"):
        move(source, order, {"Base.esm"}, 3, ())


def test_library_order_rejects_cycles_and_conflicting_locks():
    source = catalog()
    cyclic = replace(source, plugins=tuple(replace(p, masters=("Patch.esp",)) if p.name == "Master.esp" else p for p in source.plugins))
    with pytest.raises(ManagerError, match="cycle"):
        default_order(cyclic)
    with pytest.raises(ManagerError, match="Locked"):
        default_order(replace(source, locked=(("Patch.esp", 0),)))


def test_inventory_default_uses_provider_priority_then_dependencies(tmp_path):
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(make_mo2(tmp_path)))
    profile = library.profiles()[0].id
    first = empty_mod(library, profile, "First provider")
    second = empty_mod(library, profile, "Second provider")
    rows = {row.id: row for row in library.rows(profile)}
    (Path(rows[first].source) / "Z-first.esp").write_bytes(b"fixture")
    (Path(rows[second].source) / "A-second.esp").write_bytes(b"fixture")
    with library._db() as db:
        import json
        files = library.profiles()[0].files
        files["loadorder.txt"] = ""
        files["plugins.txt"] = ""
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile))
    source = inventory(library, profile)
    assert not source.custom_order
    def reader(path, game, cancel=None):
        return {"flags":{}}, (("A-second.esp",) if path.name == "Z-first.esp" else ())
    inspected = inspect(source, reader)
    names = [p.name for p in inspected.plugins]
    assert names.index("A-second.esp") < names.index("Z-first.esp")


def test_unreadable_header_keeps_other_plugins_visible(tmp_path):
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(make_mo2(tmp_path)))
    profile = library.profiles()[0].id
    source = inventory(library, profile)
    failing = next(p.name for p in source.plugins if p.path)
    def reader(path, game, cancel=None):
        if path.name == failing:
            raise ManagerError("Invalid generated header")
        return {"flags": {}}, ()
    result = inspect(source, reader)
    assert len(result.plugins) == len(source.plugins)
    assert next(p for p in result.plugins if p.name == failing).header is None
    assert any(failing in issue for issue in result.issues)
