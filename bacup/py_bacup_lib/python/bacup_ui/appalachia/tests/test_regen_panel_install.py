from pathlib import Path

import pytest

from bacup_ui.appalachia.tests.test_regen_panel_options import _panel, _ws

_DOCS = Path.home() / "Documents" / "My Games" / "Fallout4"


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
