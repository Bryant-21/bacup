from __future__ import annotations

import logging
from types import SimpleNamespace

from bacup_ui.conversion.panels import conversion_log
from bacup_ui.conversion.panels.conversion_log import (
    ConversionLogPanel,
    _is_at_log_bottom,
)


def test_log_auto_follow_pauses_when_user_scrolls_up():
    assert _is_at_log_bottom(90.0, 100.0) is True
    assert _is_at_log_bottom(89.9, 100.0) is False


def test_log_draw_does_not_force_scrolled_view_to_bottom(monkeypatch):
    panel = ConversionLogPanel(workspace=object())
    scroll_calls = []
    monkeypatch.setattr(
        conversion_log.imgui, "checkbox", lambda _label, value: (False, value)
    )
    monkeypatch.setattr(conversion_log.imgui, "same_line", lambda: None)
    monkeypatch.setattr(conversion_log.imgui, "button", lambda _label: False)
    monkeypatch.setattr(conversion_log.imgui, "separator", lambda: None)
    monkeypatch.setattr(
        conversion_log.imgui,
        "get_content_region_avail",
        lambda: SimpleNamespace(y=100.0),
    )
    monkeypatch.setattr(conversion_log.imgui, "begin_child", lambda *_args: True)
    monkeypatch.setattr(conversion_log.imgui, "get_scroll_y", lambda: 25.0)
    monkeypatch.setattr(conversion_log.imgui, "get_scroll_max_y", lambda: 100.0)
    monkeypatch.setattr(
        conversion_log.imgui,
        "set_scroll_here_y",
        lambda value: scroll_calls.append(value),
    )
    monkeypatch.setattr(conversion_log.imgui, "end_child", lambda: None)

    panel.draw_body()

    assert scroll_calls == []


def test_conversion_log_records_phase_boundaries(caplog):
    panel = ConversionLogPanel(workspace=object())

    with caplog.at_level(logging.INFO, logger="toolkit.conversion"):
        panel.handle_event(
            {
                "type": "phase_start",
                "data": {
                    "phase": 4,
                    "phase_name": "Convert NIFs",
                    "status": "running",
                },
            }
        )
        panel.handle_event(
            {
                "type": "phase_complete",
                "data": {
                    "phase": 4,
                    "phase_name": "Convert NIFs",
                    "status": "completed",
                    "completed_items": 0,
                    "total_items": 0,
                },
            }
        )

    assert panel._entries == [
        ("INFO", "Starting phase 4: Convert NIFs"),
        ("INFO", "Completed phase 4: Convert NIFs (0/0 items)"),
    ]
    assert "Starting phase 4: Convert NIFs" in caplog.text
    assert "Completed phase 4: Convert NIFs (0/0 items)" in caplog.text


def test_conversion_log_records_phase_timing(caplog):
    panel = ConversionLogPanel(workspace=object())

    with caplog.at_level(logging.INFO, logger="toolkit.conversion"):
        panel.handle_event(
            {
                "type": "phase_complete",
                "data": {
                    "phase": 4,
                    "phase_name": "Convert NIFs",
                    "status": "completed",
                    "completed_items": 1,
                    "total_items": 1,
                    "elapsed_seconds": 1.23456,
                },
            }
        )

    assert panel._entries == [
        ("INFO", "Completed phase 4: Convert NIFs (1/1 items) in 1.235s"),
    ]
    assert "Completed phase 4: Convert NIFs (1/1 items) in 1.235s" in caplog.text
