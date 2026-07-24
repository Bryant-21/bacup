"""ConversionRunner — background thread with progress queue and cancellation.

Replaces AsyncWorker for conversion tasks that need streaming progress.
The UI polls drain() each frame to get structured events.
"""
from __future__ import annotations

import dataclasses
import logging
import queue
import sys
import threading
from datetime import datetime
from typing import Any, Callable

from bacup_lib.models import PhaseProgress

_log = logging.getLogger("conversion.runner")


def emit_runner_status(runner: Any, message: str) -> None:
    emit_status = getattr(runner, "emit_status", None)
    if callable(emit_status):
        emit_status(message)


def _ts() -> str:
    """Wall-clock HH:MM:SS for streamed log lines during long headless runs."""
    return datetime.now().strftime("%H:%M:%S")


class ConversionRunner:
    """Run a conversion function in a background thread with progress streaming."""

    def __init__(self, work_fn: Callable[[ConversionRunner], None]):
        self._work_fn = work_fn
        self._queue: queue.Queue[dict] = queue.Queue()
        self._cancelled = threading.Event()
        self._thread: threading.Thread | None = None
        self.done = False
        self.error: Exception | None = None

    def start(self) -> None:
        """Start the background thread."""
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def _run(self) -> None:
        try:
            self._work_fn(self)
        except Exception as e:
            self.error = e
            self.emit_log("ERROR", f"Conversion failed: {e}")
            _log.exception("Conversion runner error")
        finally:
            self.done = True

    def cancel(self) -> None:
        """Request cancellation. The work function must check is_cancelled()."""
        self._cancelled.set()

    def is_cancelled(self) -> bool:
        """Check if cancellation has been requested."""
        return self._cancelled.is_set()

    def drain(self) -> list[dict]:
        """Drain all pending events from the queue (call from main thread)."""
        events = []
        while True:
            try:
                events.append(self._queue.get_nowait())
            except queue.Empty:
                break
        return events

    # ---- Event emitters (called from work thread) ----

    def emit_phase_start(self, progress: PhaseProgress) -> None:
        self._queue.put({"type": "phase_start", "data": dataclasses.asdict(progress)})

    def emit_item_progress(self, progress: PhaseProgress) -> None:
        self._queue.put({"type": "item_progress", "data": dataclasses.asdict(progress)})

    def emit_phase_complete(self, progress: PhaseProgress) -> None:
        self._queue.put({"type": "phase_complete", "data": dataclasses.asdict(progress)})

    def emit_log(self, level: str, message: str) -> None:
        self._queue.put({"type": "log", "level": level, "message": message})

    def emit_status(self, message: str) -> None:
        self._queue.put({"type": "status", "message": message})

    def emit_complete(self, mod_path: str, summary: Any) -> None:
        self._queue.put({
            "type": "complete",
            "mod_path": mod_path,
            "summary": dataclasses.asdict(summary) if dataclasses.is_dataclass(summary) else summary,
        })


class NullConversionRunner:
    """Silent runner. Drops all events. Use for tests that only need exit code."""

    def is_cancelled(self) -> bool:
        return False

    def emit_log(self, level: str, message: str) -> None:
        pass

    def emit_status(self, message: str) -> None:
        pass

    def emit_phase_start(self, progress: PhaseProgress) -> None:
        pass

    def emit_item_progress(self, progress: PhaseProgress) -> None:
        pass

    def emit_phase_complete(self, progress: PhaseProgress) -> None:
        pass

    def emit_complete(self, mod_path: str, summary: Any) -> None:
        pass


class Drainer:
    """Background thread that polls ConversionRun.drain_events and forwards
    each event to a ConversionRunner's emit_* methods.

    Lifecycle: start() before the first run_phase call; stop() in a finally
    block. The drainer runs at ~60 Hz; events arrive in batches.
    """

    def __init__(self, run: Any, runner: ConversionRunner, hz: float = 60.0) -> None:
        self._run = run
        self._runner = runner
        self._period = 1.0 / hz
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        self._thread = threading.Thread(
            target=self._loop, daemon=True, name="conversion-drainer"
        )
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=2.0)
        # Final drain so trailing events aren't lost.
        for ev in self._run.drain_events(256):
            self._dispatch(ev)

    def _loop(self) -> None:
        import time as _time
        while not self._stop.is_set():
            for ev in self._run.drain_events(256):
                self._dispatch(ev)
            _time.sleep(self._period)

    def _dispatch(self, ev: dict) -> None:
        kind = ev.get("kind")
        if kind == "log":
            self._runner.emit_log(ev.get("level", "INFO"), ev.get("message", ""))
        elif kind == "progress":
            phase = ev.get("phase", "")
            current = ev.get("current", 0)
            total = ev.get("total", 0)
            item = ev.get("item", "")
            self._runner.emit_log("INFO", f"[{phase}] {current}/{total} {item}".rstrip())
        elif kind == "started":
            self._runner.emit_log("INFO", f"phase started: {ev.get('phase')}")
        elif kind == "completed":
            r = ev.get("report", {})
            phase = ev.get("phase", "")
            self._runner.emit_log(
                "INFO",
                f"phase completed: {phase} "
                f"changed={r.get('records_changed', 0)} "
                f"added={r.get('records_added', 0)} "
                f"dropped={r.get('records_dropped', 0)} "
                f"warnings={r.get('warnings', 0)} "
                f"elapsed_ms={r.get('elapsed_ms', 0)}",
            )


class StreamingConversionRunner:
    """Prints events to stdout. Use for headless scripts and CLI invocations."""

    def __init__(self, stream=None) -> None:
        # Resolve at construction time, not import time, so capsys/redirected
        # stdout in tests sees the redirection.
        self._stream = stream if stream is not None else sys.stdout

    def is_cancelled(self) -> bool:
        return False

    def emit_log(self, level: str, message: str) -> None:
        print(f"  [{_ts()}] [{level}] {message}", file=self._stream, flush=True)

    def emit_status(self, message: str) -> None:
        print(f"  [{_ts()}] [status] {message}", file=self._stream, flush=True)

    def emit_phase_start(self, progress: PhaseProgress) -> None:
        print(f"  [{_ts()}] [phase_start] {progress}", file=self._stream, flush=True)

    def emit_item_progress(self, progress: PhaseProgress) -> None:
        # Quiet on per-item progress — too noisy for stdout. Override if needed.
        pass

    def emit_phase_complete(self, progress: PhaseProgress) -> None:
        print(f"  [{_ts()}] [phase_complete] {progress}", file=self._stream, flush=True)

    def emit_complete(self, mod_path: str, summary: Any) -> None:
        print(f"  [{_ts()}] [complete] {mod_path} {summary}", file=self._stream, flush=True)
