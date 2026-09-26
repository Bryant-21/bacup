from unittest.mock import MagicMock

from bacup_ui.conversion.panels import regen_panel
from bacup_ui.conversion.panels.regen_panel import RegenPanel


def test_terminal_error_opens_once_and_copies_full_details(monkeypatch):
    panel = RegenPanel.__new__(RegenPanel)
    event = {"type": "error", "message": "Damaged animation file\nSize: 0 bytes", "details": "Traceback: native cause"}
    panel.handle_event(event)
    assert panel._runner_status == "Conversion stopped"
    assert not panel._progress_succeeded
    imgui = MagicMock()
    imgui.begin_popup_modal.return_value = (True, True)
    imgui.get_frame_height_with_spacing.return_value = 30
    imgui.button.side_effect = lambda label: label.startswith("Copy")
    monkeypatch.setattr(regen_panel, "imgui", imgui)
    monkeypatch.setattr(regen_panel, "prepare_dialog", lambda *_args: None)
    panel._draw_error_popup()
    panel._draw_error_popup()
    assert imgui.open_popup.call_count == 1
    imgui.text_wrapped.assert_called_with(event["message"])
    imgui.set_clipboard_text.assert_called_with(event["message"] + "\n\n" + event["details"])
    imgui.button.side_effect = lambda label: label.startswith("Close")
    panel._draw_error_popup()
    assert panel._conversion_error is None
    imgui.close_current_popup.assert_called_once()
