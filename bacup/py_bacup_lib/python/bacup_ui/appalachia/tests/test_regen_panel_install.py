from pathlib import Path

import pytest

from bacup_ui.appalachia.tests.test_regen_panel_options import _panel, _ws
from bacup_ui.conversion.panels.regen_panel import RegenPanel

_DOCS = Path.home() / "Documents" / "My Games" / "Fallout4"


@pytest.mark.parametrize("saved_location, expected", [
    (None, "mo2"),
    ("unknown", "mo2"),
    ("mo2", "mo2"),
    ("game", "game"),
    ("vortex", "vortex"),
    ("none", "none"),
])
def test_install_location_defaults_to_folder_and_preserves_saved_choices(saved_location, expected):
    settings = {} if saved_location is None else {"install_location": saved_location}
    panel = RegenPanel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76", settings))

    assert panel.install_location == expected


@pytest.mark.parametrize("location", ["mo2", "vortex"])
def test_missing_install_folder_disables_actions(location):
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.install_location = location
    panel.install_path = "   "

    assert panel.can_convert() is False
    assert panel.can_deploy_existing() is False
    assert panel.can_undeploy() is False
    assert "Choose an install folder" in panel._deploy_existing_hint()


@pytest.mark.parametrize("action", [
    "start_conversion", "start_deploy_existing", "start_undeploy", "start_resume_from_phase",
])
def test_default_folder_without_path_cannot_fall_back_to_game_directory(action):
    panel = RegenPanel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))

    with pytest.raises(ValueError, match="Choose an install folder"):
        getattr(panel, action)()


def test_folder_install_outside_mo2_uses_global_ini(tmp_path, monkeypatch):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    install_path = tmp_path / "My converted mod"
    panel = RegenPanel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76", {
        "install_path": str(install_path),
    }))

    paths = panel.build_paths()

    assert paths.deploy_data_dir == install_path
    assert paths.runtime_ini_path == _DOCS / "Fallout4Custom.ini"


@pytest.mark.parametrize("location, use_profile_ini, expected_ini", [
    ("mo2", True, "profile"),
    ("mo2", False, "docs"),
    ("vortex", True, "docs"),
])
def test_mod_manager_install_resolves_deploy_dir_and_runtime_ini(
    tmp_path, monkeypatch, location, use_profile_ini, expected_ini
):
    monkeypatch.setattr("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: Path("X:/app"))
    install_path = tmp_path / "mods" / "SeventySix"
    install_path.mkdir(parents=True)
    (tmp_path / "ModOrganizer.ini").write_text(
        "[General]\nselected_profile=MyProfile\n", encoding="utf-8"
    )
    panel = _panel(_ws("C:/FO4", "C:/FO76", "C:/x/fo76"))
    panel.install_location = location
    panel.install_path = str(install_path)
    panel.mo2_use_profile_ini = use_profile_ini

    paths = panel.build_paths()

    assert paths.deploy_data_dir == install_path
    assert paths.runtime_ini_path == (
        tmp_path / "profiles" / "MyProfile" / "fallout4custom.ini"
        if expected_ini == "profile"
        else _DOCS / "Fallout4Custom.ini"
    )
