from __future__ import annotations

from types import SimpleNamespace

from ui.tools.materials.texture_upgrader import TextureUpgraderTool, game_data_dir


def test_game_data_dir_comes_from_the_fo4_root():
    settings = SimpleNamespace(get_game_paths=lambda game: {"root_dir": "C:/Games/Fallout 4"} if game == "fo4" else {})
    assert game_data_dir(settings).replace("\\", "/") == "C:/Games/Fallout 4/Data"
    assert game_data_dir(SimpleNamespace(get_game_paths=lambda game: {})) == ""
    assert game_data_dir(None) == ""


def test_upgrade_refuses_without_a_mod_folder(tmp_path):
    tool = TextureUpgraderTool()
    tool._data_dir = str(tmp_path)
    tool._validate_and_start()
    assert tool._error_msg == "Select a mod folder." and not tool._running


def test_workspace_is_registered():
    from ui.toolkit.workspaces import _WORKSPACE_SPECS

    assert ("texture_upgrader", "ui.toolkit.workspaces.texture_tools", "TextureUpgraderWorkspace") in _WORKSPACE_SPECS
