"""Automatic offline statuses, invalidation and applicable archive predictions."""
import json
import threading
import time

import pytest

from creation_lib.mod_manager import launch_targets, local_status, target_mod_sources
from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.status import badges, file_states
from .test_archive_index import ba2_fixture
from .test_controller import finish
from . import test_launch_resolution as target_fixtures
from .test_standalone import game_fixture

configured = target_fixtures.configured


def providers(library, profile, root):
    paths = {"low": ("shared/a.txt",), "middle": ("shared/a.txt", "shared/b.txt"), "high": ("shared/b.txt",), "alone": ("unique.txt",), "off": ("shared/a.txt",)}
    with library._db() as db:
        for priority, (identifier, names) in enumerate(paths.items()):
            folder = root / identifier
            folder.mkdir()
            for name in names:
                target = folder / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(identifier.encode())
            db.execute("INSERT INTO mods(id,name,source,kind) VALUES (?,?,?,'external')", (identifier, identifier, str(folder)))
            db.execute("INSERT INTO entries VALUES (?,?,?,?,?)", (profile, identifier, priority, int(identifier != "off"), ""))
    return paths


def wait_scan(manager, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        manager.poll()
        if manager.conflict_scan_state in {"ready", "partial", "error"} and manager._conflict_future is None:
            return
        threading.Event().wait(.01)
    pytest.fail("Local status scan did not finish: " + manager.conflict_scan_error)


def test_open_scan_no_manual_command_or_nexus_priority_enable_and_source_changes(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game_fixture(tmp_path))
    providers(library, profile, tmp_path)
    manager = ManagerController()
    try:
        manager.library, manager.profile_id = library, profile
        manager.refresh()
        assert manager.conflict_scan_state == "queued" and not manager.busy
        assert badges(manager.rows[0], None, {}, {}, scan_state="queued")[0].text == "…"
        wait_scan(manager)
        assert manager.conflict_scan_state == "ready" and manager.nexus_client is None
        states = manager.status_rows()
        assert {key: value["conflicts"] for key, value in states.items()} == {"low": "loses", "middle": "mixed", "high": "wins", "alone": "uncontested", "off": "inactive"}
        assert manager.conflict_index is manager.conflict_index
        manager.customize("middle", enabled=False)
        finish(manager)
        assert manager.file_index is None and manager.conflict_scan_state == "queued"
        wait_scan(manager)
        assert manager.status_rows()["low"]["conflicts"] == "uncontested"
        manager.bulk_organize(("middle",), enabled=True, priority=0)
        finish(manager)
        wait_scan(manager)
        assert manager.status_rows()["low"]["conflicts"] == "wins"
        (tmp_path / "alone" / "unique.txt").unlink()
        # Simulate the periodic source-change poll becoming due.
        manager._conflict_due = time.monotonic()
        manager.poll()
        wait_scan(manager)
        assert manager.status_rows()["alone"]["conflicts"] == "empty"
    finally:
        manager.cleanup()


def test_target_source_and_enable_substitution_scans_effective_context(configured, tmp_path):
    library, profile, games = configured
    providers(library, profile, tmp_path)
    identifiers = [launch_targets.apply(library, launch_targets.review(library, "Target " + str(index), game)) for index, game in enumerate(games)]
    variant = tmp_path / "variant"
    variant.mkdir()
    (variant / "unique.txt").write_bytes(b"variant override")
    review = target_mod_sources.review(library, identifiers[1], profile, "high", source=str(variant), enabled=True)
    target_mod_sources.apply(library, review, accept_identity=True)
    manager = ManagerController()
    try:
        manager.library, manager.profile_id = library, profile
        manager.refresh()
        manager.select_target(identifiers[0])
        wait_scan(manager)
        assert manager.status_rows()["high"]["conflicts"] == "wins"
        old = manager._conflict_index
        manager.select_target(identifiers[1])
        assert manager.conflict_index is None and manager.plugin_catalog is None
        wait_scan(manager)
        assert manager.status_rows()["middle"]["conflicts"] == "wins"
        assert manager.status_rows()["high"]["conflicts"] == "loses"  # higher-priority alone wins unique.txt
        assert manager.rows_by_id["high"].source == str(variant)
        manager._accept_conflicts(manager._conflict_generation - 1, (old, None), final=True)
        assert manager.rows_by_id["high"].source == str(variant) and manager._conflict_index is not old
        assert library.rows(profile)[2].source == str(tmp_path / "high")
    finally:
        manager.cleanup()


def test_applicable_archive_members_cached_and_unresolved_precedence_explicit(tmp_path):
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game_fixture(tmp_path))
    providers(library, profile, tmp_path)
    ba2_fixture(tmp_path / "low" / "Low.ba2", ["archive/shared.txt"])
    ba2_fixture(tmp_path / "middle" / "Middle.ba2", ["archive/shared.txt"])
    with library._db() as db:
        files = library.profiles()[0].files
        files["Fallout4.ini"] = "[Archive]\nsResourceArchiveList=Low.ba2,Middle.ba2\n"
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile))
    manager = ManagerController()
    try:
        manager.library, manager.profile_id = library, profile
        manager.refresh()
        wait_scan(manager)
        assert manager.archive_index and len(manager.archive_index.members) == 1
        assert "loses" in manager.conflict_index.states["low"] and "wins" in manager.conflict_index.states["middle"]
        badge = badges(manager.rows_by_id["middle"], manager.conflict_index, file_states(manager.conflict_index), {})[0]
        assert "applicable archive members" in badge.description and "predictions" in badge.description
        manager.refresh_conflicts(delay=0)
        wait_scan(manager)
        with library._db() as db:
            assert db.execute("SELECT COUNT(*) FROM archive_metadata").fetchone()[0] == 2
        # Add a loose competitor and remove explicit loose override settings.
        (tmp_path / "alone" / "archive").mkdir()
        (tmp_path / "alone" / "archive/shared.txt").write_bytes(b"loose competitor")
        with library._db() as db:
            files["Fallout4Custom.ini"] = "[Archive]\nbInvalidateOlderFiles=0\nsResourceDataDirsFinal=STRINGS\\\n"
            db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile))
        manager.refresh()
        wait_scan(manager)
        assert manager.conflict_scan_state == "partial" and "unresolved" in manager.conflict_index.states["alone"]
        assert manager.status_rows()["alone"]["conflicts"] == "unknown"
    finally:
        manager.cleanup()


def test_partial_missing_source_preserves_observed_status_and_scan_error_retry(tmp_path, monkeypatch):
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game_fixture(tmp_path))
    providers(library, profile, tmp_path)
    library.customize(profile, "off", enabled=False)
    with library._db() as db:
        db.execute("UPDATE mods SET source=? WHERE id='off'", (str(tmp_path / "missing"),))
    manager = ManagerController()
    try:
        manager.library, manager.profile_id = library, profile
        manager.refresh()
        wait_scan(manager)
        assert manager.conflict_scan_state == "partial"
        assert manager.status_rows()["middle"]["conflicts"] == "mixed"
        assert manager.status_rows()["off"]["conflicts"] == "unavailable"
        badge = badges(manager.rows_by_id["middle"], manager.conflict_index, manager.conflict_index.states, {})[0]
        assert badge.text == "W/L" and "Partial inventory" in badge.description
        original = local_status.scan
        monkeypatch.setattr(local_status, "scan", lambda *args, **kwargs: (_ for _ in ()).throw(OSError("generated scan failure")))
        manager.refresh_conflicts(delay=0)
        wait_scan(manager)
        assert manager.conflict_scan_state == "error" and "generated scan failure" in manager.conflict_scan_error
        monkeypatch.setattr(local_status, "scan", original)
        manager.refresh_conflicts(delay=0)
        wait_scan(manager)
        assert manager.status_rows()["middle"]["conflicts"] == "mixed"
    finally:
        manager.cleanup()
