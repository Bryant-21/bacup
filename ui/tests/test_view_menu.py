import sys
from unittest.mock import MagicMock, patch

# Stub imgui_bundle before importing — no display needed
_imgui_mock = MagicMock()
sys.modules.setdefault("imgui_bundle", _imgui_mock)
sys.modules.setdefault("imgui_bundle.imgui", _imgui_mock)
sys.modules.setdefault("imgui_bundle.hello_imgui", _imgui_mock)

from ui.toolkit.view_menu import ViewMenuHelper


def test_display_name_strips_suffix():
    assert ViewMenuHelper._display_name("Files##papyrus") == "Files"


def test_display_name_no_suffix():
    assert ViewMenuHelper._display_name("AI Chat") == "AI Chat"


def test_display_name_first_hash_only():
    assert ViewMenuHelper._display_name("A##b##c") == "A"


def test_display_name_empty():
    assert ViewMenuHelper._display_name("") == ""


def test_init_stores_shared_labels():
    h = ViewMenuHelper(["AI Chat", "Log"])
    assert h._shared_labels == ["AI Chat", "Log"]


def test_find_window_returns_none_when_not_found():
    fake_window = MagicMock()
    fake_window.label = "Other##nif"
    fake_dp = MagicMock()
    fake_dp.dockable_windows = [fake_window]
    fake_rp = MagicMock()
    fake_rp.docking_params = fake_dp

    with patch("ui.toolkit.view_menu.hello_imgui") as mock_hi:
        mock_hi.get_runner_params.return_value = fake_rp
        result = ViewMenuHelper._find_window("Missing##nif")

    assert result is None


def test_find_window_returns_correct_window():
    fake_window = MagicMock()
    fake_window.label = "Files##papyrus"
    fake_dp = MagicMock()
    fake_dp.dockable_windows = [fake_window]
    fake_rp = MagicMock()
    fake_rp.docking_params = fake_dp

    with patch("ui.toolkit.view_menu.hello_imgui") as mock_hi:
        mock_hi.get_runner_params.return_value = fake_rp
        result = ViewMenuHelper._find_window("Files##papyrus")

    assert result is fake_window
