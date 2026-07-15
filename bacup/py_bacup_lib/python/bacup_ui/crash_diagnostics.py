"""Crash diagnostics for the standalone B.A.C.U.P. process."""

from __future__ import annotations

import faulthandler
import logging
import threading

_log = logging.getLogger("bacup.crash_diagnostics")


def _enable_fatal_error_logging() -> bool:
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


def _resource_snapshot() -> str | None:
    try:
        import psutil

        process = psutil.Process()
        process_memory = process.memory_info()
        system_memory = psutil.virtual_memory()
        pagefile = psutil.swap_memory()
    except Exception:
        return None

    gib = 1024**3
    return (
        f"rss={process_memory.rss / gib:.2f} GiB "
        f"vms={process_memory.vms / gib:.2f} GiB "
        f"system_available={system_memory.available / gib:.2f}/{system_memory.total / gib:.2f} GiB "
        f"pagefile_free={pagefile.free / gib:.2f}/{pagefile.total / gib:.2f} GiB"
    )


def start_crash_diagnostics(interval_seconds: float = 10.0) -> threading.Event:
    """Enable fatal stack dumps and periodic memory snapshots until stopped."""
    _enable_fatal_error_logging()
    stop_event = threading.Event()
    snapshot = _resource_snapshot()
    if snapshot is None:
        _log.warning("Memory heartbeat unavailable: psutil is not installed")
        return stop_event

    _log.info("Memory heartbeat: %s", snapshot)

    def monitor() -> None:
        while not stop_event.wait(interval_seconds):
            current = _resource_snapshot()
            if current is None:
                _log.warning("Memory heartbeat stopped: resource probe failed")
                return
            _log.debug("Memory heartbeat: %s", current)

    threading.Thread(
        target=monitor,
        name="bacup-memory-heartbeat",
        daemon=True,
    ).start()
    return stop_event
