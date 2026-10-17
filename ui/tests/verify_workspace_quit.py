"""Real window closes and mouse clicks across dirty ESP/SWF, using fixtures."""
from __future__ import annotations

import argparse
import ctypes
from dataclasses import replace
import json
from pathlib import Path
import struct
from unittest.mock import patch

from imgui_bundle import hello_imgui, immapp, imgui

from creation_lib.esp.native_runtime import (
    RecordSummary, plugin_handle_close, plugin_handle_load, plugin_handle_record_subrecords,
)
from ui.esp_editor import app as esp_module
from ui.swf_editor.document import SwfEditingDocument
from ui.tests.test_esp_editor_workflow import plugin_file
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--first", choices=("esp_editor", "swf"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(Path(__file__).resolve().parents[2])
    fixtures = output.parent / f"fixtures-{args.first}"
    fixtures.mkdir(parents=True, exist_ok=True)
    esp_path = plugin_file(fixtures, "B21_Transaction.esp", value=75)
    swf_path = fixtures / "B21_Transaction.swfproj"
    settings = ToolkitSettings(path=fixtures / "isolated.json", editor_settings_path=fixtures / "absent.json")
    settings.save = lambda: None
    settings.setup_complete = True
    settings.active_workspace = "esp_editor"
    for game in ("fo4", "fo76", "skyrimse", "starfield", "fo3", "fnv"):
        settings.set_game_root_dir(game, "")
        settings.set_game_extracted_dir(game, "")
        settings._paths[game]["additional_paths"] = []
    variant = replace(get_variant("full"), include_ai_panel=False)
    app = ToolkitApp(create_workspaces(settings, workspace_ids=("esp_editor", "swf", "mod_manager")), settings, app_variant=variant)
    first = app._ws_map[args.first]
    second = app._ws_map["swf" if args.first == "esp_editor" else "esp_editor"]
    app._workspaces = [first, second, app._ws_map["mod_manager"]]
    original_runner, original_button = immapp.run, imgui.button
    original_backup, original_save = esp_module._make_backup, SwfEditingDocument.save
    positions, trace, checks = {}, [], []
    frames, stage, iterator, action, action_age = 0, "load", None, None, 0
    esp, swf, expected_value, expected_source = None, None, 75, None
    fail_save, failure_seen, complete = False, False, False

    def button(label, *a, **kw):
        result = original_button(label, *a, **kw)
        lo, hi = imgui.get_item_rect_min(), imgui.get_item_rect_max()
        positions[label] = ((lo.x + hi.x) / 2, (lo.y + hi.y) / 2)
        if result:
            trace.append({"frame": frames, "click": label})
        return result

    def backup(path):
        nonlocal failure_seen
        if fail_save and second.id == "esp_editor":
            failure_seen = True
            raise OSError("combined fixture backup failure")
        return original_backup(path)

    def save(document, path):
        nonlocal failure_seen
        if fail_save and second.id == "swf":
            failure_seen = True
            raise OSError("combined fixture SWF save failure")
        return original_save(document, path)

    def native_close():
        glfw = app._glfw_close_dll
        glfw.glfwGetWin32Window.argtypes = [ctypes.c_void_p]
        glfw.glfwGetWin32Window.restype = ctypes.c_void_p
        hwnd = glfw.glfwGetWin32Window(hello_imgui.get_glfw_window_address())
        user32 = ctypes.WinDLL("user32", use_last_error=True)
        user32.PostMessageW.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_size_t, ctypes.c_ssize_t]
        user32.PostMessageW.restype = ctypes.c_int
        assert hwnd and user32.PostMessageW(hwnd, 0x0010, 0, 0)
        trace.append({"frame": frames, "event": "native WM_CLOSE"})

    def edit(workspace, label):
        nonlocal expected_value
        if workspace.id == "esp_editor":
            index = next(i for i, field in enumerate(esp.selection.fields) if field.signature == "DATA")
            field = esp.selection.fields[index]
            expected_value += 1
            field.components[0].value = expected_value
            esp._commit_field(index, field)
            assert not esp.selection.document.edit_errors[index], esp.selection.document.edit_errors[index]
            data = next(raw for sig, raw, _ in plugin_handle_record_subrecords(esp.selection.plugin_handle, 0x800) if sig == "DATA")
            assert struct.unpack_from("<i", data)[0] == expected_value
        else:
            swf.document.sources["DocumentMenu.as"] += f"\n// {label}"
        trace.append({"frame": frames, "edit": workspace.id, "label": label})

    def waiting(workspace):
        if app._quit_waiting is not workspace:
            return False
        if workspace.id == "esp_editor":
            return esp._exit_popup_active and "Discard all and quit" in positions
        return swf._pending_for_exit and "Discard edits" in positions

    def approved(workspace):
        return id(workspace) in app._quit_approvals

    def dirty(workspace):
        return bool(esp._dirty_handles) if workspace.id == "esp_editor" else swf.document.dirty

    def discard(workspace):
        return "Discard all and quit" if workspace.id == "esp_editor" else "Discard edits"

    def save_label(workspace):
        return "Save all and quit" if workspace.id == "esp_editor" else "Save project"

    def workflow(params):
        nonlocal fail_save, complete, expected_source
        # Discard followed by another workspace's Cancel preserves both edits.
        yield ("close",)
        yield ("until", lambda: waiting(first))
        yield ("click", discard(first))
        yield ("until", lambda: approved(first) and waiting(second))
        assert dirty(first) and dirty(second)
        yield ("close",)
        yield ("frames", 3)
        assert approved(first) and waiting(second) and not params.app_shall_exit
        yield ("click", "Cancel")
        yield ("until", lambda: not app._quit_pending)
        assert dirty(first) and dirty(second) and not app._quit_approvals
        assert not esp._exit_popup_active and not swf._pending_for_exit
        checks.append("First discard persisted across repeated WM_CLOSE; second Cancel preserved both dirty editors and reset approvals")

        # Changed reviewed content must take control back from the later modal.
        yield ("close",)
        yield ("until", lambda: waiting(first))
        yield ("click", discard(first))
        yield ("until", lambda: approved(first) and waiting(second))
        edit(first, "changed after discard approval")
        yield ("until", lambda: not approved(first) and waiting(first))
        assert not swf._pending_for_exit if second.id == "swf" else not esp._exit_popup_active
        yield ("click", "Cancel")
        yield ("until", lambda: not app._quit_pending)
        assert dirty(first) and dirty(second)
        checks.append("New edits invalidated a discard approval and closed the later modal before re-prompting")

        # A failed second save leaves the first approval and dirty data intact.
        yield ("close",)
        yield ("until", lambda: waiting(first))
        yield ("click", discard(first))
        yield ("until", lambda: approved(first) and waiting(second))
        unchanged_destinations = (esp_path.read_bytes(), swf_path.read_bytes())
        fail_save = True
        yield ("click", save_label(second))
        yield ("until", lambda: failure_seen and not esp._busy and waiting(second))
        assert dirty(first) and dirty(second) and approved(first) and not params.app_shall_exit
        assert (esp_path.read_bytes(), swf_path.read_bytes()) == unchanged_destinations
        yield ("close",)
        yield ("frames", 3)
        assert waiting(second) and approved(first) and not params.app_shall_exit
        yield ("click", "Cancel")
        yield ("until", lambda: not app._quit_pending)
        assert dirty(first) and dirty(second) and not app._quit_approvals
        fail_save = False
        checks.append("Save failure and repeated close could not bypass dirty data; Cancel reset the failed transaction")

        # Even a saved approval must be reconsidered if new dirty data appears.
        yield ("close",)
        yield ("until", lambda: waiting(first))
        yield ("click", save_label(first))
        yield ("until", lambda: approved(first) and waiting(second))
        assert not dirty(first) and dirty(second)
        edit(first, "changed after save approval")
        yield ("until", lambda: not approved(first) and waiting(first))
        yield ("click", "Cancel")
        yield ("until", lambda: not app._quit_pending)
        assert dirty(first) and dirty(second)
        checks.append("New edits invalidated a saved approval; Cancel retained the new edits")

        # Finish through actual successful save prompts, then reopen both files.
        yield ("close",)
        yield ("until", lambda: waiting(first))
        yield ("click", save_label(first))
        yield ("until", lambda: approved(first) and waiting(second))
        expected_source = swf.document.sources["DocumentMenu.as"]
        complete = True
        yield ("click", save_label(second))
        yield ("until", lambda: params.app_shall_exit)

    def run(runner_params, **kwargs):
        params = runner_params
        params.ini_disable = True
        params.app_window_params.hidden = True
        params.app_window_params.window_geometry.size = (1600, 1000)
        params.fps_idling.enable_idling = False
        def events():
            io = imgui.get_io()
            io.add_focus_event(True)
            if stage == "workflow" and action and action[0] == "click":
                if action_age == 0:
                    assert action[1] in positions, f"Missing actual button: {action[1]}"
                    io.add_mouse_pos_event(*positions[action[1]])
                elif action_age in (1, 2):
                    io.add_mouse_button_event(0, action_age == 1)
        def finish():
            nonlocal frames, stage, esp, swf, iterator, action, action_age
            frames += 1
            if stage == "load":
                esp = app._ws_map["esp_editor"]._app
                assert app._ensure_initialized(app._ws_map["swf"])
                swf = app._ws_map["swf"]._app
                swf._replace_document(SwfEditingDocument.menu())
                swf.document.path = swf_path
                assert swf.save_document()
                esp.open_plugin(str(esp_path))
                stage = "select"
            elif stage == "select" and not esp._busy and esp.session.plugins:
                esp._select_record(esp.session.active.handle, RecordSummary(0x800, "MISC"))
                stage = "prepare"
            elif stage == "prepare" and esp.selection.fields:
                edit(app._ws_map["esp_editor"], "initial unsaved ESP")
                edit(app._ws_map["swf"], "initial unsaved SWF")
                assert dirty(first) and dirty(second)
                assert app._switch_workspace("mod_manager", remember=False)
                assert not app._ws_map["esp_editor"].active and not app._ws_map["swf"].active
                iterator = workflow(params)
                action = next(iterator)
                stage, action_age = "workflow", 0
            elif stage == "workflow":
                action_age += 1
                done = False
                if action[0] == "close":
                    native_close()
                    done = True
                elif action[0] == "click":
                    done = action_age >= 5
                elif action[0] == "frames":
                    done = action_age >= action[1]
                elif action[0] == "until":
                    done = action_age >= 2 and action[1]()
                if done:
                    try:
                        action = next(iterator)
                        action_age = 0
                    except StopIteration:
                        stage = "complete"
                if action_age > 240:
                    raise RuntimeError(f"Quit action stalled: {action}; waiting={getattr(app._quit_waiting, 'id', None)}; approvals={list(app._quit_approvals)}; trace={trace[-8:]}")
            if frames > 2000:
                raise RuntimeError(f"Quit setup stalled in {stage}")
        cleanup = params.callbacks.before_exit
        def verify_cleanup():
            assert complete and not dirty(first) and not dirty(second) and not esp._busy
            assert app._active_ws.id == "mod_manager"
            checks.append("Cleanup ran only after both saves succeeded, with Mod Manager active")
            cleanup()
        params.callbacks.pre_new_frame = events
        params.callbacks.post_render_dockable_windows = finish
        params.callbacks.before_exit = verify_cleanup
        original_runner(runner_params=params, **kwargs)

    with patch("ui.toolkit.app.immapp.run", run), patch.object(imgui, "button", button), \
         patch.object(esp_module, "_make_backup", backup), patch.object(SwfEditingDocument, "save", save):
        app.run()
    handle = plugin_handle_load(str(esp_path), game="fo4")
    try:
        data = next(raw for sig, raw, _ in plugin_handle_record_subrecords(handle, 0x800) if sig == "DATA")
        assert struct.unpack_from("<i", data)[0] == expected_value
    finally:
        plugin_handle_close(handle)
    assert SwfEditingDocument.open(swf_path).sources["DocumentMenu.as"] == expected_source
    checks.append("Reopened ESP value and SWF source exactly match the final edits")
    output.write_text(json.dumps({"passed": True, "first": args.first, "frames": frames,
                                  "mod_manager_active": True, "checks": checks, "trace": trace,
                                  "reopened_esp_value": expected_value}, indent=2), encoding="utf-8")
    print(f"Combined native quit passed ({args.first} first), {frames} frames: {output}")


if __name__ == "__main__":
    main()
