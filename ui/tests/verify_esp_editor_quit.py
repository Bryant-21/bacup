"""Exercise real GLFW/Win32 close events with disposable ESP editor state.

This uses the supported ToolkitApp runner, isolated settings and test plugins.
Buttons are selected programmatically after their actual modal is displayed.
"""
from __future__ import annotations

import argparse
import ctypes
import json
import struct
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from imgui_bundle import hello_imgui, immapp, imgui

from creation_lib.esp.native_runtime import (
    RecordSummary, plugin_handle_call, plugin_handle_close, plugin_handle_load,
    plugin_handle_record_subrecords,
)
from ui.esp_editor import app as editor_module
from ui.tests.test_esp_editor_workflow import plugin_file
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--with-mod-manager", action="store_true", help="Verify the concurrently added workspace in the full shell")
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(Path(__file__).resolve().parents[2])
    fixtures = output.parent / "quit-fixtures"
    fixtures.mkdir(parents=True, exist_ok=True)
    base_path = plugin_file(fixtures, "B21_QuitBase.esm", value=25)
    patch_path = plugin_file(fixtures, "B21_QuitPatch.esp", masters=(base_path.name,), value=75)
    settings = ToolkitSettings(path=fixtures / "isolated.json", editor_settings_path=fixtures / "absent.json")
    settings.save = lambda: None
    settings.setup_complete = True
    settings.active_workspace = "esp_editor"
    for game in ("fo4", "fo76", "skyrimse", "starfield", "fo3", "fnv"):
        settings.set_game_root_dir(game, "")
        settings.set_game_extracted_dir(game, "")
        settings._paths[game]["additional_paths"] = []
    variant = replace(get_variant("full" if args.with_mod_manager else "esp_editor"), include_ai_panel=False)
    workspace_ids = ("esp_editor", "mod_manager") if args.with_mod_manager else ("esp_editor",)
    app = ToolkitApp(create_workspaces(settings, workspace_ids=workspace_ids), settings, app_variant=variant)
    runner = immapp.run
    button = imgui.button
    backup = editor_module._make_backup
    state, clicked, frames = "load", "", 0
    trace = []
    backup_failed = False
    handles = set()

    def choose(label, *args, **kwargs):
        nonlocal clicked
        if label == clicked:
            clicked = ""
            trace.append({"frame": frames, "button": label})
            return True
        return button(label, *args, **kwargs)

    def fail_one_backup(path):
        nonlocal backup_failed
        if Path(path).name == base_path.name and not backup_failed:
            backup_failed = True
            raise OSError("fixture backup failure")
        return backup(path)

    def native_close():
        glfw = app._glfw_close_dll
        glfw.glfwGetWin32Window.argtypes = [ctypes.c_void_p]
        glfw.glfwGetWin32Window.restype = ctypes.c_void_p
        hwnd = glfw.glfwGetWin32Window(hello_imgui.get_glfw_window_address())
        user32 = ctypes.WinDLL("user32", use_last_error=True)
        user32.PostMessageW.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_size_t, ctypes.c_ssize_t]
        user32.PostMessageW.restype = ctypes.c_int
        assert hwnd and user32.PostMessageW(hwnd, 0x0010, 0, 0)  # WM_CLOSE
        trace.append({"frame": frames, "event": "native WM_CLOSE"})

    def render(runner_params, **kwargs):
        params = runner_params
        params.ini_disable = True
        params.app_window_params.hidden = True
        params.fps_idling.enable_idling = False
        def finish():
            nonlocal frames, state, clicked, handles
            frames += 1
            editor = app._ws_map["esp_editor"]._app
            if state == "load":
                editor.open_folder(str(fixtures))
                state = "select"
            elif state == "select" and not editor._busy and editor.session.plugins:
                plugin = editor.session.get_by_name(patch_path.name)
                editor._select_record(plugin.handle, RecordSummary(0x800, "MISC"))
                state = "edit"
            elif state == "edit" and editor.selection.fields:
                index = next(i for i, f in enumerate(editor.selection.fields) if f.signature == "DATA")
                field = editor.selection.fields[index]
                field.components[0].value = 1234
                editor._commit_field(index, field)
                base = editor.session.get_by_name(base_path.name)
                data = plugin_handle_record_subrecords(base.handle, 0x800)
                data = [(sig, struct.pack("<if", 5678, 2.5) if sig == "DATA" else raw, semantic)
                        for sig, raw, semantic in data]
                plugin_handle_call(base.handle, "set_record_subrecords", 0x800, data)
                editor._dirty_handles.add(base.handle)
                handles = set(editor._dirty_handles)
                assert len(handles) == 2
                if args.with_mod_manager:
                    assert app._switch_workspace("mod_manager", remember=False)
                else:
                    app._active_ws.on_deactivate()
                assert not app._ws_map["esp_editor"].active
                native_close()
                state = "cancel_prompt"
            elif state == "cancel_prompt" and editor._exit_popup_active:
                native_close()  # repeated window close while the prompt is open
                state = "repeat_before_cancel"
            elif state == "repeat_before_cancel":
                clicked = "Cancel"
                state = "cancelled"
            elif state == "cancelled" and not app._quit_pending:
                assert not params.app_shall_exit and editor._dirty_handles == handles
                trace.append({"frame": frames, "check": "inactive editor cancelled; both edits retained"})
                native_close()
                state = "failure_prompt"
            elif state == "failure_prompt" and editor._exit_popup_active:
                clicked = "Save all and quit"
                state = "failed_save"
            elif state == "failed_save" and not editor._busy and backup_failed and editor._latest_error:
                assert not params.app_shall_exit and len(editor._dirty_handles) == 1
                trace.append({"frame": frames, "check": "partial save failure blocked quit; failed plugin remains dirty"})
                native_close()
                state = "failure_repeat_before_cancel"
            elif state == "failure_repeat_before_cancel":
                clicked = "Cancel"
                state = "failure_cancelled"
            elif state == "failure_cancelled" and not app._quit_pending:
                assert not params.app_shall_exit and len(editor._dirty_handles) == 1
                trace.append({"frame": frames, "check": "repeated close after failed save cancelled safely"})
                native_close()
                state = "success_prompt"
            elif state == "success_prompt" and editor._exit_popup_active:
                clicked = "Save all and quit"
                state = "success"
            if frames > 600:
                raise RuntimeError(f"Quit workflow stalled in {state}: {editor._latest_error}; trace: {trace[-8:]}")
        params.callbacks.post_render_dockable_windows = finish
        cleanup = params.callbacks.before_exit
        def verify_cleanup():
            editor = app._ws_map["esp_editor"]._app
            assert state == "success" and not editor._busy and not editor._dirty_handles
            trace.append({"frame": frames, "check": "cleanup reached only after all saves succeeded"})
            cleanup()
        params.callbacks.before_exit = verify_cleanup
        runner(runner_params=params, **kwargs)

    with patch("ui.toolkit.app.immapp.run", render), patch.object(imgui, "button", choose), patch.object(editor_module, "_make_backup", fail_one_backup):
        app.run()
    for path, expected in ((base_path, 5678), (patch_path, 1234)):
        handle = plugin_handle_load(str(path), game="fo4")
        try:
            raw = next(raw for sig, raw, _ in plugin_handle_record_subrecords(handle, 0x800) if sig == "DATA")
            assert struct.unpack_from("<i", raw)[0] == expected
        finally:
            plugin_handle_close(handle)
    output.write_text(json.dumps({"passed": True, "frames": frames, "mod_manager_active": args.with_mod_manager, "trace": trace,
                                  "reopened_values": {base_path.name: 5678, patch_path.name: 1234}}, indent=2), encoding="utf-8")
    print(f"Native quit workflow passed in {frames} frames: {output}")


if __name__ == "__main__":
    main()
