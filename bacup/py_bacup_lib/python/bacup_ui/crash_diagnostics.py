"""Crash diagnostics for the standalone B.A.C.U.P. process."""

from __future__ import annotations

import faulthandler
import logging

_log = logging.getLogger("bacup.crash_diagnostics")


def enable_fatal_error_logging() -> bool:
    file_handler = next(
        (
            handler
            for handler in logging.getLogger().handlers
            if isinstance(handler, logging.FileHandler) and handler.stream is not None
        ),
        None,
    )
    if file_handler is None:
        _log.warning("Fatal error logging unavailable: no file log handler")
        return False
    try:
        faulthandler.enable(file=file_handler.stream, all_threads=True)
    except Exception as exc:
        _log.warning("Fatal error logging unavailable: %s", exc)
        return False
    _log.info("Fatal error logging enabled: %s", file_handler.baseFilename)
    return True
