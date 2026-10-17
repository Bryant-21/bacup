"""Disposable owned save copies, companions, cancellation and commit recovery."""
from dataclasses import replace
import io
from pathlib import Path
import threading

import pytest

from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.save_transfer import apply, review
from creation_lib.mod_manager.saves import inventory
from .test_saves import save_fixture
from .test_standalone import game_fixture


@pytest.fixture
def owned(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game_fixture(tmp_path))
    other = library.create_profile("Other", clone=profile)
    root = library.root / "runtime" / "profiles" / profile / "saves"
    root.mkdir(parents=True)
    path = save_fixture(root / "Fixture.fos")
    path.with_suffix(".f4se").write_bytes(b"generated extender companion")
    return library, profile, other, path


def test_review_copy_pair_backup_repeat_and_reopen_preserve_sources(owned):
    library, profile, other, path = owned
    original = {item.name: item.read_bytes() for item in path.parent.iterdir()}
    plan = review(library, inventory(library, profile), str(path), other)
    assert not (library.root / "runtime" / "profiles" / other / "saves").exists()
    destination = Path(apply(library, plan))
    assert {item.name: item.read_bytes() for item in destination.iterdir()} == original
    with pytest.raises(ManagerError, match="already exists"):
        apply(library, plan)
    backup = Path(apply(library, review(library, inventory(library, profile), str(path))))
    assert backup.joinpath("receipt.json").is_file()
    assert all((backup / name).read_bytes() == data for name, data in original.items())
    assert {item.name: item.read_bytes() for item in path.parent.iterdir()} == original
    assert len(inventory(Library(library.root), other).files) == 1


@pytest.mark.parametrize("fault", ["stale-save", "stale-companion", "collision", "foreign-source", "wrong-game", "cancelled"])
def test_failed_or_cancelled_staging_never_places_destination(owned, monkeypatch, fault):
    library, profile, other, path = owned
    plan = review(library, inventory(library, profile), str(path), other)
    destination = library.root / "runtime" / "profiles" / other / "saves"
    cancel = threading.Event()
    if fault.startswith("stale"):
        (path if fault == "stale-save" else path.with_suffix(".f4se")).write_bytes(b"changed after review")
    elif fault == "collision":
        destination.mkdir(parents=True)
        (destination / "FIXTURE.F4SE").write_bytes(b"existing companion")
    elif fault == "foreign-source":
        plan = replace(plan, files=(replace(plan.files[0], source=str(path.parent.parent / path.name)),))
    elif fault == "wrong-game":
        plan = replace(plan, game="skyrimse")
    else:
        monkeypatch.setattr("creation_lib.mod_manager.save_transfer._checkpoint", lambda name: cancel.set())
    with pytest.raises(Cancelled if fault == "cancelled" else ManagerError):
        apply(library, plan, cancel)
    assert not destination.exists() or [item.name for item in destination.iterdir()] == ["FIXTURE.F4SE"]
    assert not list(library.root.joinpath(".staging").iterdir())


def test_interrupted_first_placement_completes_pair_on_reopen(owned, monkeypatch):
    library, profile, other, path = owned
    original = path.read_bytes()
    plan = review(library, inventory(library, profile), str(path), other)
    def failed(name):
        if name == "placed":
            raise OSError("fixture crash after first owned placement")
    with monkeypatch.context() as patch:
        patch.setattr("creation_lib.mod_manager.save_transfer._checkpoint", failed)
        with pytest.raises(ManagerError, match="Reopen"):
            apply(library, plan)
    restored = Library(library.root)
    assert any("owned save" in line for line in restored.recovery_report)
    assert inventory(restored, other).files[0].companion
    assert path.read_bytes() == original
    assert not list(library.root.joinpath(".save-transfers").glob("*.json"))


def test_changed_partial_destination_stops_recovery_without_erasing_it(owned, monkeypatch):
    library, profile, other, path = owned
    plan = review(library, inventory(library, profile), str(path), other)
    with monkeypatch.context() as patch:
        patch.setattr("creation_lib.mod_manager.save_transfer._checkpoint", lambda name: (_ for _ in ()).throw(OSError("fixture crash")) if name == "placed" else None)
        with pytest.raises(ManagerError, match="Reopen"):
            apply(library, plan)
    placed = library.root / "runtime" / "profiles" / other / "saves" / path.name
    placed.write_bytes(b"user changed partial destination")
    with pytest.raises(ManagerError, match="occupied or changed"):
        Library(library.root)
    assert placed.read_bytes() == b"user changed partial destination"
    assert list(library.root.joinpath(".save-transfers").glob("*.json"))


@pytest.mark.parametrize("phase", ["checksum", "staging"])
def test_growing_save_stream_cannot_exceed_reviewed_read_or_staging_size(owned, monkeypatch, phase):
    from creation_lib.mod_manager.save_transfer import checksum
    library, profile, other, path = owned
    original = path.read_bytes()
    plan = review(library, inventory(library, profile), str(path), other)
    actual_open, read_calls = Path.open, 0
    def growing(file, mode="r", *args, **kwargs):
        nonlocal read_calls
        if file == path and mode == "rb":
            read_calls += 1
            if phase == "checksum" or read_calls == 2:
                return io.BytesIO(original + b"grew during this bounded read")
        return actual_open(file, mode, *args, **kwargs)
    monkeypatch.setattr(Path, "open", growing)
    with pytest.raises(ManagerError, match="grew beyond its reviewed size"):
        checksum(path) if phase == "checksum" else apply(library, plan)
    assert actual_open(path, "rb").read() == original
    assert not (library.root / "runtime/profiles" / other / "saves").exists()
    assert not list(library.root.joinpath(".staging").iterdir())
