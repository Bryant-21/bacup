from __future__ import annotations

from bacup_lib.workflows.unified import _script_patch_source


def test_idle_timer_dependency_has_no_guessed_durable_patch() -> None:
    assert _script_patch_source("COMP_IdleTimerScript") is None
