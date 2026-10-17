from dataclasses import asdict
import json
import threading

import pytest

from creation_lib.mod_manager import launch_targets, plugin_state
from creation_lib.mod_manager import profile_backup
from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.profile_state import restore_ini, save_ini
from creation_lib.mod_manager.runtime import active_sessions, prepare
from .test_controller import finish
from .test_pe_metadata import fixture_version, image


def test_logical_profile_clone_retains_each_targets_ini_and_plugin_overlay(targets):
    library, profile, _, ids = targets
    views = [launch_targets.TargetLibrary(library, identifier) for identifier in ids]
    for index, view in enumerate(views):
        current = view.profile_snapshot(profile)[0].files
        view.save_ini(profile, "Fallout4Prefs.ini", f"[Display]\nTarget={index}\n", current["Fallout4Prefs.ini"])
        with library._db() as db:
            current = view.profile_files(db, profile, json.loads(db.execute("SELECT files FROM profiles WHERE id=?", (profile,)).fetchone()[0]))
            view.write_profile_fields(db, profile, {**current, "plugins.txt": f"# target {index}\n"})
    original = library.profile_snapshot(profile)
    clone = library.create_profile("Cloned target settings", profile)
    assert library.profile_snapshot(clone)[0].files == original[0].files
    assert library.rows(clone) == original[1]
    for view in views:
        source, copied = (view.profile_snapshot(identifier)[0].files for identifier in (profile, clone))
        assert all(copied.get(name) == source.get(name) for name in launch_targets.PROFILE_FIELDS)
    assert all(not (view.root / "runtime/profiles" / clone).exists() for view in views)


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_profile_clone_target_overlays_and_logical_metadata_roll_back_together(targets, monkeypatch, outcome):
    library, profile, _, ids = targets
    view = launch_targets.TargetLibrary(library, ids[0])
    current = view.profile_snapshot(profile)[0].files
    view.save_ini(profile, "Fallout4Prefs.ini", "[Display]\nRetained=1\n", current["Fallout4Prefs.ini"])
    before = library.profiles()
    with library._db() as db:
        overlays = [tuple(row) for row in db.execute("SELECT * FROM launch_target_profiles ORDER BY target_id,profile_id")]
    cancel = threading.Event()
    def fault():
        if outcome == "cancelled":
            cancel.set()
        else:
            raise OSError("Fixture clone commit failure")
    monkeypatch.setattr(library, "_profile_create_checkpoint", fault)
    with pytest.raises(Cancelled if outcome == "cancelled" else OSError):
        library.create_profile("Never published", profile, cancel=cancel)
    assert library.profiles() == before
    with library._db() as db:
        assert [tuple(row) for row in db.execute("SELECT * FROM launch_target_profiles ORDER BY target_id,profile_id")] == overlays


@pytest.fixture
def targets(tmp_path, monkeypatch):
    monkeypatch.setattr("creation_lib.mod_manager.pe_metadata.file_version", fixture_version)
    library = Library(tmp_path / "library", create=True)
    games = []
    for index, build in enumerate(((1, 10, 163, 0), (1, 11, 240, 0))):
        game = tmp_path / ("game" + str(index))
        (game / "Data").mkdir(parents=True)
        image(game / "Fallout4.exe", version=build)
        (game / "Fallout4_default.ini").write_text(f"[General]\nTarget={index}\n")
        (game / "Data" / "Fallout4.esm").write_bytes(b"inert header " + bytes([index]))
        if index:
            (game / "Data" / "DLCRobot.esm").write_bytes(b"target-only header")
        games.append(game)
    profile = create(library, "fo4", games[0])
    plans = [launch_targets.review(library, "FO4 " + str(index), game) for index, game in enumerate(games)]
    # Each review is frozen against the list, so review/apply sequentially.
    ids = [launch_targets.apply(library, plans[0])]
    ids.append(launch_targets.apply(library, launch_targets.review(library, "FO4 1", games[1])))
    return library, profile, games, ids


def test_shared_mods_and_target_inis_plugin_state_defaults_reopen(targets):
    library, profile, games, ids = targets
    original = library.profiles()[0].files.copy()
    views = [launch_targets.TargetLibrary(library, identifier) for identifier in ids]
    assert views[0].root != views[1].root
    assert views[0].library_id != views[1].library_id != library.library_id
    for index, view in enumerate(views):
        local, rows, configuration = view.profile_snapshot(profile)
        assert f"Target={index}" in local.files["Fallout4.ini"]
        assert configuration["game"]["gamepath"] == str(games[index])
        assert rows == library.rows(profile)
        save_ini(view, profile, "Fallout4.ini", f"[General]\nEdited={index}\n", local.files["Fallout4.ini"])
    assert library.profiles()[0].files == original
    assert "Edited=0" in views[0].profiles()[0].files["Fallout4.ini"]
    assert "Edited=1" in launch_targets.TargetLibrary(Library(library.root), ids[1]).profiles()[0].files["Fallout4.ini"]
    current = views[0].profiles()[0].files["Fallout4.ini"]
    restore_ini(views[0], profile, "Fallout4.ini", current)
    assert "Target=0" in views[0].profiles()[0].files["Fallout4.ini"]
    def reader(path, game, cancel=None):
        return {"flags": {"esm": True}}, (() if path.name == "Fallout4.esm" else ("Fallout4.esm",))
    for view in views:
        catalog = plugin_state.inspect(plugin_state.inventory(view, profile), reader)
        plugin_state.apply(view, plugin_state.review(catalog))
    first, second = [view.profiles()[0] for view in views]
    assert "DLCRobot.esm" not in first.files["plugins.txt"]
    assert "DLCRobot.esm" in second.files["plugins.txt"]
    assert library.profiles()[0].files == original
    assert plugin_state.signature(views[0], profile) != plugin_state.signature(views[1], profile)


def test_repeat_review_and_cancel_failure_stale_input_transaction(targets, monkeypatch):
    library, _, games, ids = targets
    initial = [asdict(item) for item in launch_targets.records(library)]
    repeat = launch_targets.review(library, "FO4 0", games[0])
    assert launch_targets.apply(library, repeat) == ids[0]
    assert [asdict(item) for item in launch_targets.records(library)] == initial
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        launch_targets.apply(library, repeat, cancel)
    def failure():
        raise RuntimeError("generated fault")
    monkeypatch.setattr(launch_targets, "_checkpoint", failure)
    plan = launch_targets.review(library, "Another", games[0])
    with pytest.raises(RuntimeError):
        launch_targets.apply(library, plan)
    assert len(launch_targets.records(library)) == 2
    monkeypatch.setattr(launch_targets, "_checkpoint", lambda: (games[0] / "Fallout4_default.ini").write_text("[General]\nChanged=1\n"))
    with pytest.raises(ManagerError, match="changed after review"):
        launch_targets.apply(library, plan)
    assert len(launch_targets.records(library)) == 2


def test_target_edit_and_plugin_review_stale_signature(targets):
    library, profile, games, ids = targets
    view = launch_targets.TargetLibrary(library, ids[0])
    catalog = plugin_state.inventory(view, profile)
    plan = launch_targets.review(library, "Moved target", games[1], identifier=ids[0])
    launch_targets.apply(library, plan)
    assert plugin_state.signature(view, profile) != catalog.signature
    assert view.profile_snapshot(profile)[2]["game"]["gamepath"] == str(games[1])


def test_controller_switch_clears_context_preserves_modlist_and_settings(targets):
    library, profile, _, ids = targets
    manager = ManagerController()
    try:
        manager.library, manager.profile_id = library, profile
        manager.refresh()
        original_executables = list(manager.executables)
        manager.select_target(ids[0])
        first = manager.context_library
        manager.plugin_catalog = object()
        manager.combined_index = object()
        manager.save_inspections = {"old": object()}
        manager.select_target(ids[1])
        assert manager.context_library is not first
        assert manager.plugin_catalog is manager.combined_index is None
        assert manager.save_inspections == {}
        assert manager.executables == original_executables
        manager.select_target("")
        assert manager.context_library is library
        manager.review_target("New", library.root)
        with pytest.raises(ManagerError):
            manager._future.result(timeout=10)
        manager.poll()
        assert manager.target_review is None and manager.error
    finally:
        manager.cleanup()


def test_target_active_runtime_locks_base_and_switch_and_recovery(targets, monkeypatch):
    library, profile, _, ids = targets
    view = launch_targets.TargetLibrary(library, ids[0])
    runtime_profile = view.root / "runtime" / "profiles" / profile
    runtime_profile.mkdir(parents=True)
    session = view.root / "runtime" / "sessions" / "generated"
    session.mkdir(parents=True)
    (runtime_profile / "active.json").write_text(json.dumps({"session": session.name}))
    (session / "status.json").write_text(json.dumps({"state": "running"}))
    monkeypatch.setattr("creation_lib.mod_manager.process_identity.session_liveness", lambda _: "unverified")
    assert active_sessions(library.root)[0][0] == session
    with pytest.raises(ManagerError, match="running"):
        library.create_profile("Blocked")
    manager = ManagerController()
    try:
        manager.library = library
        manager.refresh()
        assert manager.runtime_active
        manager.select_target(ids[1])
        assert manager.target_id == ids[1] and manager.runtime_active
    finally:
        manager.cleanup()


def test_unready_target_cannot_fall_back_to_game_executable(targets, tmp_path):
    library, profile, games, ids = targets
    view = launch_targets.TargetLibrary(library, ids[0])
    bundle = tmp_path / "bundle"
    bundle.mkdir()
    with pytest.raises(ManagerError, match="F4SE"):
        prepare(view, profile, view.target().executable, games[0], bundle)
    with library._db() as db:
        files = json.loads(db.execute("SELECT files FROM profiles WHERE id=?", (profile,)).fetchone()[0])
        files["_modbox_game"] = "Skyrim"
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile))
    assert view.profiles() == []
    with pytest.raises(ManagerError, match="Fallout 4 logical profile"):
        view.profile_snapshot(profile)


def test_controller_target_review_decline_and_repeated_apply(targets):
    library, profile, games, ids = targets
    manager = ManagerController()
    try:
        manager.library, manager.profile_id = library, profile
        manager.refresh()
        manager.review_target("Reviewed", games[0])
        finish(manager)
        assert len(manager.targets) == 2
        manager.target_review = None
        assert len(launch_targets.records(library)) == 2
        manager.review_target("FO4 0", games[0])
        finish(manager)
        manager.apply_target(automatic=False)
        finish(manager)
        assert manager.target_id == ids[0] and len(manager.targets) == 2
    finally:
        manager.cleanup()


def test_target_metadata_backups_reject_cross_target_and_restore_only_overlay(targets):
    library, profile, _, ids = targets
    first, second = [launch_targets.TargetLibrary(library, value) for value in ids]
    original = library.profiles()[0].files.copy()
    backup = profile_backup.capture(first, profile)
    assert profile_backup.history(first, profile)[0]["id"] == backup
    assert profile_backup.history(second, profile) == profile_backup.history(library, profile) == []
    for view in (second, library):
        with pytest.raises(ManagerError, match="another launch target"):
            profile_backup.review(view, profile, backup)
        with pytest.raises(ManagerError, match="another launch target"):
            profile_backup.remove(view, profile, backup)
    saved = first.profiles()[0].files["Fallout4.ini"]
    save_ini(first, profile, "Fallout4.ini", "[General]\nChanged=1\n", saved)
    plan = profile_backup.review(first, profile, backup)
    profile_backup.apply(first, plan)
    assert first.profiles()[0].files["Fallout4.ini"] == saved
    assert library.profiles()[0].files == original
    assert "Changed=1" not in second.profiles()[0].files["Fallout4.ini"]


def test_target_output_save_state_and_interrupted_recovery_are_disjoint(targets):
    from creation_lib.mod_manager import overwrite, save_transfer
    library, profile, _, ids = targets
    first, second = [launch_targets.TargetLibrary(library, value) for value in ids]
    output = first.root / "runtime" / "profiles" / profile / "overwrite"
    output.mkdir(parents=True)
    (output / "generated.txt").write_bytes(b"target-one")
    plan = overwrite.review(first, profile)
    assert plan.files == 1 and overwrite.review(second, profile).files == 0
    overwrite.clear(first, plan)
    assert overwrite.history(first, profile) and not overwrite.history(second, profile)
    save_root = save_transfer.root(first, profile)
    save_root.mkdir(parents=True, exist_ok=True)
    (save_root / "fixture.fos").write_bytes(b"target-save")
    assert not save_transfer.root(second, profile).exists()
    pending = first.root / "f4se" / ("pending-" + "a" * 32)
    pending.mkdir(parents=True)
    (pending / "partial.zip").write_bytes(b"incomplete ignored")
    reopened = Library(library.root)
    assert any("interrupted F4SE preparation" in item for item in reopened.recovery_report)
    assert (pending / "partial.zip").read_bytes() == b"incomplete ignored"
    assert launch_targets.records(reopened)[0].f4se["state"] != "ready"


def test_target_profile_ini_stale_failure_cancel_and_same_shared_sources(targets, monkeypatch):
    library, profile, _, ids = targets
    view = launch_targets.TargetLibrary(library, ids[0])
    before = view.profiles()[0].files["Fallout4.ini"]
    with pytest.raises(ManagerError, match="changed"):
        save_ini(view, profile, "Fallout4.ini", "[General]\nChanged=1\n", "stale")
    cancel = threading.Event()
    monkeypatch.setattr(launch_targets, "_checkpoint", cancel.set)
    with pytest.raises(Cancelled):
        save_ini(view, profile, "Fallout4.ini", "[General]\nChanged=1\n", before, cancel)
    assert view.profiles()[0].files["Fallout4.ini"] == before
    monkeypatch.setattr(launch_targets, "_checkpoint", lambda: (_ for _ in ()).throw(OSError("fixture failure")))
    with pytest.raises(OSError):
        save_ini(view, profile, "Fallout4.ini", "[General]\nChanged=1\n", before)
    assert view.profiles()[0].files["Fallout4.ini"] == before


def test_mo2_reference_repeat_and_fomod_target_context_install_preserves_source(targets, tmp_path):
    from .fixtures import make_mo2, make_package
    from creation_lib.mod_manager.mo2 import inspect_instance
    from creation_lib.mod_manager import installer_context
    from creation_lib.mod_manager.fomod import Fomod
    from creation_lib.mod_manager.package import Package
    from contextlib import closing
    library, profile, games, ids = targets
    (tmp_path / "import fixture").mkdir()
    source = make_mo2(tmp_path / "import fixture")
    original = {str(path): path.read_bytes() for path in source.rglob("*") if path.is_file()}
    preview = inspect_instance(source)
    library.import_mo2(preview)
    imported = next(item.id for item in library.profiles() if item.source)
    first, second = [launch_targets.TargetLibrary(library, identifier) for identifier in ids]
    saved = next(item for item in first.profiles() if item.id == imported).files["Fallout4Custom.ini"]
    save_ini(first, imported, "Fallout4Custom.ini", "[General]\nLocal=one\n", saved)
    # Read the profile by stable identity; all profiles are sorted by display name.
    local = next(item for item in first.profiles() if item.id == imported)
    library.import_mo2(preview)
    assert next(item for item in first.profiles() if item.id == imported).files == local.files
    (tmp_path / "installer fixture").mkdir()
    package_path = make_package(tmp_path / "installer fixture")
    with closing(Package(package_path)) as package:
        fomod = Fomod(package.read_config(), compatibility="mo2")
        context, _ = installer_context.capture(first, imported, fomod)
        with pytest.raises(ManagerError, match="different library"):
            installer_context.validate(second, context)
        (games[0] / "Data" / "new-context.txt").write_bytes(b"target-specific changed provider")
        with pytest.raises(ManagerError, match="changed"):
            installer_context.validate(first, context)
    assert {str(path): path.read_bytes() for path in source.rglob("*") if path.is_file()} == original
