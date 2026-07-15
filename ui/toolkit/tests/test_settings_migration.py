import json

from ui.toolkit.settings_migration import migrate_legacy_settings


def _legacy_settings():
    return {
        "active_workspace": "nif",
        "active_game": "fo4",
        "window_width": 1600,
        "window_height": 900,
        "setup_complete": True,
        "mod_prefix": "B21",
        "theme": "falloutnv",
        "paths": {
            "fo4": {
                "root_dir": "N:/Steam/Fallout 4",
                "extracted_dir": "X:/extracted/fo4",
                "additional_paths": [],
                "scripts_user_dir": "",
                "scripts_base_dir": "",
            },
            "script_source_paths": ["X:/scripts"],
        },
        "indexes": {"fo4_data": True, "nifs": True},
        "tools": {"spriggit_cli": "C:/tools/SpriggitCLI.exe"},
        "gitea": {"url": "https://example.test", "username": "alice"},
        "workspaces": {
            "nif": {"fov": 45.0},
            "bsa_viewer": {"recent_archives": ["Fallout4.ba2"]},
            "cloth_maker": {"show_particles": True},
            "weight_painter": {"brush_radius": 8.0},
            "papyrus": {"font_size": 14},
            "conversion": {"source_game": "fo76"},
            "materials": {"recent_files": ["a.bgsm"]},
            "esp_editor": {"recent_files": ["a.esp"]},
            "mod_builder": {"transcription_fallback": "parakeet"},
        },
    }


def test_migrate_legacy_settings_writes_shared_and_variant_files(tmp_path):
    legacy_path = tmp_path / "toolkit_settings.json"
    legacy_path.write_text(json.dumps(_legacy_settings()), encoding="utf-8")

    result = migrate_legacy_settings(
        legacy_path,
        output_dir=tmp_path / "settings_data",
        timestamp="20260503-120000",
    )

    assert result.backup_path == tmp_path / "toolkit_settings.20260503-120000.json.bak"
    assert result.backup_path.is_file()
    assert result.shared_path == tmp_path / "settings_data" / "shared_settings.json"
    assert result.variant_paths["nif"] == tmp_path / "settings_data" / "variants" / "nif.json"

    shared = json.loads(result.shared_path.read_text(encoding="utf-8"))
    assert shared["active_game"] == "fo4"
    assert shared["mod_prefix"] == "B21"
    assert shared["theme"] == "falloutnv"
    assert shared["paths"]["fo4"]["root_dir"] == "N:/Steam/Fallout 4"
    assert shared["paths"]["script_source_paths"] == ["X:/scripts"]
    assert shared["gitea"]["username"] == "alice"
    assert "workspaces" not in shared

    nif = json.loads(result.variant_paths["nif"].read_text(encoding="utf-8"))
    assert nif["variant_id"] == "nif"
    assert nif["active_workspace"] == "nif"
    assert nif["window_width"] == 1600
    assert nif["workspaces"] == {"nif": {"fov": 45.0}}

    full = json.loads(result.variant_paths["full"].read_text(encoding="utf-8"))
    assert full["variant_id"] == "full"
    assert full["active_workspace"] == "nif"
    assert full["workspaces"]["mod_builder"] == {"transcription_fallback": "parakeet"}
