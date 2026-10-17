"""Overlapping generated CDN streams, independent checkpoints and process locks."""
import json
from pathlib import Path
import threading
from urllib.parse import urlsplit
from urllib.error import HTTPError

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.download_queue import DownloadQueue
from creation_lib.mod_manager.download_scheduler import Scheduler
from creation_lib.mod_manager.download_locks import job_lock
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.nexus import NexusClient, parse_nxm
from creation_lib.mod_manager.paths import ManagerError
from .test_nexus import Response, Transport


def link(index):
    return parse_nxm(f"nxm://fallout4/mods/123/files/{index + 1}?key=private-fixture-link&expires=4000000000&user_id=42")


class Wire(Transport):
    def __init__(self, *, fail=False, rate=False):
        super().__init__(b"generated parallel payload" * 100_000, premium=True)
        self.release = threading.Event()
        self.entered = [threading.Event() for _ in range(10)]
        self.lock = threading.Lock()
        self.active = self.maximum = self.calls = 0
        self.fail, self.rate = fail, rate

    def __call__(self, request, timeout):
        if urlsplit(request.full_url).hostname != "files.nexusmods.com":
            if self.rate and "/files/" in urlsplit(request.full_url).path:
                self.requests.append(request)
                raise HTTPError(request.full_url, 429, "fixture", {"Retry-After": "60"}, None)
            return super().__call__(request, timeout)
        assert request.get_header("Apikey") is None
        self.requests.append(request)
        with self.lock:
            index = self.calls
            self.calls += 1
        wire = self
        class Stream(Response):
            def __init__(self):
                super().__init__(wire.payload, {"Content-Length": str(len(wire.payload)), "ETag": '"parallel-v1"'})
                self.started = False
            def read1(self, size=-1):
                if not self.started:
                    self.started = True
                    with wire.lock:
                        wire.active += 1
                        wire.maximum = max(wire.maximum, wire.active)
                    wire.entered[index].set()
                    assert wire.release.wait(10)
                    if wire.fail and index == 0:
                        raise OSError("generated CDN failure containing private-fixture-ticket")
                return super().read1(size)
            def close(self):
                if self.started and not self.closed:
                    with wire.lock:
                        wire.active -= 1
                super().close()
        return Stream()

    def client(self):
        client = NexusClient("private-fixture-api-key", transport=self)
        client.connect()
        return client


def wait_transfers(scheduler):
    for transfer in list(scheduler.transfers.values()):
        transfer.future.exception(timeout=15)
    return scheduler.poll()


@pytest.mark.parametrize("slots", [2, 3, 4])
def test_slots_overlap_independent_streams_and_keep_queued_identities_persistent(tmp_path, monkeypatch, slots):
    library, wire = Library(tmp_path / "library", create=True), Wire()
    original, copies = NexusClient.for_transfer, []
    def copy(client):
        value = original(client)
        copies.append(value)
        return value
    monkeypatch.setattr(NexusClient, "for_transfer", copy)
    client = wire.client()
    scheduler = Scheduler(library, slots)
    try:
        transfers = [scheduler.add(client, link(index)) for index in range(slots + 1)]
        assert all(wire.entered[index].wait(5) for index in range(slots))
        # The enqueue executor persists waiting file identities separately from
        # occupied CDN slots. Flush that executor without delaying a transfer.
        scheduler._enqueue_pool.submit(lambda: None).result(timeout=5)
        assert len(DownloadQueue(library).records()) == slots + 1
        assert wire.maximum == slots and not transfers[-1].future.done()
        with pytest.raises(ManagerError, match="already has"):
            scheduler.add(client, link(0))
        with pytest.raises(ManagerError, match="Another transfer"):
            DownloadQueue(library).run(transfers[0].identifier, client, link(0))
        with pytest.raises(ManagerError, match="Another transfer"):
            DownloadQueue(library).remove(transfers[0].identifier)
        # Downloads do not hold the payload transaction lock during network I/O.
        with library.owned.lock():
            profile = library.create_profile("While downloading")
        assert library.profiles()[0].id == profile
        wire.release.set()
        outcomes = wait_transfers(scheduler)
        assert len(outcomes) == slots + 1 and all(state == "complete" for _, state, _ in outcomes)
        assert all(Path(path).read_bytes() == wire.payload for _, _, path in outcomes)
        assert all(not copied._api_key and copied.user is None for copied in copies)
        assert client._api_key == "private-fixture-api-key"
        persisted = json.dumps(DownloadQueue(library).records())
        assert "private-fixture" not in persisted and "nexusmods.com" not in persisted
    finally:
        wire.release.set()
        scheduler.close()
        client.forget()


def test_one_paused_transfer_does_not_pause_other_stream_and_resumes_locally(tmp_path):
    library, wire = Library(tmp_path / "library", create=True), Wire()
    scheduler, client = Scheduler(library, 2), wire.client()
    try:
        first, second = scheduler.add(client, link(0)), scheduler.add(client, link(1))
        assert wire.entered[0].wait(5) and wire.entered[1].wait(5)
        scheduler.pause(first.identity)
        wire.release.set()
        outcomes = {identity: state for identity, state, _ in wait_transfers(scheduler)}
        assert outcomes[first.identity] == "paused" and outcomes[second.identity] == "complete"
        job = DownloadQueue(library).get(first.identifier)
        assert job["state"] == "paused" and job["record"]["bytes"] > 0
        scheduler.add(client, link(0), first.identifier)
        outcomes = wait_transfers(scheduler)
        assert outcomes[0][1] == "complete" and len(library.downloads()) == 2
    finally:
        wire.release.set()
        scheduler.close()


def test_failed_stream_is_independent_and_failure_messages_redact_transport_details(tmp_path):
    library, wire = Library(tmp_path / "library", create=True), Wire(fail=True)
    scheduler, client = Scheduler(library, 2), wire.client()
    try:
        scheduler.add(client, link(0))
        scheduler.add(client, link(1))
        assert wire.entered[0].wait(5) and wire.entered[1].wait(5)
        wire.release.set()
        outcomes = wait_transfers(scheduler)
        assert sorted(state for _, state, _ in outcomes) == ["complete", "failed"]
        assert all("private-fixture" not in detail for _, _, detail in outcomes)
        assert sorted(job["state"] for job in DownloadQueue(library).records()) == ["complete", "failed"]
    finally:
        wire.release.set()
        scheduler.close()


def test_rate_limit_blocks_other_metadata_requests_and_retains_jobs(tmp_path):
    library, wire = Library(tmp_path / "library", create=True), Wire(rate=True)
    scheduler, client = Scheduler(library, 2), wire.client()
    try:
        scheduler.add(client, link(0))
        scheduler.add(client, link(1))
        outcomes = wait_transfers(scheduler)
        assert all(state == "failed" for _, state, _ in outcomes)
        assert len([r for r in wire.requests if "/files/" in urlsplit(r.full_url).path]) == 1
        assert len(DownloadQueue(library).records()) == 2 and not library.downloads()
    finally:
        scheduler.close()


def test_controller_background_completion_preserves_open_reviews_and_library_account_guards(tmp_path):
    wire, m = Wire(), ManagerController()
    try:
        m.library = Library(tmp_path / "library", create=True)
        m.nexus_client = wire.client()
        m.set_download_slots(2)
        m.accept_nxm(link(0))
        m.accept_nxm(link(1))
        m.download_nxm(link(0).identity)
        m.download_nxm(link(1).identity)
        assert wire.entered[0].wait(5) and wire.entered[1].wait(5)
        assert m.downloads_active and not m.busy
        sentinel = object()
        m.field_review = sentinel
        for operation in (lambda: m.open_library(tmp_path / "other", create=True), m.disconnect_nexus,
                          lambda: m.connect_nexus("other-key"), lambda: m.set_download_slots(3), m.start_launch):
            with pytest.raises(ManagerError, match="Pause background"):
                operation()
        wire.release.set()
        for transfer in list(m._download_scheduler.transfers.values()):
            transfer.future.result(timeout=15)
        m.poll()
        assert not m.downloads_active and len(m.download_records) == 2 and not m.pending_links
        assert all(job["state"] == "complete" for job in m.download_jobs)
        assert m.field_review is sentinel and not (tmp_path / "other").exists()
        m.open_library(tmp_path / "other", create=True)
        m._future.result(timeout=10)
        m.poll()
        m.accept_nxm(link(2))
        m.download_nxm(link(2).identity)
        for transfer in list(m._download_scheduler.transfers.values()):
            transfer.future.result(timeout=15)
        m.poll()
        assert m._download_scheduler.library.root == m.library.root and len(m.download_records) == 1
    finally:
        wire.release.set()
        m.cleanup()


def test_job_lock_reparse_invalid_identifier_and_runtime_guards(tmp_path, monkeypatch):
    library = Library(tmp_path / "library", create=True)
    for invalid in ("../outside", "not-an-id"):
        with pytest.raises(ManagerError):
            with job_lock(library, invalid):
                pytest.fail("Invalid lock entered")
    monkeypatch.setattr("creation_lib.mod_manager.runtime.active_sessions", lambda _: [("fixture", {"state": "running"})])
    with pytest.raises(ManagerError, match="runtime"):
        with job_lock(library, "a" * 32):
            pytest.fail("Runtime lock entered")


def test_close_checkpoints_running_and_waiting_files_for_restart(tmp_path):
    library, wire = Library(tmp_path / "library", create=True), Wire()
    scheduler, client = Scheduler(library, 2), wire.client()
    transfers = [scheduler.add(client, link(index)) for index in range(5)]
    try:
        assert wire.entered[0].wait(5) and wire.entered[1].wait(5)
        scheduler._enqueue_pool.submit(lambda: None).result(timeout=5)
        scheduler.pause()
        wire.release.set()
        scheduler.close()
        assert all(transfer.future.done() for transfer in transfers)
        reopened = Library(library.root)
        jobs = DownloadQueue(reopened).records()
        assert len(jobs) == 5 and all(job["state"] == "paused" for job in jobs)
        assert wire.calls == 2 and not reopened.downloads()
        resume = Scheduler(reopened, 2)
        try:
            resume.add(client, link(4), transfers[4].identifier)
            assert wait_transfers(resume)[0][1] == "complete"
        finally:
            resume.close()
    finally:
        wire.release.set()
        scheduler.close()


def test_download_lock_rejects_reparse_directory_without_touching_target(tmp_path):
    from .test_mo2 import junction
    library = Library(tmp_path / "library", create=True)
    outside = tmp_path / "outside"
    outside.mkdir()
    (outside / "sentinel").write_bytes(b"retained fixture")
    junction(library.root / "downloads" / ".locks", outside)
    with pytest.raises(ManagerError, match="reparse|Links"):
        with job_lock(library, "a" * 32):
            pytest.fail("Reparse directory was accepted")
    assert sorted(path.name for path in outside.iterdir()) == ["sentinel"]


def test_other_process_cannot_lock_same_download(tmp_path):
    import os
    import subprocess
    library = Library(tmp_path / "library", create=True)
    code = "import sys\nfrom creation_lib.mod_manager.library import Library\nfrom creation_lib.mod_manager.download_locks import job_lock\nwith job_lock(Library(sys.argv[1]), sys.argv[2]):\n print('locked', flush=True)\n sys.stdin.readline()\n"
    child = subprocess.Popen(["uv", "run", "--no-sync", "python", "-c", code, str(library.root), "a" * 32],
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                             creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    try:
        assert child.stdout.readline().strip() == "locked"
        with pytest.raises(ManagerError, match="Another transfer"):
            with job_lock(library, "a" * 32):
                pytest.fail("Another process's lock was ignored")
        # Independent identities remain usable during the held lock.
        with job_lock(library, "b" * 32):
            pass
        child.communicate("release\n", timeout=10)
        assert child.returncode == 0
        with job_lock(library, "a" * 32):
            pass
    finally:
        if child.poll() is None:
            child.terminate()
            child.communicate(timeout=10)


def test_concurrent_files_keep_distinct_payload_metadata_and_hashes(tmp_path):
    import hashlib
    barrier = threading.Barrier(2)
    payloads = {1: b"first generated file" * 90_000, 2: b"second generated file" * 90_000}
    class Distinct(Transport):
        def __call__(self, request, timeout):
            url = urlsplit(request.full_url)
            if url.hostname == "files.nexusmods.com":
                identifier = int(Path(url.path).stem.removeprefix("File"))
                class Stream(Response):
                    first = True
                    def read1(self, size=-1):
                        if self.first:
                            self.first = False
                            barrier.wait(timeout=5)
                        return super().read1(size)
                return Stream(payloads[identifier], {"Content-Length": str(len(payloads[identifier]))})
            if "/files/" in url.path:
                identifier = int(url.path.split("/files/")[1].split("/")[0].removesuffix(".json"))
                data = [{"URI": f"https://files.nexusmods.com/File{identifier}.zip"}] if url.path.endswith("download_link.json") else {"file_name": f"File{identifier}.zip", "version": str(identifier), "md5": hashlib.md5(payloads[identifier], usedforsecurity=False).hexdigest()}
                return Response(json.dumps(data).encode())
            return super().__call__(request, timeout)
    client = NexusClient("fixture-key", transport=Distinct(premium=True))
    client.connect()
    library, scheduler = Library(tmp_path / "library", create=True), None
    try:
        scheduler = Scheduler(library, 2)
        scheduler.add(client, link(0))
        scheduler.add(client, link(1))
        assert all(state == "complete" for _, state, _ in wait_transfers(scheduler))
        for row in library.downloads():
            assert Path(row["path"]).read_bytes() == payloads[row["file_id"]]
            assert row["checksum"] == hashlib.sha256(payloads[row["file_id"]]).hexdigest()
            assert row["version"] == str(row["file_id"])
    finally:
        if scheduler:
            scheduler.close()
        client.forget()
