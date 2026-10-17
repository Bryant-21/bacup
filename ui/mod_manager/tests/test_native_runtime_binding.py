"""Real ctypes structures with DLL-free process/job API mocks."""
import ctypes as ct
import os
from types import SimpleNamespace

import pytest

from creation_lib.mod_manager.native_runtime import NativeVFS, ProcessInfo, StartupInfoEx, JobExtended
from creation_lib.mod_manager.paths import Cancelled, ManagerError


pytestmark = pytest.mark.skipif(os.name != "nt" or ct.sizeof(ct.c_void_p) != 8, reason="x64 Windows ABI")


def mock_native(resume_result=0, inject_result=True):
    events = []
    def initialize(buffer, count, flags, size):
        ct.cast(size, ct.POINTER(ct.c_size_t)).contents.value = 64
        return buffer is not None
    def limits(job, kind, value, size):
        value = ct.cast(value, ct.POINTER(JobExtended)).contents
        assert kind == 9 and size == 144 and value.basic.flags == 0x2000
        return True
    def attributes(attrs, flags, kind, value, size, prev, returned):
        assert kind == 0x2000D and size == 8
        assert ct.cast(value, ct.POINTER(ct.c_void_p)).contents.value == 101
        events.append("job-at-creation")
        return True
    def create(exe, command, pattrs, tattrs, inherit, flags, env, cwd, startup, info):
        assert flags & 4 and flags & 0x80000 and not inherit
        assert ct.cast(startup, ct.POINTER(StartupInfoEx)).contents.startup.cb == 112
        if env:
            block = ct.cast(env, ct.POINTER(ct.c_wchar))
            text, position = "", 0
            while not (block[position] == "\0" and block[position + 1] == "\0"):
                text += block[position]
                position += 1
            events.append(("environment", text))
        out = ct.cast(info, ct.POINTER(ProcessInfo)).contents
        out.process, out.thread, out.pid, out.tid = 202, 303, 404, 505
        events.append("create-suspended")
        return inject_result
    def resume(thread):
        assert thread == 303
        events.append("resume")
        return resume_result
    value = NativeVFS.__new__(NativeVFS)
    value.observed_processes = {}
    value.connected, value.job, value.process = False, None, None
    value.kernel = SimpleNamespace(CreateJobObjectW=lambda *args: 101, SetInformationJobObject=limits,
                                   InitializeProcThreadAttributeList=initialize, UpdateProcThreadAttribute=attributes,
                                   DeleteProcThreadAttributeList=lambda _: events.append("delete-attributes"),
                                   ResumeThread=resume, CloseHandle=lambda handle: events.append(("close", handle)))
    def process_times(handle, created, *args):
        assert handle == 202
        ct.cast(created, ct.POINTER(ct.wintypes.FILETIME)).contents.dwLowDateTime = 100404
        return True
    value.kernel.GetProcessId = lambda handle: 404
    value.kernel.GetProcessTimes = process_times
    value.api = SimpleNamespace(usvfsCreateProcessHooked=create)
    return value, events


@pytest.mark.parametrize("failure", ["", "cancel", "status", "resume", "injection"])
def test_pid_persistence_precedes_resume_and_all_handles_close_once(failure):
    native, events = mock_native(0xFFFFFFFF if failure == "resume" else 0, failure != "injection")
    def persist(pid, identity):
        assert identity == {"pid": 404, "created": "100404"}
        events.append(("persist", pid))
        if failure == "cancel":
            raise Cancelled("fixture cancellation")
        if failure == "status":
            raise OSError("fixture status disk failure")
    try:
        if failure:
            with pytest.raises((ManagerError, Cancelled, OSError)):
                native.start(["C:/fixture.exe"], "C:/", before_resume=persist)
        else:
            assert native.start(["C:/fixture.exe"], "C:/", before_resume=persist) == 404
            assert events.index(("persist", 404)) < events.index("resume")
        if failure in {"cancel", "status", "injection"}:
            assert "resume" not in events
    finally:
        native.close()
    for handle in (101, 202, 303):
        assert events.count(("close", handle)) == 1


def test_windows_environment_overrides_are_case_insensitive(monkeypatch):
    monkeypatch.setenv("STEAMAPPID", "42")
    monkeypatch.setenv("STEAMGAMEID", "42")
    native, events = mock_native()
    try:
        native.start(["C:/fixture.exe"], "C:/", environment={"SteamAppId": "377160", "SteamGameId": "377160"})
    finally:
        native.close()
    entries = next(event[1].split("\0") for event in events if isinstance(event, tuple) and event[0] == "environment")
    assert [entry for entry in entries if entry.upper().startswith("STEAMAPPID=")] == ["STEAMAPPID=377160"]
    assert [entry for entry in entries if entry.upper().startswith("STEAMGAMEID=")] == ["STEAMGAMEID=377160"]


@pytest.mark.parametrize("member,timing,query_ok", [(True, True, True), (False, True, True),
                                                   (True, False, True), (True, True, False)])
def test_registered_pid_is_owned_only_when_retained_handle_is_in_this_job(member, timing, query_ok):
    from .test_process_identity import api, token
    native = NativeVFS.__new__(NativeVFS)
    native.observed_processes = {}
    native.job, native.process, native.identity_unverified = 101, None, False
    native.kernel, closed = api(times=timing)
    def vfs_processes(count, buffer):
        ct.cast(count, ct.POINTER(ct.c_size_t)).contents.value = 1
        buffer[0] = 42
        return True
    def job_processes(job, kind, buffer, size, returned):
        assert job == 101 and kind == 3
        header = (ct.wintypes.DWORD * 2).from_buffer(buffer)
        header[0], header[1] = 1, 1
        ct.c_size_t.from_buffer(buffer, 8).value = 42
        return True
    def membership(handle, job, result):
        assert handle == 909 and job == 101
        ct.cast(result, ct.POINTER(ct.wintypes.BOOL)).contents.value = member
        return query_ok
    native.api = SimpleNamespace(usvfsGetVFSProcessList=vfs_processes)
    native.kernel.QueryInformationJobObject = job_processes
    native.kernel.IsProcessInJob = membership
    if not timing or not query_ok:
        with pytest.raises(ManagerError, match="could not be verified|could not verify"):
            native.running_processes()
        assert native.identity_unverified
    else:
        assert native.running_processes() == ([token()] if member else [])
        assert not native.identity_unverified
    assert closed == ([] if member and timing and query_ok else [909])
    if native.observed_processes:
        native.job, native.connected = None, False
        native.close()
        assert closed == [909]


def test_retained_root_wait_failure_cannot_report_runtime_completion():
    native = NativeVFS.__new__(NativeVFS)
    native.observed_processes = {}
    native.job, native.process, native.identity_unverified = 101, SimpleNamespace(process=202, pid=404), False
    def vfs_processes(count, buffer):
        ct.cast(count, ct.POINTER(ct.c_size_t)).contents.value = 0
        return True
    native.api = SimpleNamespace(usvfsGetVFSProcessList=vfs_processes)
    native.kernel = SimpleNamespace(QueryInformationJobObject=lambda *args: True,
                                   WaitForSingleObject=lambda *args: 0xFFFFFFFF)
    with pytest.raises(ManagerError, match="retained runtime process handle"):
        native.running_processes()
    assert native.identity_unverified
