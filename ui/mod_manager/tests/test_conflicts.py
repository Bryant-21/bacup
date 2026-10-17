import os
from pathlib import Path
import threading

import pytest

from creation_lib.mod_manager.conflicts import build_index, reveal
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .fixtures import make_mo2
from .test_mo2 import junction


def setup(tmp_path):
    instance = make_mo2(tmp_path)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(instance))
    profile = next(item.id for item in library.profiles() if item.name == "Default")
    rows = {row.name: row for row in library.rows(profile)}
    return library, profile, rows


def test_all_providers_winner_losers_hidden_disabled_and_cache(tmp_path):
    library, profile, rows = setup(tmp_path)
    (Path(rows["Disabled"].source) / "inactive.txt").write_text("inactive fixture")
    index = build_index(library, profile)
    shared = next(item for item in index.files if item.path == "textures/shared.txt")
    assert len(shared.providers) == 3 and len(shared.active) == 2
    assert shared.winner.mod_id == rows["Override"].id
    assert index.filtered("shared", "losing", rows["Base"].id) == [shared]
    assert index.filtered("shared", "winning", rows["Override"].id) == [shared]
    assert index.filtered("shared", "winning", rows["Disabled"].id) == []
    assert index.filtered("shared", "losing", rows["Disabled"].id) == []
    assert index.filtered("shared", "conflicts") == [shared]
    assert index.filtered("Override", "conflicts")
    assert len(index.filtered("", "hidden")) == 1
    hidden = index.filtered("", "hidden")[0]
    assert hidden.path == "hidden" and hidden.winner is None
    assert any(item.path == "inactive.txt" for item in index.filtered("", "inactive"))
    results = index.filtered("shared", "all")
    assert results is index.filtered("shared", "all")
    assert not any(item.path.casefold() == "meta.ini" for item in index.files)


def test_hidden_parent_folders_are_inactive_and_omitted_from_stage(tmp_path):
    library, profile, rows = setup(tmp_path)
    hidden = Path(rows["Override"].source) / "textures.mohidden/nested"
    hidden.mkdir(parents=True)
    (hidden / "hidden.txt").write_bytes(b"inert hidden folder fixture")
    index = build_index(library, profile)
    entry = next(item for item in index.files if item.path == "textures/nested/hidden.txt")
    assert entry.winner is None and all(provider.hidden for provider in entry.providers)
    assert not any("mohidden" in path for path in library.export_plan(profile).files)


def test_priority_matches_stage_and_tie_is_deterministic_across_display_sort(tmp_path):
    library, profile, rows = setup(tmp_path)
    index = build_index(library, profile)
    entry = next(item for item in index.files if item.path == "textures/shared.txt")
    assert library.export_plan(profile).files["textures/shared.txt"][1] == entry.winner.path
    library.reorder(profile, rows["Base"].id, 3)
    index = build_index(library, profile)
    assert next(item for item in index.files if item.path == "textures/shared.txt").winner.name == "Base"
    with library._db() as db:
        db.execute("UPDATE entries SET priority=1 WHERE profile_id=?", (profile,))
    winner = max(rows["Base"].id, rows["Override"].id)
    index = build_index(library, profile)
    entry = next(item for item in index.files if item.path == "textures/shared.txt")
    assert entry.winner.mod_id == winner
    assert library.export_plan(profile).files["textures/shared.txt"][1] == entry.winner.path


def test_windows_case_aliases_and_hidden_file_never_replace_active(tmp_path):
    library, profile, rows = setup(tmp_path)
    root = Path(rows["Override"].source)
    file = root / "textures/shared.txt"
    file.rename(root / "textures/SHARED.TXT")
    (root / "textures/shared.txt.mohidden").write_text("hidden alternative")
    entry = next(item for item in build_index(library, profile).files if item.path.casefold() == "textures/shared.txt")
    assert len(entry.providers) == 4 and entry.winner.name == "Override" and not entry.winner.hidden


def test_unavailable_and_unmanaged_issues_do_not_claim_game_inventory(tmp_path):
    library, profile, rows = setup(tmp_path)
    with library._db() as db:
        db.execute("UPDATE mods SET source='' WHERE id=?", (rows["Base"].id,))
        db.execute("UPDATE mods SET kind='foreign',source='' WHERE id=?", (rows["Disabled"].id,))
    index = build_index(library, profile)
    assert any("Unavailable imported source: Base" in item for item in index.issues)
    assert any("Unmanaged game content is not indexed" in item for item in index.issues)
    assert next(item for item in index.files if item.path == "textures/shared.txt").winner.name == "Override"
    with pytest.raises(ManagerError, match="no importable files"):
        library.export_plan(profile)


def test_disabled_sources_can_be_excluded_and_inert_scripts_are_ignored(tmp_path):
    library, profile, rows = setup(tmp_path)
    folder = Path(rows["Base"].source) / "fomod"
    folder.mkdir()
    (folder / "script.cs").write_text("must never execute")
    index = build_index(library, profile, include_disabled=False)
    assert all(provider.enabled for item in index.files for provider in item.providers)
    assert not any("script.cs" in item.path for item in index.files)
    assert library.export_plan(profile)


def test_source_reparse_is_reported_without_reading_target(tmp_path):
    library, profile, rows = setup(tmp_path)
    target = tmp_path / "target"
    target.mkdir()
    (target / "private.txt").write_text("fixture")
    link = Path(rows["Base"].source) / "link"
    junction(link, target)
    try:
        index = build_index(library, profile)
        assert any("Unindexed source Base" in item and "reparse" in item for item in index.issues)
        assert not any("private" in item.path for item in index.files)
    finally:
        link.rmdir() if os.name == "nt" else link.unlink()


def test_cancellation_and_profile_change_during_scan_invalidate_result(tmp_path):
    library, profile, rows = setup(tmp_path)
    cancel = threading.Event()
    with pytest.raises(Cancelled):
        build_index(library, profile, cancel, lambda fraction, name: cancel.set())
    changed = False
    def change(fraction, name):
        nonlocal changed
        if not changed:
            library.customize(profile, rows["Base"].id, enabled=False)
            changed = True
    with pytest.raises(ManagerError, match="organization changed"):
        build_index(library, profile, progress=change)


def test_reveal_uses_explorer_selection_not_source_execution_and_rejects_stale(tmp_path, monkeypatch):
    library, profile, _ = setup(tmp_path)
    provider = next(item for item in build_index(library, profile).files if item.path == "textures/shared.txt").winner
    calls = []
    monkeypatch.setattr("subprocess.Popen", lambda arguments: calls.append(arguments))
    reveal(provider)
    assert calls == [["explorer.exe", f"/select,{Path(provider.path).absolute()}"]]
    Path(provider.path).write_text("source edited externally")
    with pytest.raises(ManagerError, match="changed or disappeared"):
        reveal(provider)
    assert len(calls) == 1


def test_conflict_index_remains_consistent_after_owned_migration(tmp_path):
    library, profile, rows = setup(tmp_path)
    before = build_index(library, profile)
    library.migrate(library.review_migration([rows["Override"].id]))
    after = build_index(library, profile)
    assert [item.path for item in before.files] == [item.path for item in after.files]
    assert [(item.path, item.winner.mod_id if item.winner else None) for item in before.files] == [(item.path, item.winner.mod_id if item.winner else None) for item in after.files]
    assert next(item for item in after.files if item.path == "textures/shared.txt").winner.path.startswith(str(library.root))
