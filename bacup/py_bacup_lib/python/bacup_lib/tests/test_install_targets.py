from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.install_targets import (
    resolve_deploy_and_ini,
    resolve_mo2_profile_ini,
)

_FO4_DATA_DIR = Path("C:/Games/Fallout4/Data")
_DOCS_INI = Path("C:/Users/tester/Documents/My Games/Fallout4/Fallout4Custom.ini")
_VORTEX = "C:/Games/Vortex/fallout4/mods/MyMod"


def _mo2_folder(tmp_path: Path, layout: str) -> Path:
    mod_folder = tmp_path / ("mods" if layout == "mods" else "not_mods") / "SeventySix"
    mod_folder.mkdir(parents=True)
    return mod_folder


@pytest.mark.parametrize(
    ("layout", "organizer_ini", "expected_profile"),
    [
        ("mods", "[General]\nselected_profile=MyProfile\n", "MyProfile"),
        ("mods", '[General]\nselected_profile="MyProfile"\n', "MyProfile"),
        ("mods", None, "Default"),
        ("mods", "[General]\nselected_profile=@ByteArray(\\x8f\\x8e)\n", "Default"),
        ("other", None, None),
    ],
)
def test_resolve_mo2_profile_ini(tmp_path, layout, organizer_ini, expected_profile):
    mod_folder = _mo2_folder(tmp_path, layout)
    if organizer_ini is not None:
        (tmp_path / "ModOrganizer.ini").write_text(organizer_ini, encoding="utf-8")

    result = resolve_mo2_profile_ini(mod_folder)

    if expected_profile is None:
        assert result is None
    else:
        assert result == tmp_path / "profiles" / expected_profile / "fallout4custom.ini"


_PROFILE_INI = "profile"
_MOD_FOLDER = "mod_folder"


@pytest.mark.parametrize(
    ("location", "install_path", "use_profile_ini", "expected"),
    [
        ("game", "", True, (True, None, _DOCS_INI, None)),
        ("  GaRbAgE  ", "", True, (True, None, _DOCS_INI, None)),
        ("  NONE  ", "", True, (False, None, None, None)),
        ("vortex", _VORTEX, True, (True, Path(_VORTEX), _DOCS_INI, None)),
        ("vortex", "   ", True, (False, None, None, "Vortex install folder not set")),
        ("mo2", "", True, (False, None, None, "MO2 mod folder not set")),
        ("mo2", "mods", True, (True, _MOD_FOLDER, _PROFILE_INI, None)),
        ("mo2", "mods", False, (True, _MOD_FOLDER, _DOCS_INI, None)),
        ("mo2", "other", False, (True, _MOD_FOLDER, _DOCS_INI, None)),
        (
            "mo2",
            "other",
            True,
            (
                True,
                _MOD_FOLDER,
                None,
                "Could not derive MO2 profile INI: expected a .../mods/<Name> folder",
            ),
        ),
    ],
)
def test_resolve_deploy_and_ini(tmp_path, location, install_path, use_profile_ini, expected):
    mod_folder = None
    if location == "mo2" and install_path:
        mod_folder = _mo2_folder(tmp_path, install_path)
        (tmp_path / "ModOrganizer.ini").write_text(
            "[General]\nselected_profile=MyProfile\n", encoding="utf-8"
        )
        install_path = str(mod_folder)
    replacements = {
        _MOD_FOLDER: mod_folder,
        _PROFILE_INI: tmp_path / "profiles" / "MyProfile" / "fallout4custom.ini",
    }
    deploy, deploy_data_dir, runtime_ini_path, warning = (
        replacements.get(value, value) if isinstance(value, str) else value
        for value in expected
    )

    result = resolve_deploy_and_ini(
        install_location=location,
        install_path=install_path,
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
        mo2_use_profile_ini=use_profile_ini,
    )

    assert result.deploy is deploy
    assert result.deploy_data_dir == deploy_data_dir
    assert result.runtime_ini_path == runtime_ini_path
    assert result.warning == warning
