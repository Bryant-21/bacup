"""Per-run logging setup for ModBox21 UI apps.

Each app launch writes to a new timestamped log file and prunes older runs.
stdout/stderr are redirected into the logger so third-party library output is
captured alongside application logs.
"""

from __future__ import annotations

from datetime import datetime
import logging
import logging.handlers
import os
import sys
import threading
from pathlib import Path


def setup_logging(app_name: str = "toolkit", log_dir: Path | None = None, max_backups: int = 20) -> logging.Logger:
    """Configure root logger with a fresh per-run timestamped log file.

    Args:
        app_name: Base name for the log file (e.g. "toolkit" →
            logs/toolkit-YYYYMMDD-HHMMSS-pid1234.log).
        log_dir: Directory to write logs. Defaults to the standard logs dir.
        max_backups: Number of per-run log files to keep.

    Returns:
        A named logger for the calling app (``logging.getLogger(app_name)``).
    """
    if log_dir is None:
        from app.paths import get_logs_dir
        log_dir = get_logs_dir()

    log_dir = Path(log_dir)
    log_dir.mkdir(parents=True, exist_ok=True)

    _prune_old_logs(log_dir, app_name, max_backups)
    log_file = _new_log_path(log_dir, app_name)

    # File handler — captures everything at DEBUG+
    file_handler = logging.FileHandler(log_file, encoding="utf-8", delay=False)
    file_handler.setLevel(logging.DEBUG)
    file_handler.setFormatter(
        logging.Formatter("%(asctime)s %(levelname)-8s [%(name)s] %(message)s")
    )

    # Console handler — INFO+ for interactive dev runs when a console exists.
    console_stream = next(
        (
            stream
            for stream in (sys.stderr, sys.__stderr__, sys.stdout, sys.__stdout__)
            if stream is not None
        ),
        None,
    )
    console_handler = None
    if console_stream is not None:
        console_handler = logging.StreamHandler(console_stream)
        console_handler.setLevel(logging.INFO)
        console_handler.setFormatter(
            logging.Formatter("%(levelname)-8s [%(name)s] %(message)s")
        )

    root_logger = logging.getLogger()
    root_logger.setLevel(logging.DEBUG)
    # Clear any handlers set before this call (e.g. basicConfig in __main__)
    root_logger.handlers.clear()
    root_logger.addHandler(file_handler)
    if console_handler is not None:
        root_logger.addHandler(console_handler)

    # Redirect stdout/stderr so third-party print() output lands in the log
    sys.stdout = _LoggerStream(logging.getLogger("stdout"), logging.INFO, _real_stdout)
    sys.stderr = _LoggerStream(
        logging.getLogger("stderr"),
        logging.ERROR,
        _real_stderr or _real_stdout,
    )

    # Route Python warnings (e.g. numpy RuntimeWarning) through logging
    # instead of the default stderr-only path. They land in the
    # ``py.warnings`` logger with source location context.
    logging.captureWarnings(True)

    # Uncaught exceptions on the main thread → log (instead of vanishing
    # into stderr after the interpreter starts shutting down).
    _install_excepthooks(logging.getLogger("unhandled"))

    logger = logging.getLogger(app_name)
    logger.info("Logging started — %s", log_file)
    return logger


def _install_excepthooks(logger: logging.Logger) -> None:
    """Route uncaught exceptions on main + worker threads to ``logger``."""

    def _main_hook(exc_type, exc_value, exc_tb):
        if issubclass(exc_type, KeyboardInterrupt):
            # Let Ctrl+C propagate as normal.
            sys.__excepthook__(exc_type, exc_value, exc_tb)
            return
        logger.critical(
            "Unhandled exception",
            exc_info=(exc_type, exc_value, exc_tb),
        )

    sys.excepthook = _main_hook

    # threading.excepthook is Python 3.8+. Available on all supported
    # Python versions for this project.
    def _thread_hook(args: "threading.ExceptHookArgs") -> None:
        if issubclass(args.exc_type, SystemExit):
            return
        logger.critical(
            "Unhandled exception in thread %s",
            args.thread.name if args.thread else "<unknown>",
            exc_info=(args.exc_type, args.exc_value, args.exc_traceback),
        )

    threading.excepthook = _thread_hook


def _new_log_path(log_dir: Path, app_name: str) -> Path:
    """Return a unique per-run log path."""
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    pid = os.getpid()
    candidate = log_dir / f"{app_name}-{stamp}-pid{pid}.log"
    if not candidate.exists():
        return candidate

    for suffix in range(1, 1000):
        candidate = log_dir / f"{app_name}-{stamp}-pid{pid}-{suffix}.log"
        if not candidate.exists():
            return candidate

    raise RuntimeError(f"Could not allocate unique log path in {log_dir}")


def _prune_old_logs(log_dir: Path, app_name: str, max_backups: int) -> None:
    """Keep only the most recent ``max_backups`` per-run logs."""
    if max_backups < 1:
        return

    run_logs = sorted(
        log_dir.glob(f"{app_name}-*.log"),
        key=lambda path: path.stat().st_mtime,
        reverse=True,
    )
    for old_log in run_logs[max_backups - 1:]:
        try:
            old_log.unlink()
        except OSError:
            # Another process may still have a historical log open.
            continue


# Keep references to the real stdout/stderr so _LoggerStream can fall back to
# them when logging itself needs to surface an error.
_real_stdout = sys.__stdout__
_real_stderr = sys.__stderr__


class _LoggerStream:
    """File-like object that forwards writes to a Python logger."""

    def __init__(self, logger: logging.Logger, level: int, fallback=None):
        self.logger = logger
        self.level = level
        self._fallback = fallback

    def write(self, message: str) -> None:
        if not message.strip():
            return
        try:
            self.logger.log(self.level, message.strip())
        except UnicodeEncodeError:
            safe = message.encode("utf-8", errors="replace").decode("utf-8")
            self.logger.log(self.level, safe.strip())
        except Exception:
            if self._fallback is not None:
                try:
                    self._fallback.write(message)
                except Exception:
                    pass

    def flush(self) -> None:
        pass

    def isatty(self) -> bool:
        return False
