from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_ui.appalachia.appalachia_workspace import AppalachiaWorkspace
from bacup_ui.conversion.panels.regen_panel import RegenPanel


class _Settings:
    def __init__(self):
        self.workspaces = {"appalachia": {}}
        self.paths = {
            game: {"root_dir": f"C:/{game}", "extracted_dir": f"C:/{game}/Data"}
            for game in ("fo4", "fo76", "fnv", "fo3", "skyrimse", "starfield")
        }

    def get_workspace_settings(self, workspace_id):
        return dict(self.workspaces.get(workspace_id, {}))

    def set_workspace_settings(self, workspace_id, values):
        self.workspaces.setdefault(workspace_id, {}).update(values)

    def get_game_paths(self, game_id):
        return dict(self.paths.get(game_id, {}))


def test_shared_runner_rejects_concurrent_projects():
    workspace = AppalachiaWorkspace(_Settings())
    first = SimpleNamespace(done=False, start=lambda: None)
    workspace.start_conversion_runner(object(), first)

    with pytest.raises(RuntimeError, match="already running"):
        workspace.start_conversion_runner(
            object(),
            SimpleNamespace(done=False, start=lambda: None),
        )


def test_fixed_projects_resolve_pair_specific_output_plugins(monkeypatch):
    settings = _Settings()
    settings.paths["fo3"]["extracted_dir"] = "C:/x/fo3"
    workspace = SimpleNamespace(_toolkit_settings=settings, _runner=None)
    monkeypatch.setattr(
        "bacup_ui.conversion.panels.regen_panel.get_exe_dir",
        lambda: Path("X:/BACUP"),
    )
    wasteland = RegenPanel(
        workspace,
        fixed_pair_id="fnvfo3:fo4",
        project_id="wasteland",
    )
    north = RegenPanel(
        workspace,
        fixed_pair_id="skyrimse:fo4",
        project_id="north",
    )

    wasteland_paths = wasteland.build_paths()
    north_paths = north.build_paths()
    assert wasteland.generated_plugin_path().name == "FalloutNV.esm"
    assert north.generated_plugin_path().name == "Skyrim.esm"
    assert wasteland_paths.additional_source_asset_roots == (
        Path("C:/x/fo3"),
        Path("C:/fo3/Data"),
    )
    assert north_paths.additional_source_asset_roots == ()
    assert wasteland._required_game_ids() == ("fo4", "fnv", "fo3")
    assert north._required_game_ids() == ("fo4", "skyrimse")


@pytest.mark.parametrize("pair_id, project_id, source_game", [
    ("fo76:fo4", "appalachia", "fo76"),
    ("fnvfo3:fo4", "wasteland", "fo3"),
])
def test_can_convert_needs_source_extractions_but_not_fo4_extraction(pair_id, project_id, source_game):
    settings = _Settings()
    settings.paths["fo4"]["extracted_dir"] = ""
    workspace = SimpleNamespace(_toolkit_settings=settings, _runner=None)
    panel = RegenPanel(workspace, fixed_pair_id=pair_id, project_id=project_id)
    panel._store_installs_ok = lambda: True

    assert panel.can_convert() is True

    settings.paths[source_game]["extracted_dir"] = ""
    assert panel.can_convert() is False
