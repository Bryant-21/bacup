"""Retained owned save restore freshness, no-replace placement and recovery."""
from dataclasses import replace
import json
from pathlib import Path
import threading

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.save_backups import inventory, review
from creation_lib.mod_manager.save_transfer import apply, review as copy_review
from creation_lib.mod_manager.saves import inventory as saves
from .test_save_transfer import owned as owned_fixture

owned = pytest.fixture(owned_fixture.__wrapped__)


def bundle(owned):
    library, profile, other, path = owned
    backup = Path(apply(library, copy_review(library, saves(library, profile), str(path))))
    return library, profile, other, path, backup


def test_receipt_inventory_restore_pair_repeat_and_deleted_source_profile(owned):
    library, profile, other, path, backup = bundle(owned)
    original = {item.name: item.read_bytes() for item in backup.iterdir()}
    records = inventory(library, profile)
    assert len(records) == 1 and records[0].names == ("Fixture.fos", "Fixture.f4se")
    plan = review(library, profile, backup.name, other)
    library.delete_profile(profile)
    destination = Path(apply(library, plan))
    assert destination.joinpath("Fixture.fos").read_bytes() == path.read_bytes()
    assert destination.joinpath("Fixture.f4se").read_bytes() == b"generated extender companion"
    with pytest.raises(ManagerError, match="already exists"):
        apply(library, plan)
    assert {item.name: item.read_bytes() for item in backup.iterdir()} == original
    assert len(saves(Library(library.root), other).files) == 1


def test_restore_into_same_profile_requires_empty_destination_names(owned):
    library, profile, _, path, backup = bundle(owned)
    with pytest.raises(ManagerError, match="already exists"):
        review(library, profile, backup.name, profile)
    original = path.read_bytes()
    path.unlink()
    path.with_suffix(".f4se").unlink()
    apply(library, review(library, profile, backup.name, profile))
    assert path.read_bytes() == original and path.with_suffix(".f4se").is_file()


@pytest.mark.parametrize("fault", ["save", "companion", "receipt", "unexpected", "wrong-source", "cancelled", "collision"])
def test_stale_cancelled_or_failed_restore_preserves_backup_and_destinations(owned, fault, monkeypatch):
    library, profile, other, _, backup = bundle(owned)
    plan = review(library, profile, backup.name, other)
    cancel = threading.Event()
    destination = library.root / "runtime/profiles" / other / "saves"
    if fault in {"save", "companion"}:
        backup.joinpath("Fixture.fos" if fault == "save" else "Fixture.f4se").write_bytes(b"edited backup bytes")
    elif fault == "receipt":
        receipt = backup / "receipt.json"
        receipt.write_text(json.dumps(json.loads(receipt.read_text("utf-8")), indent=2), "utf-8")
    elif fault == "unexpected":
        backup.joinpath("unexpected.txt").write_text("must retain this file too")
    elif fault == "wrong-source":
        plan = replace(plan, files=(replace(plan.files[0], source=str(backup.parent / "foreign.fos")),))
    elif fault == "cancelled":
        monkeypatch.setattr("creation_lib.mod_manager.save_transfer._checkpoint", lambda stage: cancel.set())
    else:
        destination.mkdir(parents=True)
        destination.joinpath("FIXTURE.F4SE").write_bytes(b"existing user companion")
    snapshot = {item.name: item.read_bytes() for item in backup.iterdir()}
    with pytest.raises(Cancelled if fault == "cancelled" else ManagerError):
        apply(library, plan, cancel)
    assert {item.name: item.read_bytes() for item in backup.iterdir()} == snapshot
    assert not destination.exists() or [item.name for item in destination.iterdir()] == ["FIXTURE.F4SE"]
    assert not list(library.root.joinpath(".staging").iterdir())


def test_interrupted_restore_finishes_pair_on_reopen(owned, monkeypatch):
    library, profile, other, _, backup = bundle(owned)
    plan = review(library, profile, backup.name, other)
    def interrupted(stage):
        if stage == "placed":
            raise OSError("disposable interrupted restore")
    with monkeypatch.context() as patch:
        patch.setattr("creation_lib.mod_manager.save_transfer._checkpoint", interrupted)
        with pytest.raises(ManagerError, match="Reopen"):
            apply(library, plan)
    restored = Library(library.root)
    assert saves(restored, other).files[0].companion
    assert backup.joinpath("Fixture.fos").is_file()


@pytest.mark.parametrize("fault", ["oversized", "bad-json", "bad-companion", "missing", "bad-id"])
def test_invalid_bundle_inventory_is_bounded_and_fails_explicitly(owned, fault):
    library, profile, _, _, backup = bundle(owned)
    receipt = backup / "receipt.json"
    if fault == "oversized":
        receipt.write_bytes(b"x" * 65537)
    elif fault == "bad-json":
        receipt.write_bytes(b"[]")
    elif fault == "bad-companion":
        record = json.loads(receipt.read_text("utf-8"))
        record["files"][1]["name"] = "Foreign.f4se"
        receipt.write_text(json.dumps(record), "utf-8")
    elif fault == "missing":
        backup.joinpath("Fixture.f4se").unlink()
    else:
        backup.rename(backup.with_name("foreign"))
    with pytest.raises(ManagerError):
        inventory(library, profile)


def test_wrong_game_cancel_and_active_runtime_do_not_restore(owned, monkeypatch):
    library, profile, other, _, backup = bundle(owned)
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        review(library, profile, backup.name, other, cancel)
    plan = review(library, profile, backup.name, other)
    with pytest.raises(ManagerError, match="same game"):
        apply(library, replace(plan, game="skyrimse"))
    monkeypatch.setattr("creation_lib.mod_manager.runtime.active_sessions", lambda root: ["active fixture"])
    with pytest.raises(ManagerError, match="runtime"):
        apply(library, plan)
