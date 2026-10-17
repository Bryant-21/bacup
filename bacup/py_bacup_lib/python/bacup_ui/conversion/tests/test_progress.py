from datetime import datetime

import pytest

from bacup_ui.conversion.progress import ProgressEstimate, estimated_phase_weights


def test_progress_credits_only_finished_work():
    estimate = ProgressEstimate({"nifs": 900, "records": 100, "pack": 100, "lodgen": 100})
    estimate.update(
        {
            "ui_key": "nifs",
            "status": "running",
            "total_items": 10,
            "completed_items": 5,
        },
        now=0,
    )
    estimate.update({"ui_key": "records", "status": "completed"}, now=0)
    estimate.update({"ui_key": "lodgen", "status": "skipped"}, now=0)
    assert estimate.fraction(now=0) == 550 / 1100


@pytest.mark.parametrize("start_phase, expected", [
    ("lodgen", {"lodgen": 240, "pack": 300, "deploy": 120}),
    ("deploy", {"pack": 300, "deploy": 120}),
])
def test_recovery_only_budgets_remaining_phases(start_phase, expected):
    assert estimated_phase_weights("fo76:fo4", start_phase=start_phase) == expected


def test_phase_eta_uses_cell_counts_and_elapsed_time():
    estimate = ProgressEstimate({"build_precombines": 120})
    phase = {"ui_key": "build_precombines", "status": "running", "total_items": 45564, "completed_items": 0}
    estimate.update(phase, now=0)
    assert estimate.phase_eta("build_precombines", now=60) == "Estimating..."
    phase["completed_items"] = 22782
    estimate.update(phase, now=600)
    assert estimate.phase_eta("build_precombines", now=600, wall_time=datetime(2026, 10, 13, 14, 38)) == (
        "About 10 min left · finishes ~2:48 PM"
    )
    phase["completed_items"] = 45564
    estimate.update(phase, now=1200)
    assert estimate.phase_eta("build_precombines", now=1200) == "Finalizing..."
    phase["status"] = "completed"
    estimate.update(phase, now=1210)
    assert estimate.phase_eta("build_precombines", now=1210) == ""


def test_phase_eta_waits_for_samples_and_includes_next_day():
    estimate = ProgressEstimate({"pack": 300})
    estimate.update({"ui_key": "pack", "status": "running", "total_items": 10, "completed_items": 5}, now=0)
    assert estimate.phase_eta("pack", now=1) == "Estimating..."
    assert estimate.phase_eta("pack", now=30, wall_time=datetime(2026, 10, 13, 23, 59, 45)) == (
        "Less than 1 min left · finishes ~Wed Oct 14 12:00 AM"
    )
    assert estimate.phase_eta("pending", now=30) == ""
