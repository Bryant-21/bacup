"""PID reuse, fail-closed recovery and retained-handle termination fixtures."""
import ctypes as ct
from ctypes import wintypes as w
import json
import os
import subprocess
import sys
from types import SimpleNamespace

import pytest

from creation_lib.mod_manager import process_identity as identity
from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import ManagerError


def token(pid=42, created=123456789):
    return {"pid": pid, "created": str(created)}


def api(*, created=123456789, pid=42, state=258, error=0, times=True):
    closed = []
    def opened(access, inherit, number):
        assert access == 0x101000 and not inherit and number == 42
        ct.set_last_error(error)
        return None if error else 909
    def timing(handle, creation, *args):
        assert handle == 909
        value = ct.cast(creation, ct.POINTER(w.FILETIME)).contents
        value.dwLowDateTime, value.dwHighDateTime = created & 0xFFFFFFFF, created >> 32
        return times
    return SimpleNamespace(OpenProcess=opened, GetProcessId=lambda handle: pid, GetProcessTimes=timing,
                           WaitForSingleObject=lambda handle, timeout: state,
                           CloseHandle=lambda handle: closed.append(handle)), closed


@pytest.mark.parametrize("options,expected", [({}, "alive"), ({"created": 123456790}, "reused"),
    ({"state": 0}, "exited"), ({"error": 87}, "exited"), ({"error": 5}, "unverified"),
    ({"state": 0xFFFFFFFF}, "unverified"), ({"times": False}, "unverified"), ({"pid": 43}, "unverified")])
def test_same_retained_handle_checks_creation_then_liveness_and_closes(options, expected):
    kernel, closed = api(**options)
    assert identity.process_state(token(), kernel=kernel) == expected
    assert closed == ([] if options.get("error") else [909])


@pytest.mark.parametrize("value", [None, {"pid": 42}, {"pid": True, "created": "123"},
    {"pid": -1, "created": "123"}, {"pid": 2**32, "created": "123"},
    {"pid": 42, "created": "00123"}, {"pid": 42, "created": "0"},
    {"pid": 42, "created": str(2**64)}, {"pid": 42, "created": "\u0661\u0662\u0663"},
    {"pid": 42, "created": "123456789", "extra": "metadata"}])
def test_invalid_or_legacy_identity_is_not_adopted(value):
    assert not identity.valid_identity(value)
    assert identity.process_state(value, kernel=SimpleNamespace()) == "unverified"


def test_termination_uses_only_matching_retained_handle_never_pid_lookup(monkeypatch):
    calls = []
    process = SimpleNamespace(pid=42, _handle=909, poll=lambda: None, terminate=lambda: calls.append("handle"))
    monkeypatch.setattr(identity, "from_handle", lambda pid, handle: token())
    with pytest.raises(ManagerError, match="termination refused"):
        identity.terminate_owned(process, token(created=123456790))
    assert not calls
    assert identity.terminate_owned(process, token()) and calls == ["handle"]
    process.poll = lambda: 0
    assert not identity.terminate_owned(process, token())


@pytest.mark.parametrize("condition", ["alive", "reused", "exited", "unverified", "legacy", "mismatched"])
def test_reopen_recovery_uses_creation_identity_and_never_terminates(tmp_path, monkeypatch, condition):
    library = Library(tmp_path / "library", create=True)
    profile_id = library.create_profile("Fixture")
    session = library.root / "runtime" / "sessions" / "identity-fixture"
    session.mkdir(parents=True)
    profile = library.root / "runtime" / "profiles" / profile_id
    profile.mkdir(parents=True)
    (profile / "active.json").write_text(json.dumps({"session": session.name}))
    status = {"state": "running", "helper_pid": 42, "helper_identity": token(), "profile_id": profile_id}
    if condition == "legacy":
        del status["helper_identity"]
    elif condition == "mismatched":
        status["helper_identity"] = token(pid=43)
    (session / "status.json").write_text(json.dumps(status))
    monkeypatch.setattr(identity, "process_state", lambda value: condition)
    monkeypatch.setattr(identity, "terminate_owned", lambda *args: pytest.fail("recovery terminated a process"))
    controller = ManagerController()
    try:
        controller.library = Library(library.root)
        controller.refresh()
        blocked = condition not in {"reused", "exited"}
        assert controller.runtime_active == blocked
        if blocked:
            with pytest.raises(ManagerError, match="running native runtime"):
                controller.library.create_profile("Blocked")
            if condition != "alive":
                assert controller.runtime_status["state"] == "identity-unverified"
        else:
            assert controller.library.create_profile("Old runtime ended")
    finally:
        controller.cleanup()


def test_child_identity_completeness_and_query_failure_remain_locked(monkeypatch):
    monkeypatch.setattr(identity, "process_state", lambda value: "exited")
    status = {"helper_pid": 42, "helper_identity": token(), "children": [50]}
    assert identity.session_liveness(status) == "unverified"
    status["child_identities"] = [token(pid=50)]
    assert identity.session_liveness(status) == "exited"
    status["identity_unverified"] = True
    assert identity.session_liveness(status) == "unverified"


def test_duplicate_or_extra_child_identity_records_are_unverified(monkeypatch):
    monkeypatch.setattr(identity, "process_state", lambda value: "exited")
    status = {"helper_pid": 42, "helper_identity": token(), "children": [50, 50],
              "child_identities": [token(pid=50), token(pid=50, created=123456790)]}
    assert identity.session_liveness(status) == "unverified"
    status["children"] = []
    assert identity.session_liveness(status) == "unverified"


@pytest.mark.skipif(os.name != "nt", reason="Windows creation-time handles")
def test_real_disposable_process_wrong_creation_exit_and_restart(tmp_path):
    def spawn():
        return subprocess.Popen([getattr(sys, "_base_executable", None) or sys.executable, "-c", "import time; time.sleep(20)"], cwd=tmp_path,
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                creationflags=subprocess.CREATE_NO_WINDOW)
    first = spawn()
    first_identity = identity.from_handle(first.pid, first._handle)
    try:
        assert identity.process_state(first_identity) == "alive"
        wrong = {**first_identity, "created": str(int(first_identity["created"]) + 1)}
        assert identity.process_state(wrong) == "reused"
        with pytest.raises(ManagerError, match="termination refused"):
            identity.terminate_owned(first, wrong)
        assert first.poll() is None
    finally:
        identity.terminate_owned(first, first_identity)
        first.wait(timeout=10)
    assert identity.process_state(first_identity) in {"exited", "reused"}
    second = spawn()
    second_identity = identity.from_handle(second.pid, second._handle)
    try:
        assert second_identity != first_identity and identity.process_state(second_identity) == "alive"
        assert identity.process_state(first_identity) in {"exited", "reused"}
    finally:
        identity.terminate_owned(second, second_identity)
        second.wait(timeout=10)
