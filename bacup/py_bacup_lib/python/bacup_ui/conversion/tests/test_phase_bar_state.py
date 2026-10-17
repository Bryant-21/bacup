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


def test_phase_row_shows_cell_count(monkeypatch):
    from bacup_ui.conversion.widgets import phase_progress

    rows = []
    monkeypatch.setattr(phase_progress, "progress_row", lambda *args, **kwargs: rows.append(kwargs))
    phase = {"ui_key": "build_precombines", "status": "running", "total_items": 45564,
             "completed_items": 22782, "current_item": "Building precombined meshes"}
    phase_progress.draw_phase_progress("test", [("build_precombines", "Build Precombines")], [phase])
    assert rows[-1]["count"] == "22,782 / 45,564"
    assert rows[-1]["detail"] == "Building precombined meshes"
    phase["status"] = "completed"
    phase_progress.draw_phase_progress("test", [("build_precombines", "Build Precombines")], [phase])
    assert rows[-1]["count"] == "45,564 / 45,564"
    assert rows[-1]["detail"] == ""
