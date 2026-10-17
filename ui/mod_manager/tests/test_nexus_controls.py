"""Nexus controls use mocked sessions; no account, browser or game writes."""
import json
import threading
from dataclasses import replace
from pathlib import Path

import pytest

from creation_lib.mod_manager import nexus_account, nexus_batch, target_mod_sources
from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.download_install import capture, destination
from creation_lib.mod_manager.launch_targets import TargetLibrary
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.nexus import NexusClient, NexusError
from creation_lib.mod_manager.nexus_links import file_page
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_controller import finish
from .test_download_install import imported, owned_download  # noqa: F401
from .test_nexus import Response
from .test_nexus_state import Transport, fixture_data
from .test_target_customization import context  # noqa: F401
from .test_launch_targets import targets  # noqa: F401


class Client:
    def __init__(self):
        self.calls, self.fail = [], {}

    def get(self, endpoint, value):
        self.calls.append(endpoint)
        if endpoint in self.fail:
            raise self.fail[endpoint]
        return value

    def endorsements(self):
        return self.get("endorsements", [])

    def tracked_mods(self):
        return self.get("tracking", [])

    def files(self, game, mod):
        return self.get(f"files-{mod}", fixture_data())

    def mod_info(self, game, mod):
        return self.get(f"version-{mod}", {"version": "2.0"})


def test_batch_deduplicates_shared_account_reads_and_published_file_relationships():
    client = Client()
    result = nexus_batch.read(client, [("fallout4", 123), ("fallout4", 124), ("fallout4", 123)], installed_files={("fallout4", 123): [1]})
    assert len(client.calls) == 6 and client.calls.count("endorsements") == client.calls.count("tracking") == 1
    assert len(result.states) == 2 and not result.issues
    assert "_updatable_files" in result.states[("fallout4", 123)].__dict__
    assert 1 in result.states[("fallout4", 123)]._replacement_cache
    for state in result.states.values():
        assert state.endorsement == "Undecided" and state.tracked is False
        assert [file.id for file in state.replacements(1)] == [3, 4]


def test_twenty_mod_batch_uses_exactly_forty_two_mocked_requests():
    client = Client()
    result = nexus_batch.read(client, [("fallout4", mod) for mod in range(1, 21)])
    assert len(result.states) == 20 and len(client.calls) == 42


@pytest.mark.parametrize("endpoint", ["endorsements", "tracking", "version-123", "files-123"])
def test_partial_batch_failure_retains_other_successes_and_unknown(endpoint):
    client = Client()
    client.fail[endpoint] = NexusError("fixture offline")
    result = nexus_batch.read(client, [("fallout4", 123), ("fallout4", 124)])
    assert len(result.issues) == 1 and result.versions[("fallout4", 124)] == "2.0"
    assert result.states[("fallout4", 124)] is not None
    if endpoint == "endorsements":
        assert result.states[("fallout4", 123)].endorsement == "Unknown"
    elif endpoint == "tracking":
        assert result.states[("fallout4", 123)].tracked is None
    elif endpoint == "version-123":
        assert result.versions[("fallout4", 123)] is None
    else:
        assert result.states[("fallout4", 123)] is None


@pytest.mark.parametrize("status", [401, 403, 429])
def test_rate_limit_or_auth_failure_stops_further_reads(status):
    client = Client()
    client.fail["tracking"] = NexusError("fixture limited", 600, http_status=status)
    result = nexus_batch.read(client, [("fallout4", 123), ("fallout4", 124)])
    assert client.calls == ["endorsements", "tracking"] and result.retry_after == 600
    assert not any(result.states.values()) and not any(result.versions.values())


@pytest.mark.parametrize("size", [0, 21])
def test_batches_enforce_request_bound(size):
    client = Client()
    with pytest.raises(NexusError):
        nexus_batch.read(client, [("fallout4", index + 1) for index in range(size)])
    assert not client.calls


def test_cancel_stops_batch_before_request_and_midpass():
    client, cancel = Client(), threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        nexus_batch.read(client, [("fallout4", 123)], cancel)
    assert not client.calls
    cancel.clear()
    def progress(*_):
        cancel.set()
    with pytest.raises(Cancelled):
        nexus_batch.read(client, [("fallout4", 123), ("fallout4", 124)], cancel, progress)
    assert "files-124" not in client.calls


def test_unendorse_uses_abstain_endpoint_and_202_is_unconfirmed():
    transport = Transport()
    client = NexusClient("fixture-key", transport=transport)
    client.unendorse("fallout4", 123, "1.2")
    request = transport.requests[-1]
    assert request.full_url.endswith("/fallout4/mods/123/abstain.json")
    assert request.get_method() == "POST" and json.loads(request.data) == {"Version": "1.2"}
    def accepted(request, timeout):
        result = Response(b"{}")
        result.status = 202
        return result
    with pytest.raises(NexusError, match="unconfirmed"):
        NexusClient("fixture-key", transport=accepted).unendorse("fallout4", 123, "1.2")


@pytest.mark.parametrize("identity", [("bad", 123, 456), ("fallout4", True, 456), ("fallout4", 123, 0), ("fallout4", 123, True)])
def test_free_browser_url_rejects_invalid_identities(identity):
    with pytest.raises(NexusError):
        file_page(*identity)


def test_free_browser_url_contains_only_public_file_identity():
    assert file_page("fallout4", 123, 456) == "https://www.nexusmods.com/fallout4/mods/123?tab=files&file_id=456"


@pytest.fixture
def manager(tmp_path):
    m = ManagerController()
    m.library = Library(tmp_path / "library", create=True)
    m.profile_id = m.library.create_profile("Fixture")
    with m.library._db() as db:
        db.execute("INSERT INTO mods(id,name,source,kind,nexus_game,nexus_mod_id,nexus_file_id,version) VALUES ('mod','Fixture','','external','fallout4',123,1,'1.2')")
        db.execute("INSERT INTO entries VALUES (?,'mod',0,1,'')", (m.profile_id,))
    m.refresh()
    m.nexus_client = NexusClient("fixture-key", transport=Transport())
    m.nexus_client.user = {"user_id": 42, "name": "Fixture account", "premium": False}
    yield m
    m.cleanup()


@pytest.mark.parametrize("change", ["account", "target", "file", "version"])
def test_account_review_rejects_stale_context_before_write(manager, change):
    plan = nexus_account.review(manager, "mod", "unendorse")
    if change == "account":
        manager.nexus_client.user["user_id"] = 43
    elif change == "target":
        manager.target_id = "different"
    else:
        row = manager.rows[0]
        updated = replace(row, **({"nexus_file_id": 456} if change == "file" else {"version": "2"}))
        manager.rows, manager.rows_by_id = [updated], {updated.id: updated}
    with pytest.raises(NexusError, match="changed"):
        nexus_account.validate(manager, plan)
    assert not manager.nexus_client._transport.requests


def test_controller_batch_clears_stale_results_and_discards_changed_session(manager, monkeypatch):
    result = nexus_batch.read(Client(), [("fallout4", 123)])
    manager.nexus_states.update(result.states)
    pending = []
    monkeypatch.setattr(manager, "submit", lambda label, work, complete: pending.append((work, complete)))
    manager.check_nexus_batch()
    assert not manager.nexus_states
    manager.nexus_client.user["user_id"] = 43
    pending[0][1](result)
    assert not manager.nexus_states and "discarded" in manager.status


def test_target_file_binding_is_scoped_and_invalidates_on_shared_reassociation(context):  # noqa: F811
    library, profile, _, ids, source = context
    with library._db() as db:
        db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=123,nexus_file_id=1 WHERE id='mod'")
    plan = target_mod_sources.review(library, ids[0], profile, "mod", nexus_file_id=3, version="3")
    target_mod_sources.apply(library, plan)
    assert TargetLibrary(library, ids[0]).profile_snapshot(profile)[1][0].nexus_file_id == 3
    assert TargetLibrary(library, ids[1]).profile_snapshot(profile)[1][0].nexus_file_id == 1
    target_mod_sources.set_enabled(library, ids[0], profile, ["mod"], False)
    assert TargetLibrary(library, ids[0]).profile_snapshot(profile)[1][0].nexus_file_id == 3
    with library._db() as db:
        db.execute("UPDATE mods SET nexus_mod_id=124 WHERE id='mod'")
    assert TargetLibrary(library, ids[0]).profile_snapshot(profile)[1][0].nexus_file_id == 0
    assert (source / "file.txt").read_bytes() == b"shared"


@pytest.mark.parametrize("file_id", [None, 0, 3])
def test_variant_file_id_never_inherits_shared_source(context, tmp_path, file_id):  # noqa: F811
    library, profile, _, ids, _ = context
    with library._db() as db:
        db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=123,nexus_file_id=1 WHERE id='mod'")
    root = tmp_path / "variant"
    root.mkdir()
    (root / "file.txt").write_bytes(b"variant")
    plan = target_mod_sources.review(library, ids[0], profile, "mod", source=str(root), nexus_file_id=file_id)
    target_mod_sources.apply(library, plan, accept_identity=True)
    assert TargetLibrary(library, ids[0]).profile_snapshot(profile)[1][0].nexus_file_id == (file_id or 0)
    assert library.rows(profile)[0].nexus_file_id == 1


@pytest.mark.parametrize("invalid", [True, -1, 10**12, "123"])
def test_invalid_target_file_identity_is_rejected(context, invalid):  # noqa: F811
    library, profile, _, ids, _ = context
    with pytest.raises(ManagerError):
        target_mod_sources.review(library, ids[0], profile, "mod", nexus_file_id=invalid)


@pytest.mark.parametrize("outcome", ["complete", "cancel", "failed", "cancel-at-commit"])
def test_downloaded_reference_update_routes_to_owned_layer_preserving_source(imported, outcome, monkeypatch):  # noqa: F811
    manager, _ = imported
    row = next(row for row in manager.rows if row.kind == "external")
    with manager.library._db() as db:
        db.execute("UPDATE mods SET nexus_game='fallout4',nexus_mod_id=123,nexus_file_id=1 WHERE id=?", (row.id,))
    manager.refresh()
    path = owned_download(manager.library)
    review = capture(manager.context_library, manager.profile_id, path)
    candidate = next(item for item in review.candidates if item.id == row.id)
    assert candidate.overlay_eligible and not candidate.eligible
    with pytest.raises(ManagerError):
        destination(review, row.id)
    assert destination(review, row.id, overlay=True) == row.id
    source = {p: p.read_bytes() for p in Path(row.source).rglob("*") if p.is_file()}
    before = manager.library.rows(manager.profile_id)
    manager.prepare_install(path, download_review=review, overlay_id=row.id)
    finish(manager)
    assert manager.prepared and manager.prepared.overlay_review
    if outcome == "cancel":
        manager.discard_install()
        assert manager.library.rows(manager.profile_id) == before
    else:
        manager.review_install()
        finish(manager)
        if outcome in {"failed", "cancel-at-commit"}:
            def checkpoint(name):
                if name == "before-commit":
                    raise OSError("fixture failure") if outcome == "failed" else Cancelled()
            monkeypatch.setattr(manager.library.owned, "_checkpoint", checkpoint)
        manager.install("Downloaded additive update")
        if outcome in {"failed", "cancel-at-commit"}:
            with pytest.raises(OSError if outcome == "failed" else Cancelled):
                manager._future.result(timeout=10)
            manager.poll()
        else:
            finish(manager)
        if outcome == "complete":
            added = next(item for item in manager.rows if item.kind == "owned")
            assert added.nexus_file_id == 2
            assert Path(added.source).joinpath("file.txt").read_bytes() == b"replacement"
        else:
            assert manager.library.rows(manager.profile_id) == before
            assert not list((manager.library.root / ".staging").iterdir())
    assert {p: p.read_bytes() for p in source} == source
