from __future__ import annotations

from ui.toolkit.workspaces.nif_workspace import NifWorkspace


def test_draw_menu_bootstraps_nif_panels_when_toolbar_missing(monkeypatch):
    ws = NifWorkspace()
    calls = {"init": 0, "draw_menu_items": 0, "import_modal": 0}

    class FakeToolbar:
        def draw_menu_items(self, include_help=True):
            assert include_help is False
            calls["draw_menu_items"] += 1

        def _render_import_options_modal(self):
            calls["import_modal"] += 1

    class FakeApp:
        def _init_panels(self):
            calls["init"] += 1
            self.toolbar = FakeToolbar()

    ws._app = FakeApp()
    ws._view_helper = None
    monkeypatch.setattr(
        "ui.toolkit.workspaces.nif_workspace.imgui.begin_menu",
        lambda *args, **kwargs: False,
    )

    ws.draw_menu()

    assert calls["init"] == 1
    assert calls["draw_menu_items"] == 1
    assert calls["import_modal"] == 1
