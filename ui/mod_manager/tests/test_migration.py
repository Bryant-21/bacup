import json
from dataclasses import replace
import os
import shutil
from pathlib import Path
import subprocess
import sys
import threading
from types import SimpleNamespace

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.migration import MigrationCancelled, MigrationInterrupted
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import ManagerError
from creation_lib.mod_manager.transactions import CommittedTransaction
from .fixtures import make_mo2
from .test_mo2 import junction


def setup(tmp_path):
    instance = make_mo2(tmp_path)
    modlist = instance / "profiles" / "Default" / "modlist.txt"
    modlist.write_text(modlist.read_text() + "*Unmanaged: Fixture\n")
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(instance))
    profile = next(item.id for item in library.profiles() if item.name == "Default")
    row = next(item for item in library.rows(profile) if item.name == "Base")
    return library, profile, row, instance


def contents(path):
    package = Package(path, inspect_installer=False)
    try:
        return {name: (Path(path) / name).read_bytes() for name in package.files.values()}
    finally:
        package.close()


def test_reviewed_copy_preserves_identity_all_profiles_complete_folder_and_origin(tmp_path):
    library, profile, row, instance = setup(tmp_path)
    source = Path(row.source)
    (source / "empty/nested").mkdir(parents=True)
    (source / "fomod").mkdir()
    (source / "fomod/script.cs").write_text("inert fixture")
    (source / "disabled.txt.mohidden").write_text("preserve hidden")
    library.customize(profile, row.id, group="Custom/Files", tags="favorite", notes="preserve")
    before = {item.id: next(mod for mod in library.rows(item.id) if mod.id == row.id) for item in library.profiles()}
    original = contents(source)
    review = library.review_migration([row.id, row.id])
    assert len(review.sources) == 1 and len(review.sources[0].profiles) == 2
    assert review.required > review.bytes and review.free > review.required
    progress = []
    assert library.migrate(review, progress=lambda fraction, name: progress.append(fraction)) == (row.id,)
    assert progress and max(progress) == 1
    assert progress == sorted(progress)
    destination = library.root / "mods" / row.id
    assert contents(destination) == original and contents(source) == original
    assert (destination / "empty/nested").is_dir()
    for identifier, old in before.items():
        current = next(mod for mod in library.rows(identifier) if mod.id == row.id)
        assert current.kind == "owned" and current.source == str(destination)
        assert (current.priority, current.group, current.enabled, current.tags, current.notes) == (old.priority, old.group, old.enabled, old.tags, old.notes)
    assert library.origins()[row.id]["source"] == str(source)
    assert library.origins()[row.id]["identity"]
    assert library.import_mo2(inspect_instance(instance)) == 0
    assert next(mod for mod in library.rows(profile) if mod.id == row.id).kind == "owned"
    # The new owned lifecycle never uses the retained original as a target.
    operation = library.uninstall(library.review_owned(row.id))
    library.restore(operation)
    assert contents(source) == original and library.origins()[row.id]["source"] == str(source)
    assert contents(destination) == original


@pytest.mark.parametrize("change", ["bytes", "directory", "metadata", "profile"])
def test_stale_review_is_refused_before_copy(tmp_path, change):
    library, profile, row, _ = setup(tmp_path)
    review = library.review_migration([row.id])
    if change == "bytes":
        path = Path(row.source) / "textures/shared.txt"
        info = path.stat()
        path.write_bytes(path.read_bytes().swapcase())
        os.utime(path, ns=(info.st_atime_ns, info.st_mtime_ns))
    elif change == "directory":
        (Path(row.source) / "new-empty").mkdir()
    elif change == "metadata":
        library.customize(profile, row.id, notes="changed")
    else:
        library.create_profile("New reference", profile)
    with pytest.raises(MigrationInterrupted, match="changed after review"):
        library.migrate(review)
    assert not (library.root / "mods" / row.id).exists()
    assert next(item for item in library.rows(profile) if item.id == row.id).kind == "external"


def test_low_space_and_occupied_destination_do_not_touch_sources_or_destination(tmp_path, monkeypatch):
    library, _, row, _ = setup(tmp_path)
    review = library.review_migration([row.id])
    original = contents(row.source)
    usage = shutil.disk_usage
    monkeypatch.setattr("creation_lib.mod_manager.migration.shutil.disk_usage", lambda root: SimpleNamespace(free=0))
    with pytest.raises(ManagerError, match="Insufficient free space"):
        library.migrate(review)
    assert list((library.root / ".transactions").iterdir()) == []
    monkeypatch.setattr("creation_lib.mod_manager.migration.shutil.disk_usage", usage)
    destination = library.root / "mods" / row.id
    destination.mkdir()
    (destination / "unrelated.txt").write_text("retain")
    with pytest.raises(MigrationInterrupted, match="occupied"):
        library.migrate(review)
    assert (destination / "unrelated.txt").read_text() == "retain"
    assert contents(row.source) == original


def test_oversized_review_batch_requests_smaller_selection_without_copying(tmp_path, monkeypatch):
    library, profile, _, _ = setup(tmp_path)
    monkeypatch.setattr("creation_lib.mod_manager.migration.MAX_BATCH_ENTRIES", 6)
    selected = [item.id for item in library.rows(profile) if item.kind == "external"]
    with pytest.raises(ManagerError, match="Select a smaller batch"):
        library.review_migration(selected)
    assert not library.origins() and not any((library.root / "mods").iterdir())


@pytest.mark.parametrize("field", ["directories", "source"])
def test_forged_review_fields_cannot_redirect_copy_or_create_escaping_directories(tmp_path, field):
    library, _, row, _ = setup(tmp_path)
    review = library.review_migration([row.id])
    unrelated = tmp_path / "unrelated"
    unrelated.mkdir()
    changes = {"directories": ("../escape",)} if field == "directories" else {"source": str(unrelated)}
    forged = replace(review, sources=(replace(review.sources[0], **changes),))
    with pytest.raises(MigrationInterrupted, match="changed after review"):
        library.migrate(forged)
    assert not (library.root / "escape").exists()
    assert not library.origins() and not any((library.root / "mods").iterdir())


def test_source_same_signature_change_during_copy_is_detected_and_rolled_back(tmp_path, monkeypatch):
    library, profile, row, _ = setup(tmp_path)
    review = library.review_migration([row.id])
    copy = library._copy_plan
    def alter(sources, destination, *args, **kwargs):
        file = Path(row.source) / "textures/shared.txt"
        info = file.stat()
        file.write_bytes(file.read_bytes().swapcase())
        os.utime(file, ns=(info.st_atime_ns, info.st_mtime_ns))
        return copy(sources, destination, *args, **kwargs)
    monkeypatch.setattr(library, "_copy_plan", alter)
    with pytest.raises(MigrationInterrupted, match="bytes changed"):
        library.migrate(review)
    assert next(item for item in library.rows(profile) if item.id == row.id).kind == "external"
    assert not library.origins() and not (library.root / "mods" / row.id).exists()
    assert list((library.root / ".history").iterdir())


@pytest.mark.parametrize("checkpoint", ["prepared", "placed", "before-commit"])
def test_late_cancel_rolls_back_only_owned_copy(tmp_path, monkeypatch, checkpoint):
    library, profile, row, _ = setup(tmp_path)
    review, cancel = library.review_migration([row.id]), threading.Event()
    original = contents(row.source)
    monkeypatch.setattr(library.owned, "_checkpoint", lambda name: cancel.set() if name == checkpoint else None)
    with pytest.raises(MigrationCancelled):
        library.migrate(review, cancel)
    assert next(item for item in library.rows(profile) if item.id == row.id).kind == "external"
    assert contents(row.source) == original and not library.origins()
    assert not (library.root / "mods" / row.id).exists()
    assert Library(library.root).recovery_report == []


def test_batch_cancel_retains_completed_copy_and_leaves_remaining_sources_external(tmp_path, monkeypatch):
    library, profile, row, _ = setup(tmp_path)
    rows = [item for item in library.rows(profile) if item.kind == "external"]
    original = {item.id: contents(item.source) for item in rows}
    review, cancel = library.review_migration([item.id for item in rows]), threading.Event()
    monkeypatch.setattr(library.owned, "_checkpoint", lambda name: cancel.set() if name == "committed" else None)
    with pytest.raises(MigrationCancelled) as error:
        library.migrate(review, cancel)
    assert len(error.value.completed) == 1
    current = {item.id: item for item in library.rows(profile)}
    assert current[rows[0].id].kind == "owned" and current[rows[1].id].kind == "external"
    assert {item.id: contents(item.source) for item in rows} == original


def test_unmanaged_and_nested_link_sources_are_not_migrated(tmp_path):
    library, profile, row, _ = setup(tmp_path)
    assert not any(item.kind == "foreign" for item in library.rows(profile))
    with library._db() as db:
        assert not db.execute("SELECT 1 FROM mods WHERE kind='foreign'").fetchone()
    target = tmp_path / "nested target"
    target.mkdir()
    link = Path(row.source) / "nested"
    junction(link, target)
    try:
        with pytest.raises(ManagerError, match="reparse"):
            library.review_migration([row.id])
    finally:
        link.rmdir() if os.name == "nt" else link.unlink()


@pytest.mark.parametrize("checkpoint", ["journal", "prepared", "placed", "before-commit", "committed"])
def test_actual_process_exit_migration_recovers_without_mutating_source(tmp_path, checkpoint):
    library, profile, row, _ = setup(tmp_path)
    original = contents(row.source)
    arguments = {"library": str(library.root), "operation": "migrate", "mod_id": row.id, "checkpoint": checkpoint}
    result = subprocess.run([sys.executable, "-m", "ui.mod_manager.tests.crash_worker", json.dumps(arguments)], capture_output=True, timeout=30)
    assert result.returncode == 91, result.stderr.decode()
    recovered = Library(library.root)
    current = next(item for item in recovered.rows(profile) if item.id == row.id)
    assert current.kind == ("owned" if checkpoint == "committed" else "external")
    assert bool(recovered.origins()) == (checkpoint == "committed")
    assert contents(row.source) == original and recovered.recovery_report
    assert Library(library.root).recovery_report == []


def test_uncertain_finalization_reports_committed_and_recovers(tmp_path, monkeypatch):
    library, profile, row, _ = setup(tmp_path)
    review = library.review_migration([row.id])
    monkeypatch.setattr(library.owned, "_finish", lambda record: (_ for _ in ()).throw(OSError("fixture finalization error")))
    with pytest.raises(CommittedTransaction):
        library.migrate(review)
    assert next(item for item in library.rows(profile) if item.id == row.id).kind == "owned"
    # Excluding unmanaged metadata is an explicit profile edit, never a game write.
    for item in library.rows(profile):
        if item.kind == "foreign":
            library.customize(profile, item.id, enabled=False)
    assert library.export_plan(profile)
    assert Library(library.root).recovery_report


def test_empty_source_preserves_empty_directories(tmp_path):
    library, profile, row, _ = setup(tmp_path)
    source = Path(row.source)
    for name in contents(source):
        (source / name).unlink()
    (source / "empty/nested").mkdir(parents=True)
    library.migrate(library.review_migration([row.id]))
    assert (library.root / "mods" / row.id / "empty/nested").is_dir()
    assert next(item for item in library.rows(profile) if item.id == row.id).kind == "owned"
