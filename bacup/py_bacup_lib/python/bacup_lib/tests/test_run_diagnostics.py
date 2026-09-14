from __future__ import annotations

import contextlib
import logging
import os
from types import SimpleNamespace

import pytest

from bacup_lib import run_diagnostics


class _Runner:
    def __init__(self) -> None:
        self.events = []

    def is_cancelled(self):
        return False

    def emit_log(self, level, message):
        self.events.append(("log", level, message))

    def emit_status(self, message):
        self.events.append(("status", message))

    def emit_phase_start(self, progress):
        self.events.append(("phase_start", progress))

    def emit_item_progress(self, progress):
        self.events.append(("item_progress", progress))

    def emit_phase_complete(self, progress):
        self.events.append(("phase_complete", progress))

    def emit_complete(self, mod_path, summary):
        self.events.append(("complete", mod_path, summary))


def test_create_run_diagnostics_dir_is_unique(tmp_path):
    first = run_diagnostics.create_run_diagnostics_dir(tmp_path, "SeventySix")
    second = run_diagnostics.create_run_diagnostics_dir(tmp_path, "SeventySix")

    assert first != second
    assert first.parent == tmp_path / "conversion" / "SeventySix"
    assert first.is_dir()
    assert second.is_dir()


def test_native_stderr_tee_captures_os_level_stderr(tmp_path):
    log_path = tmp_path / "native_stderr.log"

    with run_diagnostics._native_stderr_tee(log_path):
        os.write(2, b"native timing detail\n")

    assert "native timing detail" in log_path.read_text(encoding="utf-8")


def test_full_logging_scope_defaults_to_info_without_drop_trace(
    tmp_path,
    monkeypatch,
):
    monkeypatch.delenv("MODBOX_TRACE_DROPS", raising=False)
    monkeypatch.delenv("MODBOX_TRACE_DROPS_FILE", raising=False)
    calls = []
    native = SimpleNamespace(
        conversion_configure_drop_trace=lambda *args: calls.append(args)
    )
    monkeypatch.setattr(
        "bacup_lib.native_runtime.load_native_module",
        lambda: native,
    )
    monkeypatch.setattr(
        run_diagnostics,
        "_native_stderr_tee",
        lambda _path: contextlib.nullcontext(),
    )
    runner = _Runner()

    with run_diagnostics.full_logging_scope(tmp_path, runner) as wrapped:
        wrapped.emit_log("WARN", "runner warning")
        logging.getLogger("regen").info("pipeline detail")
        logging.getLogger("regen").debug("verbose detail")

    assert calls == [
        (None, None),
        (None, None),
    ]
    content = (tmp_path / "regen.log").read_text(encoding="utf-8")
    assert "Full logging enabled" in content
    assert "runner warning" in content
    assert "pipeline detail" in content
    assert "verbose detail" not in content
    assert not (tmp_path / "drop_trace.log").exists()
    assert ("log", "WARN", "runner warning") in runner.events


@pytest.mark.parametrize("configured_path", [None, "custom/drop-trace.log"])
def test_full_logging_scope_honors_explicit_drop_trace_filter_and_path(
    tmp_path,
    monkeypatch,
    configured_path,
):
    monkeypatch.setenv("MODBOX_TRACE_DROPS", "FACT:VENC")
    if configured_path is None:
        monkeypatch.delenv("MODBOX_TRACE_DROPS_FILE", raising=False)
        expected_path = tmp_path / "drop_trace.log"
    else:
        expected_path = tmp_path / configured_path
        monkeypatch.setenv("MODBOX_TRACE_DROPS_FILE", str(expected_path))
    calls = []
    native = SimpleNamespace(
        conversion_configure_drop_trace=lambda *args: calls.append(args)
    )
    monkeypatch.setattr(
        "bacup_lib.native_runtime.load_native_module",
        lambda: native,
    )
    monkeypatch.setattr(
        run_diagnostics,
        "_native_stderr_tee",
        lambda _path: contextlib.nullcontext(),
    )

    with run_diagnostics.full_logging_scope(tmp_path, _Runner()):
        pass

    assert calls == [
        ("FACT:VENC", str(expected_path)),
        (None, None),
    ]


def test_full_logging_scope_restores_logger_and_trace_after_exception(
    tmp_path,
    monkeypatch,
):
    monkeypatch.setenv("MODBOX_TRACE_DROPS", "1")
    monkeypatch.delenv("MODBOX_TRACE_DROPS_FILE", raising=False)
    calls = []
    native = SimpleNamespace(
        conversion_configure_drop_trace=lambda *args: calls.append(args)
    )
    monkeypatch.setattr(
        "bacup_lib.native_runtime.load_native_module",
        lambda: native,
    )
    monkeypatch.setattr(
        run_diagnostics,
        "_native_stderr_tee",
        lambda _path: contextlib.nullcontext(),
    )
    root_logger = logging.getLogger()
    previous_level = root_logger.level
    previous_handlers = tuple(root_logger.handlers)
    root_logger.setLevel(logging.ERROR)
    expected_level = root_logger.level
    try:
        with pytest.raises(RuntimeError, match="diagnostic failure"):
            with run_diagnostics.full_logging_scope(tmp_path, _Runner()):
                raise RuntimeError("diagnostic failure")

        assert root_logger.level == expected_level
        assert tuple(root_logger.handlers) == previous_handlers
    finally:
        root_logger.setLevel(previous_level)

    assert calls == [
        ("1", str(tmp_path / "drop_trace.log")),
        (None, None),
    ]
    content = (tmp_path / "regen.log").read_text(encoding="utf-8")
    assert "Conversion failed" in content
    assert "Traceback" in content
    assert "diagnostic failure" in content
