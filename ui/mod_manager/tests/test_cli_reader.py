"""Real Windows process/CLI probes using only generated disposable inputs."""
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time
from types import SimpleNamespace

import pytest

from creation_lib.mod_manager.cli_reader import run_json
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.process_identity import process_state
from creation_lib.mod_manager.read_process import ReadProcess
from creation_lib.mod_manager.plugin_state import modkit_metadata


pytestmark = pytest.mark.skipif(os.name != "nt", reason="Windows process-tree supervision")


def fixture_reader(monkeypatch, script):
    arguments = []
    def create(argv, output, errors):
        arguments.append(argv)
        return ReadProcess([sys.executable, "-c", script], output, errors)
    monkeypatch.setattr("creation_lib.mod_manager.cli_reader.shutil.which", lambda name: str(Path(sys.executable)))
    monkeypatch.setattr("creation_lib.mod_manager.cli_reader.ReadProcess", create)
    return arguments


def test_reader_json_global_arguments_and_read_only_allowlist(monkeypatch):
    arguments = fixture_reader(monkeypatch, 'import json; print(json.dumps({"fixture": True}))')
    assert run_json(("esp", "header", "generated.esp"), game="fo4") == {"fixture": True}
    assert arguments[0][1:] == ["--format", "json", "--game", "fo4", "esp", "header", "generated.esp"]
    for command in (("esp", "new", "generated.esp"), ("archive", "extract-all", "fixture.ba2"), ("esp", "header", "fixture", "--output", "outside")):
        with pytest.raises(ManagerError, match="read-only"):
            run_json(command)
    assert len(arguments) == 1


@pytest.mark.parametrize("mode", ["cancel", "timeout", "overflow"])
def test_reader_reaps_redirector_descendants_and_bounds_output(monkeypatch, tmp_path, mode):
    marker = tmp_path / "identities.json"
    child = "import sys,time;\nwhile True:\n sys.stdout.write('x'*65536); sys.stdout.flush(); time.sleep(.01)" if mode == "overflow" else "import time; time.sleep(60)"
    script = ("import json,subprocess,sys,time; from creation_lib.mod_manager.process_identity import from_handle,current_identity; "
              f"child=subprocess.Popen([sys.executable,'-c',{child!r}]); "
              f"from pathlib import Path; marker=Path({str(marker)!r}); "
              "temporary=marker.with_suffix('.tmp'); temporary.write_text(json.dumps([current_identity(),from_handle(child.pid,child._handle)])); temporary.replace(marker); "
              "time.sleep(60)")
    fixture_reader(monkeypatch, script)
    cancel = threading.Event()
    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
        future = pool.submit(run_json, ("archive", "list", "generated.ba2"), cancel=cancel, timeout=2, max_bytes=1024)
        deadline = time.monotonic() + 3
        while not marker.is_file() and time.monotonic() < deadline:
            time.sleep(.01)
        assert marker.is_file(), "Generated redirector failed to start its child."
        identities = json.loads(marker.read_text())
        if mode == "cancel":
            cancel.set()
        expected = Cancelled if mode == "cancel" else ManagerError
        with pytest.raises(expected, match="cancelled" if mode == "cancel" else "safety limit" if mode == "overflow" else "time limit"):
            future.result(timeout=5)
    deadline = time.monotonic() + 2
    while any(process_state(identity) == "alive" for identity in identities) and time.monotonic() < deadline:
        time.sleep(.01)
    assert all(process_state(identity) == "exited" for identity in identities)


def test_reader_waits_for_successful_descendant_and_rejects_bad_json(monkeypatch):
    fixture_reader(monkeypatch, "import subprocess,sys; subprocess.Popen([sys.executable,'-c','import time; time.sleep(.1); print(123)'])")
    assert run_json(("esp", "header", "generated.esp")) == 123
    fixture_reader(monkeypatch, "print('not JSON')")
    with pytest.raises(ManagerError, match="invalid JSON"):
        run_json(("esp", "header", "generated.esp"))


def test_supported_cli_generated_header_and_masters_do_not_change_payload(tmp_path):
    executable = shutil.which("modkit.exe") or shutil.which("modkit")
    if not executable:
        pytest.skip("Supported modkit CLI is unavailable")
    path = tmp_path / "Generated.esp"
    result = subprocess.run([executable, "--game", "fo4", "--format", "json", "esp", "new", str(path), "--no-masters"], capture_output=True, timeout=30)
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    before = hashlib.sha256(path.read_bytes()).hexdigest()
    header, masters = modkit_metadata(path, "fo4")
    assert isinstance(header["flags"], dict) and masters == ()
    assert hashlib.sha256(path.read_bytes()).hexdigest() == before


@pytest.mark.parametrize("mode", ["job-close", "timeout", "wait-failed", "process-close"])
def test_failed_cleanup_retains_handle_ownership_and_reports_failure(mode):
    reader = ReadProcess.__new__(ReadProcess)
    reader.job, reader.process = 11, 12
    closed = []
    def close(handle):
        closed.append(handle)
        return not (mode == "job-close" and handle == 11 or mode == "process-close" and handle == 12)
    reader.kernel = SimpleNamespace(CloseHandle=close, WaitForSingleObject=lambda handle, duration: 258 if mode == "timeout" else 0xFFFFFFFF if mode == "wait-failed" else 0)
    with pytest.raises(ManagerError, match="cleanup") as caught:
        reader.close()
    assert caught.value.reader is reader and reader.process == 12
    assert reader.job == (11 if mode == "job-close" else None)
    # A later retry retains exact handles rather than opening numeric PIDs.
    reader.kernel = SimpleNamespace(CloseHandle=lambda handle: True, WaitForSingleObject=lambda handle, duration: 0)
    reader.close()
    assert reader.job is reader.process is None
