"""Scope switches retain owned state and cannot select real global save/configuration paths."""
from dataclasses import replace
import json
import threading

import pytest

from creation_lib.mod_manager import target_preferences as preferences
from creation_lib.mod_manager.launch_targets import TargetLibrary
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.runtime import prepare, profile_signature
from creation_lib.mod_manager.runtime_helper import run
from creation_lib.mod_manager.saves import inventory
from .test_launch_targets import targets  # noqa: F401
from .test_runtime import FakeVFS, request
from .test_runtime_setup import make_bundle


@pytest.mark.parametrize("ini", ["target", "shared"])
@pytest.mark.parametrize("saves", ["target", "shared"])
def test_independent_scopes_preserve_owned_files_and_latent_local_ini(targets, ini, saves):  # noqa: F811
    library, profile, _, ids = targets
    view = TargetLibrary(library, ids[1])
    previous = view.profile_snapshot(profile)[0].files["Fallout4.ini"]
    view.save_ini(profile, "Fallout4.ini", "[General]\nLocal=retained\n", previous)
    local_save = view.root / "runtime/profiles" / profile / "saves/local.fos"
    shared_save = library.root / "runtime/profiles" / profile / "saves/shared.fos"
    for file in (local_save, shared_save):
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_bytes(file.name.encode())
    original = library.profiles()[0].files.copy()
    preferences.apply(library, preferences.review(library, ids[1], profile, ini=ini, saves=saves))
    current = view.profile_snapshot(profile)[0].files
    assert ("Local=retained" in current["Fallout4.ini"]) == (ini == "target")
    assert view.owned_save_root(profile) == (shared_save if saves == "shared" else local_save).parent
    save_view = preferences.SaveLibrary(view) if saves == "shared" else view
    assert [file.name for file in inventory(save_view, profile).files] == ["shared.fos" if saves == "shared" else "local.fos"]
    if ini == "shared":
        with pytest.raises(ManagerError, match="read-only"):
            view.save_ini(profile, "Fallout4.ini", "[General]\nChanged=1\n", current["Fallout4.ini"])
        # A plugin-state write must not overwrite dormant target INIs with shared texts.
        with library._db() as db:
            db.execute("BEGIN IMMEDIATE")
            view.write_profile_fields(db, profile, {**current, "plugins.txt": "# fixture activation\n"})
    preferences.apply(library, preferences.review(library, ids[1], profile))
    assert "Local=retained" in view.profile_snapshot(profile)[0].files["Fallout4.ini"]
    assert library.profiles()[0].files == original
    assert local_save.read_bytes() == b"local.fos" and shared_save.read_bytes() == b"shared.fos"


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_preferences_rollback_and_stale_signature(targets, monkeypatch, outcome):  # noqa: F811
    library, profile, _, ids = targets
    view = TargetLibrary(library, ids[0])
    before = profile_signature(view, profile)
    plan = preferences.review(library, ids[0], profile, saves="shared")
    cancel = threading.Event()
    def fault():
        if outcome == "cancelled":
            cancel.set()
        else:
            raise OSError("Fixture preference SQL failure")
    monkeypatch.setattr(preferences, "_checkpoint", fault)
    with pytest.raises((Cancelled, OSError)):
        preferences.apply(library, plan, cancel)
    assert profile_signature(view, profile) == before


@pytest.mark.parametrize("shared", [False, True])
def test_helper_maps_only_canonical_selected_owned_save_scope(targets, tmp_path, monkeypatch, shared):  # noqa: F811
    library, profile, games, ids = targets
    view = TargetLibrary(library, ids[0])
    if shared:
        preferences.apply(library, preferences.review(library, ids[0], profile, saves="shared"))
    monkeypatch.setattr("creation_lib.mod_manager.f4se_setup.validate_launch", lambda *args, **kwargs: {"build": [1, 10, 163, 0], "version": "0.6.23"})
    # The readiness seam is fixture-only; no executable or runtime DLL is executed.
    bundle = make_bundle(tmp_path / "bundle")
    user = tmp_path / "user"
    for name in ("plugins", "ini"):
        (user / name).mkdir(parents=True)
    from creation_lib.mod_manager.target_options import executable
    plan = prepare(view, profile, executable(library, view.target()), str(games[0]), str(bundle), user_targets=(user / "plugins", user / "ini"), isolate_documents=True)
    expected = (library.root if shared else view.root) / "runtime/profiles" / profile / "saves"
    expected.mkdir(parents=True, exist_ok=True)
    (expected / "fixture.fos").write_bytes(b"baseline save")
    assert plan.save_root == str(expected)
    assert plan.save_subpath.endswith(profile) == shared
    adapters = []
    def factory(*args):
        adapter = FakeVFS(*args)
        adapters.append(adapter)
        return adapter
    foreign = tmp_path / "unapproved save path"
    path = request(view, replace(plan, save_root=str(foreign)))
    assert run(path, native_factory=factory, wait=lambda _: None) == 1
    assert not adapters and not foreign.exists()
    path.write_text(json.dumps(replace(plan, save_subpath="unapproved_namespace").record()), encoding="utf-8")
    assert run(path, native_factory=factory, wait=lambda _: None) == 1
    assert not adapters
    path.write_text(json.dumps(plan.record()), encoding="utf-8")
    assert run(path, native_factory=factory, wait=lambda _: None) == 0
    isolated = path.parent / "output" / "saves"
    assert ("directory", str(isolated), str(user / "ini" / plan.save_subpath), True) in adapters[0].operations
    assert (isolated / "fixture.fos").read_bytes() == b"baseline save"
    (isolated / "fixture.fos").write_bytes(b"session save")
    assert (expected / "fixture.fos").read_bytes() == b"baseline save"
    assert expected.is_dir() and not (user / "ini" / plan.save_subpath).exists()


def test_unsupported_global_scope_is_rejected(targets):  # noqa: F811
    library, profile, _, ids = targets
    with pytest.raises(ManagerError, match="manager-shared"):
        preferences.review(library, ids[0], profile, saves="global")


def test_stale_preferences_refused(targets):  # noqa: F811
    library, profile, _, ids = targets
    plan = preferences.review(library, ids[0], profile, saves="shared")
    preferences.apply(library, preferences.review(library, ids[0], profile, ini="shared"))
    with pytest.raises(ManagerError, match="changed after"):
        preferences.apply(library, plan)
    assert preferences.value(library, ids[0]) == {"ini": "shared", "saves": "target"}


def test_controller_shared_save_copy_backup_and_target_switch_use_correct_owner(targets):  # noqa: F811
    from creation_lib.mod_manager.controller import ManagerController
    from .test_controller import finish
    from .test_saves import save_fixture
    library, profile, _, ids = targets
    other = library.create_profile("Other logical profile", clone=profile)
    root = library.root / "runtime/profiles" / profile / "saves"
    root.mkdir(parents=True)
    save = save_fixture(root / "Fixture.fos")
    save.with_suffix(".f4se").write_bytes(b"owned companion")
    preferences.apply(library, preferences.review(library, ids[0], profile, saves="shared"))
    manager = ManagerController()
    try:
        manager.library, manager.profile_id = library, profile
        manager.refresh()
        manager.select_target(ids[0])
        manager.inspect_saves()
        finish(manager)
        manager.review_save_copy(str(save), other)
        finish(manager)
        manager.apply_save_copy()
        finish(manager)
        assert (library.root / "runtime/profiles" / other / "saves/Fixture.fos").read_bytes() == save.read_bytes()
        manager.inspect_saves()
        finish(manager)
        manager.review_save_copy(str(save))
        finish(manager)
        manager.apply_save_copy()
        finish(manager)
        manager.inspect_save_backups()
        finish(manager)
        assert len(manager.save_backups) == 1 and (library.root / ".save-history" / profile).is_dir()
        manager.select_target(ids[1])
        assert manager.save_library.root == manager.context_library.root and not manager.save_index and not manager.save_backups
        assert save.is_file() and save.with_suffix(".f4se").read_bytes() == b"owned companion"
    finally:
        manager.cleanup()
