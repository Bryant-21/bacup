from __future__ import annotations

import logging

from bacup_ui import crash_diagnostics


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
        assert crash_diagnostics.enable_fatal_error_logging() is True
        assert calls == [(handler.stream, True)]
    finally:
        root.handlers = previous_handlers
        handler.close()
