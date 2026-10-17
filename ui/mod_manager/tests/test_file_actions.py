from dataclasses import replace
import threading

import pytest

from creation_lib.mod_manager.conflicts import build_index
from creation_lib.mod_manager.file_actions import parent_folder, toggle_hidden, toggle_parent_hidden
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_transactions import setup_owned


def provider(library, profile, path="file.txt"):
    index = build_index(library, profile)
    return next(entry for entry in index.files if entry.path == path).providers[0]


def test_owned_hide_unhide_changes_logical_winner_and_retains_full_backup(tmp_path):
    library, profile, mod, _ = setup_owned(tmp_path)
    source = library.root / "mods" / mod
    original = source.joinpath("file.txt").read_bytes()
    source.joinpath("empty", "nested").mkdir(parents=True)
    toggle_hidden(library, provider(library, profile))
    index = build_index(library, profile)
    assert index.files[0].path == "file.txt" and index.files[0].winner is None
    assert source.joinpath("file.txt.mohidden").read_bytes() == original
    assert source.joinpath("empty", "nested").is_dir()
    assert len(library.history()) == 1
    toggle_hidden(library, index.files[0].providers[0])
    assert source.joinpath("file.txt").read_bytes() == original and not source.joinpath("file.txt.mohidden").exists()
    assert source.joinpath("empty", "nested").is_dir()
    assert build_index(library, profile).files[0].winner is not None and len(library.history()) == 2


@pytest.mark.parametrize("mode", ["cancelled", "failed", "stale", "collision", "foreign"])
def test_hide_rejects_cancel_failure_staleness_collisions_and_foreign_paths(tmp_path, monkeypatch, mode):
    library, profile, mod, external = setup_owned(tmp_path)
    selected = provider(library, profile)
    source = library.root / "mods" / mod
    cancel = threading.Event()
    if mode == "cancelled":
        cancel.set()
    elif mode == "failed":
        def fail(phase):
            if phase == "placed":
                raise OSError("generated hidden transaction failure")
        monkeypatch.setattr(library.owned, "_checkpoint", fail)
    elif mode == "stale":
        source.joinpath("file.txt").write_text("later user change")
    elif mode == "collision":
        source.joinpath("file.txt.mohidden").write_text("different hidden content")
    else:
        selected = replace(selected, path=str(external / "file.txt"))
    before = source.joinpath("file.txt").read_bytes()
    with pytest.raises(Cancelled if mode == "cancelled" else OSError if mode == "failed" else ManagerError):
        toggle_hidden(library, selected, cancel)
    assert source.joinpath("file.txt").read_bytes() == before and not library.history()


def test_hide_rechecks_selected_stamp_after_review(tmp_path, monkeypatch):
    library, profile, mod, _ = setup_owned(tmp_path)
    selected = provider(library, profile)
    review = library.review_owned
    def changed(*args, **kwargs):
        library.root.joinpath("mods", mod, "file.txt").write_text("later change during review")
        return review(*args, **kwargs)
    monkeypatch.setattr(library, "review_owned", changed)
    with pytest.raises(ManagerError, match="changed while preparing"):
        toggle_hidden(library, selected)
    assert library.root.joinpath("mods", mod, "file.txt").read_text() == "later change during review"
    assert not library.history()


def test_parent_hide_unhide_keeps_nested_empty_and_individually_hidden_content(tmp_path):
    library, profile, mod, _ = setup_owned(tmp_path)
    root = library.root / "mods" / mod
    root.joinpath("textures", "empty", "nested").mkdir(parents=True)
    root.joinpath("textures", "fixture.txt").write_bytes(b"generated parent content")
    root.joinpath("textures", "already.txt.mohidden").write_bytes(b"individually hidden")
    chosen = provider(library, profile, "textures/fixture.txt")
    assert parent_folder(library, chosen) == "textures"
    assert toggle_parent_hidden(library, chosen) == "textures.mohidden"
    assert root.joinpath("textures.mohidden", "empty", "nested").is_dir()
    assert all(entry.winner is None for entry in build_index(library, profile).files if entry.path.startswith("textures/"))
    chosen = provider(library, profile, "textures/fixture.txt")
    with pytest.raises(ManagerError, match="parent folder"):
        toggle_hidden(library, chosen)
    assert toggle_parent_hidden(library, chosen) == "textures"
    assert root.joinpath("textures", "already.txt.mohidden").read_bytes() == b"individually hidden"
    assert root.joinpath("textures", "empty", "nested").is_dir()
    assert provider(library, profile, "textures/fixture.txt").hidden is False
    assert len(library.history()) == 2


@pytest.mark.parametrize("fault", ["collision", "cancelled", "failed", "stale"])
def test_parent_hide_failures_preserve_entire_owned_tree(tmp_path, monkeypatch, fault):
    library, profile, mod, _ = setup_owned(tmp_path)
    root = library.root / "mods" / mod
    root.joinpath("textures").mkdir()
    path = root / "textures" / "fixture.txt"
    path.write_bytes(b"generated parent content")
    chosen = provider(library, profile, "textures/fixture.txt")
    cancel = threading.Event()
    if fault == "collision":
        root.joinpath("textures.mohidden").mkdir()
    elif fault == "cancelled":
        cancel.set()
    elif fault == "stale":
        path.write_bytes(b"later edit")
    else:
        def failed(phase):
            if phase == "placed":
                raise OSError("generated parent transaction failure")
        monkeypatch.setattr(library.owned, "_checkpoint", failed)
    before = path.read_bytes()
    with pytest.raises(Cancelled if fault == "cancelled" else OSError if fault == "failed" else ManagerError):
        toggle_parent_hidden(library, chosen, cancel)
    assert path.read_bytes() == before and not library.history()
