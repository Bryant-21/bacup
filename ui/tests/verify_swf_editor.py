"""Render and exercise SWF workflows through the supported toolkit runner.

All settings, sources, SWFs, preview state and screenshots are disposable.
uv run python -m ui.tests.verify_swf_editor --output .codex/tmp/swf-editor
"""
from __future__ import annotations

import argparse
from dataclasses import replace
import json
import logging
from pathlib import Path
from unittest.mock import patch

from imgui_bundle import hello_imgui, immapp, imgui
from PIL import Image

from ui.tests.test_swf_editor_workflow import artwork, compiled
from ui.swf_editor.document import SwfEditingDocument
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces


def main():
    logging.basicConfig(level=logging.INFO, force=True)
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--view", choices=("art", "source", "edit", "structure", "inspector", "diagnostics", "quit", "combined"), default="source")
    args = parser.parse_args()
    directory = args.output.resolve()
    directory.relative_to(Path(__file__).resolve().parents[2])
    directory.mkdir(parents=True, exist_ok=True)
    settings = ToolkitSettings(path=directory / "isolated-settings.json", editor_settings_path=directory / "absent.json")
    settings.save = lambda: None
    settings.setup_complete = True
    settings.active_workspace = "swf"
    settings.window_width, settings.window_height = 1600, 1000
    for game in ("fo4", "fo76", "skyrimse", "starfield", "fo3", "fnv"):
        settings.set_game_root_dir(game, "")
        settings.set_game_extracted_dir(game, "")
        settings._paths[game]["additional_paths"] = []
    variant = replace(get_variant("full"), include_ai_panel=False)
    app = ToolkitApp(create_workspaces(settings, workspace_ids=("swf", "esp_editor", "mod_manager")), settings, app_variant=variant)
    runner = immapp.run
    begin_tab = imgui.begin_tab_item
    button = imgui.button
    begin_window = imgui.begin
    frames, initialized, requested = 0, False, False
    trace = []
    selected_tab = {"source": "AS3 Sources", "edit": "AS3 Sources", "structure": "Structure", "inspector": "ABC Inspector",
                    "diagnostics": "Build & Preview", "quit": "AS3 Sources", "combined": "AS3 Sources"}.get(args.view)
    combined_stage = 0

    def select_tab(label, *positional, **kwargs):
        if label == selected_tab:
            kwargs["flags"] = imgui.TabItemFlags_.set_selected
        return begin_tab(label, *positional, **kwargs)

    def choose(label, *positional, **kwargs):
        real = button(label, *positional, **kwargs)
        if args.view in {"quit", "combined"} and label == "Cancel" and frames == 24:
            trace.append("cancel quit")
            return True
        if args.view == "combined" and label == "Discard all and quit" and frames == 33:
            trace.append("discard disposable ESP fixture")
            return True
        return real

    def focus_window(label, *positional, **kwargs):
        wanted = "Viewport##swf" if args.view == "art" else "Document##swf"
        if label == wanted and (args.view != "edit" or frames < 6):
            imgui.set_next_window_focus()
        return begin_window(label, *positional, **kwargs)

    def render(runner_params, **kwargs):
        nonlocal frames, initialized, requested
        params = runner_params
        params.ini_disable = True
        params.app_window_params.hidden = True
        params.app_window_params.window_geometry.size = (1600, 1000)
        params.fps_idling.enable_idling = False
        def checked(callback):
            def call(*a, **kw):
                try:
                    return callback(*a, **kw)
                except BaseException:
                    import traceback
                    traceback.print_exc()
                    raise
            return call
        for window in params.docking_params.dockable_windows:
            window.gui_function = checked(window.gui_function)
        params.callbacks.show_gui = checked(params.callbacks.show_gui)
        params.callbacks.show_menus = checked(params.callbacks.show_menus)

        def finish():
            nonlocal frames, initialized, requested, combined_stage
            frames += 1
            if frames < 3:
                print(f"SWF render frame {frames}", flush=True)
            editor = app._ws_map["swf"]._app
            if not initialized and editor:
                if args.view == "art":
                    editor._replace_document(artwork())
                    editor.scene.selection = [(0, 0)]
                elif args.view in {"structure", "inspector"}:
                    path = directory / "fixture-menu.swf"
                    path.write_bytes(compiled())
                    editor.open_document(path)
                    editor.document_panel.method = "value"
                    if args.view == "inspector":
                        editor.document_panel.inspect("disassembly")
                else:
                    editor._replace_document(SwfEditingDocument.menu())
                    if args.view == "diagnostics":
                        editor.document.sources["DocumentMenu.as"] = "package { public class }"
                        editor.start_build()
                initialized = True
                if args.view == "combined":
                    from ui.tests.test_esp_editor_workflow import plugin_file
                    path = plugin_file(directory, "B21_QuitFixture.esp")
                    assert app._ensure_initialized(app._ws_map["esp_editor"])
                    app._ws_map["esp_editor"]._app.open_plugin(str(path))
            if args.view == "combined" and initialized:
                esp = app._ws_map["esp_editor"]._app
                esp.poll()
                if not esp._busy and esp.session.plugins and combined_stage == 0:
                    esp._dirty_handles.add(esp.session.plugins[0].handle)
                    combined_stage = 1
            if args.view == "edit" and initialized:
                if frames == 9:
                    editor.document_panel.editor.set_cursor(type(editor.document_panel.editor).DocPos(0, 0))
                    editor.document_panel.editor.set_focus()
                if frames == 10:
                    imgui.get_io().add_input_characters_utf8("// B21_WIDGET_EDIT\n")
                if frames == 14:
                    assert "B21_WIDGET_EDIT" in editor.document.sources["DocumentMenu.as"]
                    assert editor.document.dirty
                    path = directory / "widget-edited.swfproj"
                    editor.document.save(path)
                    reopened = SwfEditingDocument.open(path)
                    assert "B21_WIDGET_EDIT" in reopened.sources["DocumentMenu.as"]
                    assert reopened.build()
                    editor.start_build()
                    trace.append("typed into AS3 widget; saved/reopened and compiled current source")
                if frames == 16:
                    imgui.get_io().add_input_characters_utf8("// B21_SECOND_EDIT\n")
                if frames == 18:
                    assert editor.document.dirty
                    imgui.get_io().add_key_event(imgui.Key.mod_ctrl, True)
                    imgui.get_io().add_key_event(imgui.Key.s, True)
                if frames == 19:
                    imgui.get_io().add_key_event(imgui.Key.s, False)
                    imgui.get_io().add_key_event(imgui.Key.mod_ctrl, False)
                if frames == 21:
                    assert not editor.document.dirty, editor.status_text
                    assert "B21_SECOND_EDIT" in SwfEditingDocument.open(editor.document.path).sources["DocumentMenu.as"]
                    imgui.get_io().add_key_event(imgui.Key.f5, True)
                    trace.append("Ctrl+S saved the second edit while source widget had focus")
                if frames == 22:
                    imgui.get_io().add_key_event(imgui.Key.f5, False)
            if args.view in {"quit", "combined"} and initialized and not requested and frames >= 15:
                assert not editor.document.dirty, "Drawing an untouched source must not dirty its buffers"
                editor.document.sources["DocumentMenu.as"] += "\n// unsaved fixture"
                assert app._switch_workspace("mod_manager", remember=False)
                app._request_quit()
                app._request_quit()
                requested = True
            if args.view in {"quit", "combined"} and frames == 27:
                assert not app._quit_pending
                assert editor.document.dirty
                assert not app._ws_map["swf"].active
                trace.append("cancel preserves dirty source")
                if args.view == "combined":
                    assert esp._dirty_handles
                    assert editor._pending_action is None, "SWF must wait for the ESP modal before opening its own"
                    trace.append("ESP cancel preserves both editors; SWF did not stack a second modal")
                app._request_quit()
            if frames > 200:
                raise RuntimeError("SWF verification did not finish")

        params.callbacks.post_render_dockable_windows = finish
        confirm = params.callbacks.before_imgui_render
        def stop():
            confirm()
            if frames >= 45:
                editor = app._ws_map["swf"]._app
                if args.view == "diagnostics":
                    assert editor.status_text.startswith("Error"), editor.status_text
                if args.view == "edit":
                    assert editor.compiled_key == editor.document.state_key(), editor.status_text
                    trace.append("F5 built the saved source while the widget had focus")
                if args.view in {"quit", "combined"}:
                    assert app._quit_pending and editor._pending_for_exit
                    if editor._pending_draw_frame != imgui.get_frame_count():
                        trace.append("BLOCKED: ESP discard reopened its modal over the waiting SWF guard")
                        print("Combined quit blocked by repeated ESP discard prompt", flush=True)
                    else:
                        trace.append("repeated quit renders SWF guard")
                # Harness termination only; discard these generated fixtures.
                params.app_shall_exit = True
        params.callbacks.before_imgui_render = stop
        runner(runner_params=params, **kwargs)

    with patch("ui.toolkit.app.immapp.run", render), patch.object(imgui, "begin_tab_item", select_tab), patch.object(imgui, "button", choose), patch.object(imgui, "begin", focus_window):
        app.run()
    path = directory / f"{args.view}.png"
    Image.fromarray(hello_imgui.final_app_window_screenshot()).save(path)
    blocked = any(item.startswith("BLOCKED:") for item in trace)
    status = "blocked" if blocked else "passed"
    (directory / f"{args.view}-trace.json").write_text(json.dumps({"view": args.view, "status": status,
                                                                "frames": frames, "trace": trace}, indent=2))
    print(f"{status.title()} and rendered {args.view}: {path} ({frames} frames)")
    if blocked:
        raise SystemExit(2)


if __name__ == "__main__":
    main()
