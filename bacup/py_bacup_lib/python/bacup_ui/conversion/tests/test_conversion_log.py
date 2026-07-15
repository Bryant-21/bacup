from __future__ import annotations

import logging

from bacup_ui.conversion.panels.conversion_log import ConversionLogPanel


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
