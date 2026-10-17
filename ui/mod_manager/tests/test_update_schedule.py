from types import SimpleNamespace
import time

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.nexus import NexusError
from .test_controller import finish


def manager_at(root, requests):
    manager = ManagerController()
    manager.library = Library(root, create=True)
    manager.rows = [SimpleNamespace(nexus_game="fallout4", nexus_mod_id=i) for i in range(1, 46)]
    manager.nexus_client = SimpleNamespace(user={"user_id": 42},
        mod_info=lambda game, mod: requests.append(mod) or {"version": "2"}, forget=lambda: None)
    return manager


def test_full_automatic_sweep_continues_after_short_pause_and_resumes_on_reopen(tmp_path, monkeypatch):
    wall = [time.time()]
    monkeypatch.setattr(time, "time", lambda: wall[0])
    requests = []
    root = tmp_path / "library"
    manager = manager_at(root, requests)
    try:
        manager.tick_updates()
        finish(manager)
        assert requests == list(range(1, 21))
        assert 55 < manager._next_update - time.monotonic() <= 61
    finally:
        manager.cleanup()
    wall[0] += 61
    manager = manager_at(root, requests)
    try:
        manager.tick_updates()
        finish(manager)
        assert requests == list(range(1, 41))
        assert len(manager.updates) == 40
        wall[0] += 61
        manager._next_update = manager._rate_blocked_until = 0
        manager.tick_updates()
        finish(manager)
        assert requests == list(range(1, 46))
        assert len(manager.updates) == 45
        assert manager._next_update - time.monotonic() > 5 * 3600
        manager.tick_updates()
        assert not manager.busy
    finally:
        manager.cleanup()


def test_rate_backoff_survives_reopen_and_switch_to_file_checks(tmp_path):
    root = tmp_path / "library"
    requests = []
    manager = manager_at(root, requests)
    try:
        def limited(*args):
            raise NexusError("Rate limited", 7200, http_status=429)
        manager.nexus_client.mod_info = limited
        manager.check_updates()
        with pytest.raises(NexusError):
            manager._future.result(timeout=5)
        manager.poll()
        assert manager._next_update - time.monotonic() > 7000
    finally:
        manager.cleanup()
    manager = manager_at(root, requests)
    try:
        manager.auto_file_checks = True
        manager.tick_updates()
        assert not manager.busy and not requests
        with pytest.raises(NexusError, match="backoff"):
            manager.check_nexus_batch()
    finally:
        manager.cleanup()


def test_idle_schedule_ticks_reuse_identities_until_inventory_changes(tmp_path):
    requests = []
    manager = manager_at(tmp_path / "library", requests)
    class Rows(list):
        iterations = 0
        def __iter__(self):
            self.iterations += 1
            return super().__iter__()
    try:
        manager.auto_updates = False
        manager.rows = Rows(manager.rows)
        manager.tick_updates()
        assert manager.rows.iterations == 1
        revision = manager._status_revision
        for _ in range(60):
            manager.tick_updates()
        assert manager.rows.iterations == 1 and manager._status_revision == revision
        manager.rows = Rows([*manager.rows, SimpleNamespace(nexus_game="fallout4", nexus_mod_id=99)])
        manager.tick_updates()
        assert manager.rows.iterations == 1 and len(manager._update_schedule_ids) == 46
    finally:
        manager.cleanup()
