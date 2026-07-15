from bacup_ui.conversion.widgets.phase_progress import phase_bar_state


def test_completed_batch_phase_is_complete_even_without_totals():
    assert phase_bar_state({"status": "completed", "total_items": 0}) == ("complete", 1.0)


def test_completed_counted_phase_is_complete():
    assert phase_bar_state(
        {"status": "completed", "total_items": 10, "completed_items": 7}
    ) == ("complete", 1.0)


def test_running_batch_phase_is_indeterminate():
    assert phase_bar_state({"status": "running", "total_items": 0}) == ("indeterminate", 0.0)


def test_running_counted_phase_is_determinate_fraction():
    mode, fraction = phase_bar_state(
        {"status": "running", "total_items": 4, "completed_items": 1}
    )
    assert mode == "determinate"
    assert abs(fraction - 0.25) < 1e-9


def test_pending_phase_is_none():
    assert phase_bar_state({"status": "pending"}) == ("none", 0.0)
