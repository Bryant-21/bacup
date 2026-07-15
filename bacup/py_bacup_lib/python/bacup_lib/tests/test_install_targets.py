from __future__ import annotations

from pathlib import Path

from bacup_lib.install_targets import (
    resolve_deploy_and_ini,
    resolve_mo2_profile_ini,
)

_FO4_DATA_DIR = Path("C:/Games/Fallout4/Data")
_DOCS_INI = Path("C:/Users/tester/Documents/My Games/Fallout4/Fallout4Custom.ini")


def _make_mo2_mod_folder(tmp_path: Path, name: str = "SeventySix") -> Path:
    mod_folder = tmp_path / "mods" / name
    mod_folder.mkdir(parents=True)
    return mod_folder


def test_resolve_mo2_profile_ini_reads_selected_profile(tmp_path):
    mod_folder = _make_mo2_mod_folder(tmp_path)
    (tmp_path / "ModOrganizer.ini").write_text(
        "[General]\nselected_profile=MyProfile\n", encoding="utf-8"
    )

    result = resolve_mo2_profile_ini(mod_folder)

    assert result == tmp_path / "profiles" / "MyProfile" / "fallout4custom.ini"


def test_resolve_mo2_profile_ini_strips_quotes(tmp_path):
    mod_folder = _make_mo2_mod_folder(tmp_path)
    (tmp_path / "ModOrganizer.ini").write_text(
        '[General]\nselected_profile="MyProfile"\n', encoding="utf-8"
    )

    result = resolve_mo2_profile_ini(mod_folder)

    assert result == tmp_path / "profiles" / "MyProfile" / "fallout4custom.ini"


def test_resolve_mo2_profile_ini_missing_ini_falls_back_to_default(tmp_path):
    mod_folder = _make_mo2_mod_folder(tmp_path)

    result = resolve_mo2_profile_ini(mod_folder)

    assert result == tmp_path / "profiles" / "Default" / "fallout4custom.ini"


def test_resolve_mo2_profile_ini_bytearray_value_falls_back_to_default(tmp_path):
    mod_folder = _make_mo2_mod_folder(tmp_path)
    (tmp_path / "ModOrganizer.ini").write_text(
        "[General]\nselected_profile=@ByteArray(\\x8f\\x8e)\n", encoding="utf-8"
    )

    result = resolve_mo2_profile_ini(mod_folder)

    assert result == tmp_path / "profiles" / "Default" / "fallout4custom.ini"


def test_resolve_mo2_profile_ini_non_mods_layout_returns_none(tmp_path):
    mod_folder = tmp_path / "not_mods" / "SeventySix"
    mod_folder.mkdir(parents=True)

    assert resolve_mo2_profile_ini(mod_folder) is None


def test_resolve_deploy_and_ini_game_mode():
    result = resolve_deploy_and_ini(
        install_location="game",
        install_path="",
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
    )

    assert result.deploy is True
    assert result.deploy_data_dir is None
    assert result.runtime_ini_path == _DOCS_INI
    assert result.warning is None


def test_resolve_deploy_and_ini_vortex_with_path():
    install_path = "C:/Games/Vortex/fallout4/mods/MyMod"
    result = resolve_deploy_and_ini(
        install_location="vortex",
        install_path=install_path,
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
    )

    assert result.deploy is True
    assert result.deploy_data_dir == Path(install_path)
    assert result.runtime_ini_path == _DOCS_INI
    assert result.warning is None


def test_resolve_deploy_and_ini_vortex_without_path():
    result = resolve_deploy_and_ini(
        install_location="vortex",
        install_path="   ",
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
    )

    assert result.deploy is False
    assert result.deploy_data_dir is None
    assert result.runtime_ini_path is None
    assert result.warning == "Vortex install folder not set"


def test_resolve_deploy_and_ini_mo2_with_valid_layout(tmp_path):
    mod_folder = _make_mo2_mod_folder(tmp_path)
    (tmp_path / "ModOrganizer.ini").write_text(
        "[General]\nselected_profile=MyProfile\n", encoding="utf-8"
    )

    result = resolve_deploy_and_ini(
        install_location="mo2",
        install_path=str(mod_folder),
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
    )

    assert result.deploy is True
    assert result.deploy_data_dir == mod_folder
    assert result.runtime_ini_path == tmp_path / "profiles" / "MyProfile" / "fallout4custom.ini"
    assert result.warning is None


def test_resolve_deploy_and_ini_mo2_global_ini_with_valid_layout(tmp_path):
    mod_folder = _make_mo2_mod_folder(tmp_path)
    (tmp_path / "ModOrganizer.ini").write_text(
        "[General]\nselected_profile=MyProfile\n", encoding="utf-8"
    )

    result = resolve_deploy_and_ini(
        install_location="mo2",
        install_path=str(mod_folder),
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
        mo2_use_profile_ini=False,
    )

    assert result.deploy is True
    assert result.deploy_data_dir == mod_folder
    assert result.runtime_ini_path == _DOCS_INI
    assert result.warning is None


def test_resolve_deploy_and_ini_mo2_global_ini_ignores_non_mods_layout(tmp_path):
    mod_folder = tmp_path / "not_mods" / "SeventySix"
    mod_folder.mkdir(parents=True)

    result = resolve_deploy_and_ini(
        install_location="mo2",
        install_path=str(mod_folder),
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
        mo2_use_profile_ini=False,
    )

    assert result.deploy is True
    assert result.deploy_data_dir == mod_folder
    assert result.runtime_ini_path == _DOCS_INI
    assert result.warning is None


def test_resolve_deploy_and_ini_mo2_without_path():
    result = resolve_deploy_and_ini(
        install_location="mo2",
        install_path="",
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
    )

    assert result.deploy is False
    assert result.deploy_data_dir is None
    assert result.runtime_ini_path is None
    assert result.warning == "MO2 mod folder not set"


def test_resolve_deploy_and_ini_mo2_non_mods_layout_warns(tmp_path):
    mod_folder = tmp_path / "not_mods" / "SeventySix"
    mod_folder.mkdir(parents=True)

    result = resolve_deploy_and_ini(
        install_location="mo2",
        install_path=str(mod_folder),
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
    )

    assert result.deploy is True
    assert result.deploy_data_dir == mod_folder
    assert result.runtime_ini_path is None
    assert result.warning == "Could not derive MO2 profile INI: expected a .../mods/<Name> folder"


def test_resolve_deploy_and_ini_none_mode():
    result = resolve_deploy_and_ini(
        install_location="none",
        install_path="",
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
    )

    assert result.deploy is False
    assert result.deploy_data_dir is None
    assert result.runtime_ini_path is None
    assert result.warning is None


def test_resolve_deploy_and_ini_unknown_mode_treated_as_game():
    result = resolve_deploy_and_ini(
        install_location="  GaRbAgE  ",
        install_path="",
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
    )

    assert result.deploy is True
    assert result.deploy_data_dir is None
    assert result.runtime_ini_path == _DOCS_INI
    assert result.warning is None


def test_resolve_deploy_and_ini_case_insensitive_mode():
    result = resolve_deploy_and_ini(
        install_location="  NONE  ",
        install_path="",
        fo4_data_dir=_FO4_DATA_DIR,
        docs_custom_ini=_DOCS_INI,
    )

    assert result.deploy is False
