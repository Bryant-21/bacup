import json

from ui.toolkit.settings import ToolkitSettings


def test_settings_persist_shared_and_variant_files(tmp_path):
    shared_path = tmp_path / "shared_settings.json"
    variant_path = tmp_path / "variants" / "materials.json"

    settings = ToolkitSettings(
        shared_path=shared_path,
        variant_path=variant_path,
        variant_id="materials",
        editor_settings_path=tmp_path / "missing_editor_settings.json",
    )
    settings.active_game = "starfield"
    settings.mod_prefix = "B21"
    settings.theme = "falloutnv"
    settings.window_width = 1280
    settings.window_height = 720
    settings.active_workspace = "materials"
    settings._paths["starfield"]["root_dir"] = "N:/Steam/Starfield"
    settings.set_workspace_settings("materials", {"recent_files": ["a.bgsm"]})

    settings.save()

    shared = json.loads(shared_path.read_text(encoding="utf-8"))
    variant = json.loads(variant_path.read_text(encoding="utf-8"))

    assert shared["active_game"] == "starfield"
    assert shared["mod_prefix"] == "B21"
    assert shared["theme"] == "falloutnv"
    assert shared["paths"]["starfield"]["root_dir"] == "N:/Steam/Starfield"
    assert "window_width" not in shared
    assert "workspaces" not in shared

    assert variant["variant_id"] == "materials"
    assert variant["active_workspace"] == "materials"
    assert variant["window_width"] == 1280
    assert variant["window_height"] == 720
    assert variant["workspaces"]["materials"] == {"recent_files": ["a.bgsm"]}
    assert "paths" not in variant


def test_settings_load_shared_values_across_variants(tmp_path):
    shared_path = tmp_path / "shared_settings.json"
    first_variant = tmp_path / "variants" / "nif.json"
    second_variant = tmp_path / "variants" / "papyrus.json"

    nif = ToolkitSettings(
        shared_path=shared_path,
        variant_path=first_variant,
        variant_id="nif",
        editor_settings_path=tmp_path / "missing_editor_settings.json",
    )
    nif.active_game = "fo76"
    nif.mod_prefix = "B21"
    nif._paths["fo76"]["root_dir"] = "N:/Steam/Fallout76"
    nif.set_workspace_settings("nif", {"fov": 60.0})
    nif.save()

    papyrus = ToolkitSettings(
        shared_path=shared_path,
        variant_path=second_variant,
        variant_id="papyrus",
        editor_settings_path=tmp_path / "missing_editor_settings.json",
    )

    assert papyrus.active_game == "fo76"
    assert papyrus.mod_prefix == "B21"
    assert papyrus.get_game_paths("fo76")["root_dir"] == "N:/Steam/Fallout76"
    assert papyrus.get_workspace_settings("nif") == {}
