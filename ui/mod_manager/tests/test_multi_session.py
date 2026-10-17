from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace
import json
from pathlib import Path
import threading
import uuid

import pytest

from creation_lib.mod_manager import runtime
from creation_lib.mod_manager.backend import call
from creation_lib.mod_manager.runtime_helper import run
from .test_runtime import fixture as fixture, FakeVFS, request


@pytest.mark.parametrize("project", [False, True])
def test_overlapping_launches_of_one_profile_have_independent_outputs_and_ports(fixture, monkeypatch, project):
    library, profile_id, executable, game, bundle, targets = fixture
    plan = runtime.prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    if project:
        plan = replace(plan, runtime="modbox-vfs-fo4-x64")
        monkeypatch.setattr("creation_lib.mod_manager.native_builder.validate", lambda _: bundle)
    requests = [request(library, plan), request(library, replace(plan, session_id=uuid.uuid4().hex))]
    barrier = threading.Barrier(2, timeout=15)
    adapters = []

    class Overlapping(FakeVFS):
        def configure(self, owner, config, protected=()):
            self.owner, self.config = owner, config

        def start(self, argv, cwd, **kwargs):
            self.environment = kwargs["environment"]
            adapters.append(self)
            barrier.wait()
            assert len(runtime.active_sessions(library.root)) == 2
            barrier.wait()
            return super().start(argv, cwd, **kwargs)

    with ThreadPoolExecutor(max_workers=2) as pool:
        jobs = [pool.submit(run, path, native_factory=Overlapping, wait=lambda _: None) for path in requests]
        assert [job.result(timeout=30) for job in jobs] == [0, 0]
    statuses = [json.loads((path.parent / "status.json").read_text()) for path in requests]
    assert len({status["mcp_port"] for status in statuses}) == 2
    assert {int(adapter.environment["B21_MCP_PORT"]) for adapter in adapters} == {status["mcp_port"] for status in statuses}
    outputs = [{entry[1] for entry in adapter.operations if entry[0] == "directory"} for adapter in adapters]
    assert outputs[0].isdisjoint(outputs[1])
    if project:
        assert {adapter.owner for adapter in adapters} == {path.parent for path in requests}
        assert all(adapter.config.parent == adapter.owner for adapter in adapters)
    for path, status in zip(requests, statuses):
        assert status["output_root"] == str(path.parent / "output")
        assert (path.parent / "output" / "ini" / "Fallout4.ini").is_file()
    assert not (library.root / "runtime" / "profiles" / profile_id).exists()


def test_snapshot_writes_do_not_mutate_the_baseline_or_another_session(tmp_path):
    baseline = tmp_path / "baseline"
    baseline.mkdir()
    (baseline / "save.fos").write_bytes(b"original")
    outputs = [tmp_path / "first", tmp_path / "second"]
    for output in outputs:
        call("snapshot_directory", baseline, output)
    (outputs[0] / "save.fos").write_bytes(b"new save")
    assert (baseline / "save.fos").read_bytes() == b"original"
    assert (outputs[1] / "save.fos").read_bytes() == b"original"


def test_mcp_port_claim_is_exclusive_and_released(tmp_path):
    first = call("PortLease", tmp_path)
    try:
        with pytest.raises(OSError, match="No available MCP port"):
            call("PortLease", tmp_path, first.port)
        second = call("PortLease", tmp_path)
        assert second.port != first.port
        second.close()
        port = first.port
    finally:
        first.close()
    reused = call("PortLease", tmp_path, port)
    reused.close()


def test_manager_cli_uses_shared_backend_without_loading_ui(tmp_path):
    import subprocess
    import sys
    code = (
        "import sys; from click.testing import CliRunner; from cli.main import cli; "
        "r=CliRunner().invoke(cli,['manager','launch','--help']); "
        "assert r.exit_code == 0, r.output; "
        "assert not any(n.startswith('ui.mod_manager') for n in sys.modules)"
    )
    result = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True)
    assert result.returncode == 0, result.stderr


def test_native_session_registry_finds_targets_and_deduplicates_legacy_pointers(tmp_path):
    root_session = tmp_path / "runtime" / "sessions" / "first"
    target_session = tmp_path / ".launch-targets" / ("a" * 32) / "runtime" / "sessions" / "second"
    for path, state in ((root_session, "running"), (target_session, "completed")):
        path.mkdir(parents=True)
        (path / "status.json").write_text(json.dumps({"state": state}))
    profile = tmp_path / "runtime" / "profiles" / "legacy"
    profile.mkdir(parents=True)
    (profile / "active.json").write_text('{"session":"first"}')
    records = call("session_records", tmp_path)
    assert [Path(path) for path, _ in records] == [root_session]
    assert Path(call("session_path", tmp_path, "second")) == target_session
    with pytest.raises(ValueError, match="not found"):
        call("session_path", tmp_path, "missing")
    (profile / "active.json").write_text('{"session":"../escape"}')
    with pytest.raises(ValueError, match="Unsafe"):
        call("session_records", tmp_path)


def test_native_registry_rejects_oversized_status(tmp_path):
    session = tmp_path / "runtime" / "sessions" / "oversized"
    session.mkdir(parents=True)
    (session / "status.json").write_text(" " * 65537)
    with pytest.raises(ValueError, match="safety limit"):
        call("session_records", tmp_path)
