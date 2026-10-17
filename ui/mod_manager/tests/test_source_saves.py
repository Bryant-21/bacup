"""Explicit read-only source browsing and no-replace copies into owned profiles."""
from dataclasses import replace
from pathlib import Path
import threading

import pytest

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.save_backups import review as restore_review
from creation_lib.mod_manager.save_transfer import apply, review
from creation_lib.mod_manager.saves import fo4_header, inventory, inventory_folder
from .test_save_transfer import owned as owned_fixture
from .test_saves import save_fixture

owned = pytest.fixture(owned_fixture.__wrapped__)


def source_fixture(owned, tmp_path):
    library, profile, other, _ = owned
    folder = tmp_path / "source global saves fixture"
    folder.mkdir()
    path = save_fixture(folder / "Source.fos")
    path.with_suffix(".f4se").write_bytes(b"generated source companion")
    folder.joinpath("ignored.txt").write_text("read-only ordinary unrelated fixture")
    return library, profile, other, folder, path


def test_explicit_inventory_header_copy_current_profile_backup_restore_and_repeat(owned, tmp_path):
    library, profile, other, folder, path = source_fixture(owned, tmp_path)
    original = {item.name: item.read_bytes() for item in folder.iterdir()}
    index = inventory_folder(library, profile, folder)
    assert index.external and len(index.files) == 1 and index.root == str(folder)
    assert fo4_header(index.files[0])["player"] == "Fixture Character"
    plan = review(library, index, str(path), profile)
    destination = Path(apply(library, plan))
    assert destination.joinpath("Source.fos").read_bytes() == original["Source.fos"]
    assert destination.joinpath("Source.f4se").read_bytes() == original["Source.f4se"]
    assert len(inventory(Library(library.root), profile).files) == 2
    with pytest.raises(ManagerError, match="already exists"):
        apply(library, plan)
    backup = Path(apply(library, review(library, index, str(path))))
    restored = Path(apply(library, restore_review(library, profile, backup.name, other)))
    assert restored.joinpath("Source.fos").read_bytes() == original["Source.fos"]
    assert {item.name: item.read_bytes() for item in folder.iterdir()} == original


@pytest.mark.parametrize("fault", ["save", "companion", "missing-companion", "identity", "new-companion", "foreign-file", "wrong-game", "cancelled", "staging-cancel"])
def test_source_stale_cancelled_or_failed_copy_places_no_files_and_keeps_sources(owned, tmp_path, monkeypatch, fault):
    library, profile, other, folder, path = source_fixture(owned, tmp_path)
    if fault == "new-companion":
        path.with_suffix(".f4se").unlink()
    plan = review(library, inventory_folder(library, profile, folder), str(path), other)
    cancel = threading.Event()
    if fault in {"save", "companion", "new-companion"}:
        (path if fault == "save" else path.with_suffix(".f4se")).write_bytes(b"changed after review")
    elif fault == "missing-companion":
        path.with_suffix(".f4se").unlink()
    elif fault == "identity":
        folder.rename(tmp_path / "retained original source")
        folder.mkdir()
        save_fixture(folder / "Source.fos")
    elif fault == "foreign-file":
        plan = replace(plan, files=(replace(plan.files[0], source=str(folder.parent / path.name)), *plan.files[1:]))
    elif fault == "wrong-game":
        plan = replace(plan, game="skyrimse")
    elif fault == "cancelled":
        cancel.set()
    else:
        monkeypatch.setattr("creation_lib.mod_manager.save_transfer._checkpoint", lambda stage: cancel.set())
    original = {item.name: item.read_bytes() for item in folder.iterdir()}
    with pytest.raises(Cancelled if "cancel" in fault else ManagerError):
        apply(library, plan, cancel)
    assert {item.name: item.read_bytes() for item in folder.iterdir()} == original
    assert not (library.root / "runtime/profiles" / other / "saves").exists()
    assert not list(library.root.joinpath(".staging").iterdir())


def test_interrupted_source_pair_copy_recovers_from_owned_stage_without_rereading_source(owned, tmp_path, monkeypatch):
    library, profile, other, folder, path = source_fixture(owned, tmp_path)
    plan = review(library, inventory_folder(library, profile, folder), str(path), other)
    original = path.read_bytes()
    def crash(stage):
        if stage == "placed":
            raise OSError("disposable interruption")
    with monkeypatch.context() as patch:
        patch.setattr("creation_lib.mod_manager.save_transfer._checkpoint", crash)
        with pytest.raises(ManagerError, match="Reopen"):
            apply(library, plan)
    path.write_bytes(b"source edited independently after interrupted copy")
    recovered = Library(library.root)
    assert inventory(recovered, other).files[0].companion
    assert (recovered.root / "runtime/profiles" / other / "saves/Source.fos").read_bytes() == original
    assert path.read_bytes() == b"source edited independently after interrupted copy"


def test_source_inventory_rejects_links_missing_roots_cancel_and_network_before_read(owned, tmp_path, monkeypatch):
    from .test_mo2 import junction
    library, profile, _, folder, _ = source_fixture(owned, tmp_path)
    linked = tmp_path / "linked save folder"
    junction(linked, folder)
    try:
        with pytest.raises(ManagerError, match="Links"):
            inventory_folder(library, profile, linked)
    finally:
        linked.rmdir() if linked.is_dir() and not linked.is_symlink() else linked.unlink()
    with pytest.raises(ManagerError, match="folder"):
        inventory_folder(library, profile, tmp_path / "missing")
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        inventory_folder(library, profile, folder, cancel)
    monkeypatch.setattr("creation_lib.mod_manager.mo2.local_mod_target", lambda path: False)
    junction(linked, folder)
    try:
        with pytest.raises(ManagerError, match="network"):
            inventory_folder(library, profile, linked)
    finally:
        linked.rmdir() if linked.is_dir() and not linked.is_symlink() else linked.unlink()
