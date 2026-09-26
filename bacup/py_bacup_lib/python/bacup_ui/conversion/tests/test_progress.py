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
