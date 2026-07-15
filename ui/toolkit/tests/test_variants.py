from ui.toolkit.variants import (
    FIRST_STANDALONE_VARIANT_IDS,
    get_release_asset_names,
    get_variant,
    variant_id_from_exe_name,
)
from ui.toolkit.app import TEXTURE_WORKSPACE_IDS
from ui.toolkit.__main__ import (
    _parse_main_args,
    _prepare_first_run_settings,
)
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.workspaces import create_workspaces


def test_first_standalone_variants_match_creator_skus():
    assert FIRST_STANDALONE_VARIANT_IDS == (
        "nif",
        "bsa_viewer",
        "cloth_maker",
        "weight_painter",
        "papyrus",
        "materials",
        "esp_editor",
        "world_viewer",
    )


def test_variant_registry_defines_titles_and_defaults():
    nif = get_variant("nif")
    full = get_variant("full")

    assert nif.exe_name == "ModBox21-NIF"
    assert nif.window_title == "ModBox21 - NIF Editor"
    assert nif.icon_path == "resource/icons/modbox21-nif.ico"
    assert nif.workspace_ids == ("nif",)
    assert nif.default_workspace == "nif"
    assert nif.include_ai_panel is False
    assert nif.include_index_settings is False

    assert full.exe_name == "ModBox21"
    assert full.workspace_ids is None
    assert full.icon_path == "resource/icon.ico"
    assert full.include_ai_panel is True
    assert full.include_index_settings is True
    assert full.extraction_only_settings is False

def test_create_workspaces_filters_to_requested_ids():
    workspaces = create_workspaces(workspace_ids=("materials", "bsa_viewer"))

    assert [workspace.id for workspace in workspaces] == ["materials", "bsa_viewer"]


def test_worldspace_export_workspace_is_registered():
    workspaces = create_workspaces(workspace_ids=("worldspace_export",))

    assert [workspace.id for workspace in workspaces] == ["worldspace_export"]
    assert workspaces[0].name == "Worldspace Export"


def test_world_viewer_workspace_is_registered():
    workspaces = create_workspaces(workspace_ids=("world_viewer",))

    assert [workspace.id for workspace in workspaces] == ["world_viewer"]
    assert workspaces[0].name == "World Viewer"


def test_texture_menu_workspace_ids_are_registered():
    workspace_ids = {workspace.id for workspace in create_workspaces()}

    assert set(TEXTURE_WORKSPACE_IDS).issubset(workspace_ids)
    assert "image_utils" in TEXTURE_WORKSPACE_IDS


def test_release_asset_names_use_variant_exe_names():
    names = get_release_asset_names("2.4.6")

    assert names["full"] == "ModBox21-2.4.6.zip"
    assert names["bsa_viewer"] == "ModBox21-BSAViewer-2.4.6.zip"
    assert names["esp_editor"] == "ModBox21-ESPEditor-2.4.6.zip"
    assert names["world_viewer"] == "ModBox21-WorldViewer-2.4.6.zip"


def test_variant_id_can_be_inferred_from_frozen_exe_name():
    assert variant_id_from_exe_name("ModBox21") == "full"
    assert variant_id_from_exe_name("ModBox21-NIF") == "nif"
    assert variant_id_from_exe_name("modbox21-espeditor") == "esp_editor"
    assert variant_id_from_exe_name("modbox21-worldviewer") == "world_viewer"
    assert variant_id_from_exe_name("UnknownTool") is None


def test_world_viewer_variant_is_registered():
    variant = get_variant("world_viewer")

    assert variant.exe_name == "ModBox21-WorldViewer"
    assert variant.workspace_ids == ("world_viewer",)
    assert variant.default_workspace == "world_viewer"


def test_main_args_infer_variant_from_executable_name():
    variant_id, launch_path = _parse_main_args([], "ModBox21-BSAViewer.exe")

    assert variant_id == "bsa_viewer"
    assert launch_path is None


def test_variant_arg_overrides_executable_name():
    variant_id, launch_path = _parse_main_args(
        ["--variant=materials", "textures/foo.dds"],
        "ModBox21-NIF.exe",
    )

    assert variant_id == "materials"
    assert launch_path == "textures/foo.dds"


def test_nif_first_run_auto_detects_paths_and_skips_setup(monkeypatch, tmp_path):
    shared_path = tmp_path / "shared_settings.json"
    variant_path = tmp_path / "nif.json"
    settings = ToolkitSettings(
        shared_path=shared_path,
        variant_path=variant_path,
        editor_settings_path=tmp_path / "old_editor_settings.json",
        variant_id="nif",
    )

    def fake_detect(game_id):
        return {"fo4": "C:/Games/Fallout 4", "fnv": "C:/Games/Fallout New Vegas"}.get(game_id)

    def fake_validate(game_id, path):
        return game_id == "fnv" and path == "C:/Games/Fallout New Vegas"

    monkeypatch.setattr("ui.toolkit.path_detector.detect_game_path", fake_detect)
    monkeypatch.setattr("ui.toolkit.path_detector.validate_game_path", fake_validate)

    assert _prepare_first_run_settings(settings, get_variant("nif")) is True

    reloaded = ToolkitSettings(
        shared_path=shared_path,
        variant_path=variant_path,
        editor_settings_path=tmp_path / "old_editor_settings.json",
        variant_id="nif",
    )
    assert reloaded.setup_complete is True
    assert reloaded.get_game_paths("fnv")["root_dir"] == "C:/Games/Fallout New Vegas"
    assert reloaded.get_game_paths("fo4")["root_dir"] == ""


def test_full_first_run_still_uses_setup_wizard(tmp_path):
    settings = ToolkitSettings(
        shared_path=tmp_path / "shared_settings.json",
        variant_path=tmp_path / "full.json",
        editor_settings_path=tmp_path / "old_editor_settings.json",
        variant_id="full",
    )

    assert _prepare_first_run_settings(settings, get_variant("full")) is False
    assert settings.setup_complete is False


def test_all_variants_have_placeholder_icon_paths():
    from ui.toolkit.variants import VARIANTS

    assert {
        variant_id: variant.icon_path
        for variant_id, variant in VARIANTS.items()
    } == {
        "full": "resource/icon.ico",
        "nif": "resource/icons/modbox21-nif.ico",
        "bsa_viewer": "resource/icons/modbox21-bsa-viewer.ico",
        "cloth_maker": "resource/icons/modbox21-cloth.ico",
        "weight_painter": "resource/icons/modbox21-weights.ico",
        "papyrus": "resource/icons/modbox21-papyrus.ico",
        "materials": "resource/icons/modbox21-materials.ico",
        "esp_editor": "resource/icons/modbox21-esp-editor.ico",
        "world_viewer": "resource/icon.ico",
    }
