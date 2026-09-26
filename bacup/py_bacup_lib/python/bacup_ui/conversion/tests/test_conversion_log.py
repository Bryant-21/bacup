from bacup_ui.conversion.panels.conversion_log import _is_at_log_bottom


def test_log_auto_follow_pauses_when_user_scrolls_up():
    assert _is_at_log_bottom(90.0, 100.0) is True
    assert _is_at_log_bottom(89.9, 100.0) is False
