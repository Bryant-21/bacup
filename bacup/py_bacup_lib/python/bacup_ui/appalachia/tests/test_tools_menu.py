from types import SimpleNamespace

import bacup_ui.appalachia.appalachia_workspace as mod
from bacup_ui.appalachia.appalachia_workspace import AppalachiaWorkspace


class _Settings:
    def __init__(self, workspace=None):
        self.workspace = workspace or {}

    def get_workspace_settings(self, _workspace_id):
        return dict(self.workspace)

    def get_game_paths(self, _game_id):
        return {}


def _menu_names(monkeypatch, workspace, clicked=None):
    names = []

    def begin_menu(name, *_):
        names.append(name)
        return name == "Tools"

    monkeypatch.setattr(mod.imgui, "begin_menu", begin_menu)
    monkeypatch.setattr(mod.imgui, "end_menu", lambda: None)
    monkeypatch.setattr(mod.imgui, "menu_item", lambda label, *_: (label == clicked, False))
    workspace.draw_menu()
    return names


def test_tools_menu_follows_setup(monkeypatch):
    names = _menu_names(monkeypatch, AppalachiaWorkspace(_Settings()))
    assert names.index("Tools") == names.index("Setup") + 1


def test_tales_catalogs_item_opens_the_editor_window_with_saved_settings(monkeypatch):
    saved = {"last_plugin": "C:/mods/Mod.esp", "extra_master_dirs": ["D:/Masters"],
             "panes": {"features": 0.2, "editor": 0.5}, "editor_popped_out": False}
    workspace = AppalachiaWorkspace(_Settings({"catalog_editor": saved}))
    assert workspace._catalog_tool is None
    _menu_names(monkeypatch, workspace, clicked="Tales Catalogs...")
    tool = workspace._catalog_tool
    try:
        assert tool.visible
        assert tool.collect_settings() == saved
        assert workspace.collect_settings() == {"catalog_editor": saved}
    finally:
        workspace.cleanup()


def test_catalog_output_dir_comes_from_the_active_project(monkeypatch):
    workspace = AppalachiaWorkspace(_Settings())
    assert workspace._catalog_output_dir() == ""
    workspace._regen_panel = SimpleNamespace(build_paths=lambda: SimpleNamespace(output_root="X:/BACUP/mods/SeventySix"))
    assert workspace._catalog_output_dir() == "X:/BACUP/mods/SeventySix"
