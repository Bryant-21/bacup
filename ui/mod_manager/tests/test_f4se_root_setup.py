"""Root placement operates only on generated inert game/package fixtures."""
from dataclasses import asdict
import json
from pathlib import Path
import threading

import pytest

from creation_lib.mod_manager import f4se_setup, f4se_root_setup as roots, launch_targets
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.runtime import prepare
from .test_f4se_setup import package, setup
from .test_pe_metadata import image
from .test_runtime import pe, request, FakeVFS


def prepared(tmp_path, monkeypatch, build=(1, 10, 163, 0)):
    monkeypatch.setattr("creation_lib.mod_manager.runtime.process_names", lambda: {"steam.exe"})
    library, profile, game, identifier = setup(tmp_path, monkeypatch, build)
    f4se_setup.prepare(library, identifier, archive=package(tmp_path, build))
    return library, profile, game, identifier


def files(folder):
    return {str(path.relative_to(folder)): path.read_bytes() for path in folder.rglob("*") if path.is_file()}


@pytest.mark.parametrize("build", [(1, 10, 163, 0), (1, 10, 984, 0), (1, 11, 240, 0)])
def test_standard_bootstrap_three_builds_review_ack_repeat_reopen_restore_no_data_or_mod_copies(tmp_path, monkeypatch, build):
    library, profile, game, identifier = prepared(tmp_path, monkeypatch, build)
    image(game / "f4se_loader.exe", version=(0, 1, 0, 0))
    (game / "f4se_steam_loader.dll").write_bytes(b"custom legacy file retained for newer release")
    (game / "Data" / "custom.asset").write_bytes(b"game Data preserved")
    source = tmp_path / "source mods"
    source.mkdir()
    (source / "marker.txt").write_bytes(b"referenced, not copied")
    before, original = files(game), asdict(launch_targets.records(library)[0])
    plan = roots.review(library, identifier)
    assert len(plan.files) == (3 if build[:3] == (1, 10, 163) else 2)
    with pytest.raises(ManagerError, match="Acknowledge"):
        roots.apply(library, plan)
    assert files(game) == before
    target = roots.apply(library, plan, accept_root_writes=True)
    assert target.binary == str(game / "f4se_loader.exe") and target.f4se["route"] == "game-root"
    view = launch_targets.TargetLibrary(library, identifier)
    f4se_setup.validate_ready(target, owner_root=view.root)
    assert target.f4se["data"] == original["f4se"]["data"]
    assert view.profile_snapshot(profile)[1][0].source == target.f4se["data"]
    assert not (game / "Data" / "Scripts").exists() and (game / "Data" / "custom.asset").read_bytes() == b"game Data preserved"
    assert (source / "marker.txt").read_bytes() == b"referenced, not copied"
    assert len(roots.history(library, identifier)) == 1 and not (game / roots.LOCK).exists()
    repeated = roots.review(library, identifier)
    assert not repeated.files and roots.apply(library, repeated, accept_root_writes=True) == target
    reopened = Library(library.root)
    restore = roots.review_restore(reopened, identifier)
    with pytest.raises(ManagerError, match="Acknowledge"):
        roots.restore(reopened, restore)
    restored = roots.restore(reopened, restore, accept_root_writes=True)
    assert asdict(restored) == original and files(game) == before
    assert roots.history(reopened, identifier)[0]["state"] == "restored"
    assert not (game / roots.LOCK).exists()


@pytest.mark.parametrize("fault", ["pre-cancel", "copy-cancel", "copy-failure", "publish-cancel", "publish-failure"])
def test_cancelled_failed_install_rolls_back_every_root_component(tmp_path, monkeypatch, fault):
    library, _, game, identifier = prepared(tmp_path, monkeypatch)
    (game / "f4se_loader.exe").write_bytes(b"custom old loader")
    original, target = files(game), asdict(launch_targets.records(library)[0])
    plan, cancel = roots.review(library, identifier), threading.Event()
    if fault == "pre-cancel":
        cancel.set()
    def checkpoint(phase):
        if phase == ("installed-file" if fault.startswith("copy") else "before-publish"):
            if fault.endswith("cancel"):
                cancel.set()
            else:
                raise OSError("generated root failure")
    monkeypatch.setattr(roots, "_checkpoint", checkpoint)
    with pytest.raises((OSError, Cancelled)):
        roots.apply(library, plan, accept_root_writes=True, cancel=cancel)
    assert files(game) == original and asdict(launch_targets.records(library)[0]) == target
    assert all(item["state"] == "rolled-back" for item in roots.history(library, identifier))


@pytest.mark.parametrize("change", ["destination", "exe", "target", "stage"])
def test_stale_setup_review_refuses_any_root_write(tmp_path, monkeypatch, change):
    library, _, game, identifier = prepared(tmp_path, monkeypatch)
    plan = roots.review(library, identifier)
    if change == "destination":
        (game / "f4se_loader.exe").write_bytes(b"later custom")
    elif change == "exe":
        image(game / "Fallout4.exe", version=(1, 10, 984, 0))
    elif change == "stage":
        target = launch_targets.records(library)[0]
        (Path(target.f4se["stage"]) / "Data/Scripts/F4SE.pex").write_bytes(b"later source")
    else:
        with library._db() as db:
            item = asdict(launch_targets.records(library)[0])
            item["arguments"] = "-later"
            db.execute("UPDATE launch_targets SET record=? WHERE id=?", (json.dumps(item), identifier))
    before = files(game)
    with pytest.raises(ManagerError):
        roots.apply(library, plan, accept_root_writes=True)
    assert files(game) == before


@pytest.mark.parametrize("phase", ["before-copy", "installed-file", "before-publish", "restore-file"])
def test_crash_requires_explicit_recovery_and_preserves_sources(tmp_path, monkeypatch, phase):
    library, _, game, identifier = prepared(tmp_path, monkeypatch)
    (game / "f4se_loader.exe").write_bytes(b"custom before crash")
    original = files(game)
    if phase == "restore-file":
        roots.apply(library, roots.review(library, identifier), accept_root_writes=True)
    def checkpoint(value):
        if value == phase:
            raise KeyboardInterrupt("generated crash")
    monkeypatch.setattr(roots, "_checkpoint", checkpoint)
    with pytest.raises(KeyboardInterrupt):
        if phase == "restore-file":
            roots.restore(library, roots.review_restore(library, identifier), accept_root_writes=True)
        else:
            roots.apply(library, roots.review(library, identifier), accept_root_writes=True)
    interrupted = files(game)
    assert (game / roots.LOCK).exists()
    reopened = Library(library.root)
    assert files(game) == interrupted  # Reopening never writes to a game or silently restores it.
    assert any("root" in text.casefold() for text in reopened.recovery_report)
    monkeypatch.setattr(roots, "_checkpoint", lambda _: None)
    restored = roots.restore(reopened, roots.review_restore(reopened, identifier), accept_root_writes=True)
    assert files(game) == original and not restored.f4se.get("root_setup")


@pytest.mark.parametrize("change", ["root", "backup", "review", "foreign-lock"])
def test_restore_refuses_unrelated_changes_and_preserves_backup(tmp_path, monkeypatch, change):
    library, _, game, identifier = prepared(tmp_path, monkeypatch)
    (game / "f4se_loader.exe").write_bytes(b"custom before")
    target = roots.apply(library, roots.review(library, identifier), accept_root_writes=True)
    plan = roots.review_restore(library, identifier)
    if change in {"root", "review"}:
        (game / "f4se_loader.exe").write_bytes(b"other user's new component")
    elif change == "backup":
        backup = roots._base(library, identifier) / target.f4se["root_setup"] / "backup/f4se_loader.exe"
        backup.write_bytes(b"tampered backup")
    else:
        (game / roots.LOCK).write_bytes(b'0{"id":"foreign"}')
    before = files(game)
    with pytest.raises(ManagerError):
        roots.restore(library, plan, accept_root_writes=True)
    if change == "backup":
        assert {name: value for name, value in files(game).items() if name != roots.LOCK} == before
    else:
        assert files(game) == before


def test_root_route_uses_normal_loader_without_external_overrides_or_root_virtual_mappings(tmp_path, monkeypatch):
    from creation_lib.mod_manager.runtime_helper import run
    library, profile, game, identifier = prepared(tmp_path, monkeypatch)
    target = roots.apply(library, roots.review(library, identifier), accept_root_writes=True)
    view = launch_targets.TargetLibrary(library, identifier)
    bundle = tmp_path / "bundle"
    bundle.mkdir()
    for name, machine in (("usvfs_x64.dll", 0x8664), ("usvfs_x86.dll", 0x14c), ("usvfs_proxy_x64.exe", 0x8664), ("usvfs_proxy_x86.exe", 0x14c)):
        pe(bundle / name, machine)
    plan = prepare(view, profile, target.executable, game, bundle, user_targets=(tmp_path / "plugins", tmp_path / "ini"))
    assert not plan.experimental and not plan.root_files
    assert "-altexe" not in plan.launch.argv and "-altdll" not in plan.launch.argv
    assert plan.launch.argv[0] == str(game / "f4se_loader.exe")
    assert any(name == "Scripts/F4SE.pex" for name, *_ in plan.files)
    assert run(request(view, plan), native_factory=FakeVFS, wait=lambda _: None) == 0


def test_active_process_pending_lock_edit_and_reprepare_are_blocked(tmp_path, monkeypatch):
    library, _, game, identifier = prepared(tmp_path, monkeypatch)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.process_names", lambda: {"Fallout4.exe".casefold()})
    with pytest.raises(ManagerError, match="Close all"):
        roots.review(library, identifier)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.process_names", lambda: set())
    target = roots.apply(library, roots.review(library, identifier), accept_root_writes=True)
    with pytest.raises(ManagerError, match="Restore"):
        f4se_setup.prepare(library, identifier, archive=tmp_path / "fixture.zip")
    with pytest.raises(ManagerError, match="Restore"):
        launch_targets.review(library, target.name, game, identifier=identifier)


def test_cancelled_restore_before_writes_keeps_completed_setup_and_releases_temporary_lock(tmp_path, monkeypatch):
    library, _, game, identifier = prepared(tmp_path, monkeypatch)
    roots.apply(library, roots.review(library, identifier), accept_root_writes=True)
    plan, cancel = roots.review_restore(library, identifier), threading.Event()
    before = files(game)
    def checkpoint(phase):
        if phase == "before-restore":
            cancel.set()
    monkeypatch.setattr(roots, "_checkpoint", checkpoint)
    with pytest.raises(Cancelled):
        roots.restore(library, plan, accept_root_writes=True, cancel=cancel)
    assert files(game) == before and roots.history(library, identifier)[0]["state"] == "complete"


@pytest.mark.parametrize("fault", ["foreign-game", "foreign-target", "unsafe-component"])
def test_foreign_or_unsafe_receipt_is_read_only_refused(tmp_path, monkeypatch, fault):
    library, _, game, identifier = prepared(tmp_path, monkeypatch)
    target = roots.apply(library, roots.review(library, identifier), accept_root_writes=True)
    path = roots._base(library, identifier) / target.f4se["root_setup"] / "receipt.json"
    record = json.loads(path.read_text("utf-8"))
    if fault == "foreign-game":
        record["game"] = record["before"]["game_root"] = str(tmp_path / "another game")
    elif fault == "foreign-target":
        record["after"]["id"] = "a" * 32
    else:
        record["files"]["../outside.dll"] = record["files"].pop("f4se_loader.exe")
    path.write_text(json.dumps(record), encoding="utf-8")
    before = files(game)
    with pytest.raises(ManagerError, match="Invalid/foreign"):
        roots.review_restore(library, identifier)
    assert files(game) == before


def test_same_folder_second_target_cannot_overlap_root_ownership(tmp_path, monkeypatch):
    library, _, game, identifier = prepared(tmp_path, monkeypatch)
    second = launch_targets.apply(library, launch_targets.review(library, "Second name", game))
    f4se_setup.prepare(library, second, archive=tmp_path / "fixture.zip")
    roots.apply(library, roots.review(library, identifier), accept_root_writes=True)
    before = files(game)
    with pytest.raises(ManagerError, match="Another target"):
        roots.review(library, second)
    assert files(game) == before


def test_root_preimage_limit_and_link_refusal_are_read_only(tmp_path, monkeypatch):
    from .test_mo2 import junction
    library, _, game, identifier = prepared(tmp_path, monkeypatch)
    oversized = game / "f4se_loader.exe"
    with oversized.open("wb") as stream:
        stream.truncate(roots.MAX_FILE + 1)
    with pytest.raises(ManagerError, match="16 MiB"):
        roots.review(library, identifier)
    oversized.unlink()
    moved = tmp_path / "linked target"
    game.rename(moved)
    junction(game, moved)
    with pytest.raises(ManagerError, match="reparse|Links"):
        roots.review(library, identifier)
    assert not (moved / roots.LOCK).exists()


@pytest.mark.parametrize("stale", [False, True])
def test_partial_root_copy_is_preserved_then_explicitly_reviewed_for_recovery(tmp_path, monkeypatch, stale):
    library, _, game, identifier = prepared(tmp_path, monkeypatch)
    original = files(game)
    real_copy = roots._copy
    def fail_copy(source, destination, operation, expected):
        temporary = destination.parent / (".modbox21-f4se-" + operation + "-" + destination.name + ".tmp")
        temporary.write_bytes(b"generated partial disk write")
        raise OSError("generated partial write")
    monkeypatch.setattr(roots, "_copy", fail_copy)
    with pytest.raises(ManagerError, match="guarded rollback"):
        roots.apply(library, roots.review(library, identifier), accept_root_writes=True)
    assert (game / roots.LOCK).exists()
    plan = roots.review_restore(library, identifier)
    assert len(plan.temporaries) == 1
    monkeypatch.setattr(roots, "_copy", real_copy)
    if stale:
        (game / plan.temporaries[0][0]).write_bytes(b"another change after review")
        before = files(game)
        with pytest.raises(ManagerError, match="changed after review"):
            roots.restore(library, plan, accept_root_writes=True)
        assert files(game) == before
    else:
        roots.restore(library, plan, accept_root_writes=True)
        assert files(game) == original


def test_one_imported_modlist_two_root_setups_keep_payloads_and_other_target_separate(tmp_path, monkeypatch):
    from creation_lib.mod_manager.mo2 import inspect_instance
    from .fixtures import make_mo2
    library, _, game, identifier = prepared(tmp_path, monkeypatch)
    source = make_mo2(tmp_path)
    library.import_mo2(inspect_instance(source))
    profile = next(item.id for item in library.profiles() if library.rows(item.id))
    common = library.rows(profile)
    source_before = files(source)
    second = tmp_path / "second installation"
    (second / "Data").mkdir(parents=True)
    image(second / "Fallout4.exe", version=(1, 10, 984, 0))
    second_id = launch_targets.apply(library, launch_targets.review(library, "New generation", second))
    cache = tmp_path / "second cache"
    cache.mkdir()
    f4se_setup.prepare(library, second_id, archive=package(cache, (1, 10, 984, 0)))
    first = roots.apply(library, roots.review(library, identifier), accept_root_writes=True)
    other = roots.apply(library, roots.review(library, second_id), accept_root_writes=True)
    other_before = files(second)
    for target in (first, other):
        view = launch_targets.TargetLibrary(library, target.id)
        assert [row for row in view.profile_snapshot(profile)[1] if not row.id.startswith("f4se-target-")] == common
        f4se_setup.validate_ready(target, owner_root=view.root)
    roots.restore(library, roots.review_restore(library, identifier), accept_root_writes=True)
    assert files(second) == other_before and files(source) == source_before and library.rows(profile) == common
    assert not (game / "f4se_loader.exe").exists()
