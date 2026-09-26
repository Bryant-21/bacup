"""Progress reporting for the long native stages (precombines, previs)."""

from __future__ import annotations

import time


def stage_progress(log, progress, *, stage: str, noun: str, item: str, log_every: float = 60.0):
    """Returns the ``(done, total)`` callback a native stage reports through.

    Every update goes to ``progress(done, total, item)``; about once a minute,
    and at the end, a line with an estimate of the time left goes to the log.
    """
    started = time.monotonic()
    last_log = started

    def report(done: int, total: int) -> None:
        nonlocal last_log
        if progress:
            progress(done, total, f"{item} ({done}/{total})")
        now = time.monotonic()
        if done < total and now - last_log < log_every:
            return
        last_log = now
        elapsed = now - started
        left = elapsed / done * (total - done)
        log(
            "INFO",
            f"{stage}: {done}/{total} {noun} done ({done * 100 // total}%), "
            f"{elapsed / 60:.0f} min elapsed, about {left / 60:.0f} min left",
        )

    return report
