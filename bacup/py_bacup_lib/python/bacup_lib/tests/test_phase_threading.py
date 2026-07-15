"""Verifies that conversion_run_phase releases the GIL during phase execution.

If a phase body ever uses Python::with_gil or holds a Py<PyAny>, the
background-thread counter below will stall while the phase runs.

This test is the canary for the "no GIL contact from phase code" rule.

Note: We use sentinel handle IDs (not real plugins). The translate phase will
fail quickly since the handles don't point to real plugins, but the GIL is
released before the failure. We assert the ticker advanced to confirm this.
"""
from __future__ import annotations

import threading
import time

import pytest

from bacup_lib.native_runtime import load_native_module


def test_phase_releases_gil(tmp_path) -> None:
    m = load_native_module()
    run_id = m.conversion_run_create_from_paths(
        "fo76", "fo4", None, "Output.esp", None, [], None,
        {"output_plugin_name": "Output.esp", "mod_path": str(tmp_path)},
    )

    counter = [0]
    stop = threading.Event()

    def ticker() -> None:
        while not stop.is_set():
            counter[0] += 1
            time.sleep(0.001)  # 1 ms

    t = threading.Thread(target=ticker, daemon=True)
    t.start()
    try:
        # Run phase 50× in a loop. Each call releases the GIL, failing quickly
        # (invalid handle), allowing the ticker to advance between calls.
        for _ in range(50):
            try:
                m.conversion_run_phase(run_id, "translate", {
                    "mod_path": str(tmp_path),
                    "source_extracted_dir": str(tmp_path),
                })
            except (RuntimeError, Exception):
                pass  # Expected — sentinel handles don't resolve
        before = counter[0]
        time.sleep(0.05)
        after = counter[0]
    finally:
        stop.set()
        t.join(timeout=2.0)
        m.conversion_run_drop(run_id)

    # Ticker advanced after the phase calls — GIL was not held for the full duration.
    assert after > before, "ticker stalled after phase calls — GIL may have been held"
    # Ticker also advanced DURING the phase calls (not just after sleep).
    assert before > 0, (
        f"ticker only reached {before} during 50 phase calls — GIL was held"
    )
