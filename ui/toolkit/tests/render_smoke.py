"""Render the toolkit shell with isolated settings and no file operations."""
from __future__ import annotations

import argparse
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from imgui_bundle import hello_imgui, immapp
from PIL import Image

from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.setup_wizard import SetupWizard
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace", default="nif")
    parser.add_argument("--variant", default="full")
    parser.add_argument("--screen", default="main")
    parser.add_argument("--theme", default="falloutnv")
    parser.add_argument("--scale", type=float, default=1)
    parser.add_argument("--width", type=int, default=1600)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    settings = ToolkitSettings(path=args.output.parent / "fixture-settings.json",
                               editor_settings_path=args.output.parent / "no-editor-settings.json")
    settings.save = lambda: None
    settings.theme = args.theme
    settings.setup_complete = True
    settings.active_workspace = args.workspace
    settings.window_width, settings.window_height = args.width, args.height
    for game in ("fo4", "fo76", "skyrimse", "starfield", "fo3", "fnv"):
        settings.set_game_root_dir(game, "")
        settings.set_game_extracted_dir(game, "")

    run = immapp.run

    def render(runner_params, **kwargs):
        params = runner_params
        params.ini_disable = True
        params.app_window_params.hidden = True
        params.app_window_params.window_geometry.size = (args.width, args.height)
        params.app_window_params.window_geometry.window_size_measure_mode = hello_imgui.WindowSizeMeasureMode.screen_coords
        params.dpi_aware_params.dpi_window_size_factor = args.scale
        frames = 0

        def finish():
            nonlocal frames
            frames += 1
            if frames == 3 and not args.screen.startswith("setup-"):
                if args.screen.startswith("settings-"):
                    app._settings_window.open(args.screen.split("-", 1)[1])
                app._show_about = args.screen == "about"
                app._show_theme_selector = args.screen == "theme"
                if args.screen == "help":
                    app._active_ws.toggle_user_guide()
                if args.workspace == "mod_builder" and args.screen in {"loading", "loading-known"}:
                    builder = app._active_ws._app
                    builder._running = True
                    builder._loading_label = "Preparing release"
                    builder._progress_message = "Processing audio and textures for the selected mod..."
                    builder._progress_fraction = .58 if args.screen == "loading-known" else None
                    builder._progress_lines = ["Reading source files", "Building plugin", "Processing audio and textures"]
            if frames >= 12:
                params.app_shall_exit = True

        params.callbacks.post_render_dockable_windows = finish
        run(runner_params=params, **kwargs)

    if args.screen.startswith("setup-"):
        with patch("ui.toolkit.setup_wizard.detect_game_path", return_value=None):
            wizard = SetupWizard(settings)
        wizard._step = int(args.screen.split("-")[1])
        with patch("ui.toolkit.setup_wizard.immapp.run", render):
            wizard.run()
    else:
        variant = replace(get_variant(args.variant), include_ai_panel=False)
        workspaces = create_workspaces(settings, workspace_ids=variant.workspace_ids)
        app = ToolkitApp(workspaces, settings, app_variant=variant)
        with patch("ui.toolkit.app.immapp.run", render):
            app.run()
        assert app._active_ws.id == args.workspace
        assert app._active_ws._initialized
    Image.fromarray(hello_imgui.final_app_window_screenshot()).save(args.output)


if __name__ == "__main__":
    main()
