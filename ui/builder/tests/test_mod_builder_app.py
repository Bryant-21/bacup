from __future__ import annotations

import json
import re
import zipfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from ruamel.yaml import YAML

from ui.builder.mod_builder_app import (
    ModBuilderApp,
    _DEFAULT_ARCHIVE_MAX_SIZE_GB,
    _is_light_tagged,
    _is_master_tagged,
    _is_mod_deployed,
    _mod_kind,
    _mod_list_label,
    _mod_plugin_type_label,
    _progress_fraction_from_line,
    _plugin_ext,
    _register_fo4_runtime_archive_ini_entries,
    _remove_fo4_archive_ini_entries,
    _set_plugin_header_flags,
    _set_plugin_type_files,
)
from ui.tools.assets.archlist_creator import create_loose_archlist


def test_mod_plugin_type_label_reads_authoring_plugin_yaml(tmp_path: Path):
    mod_dir = tmp_path / "mods" / "B21_Test"
    yaml_dir = mod_dir / "yaml"
    yaml_dir.mkdir(parents=True)

    (yaml_dir / "plugin.yaml").write_text(
        "\n".join(
            [
                "format_version: 1",
                "plugin: B21_Test.esp",
                "game: fo4",
                "header:",
                "  flags:",
                "  - LightPlugin",
                "",
            ]
        ),
        encoding="utf-8",
    )

    assert _plugin_ext(str(mod_dir)) == "esp"
    assert _mod_plugin_type_label(str(mod_dir), "B21_Test") == "ESP (Light)"


def test_set_plugin_type_files_updates_authoring_plugin_yaml_references(tmp_path: Path):
    mod_dir = tmp_path / "mods" / "B21_Test"
    yaml_dir = mod_dir / "yaml"
    yaml_dir.mkdir(parents=True)

    (yaml_dir / "plugin.yaml").write_text(
        "\n".join(
            [
                "format_version: 1",
                "plugin: B21_Test.esp",
                "game: fo4",
                "header:",
                "  flags: []",
                "",
            ]
        ),
        encoding="utf-8",
    )
    record = yaml_dir / "records" / "COBJ" / "B21_Test_Recipe - 000830_B21_Test.esp.yaml"
    record.parent.mkdir(parents=True)
    record.write_text(
        "\n".join(
            [
                'form_id: "000830"',
                "fields:",
                "- CreatedObject:",
                "    reference:",
                "      plugin: B21_Test.esp",
                '      object_id: "000810"',
                "",
            ]
        ),
        encoding="utf-8",
    )
    patch_yaml_dir = mod_dir / "patches" / "PatchA" / "yaml"
    patch_yaml_dir.mkdir(parents=True)
    (patch_yaml_dir / "plugin.yaml").write_text(
        "\n".join(
            [
                "format_version: 1",
                "plugin: PatchA.esp",
                "game: fo4",
                "header:",
                "  masters:",
                "  - Fallout4.esm",
                "  - B21_Test.esp",
                "  flags: []",
                "",
            ]
        ),
        encoding="utf-8",
    )

    _set_plugin_type_files(str(mod_dir), "esl", needs_light=True)

    yaml = YAML()
    doc = yaml.load((yaml_dir / "plugin.yaml").read_text(encoding="utf-8"))
    assert doc["plugin"] == "B21_Test.esl"
    assert doc["header"]["flags"] == ["LightPlugin"]
    assert _plugin_ext(str(mod_dir)) == "esl"

    renamed = yaml_dir / "records" / "COBJ" / "B21_Test_Recipe - 000830_B21_Test.esl.yaml"
    assert renamed.is_file()
    assert "plugin: B21_Test.esl" in renamed.read_text(encoding="utf-8")
    assert not record.exists()

    patch_text = (patch_yaml_dir / "plugin.yaml").read_text(encoding="utf-8")
    assert "B21_Test.esl" in patch_text

    _set_plugin_type_files(str(mod_dir), "esp", needs_light=False)

    doc = yaml.load((yaml_dir / "plugin.yaml").read_text(encoding="utf-8"))
    assert doc["plugin"] == "B21_Test.esp"
    assert doc["header"]["flags"] == []
    renamed_back = yaml_dir / "records" / "COBJ" / "B21_Test_Recipe - 000830_B21_Test.esp.yaml"
    assert renamed_back.is_file()
    assert "plugin: B21_Test.esp" in renamed_back.read_text(encoding="utf-8")


def test_set_plugin_header_flags_updates_light_and_master_tags(tmp_path: Path):
    mod_dir = tmp_path / "mods" / "B21_Test"
    yaml_dir = mod_dir / "yaml"
    yaml_dir.mkdir(parents=True)
    (mod_dir / "B21_Test.esp").write_text("", encoding="utf-8")

    (yaml_dir / "plugin.yaml").write_text(
        "\n".join(
            [
                "format_version: 1",
                "plugin: B21_Test.esp",
                "game: fo4",
                "header:",
                "  flags:",
                "  - LightPlugin",
                "",
            ]
        ),
        encoding="utf-8",
    )

    _set_plugin_header_flags(str(mod_dir), add_flags=("MasterFile",))

    yaml = YAML()
    doc = yaml.load((yaml_dir / "plugin.yaml").read_text(encoding="utf-8"))
    assert doc["header"]["flags"] == ["MasterFile", "LightPlugin"]
    assert _is_light_tagged(str(mod_dir)) is True
    assert _is_master_tagged(str(mod_dir)) is True
    assert _mod_plugin_type_label(str(mod_dir), "B21_Test") == "ESP (Master, Light)"

    _set_plugin_header_flags(str(mod_dir), remove_flags=("LightPlugin",))

    doc = yaml.load((yaml_dir / "plugin.yaml").read_text(encoding="utf-8"))
    assert doc["header"]["flags"] == ["MasterFile"]
    assert _is_light_tagged(str(mod_dir)) is False
    assert _is_master_tagged(str(mod_dir)) is True
    assert _mod_plugin_type_label(str(mod_dir), "B21_Test") == "ESP (Master)"

    _set_plugin_header_flags(str(mod_dir), remove_flags=("MasterFile",))

    doc = yaml.load((yaml_dir / "plugin.yaml").read_text(encoding="utf-8"))
    assert doc["header"]["flags"] == []
    assert _is_light_tagged(str(mod_dir)) is False
    assert _is_master_tagged(str(mod_dir)) is False
    assert _mod_plugin_type_label(str(mod_dir), "B21_Test") == "ESP"


def test_set_plugin_header_flags_normalizes_raw_known_bits(tmp_path: Path):
    mod_dir = tmp_path / "mods" / "B21_Test"
    yaml_dir = mod_dir / "yaml"
    yaml_dir.mkdir(parents=True)

    (yaml_dir / "plugin.yaml").write_text(
        "\n".join(
            [
                "format_version: 1",
                "plugin: B21_Test.esp",
                "game: fo4",
                "header:",
                '  flags: "00000280"',
                "",
            ]
        ),
        encoding="utf-8",
    )

    _set_plugin_header_flags(
        str(mod_dir),
        add_flags=("MasterFile",),
        remove_flags=("LightPlugin",),
    )

    yaml = YAML()
    doc = yaml.load((yaml_dir / "plugin.yaml").read_text(encoding="utf-8"))
    assert doc["header"]["flags"] == ["MasterFile", "Localized"]
    assert _is_light_tagged(str(mod_dir)) is False
    assert _is_master_tagged(str(mod_dir)) is True


def test_mod_list_label_marks_deployed_mods():
    assert _mod_list_label("B21_Test", "ESP", deployed=True) == "* B21_Test [ESP]"
    assert _mod_list_label("B21_Test", "ESP", deployed=False) == "B21_Test [ESP]"


def test_mod_kind_keeps_xse_with_loose_scripts_as_xse(tmp_path: Path):
    mod_dir = tmp_path / "mods" / "B21_Test"
    (mod_dir / "src").mkdir(parents=True)
    (mod_dir / "xmake.lua").write_text("", encoding="utf-8")
    (mod_dir / "Scripts" / "Source" / "User").mkdir(parents=True)
    (mod_dir / "Scripts" / "B21_Test.pex").write_text("pex", encoding="utf-8")
    (mod_dir / "Scripts" / "Source" / "User" / "B21_Test.psc").write_text(
        "Scriptname B21_Test Native Hidden",
        encoding="utf-8",
    )

    assert _mod_kind(str(mod_dir)) == "xse"


def test_mod_kind_combines_xse_with_plugin_file(tmp_path: Path):
    mod_dir = tmp_path / "mods" / "B21_Test"
    (mod_dir / "src").mkdir(parents=True)
    (mod_dir / "xmake.lua").write_text("", encoding="utf-8")
    (mod_dir / "B21_Test.esl").write_text("", encoding="utf-8")

    assert _mod_kind(str(mod_dir)) == "combined"


def test_mod_kind_combines_xse_with_yaml_files_only(tmp_path: Path):
    mod_dir = tmp_path / "mods" / "B21_Test"
    (mod_dir / "src").mkdir(parents=True)
    (mod_dir / "xmake.lua").write_text("", encoding="utf-8")
    (mod_dir / "yaml").mkdir()

    assert _mod_kind(str(mod_dir)) == "xse"

    (mod_dir / "yaml" / "plugin.yaml").write_text("plugin: B21_Test.esp", encoding="utf-8")

    assert _mod_kind(str(mod_dir)) == "combined"


def test_is_mod_deployed_detects_plugin_in_game_data(tmp_path: Path):
    mod_dir = tmp_path / "mods" / "B21_Test"
    game_data = tmp_path / "Game" / "Data"
    mod_dir.mkdir(parents=True)
    game_data.mkdir(parents=True)
    (game_data / "B21_Test.esp").write_text("", encoding="utf-8")

    assert _is_mod_deployed(str(mod_dir), "B21_Test", "mod", game_data) is True


def test_is_mod_deployed_detects_xse_tree_in_game_data(tmp_path: Path):
    mod_dir = tmp_path / "mods" / "B21_Test"
    game_data = tmp_path / "Game" / "Data"
    (mod_dir / "F4SE" / "Plugins").mkdir(parents=True)
    (game_data / "F4SE" / "Plugins").mkdir(parents=True)
    (mod_dir / "F4SE" / "Plugins" / "B21_Test.dll").write_text("dll", encoding="utf-8")
    (game_data / "F4SE" / "Plugins" / "B21_Test.dll").write_text("dll", encoding="utf-8")

    assert _is_mod_deployed(str(mod_dir), "B21_Test", "xse", game_data) is True


def test_progress_fraction_from_runner_line():
    assert _progress_fraction_from_line("[1/5] Building .esp...") == 0.0
    assert _progress_fraction_from_line("[5/5] Deploying to Data...") == 0.8
    assert _progress_fraction_from_line("=== Deploy complete ===") == 1.0
    assert _progress_fraction_from_line("Copied: B21_Test.esp") is None


def test_register_fo4_runtime_archive_ini_entries_seeds_custom_ini(tmp_path: Path):
    custom_ini = tmp_path / "Fallout4Custom.ini"
    game_ini = tmp_path / "Fallout4.ini"
    game_ini.write_text(
        "\n".join(
            [
                "[Archive]",
                "SResourceArchiveList=Fallout4 - Main.ba2",
                "SResourceArchiveList2=Fallout4 - Animations.ba2",
                "sResourceIndexFileList=Fallout4 - Textures1.ba2",
                "",
            ]
        ),
        encoding="utf-8",
    )

    added = _register_fo4_runtime_archive_ini_entries(
        [
            "B21_Test - Meshes.ba2",
            "B21_Test - Animations.ba2",
            "B21_Test - Textures1.ba2",
            "B21_Test - Meshes_xbox.ba2",
        ],
        ini_path=custom_ini,
        base_ini_path=game_ini,
    )

    assert added == [
        "B21_Test - Meshes.ba2",
        "B21_Test - Animations.ba2",
        "B21_Test - Textures1.ba2",
    ]
    text = custom_ini.read_text(encoding="utf-8")
    assert "SResourceArchiveList=Fallout4 - Main.ba2, B21_Test - Meshes.ba2" in text
    assert "SResourceArchiveList2=Fallout4 - Animations.ba2, B21_Test - Animations.ba2" in text
    assert "sResourceIndexFileList=Fallout4 - Textures1.ba2, B21_Test - Textures1.ba2" in text
    assert "B21_Test - Meshes_xbox.ba2" not in text


def test_remove_fo4_archive_ini_entries_removes_only_requested_mod_archives(tmp_path: Path):
    custom_ini = tmp_path / "Fallout4Custom.ini"
    custom_ini.write_text(
        "\n".join(
            [
                "[Archive]",
                "SResourceArchiveList=Fallout4 - Main.ba2, B21_Test - Meshes.ba2",
                "SResourceArchiveList2=B21_Test - Animations.ba2",
                "sResourceIndexFileList=Other - Textures.ba2, B21_Test - Textures.ba2",
                "",
            ]
        ),
        encoding="utf-8",
    )

    removed = _remove_fo4_archive_ini_entries(
        [
            "B21_Test - Meshes.ba2",
            "B21_Test - Animations.ba2",
            "B21_Test - Textures.ba2",
        ],
        ini_path=custom_ini,
    )

    assert removed == [
        "B21_Test - Meshes.ba2",
        "B21_Test - Animations.ba2",
        "B21_Test - Textures.ba2",
    ]
    text = custom_ini.read_text(encoding="utf-8")
    assert "B21_Test" not in text
    assert "Fallout4 - Main.ba2" in text
    assert "Other - Textures.ba2" in text


def test_builder_progress_state_tracks_latest_lines():
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp()

    app._reset_progress_state("Deploying B21_Test")
    assert app._progress_message == "Starting..."
    assert app._progress_fraction is None

    app._record_progress_line("[2/5] Packing BA2 archives...")
    assert app._progress_message == "[2/5] Packing BA2 archives..."
    assert app._progress_fraction == 0.2

    for idx in range(5):
        app._record_progress_line(f"Copied file {idx}")
    assert app._progress_lines == ["Copied file 2", "Copied file 3", "Copied file 4"]


def test_mod_selection_summary_does_not_walk_mod_tree(tmp_path: Path):
    mods_dir = tmp_path / "mods"
    mod_dir = mods_dir / "B21_Test"
    (mod_dir / "yaml").mkdir(parents=True)
    (mod_dir / "data" / "Meshes").mkdir(parents=True)
    (mod_dir / "Scripts" / "Source" / "User").mkdir(parents=True)
    (mod_dir / "B21_Test.esp").write_bytes(b"esp")
    (mod_dir / ".game").write_text("fo4", encoding="utf-8")
    (mod_dir / ".version").write_text("1.2.3", encoding="utf-8")

    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None), patch(
        "ui.builder.mod_builder_app.MODS_DIR", str(mods_dir)
    ):
        app = ModBuilderApp()

        app._mod_list = ["B21_Test"]
        app._mod_kinds = ["mod"]
        app._selected_mod_idx = 0

        with patch("ui.builder.mod_builder_app.os.walk", side_effect=AssertionError("selection walked mod tree")):
            app._on_mod_changed()

        assert "YAML" in app._info_text
        assert "Loose data" in app._info_text
        assert "Version: 1.2.3" in app._info_text


def test_utils_tab_buttons_have_unique_imgui_ids():
    source = Path("ui/builder/mod_builder_app.py").read_text(encoding="utf-8")
    start = source.index("    def _draw_utils_tab")
    end = source.index("    # \u2500\u2500 Spellcheck actions", start)
    section = source[start:end]
    labels = re.findall(r'imgui\.button\("([^"]+)"', section)
    ids = [label.split("##", 1)[1] if "##" in label else label for label in labels]
    duplicates = {item for item in ids if ids.count(item) > 1}

    assert duplicates == set()


def test_builder_addon_registry_view_loads_current_registry_module(tmp_path: Path):
    mods_dir = tmp_path / "mods"
    mod_dir = mods_dir / "B21_Test"
    mod_dir.mkdir(parents=True)
    (mods_dir / ".addon_registry.json").write_text(
        json.dumps(
            {
                "version": 1,
                "allocations": {
                    "20000": {
                        "mod": "B21_Test",
                        "editor_id": "B21_Test_Node",
                        "game": "fo4",
                    }
                },
                "next_index": 20001,
            }
        ),
        encoding="utf-8",
    )

    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None), patch(
        "ui.builder.mod_builder_app.MODS_DIR", str(mods_dir)
    ):
        app = ModBuilderApp()
        app._refresh_addon_registry_view()

        assert app._addon_registry_status == "1 allocation(s), 0 stale, next=20001"
        assert app._addon_registry_entries == [
            (20000, {"mod": "B21_Test", "editor_id": "B21_Test_Node", "game": "fo4"})
        ]
        assert app._addon_registry_selected_index == 20000
        assert app._get_mod_addon_count("B21_Test") == 1


def test_builder_deploy_actions_forward_skip_validation_flag(tmp_path: Path):
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp()

    app._selected_mod = lambda: "B21_Test"
    app._get_mod_game = lambda: "fo4"
    app._selected_mod_kind = lambda: "mod"
    app._resolve_game_data_path = lambda game: tmp_path / "Game" / "Data"
    app._skip_build = False
    app._skip_pack = False
    app._esp_only = False
    app._xbox = False
    app._deploy_patches = False
    app._skip_validation = True
    app._run_fn = lambda target_fn, on_done=None, description="": target_fn(lambda msg: None)

    deploy_calls: list[dict] = []
    loose_calls: list[dict] = []

    with patch("app.paths.get_app_root", return_value=tmp_path), patch(
        "app.paths.get_db_dir", return_value=tmp_path / "db"
    ), patch("app.paths.get_resource_dir", return_value=tmp_path / "resource"), patch(
        "creation_lib.build.deployer.deploy_mod",
        side_effect=lambda *args, **kwargs: deploy_calls.append(kwargs),
    ), patch(
        "creation_lib.build.loose_deploy.deploy_loose_assets",
        side_effect=lambda *args, **kwargs: loose_calls.append(kwargs),
    ):
        app._on_deploy()
        app._on_deploy_loose()

    assert deploy_calls[0]["skip_validation"] is True
    assert deploy_calls[0]["archive_max_bytes"] == int(
        _DEFAULT_ARCHIVE_MAX_SIZE_GB * 1024**3
    )
    assert deploy_calls[0]["expanded_archives"] is False
    assert deploy_calls[0]["archive_workers"] == 0
    assert deploy_calls[0]["archive_transfer_mode"] == "copy"
    assert loose_calls[0]["skip_validation"] is True
    assert loose_calls[0]["workers"] == 0


def test_builder_deploy_actions_forward_mo2_copy_target(tmp_path: Path):
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp()

    real_data = tmp_path / "Fallout4" / "Data"
    mo2_dir = tmp_path / "ModOrganizer" / "mods" / "B21_Test"
    app._selected_mod = lambda: "B21_Test"
    app._get_mod_game = lambda: "fo4"
    app._selected_mod_kind = lambda: "mod"
    app._resolve_game_data_path = lambda game: real_data
    app._resolve_deploy_data_path = lambda game: mo2_dir
    app._skip_build = False
    app._skip_pack = False
    app._esp_only = False
    app._xbox = False
    app._deploy_patches = False
    app._skip_validation = False
    app._run_fn = lambda target_fn, on_done=None, description="": target_fn(lambda msg: None)

    deploy_calls: list[dict] = []
    loose_calls: list[dict] = []

    with patch("app.paths.get_app_root", return_value=tmp_path), patch(
        "app.paths.get_db_dir", return_value=tmp_path / "db"
    ), patch("app.paths.get_resource_dir", return_value=tmp_path / "resource"), patch(
        "creation_lib.build.deployer.deploy_mod",
        side_effect=lambda *args, **kwargs: deploy_calls.append(kwargs),
    ), patch(
        "creation_lib.build.loose_deploy.deploy_loose_assets",
        side_effect=lambda *args, **kwargs: loose_calls.append(kwargs),
    ):
        app._on_deploy()
        app._on_deploy_loose()

    assert deploy_calls[0]["game_data_dir"] == real_data
    assert deploy_calls[0]["deploy_data_dir"] == mo2_dir
    assert loose_calls[0]["game_data_dir"] == real_data
    assert loose_calls[0]["deploy_data_dir"] == mo2_dir


def test_builder_deploy_forwards_archive_max_size_setting(tmp_path: Path):
    class Settings:
        def get_workspace_settings(self, workspace_id):
            assert workspace_id == "mod_builder"
            return {"archive_max_size_gb": 2.5, "asset_workers": 6}

    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp(Settings())

    app._selected_mod = lambda: "B21_Test"
    app._get_mod_game = lambda: "fo4"
    app._selected_mod_kind = lambda: "mod"
    app._resolve_game_data_path = lambda game: tmp_path / "Game" / "Data"
    app._skip_build = False
    app._skip_pack = False
    app._esp_only = False
    app._xbox = False
    app._deploy_patches = False
    app._skip_validation = False
    app._run_fn = lambda target_fn, on_done=None, description="": target_fn(lambda msg: None)

    deploy_calls: list[dict] = []
    with patch("app.paths.get_app_root", return_value=tmp_path), patch(
        "app.paths.get_resource_dir", return_value=tmp_path / "resource"
    ), patch(
        "creation_lib.build.deployer.deploy_mod",
        side_effect=lambda *args, **kwargs: deploy_calls.append(kwargs),
    ):
        app._on_deploy()

    assert deploy_calls[0]["archive_max_bytes"] == int(2.5 * 1024**3)
    assert deploy_calls[0]["expanded_archives"] is False
    assert deploy_calls[0]["archive_workers"] == 6


def test_builder_deploy_forwards_move_archive_option(tmp_path: Path):
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp()

    app._selected_mod = lambda: "B21_Test"
    app._get_mod_game = lambda: "fo4"
    app._selected_mod_kind = lambda: "mod"
    app._resolve_game_data_path = lambda game: tmp_path / "Game" / "Data"
    app._skip_build = False
    app._skip_pack = False
    app._esp_only = False
    app._xbox = False
    app._move_archives = True
    app._deploy_patches = False
    app._skip_validation = False
    app._run_fn = lambda target_fn, on_done=None, description="": target_fn(lambda msg: None)

    deploy_calls: list[dict] = []
    with patch("app.paths.get_app_root", return_value=tmp_path), patch(
        "app.paths.get_resource_dir", return_value=tmp_path / "resource"
    ), patch(
        "creation_lib.build.deployer.deploy_mod",
        side_effect=lambda *args, **kwargs: deploy_calls.append(kwargs),
    ):
        app._on_deploy()

    assert deploy_calls[0]["archive_transfer_mode"] == "move"


def test_builder_archive_max_size_setting_persists_to_workspace():
    class Settings:
        def __init__(self):
            self.calls = []
            self.saved = False

        def get_workspace_settings(self, workspace_id):
            assert workspace_id == "mod_builder"
            return {}

        def set_workspace_settings(self, workspace_id, settings):
            self.calls.append((workspace_id, settings))

        def save(self):
            self.saved = True

    settings = Settings()
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp(settings)

    app._set_archive_max_size_gb(2.75)

    assert settings.calls == [("mod_builder", {"archive_max_size_gb": 2.75})]
    assert settings.saved is True


def test_builder_move_archive_setting_persists_to_workspace():
    class Settings:
        def __init__(self):
            self.calls = []
            self.saved = False

        def get_workspace_settings(self, workspace_id):
            assert workspace_id == "mod_builder"
            return {"archive_max_size_gb": 2.5}

        def set_workspace_settings(self, workspace_id, settings):
            self.calls.append((workspace_id, settings))

        def save(self):
            self.saved = True

    settings = Settings()
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp(settings)

    app._set_move_archives(True)

    assert settings.calls == [
        ("mod_builder", {"archive_max_size_gb": 2.5, "move_archives": True})
    ]
    assert settings.saved is True


def test_builder_asset_worker_setting_persists_to_workspace():
    class Settings:
        def __init__(self):
            self.calls = []
            self.saved = False

        def get_workspace_settings(self, workspace_id):
            assert workspace_id == "mod_builder"
            return {"archive_max_size_gb": 2.5}

        def set_workspace_settings(self, workspace_id, settings):
            self.calls.append((workspace_id, settings))

        def save(self):
            self.saved = True

    settings = Settings()
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp(settings)

    app._set_asset_workers(4)

    assert settings.calls == [
        ("mod_builder", {"archive_max_size_gb": 2.5, "asset_workers": 4})
    ]
    assert settings.saved is True


def test_builder_loose_deploy_forwards_asset_worker_setting(tmp_path: Path):
    class Settings:
        def get_workspace_settings(self, workspace_id):
            assert workspace_id == "mod_builder"
            return {"asset_workers": 6}

    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp(Settings())

    app._selected_mod = lambda: "B21_Test"
    app._get_mod_game = lambda: "fo4"
    app._selected_mod_kind = lambda: "mod"
    app._resolve_game_data_path = lambda game: tmp_path / "Game" / "Data"
    app._resolve_deploy_data_path = lambda game: tmp_path / "Game" / "Data"
    app._skip_build = False
    app._skip_validation = False
    app._skip_papyrus_compile = False
    app._pc_max_res_idx = 0
    app._pc_effects_max_res_idx = None
    app._run_fn = lambda target_fn, on_done=None, description="": target_fn(lambda msg: None)

    loose_calls: list[dict] = []
    with patch("app.paths.get_app_root", return_value=tmp_path), patch(
        "creation_lib.build.loose_deploy.deploy_loose_assets",
        side_effect=lambda *args, **kwargs: loose_calls.append(kwargs),
    ):
        app._on_deploy_loose()

    assert loose_calls[0]["workers"] == 6


def test_builder_deploy_forwards_expanded_archive_option(tmp_path: Path):
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp()

    app._selected_mod = lambda: "B21_Test"
    app._get_mod_game = lambda: "fo4"
    app._selected_mod_kind = lambda: "mod"
    app._resolve_game_data_path = lambda game: tmp_path / "Game" / "Data"
    app._skip_build = False
    app._skip_pack = False
    app._esp_only = False
    app._xbox = False
    app._expanded_archives = True
    app._deploy_patches = False
    app._skip_validation = False
    app._run_fn = lambda target_fn, on_done=None, description="": target_fn(lambda msg: None)

    deploy_calls: list[dict] = []
    with patch("app.paths.get_app_root", return_value=tmp_path), patch(
        "app.paths.get_resource_dir", return_value=tmp_path / "resource"
    ), patch(
        "creation_lib.build.deployer.deploy_mod",
        side_effect=lambda *args, **kwargs: deploy_calls.append(kwargs),
    ):
        app._on_deploy()

    assert deploy_calls[0]["expanded_archives"] is True


def test_builder_deploy_updates_fo4_archive_ini_when_enabled(tmp_path: Path):
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp()

    app._selected_mod = lambda: "B21_Test"
    app._get_mod_game = lambda: "fo4"
    app._selected_mod_kind = lambda: "mod"
    app._resolve_game_data_path = lambda game: tmp_path / "Game" / "Data"
    app._skip_build = False
    app._skip_pack = False
    app._esp_only = False
    app._xbox = False
    app._expanded_archives = True
    app._update_fo4_archive_ini = True
    app._deploy_patches = False
    app._skip_validation = False
    progress: list[str] = []
    app._run_fn = lambda target_fn, on_done=None, description="": target_fn(progress.append)

    registered_calls: list[list[str]] = []

    def fake_register(archive_names):
        registered_calls.append(archive_names)
        return ["B21_Test - Meshes.ba2"]

    with patch("app.paths.get_app_root", return_value=tmp_path), patch(
        "app.paths.get_resource_dir", return_value=tmp_path / "resource"
    ), patch(
        "creation_lib.build.deployer.deploy_mod",
        return_value=SimpleNamespace(
            archives_deployed=[
                "B21_Test - Meshes.ba2",
                "B21_Test - Textures1.ba2",
            ]
        ),
    ), patch(
        "ui.builder.mod_builder_app._register_fo4_runtime_archive_ini_entries",
        side_effect=fake_register,
    ):
        app._on_deploy()

    assert registered_calls == [[
        "B21_Test - Meshes.ba2",
        "B21_Test - Textures1.ba2",
    ]]
    assert progress[-1] == "Updated Fallout4Custom.ini archive entries: B21_Test - Meshes.ba2"


def test_builder_undeploy_removes_fo4_archive_ini_entries_when_enabled(tmp_path: Path):
    custom_ini = tmp_path / "Fallout4Custom.ini"
    custom_ini.write_text(
        "\n".join(
            [
                "[Archive]",
                "SResourceArchiveList=Fallout4 - Main.ba2, B21_Test - Meshes.ba2",
                "sResourceIndexFileList=B21_Test - Textures.ba2",
                "",
            ]
        ),
        encoding="utf-8",
    )

    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp()

    app._selected_mod = lambda: "B21_Test"
    app._get_mod_game = lambda: "fo4"
    app._selected_mod_kind = lambda: "mod"
    app._resolve_game_data_path = lambda game: tmp_path / "Game" / "Data"
    app._update_fo4_archive_ini = True
    app._deploy_patches = False
    progress: list[str] = []
    app._run_fn = lambda target_fn, on_done=None, description="": target_fn(progress.append)

    with patch("app.paths.get_app_root", return_value=tmp_path), patch(
        "creation_lib.build.deployer.undeploy_mod",
        return_value=["B21_Test.esp", "B21_Test - Meshes.ba2"],
    ), patch(
        "ui.builder.mod_builder_app._resolve_fo4_custom_ini_path",
        return_value=custom_ini,
    ):
        app._on_undeploy()

    text = custom_ini.read_text(encoding="utf-8")
    assert "B21_Test" not in text
    assert "Fallout4 - Main.ba2" in text
    assert progress[-1] == (
        "Removed Fallout4Custom.ini archive entries: "
        "B21_Test - Meshes.ba2, B21_Test - Textures.ba2"
    )


def test_builder_undeploy_removes_expanded_fo4_archive_ini_entries_when_checkbox_off(tmp_path: Path):
    custom_ini = tmp_path / "Fallout4Custom.ini"
    custom_ini.write_text(
        "\n".join(
            [
                "[Archive]",
                (
                    "SResourceArchiveList=Fallout4 - Main.ba2, "
                    "B21_Test - Meshes.ba2, B21_Test - Misc.ba2, "
                    "B21_Test - Materials.ba2, B21_Test - Interface.ba2, "
                    "B21_Test - LOD1.ba2"
                ),
                "sResourceIndexFileList=B21_Test - Textures.ba2",
                "",
            ]
        ),
        encoding="utf-8",
    )

    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp()

    app._selected_mod = lambda: "B21_Test"
    app._get_mod_game = lambda: "fo4"
    app._selected_mod_kind = lambda: "mod"
    app._resolve_game_data_path = lambda game: tmp_path / "Game" / "Data"
    app._update_fo4_archive_ini = False
    app._deploy_patches = False
    progress: list[str] = []
    app._run_fn = lambda target_fn, on_done=None, description="": target_fn(progress.append)

    with patch("app.paths.get_app_root", return_value=tmp_path), patch(
        "creation_lib.build.deployer.undeploy_mod",
        return_value=["B21_Test.esp"],
    ), patch(
        "ui.builder.mod_builder_app._resolve_fo4_custom_ini_path",
        return_value=custom_ini,
    ):
        app._on_undeploy()

    text = custom_ini.read_text(encoding="utf-8")
    assert "B21_Test" not in text
    assert "Fallout4 - Main.ba2" in text
    assert progress[-1] == (
        "Removed Fallout4Custom.ini archive entries: "
        "B21_Test - Meshes.ba2, B21_Test - Misc.ba2, "
        "B21_Test - Materials.ba2, B21_Test - Interface.ba2, "
        "B21_Test - LOD1.ba2, B21_Test - Textures.ba2"
    )


def test_dictionary_popup_opens_from_deferred_flag():
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp()

    opened = []
    with patch("ui.builder.mod_builder_app.imgui.open_popup", side_effect=opened.append), patch(
        "ui.builder.mod_builder_app.imgui.begin_popup",
        return_value=False,
    ):
        app._dict_popup_open = True
        app._draw_dictionary_popup()

    assert opened == ["##dict_popup"]
    assert app._dict_popup_open is False


def test_builder_release_pack_options_include_archive_max_size_setting():
    class Settings:
        def get_workspace_settings(self, workspace_id):
            assert workspace_id == "mod_builder"
            return {"archive_max_size_gb": 3.25, "asset_workers": 5}

    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp(Settings())
    app._get_mod_game = lambda: "fo4"

    options = app._release_pack_options()

    assert options["archive_max_bytes"] == int(3.25 * 1024**3)
    assert options["expanded_archives"] is False
    assert options["archive_workers"] == 5


def test_builder_deploy_and_release_tabs_render_archive_setting_fields():
    source = Path("ui/builder/mod_builder_app.py").read_text(encoding="utf-8")

    deploy_start = source.index("    def _draw_deploy_tab")
    release_start = source.index("    def _draw_release_tab")
    migrate_start = source.index("    def _draw_migrate_tab")
    deploy_section = source[deploy_start:release_start]
    release_section = source[release_start:migrate_start]

    assert "self._draw_archive_max_size_field()" in deploy_section
    assert "self._draw_archive_max_size_field()" in release_section
    assert "self._draw_asset_workers_field()" in deploy_section
    assert "self._draw_asset_workers_field()" in release_section


def test_builder_deploy_tab_renders_fo4_archive_ini_checkbox():
    source = Path("ui/builder/mod_builder_app.py").read_text(encoding="utf-8")

    deploy_start = source.index("    def _draw_deploy_tab")
    release_start = source.index("    def _draw_release_tab")
    deploy_section = source[deploy_start:release_start]

    assert '"FO4 Archive INI"' in deploy_section
    assert "Fallout4Custom.ini" in deploy_section


def test_builder_deploy_tab_renders_move_ba2_option():
    source = Path("ui/builder/mod_builder_app.py").read_text(encoding="utf-8")

    deploy_start = source.index("    def _draw_deploy_tab")
    release_start = source.index("    def _draw_release_tab")
    deploy_section = source[deploy_start:release_start]

    assert '"Move BA2s"' in deploy_section
    assert "_set_move_archives" in deploy_section


def test_builder_deploy_tab_places_mo2_option_after_flag_checkboxes():
    source = Path("ui/builder/mod_builder_app.py").read_text(encoding="utf-8")

    deploy_start = source.index("    def _draw_deploy_tab")
    release_start = source.index("    def _draw_release_tab")
    deploy_section = source[deploy_start:release_start]
    skip_validation_idx = deploy_section.index('"Skip Validation"')
    mo2_idx = deploy_section.index("self._draw_mo2_deploy_option()", skip_validation_idx)
    tex_column_idx = deploy_section.index("imgui.table_set_column_index(1)", skip_validation_idx)

    assert skip_validation_idx < mo2_idx < tex_column_idx


def test_builder_release_pack_options_include_expanded_archive_option():
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        app = ModBuilderApp()
    app._get_mod_game = lambda: "fo4"
    app._release_expanded_archives = True

    options = app._release_pack_options()

    assert options["expanded_archives"] is True


def test_builder_release_cleanup_removes_all_discovered_archives(tmp_path: Path):
    mods_dir = tmp_path / "mods"
    mod_dir = mods_dir / "B21_Test"
    mod_dir.mkdir(parents=True)
    removable = [
        mod_dir / "B21_Test - Main.ba2",
        mod_dir / "B21_Test - Meshes1.ba2",
        mod_dir / "B21_Test - Textures_xbox.ba2",
        mod_dir / "B21_Test - Main.bsa",
    ]
    for path in removable:
        path.write_bytes(b"archive")
    keep = [
        mod_dir / "B21_Test.ba2",
        mod_dir / "B21_Test - HiRes.ba2",
        mod_dir / "B21_Test - Main.zip",
        mod_dir / "Other - Main.ba2",
    ]
    for path in keep:
        path.write_bytes(b"keep")

    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None), patch(
        "ui.builder.mod_builder_app.MODS_DIR", str(mods_dir)
    ):
        app = ModBuilderApp()
        app._release_clean_archives("B21_Test")

    assert all(not path.exists() for path in removable)
    assert all(path.exists() for path in keep)


def test_release_failure_restores_backed_up_archives(tmp_path: Path):
    mods_dir = tmp_path / "mods"
    mod_dir = mods_dir / "B21_Test"
    mod_dir.mkdir(parents=True)
    old_archive = mod_dir / "B21_Test - Main.ba2"
    old_archive.write_bytes(b"old")

    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None), patch(
        "ui.builder.mod_builder_app.MODS_DIR", str(mods_dir)
    ):
        app = ModBuilderApp()
        app._release_clean_archives("B21_Test")
        assert not old_archive.exists()
        app._fail_release("failed")

    assert old_archive.read_bytes() == b"old"


def test_release_package_empty_payload_restores_backed_up_archives(tmp_path: Path):
    mods_dir = tmp_path / "mods"
    mod_dir = mods_dir / "B21_Test"
    mod_dir.mkdir(parents=True)
    old_archive = mod_dir / "B21_Test - Main.ba2"
    old_archive.write_bytes(b"old")

    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None), patch(
        "ui.builder.mod_builder_app.MODS_DIR", str(mods_dir)
    ):
        app = ModBuilderApp()
        app._release_clean_archives("B21_Test")
        assert not old_archive.exists()
        app._do_release_package("B21_Test")

    assert old_archive.read_bytes() == b"old"


def test_builder_release_package_includes_all_discovered_archives(tmp_path: Path):
    mods_dir = tmp_path / "mods"
    mod_dir = mods_dir / "B21_Test"
    mod_dir.mkdir(parents=True)
    (mod_dir / "B21_Test.esp").write_bytes(b"esp")
    (mod_dir / "B21_Test - Meshes1.ba2").write_bytes(b"mesh")
    (mod_dir / "B21_Test - Textures2.ba2").write_bytes(b"tex")
    (mod_dir / "B21_Test - Main_xbox.ba2").write_bytes(b"xbox")

    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None), patch(
        "ui.builder.mod_builder_app.MODS_DIR", str(mods_dir)
    ):
        app = ModBuilderApp()
        app._get_mod_game = lambda: "fo4"
        app._release_version = "1.0.0"
        app._do_release_package("B21_Test")

    with zipfile.ZipFile(mod_dir / "release" / "B21_Test.zip") as zf:
        names = set(zf.namelist())
    assert {
        "B21_Test.esp",
        "B21_Test - Meshes1.ba2",
        "B21_Test - Textures2.ba2",
        "B21_Test - Main_xbox.ba2",
    }.issubset(names)


def test_create_loose_archlist_includes_deployable_sources(tmp_path: Path):
    mod_dir = tmp_path / "mods" / "B21_Test"
    (mod_dir / "data" / "Scripts").mkdir(parents=True)
    (mod_dir / "data" / "Textures").mkdir(parents=True)
    (mod_dir / "Meshes" / "Actors").mkdir(parents=True)
    (mod_dir / "Strings").mkdir(parents=True)

    (mod_dir / "data" / "Scripts" / "B21_Test.pex").write_text("pex", encoding="utf-8")
    (mod_dir / "data" / "Textures" / "foo.dds").write_text("dds", encoding="utf-8")
    (mod_dir / "data" / "ignore.ini").write_text("ini", encoding="utf-8")
    (mod_dir / "Meshes" / "Actors" / "body.hkx").write_text("hkx", encoding="utf-8")
    (mod_dir / "Meshes" / "Actors" / "source.xml").write_text("xml", encoding="utf-8")
    (mod_dir / "Strings" / "B21_Test_English.strings").write_text("strings", encoding="utf-8")
    (mod_dir / "B21_Test.esp").write_text("", encoding="utf-8")

    output_file = mod_dir / "B21_Test.archlist"
    ok, count = create_loose_archlist(str(mod_dir), str(output_file))

    assert ok is True
    assert count == 4
    assert output_file.is_file()
    lines = output_file.read_text(encoding="utf-8").splitlines()
    assert lines == [
        "[",
        '\t"Data\\\\Meshes\\\\Actors\\\\body.hkx",',
        '\t"Data\\\\Scripts\\\\B21_Test.pex",',
        '\t"Data\\\\Strings\\\\B21_Test_English.strings",',
        '\t"Data\\\\Textures\\\\foo.dds"',
        "]",
    ]
