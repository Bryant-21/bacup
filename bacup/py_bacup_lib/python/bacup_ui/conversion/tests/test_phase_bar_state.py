import pytest

from bacup_ui.conversion.widgets.phase_progress import phase_bar_state


@pytest.mark.parametrize("phase, expected", [
    ({"status": "completed", "total_items": 0}, ("complete", 1.0)),
    ({"status": "completed", "total_items": 10, "completed_items": 7}, ("complete", 1.0)),
    ({"status": "running", "total_items": 0}, ("indeterminate", 0.0)),
    ({"status": "running", "total_items": 4, "completed_items": 1}, ("determinate", 0.25)),
    ({"status": "pending"}, ("none", 0.0)),
])
def test_phase_bar_state(phase, expected):
    mode, fraction = phase_bar_state(phase)
    assert mode == expected[0]
    assert fraction == pytest.approx(expected[1])
