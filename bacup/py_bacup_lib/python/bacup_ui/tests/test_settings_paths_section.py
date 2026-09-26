from creation_lib.ui.settings import paths_section
from creation_lib.ui.shell.settings_window import SettingsWindow
from ui.toolkit.settings import ToolkitSettings


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


def test_paths_edits_round_trip_to_canonical_game_paths(tmp_path):
    settings = _settings(tmp_path)
    settings.set_game_root_dir("fnv", "C:/Games/Fallout New Vegas")
    window = _window(settings)
    window.open("paths")
    assert paths_section._state.game_paths["fnv"]["root"] == "C:/Games/Fallout New Vegas"
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
