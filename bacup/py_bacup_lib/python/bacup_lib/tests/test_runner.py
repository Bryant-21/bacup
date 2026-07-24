"""Tests for ConversionRunner — background thread with progress queue."""
from __future__ import annotations

import time

import pytest


def test_runner_completes():
    from bacup_lib.runner import ConversionRunner

    results = []

    def work(runner: ConversionRunner):
        runner.emit_log("INFO", "Starting")
        runner.emit_log("INFO", "Done")
        results.append("finished")

    runner = ConversionRunner(work)
    runner.start()

    # Wait for completion
    for _ in range(100):
        runner.drain()
        if runner.done:
            break
        time.sleep(0.01)

    assert runner.done
    assert runner.error is None
    assert "finished" in results


def test_runner_emits_progress():
    from bacup_lib.models import PhaseProgress
    from bacup_lib.runner import ConversionRunner

    def work(runner: ConversionRunner):
        progress = PhaseProgress(phase=1, phase_name="Test", total_items=2)
        runner.emit_phase_start(progress)
        progress.completed_items = 1
        progress.current_item = "item1"
        runner.emit_item_progress(progress)
        progress.completed_items = 2
        progress.current_item = "item2"
        runner.emit_item_progress(progress)
        progress.status = "completed"
        runner.emit_phase_complete(progress)

    runner = ConversionRunner(work)
    events = []

    runner.start()
    for _ in range(100):
        for event in runner.drain():
            events.append(event)
        if runner.done:
            break
        time.sleep(0.01)

    phase_starts = [e for e in events if e["type"] == "phase_start"]
    item_progress = [e for e in events if e["type"] == "item_progress"]
    phase_completes = [e for e in events if e["type"] == "phase_complete"]

    assert len(phase_starts) == 1
    assert len(item_progress) == 2
    assert len(phase_completes) == 1


def test_runner_emits_status():
    from bacup_lib.runner import ConversionRunner

    runner = ConversionRunner(lambda active: active.emit_status("Writing reports"))
    runner.start()
    for _ in range(100):
        if runner.done:
            break
        time.sleep(0.01)

    assert runner.drain() == [{"type": "status", "message": "Writing reports"}]


def test_runner_cancellation():
    from bacup_lib.runner import ConversionRunner

    iterations = []

    def work(runner: ConversionRunner):
        for i in range(100):
            if runner.is_cancelled():
                break
            iterations.append(i)
            time.sleep(0.01)

    runner = ConversionRunner(work)
    runner.start()
    time.sleep(0.05)
    runner.cancel()

    for _ in range(100):
        runner.drain()
        if runner.done:
            break
        time.sleep(0.01)

    assert runner.done
    assert len(iterations) < 100  # Should have stopped early


def test_runner_captures_error():
    from bacup_lib.runner import ConversionRunner

    def work(runner: ConversionRunner):
        raise ValueError("Test error")

    runner = ConversionRunner(work)
    runner.start()

    for _ in range(100):
        runner.drain()
        if runner.done:
            break
        time.sleep(0.01)

    assert runner.done
    assert runner.error is not None
    assert "Test error" in str(runner.error)
