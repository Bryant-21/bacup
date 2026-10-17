"""Default global discovery uses disposable redirected Documents and never writes there."""
from dataclasses import replace
import threading

import pytest

from creation_lib.mod_manager import global_saves
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_save_transfer import owned as owned_fixture
from .test_saves import save_fixture


def fixture(tmp_path, monkeypatch):
    library, profile, _, _ = owned_fixture.__wrapped__(tmp_path)
    documents = tmp_path / "redirected Documents"
    source = documents / "My Games" / "Fallout4" / "Saves"
    source.mkdir(parents=True)
    save_fixture(source / "Global.fos")
    (source / "Global.f4se").write_bytes(b"generated global companion")
    seen = []
    def known(identifier):
        seen.append(identifier)
        return documents
    monkeypatch.setattr(global_saves, "known_folder", known)
    return library, profile, documents, source, seen


def test_redirected_documents_repeat_inventory_and_owned_copy_are_read_only(tmp_path, monkeypatch):
    from creation_lib.mod_manager.save_transfer import apply, review
    library, profile, _, source, seen = fixture(tmp_path, monkeypatch)
    original = {path.name: path.read_bytes() for path in source.iterdir()}
    first = global_saves.inventory(library, profile)
    second = global_saves.inventory(library, profile)
    assert first == second and first.external and first.root == str(source)
    assert seen == [global_saves.DOCUMENTS] * 2
    target = library.create_profile("Imported global save", clone=profile)
    apply(library, review(library, first, first.files[0].path, target))
    assert {path.name: path.read_bytes() for path in source.iterdir()} == original
    assert library.root.joinpath("runtime", "profiles", target, "saves", "Global.f4se").read_bytes() == original["Global.f4se"]


def test_missing_default_folder_is_not_created_and_cancel_skips_discovery(tmp_path, monkeypatch):
    library, profile, documents, source, seen = fixture(tmp_path, monkeypatch)
    for path in source.iterdir():
        path.unlink()
    source.rmdir()
    with pytest.raises(ManagerError, match="absent"):
        global_saves.inventory(library, profile)
    assert not source.exists() and documents.is_dir()
    seen.clear()
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        global_saves.inventory(library, profile, cancel)
    assert not seen


@pytest.mark.parametrize("failure", ["relative", "unavailable", "other-game", "changed-game"])
def test_invalid_discovery_keeps_source_intact(tmp_path, monkeypatch, failure):
    library, profile, _, source, _ = fixture(tmp_path, monkeypatch)
    original = {path.name: path.read_bytes() for path in source.iterdir()}
    if failure == "relative":
        monkeypatch.setattr(global_saves, "known_folder", lambda _: "relative/Documents")
    elif failure == "unavailable":
        def unavailable(_):
            raise ManagerError("Documents lookup unavailable")
        monkeypatch.setattr(global_saves, "known_folder", unavailable)
    elif failure == "other-game":
        original_snapshot = library.profile_snapshot
        def other(identifier):
            p, rows, config = original_snapshot(identifier)
            return replace(p, files={**p.files, "_modbox_game": "starfield"}), rows, config
        monkeypatch.setattr(library, "profile_snapshot", other)
    else:
        original_inventory = global_saves.inventory_folder
        monkeypatch.setattr(global_saves, "inventory_folder", lambda *args: replace(original_inventory(*args), game="starfield"))
    with pytest.raises(ManagerError):
        global_saves.inventory(library, profile)
    assert {path.name: path.read_bytes() for path in source.iterdir()} == original


def test_global_directory_junction_is_rejected_without_following(tmp_path, monkeypatch):
    from .test_mo2 import junction
    library, profile, _, source, _ = fixture(tmp_path, monkeypatch)
    original = {path.name: path.read_bytes() for path in source.iterdir()}
    retained = source.with_name("retained fixture")
    source.rename(retained)
    junction(source, retained)
    try:
        with pytest.raises(ManagerError, match="reparse"):
            global_saves.inventory(library, profile)
        assert {path.name: path.read_bytes() for path in retained.iterdir()} == original
    finally:
        source.rmdir()
