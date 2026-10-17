"""Real disposable Windows manager processes and synthetic browser links only."""
from dataclasses import replace
import json
from multiprocessing.connection import Client
import os
import queue
import subprocess
import sys
import threading
import uuid

import pytest

from creation_lib.mod_manager import nxm_instances as routes
from creation_lib.mod_manager.nexus import parse_nxm
from .test_nexus import LINK
from .test_nexus_bulk import manager  # noqa: F401


class Actor:
    def __init__(self, root, name, game="fallout4"):
        self.output = queue.Queue()
        self.process = subprocess.Popen([sys.executable, "-m", "ui.mod_manager.tests.nxm_process_fixture", "--directory", str(root / "ipc"),
                                         "--library", str(root / name), "--name", name, "--game", game],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
                                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        def read():
            for line in self.process.stdout:
                self.output.put(json.loads(line))
        self.thread = threading.Thread(target=read, daemon=True)
        self.thread.start()
        self.ready = self.output.get(timeout=15)
        assert self.ready["ready"]

    def command(self, command, **values):
        self.process.stdin.write(json.dumps({"command": command, **values}) + "\n")
        self.process.stdin.flush()
        return self.output.get(timeout=10)

    def close(self):
        if self.process.poll() is None:
            try:
                self.command("close")
                self.process.wait(timeout=5)
            except (OSError, queue.Empty, subprocess.TimeoutExpired):
                self.process.terminate()  # retained handle to this owned disposable process
                self.process.wait(timeout=5)
        for stream in (self.process.stdin, self.process.stdout):
            stream.close()


@pytest.fixture
def actors(tmp_path):
    created = []
    def create(name, game="fallout4"):
        actor = Actor(tmp_path, name, game)
        created.append(actor)
        return actor
    yield create, tmp_path / "ipc"
    for actor in reversed(created):
        actor.close()


def request():
    return routes.Request(uuid.uuid4().hex, parse_nxm(LINK))


def test_two_matching_processes_do_not_forward_to_arbitrary_window(actors):
    create, directory = actors
    first, second = create("first"), create("second")
    destinations = routes.discover(directory, "fallout4")
    assert len(destinations) == 2 and destinations[0].instance_id != destinations[1].instance_id
    assert not routes.forward_link(directory, LINK)
    pending = routes.take_startup(parse_nxm(LINK))
    assert not pending.uncertain
    assert first.command("status")["queued"] == second.command("status")["queued"] == 0
    destination = next(d for d in destinations if d.instance_id == second.ready["instance_id"])
    response = routes.send(destination, pending)
    assert response["state"] == "accepted"
    assert first.command("drain")["pending"] == []
    result = second.command("drain")
    assert result["pending"] == [["fallout4", 123, 456]] and result["downloads"] == 0
    assert result["bindings"][0][0] == destination.context["library_id"]


def test_different_games_forward_only_to_matching_process(actors):
    create, directory = actors
    other, matching = create("skyrim", "skyrim"), create("fo4")
    assert routes.forward_link(directory, LINK)
    assert matching.command("drain")["pending"] == [["fallout4", 123, 456]]
    assert other.command("drain")["pending"] == []


def test_context_change_rejects_stale_selection_without_queuing(actors):
    create, directory = actors
    actor = create("fixture")
    destination = routes.discover(directory, "fallout4")[0]
    actor.command("switch", game="skyrim")
    assert routes.send(destination, request())["state"] == "stale"
    assert actor.command("drain")["pending"] == []
    assert not routes.discover(directory, "fallout4")


def test_replayed_delivery_id_across_instances_queues_exactly_once(actors):
    create, directory = actors
    first, second = create("first"), create("second")
    destinations, item = routes.discover(directory), request()
    a = routes.send(destinations[0], item)
    b = routes.send(destinations[1], item)
    assert a["instance_id"] == b["instance_id"]
    results = [actor.command("drain") for actor in (first, second)]
    assert sum(len(result["pending"]) for result in results) == 1
    assert sum(result["downloads"] for result in results) == 0


def test_lost_ack_retry_retains_delivery_id_and_no_duplicate(actors):
    create, directory = actors
    actor = create("fixture")
    destination, item = routes.discover(directory)[0], request()
    with Client(destination.address, family="AF_PIPE") as connection:
        connection.send_bytes(json.dumps({"command": "deliver", "id": item.id, "context": destination.context, "url": LINK}).encode())
        assert connection.poll(3)  # deliberately close without reading acknowledgement
    response = routes.send(destination, item)
    assert response["state"] == "accepted"
    assert actor.command("drain")["pending"] == [["fallout4", 123, 456]]
    assert routes.send(destination, item, status_only=True)["state"] == "delivered"
    assert actor.command("status")["queued"] == 0


def test_startup_uncertainty_keeps_original_id_and_destination(actors, monkeypatch):
    create, directory = actors
    actor = create("fixture")
    original = routes.send
    def uncertain(destination, item):
        assert original(destination, item)["state"] == "accepted"
        return {"state": "uncertain"}
    monkeypatch.setattr(routes, "send", uncertain)
    assert not routes.forward_link(directory, LINK)
    item = routes.take_startup(parse_nxm(LINK))
    assert item.uncertain and item.destination.instance_id == actor.ready["instance_id"]
    assert original(item.destination, item, status_only=True)["state"] == "accepted"
    assert actor.command("drain")["pending"] == [["fallout4", 123, 456]]


def test_crashed_process_record_is_ignored_and_pruned_by_creation_identity(actors):
    create, directory = actors
    actor = create("fixture")
    path = directory / "nxm-instances" / (actor.ready["instance_id"] + ".json")
    actor.process.terminate()
    actor.process.wait(timeout=5)
    assert path.exists() and not routes.discover(directory)
    assert not path.exists()


def test_wrong_process_creation_identity_sends_no_link(actors):
    create, directory = actors
    actor = create("fixture")
    destination = routes.discover(directory)[0]
    wrong = replace(destination, process={**destination.process, "created": str(int(destination.process["created"]) + 1)})
    assert routes.send(wrong, request())["state"] == "uncertain"
    assert actor.command("status")["queued"] == 0


def test_closed_instance_does_not_receive_or_start_fallback_download(actors):
    create, directory = actors
    actor = create("fixture")
    destination = routes.discover(directory)[0]
    actor.close()
    assert not routes.discover(directory)
    assert routes.send(destination, request())["state"] == "uncertain"


def test_discovery_and_receipts_persist_no_nexus_or_ipc_session_secret(actors):
    create, directory = actors
    actor = create("fixture")
    assert routes.forward_link(directory, LINK)
    actor.command("drain")
    for path in directory.rglob("*.json"):
        text = path.read_text()
        assert "fixture-key" not in text and "expires" not in text and "user_id" not in text and "auth" not in text and "nxm://" not in text
        assert "apikey" not in text.casefold()


@pytest.mark.parametrize("reply", [None, [], {"state": "accepted"}, {"state": "unexpected"}, {"state": "unknown", "url": LINK}])
def test_malformed_pipe_response_remains_uncertain_without_queueing(tmp_path, monkeypatch, reply):
    root = tmp_path / "library"
    root.mkdir()
    value = {**routes.EMPTY, "library_id": "fixture", "library_root": str(root), "profile_id": "fixture", "profile_name": "Fixture", "game": "fallout4", "ready": True}
    inbox = routes.Inbox(tmp_path / "app", context=value)
    try:
        monkeypatch.setattr(inbox, "_dispatch", lambda data: reply)
        assert routes.send(inbox.destination, request()) == {"state": "uncertain"}
        assert inbox.requests.empty()
    finally:
        inbox.close()


def test_receipt_write_failure_keeps_already_queued_link_without_rerouting(manager, tmp_path, monkeypatch):  # noqa: F811
    from types import SimpleNamespace
    from creation_lib.mod_manager import nxm_routing
    inbox = routes.Inbox(tmp_path / "app", context=nxm_routing.context(manager))
    try:
        assert routes.send(inbox.destination, request())["state"] == "accepted"
        item = inbox.requests.get(timeout=3)
        def fail(*args):
            raise OSError("disposable receipt write failure")
        monkeypatch.setattr(inbox, "mark", fail)
        workspace = SimpleNamespace(_link_inbox=inbox, manager=manager, nxm_router=SimpleNamespace(open=fail))
        nxm_routing.receive(workspace, item)
        assert len(manager.pending_links) == 1 and "queued here" in manager.status
        assert not manager.library.downloads() and workspace._tab == "Nexus"
        assert routes.send(inbox.destination, item, status_only=True)["state"] == "accepted"
    finally:
        inbox.close()


@pytest.mark.parametrize("invalid", ["bad-json", "wrong-format", "relative-library", "oversized"])
def test_invalid_discovery_records_are_unavailable(actors, invalid):
    create, directory = actors
    actor = create("fixture")
    path = directory / "nxm-instances" / (actor.ready["instance_id"] + ".json")
    data = json.loads(path.read_text())
    if invalid == "bad-json":
        path.write_text("{")
    elif invalid == "oversized":
        path.write_text("x" * 16385)
    else:
        if invalid == "wrong-format":
            data["format"] = 99
        else:
            data["context"]["library_root"] = "relative"
        path.write_text(json.dumps(data))
    assert not routes.discover(directory)
