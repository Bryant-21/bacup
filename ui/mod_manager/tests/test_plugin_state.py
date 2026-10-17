from dataclasses import replace
import json
import threading

import pytest

from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.plugin_state import CachedMetadata, apply, inspect, inventory, load_indices, locked_order, review, validate_runtime
from creation_lib.mod_manager.profile_state import rename, save_ini
from .test_standalone import game_fixture


@pytest.fixture
def plugin_fixture(tmp_path):
    game = game_fixture(tmp_path)
    (game / "Data" / "Fallout4.esm").write_bytes(b"generated base header placeholder")
    (game / "Data" / "DLCRobot.esm").write_bytes(b"generated DLC header placeholder")
    (game / "Data" / "Addon.esp").write_bytes(b"generated add-on header placeholder")
    library = Library(tmp_path / "library", create=True)
    profile_id = create(library, "fo4", game)
    def reader(path, game_id, cancel=None):
        return {"author": "Fixture", "version": 1., "flags": {"esm": path.suffix == ".esm"}}, (() if path.name == "Fallout4.esm" else ("Fallout4.esm",))
    catalog = inspect(inventory(library, profile_id), reader)
    return library, profile_id, game, catalog


def test_local_activation_core_order_dependencies_reopen_and_runtime_guard(plugin_fixture):
    library, profile_id, game, catalog = plugin_fixture
    assert [p.name for p in catalog.plugins if p.forced] == ["Fallout4.esm", "DLCRobot.esm"]
    active = [p.name for p in catalog.plugins]
    plan = review(catalog, enabled=active)
    apply(library, plan)
    profile = Library(library.root).profiles()[0]
    assert profile.files["plugins.txt"] == "*Fallout4.esm\n*DLCRobot.esm\n*Addon.esp\n"
    validate_runtime(profile.files, {}, game, library.root / "runtime" / "profiles" / profile_id / "overwrite")
    (game / "Data" / "Fallout4.esm").write_bytes(b"changed generated placeholder")
    with pytest.raises(ManagerError, match="changed"):
        validate_runtime(profile.files, {}, game, library.root / "runtime" / "profiles" / profile_id / "overwrite")


@pytest.mark.parametrize("mode", ["disable-core", "move-core", "missing-master", "unknown-header", "cycle", "extension-master"])
def test_plugin_review_rejects_invalid_activation_and_orders(plugin_fixture, mode):
    _, _, _, catalog = plugin_fixture
    order = [p.name for p in catalog.plugins]
    active = {p.name.casefold() for p in catalog.plugins}
    if mode == "disable-core":
        active.remove("fallout4.esm")
    elif mode == "move-core":
        order.reverse()
    elif mode == "missing-master":
        catalog = replace(catalog, plugins=tuple(replace(p, masters=("Missing.esm",)) if p.name == "Addon.esp" else p for p in catalog.plugins))
    elif mode == "unknown-header":
        catalog = replace(catalog, plugins=tuple(replace(p, header=None) if p.name == "Addon.esp" else p for p in catalog.plugins))
    elif mode == "cycle":
        catalog = replace(catalog, plugins=tuple(replace(p, masters=("Addon.esp",)) if p.name == "Fallout4.esm" else p for p in catalog.plugins))
    else:
        late = replace(catalog.plugins[-1], name="Late.esm", header={"flags": {}}, forced=False)
        regular = replace(late, name="Early.esp")
        catalog = replace(catalog, plugins=(*catalog.plugins[:2], regular, late))
        order, active = [p.name for p in catalog.plugins], {p.name.casefold() for p in catalog.plugins}
    with pytest.raises(ManagerError):
        review(catalog, order, active)


def test_review_rejects_added_provider_cancel_and_stale_configuration(plugin_fixture, tmp_path):
    library, profile_id, game, catalog = plugin_fixture
    plan = review(catalog)
    before = library.profiles()[0].files.copy()
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        apply(library, plan, cancel)
    assert library.profiles()[0].files == before
    (game / "Data" / "New.esp").write_bytes(b"generated after review")
    with pytest.raises(ManagerError, match="inventory/provider changed"):
        apply(library, plan)
    assert library.profiles()[0].files == before


def test_runtime_detects_changed_local_order_with_same_active_names(plugin_fixture):
    library, profile_id, game, catalog = plugin_fixture
    apply(library, review(catalog, enabled=[p.name for p in catalog.plugins]))
    files = library.profiles()[0].files
    files["loadorder.txt"] = "Addon.esp\nDLCRobot.esm\nFallout4.esm\n"
    with pytest.raises(ManagerError, match="activation/order"):
        validate_runtime(files, {}, game, library.root / "runtime" / "profiles" / profile_id / "overwrite")


def test_creation_club_forced_order_and_missing_primary(plugin_fixture):
    library, profile_id, game, _ = plugin_fixture
    (game / "Fallout4.ccc").write_text("# fixture\nCCFixture.esl\n")
    catalog = inventory(library, profile_id)
    assert any("CCFixture.esl" in issue for issue in catalog.issues)
    assert not any(p.name == "CCFixture.esl" for p in catalog.plugins)


def test_reviewed_load_order_follows_selected_installation_at_launch(plugin_fixture, tmp_path, monkeypatch):
    from creation_lib.mod_manager.runtime import prepare
    from creation_lib.mod_manager.launcher import Executable
    from .test_runtime import pe
    library, profile, game, catalog = plugin_fixture
    apply(library, review(catalog, enabled=[p.name for p in catalog.plugins]))
    saved = library.profiles()[0].files
    other = tmp_path / "selected installation"
    (other / "Data").mkdir(parents=True)
    pe(other / "f4se_loader.exe")
    pe(other / "Fallout4.exe")
    (other / "Data/Fallout4.esm").write_bytes(b"different installation base")
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.ensure", lambda *args: tmp_path / "bundle")
    plan = prepare(library, profile, Executable("selected", "F4SE", str(other / "f4se_loader.exe")), str(game), "",
                   user_targets=(tmp_path / "plugins", tmp_path / "ini"))
    snapshots = {name.casefold(): text for name, text, _ in plan.snapshots}
    assert plan.game_root == str(other)
    assert snapshots["plugins.txt"] == "*Fallout4.esm\n"
    assert snapshots["loadorder.txt"] == "Fallout4.esm\n"
    assert library.profiles()[0].files == saved
    # Switching installations does not permit unrelated mod-provider replacements.
    alternate = tmp_path / "different mod provider"
    alternate.mkdir()
    (alternate / "Fallout4.esm").write_bytes(b"unexpected provider")
    with pytest.raises(ManagerError, match="another provider"):
        validate_runtime(saved, {"fallout4.esm": ("Fallout4.esm", alternate / "Fallout4.esm")}, other, tmp_path / "overwrite", str(game))


def test_header_inspection_cancel_after_final_progress_is_not_published(plugin_fixture):
    _, _, _, catalog = plugin_fixture
    cancel = threading.Event()
    def progress(value, message):
        if value == 1:
            cancel.set()
    with pytest.raises(Cancelled):
        inspect(catalog, lambda path, game, event: ({"flags": {}}, ()), cancel, progress)


def test_profile_rename_ini_save_backup_cancel_and_stale_editor(plugin_fixture):
    library, profile_id, _, _ = plugin_fixture
    profile = library.profiles()[0]
    rename(library, profile_id, "My local profile")
    assert library.profiles()[0].name == "My local profile" and library.profiles()[0].id == profile_id
    name = "Fallout4Custom.ini"
    save_ini(library, profile_id, name, "[Display]\nfixture=1\n", profile.files[name])
    files = library.profiles()[0].files
    assert files[name].startswith("[Display]") and json.loads(files["_modbox_ini_backup"])[name] == profile.files[name]
    assert name not in json.loads(files["_modbox_minimal_inis"])
    with pytest.raises(ManagerError, match="changed"):
        save_ini(library, profile_id, name, "[Display]\nfixture=2\n", profile.files[name])
    with pytest.raises(ManagerError, match="sTestFile"):
        save_ini(library, profile_id, name, "[General]\nsTestFile1=hidden.esp\n", files[name])
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        save_ini(library, profile_id, name, "[Display]\nfixture=3\n", files[name], cancel)
    assert library.profiles()[0].files == files


def test_header_cache_reopens_and_invalidates_changed_sources(plugin_fixture):
    library, _, _, catalog = plugin_fixture
    path = catalog.plugins[0].path
    calls = []
    def reader(path, game, cancel=None):
        calls.append(str(path))
        return {"flags": {"esm": True}}, ()
    cache = CachedMetadata(library, reader)
    cache(path, "fo4")
    CachedMetadata(Library(library.root), reader)(path, "fo4")
    assert len(calls) == 1
    from pathlib import Path
    Path(path).write_bytes(b"changed generated header")
    cache(path, "fo4")
    assert len(calls) == 2


def test_plugin_locks_persist_unknown_names_guard_moves_and_runtime_edits(plugin_fixture):
    library, profile_id, game, catalog = plugin_fixture
    locks = (("Addon.esp", 2), ("Absent.esp", 8))
    apply(library, review(catalog, enabled=[p.name for p in catalog.plugins], locked=locks))
    reopened = inventory(Library(library.root), profile_id)
    assert reopened.locked == locks
    files = library.profiles()[0].files
    validate_runtime(files, {}, game, library.root / "runtime" / "profiles" / profile_id / "overwrite")
    files["lockedorder.txt"] = "Addon.esp|1\nAbsent.esp|8\n"
    with pytest.raises(ManagerError, match="locks changed"):
        validate_runtime(files, {}, game, library.root / "runtime" / "profiles" / profile_id / "overwrite")
    with pytest.raises(ManagerError, match="locked|Locked"):
        review(catalog, locked=(("Addon.esp", 1),))
    with pytest.raises(ManagerError, match="cannot be locked"):
        review(catalog, locked=(("Fallout4.esm", 0),))
    assert json.loads(library.profiles()[0].files["_modbox_plugin_backup"])["lockedorder.txt"] == ""


@pytest.mark.parametrize("text", ["Addon.esp|-1", "Addon.esp|1|2", "Addon.esp|10000", "Addon.esp|1\naddon.ESP|2", "../Addon.esp|1", "Addon.esp|１"])
def test_malformed_locked_order_fails_explicitly(text):
    with pytest.raises(ManagerError):
        locked_order(text)


def test_full_light_disabled_indices_and_unknown_header_are_truthful(plugin_fixture):
    _, _, _, catalog = plugin_fixture
    light = replace(catalog.plugins[-1], name="Fixture.esl", header={"flags": {"esl": True}})
    catalog = replace(catalog, plugins=(*catalog.plugins, light))
    order = [p.name for p in catalog.plugins]
    values = load_indices(catalog, order, order)
    assert values == {"fallout4.esm": "00", "dlcrobot.esm": "01", "addon.esp": "02", "fixture.esl": "FE:000"}
    assert load_indices(catalog, order, order[:-1])["fixture.esl"] == "—"
    unknown = replace(catalog, plugins=tuple(replace(p, header=None) if p.name == "Addon.esp" else p for p in catalog.plugins))
    assert set(load_indices(unknown, order, order).values()) == {"?"}


def test_plugin_inventory_rejects_relative_game_context_before_scanning(plugin_fixture):
    library, profile_id, _, _ = plugin_fixture
    with library._db() as db:
        row = db.execute("SELECT id,configuration FROM instances").fetchone()
        config = json.loads(row["configuration"])
        config["game"]["gamepath"] = "relative/game"
        db.execute("UPDATE instances SET configuration=? WHERE id=?", (json.dumps(config), row["id"]))
    with pytest.raises(ManagerError, match="absolute"):
        inventory(library, profile_id)


def test_cache_hit_cancellation_does_not_fall_through_to_reader(plugin_fixture):
    library, _, _, catalog = plugin_fixture
    class CancelAfterCacheCheck:
        checks = 0
        def is_set(self):
            self.checks += 1
            return self.checks > 1
    cache = CachedMetadata(library, lambda *args: ({"flags": {}}, ()))
    cache(catalog.plugins[0].path, "fo4")
    cache.reader = lambda *args: pytest.fail("Cancelled cache hit called the expensive reader")
    with pytest.raises(Cancelled):
        cache(catalog.plugins[0].path, "fo4", CancelAfterCacheCheck())
