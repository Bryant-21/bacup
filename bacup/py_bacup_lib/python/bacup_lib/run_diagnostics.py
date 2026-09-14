from __future__ import annotations

import contextlib
import datetime
import logging
import os
import threading
from pathlib import Path

from bacup_lib.models import PhaseProgress


def create_run_diagnostics_dir(logs_root: Path, mod_name: str) -> Path:
    base = Path(logs_root) / "conversion" / mod_name
    timestamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    stem = f"{timestamp}-pid{os.getpid()}"
    for index in range(1000):
        suffix = "" if index == 0 else f"-{index}"
        diagnostics_root = base / f"{stem}{suffix}"
        try:
            diagnostics_root.mkdir(parents=True, exist_ok=False)
        except FileExistsError:
            continue
        return diagnostics_root
    raise RuntimeError(f"could not allocate a unique run log directory under {base}")


class _LoggedRunner:
    def __init__(self, runner, logger: logging.Logger) -> None:
        self._runner = runner
        self._logger = logger

    def is_cancelled(self) -> bool:
        return self._runner.is_cancelled()

    def emit_log(self, level: str, message: str) -> None:
        log_level = logging.WARNING if level == "WARN" else getattr(
            logging, level, logging.INFO
        )
        self._logger.log(log_level, "%s", message)
        self._runner.emit_log(level, message)

    def emit_status(self, message: str) -> None:
        self._logger.info("[status] %s", message)
        self._runner.emit_status(message)

    def emit_phase_start(self, progress: PhaseProgress) -> None:
        self._logger.info("[phase_start] %s", progress)
        self._runner.emit_phase_start(progress)

    def emit_item_progress(self, progress: PhaseProgress) -> None:
        self._runner.emit_item_progress(progress)

    def emit_phase_complete(self, progress: PhaseProgress) -> None:
        self._logger.info("[phase_complete] %s", progress)
        self._runner.emit_phase_complete(progress)

    def emit_complete(self, mod_path: str, summary) -> None:
        self._logger.info("[complete] %s %s", mod_path, summary)
        self._runner.emit_complete(mod_path, summary)


@contextlib.contextmanager
def _native_stderr_tee(log_path: Path):
    log_path.parent.mkdir(parents=True, exist_ok=True)
    sink = log_path.open("ab", buffering=0)
    original_fd: int | None
    try:
        original_fd = os.dup(2)
    except OSError:
        original_fd = None
    read_fd, write_fd = os.pipe()
    os.dup2(write_fd, 2)
    os.close(write_fd)

    def pump() -> None:
        while True:
            try:
                chunk = os.read(read_fd, 65536)
            except OSError:
                break
            if not chunk:
                break
            if original_fd is not None:
                try:
                    os.write(original_fd, chunk)
                except OSError:
                    pass
            sink.write(chunk)

    thread = threading.Thread(
        target=pump,
        name="conversion-native-stderr-tee",
        daemon=True,
    )
    thread.start()
    try:
        yield
    finally:
        if original_fd is None:
            try:
                os.close(2)
            except OSError:
                pass
        else:
            os.dup2(original_fd, 2)
            os.close(original_fd)
        thread.join(timeout=5)
        try:
            os.close(read_fd)
        except OSError:
            pass
        sink.close()


@contextlib.contextmanager
def full_logging_scope(diagnostics_root: Path, runner):
    from bacup_lib.native_runtime import load_native_module

    native = load_native_module()
    diagnostics_root.mkdir(parents=True, exist_ok=True)
    handler = logging.FileHandler(
        diagnostics_root / "regen.log",
        mode="w",
        encoding="utf-8",
    )
    handler.setLevel(logging.INFO)
    handler.setFormatter(logging.Formatter("%(asctime)s %(levelname)s %(message)s"))
    root_logger = logging.getLogger()
    previous_root_level = root_logger.level
    root_logger.setLevel(logging.INFO)
    root_logger.addHandler(handler)
    run_logger = logging.getLogger("regen.ui")
    wrapped_runner = _LoggedRunner(runner, run_logger)
    trace_filter = os.environ.get("MODBOX_TRACE_DROPS") or None
    trace_path = os.environ.get("MODBOX_TRACE_DROPS_FILE")
    try:
        if trace_filter is None:
            native.conversion_configure_drop_trace(None, None)
        else:
            native.conversion_configure_drop_trace(
                trace_filter,
                trace_path or str(diagnostics_root / "drop_trace.log"),
            )
        with _native_stderr_tee(diagnostics_root / "native_stderr.log"):
            wrapped_runner.emit_log(
                "INFO",
                f"Full logging enabled: {diagnostics_root}",
            )
            yield wrapped_runner
    except Exception:
        run_logger.exception("Conversion failed")
        raise
    finally:
        try:
            native.conversion_configure_drop_trace(None, None)
        finally:
            root_logger.removeHandler(handler)
            root_logger.setLevel(previous_root_level)
            handler.close()
