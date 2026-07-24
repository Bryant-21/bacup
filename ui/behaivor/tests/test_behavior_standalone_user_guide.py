from __future__ import annotations

import pytest


@pytest.fixture(autouse=True)
def _stub_game_dbs(monkeypatch):
    """BehaviorEditorApp() scans the ``data/`` dir for ``*_havok.db`` files on
    construction; the public repo ships no ``data/`` dir. Stub the scan so these
    menu/toolbar tests don't depend on a populated data directory."""
    monkeypatch.setattr("ui.behaivor.main_window.get_available_game_dbs", lambda: {})


def test_behavior_editor_host_menu_can_skip_local_help(monkeypatch):
    from ui.behaivor.main_window import BehaviorEditorApp

    app = BehaviorEditorApp()
    labels = []

    monkeypatch.setattr("ui.behaivor.main_window.imgui.begin_menu", lambda label: labels.append(label) or False)

    app.draw_menu_items(include_help=False)

    assert "Help" not in labels


def test_behavior_editor_help_menu_uses_shared_user_guide_item(monkeypatch):
    from ui.behaivor.main_window import BehaviorEditorApp

    app = BehaviorEditorApp()
    calls = []

    monkeypatch.setattr("ui.behaivor.main_window.imgui.begin_menu", lambda label: label == "Help")
    monkeypatch.setattr("ui.behaivor.main_window.draw_user_guide_menu_item", lambda provider: calls.append(provider))
    monkeypatch.setattr("ui.behaivor.main_window.imgui.separator", lambda: None)
    monkeypatch.setattr("ui.behaivor.main_window.imgui.menu_item", lambda *args, **kwargs: (False, False))
    monkeypatch.setattr("ui.behaivor.main_window.imgui.end_menu", lambda: None)

    app.draw_menu_items(include_help=True)

    assert calls == [app]


def test_behavior_editor_toolbar_can_include_shared_help_button(monkeypatch):
    from ui.behaivor import main_window
    from ui.behaivor.main_window import BehaviorEditorApp

    app = BehaviorEditorApp()
    calls = []

    monkeypatch.setattr(main_window.imgui, "button", lambda icon: False)
    monkeypatch.setattr(main_window.imgui, "set_item_tooltip", lambda text: None)
    monkeypatch.setattr(main_window.imgui, "same_line", lambda: None)
    monkeypatch.setattr(main_window, "draw_toolbar_help_button", lambda provider, icon_font=None: calls.append(provider))

    main_window._show_toolbar(app, include_help=True)

    assert calls == [app]
