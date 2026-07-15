from __future__ import annotations

import logging
from types import SimpleNamespace

from bacup_ui import crash_diagnostics


def test_resource_snapshot_records_process_system_and_pagefile_memory(monkeypatch):
    fake_psutil = SimpleNamespace(
        Process=lambda: SimpleNamespace(
            memory_info=lambda: SimpleNamespace(rss=2 * 1024**3, vms=5 * 1024**3)
        ),
        virtual_memory=lambda: SimpleNamespace(
            available=3 * 1024**3, total=16 * 1024**3
        ),
        swap_memory=lambda: SimpleNamespace(free=1 * 1024**3, total=4 * 1024**3),
    )
    monkeypatch.setitem(__import__("sys").modules, "psutil", fake_psutil)

    snapshot = crash_diagnostics._resource_snapshot()

    assert snapshot == (
        "rss=2.00 GiB vms=5.00 GiB "
        "system_available=3.00/16.00 GiB pagefile_free=1.00/4.00 GiB"
    )


def test_start_crash_diagnostics_records_immediate_snapshot(monkeypatch, caplog):
    monkeypatch.setattr(crash_diagnostics, "_enable_fatal_error_logging", lambda: True)
    monkeypatch.setattr(crash_diagnostics, "_resource_snapshot", lambda: "snapshot")

    with caplog.at_level(logging.INFO, logger="bacup.crash_diagnostics"):
        stop_event = crash_diagnostics.start_crash_diagnostics(interval_seconds=60.0)
        stop_event.set()

    assert "Memory heartbeat: snapshot" in caplog.text


def test_fatal_error_logging_uses_active_file_log(monkeypatch, tmp_path):
    root = logging.getLogger()
    handler = logging.FileHandler(tmp_path / "bacup.log", encoding="utf-8")
    previous_handlers = list(root.handlers)
    calls = []
    root.handlers = [handler]
    monkeypatch.setattr(
        crash_diagnostics.faulthandler,
        "enable",
        lambda *, file, all_threads: calls.append((file, all_threads)),
    )
    try:
        assert crash_diagnostics._enable_fatal_error_logging() is True
        assert calls == [(handler.stream, True)]
    finally:
        root.handlers = previous_handlers
        handler.close()


def test_launcher_stops_crash_diagnostics_on_exit(monkeypatch):
    import bacup_ui.__main__ as launcher

    stopped = []
    stop_event = SimpleNamespace(set=lambda: stopped.append(True))
    monkeypatch.setattr(
        crash_diagnostics, "start_crash_diagnostics", lambda: stop_event
    )
    monkeypatch.setattr(launcher, "run_bacup", lambda _path: None)

    launcher.main(["input.ba2"])

    assert stopped == [True]
