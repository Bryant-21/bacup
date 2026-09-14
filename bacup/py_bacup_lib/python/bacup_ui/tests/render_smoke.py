"""Render the real BACUP UI with isolated settings and simulated conversion state."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from imgui_bundle import hello_imgui, imgui, immapp
from PIL import Image

from bacup_ui.appalachia.appalachia_workspace import AppalachiaWorkspace
from bacup_ui.setup import BacupProjectPicker, BacupProjectSetup
from bacup_ui.variant import BACUP_VARIANT
from creation_lib.ui.theme import get_theme
from creation_lib.ui.theme.appearance import configure_runner_appearance
from ui.toolkit.settings import ToolkitSettings


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--screen", default="main")
    parser.add_argument("--state", default="idle", choices=("idle", "running", "error", "cancelled", "completed"))
    parser.add_argument("--theme", default="falloutnv")
    parser.add_argument("--scale", type=float, default=1)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=760)
    parser.add_argument("--frames", type=int, default=12)
    parser.add_argument("--fallback-font", action="store_true")
    logs = parser.add_mutually_exclusive_group()
    logs.add_argument("--hide-logs", action="store_true")
    logs.add_argument("--show-logs", action="store_true")
    parser.add_argument("--music", action="store_true")
    parser.add_argument("--expand-sections", action="store_true")
    parser.add_argument("--settings-scroll", type=float, default=0)
    parser.add_argument("--deploy-ready", action="store_true")
    parser.add_argument("--space", choices=("measuring", "healthy", "low", "unavailable", "split"), default="measuring")
    parser.add_argument("--extra-projects", type=int, default=0)
    parser.add_argument("--project", default="appalachia")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    collapsing_header = imgui.collapsing_header

    def expanded_header(label, *positional, **kwargs):
        if args.expand_sections:
            imgui.set_next_item_open(True)
        return collapsing_header(label, *positional, **kwargs)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    settings = ToolkitSettings(path=args.output.parent / "fixture-settings.json",
                               editor_settings_path=args.output.parent / "no-editor-settings.json",
                               variant_id="appalachia")
    settings.save = lambda: None
    settings.theme = args.theme
    settings.setup_complete = True
    settings.active_workspace = "appalachia"
    from bacup_ui.setup import set_active_project

    set_active_project(settings, args.project)
    settings.set_workspace_settings("appalachia", {"show_logs": args.show_logs})
    if args.extra_projects:
        from dataclasses import replace
        from bacup_ui import setup as setup_module
        from bacup_ui.appalachia import appalachia_workspace as workspace_module

        for index in range(args.extra_projects):
            key = f"future-{index}"
            setup_module.PROJECT_PROFILES[key] = replace(setup_module.PROJECT_PROFILES["appalachia"],
                id=key, title=f"A future project with a longer name {index + 1}")
        projects = tuple((p.id, p.title, p.conversion_id) for p in setup_module.PROJECT_PROFILES.values())
        workspace_module._PROJECTS = workspace_module._ENABLED_PROJECTS = projects
    for game in ("fo4", "fo76", "fnv", "fo3", "skyrimse", "starfield"):
        settings.set_game_root_dir(game, str(args.output.parent / "fixture-games" / game))
        settings.set_game_extracted_dir(game, str(args.output.parent / "fixture-games" / game / "Data"))
    workspace = AppalachiaWorkspace(settings)
    frames = 0
    original_initialize = workspace.initialize
    params = hello_imgui.RunnerParams()
    params.app_window_params.window_title = "BACUP — UI verification"
    params.app_window_params.window_geometry.size = (args.width, args.height)
    params.app_window_params.window_geometry.window_size_measure_mode = hello_imgui.WindowSizeMeasureMode.screen_coords
    params.app_window_params.hidden = True
    params.ini_disable = True
    params.dpi_aware_params.dpi_window_size_factor = args.scale
    configure_runner_appearance(params, get_theme(args.theme))
    picker = BacupProjectPicker(settings) if args.screen == "picker" else None
    setup = BacupProjectSetup(settings, args.project) if args.screen.startswith("setup-") else None
    if setup:
        setup.step = int(args.screen.split("-")[1])
        setup._space_prepare_started = True
        setup._space_prepare_state = lambda: (False, "", 0)
        setup._poll_space_prepare = lambda: None
        setup._start_space_prepare = lambda: None
        if setup.step == setup.STEP_EXTRACT:
            setup._extractor = SimpleNamespace(progress=.42, status="Extracting source assets from a long archive filename.ba2…",
                                               done=False, results={}, error="")

    def initialize_workspace():
        original_initialize()
        workspace.active = True
        for panel in workspace._regen_panels.values():
            if args.settings_scroll:
                draw_settings = panel._draw_settings_column

                def scrolled_settings(draw_settings=draw_settings):
                    imgui.set_scroll_y(args.settings_scroll)
                    draw_settings()

                panel._draw_settings_column = scrolled_settings
            panel.install_location = "mo2"
            panel.install_path = str(args.output.parent / "fixture-output" / "A very long project folder name")
            panel._detect_ba2_target = lambda: ("og", "1.10.163")
            panel._detected_installed_version = lambda: "Not installed"
            panel._store_install_result = lambda _game: SimpleNamespace(ok=True, store="steam")
            panel.disk_usage_summary = lambda: {"mod_ba2": 0, "deployed_ba2": 0}
            panel.disk_usage_loading = lambda: False
            panel._disk_space_projection = lambda **_kwargs: None
            if args.space != "measuring":
                from bacup_ui.conversion.panels.regen_panel import _CONVERSION_SPACE_ESTIMATES, _DiskSpaceVolume

                gib = 1024**3
                loose_bytes, packed_bytes = _CONVERSION_SPACE_ESTIMATES[panel._pair().pair_id]
                required_bytes = loose_bytes + packed_bytes
                free = -1 if args.space == "unavailable" else required_bytes // 2 if args.space == "low" else 780 * gib
                volumes = (_DiskSpaceVolume("I:", args.output.parent, ("loose workspace", "packed BA2s"),
                                            required_bytes, 2000 * gib, free),)
                if args.space == "split":
                    volumes = (
                        _DiskSpaceVolume("I:", args.output.parent, ("loose workspace",), loose_bytes, 2000 * gib, 780 * gib),
                        _DiskSpaceVolume("J:", args.output.parent, ("packed BA2s",), packed_bytes, 1000 * gib, 400 * gib),
                    )
                panel._disk_space_projection = lambda volumes=volumes, **_kwargs: volumes
            panel.can_convert = lambda: args.state not in {"running"}
            panel.can_deploy_existing = lambda: args.deploy_ready and args.state != "running"
            if args.deploy_ready:
                panel._deploy_existing_hint = lambda: ""
            panel._log_panel._entries = [("INFO", "Ready. Select conversion options to begin.")]
        panel = workspace._regen_panel
        if args.music:
            from bacup_ui.conversion.music import MusicPlayerState

            music_state = MusicPlayerState(
                active=True, paused=False,
                current_track=Path("mus_maintheme_with_a_long_track_name.wav"),
                track_index=0, track_count=5, position_seconds=30,
                duration_seconds=120, volume=.12,
            )
            panel.music_muted = False
            panel._music_player = SimpleNamespace(snapshot=lambda: music_state, stop=lambda: None)
        if args.state != "idle":
            phases = [
                {"ui_key": "prepare", "phase_name": "Prepare source assets", "status": "completed"},
                {"ui_key": "convert_textures", "phase_name": "Convert textures", "status": "completed" if args.state == "completed" else args.state,
                 "total_items": 100, "completed_items": 62, "current_item": "textures/landscape/long-file-name.dds", "error": "Example failure; details are in the log."},
                {"ui_key": "convert_havok", "phase_name": "Convert Havok", "status": "running" if args.state == "running" else "skipped"},
            ]
            for phase in phases:
                panel.handle_event({"type": "item_progress", "data": phase})
            panel._runner_status = "Converting textures and animation assets"
            panel._progress_succeeded = args.state == "completed"
            panel._log_panel._entries += [("WARN", "Example diagnostic message with a long asset path that wraps inside the log panel.")]
        if args.state == "running":
            workspace._runner = SimpleNamespace(done=False, drain=lambda: [], cancel=lambda: None)
            workspace._runner_owner = panel
        if args.screen == "changelog":
            workspace._changelog_pending = True
        elif args.screen == "confirmation":
            workspace._setup_confirm_pending = True
        elif args.screen in {"cleanup", "cleanup-busy"}:
            panel._cleanup_dialog_open = True
            panel._cleanup_status = "scanning" if args.screen == "cleanup-busy" else "idle"
            panel._cleanup_targets = tuple(SimpleNamespace(key=key, label=label, size_bytes=size) for key, label, size in (
                ("source", "Extracted source game assets", 180 * 1024**3),
                ("cache", "Conversion workspace and temporary files", 120 * 1024**3),
            ))
            panel._cleanup_selected = {"cache"}
        elif args.screen == "preflight":
            panel._preflight_report = SimpleNamespace(required_missing=[SimpleNamespace(
                label="Source archive assets", checked_path=str(args.output.parent / "fixture-games" / "A very long folder name" / "meshes"),
                fix_hint="Use project setup to extract the required source game assets.")], optional_missing=[])
        elif args.screen == "space-warning":
            panel._low_space_warning = [SimpleNamespace(unavailable=False, key="I:", path=args.output.parent,
                labels=["Conversion workspace", "Packed output"], required_bytes=180 * 1024**3, free_bytes=90 * 1024**3)]
        elif args.screen == "completion":
            panel._completion = {"deployed": False, "elapsed_seconds": 5025, "mod_path": str(args.output.parent / "fixture-output" / "A very long project folder name")}

    def draw():
        nonlocal frames
        if not workspace._initialized and picker is None and setup is None:
            initialize_workspace()
        if picker:
            picker._draw()
        elif setup:
            setup._draw()
        else:
            viewport = imgui.get_main_viewport()
            imgui.set_next_window_pos(viewport.work_pos)
            imgui.set_next_window_size(viewport.work_size)
            workspace._draw_projects()
        frames += 1
        if args.screen in {"changelog", "confirmation"}:
            workspace._draw_changelog_popup()
            workspace._draw_setup_confirm_popup()
        if frames >= args.frames:
            args.output.with_suffix(".json").write_text(json.dumps({
                "font_size": imgui.get_font_size(), "font_scale_dpi": imgui.get_style().font_scale_dpi,
                "dpi_factor": hello_imgui.dpi_window_size_factor(), "display_size": list(imgui.get_io().display_size),
            }), encoding="utf-8")
            params.app_shall_exit = True

    params.callbacks.show_gui = draw
    if args.fallback_font:
        params.callbacks.load_additional_fonts = lambda: imgui.get_io().fonts.add_font_default()
    # Render-only fixtures never start extraction, deployment, or cleanup work.
    with patch("bacup_ui.conversion.panels.regen_panel.get_exe_dir", lambda: args.output.parent), \
         patch.object(imgui, "collapsing_header", expanded_header), \
         patch("bacup_ui.setup.get_exe_dir", lambda: args.output.parent):
        if args.screen in {"host", "preferences", "preferences-paths", "preferences-extraction", "about", "theme", "toolkit"}:
            from ui.toolkit.app import ToolkitApp
            from ui.toolkit.variants import get_variant

            workspace.initialize = initialize_workspace
            app = ToolkitApp([workspace], settings, app_variant=get_variant("full") if args.screen == "toolkit" else BACUP_VARIANT)
            if args.screen.startswith("preferences"):
                section_id = "paths" if args.screen.endswith("paths") else "indexes" if args.screen.endswith("extraction") else "general"
                app._settings_window.open(section_id)
            app._show_about = args.screen == "about"
            app._show_theme_selector = args.screen == "theme"
            run = immapp.run

            def run_host(runner_params, **kwargs):
                nonlocal params
                params = runner_params
                params.ini_disable = True
                params.app_window_params.hidden = True
                params.app_window_params.window_geometry.size = (args.width, args.height)
                params.app_window_params.window_geometry.window_size_measure_mode = hello_imgui.WindowSizeMeasureMode.screen_coords
                params.dpi_aware_params.dpi_window_size_factor = args.scale

                def finish_frame():
                    nonlocal frames
                    frames += 1
                    if frames >= args.frames:
                        params.app_shall_exit = True
                params.callbacks.post_render_dockable_windows = finish_frame
                run(runner_params=params, **kwargs)

            with patch("ui.toolkit.app.immapp.run", run_host):
                app.run()
        else:
            immapp.run(params)
    Image.fromarray(hello_imgui.final_app_window_screenshot()).save(args.output)


if __name__ == "__main__":
    main()
