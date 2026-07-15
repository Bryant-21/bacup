from __future__ import annotations

import logging

import app.paths as app_paths

from ui.core import logging_utils


def test_setup_logging_skips_console_handler_without_console_streams(tmp_path, monkeypatch):
    monkeypatch.setattr(logging_utils.sys, "stdout", None, raising=False)
    monkeypatch.setattr(logging_utils.sys, "stderr", None, raising=False)
    monkeypatch.setattr(logging_utils.sys, "__stdout__", None, raising=False)
    monkeypatch.setattr(logging_utils.sys, "__stderr__", None, raising=False)
    monkeypatch.setattr(app_paths, "get_logs_dir", lambda: tmp_path)

    root = logging.getLogger()
    handlers_before = list(root.handlers)
    for handler in handlers_before:
        handler.close()
    root.handlers.clear()

    try:
        logging_utils.setup_logging("toolkit", log_dir=tmp_path)

        handlers = list(root.handlers)
        assert len(handlers) == 1
        assert isinstance(handlers[0], logging.FileHandler)
        assert handlers[0].baseFilename.endswith(".log")
    finally:
        for handler in list(root.handlers):
            handler.close()
        root.handlers.clear()
