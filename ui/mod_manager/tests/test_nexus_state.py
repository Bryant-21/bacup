"""Nexus wire contracts and account writes use in-memory HTTP fixtures only."""
import json
import threading
from urllib.parse import urlsplit

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.nexus import NexusClient, NexusError
from creation_lib.mod_manager.nexus_state import inspect
from creation_lib.mod_manager.paths import Cancelled
from .test_controller import finish
from .test_nexus import Response


def fixture_data():
    return {"files": [{"file_id": identifier, "file_name": f"Fixture-{identifier}.zip", "version": str(identifier), "category_name": category}
                      for identifier, category in ((1, "Old version"), (2, "Archived"), (3, "Main"), (4, "Optional"))],
            "file_updates": [{"old_file_id": old, "new_file_id": new} for old, new in ((1, 2), (2, 3), (1, 4))]}


class Transport:
    def __init__(self):
        self.requests = []
        self.data = fixture_data()
        self.endorsements = [{"domain_name": "fallout4", "mod_id": 123, "status": "Endorsed"}]
        self.tracked = [{"domain_name": "fallout4", "mod_id": 123}]

    def __call__(self, request, timeout):
        self.requests.append(request)
        path = urlsplit(request.full_url).path
        if path.endswith("files.json"):
            data = self.data
        elif path.endswith("endorsements.json"):
            data = self.endorsements
        elif path.endswith("tracked_mods.json") and request.get_method() == "GET":
            data = self.tracked
        else:
            data = {"message": "Fixture accepted"}
        return Response(json.dumps(data).encode())


def test_files_follow_published_update_chain_and_read_dedicated_account_endpoints():
    transport = Transport()
    client = NexusClient("fixture-secret", transport=transport)
    state = inspect(client, "fallout4", 123)
    assert [file.id for file in state.replacements(1)] == [3, 4]
    assert not state.replacements(3) and not state.replacements(999)
    assert state.endorsement == "Endorsed" and state.tracked is True
    assert [urlsplit(request.full_url).path for request in transport.requests] == [
        "/v1/games/fallout4/mods/123/files.json", "/v1/user/endorsements.json", "/v1/user/tracked_mods.json"]
    transport.endorsements, transport.tracked = [], []
    state = inspect(client, "fallout4", 123)
    assert state.endorsement == "Undecided" and state.tracked is False


def test_account_request_contract_uses_json_version_and_identity_without_url_credentials():
    transport = Transport()
    client = NexusClient("fixture-secret", transport=transport)
    client.endorse("fallout4", 123, "1.2")
    client.track("fallout4", 123)
    client.track("fallout4", 123, False)
    assert [request.get_method() for request in transport.requests] == ["POST", "POST", "DELETE"]
    assert [json.loads(request.data) for request in transport.requests] == [
        {"Version": "1.2"}, {"domain_name": "fallout4", "mod_id": 123}, {"domain_name": "fallout4", "mod_id": 123}]
    assert all(request.get_header("Content-type") == "application/json" for request in transport.requests)
    assert all("fixture-secret" not in request.full_url for request in transport.requests)
    assert all(request.get_header("Apikey") == "fixture-secret" for request in transport.requests)


@pytest.mark.parametrize("invalid", ["cycle", "unsafe", "duplicate", "oversized", "bad-id", "duplicate-endorsement"])
def test_invalid_metadata_and_account_identity_fail_without_update_inference(invalid):
    transport = Transport()
    if invalid == "cycle":
        transport.data["file_updates"].append({"old_file_id": 3, "new_file_id": 1})
    elif invalid == "unsafe":
        transport.data["files"][0]["file_name"] = "../escape.zip"
    elif invalid == "duplicate":
        transport.data["files"].append(transport.data["files"][0])
    elif invalid == "oversized":
        transport.data["files"] *= 2501
    elif invalid == "bad-id":
        transport.data["files"][0]["file_id"] = True
    else:
        transport.endorsements *= 2
    with pytest.raises(ValueError):
        inspect(NexusClient("fixture-secret", transport=transport), "fallout4", 123)


def test_cancel_before_remote_request_and_after_remote_acceptance_have_distinct_results(tmp_path):
    manager = ManagerController()
    manager.library = Library(tmp_path / "library", create=True)
    manager.profile_id = manager.library.create_profile("Fixture")
    with manager.library._db() as db:
        db.execute("INSERT INTO mods(id,name,source,kind,nexus_game,nexus_mod_id,version) VALUES ('fixture','Fixture','','external','fallout4',123,'1.2')")
        db.execute("INSERT INTO entries VALUES (?,'fixture',0,1,'')", (manager.profile_id,))
    manager.refresh()
    client = NexusClient("fixture-secret", transport=Transport())
    client.user = {"user_id": 42, "name": "Fixture", "premium": True}
    manager.nexus_client = client
    started, release = threading.Event(), threading.Event()
    blocker = manager._executor.submit(lambda: (started.set(), release.wait(5)))
    try:
        assert started.wait(5)
        manager.nexus_account_action("fixture", "endorse")
        manager.cancel()
        release.set()
        blocker.result(timeout=5)
        with pytest.raises(Cancelled):
            manager._future.result(timeout=5)
        manager.poll()
        assert not client._transport.requests and manager.job_state == "cancelled"
        def accepted(*args, **kwargs):
            manager._cancel.set()
        client.endorse = accepted
        manager.nexus_account_action("fixture", "endorse")
        finish(manager)
        assert manager.job_state == "completed" and "accepted" in manager.status
        assert "no changes committed" not in manager.status
        assert not manager.nexus_states
        manager.inspect_nexus_mod("fixture")
        finish(manager)
        assert manager.nexus_states
        manager.disconnect_nexus()
        assert not manager.nexus_states and not client._api_key
    finally:
        release.set()
        manager.cleanup()


def test_accepted_but_unconfirmed_http_status_is_reported_for_refresh():
    def transport(request, timeout):
        response = Response(b"{}")
        response.status = 202
        return response
    with pytest.raises(NexusError, match="outcome is unconfirmed"):
        NexusClient("fixture-secret", transport=transport).endorse("fallout4", 123)
