from creation_lib.ui.settings import general_section, paths_section
from creation_lib.ui.settings import indexes_section
from creation_lib.ui.shell.settings_window import SettingsWindow
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from bacup_ui.variant import BACUP_VARIANT
from ui.toolkit.variants import get_variant


def _settings(tmp_path, *, variant_id="appalachia"):
    return ToolkitSettings(
        path=tmp_path / f"{variant_id}.json",
        editor_settings_path=tmp_path / "missing-editor-settings.json",
        variant_id=variant_id,
    )


def _window(settings):
    window = SettingsWindow(settings)
    window.register_section(paths_section.make_section(settings))
    return window


class _Workspace:
    id = "appalachia"

    def get_settings_defaults(self):
        return {}

    def apply_settings(self, _settings):
        pass


def test_setup_written_paths_appear_when_paths_settings_open(tmp_path):
    settings = _settings(tmp_path)
    settings.set_game_root_dir("fnv", "C:/Games/Fallout New Vegas")
    settings.set_game_extracted_dir("fnv", "D:/BACUP/extracted/fnv")

    window = _window(settings)
    window.open("paths")

    assert paths_section._state.game_paths["fnv"]["root"] == (
        "C:/Games/Fallout New Vegas"
    )
    assert paths_section._state.game_paths["fnv"]["extracted"] == (
        "D:/BACUP/extracted/fnv"
    )


def test_paths_edits_round_trip_to_canonical_game_paths(tmp_path):
    settings = _settings(tmp_path)
    window = _window(settings)
    window.open("paths")
    paths_section._state.game_paths["skyrimse"].update(
        {
            "root": "C:/Games/Skyrim Special Edition",
            "extracted": "D:/BACUP/extracted/skyrimse",
            "additional": ["E:/SkyrimAssets"],
            "scripts_user_dir": "E:/Scripts/User",
            "scripts_base_dir": "E:/Scripts/Base",
        }
    )
    paths_section._state.script_sources = ["E:/SharedScripts"]

    window._save_settings()

    game_paths = settings.get_game_paths("skyrimse")
    assert game_paths["root_dir"] == "C:/Games/Skyrim Special Edition"
    assert game_paths["extracted_dir"] == "D:/BACUP/extracted/skyrimse"
    assert game_paths["additional_paths"] == ["E:/SkyrimAssets"]
    assert game_paths["scripts_user_dir"] == "E:/Scripts/User"
    assert game_paths["scripts_base_dir"] == "E:/Scripts/Base"
    assert settings.get_script_source_paths() == ["E:/SharedScripts"]


def test_paths_edit_exports_canonical_value_instead_of_stale_window_copy(tmp_path):
    settings = _settings(tmp_path)
    settings.set_game_root_dir("fnv", "C:/Old/Fallout New Vegas")
    env_path = tmp_path / ".env"
    env_path.write_text('FONV_DIR="C:/Old/Fallout New Vegas"\n', encoding="utf-8")
    window = SettingsWindow(settings, env_path=env_path)
    window.register_section(paths_section.make_section(settings))
    window.open("paths")
    paths_section._state.game_paths["fnv"]["root"] = (
        "C:/Games/Fallout New Vegas"
    )

    window._save_settings()

    assert settings.get_game_paths("fnv")["root_dir"] == (
        "C:/Games/Fallout New Vegas"
    )
    assert 'FONV_DIR="C:/Games/Fallout New Vegas"' in env_path.read_text(
        encoding="utf-8"
    )


def test_env_export_keeps_legacy_local_paths_without_registered_section(tmp_path):
    settings = _settings(tmp_path)
    env_path = tmp_path / ".env"
    env_path.write_text("", encoding="utf-8")
    window = SettingsWindow(settings, env_path=env_path)
    window.load_settings()
    window._game_paths["fo3"]["root"] = "C:/Legacy/Fallout 3"

    window._export_to_env()

    assert 'FO3_DIR="C:/Legacy/Fallout 3"' in env_path.read_text(encoding="utf-8")


def test_legacy_env_import_refreshes_registered_canonical_paths(tmp_path):
    settings = _settings(tmp_path)
    env_path = tmp_path / ".env"
    env_path.write_text('FO3_DIR="C:/Imported/Fallout 3"\n', encoding="utf-8")
    window = SettingsWindow(settings, env_path=env_path)
    window.register_section(paths_section.make_section(settings))
    window.open("paths")

    window._import_from_env()

    assert settings.get_game_paths("fo3")["root_dir"] == "C:/Imported/Fallout 3"
    assert paths_section._state.game_paths["fo3"]["root"] == (
        "C:/Imported/Fallout 3"
    )


def test_reopening_paths_refreshes_external_path_changes(tmp_path):
    settings = _settings(tmp_path)
    window = _window(settings)
    window.open("paths")
    window._is_open = False

    settings.set_game_root_dir("fo3", "C:/Games/Fallout 3")
    window.open("paths")

    assert paths_section._state.game_paths["fo3"]["root"] == "C:/Games/Fallout 3"


def test_paths_section_falls_back_to_legacy_section_data():
    section = paths_section.make_section()
    section.load(
        {
            "fo4": {
                "root_dir": "C:/Legacy/FO4",
                "extracted_dir": "D:/Legacy/FO4",
            }
        }
    )

    assert paths_section._state.game_paths["fo4"]["root"] == "C:/Legacy/FO4"
    assert paths_section._state.game_paths["fo4"]["extracted"] == "D:/Legacy/FO4"
    assert section.save()["fo4"]["root_dir"] == "C:/Legacy/FO4"


def test_empty_canonical_paths_migrate_from_real_legacy_section_data(tmp_path):
    settings = _settings(tmp_path)
    settings.set_settings_section(
        "paths",
        {
            "fo3": {
                "root_dir": "C:/Legacy/Fallout 3",
                "extracted_dir": "D:/Legacy/extracted/fo3",
            },
            "script_sources": ["E:/LegacyScripts"],
        },
    )

    window = _window(settings)
    window.open("paths")

    assert paths_section._state.game_paths["fo3"]["root"] == (
        "C:/Legacy/Fallout 3"
    )
    assert paths_section._state.game_paths["fo3"]["extracted"] == (
        "D:/Legacy/extracted/fo3"
    )
    assert paths_section._state.script_sources == ["E:/LegacyScripts"]

    window._save_settings()
    assert settings.get_game_paths("fo3")["root_dir"] == "C:/Legacy/Fallout 3"
    assert settings.get_script_source_paths() == ["E:/LegacyScripts"]


def test_populated_canonical_paths_win_over_legacy_section_data(tmp_path):
    settings = _settings(tmp_path)
    settings.set_game_root_dir("fo76", "C:/Canonical/Fallout 76")
    settings.set_script_source_paths(["E:/CanonicalScripts"])
    settings.set_settings_section(
        "paths",
        {
            "fo76": {"root_dir": "C:/Legacy/Fallout 76"},
            "script_sources": ["E:/LegacyScripts"],
        },
    )

    window = _window(settings)
    window.open("paths")

    assert paths_section._state.game_paths["fo76"]["root"] == (
        "C:/Canonical/Fallout 76"
    )
    assert paths_section._state.script_sources == ["E:/CanonicalScripts"]


def test_mixed_canonical_and_legacy_fields_merge_without_losing_either(tmp_path):
    settings = _settings(tmp_path)
    settings.set_game_root_dir("skyrimse", "C:/Canonical/Skyrim")
    settings.set_settings_section(
        "paths",
        {
            "skyrimse": {
                "root_dir": "C:/Legacy/Skyrim",
                "extracted_dir": "D:/Legacy/extracted/skyrimse",
                "additional_paths": ["E:/LegacyAssets"],
                "scripts_user_dir": "E:/LegacyScripts/User",
                "scripts_base_dir": "E:/LegacyScripts/Base",
            }
        },
    )

    window = _window(settings)
    window.open("paths")

    game_paths = paths_section._state.game_paths["skyrimse"]
    assert game_paths["root"] == "C:/Canonical/Skyrim"
    assert game_paths["extracted"] == "D:/Legacy/extracted/skyrimse"
    assert game_paths["additional"] == ["E:/LegacyAssets"]
    assert game_paths["scripts_user_dir"] == "E:/LegacyScripts/User"
    assert game_paths["scripts_base_dir"] == "E:/LegacyScripts/Base"

    window._save_settings()
    canonical = settings.get_game_paths("skyrimse")
    assert canonical["root_dir"] == "C:/Canonical/Skyrim"
    assert canonical["extracted_dir"] == "D:/Legacy/extracted/skyrimse"
    assert canonical["additional_paths"] == ["E:/LegacyAssets"]
    assert settings.get_settings_section("paths") == {}

    settings.set_game_extracted_dir("skyrimse", "")
    window._is_open = False
    window.open("paths")
    assert paths_section._state.game_paths["skyrimse"]["extracted"] == ""


def test_general_env_import_refreshes_registered_paths_without_save_clobber(tmp_path):
    settings = _settings(tmp_path)
    settings.set_game_root_dir("fnv", "C:/Old/Fallout New Vegas")
    app = ToolkitApp(
        [_Workspace()], settings, app_variant=BACUP_VARIANT
    )
    env_path = tmp_path / ".env"
    env_path.write_text('FONV_DIR="C:/Imported/Fallout New Vegas"\n', encoding="utf-8")
    general_section._state.env_path = env_path
    app._settings_window._env_path = env_path

    general_section._import_from_env(settings)

    assert settings.get_game_paths("fnv")["root_dir"] == (
        "C:/Imported/Fallout New Vegas"
    )
    assert paths_section._state.game_paths["fnv"]["root"] == (
        "C:/Imported/Fallout New Vegas"
    )
    app._settings_window._save_settings()
    assert settings.get_game_paths("fnv")["root_dir"] == (
        "C:/Imported/Fallout New Vegas"
    )


def test_general_env_export_commits_unsaved_registered_paths(tmp_path):
    settings = _settings(tmp_path)
    ToolkitApp(
        [_Workspace()], settings, app_variant=BACUP_VARIANT
    )
    env_path = tmp_path / ".env"
    env_path.write_text("", encoding="utf-8")
    general_section._state.env_path = env_path
    paths_section._state.game_paths["skyrimse"]["root"] = (
        "C:/Games/Skyrim Special Edition"
    )

    general_section._export_to_env(settings)

    assert settings.get_game_paths("skyrimse")["root_dir"] == (
        "C:/Games/Skyrim Special Edition"
    )
    assert 'SKYRIMSE_DIR="C:/Games/Skyrim Special Edition"' in env_path.read_text(
        encoding="utf-8"
    )


def test_bacup_registers_paths_and_shared_extraction_settings(tmp_path):
    settings = _settings(tmp_path)
    app = ToolkitApp(
        [_Workspace()], settings, app_variant=BACUP_VARIANT
    )

    assert [section.id for section in app._settings_window._sections] == [
        "general",
        "paths",
        "indexes",
    ]
    extraction = app._settings_window._sections[-1]
    assert extraction.label == "Extraction"
    assert extraction.draw is indexes_section._draw_extraction_only
    assert {"fo4", "fo76", "fnv", "fo3", "skyrimse"}.issubset(
        {game_id for game_id, _ in indexes_section._INDEX_GAMES}
    )


def test_nif_variant_keeps_extraction_settings_disabled(tmp_path):
    settings = _settings(tmp_path, variant_id="nif")
    app = ToolkitApp([_Workspace()], settings, app_variant=get_variant("nif"))

    assert [section.id for section in app._settings_window._sections] == [
        "general",
        "paths",
    ]
