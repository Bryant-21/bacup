"""Render the supported ESP workspace with disposable fixtures and settings.

uv run python -m ui.tests.render_esp_editor --view fields --output <path.png>
"""
from __future__ import annotations

import argparse
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from imgui_bundle import hello_imgui, immapp, imgui
from PIL import Image

from creation_lib.esp.native_runtime import RecordSummary
from creation_lib.esp.schema import get_schema
from ui.tests.test_esp_editor_workflow import plugin_file
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--view", choices=("fields", "overrides", "save", "close", "quit"), default="fields")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    directory = args.output.resolve().parent
    directory.mkdir(parents=True, exist_ok=True)
    fixtures = directory / "fixture-plugins"
    fixtures.mkdir(exist_ok=True)
    plugin_file(fixtures, "B21_FixtureBase.esm", value=25)
    plugin_file(fixtures, "B21_FixturePatch.esp", masters=("B21_FixtureBase.esm",), value=75)
    settings = ToolkitSettings(path=directory / "fixture-settings.json",
                               editor_settings_path=directory / "absent-settings.json")
    settings.save = lambda: None
    settings.setup_complete = True
    settings.active_workspace = "esp_editor"
    settings.window_width, settings.window_height = 1600, 900
    for game in ("fo4", "fo76", "skyrimse", "starfield", "fo3", "fnv"):
        settings.set_game_root_dir(game, "")
        settings.set_game_extracted_dir(game, "")
    settings._paths["fo4"]["additional_paths"] = []
    get_schema("fo4")
    variant = replace(get_variant("esp_editor"), include_ai_panel=False)
    app = ToolkitApp(create_workspaces(settings, workspace_ids=variant.workspace_ids), settings, app_variant=variant)
    run = immapp.run
    frames = 0
    initialized = False
    popup_requested = False

    def render(runner_params, **kwargs):
        params = runner_params
        params.ini_disable = True
        params.app_window_params.hidden = True
        params.app_window_params.window_geometry.size = (1600, 900)
        params.app_window_params.window_geometry.window_size_measure_mode = hello_imgui.WindowSizeMeasureMode.screen_coords
        params.fps_idling.enable_idling = False
        def finish():
            nonlocal frames, initialized, popup_requested
            frames += 1
            editor = app._active_ws._app
            if not initialized and editor is not None:
                editor.open_folder(str(fixtures))
                initialized = True
            if not editor._busy and editor.session.plugins and editor.selection.record is None:
                plugin = editor.session.get_by_name("B21_FixturePatch.esp")
                editor._select_record(plugin.handle, RecordSummary(0x800, "MISC", "B21_Fixture"))
            if args.view in ("save", "close", "quit") and editor.selection.fields and not popup_requested:
                editor._dirty_handles.add(editor.selection.plugin_handle)
                if args.view == "save":
                    editor.open_save_popup()
                elif args.view == "close":
                    editor.close_active()
                else:
                    app._request_quit()
                popup_requested = True
            if frames > 600:
                raise RuntimeError(f"Editor did not become ready: {editor._latest_error}")
        params.callbacks.post_render_dockable_windows = finish
        confirm_quit = params.callbacks.before_imgui_render
        def stop_after_render():
            confirm_quit()
            # Screenshot-only harness termination, after the live modal has
            # drawn. Only disposable fixtures are discarded here.
            if frames >= 45 and app._active_ws._app.selection.fields is not None:
                params.app_shall_exit = True
        params.callbacks.before_imgui_render = stop_after_render
        run(runner_params=params, **kwargs)

    begin_tab = imgui.begin_tab_item
    tree_node = imgui.tree_node
    def select_tab(label, *positional, **kwargs):
        if label == "Overrides" and args.view == "overrides":
            kwargs["flags"] = imgui.TabItemFlags_.set_selected
        return begin_tab(label, *positional, **kwargs)
    def open_group(label, *positional, **kwargs):
        if "##grp_" in label:
            imgui.set_next_item_open(True, imgui.Cond_.once)
        return tree_node(label, *positional, **kwargs)
    with patch("ui.toolkit.app.immapp.run", render), patch.object(imgui, "begin_tab_item", select_tab), patch.object(imgui, "tree_node", open_group):
        app.run()
    Image.fromarray(hello_imgui.final_app_window_screenshot()).save(args.output)
    print(f"Rendered {args.view}: {args.output} ({frames} frames)")


if __name__ == "__main__":
    main()
