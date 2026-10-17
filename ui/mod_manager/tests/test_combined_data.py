"""Combined provenance model, using generated containers and loose payloads."""
from dataclasses import replace
import json
import threading

import pytest

from creation_lib.mod_manager.archive_index import Archive, ini_settings
from creation_lib.mod_manager.combined_data import build, resolve
from creation_lib.mod_manager.conflicts import FileEntry, Provider
from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_archive_index import ba2_fixture
from .test_standalone import game_fixture


def provider(name, priority=0, enabled=True, hidden=False):
    return Provider(name, name, priority, enabled, hidden, name, 1, 1)


def archive(name, rank, *, selected=True, activation="plugin candidate"):
    return Archive(name, provider(name), selected, activation, "Generated", rank, ("textures/shared.txt",), "BA2")


def test_effective_ini_empty_override_and_loose_over_archive_prediction():
    settings = ini_settings({"Fallout4.ini": "[Archive]\nbInvalidateOlderFiles=0\nsResourceDataDirsFinal=STRINGS\\\n",
                             "Fallout4Custom.ini": "[archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n"})
    loose = FileEntry("textures/shared.txt", (provider("base", -1), provider("mod", 1), provider("disabled", 2, False), provider("hidden", 3, hidden=True)))
    archives = (archive("A - Main.ba2", 1), archive("B - Main.ba2", 2))
    result = resolve(loose.path, loose, archives, settings)
    assert result.winner.kind == "loose" and result.winner.provider.name == "mod" and len(result.active) == 4
    for unknown in ({}, {"binvalidateolderfiles": "0", "sresourcedatadirsfinal": ""}, {"binvalidateolderfiles": "1", "sresourcedatadirsfinal": "STRINGS\\"}):
        assert resolve(loose.path, loose, archives, unknown).winner is None


def test_archive_order_ambiguity_inactive_and_other_game():
    archives = (archive("Base.ba2", 0, activation="listed"), archive("First - Main.ba2", 2), archive("Last - Main.ba2", 3))
    result = resolve("shared.txt", None, archives, {})
    assert result.winner.archive.name == "Last - Main.ba2" and "predicted" in result.reason
    assert resolve("shared.txt", None, (*archives, archive("Tie - Textures.ba2", 3)), {}).winner is None
    assert resolve("shared.txt", None, (archive("Weird - Extra.ba2", 1),), {}).winner is None
    assert resolve("shared.txt", None, archives, {}, game="skyrimse").winner is None
    loose = FileEntry("shared.txt", (provider("owned"),))
    result = resolve(loose.path, loose, (replace(archives[-1], selected=False),), {})
    assert result.winner.provider.name == "owned" and len(result.active) == 1


def test_generated_archive_and_loose_union_actual_cli_repeat_cancel(tmp_path):
    game = game_fixture(tmp_path)
    ba2_fixture(game / "Data" / "Base.ba2", ["textures/shared.txt", "meshes/archive-only.txt"])
    (game / "Data" / "textures").mkdir()
    (game / "Data" / "textures" / "shared.txt").write_text("generated")
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game)
    with library._db() as db:
        files = library.profiles()[0].files
        files["Fallout4.ini"] = "[Archive]\nsResourceArchiveList=Base.ba2\n"
        files["Fallout4Custom.ini"] = "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n"
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile))
    before = {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()}
    index = build(library, profile)
    shared = next(entry for entry in index.files if entry.path == "textures/shared.txt")
    assert len(shared.active) == 2 and shared.winner.kind == "loose"
    archived = next(entry for entry in index.files if entry.path == "meshes/archive-only.txt")
    assert archived.winner.kind == "archive" and archived.loose is None
    assert any(row.path == archived.path for row in index.tree.visible(set(), "archive-only"))
    assert [entry.path for entry in build(library, profile).files] == [entry.path for entry in index.files]
    assert before == {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()}
    event = threading.Event()
    event.set()
    with pytest.raises(Cancelled):
        build(library, profile, event)


def test_changes_between_inventories_rejected(tmp_path, monkeypatch):
    game = game_fixture(tmp_path)
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game)
    from creation_lib.mod_manager import conflicts
    original = conflicts.build_index
    def changed(*args, **kwargs):
        result = original(*args, **kwargs)
        with library._db() as db:
            files = library.profiles()[0].files
            files["Fallout4Custom.ini"] += "\n[Fixture]\nChanged=1\n"
            db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile))
        return result
    monkeypatch.setattr(conflicts, "build_index", changed)
    with pytest.raises(ManagerError, match="changed"):
        build(library, profile)


def test_changed_archive_snapshot_effective_ini_is_not_mixed_with_later_loose_settings(tmp_path, monkeypatch):
    game = game_fixture(tmp_path)
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game)
    from creation_lib.mod_manager import archive_index
    original = archive_index.build
    def stale(*args, **kwargs):
        result = original(*args, **kwargs)
        return replace(result, effective_ini={"Fallout4.ini": "older captured fixture"})
    monkeypatch.setattr(archive_index, "build", stale)
    with pytest.raises(ManagerError, match="between archive and loose"):
        build(library, profile)


def test_cancel_inside_adapter_keeps_no_partial_index(tmp_path):
    game = game_fixture(tmp_path)
    (game / "Data" / "fixture.txt").write_bytes(b"generated loose")
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game)
    event = threading.Event()
    def cancel_result(*args, **kwargs):
        result = resolve(*args, **kwargs)
        event.set()
        return result
    with pytest.raises(Cancelled):
        build(library, profile, event, resolver=cancel_result)
