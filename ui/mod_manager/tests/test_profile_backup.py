import json
import threading

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.profile_backup import apply, capture, history, remove, review
from creation_lib.mod_manager.profile_state import rename, restore_ini, save_ini
from .fixtures import make_mo2
from .test_standalone import game_fixture
from creation_lib.mod_manager.instances import create


def test_effective_ini_requires_the_current_normalized_launch_baseline(tmp_path):
    import hashlib
    from creation_lib.mod_manager.profile_state import effective_ini, fo4_runtime_ini
    library = Library(tmp_path / "library", create=True)
    profile_id = create(library, "fo4", game_fixture(tmp_path))
    profile = library.profiles()[0]
    name, initial = "Fallout4Custom.ini", profile.files["Fallout4Custom.ini"]
    root = library.root / "runtime" / "profiles" / profile_id
    (root / "ini").mkdir(parents=True)
    runtime = root / "ini" / name
    runtime.write_text("[General]\nGameEdited=1\n", encoding="utf-8")
    # An unrecorded/stale runtime file never supersedes the local snapshot.
    assert effective_ini(library, profile, name) == initial
    digest = hashlib.sha256(fo4_runtime_ini(name, initial, profile_id).encode()).hexdigest()
    (root / "ini_baseline.json").write_text(json.dumps({name.casefold(): digest}), encoding="utf-8")
    assert "GameEdited=1" in effective_ini(library, profile, name)
    edited = "[General]\nLocalEdited=2\n"
    save_ini(library, profile_id, name, edited, initial)
    assert effective_ini(library, library.profiles()[0], name) == edited
    restore_ini(library, profile_id, name, edited)
    assert "GameEdited=1" in effective_ini(library, library.profiles()[0], name)
    assert "GameEdited=1" in runtime.read_text()


@pytest.mark.parametrize("name", ["../Fallout4.ini", "nested/Fallout4.ini"])
def test_effective_ini_validates_path_before_snapshot_lookup(tmp_path, name):
    from types import SimpleNamespace
    from creation_lib.mod_manager.profile_state import effective_ini
    library = SimpleNamespace(root=tmp_path)
    profile = SimpleNamespace(id="fixture", files={})
    with pytest.raises(ManagerError):
        effective_ini(library, profile, name)


def test_profile_backup_review_restore_reopen_and_source_immutability(tmp_path):
    source = make_mo2(tmp_path)
    original = {str(path): path.read_bytes() for path in source.rglob("*") if path.is_file()}
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(source))
    profile = library.profiles()[0]
    rows = library.rows(profile.id)
    identifier = capture(library, profile.id)
    rename(library, profile.id, "Changed local name")
    library.customize(profile.id, rows[0].id, group="Local/Changed", enabled=False)
    plan = review(library, profile.id, identifier)
    apply(library, plan)
    reopened = Library(library.root)
    assert next(item for item in reopened.profiles() if item.id == profile.id) == profile
    assert reopened.rows(profile.id) == rows and len(history(reopened, profile.id)) == 2
    # Restoring is itself recoverable: the newer backup holds replaced local state.
    replaced = next(item for item in history(reopened, profile.id) if item["id"] != identifier)
    apply(reopened, review(reopened, profile.id, replaced["id"]))
    assert next(item for item in reopened.profiles() if item.id == profile.id).name == "Changed local name"
    assert {str(path): path.read_bytes() for path in source.rglob("*") if path.is_file()} == original


def test_backup_cancel_stale_review_tampering_and_missing_mod_fail_closed(tmp_path):
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(make_mo2(tmp_path)))
    profile = library.profiles()[0]
    identifier = capture(library, profile.id)
    plan = review(library, profile.id, identifier)
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        apply(library, plan, cancel)
    with pytest.raises(Cancelled):
        capture(library, profile.id, cancel)
    assert len(history(library, profile.id)) == 1
    rename(library, profile.id, "Later name")
    with pytest.raises(ManagerError, match="changed after"):
        apply(library, plan)
    plan = review(library, profile.id, identifier)
    with library._db() as db:
        data = json.loads(db.execute("SELECT snapshot FROM profile_backups WHERE id=?", (identifier,)).fetchone()[0])
        data["name"] = "Changed backup"
        db.execute("UPDATE profile_backups SET snapshot=? WHERE id=?", (json.dumps(data), identifier))
    with pytest.raises(ManagerError, match="backup changed"):
        apply(library, plan)
    missing_id = data["entries"][0]["mod_id"]
    with library._db() as db:
        db.execute("DELETE FROM entries WHERE mod_id=?", (missing_id,))
        db.execute("DELETE FROM mods WHERE id=?", (missing_id,))
    with pytest.raises(ManagerError, match="missing"):
        review(library, profile.id, identifier)


def test_ini_restore_swaps_previous_snapshot_and_minimal_acknowledgement(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile_id = create(library, "fo4", game_fixture(tmp_path))
    name, initial = "Fallout4Prefs.ini", library.profiles()[0].files["Fallout4Prefs.ini"]
    edited = "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n"
    save_ini(library, profile_id, name, edited, initial)
    assert name not in json.loads(library.profiles()[0].files["_modbox_minimal_inis"])
    restore_ini(library, profile_id, name, edited)
    assert library.profiles()[0].files[name] == initial
    assert name in json.loads(library.profiles()[0].files["_modbox_minimal_inis"])
    restore_ini(library, profile_id, name, initial)
    assert library.profiles()[0].files[name] == edited
    with pytest.raises(ManagerError, match="changed"):
        restore_ini(library, profile_id, name, initial)


def test_backup_capacity_and_explicit_remove_are_bounded_and_cancellable(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile_id = library.create_profile("Empty")
    identifier = capture(library, profile_id)
    with library._db() as db:
        snapshot = db.execute("SELECT snapshot FROM profile_backups WHERE id=?", (identifier,)).fetchone()[0]
        db.executemany("INSERT INTO profile_backups(id,profile_id,snapshot) VALUES (?,?,?)", [(str(index), profile_id, snapshot) for index in range(99)])
    with pytest.raises(ManagerError, match="100"):
        capture(library, profile_id)
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        remove(library, profile_id, identifier, cancel)
    assert len(history(library, profile_id)) == 100
    remove(library, profile_id, identifier)
    assert len(history(library, profile_id)) == 99
