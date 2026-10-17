"""Mocked reviewed account/download batches and destination pinning."""
from dataclasses import replace
import json
import threading

import pytest

from creation_lib.mod_manager import nexus_bulk as bulk
from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.download_queue import DownloadQueue
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.nexus import NexusError, parse_nxm
from creation_lib.mod_manager.paths import ManagerError
from .test_download_scheduler import Wire, link


class Account:
    def __init__(self):
        self.user = {"name": "Mock account", "user_id": 42, "premium": True}
        self.calls, self.errors, self.callback = [], {}, lambda: None

    def call(self, identity, action, version=""):
        self.calls.append((identity, action, version))
        self.callback()
        if identity[1] in self.errors:
            raise self.errors[identity[1]]

    def endorse(self, game, mod, version):
        self.call((game, mod), "endorse", version)

    def unendorse(self, game, mod, version):
        self.call((game, mod), "unendorse", version)

    def track(self, game, mod, enabled):
        self.call((game, mod), "track" if enabled else "untrack")

    def forget(self):
        pass


@pytest.fixture
def manager(tmp_path):
    m = ManagerController()
    m.library = Library(tmp_path / "library", create=True)
    m.profile_id = m.library.create_profile("Fixture")
    with m.library._db() as db:
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps({"_modbox_game": "fo4"}), m.profile_id))
        for index in range(3):
            source = tmp_path / ("source-" + str(index))
            source.mkdir()
            (source / "keep.txt").write_bytes(b"immutable fixture source")
            db.execute("INSERT INTO mods(id,name,source,kind,nexus_game,nexus_mod_id,nexus_file_id,version) VALUES (?,?,?,'external','fallout4',?,?,'1.2')",
                       (str(index), "Fixture " + str(index), str(source), index + 1, index + 1))
            db.execute("INSERT INTO entries VALUES (?,?,?,1,'')", (m.profile_id, str(index), index))
    m.refresh()
    m.nexus_client = Account()
    yield m
    m.cleanup()
    assert all((tmp_path / ("source-" + str(i)) / "keep.txt").read_bytes() == b"immutable fixture source" for i in range(3))


@pytest.mark.parametrize("action", ["endorse", "unendorse", "track", "untrack"])
def test_review_only_reads_local_metadata_then_selected_actions_send_once(manager, action):
    plan = bulk.account_review(manager, ["0", "1", "2"], action)
    assert not manager.nexus_client.calls
    items = bulk.validate(manager, plan, ["0", "2"])
    result = bulk.account_run(manager, plan, items)
    assert len(manager.nexus_client.calls) == len(result.rows) == 2
    assert all("refresh" in state for _, state in result.rows)
    assert not manager.nexus_states


@pytest.mark.parametrize("failure", [401, 403, 429])
def test_auth_and_rate_failure_stop_following_items_with_unknown_outcome(manager, failure):
    plan = bulk.account_review(manager, ["0", "1", "2"], "endorse")
    items = bulk.validate(manager, plan, ["0", "1", "2"])
    manager.nexus_client.errors[2] = NexusError("fixture failure", 600, http_status=failure)
    result = bulk.account_run(manager, plan, items)
    assert len(manager.nexus_client.calls) == 2 and result.stopped and result.retry_after == 600
    assert "unconfirmed" in result.rows[1][1] and result.rows[2][1] == "Not sent"


def test_one_unavailable_mod_does_not_invent_success_or_block_other_items(manager):
    plan = bulk.account_review(manager, ["0", "1", "2"], "track")
    manager.nexus_client.errors[2] = NexusError("fixture removed", http_status=404)
    result = bulk.account_run(manager, plan, bulk.validate(manager, plan, ["0", "1", "2"]))
    assert len(manager.nexus_client.calls) == 3 and not result.stopped
    assert "unconfirmed" in result.rows[1][1] and "accepted" in result.rows[2][1]


@pytest.mark.parametrize("when", ["before", "after-first", "context-after-first"])
def test_cancellation_and_context_change_keep_partial_outcomes(manager, when):
    cancel = threading.Event()
    plan = bulk.account_review(manager, ["0", "1", "2"], "untrack")
    items = bulk.validate(manager, plan, ["0", "1", "2"])
    if when == "before":
        cancel.set()
    elif when == "after-first":
        manager.nexus_client.callback = cancel.set
    else:
        manager.nexus_client.callback = lambda: setattr(manager, "target_id", "changed")
    result = bulk.account_run(manager, plan, items, cancel)
    assert result.stopped and len(manager.nexus_client.calls) == (0 if when == "before" else 1)
    assert all("Not sent" in state for _, state in result.rows[(0 if when == "before" else 1):])


@pytest.mark.parametrize("change", ["account", "profile", "file", "version"])
def test_stale_account_review_rejects_before_any_request(manager, change):
    plan = bulk.account_review(manager, ["0"], "track")
    if change == "account":
        manager.nexus_client.user["user_id"] = 43
    elif change == "profile":
        manager.profile_id = "other"
    else:
        row = manager.rows_by_id["0"]
        row = replace(row, **({"nexus_file_id": 999} if change == "file" else {"version": "2"}))
        manager.rows_by_id["0"] = row
        manager.rows = [row if item.id == "0" else item for item in manager.rows]
    with pytest.raises(NexusError, match="changed"):
        bulk.validate(manager, plan, ["0"])
    assert not manager.nexus_client.calls


def test_duplicate_identity_deduplicates_writes_and_requires_version_choice(manager):
    with manager.library._db() as db:
        db.execute("UPDATE mods SET nexus_mod_id=1 WHERE id='1'")
    manager.refresh()
    plan = bulk.account_review(manager, ["0", "1"], "endorse")
    result = bulk.account_run(manager, plan, bulk.validate(manager, plan, ["0", "1"]))
    assert len(manager.nexus_client.calls) == 1 and "already sent once" in result.rows[1][1]
    with manager.library._db() as db:
        db.execute("UPDATE mods SET version='2' WHERE id='1'")
    manager.refresh()
    plan = bulk.account_review(manager, ["0", "1"], "endorse")
    with pytest.raises(ManagerError, match="one installed version"):
        bulk.validate(manager, plan, ["0", "1"])
    assert len(bulk.validate(manager, plan, ["1"])) == 1


def test_unmapped_or_unknown_version_items_remain_unavailable(manager):
    with manager.library._db() as db:
        db.execute("UPDATE mods SET nexus_mod_id=0 WHERE id='0'")
        db.execute("UPDATE mods SET version='' WHERE id='1'")
    manager.refresh()
    plan = bulk.account_review(manager, ["0", "1", "2"], "endorse")
    assert plan.items[0].issue and plan.items[1].issue and not plan.items[2].issue
    with pytest.raises(ManagerError):
        bulk.validate(manager, plan, ["0", "1"])


@pytest.mark.parametrize("selection", [[], [str(i) for i in range(21)]])
def test_account_batch_size_is_explicitly_bounded(manager, selection):
    with pytest.raises(ManagerError):
        bulk.account_review(manager, selection, "track")


@pytest.mark.parametrize("mode", ["normal", "one-fails", "rate-limited", "cancel"])
def test_bulk_downloads_use_existing_scheduler_without_installing_or_persisting_secrets(manager, mode):
    wire = Wire(fail=mode == "one-fails", rate=mode == "rate-limited")
    manager.nexus_client = wire.client()
    manager.set_download_slots(1)
    for index in range(3):
        manager.accept_nxm(link(index))
    plan = bulk.download_review(manager)
    assert not any("download_link" in request.full_url for request in wire.requests)
    result = bulk.queue_downloads(manager, plan, [item.id for item in plan.items])
    assert len(result.rows) == 3 and manager.download_slots == 1
    assert manager._download_scheduler and not manager.prepared
    try:
        if mode == "cancel":
            assert wire.entered[0].wait(5)
            for identity, _ in result.rows:
                manager.pause_download(identity)
        wire.release.set()
        for transfer in list(manager._download_scheduler.transfers.values()):
            transfer.future.exception(timeout=15)
        manager._poll_downloads()
        jobs = DownloadQueue(manager.library).records()
        if mode == "normal":
            assert all(job["state"] == "complete" for job in jobs)
        elif mode == "one-fails":
            assert sum(job["state"] == "complete" for job in jobs) == 2
        elif mode == "rate-limited":
            assert not any(job["state"] == "complete" for job in jobs)
        else:
            assert not any(job["state"] == "complete" for job in jobs)
        persisted = json.dumps(jobs)
        assert "private-fixture" not in persisted and "nexusmods.com" not in persisted
        assert not any(row.kind == "owned" for row in manager.rows)
    finally:
        wire.release.set()


@pytest.mark.parametrize("problem", ["expired", "other-account", "free-bare", "wrong-game", "other-destination"])
def test_unavailable_download_requests_are_blocked_in_review(manager, problem):
    manager.accept_nxm(link(0))
    identity = link(0).identity
    original = manager.pending_links[identity]
    if problem == "expired":
        manager.pending_links[identity] = replace(original, expires=1)
    elif problem == "other-account":
        manager.pending_links[identity] = replace(original, user_id=43)
    elif problem == "free-bare":
        manager.nexus_client.user["premium"] = False
        manager.pending_links[identity] = parse_nxm("nxm://fallout4/mods/123/files/1")
    elif problem == "wrong-game":
        manager.pending_links[identity] = replace(original, game="skyrim")
    else:
        manager.pending_destinations[identity] = ("other", "other", "other", "fallout4")
    plan = bulk.download_review(manager)
    assert plan.items[0].issue
    with pytest.raises(ManagerError):
        bulk.validate(manager, plan, [identity])


def test_refreshing_a_browser_key_invalidates_download_batch_without_queueing(manager):
    manager.accept_nxm(link(0))
    plan = bulk.download_review(manager)
    manager.accept_nxm(replace(link(0), key="replacement-fixture-key"))
    with pytest.raises(NexusError, match="changed"):
        bulk.validate(manager, plan, [link(0).identity])
    assert manager._download_scheduler is None


def test_profile_switch_cannot_retarget_an_existing_pending_link(manager):
    manager.accept_nxm(link(0))
    manager.profile_id = manager.library.create_profile("Other")
    with manager.library._db() as db:
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps({"_modbox_game": "fo4"}), manager.profile_id))
    manager.refresh()
    with pytest.raises(NexusError, match="another library/profile/target"):
        manager.download_nxm(link(0).identity)
    assert not list((manager.library.root / "downloads").iterdir())
