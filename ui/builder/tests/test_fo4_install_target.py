from pathlib import Path
from unittest.mock import patch

from ui.builder.mod_builder_app import ModBuilderApp


class _FakeSettings:
    def __init__(self, choices, paths=None, workspace=None):
        self._choices = choices
        self._paths = paths or {}
        self._workspace = workspace or {}

    def get_fo4_install_choices(self):
        return self._choices

    def get_game_paths(self, game):
        return self._paths.get(game, {"root_dir": ""})

    def get_workspace_settings(self, workspace_id):
        return dict(self._workspace)


def _app(settings):
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        return ModBuilderApp(settings)


def test_resolve_fo4_data_uses_selected_extra_install(tmp_path):
    choices = [
        {"label": "Primary install", "root_dir": str(tmp_path / "FO4"), "primary": True},
        {"label": "NextGen", "root_dir": str(tmp_path / "FO4NG"), "primary": False},
    ]
    app = _app(_FakeSettings(choices, {"fo4": {"root_dir": str(tmp_path / "FO4")}}))
    app._refresh_fo4_install_choices()
    app._fo4_install_idx = 1

    assert app._resolve_game_data_path("fo4") == tmp_path / "FO4NG" / "Data"
    assert app._resolve_deploy_data_path("fo4") == tmp_path / "FO4NG" / "Data"
    assert app._resolve_game_dir_path("fo4") == tmp_path / "FO4NG"


def test_resolve_fo4_data_uses_primary_at_index_zero(tmp_path):
    choices = [
        {"label": "Primary install", "root_dir": str(tmp_path / "FO4"), "primary": True},
        {"label": "NextGen", "root_dir": str(tmp_path / "FO4NG"), "primary": False},
    ]
    app = _app(_FakeSettings(choices, {"fo4": {"root_dir": str(tmp_path / "FO4")}}))
    app._refresh_fo4_install_choices()
    app._fo4_install_idx = 0

    assert app._resolve_game_data_path("fo4") == tmp_path / "FO4" / "Data"


def test_resolve_non_fo4_ignores_fo4_install_selection(tmp_path):
    choices = [
        {"label": "Primary install", "root_dir": str(tmp_path / "FO4"), "primary": True},
        {"label": "NextGen", "root_dir": str(tmp_path / "FO4NG"), "primary": False},
    ]
    paths = {
        "fo4": {"root_dir": str(tmp_path / "FO4")},
        "skyrimse": {"root_dir": str(tmp_path / "Skyrim")},
    }
    app = _app(_FakeSettings(choices, paths))
    app._refresh_fo4_install_choices()
    app._fo4_install_idx = 1  # a non-primary FO4 install is selected

    assert app._resolve_game_data_path("skyrimse") == tmp_path / "Skyrim" / "Data"


def test_install_choices_absent_settings_method_is_safe(tmp_path):
    class Bare:
        def get_workspace_settings(self, workspace_id):
            return {}

        def get_game_paths(self, game):
            return {"root_dir": str(tmp_path / "FO4")}

    app = _app(Bare())
    app._refresh_fo4_install_choices()

    assert app._fo4_install_choices == []
    assert app._selected_fo4_install_root() is None
    assert app._resolve_game_data_path("fo4") == tmp_path / "FO4" / "Data"


def test_resolve_fo4_deploy_data_uses_mo2_virtual_data_folder(tmp_path):
    mo2_dir = tmp_path / "ModOrganizer" / "mods" / "SeventySix"
    settings = _FakeSettings(
        [{"label": "Primary install", "root_dir": str(tmp_path / "FO4"), "primary": True}],
        {"fo4": {"root_dir": str(tmp_path / "FO4")}},
        {"deploy_to_mo2": True, "mo2_deploy_dir": str(mo2_dir)},
    )
    app = _app(settings)

    assert app._resolve_game_data_path("fo4") == tmp_path / "FO4" / "Data"
    assert app._resolve_deploy_data_path("fo4") == mo2_dir
    assert app._resolve_game_dir_path("fo4") == tmp_path / "FO4"
