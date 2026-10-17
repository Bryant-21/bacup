from dataclasses import replace
from contextlib import closing
import json
from pathlib import Path
import struct
import subprocess
import sys
import threading

import pytest

from creation_lib.mod_manager import plugin_file_actions as actions
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.profile_backup import digest
from creation_lib.mod_manager.plugin_state import inspect, inventory, review as activation_review, snapshot


def plugin_bytes():
    hedr = struct.pack("<4sHfII", b"HEDR", 12, 1.0, 0, 0x800)
    return struct.pack("<4sIIIIHH", b"TES4", len(hedr), 0, 0, 0, 131, 0) + hedr


@pytest.fixture
def owned(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile = library.create_profile("Local")
    with library._db() as db:
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps({"_modbox_game": "Fallout4", "plugins.txt": "A.esp\n", "loadorder.txt": "A.esp\n", "lockedorder.txt": "A.esp|0\n"}), profile))
    source = tmp_path / "package"
    source.mkdir()
    (source / "A.esp").write_bytes(plugin_bytes())
    (source / "unrelated.txt").write_bytes(b"unchanged unrelated asset")
    with closing(Package(source)) as package:
        mod = library.install(package, package.plain_plan(), "Owned fixture", profile)
    def catalog():
        return inspect(inventory(library, profile), reader=lambda *args: ({"flags": {}}, ()))
    current = catalog()
    return library, profile, mod, current, current.plugins[0], source


@pytest.mark.parametrize("action,new_name", [("rename", "Renamed.esp"), ("ghost", ""), ("esm", "")])
def test_single_file_apply_reopen_guarded_undo_preserves_assets(owned, action, new_name):
    library, profile, mod, catalog, plugin, source = owned
    root = library.root / "mods" / mod
    original, asset = (root / "A.esp").read_bytes(), (root / "unrelated.txt").stat().st_mtime_ns
    def writer(source, stage, flag, cancel):
        data = bytearray(source.read_bytes())
        struct.pack_into("<I", data, 8, 1)
        stage.write_bytes(data)
    identifier = actions.apply(library, actions.review(library, catalog, plugin, action, new_name), writer=writer)
    assert (source / "A.esp").read_bytes() == original
    assert (root / "unrelated.txt").stat().st_mtime_ns == asset
    assert len(list((library.root / ".plugin-actions" / identifier).iterdir())) == 1  # Just the original plugin; no whole-mod copy.
    if action == "rename":
        assert snapshot(library.profiles()[0].files, "loadorder.txt") == "Renamed.esp\n"
    if action == "ghost":
        refreshed = inventory(library, profile)
        assert refreshed.plugins[0].ghosted
        known = inspect(refreshed, reader=lambda *args: ({"flags": {}}, ()))
        with pytest.raises(ManagerError, match="ghosted"):
            activation_review(known, enabled={"A.esp"})
    reopened = Library(library.root)
    actions.undo(reopened, identifier)
    assert (root / "A.esp").read_bytes() == original
    assert snapshot(reopened.profiles()[0].files, "loadorder.txt") == "A.esp\n"
    with pytest.raises(ManagerError, match="changed"):
        actions.undo(reopened, identifier)


@pytest.mark.parametrize("phase", ["journal", "prepared", "old-moved", "placed", "before-commit"])
@pytest.mark.parametrize("outcome", ["failed", "cancelled"])
def test_fail_cancel_journal_restores_original_and_metadata(owned, monkeypatch, phase, outcome):
    library, _, mod, catalog, plugin, _ = owned
    plan = actions.review(library, catalog, plugin, "ghost")
    before = digest(plan.state)
    cancel = threading.Event()
    def fail(at):
        if at == phase:
            if outcome == "cancelled":
                cancel.set()
                raise Cancelled("generated cancellation")
            raise OSError("generated file failure")
    monkeypatch.setattr(actions, "_checkpoint", fail)
    with pytest.raises(Cancelled if outcome == "cancelled" else OSError):
        actions.apply(library, plan, cancel)
    assert actions._file(library.root / "mods" / mod / "A.esp") == plan.original
    with library._db() as db:
        assert digest(actions._state(db)) == before
    assert not actions.recover(library)


@pytest.mark.parametrize("change", ["metadata", "payload", "new-sidecar", "sidecar", "dependency", "active", "reference", "light", "other-profile"])
def test_stale_or_unsupported_actions_preserve_files(owned, change):
    library, profile, mod, catalog, plugin, source = owned
    if change in {"metadata", "payload", "new-sidecar"}:
        plan = actions.review(library, catalog, plugin, "rename", "B.esp")
        if change == "metadata":
            library.customize(profile, mod, notes="later")
        elif change == "payload":
            Path(plugin.path).write_bytes(b"changed after review")
        else:
            (Path(plugin.path).parent / "A - Main.ba2").write_bytes(b"generated later sidecar")
        with pytest.raises(ManagerError, match="sidecars" if change == "new-sidecar" else "changed"):
            actions.apply(library, plan)
    else:
        action = "rename"
        if change == "sidecar":
            (Path(plugin.path).parent / "A - Main.ba2").write_bytes(b"generated archive placeholder")
        elif change == "dependency":
            other = replace(plugin, name="Dependent.esp", masters=("A.esp",))
            catalog = replace(catalog, plugins=(plugin, other))
        elif change == "active":
            with library._db() as db:
                files = library.profiles()[0].files
                files["plugins.txt"] = "*A.esp\n"
                db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile))
        elif change == "reference":
            with library._db() as db:
                db.execute("UPDATE mods SET kind='external',source=? WHERE id=?", (str(source), mod))
        elif change == "light":
            action = "esm"
            plugin = replace(plugin, header={"flags": {"esl": True}})
        elif change == "other-profile":
            library.create_profile("Other", profile)
        with pytest.raises(ManagerError):
            actions.review(library, catalog, plugin, action, "B.esp")
    assert not (Path(plugin.path).parent / "B.esp").exists()


def test_real_cli_master_flag_uses_generated_plugin_and_staged_output(owned):
    import shutil
    if not shutil.which("modkit.exe"):
        pytest.skip("Supported modkit CLI unavailable")
    library, profile, _, catalog, plugin, source = owned
    original = (source / "A.esp").read_bytes()
    identifier = actions.apply(library, actions.review(library, catalog, plugin, "esm"))
    from creation_lib.mod_manager.cli_reader import run_json
    header = run_json(("esp", "header", plugin.path), game="fo4")
    assert header["flags"]["esm"]
    assert (source / "A.esp").read_bytes() == original
    current = inspect(inventory(library, profile), reader=lambda *args: (header, ()))
    clear = actions.apply(library, actions.review(library, current, current.plugins[0], "no-esm"))
    assert not run_json(("esp", "header", plugin.path), game="fo4")["flags"]["esm"]
    actions.undo(library, clear)
    assert run_json(("esp", "header", plugin.path), game="fo4")["flags"]["esm"]
    actions.undo(library, identifier)
    assert Path(plugin.path).read_bytes() == original


def test_unghost_review_apply_and_undo_restores_ghosted_inventory(owned):
    library, profile, _, catalog, plugin, source = owned
    original = Path(plugin.path).read_bytes()
    actions.apply(library, actions.review(library, catalog, plugin, "ghost"))
    ghosted = inspect(inventory(library, profile), reader=lambda *args: ({"flags": {}}, ()))
    operation = actions.apply(library, actions.review(library, ghosted, ghosted.plugins[0], "unghost"))
    assert Path(plugin.path).read_bytes() == original
    assert not inventory(library, profile).plugins[0].ghosted
    actions.undo(library, operation)
    assert inventory(library, profile).plugins[0].ghosted
    assert (source / "A.esp").read_bytes() == original


@pytest.mark.parametrize("outcome", ["failed", "cancelled"])
def test_late_fault_returns_durable_committed_action_and_undo(owned, monkeypatch, outcome):
    library, _, _, catalog, plugin, _ = owned
    def late(at):
        if at == "committed":
            if outcome == "cancelled":
                raise Cancelled("generated late cancellation")
            raise OSError("generated late finalization failure")
    monkeypatch.setattr(actions, "_checkpoint", late)
    operation = actions.apply(library, actions.review(library, catalog, plugin, "ghost"))
    assert Path(plugin.path + ".ghost").exists() and not actions.recover(library)
    monkeypatch.setattr(actions, "_checkpoint", lambda phase: None)
    actions.undo(library, operation)
    assert Path(plugin.path).read_bytes() == plugin_bytes()


def test_committed_journal_cleanup_error_identifies_commit_and_reopens_safely(owned, monkeypatch):
    library, _, _, catalog, plugin, _ = owned
    original_unlink = Path.unlink
    def fail_journal(path, *args, **kwargs):
        if path.name == "journal.json" and path.parent.parent.name == ".plugin-actions":
            raise PermissionError("generated journal cleanup failure")
        return original_unlink(path, *args, **kwargs)
    monkeypatch.setattr(Path, "unlink", fail_journal)
    with pytest.raises(ManagerError, match="committed, but journal finalization failed"):
        actions.apply(library, actions.review(library, catalog, plugin, "ghost"))
    assert Path(plugin.path + ".ghost").exists()
    monkeypatch.setattr(Path, "unlink", original_unlink)
    reopened = Library(library.root)
    record = next(item for item in reopened.recovery_report if item["kind"] == "plugin-file")
    assert record["committed"]
    actions.undo(reopened, record["id"])
    assert Path(plugin.path).read_bytes() == plugin_bytes()


@pytest.mark.parametrize("phase", ["prepared", "old-moved", "placed", "before-commit", "committed"])
def test_process_crash_reopens_with_durable_commit_or_complete_rollback(owned, phase):
    library, _, mod, _, _, _ = owned
    script = """
import os, sys
from pathlib import Path
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager import plugin_file_actions as actions
from creation_lib.mod_manager.plugin_state import inventory, inspect
library = Library(Path(sys.argv[1]))
catalog = inspect(inventory(library, library.profiles()[0].id), reader=lambda *args: ({'flags': {}}, ()))
plan = actions.review(library, catalog, catalog.plugins[0], 'ghost')
def crash(at):
    if at == sys.argv[2]:
        os._exit(37)
actions._checkpoint = crash
actions.apply(library, plan)
"""
    result = subprocess.run([sys.executable, "-c", script, str(library.root), phase], capture_output=True, timeout=20)
    assert result.returncode == 37, result.stderr.decode(errors="replace")
    reopened = Library(library.root)
    root = reopened.root / "mods" / mod
    assert reopened.recovery_report[0]["committed"] == (phase == "committed")
    assert (root / "A.esp.ghost").exists() == (phase == "committed")
    assert (root / "A.esp").exists() == (phase != "committed")
    if phase == "committed":
        actions.undo(reopened, reopened.recovery_report[0]["id"])
        assert (root / "A.esp").read_bytes() == plugin_bytes()


def test_external_destination_change_blocks_rollback_without_overwriting_it(owned, monkeypatch):
    library, _, _, catalog, plugin, _ = owned
    plan = actions.review(library, catalog, plugin, "ghost")
    target = Path(plugin.path + ".ghost")
    def changed(at):
        if at == "placed":
            target.write_bytes(b"generated later external edit")
            raise OSError("generated publish failure")
    monkeypatch.setattr(actions, "_checkpoint", changed)
    with pytest.raises(ManagerError, match="destination changed"):
        actions.apply(library, plan)
    assert target.read_bytes() == b"generated later external edit"
    backups = list((library.root / ".plugin-actions").glob("*/original"))
    assert len(backups) == 1 and backups[0].read_bytes() == plugin_bytes()
    with pytest.raises(ManagerError, match="destination changed"):
        Library(library.root)
