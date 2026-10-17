"""Full migrations operate only on generated MO2 instances and owned fixtures."""
import json
import os
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2_migration import discover
from creation_lib.mod_manager.package import Package
from .fixtures import make_mo2
from .test_controller import finish
from .test_mo2 import junction


def full_instance(directory):
    directory.mkdir(parents=True, exist_ok=True)
    root = make_mo2(directory)
    base = directory / "redirected base"
    base.mkdir()
    (root / "mods").rename(base / "mods")
    (root / "profiles").rename(base / "profiles")
    (root / "ModOrganizer.ini").write_text(f"[General]\ngameName=Fallout4\nselected_profile=Default\n[Settings]\nbase_directory={base.as_posix()}\n")
    orphan = base / "mods" / "Unassigned example"
    orphan.mkdir()
    (orphan / "orphan.txt").write_bytes(b"independent orphan data")
    (orphan / "meta.ini").write_text("[General]\nversion=7\nmodid=42\nfileid=99\nnotes=orphan notes\ntags=fixture\n")
    target = directory / "ordinary linked target"
    (target / "empty/nested").mkdir(parents=True)
    (target / "fomod").mkdir()
    (target / "fomod/script.cs").write_bytes(b"inert source script, never executed")
    (target / "hidden.txt.mohidden").write_bytes(b"hidden fixture")
    (target / "linked.txt").write_bytes(b"shared linked content")
    alias = base / "mods" / "Linked fixture"
    junction(alias, target)
    for profile in (base / "profiles").iterdir():
        file = profile / "modlist.txt"
        file.write_text("+Linked fixture\n" + file.read_text())
    (base / "overwrite").mkdir()
    (base / "overwrite/generated.txt").write_bytes(b"highest priority generated output")
    (base / "downloads").mkdir()
    (base / "downloads/archive.7z").write_bytes(b"not a decoder fixture; inventory only")
    (base / "downloads/archive.7z.meta").write_text("[General]\nmodID=42\nfileID=99\nversion=7\naccess_token=never retain\nurl=https://invalid/?token=secret\n")
    return root, base, alias, target


@pytest.fixture
def full(tmp_path, monkeypatch):
    root, base, alias, target = full_instance(tmp_path)
    monkeypatch.setattr("app.paths.get_db_dir", lambda: tmp_path / "app data")
    manager = ManagerController()
    yield manager, root, base, target
    manager.cleanup()
    alias.rmdir() if os.name == "nt" else alias.unlink()


def snapshot(preview):
    paths = {Path(preview.root) / "ModOrganizer.ini"}
    paths.update(Path(profile.source) / name for profile in preview.profiles for name in profile.files if profile.source)
    for mod in preview.inventory:
        if mod.source:
            package = Package(mod.source, inspect_installer=False)
            try:
                paths.update(Path(mod.source) / name for name in package.files.values())
            finally:
                package.close()
    return {str(path): path.read_bytes() for path in paths}


def test_one_action_imports_all_redirected_profiles_as_unchanged_original_references(full):
    manager, root, base, target = full
    preview, auto, issues = discover(root, manager.resolve_destination())
    before = snapshot(preview)
    assert len(auto) == 1 and not issues
    manager.migrate_mo2(root)
    finish(manager)
    result = manager.instance_migration
    assert result.state == "complete" and result.original_profiles == 2 and len(manager.profiles) == 3
    assert len(result.referenced) == 5 and result.downloads == 1
    assert next(profile.name for profile in manager.profiles if profile.id == manager.profile_id) == "Default"
    shared = {}
    for profile in manager.profiles:
        rows = manager.library.rows(profile.id)
        for row in rows:
            if row.kind != "separator":
                assert row.kind == "external" and not Path(row.source).is_relative_to(manager.library.root)
                shared.setdefault(row.id, set()).add(row.source)
        if profile.name == "Unassigned mods (MO2)":
            assert len(rows) == 1 and not rows[0].enabled
            assert (rows[0].version, rows[0].nexus_file_id, rows[0].tags, rows[0].notes) == ("7", 99, "fixture", "orphan notes")
        else:
            assert all(row.source != str(base / "overwrite") for row in rows)
            original = next(p for p in preview.profiles if p.id == profile.id)
            assert [(r.id, r.enabled, r.group, r.priority) for r in rows] == [(m.id, m.enabled, m.group, i) for i, m in enumerate(original.mods)]
    assert all(len(paths) == 1 for paths in shared.values())
    assert not tuple((manager.library.root / "mods").iterdir()) and not manager.library.origins()
    linked = next(row for row in manager.rows if row.name == "Linked fixture")
    assert linked.source == str(target) and (target / "empty/nested").is_dir()
    inventory = manager.instance_configuration["download_inventory"]
    assert inventory[0]["metadata"]["fileid"] == "99" and "token" not in json.dumps(inventory)
    assert snapshot(preview) == before
    manager.customize(linked.id, notes="local change", group="Local")
    finish(manager)
    original_id = manager.library.library_id
    manager.migrate_mo2(root)
    finish(manager)
    assert manager.instance_migration.new_profiles == 0 and manager.library.library_id == original_id
    assert len(manager.instance_migration.referenced) == 5 and not tuple((manager.library.root / "mods").iterdir())
    assert (base / "overwrite/generated.txt").read_bytes() == b"highest priority generated output"
    assert manager.rows_by_id[linked.id].notes == "local change" and snapshot(preview) == before


def test_primary_import_never_enumerates_hashes_or_copies_mod_payloads(full, monkeypatch):
    manager, root, _, _ = full
    def forbidden(*args, **kwargs):
        raise AssertionError("Primary import must not inspect or copy payloads")
    monkeypatch.setattr(Package, "__init__", forbidden)
    monkeypatch.setattr("creation_lib.mod_manager.migration.SourceMigration.copy", forbidden)
    monkeypatch.setattr(Library, "review_migration", forbidden)
    manager.migrate_mo2(root)
    finish(manager)
    assert len(manager.instance_migration.referenced) == 5


def test_payload_capacity_is_irrelevant_to_metadata_import(full, monkeypatch):
    import shutil
    manager, root, _, _ = full
    monkeypatch.setattr(shutil, "disk_usage", lambda path: SimpleNamespace(free=1))
    manager.migrate_mo2(root)
    finish(manager)
    assert manager.instance_migration.state == "complete"


def test_repeat_import_adds_new_references_without_losing_local_organization(full):
    manager, root, base, _ = full
    manager.migrate_mo2(root)
    finish(manager)
    base_row = next(row for row in manager.rows if row.name == "Base")
    manager.customize(base_row.id, enabled=False, group="Local", notes="local notes")
    finish(manager)
    new = base / "mods/Added later"
    new.mkdir()
    (new / "new.txt").write_bytes(b"new source content")
    modlist = base / "profiles/Default/modlist.txt"
    modlist.write_text("+Added later\n" + modlist.read_text())
    manager.migrate_mo2(root)
    finish(manager)
    assert manager.instance_migration.new_profiles == 0 and len(manager.instance_migration.referenced) == 6
    assert manager.rows_by_id[base_row.id].notes == "local notes" and not manager.rows_by_id[base_row.id].enabled
    assert manager.rows[-1].name == "Added later"
    assert next(row.source for row in manager.rows if row.name == "Added later") == str(new)


def test_skipped_profile_is_partial(full):
    manager, root, base, _ = full
    (base / "profiles/Incomplete profile").mkdir()
    manager.migrate_mo2(root)
    finish(manager)
    assert manager.instance_migration.state == "partial"
    assert any("Incomplete profile" in message for message in manager.instance_migration.issues)


def test_missing_original_source_is_reported_without_replacement(full):
    manager, root, base, _ = full
    manager.migrate_mo2(root)
    finish(manager)
    (base / "mods/Base").rename(base / "Moved Base")
    manager.migrate_mo2(root)
    finish(manager)
    assert manager.instance_migration.state == "partial" and "Base" in manager.instance_migration.unavailable
    assert not (base / "mods/Base").exists() and not tuple((manager.library.root / "mods").iterdir())


def test_second_instance_same_profile_names_selects_new_instance(full, tmp_path):
    manager, root, _, _ = full
    manager.migrate_mo2(root)
    finish(manager)
    parent = tmp_path / "second instance"
    parent.mkdir()
    second = make_mo2(parent)
    ini = second / "ModOrganizer.ini"
    ini.write_text(ini.read_text().replace("[General]", "[General]\nselected_profile=Default"))
    manager.migrate_mo2(second)
    finish(manager)
    assert len(manager.profiles) == 5 and manager.instance_configuration["paths"]["instance"] == str(second)
    assert next(profile.name for profile in manager.profiles if profile.id == manager.profile_id) == "Default"


@pytest.mark.parametrize("outcome", ["cancel", "failure"])
def test_cancelled_failed_metadata_transaction_rolls_back_and_retries(full, monkeypatch, outcome):
    manager, root, _, _ = full
    original = Library.import_mo2
    def interrupt(library, *args, **kwargs):
        def checkpoint(name):
            if outcome == "cancel":
                manager.cancel()
                from creation_lib.mod_manager.paths import check_cancel
                check_cancel(manager._cancel)
            else:
                raise OSError("fixture metadata write failure")
        kwargs["checkpoint"] = checkpoint
        return original(library, *args, **kwargs)
    with monkeypatch.context() as patch:
        patch.setattr(Library, "import_mo2", interrupt)
        manager.migrate_mo2(root)
        with pytest.raises(Exception):
            manager._future.result(timeout=10)
        manager.poll()
    assert manager.job_state == ("cancelled" if outcome == "cancel" else "failed")
    assert not manager.profiles and not tuple((manager.library.root / "mods").iterdir())
    library_id = manager.library.library_id
    manager.migrate_mo2(root)
    finish(manager)
    assert manager.library.library_id == library_id and len(manager.profiles) == 3


def test_unmanaged_game_content_excluded_and_nested_payloads_not_probed(full):
    manager, root, base, target = full
    nested = base / "mods/Unassigned example/nested"
    junction(nested, target)
    try:
        modlist = base / "profiles/Default/modlist.txt"
        modlist.write_text("+Missing fixture\n*Unmanaged: fixture\n" + modlist.read_text())
        manager.migrate_mo2(root)
        finish(manager)
        result = manager.instance_migration
        assert result.state == "partial" and result.unavailable == ["Missing fixture"]
        assert result.game_content == ["Unmanaged: fixture"]
        assert all(row.kind != "foreign" for profile in manager.profiles for row in manager.library.rows(profile.id))
        profile = next(profile for profile in manager.profiles if profile.id == manager.profile_id)
        assert json.loads(profile.files["_modbox_game_content"]) == ["Unmanaged: fixture"]
        assert "*Unmanaged: fixture" in profile.files["modlist.txt"] and "plugins.txt" in profile.files
        orphan = next(row for p in manager.profiles for row in manager.library.rows(p.id) if row.name == "Unassigned example")
        assert orphan.source == str(base / "mods/Unassigned example")
    finally:
        nested.rmdir() if os.name == "nt" else nested.unlink()


@pytest.mark.parametrize("checkpoint", ["before-profile-commit", "profiles-imported"])
def test_metadata_import_process_exit_recovers_and_retries_idempotently(full, checkpoint):
    manager, root, _, _ = full
    library = Library(manager.resolve_destination(), create=True)
    args = {"library": str(library.root), "instance": str(root), "operation": "instance_migrate", "checkpoint": checkpoint}
    process = subprocess.run([sys.executable, "-m", "ui.mod_manager.tests.crash_worker", json.dumps(args)], cwd=Path(__file__).parents[3], capture_output=True, timeout=30)
    assert process.returncode == 91, process.stderr.decode(errors="replace")
    reopened = Library(library.root)
    assert reopened.migration_runs()[0]["state"] == "importing"
    assert len(reopened.profiles()) == (0 if checkpoint == "before-profile-commit" else 3)
    manager.migrate_mo2(root)
    finish(manager)
    assert manager.instance_migration.state == "complete" and len(manager.profiles) == 3
    assert not tuple((library.root / "mods").iterdir())
    manager.migrate_mo2(root)
    finish(manager)
    assert manager.instance_migration.new_profiles == 0


def test_locally_deleted_imported_profile_stays_deleted_on_repeat(full):
    manager, root, _, _ = full
    manager.migrate_mo2(root)
    finish(manager)
    deleted = manager.profile_id
    manager.delete_profile(deleted)
    finish(manager)
    manager.migrate_mo2(root)
    finish(manager)
    assert all(profile.id != deleted for profile in manager.profiles)
    assert manager.instance_migration.retired_profiles == ["Default"]
    manager.restore_profile(deleted)
    finish(manager)
    assert manager.profile_id == deleted and len(manager.profiles) == 3


def test_excluded_overwrite_is_never_scanned_and_repeat_preserves_historical_local_state(full, monkeypatch):
    from creation_lib.mod_manager.mo2 import identity
    from creation_lib.mod_manager.overwrite import root as output_root
    manager, instance, base, _ = full
    manager.migrate_mo2(instance)
    finish(manager)
    original = (base / "overwrite/generated.txt").read_bytes()
    historical = identity(str(instance) + "/migration/overwrite")
    with manager.library._db() as db:
        db.execute("INSERT INTO mods(id,name,source,kind) VALUES (?,?,?,'external')", (historical, "Historical MO2 output", str(base / "overwrite")))
        priority = db.execute("SELECT MAX(priority)+1 FROM entries WHERE profile_id=?", (manager.profile_id,)).fetchone()[0]
        db.execute("INSERT INTO entries VALUES (?,?,?,1,'')", (manager.profile_id, historical, priority))
    owned = output_root(manager.library, manager.profile_id)
    owned.mkdir(parents=True, exist_ok=True)
    (owned / "local-output.txt").write_bytes(b"manager output")
    iterator = Path.iterdir
    def guarded(path):
        if path == base / "overwrite":
            raise AssertionError("Excluded MO2 overwrite must not be enumerated")
        return iterator(path)
    monkeypatch.setattr(Path, "iterdir", guarded)
    before = manager.library.rows(manager.profile_id)
    manager.migrate_mo2(instance)
    finish(manager)
    assert len(manager.instance_migration.referenced) == 5
    assert manager.library.rows(manager.profile_id) == before
    assert (base / "overwrite/generated.txt").read_bytes() == original
    assert (owned / "local-output.txt").read_bytes() == b"manager output"
