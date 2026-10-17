"""Actual ImGui mouse interaction in the supported toolkit, using safe fixtures."""
from __future__ import annotations

import argparse
import json
import struct
import threading
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from imgui_bundle import hello_imgui, imgui, immapp
from PIL import Image

from creation_lib.esp.native_runtime import (
    plugin_handle_load, plugin_handle_close, plugin_handle_call,
    plugin_handle_record_subrecords, plugin_handle_add_record_raw,
)

from ui.tests.test_esp_editor_workflow import plugin_file
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces
from ui.esp_editor import app as editor_module


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--expect-collapse", action="store_true")
    parser.add_argument("--category-only", action="store_true")
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(Path(__file__).resolve().parents[2])
    fixtures = output.parent / "navigation-fixtures"
    fixtures.mkdir(parents=True, exist_ok=True)
    plugin_file(fixtures, "B21_NavBase.esm")
    plugin_file(fixtures, "B21_NavPatch.esp", masters=("B21_NavBase.esm",), value=2)
    for name in ("B21_NavBase.esm", "B21_NavPatch.esp"):
        handle = plugin_handle_load(str(fixtures / name), game="fo4")
        data = plugin_handle_record_subrecords(handle, 0x800)
        data[0] = ("EDID", b"B21_Zebra\0", None)
        plugin_handle_call(handle, "set_record_subrecords", 0x800, data)
        if name.endswith(".esp"):
            for form_id, editor_id, full_name, signature in (
                (0x01000802, "B21_Alpha", "Unique Search Name", "MISC"),
                (0x01000801, "B21_Middle", "Middle item", "MISC"),
                (0x01000A00, "B21_Keyword", "Keyword", "KYWD"),
            ):
                plugin_handle_add_record_raw(handle, signature, form_id, 0, 0, 131, 0,
                    [("EDID", editor_id.encode() + b"\0", None), ("FULL", full_name.encode() + b"\0", None),
                     ("DATA", struct.pack("<if", 5, 1), None)])
        plugin_handle_call(handle, "save", str(fixtures / name))
        plugin_handle_close(handle)
    settings = ToolkitSettings(path=fixtures / "isolated.json", editor_settings_path=fixtures / "absent.json")
    settings.save = lambda: None
    settings.setup_complete = True
    settings.active_workspace = "esp_editor"
    for game in ("fo4", "fo76", "skyrimse", "starfield", "fo3", "fnv"):
        settings.set_game_root_dir(game, "")
        settings.set_game_extracted_dir(game, "")
        settings._paths[game]["additional_paths"] = []
    variant = replace(get_variant("esp_editor"), include_ai_panel=False)
    app = ToolkitApp(create_workspaces(settings, workspace_ids=variant.workspace_ids), settings, app_variant=variant)
    runner, tree = immapp.run, imgui.tree_node
    frames, stage, age = 0, "load", 0
    position, target, opened = None, None, False
    trace, ids = [], set()
    positions, visible_ids, texts, checks, actions = {}, [], [], [], []
    iterator, action, action_age = None, None, 0
    analysis_release = threading.Event()
    counts = editor_module.plugin_handle_record_counts
    def delayed_counts(*args):
        assert analysis_release.wait(timeout=10)
        return counts(*args)
    release = threading.Event()
    signatures, summaries = editor_module._get_group_api()
    def delayed_summaries(*args):
        assert release.wait(timeout=10)
        return summaries(*args)
    def tracked_tree(label, *args, **kwargs):
        nonlocal position, target, opened
        identity = imgui.get_id(label)
        result = tree(label, *args, **kwargs)
        if "grp_" in label and label.endswith("_MISC"):
            lo, hi = imgui.get_item_rect_min(), imgui.get_item_rect_max()
            position = (lo.x + 10, (lo.y + hi.y) / 2)
            target, opened = label, result
            ids.add(identity)
            positions["category"] = position
            trace.append({"frame": frames, "label": label, "id": identity, "open": result})
        return result
    def remember(label):
        key = label.split("###")[-1] if "###" in label else label.removeprefix("##")
        lo, hi = imgui.get_item_rect_min(), imgui.get_item_rect_max()
        positions[key] = ((lo.x + hi.x) / 2, (lo.y + hi.y) / 2)
    original_selectable, original_combo = imgui.selectable, imgui.begin_combo
    original_button, original_small, original_checkbox = imgui.button, imgui.small_button, imgui.checkbox
    original_input, original_text = imgui.input_text_with_hint, imgui.text_disabled
    original_header = imgui.collapsing_header
    def selectable(label, *args, **kwargs):
        result = original_selectable(label, *args, **kwargs)
        remember(label)
        if "###rec_" in label:
            visible_ids.append(int(label.rsplit("_", 1)[1]))
        return result
    def track(function):
        def wrapped(label, *args, **kwargs):
            result = function(label, *args, **kwargs)
            remember(label)
            return result
        return wrapped
    def disabled_text(text, *args):
        texts.append(text)
        return original_text(text, *args)
    def workflow(editor):
        plugin = editor.session.get_by_name("B21_NavPatch.esp")
        source = editor.session.get_by_name("B21_NavBase.esm")
        order = [(p.handle, p.load_order_index) for p in editor.session.plugins]
        yield ("until", lambda: visible_ids == [0x800, 0x01000801, 0x01000802])
        yield ("click", f"rec_{plugin.handle}_{0x01000801}")
        yield ("until", lambda: editor.selection.fields is not None)
        assert editor.selection.record.form_id == 0x01000801
        yield ("click", "category")
        yield ("frames", 3)
        assert not opened
        yield ("click", "category")
        yield ("until", lambda: opened and len(visible_ids) == 3)
        checks.append("Repeated category clicks retained loaded rows and selection")
        yield ("click", "search_field")
        yield ("click", "search_field_option_1")
        yield ("type", "filter_text", "Zebra")
        yield ("until", lambda: visible_ids == [0x800])
        assert editor.selection.record.form_id == 0x01000801
        yield ("click", "search_field")
        yield ("click", "search_field_option_2")
        yield ("type", "filter_text", "0x00000800")
        yield ("until", lambda: visible_ids == [0x800])
        yield ("click", "search_field")
        yield ("click", "search_field_option_3")
        yield ("type", "filter_text", "Unique Search Name")
        yield ("until", lambda: visible_ids == [0x01000802] and not editor._filter_futures)
        yield ("click", "search_field")
        yield ("click", "search_field_option_4")
        yield ("type", "filter_text", "Misc. Item")
        yield ("until", lambda: len(visible_ids) == 3)
        checks.append("Actual search dropdown and keyboard input cover Editor ID, prefixed FormID, Name and record type; hidden selection retained")
        yield ("click", "search_field")
        yield ("click", "search_field_option_0")
        yield ("type", "filter_text", "")
        yield ("click", "record_sort")
        yield ("click", "record_sort_option_1")
        yield ("until", lambda: visible_ids == [0x01000802, 0x01000801, 0x800])
        yield ("click", "Descending##record_direction")
        yield ("until", lambda: visible_ids == [0x800, 0x01000801, 0x01000802])
        assert [(p.handle, p.load_order_index) for p in editor.session.plugins] == order
        yield ("click", "record_type")
        yield ("click", "record_type_option_1")  # KYWD sorts before MISC
        yield ("until", lambda: not visible_ids and "No matching records." in texts)
        assert opened and editor.selection.record.form_id == 0x01000801
        yield ("click", "record_type")
        yield ("click", "record_type_option_0")
        yield ("type", "filter_text", "NO_SUCH_MATCH")
        yield ("until", lambda: "No matching records." in texts and not visible_ids)
        yield ("type", "filter_text", "")
        yield ("until", lambda: len(visible_ids) == 3)
        checks.append("Sort/type/empty-result switching preserves expansion, selection and plugin priority")
        yield ("click", "nav_legend")
        yield ("frames", 3)
        yield ("click", "Overrides overview##open_overview")
        yield ("until", lambda: editor._show_overview)
        yield ("click", "Analyze load order##overview_analyze")
        yield ("until", lambda: editor._analysis_future is not None)
        yield ("click", "Cancel analysis##overview_cancel")
        assert editor._analysis_cancelled
        analysis_release.set()
        yield ("until", lambda: editor._analysis_future is None)
        yield ("click", "Analyze load order##overview_analyze")
        yield ("until", lambda: editor._scan_complete and editor._analysis_label == "Analysis complete")
        yield ("click", "overview_status")
        yield ("click", "overview_status_option_2")
        yield ("until", lambda: "No matching override chains." in texts)
        yield ("click", "overview_status")
        yield ("click", "overview_status_option_1")
        yield ("click", "Active plugin only##overview_scope")
        yield ("until", lambda: len(editor._overview_reports()) == 1)
        yield ("click", "overview_status")
        yield ("click", "overview_status_option_4")
        yield ("until", lambda: "No matching override chains." in texts)
        yield ("click", "overview_status")
        yield ("click", "overview_status_option_0")
        yield ("click", "Active plugin only##overview_scope")
        report = editor._overview_reports()[0]
        key = f"{report.winner.plugin_handle}_{report.winner.form_id}"
        yield ("click", f"overview_source_{key}")
        yield ("until", lambda: not editor._show_overview and editor.selection.fields is not None)
        assert editor.selection.plugin_handle == source.handle and editor.selection.record.form_id == 0x800
        yield ("click", "Overrides overview##open_overview")
        yield ("click", f"overview_winner_{key}")
        yield ("until", lambda: not editor._show_overview and editor.selection.fields is not None)
        assert editor.selection.plugin_handle == plugin.handle
        yield ("click", "Overrides overview##open_overview")
        yield ("type", "overview_query", "NO_MATCH")
        yield ("until", lambda: "No matching override chains." in texts)
        yield ("type", "overview_query", "")
        yield ("until", lambda: len(editor._overview_reports()) == 1)
        checks.append("Overview cancellation/retry, status/scope filters, source/winner navigation and empty results exercised with actual clicks")
    def render(runner_params, **kwargs):
        nonlocal frames, stage, age
        params = runner_params
        params.ini_disable = True
        params.app_window_params.hidden = True
        params.fps_idling.enable_idling = False
        params.app_window_params.window_geometry.size = (1600, 1000)
        def events():
            io = imgui.get_io()
            io.add_focus_event(True)
            visible_ids.clear()
            texts.clear()
            if stage == "click" and age == 0:
                io.add_mouse_pos_event(*position)
            elif stage == "click" and age in (1, 2):
                io.add_mouse_button_event(0, age == 1)
            if stage == "workflow" and action and action[0] in ("click", "type"):
                key = action[1]
                if action_age == 0:
                    assert key in positions, f"Widget unavailable: {key}; known: {sorted(positions)}"
                    io.add_mouse_pos_event(*positions[key])
                elif action_age in (1, 2):
                    io.add_mouse_button_event(0, action_age == 1)
                if action[0] == "type":
                    if action_age == 3:
                        io.add_key_event(imgui.Key.mod_ctrl, True)
                        io.add_key_event(imgui.Key.a, True)
                    elif action_age == 4:
                        io.add_key_event(imgui.Key.a, False)
                        io.add_key_event(imgui.Key.mod_ctrl, False)
                    elif action_age == 5:
                        io.add_key_event(imgui.Key.backspace, True)
                    elif action_age == 6:
                        io.add_key_event(imgui.Key.backspace, False)
                        io.add_input_characters_utf8(action[2])
        def finish():
            nonlocal frames, stage, age, iterator, action, action_age
            frames += 1
            age += 1
            editor = app._active_ws._app
            if stage == "load":
                editor.open_folder(str(fixtures))
                stage, age = "wait", 0
            elif stage == "wait" and target and not editor._busy:
                stage, age = "click", 0
            elif stage == "click" and age >= 5:
                assert opened, "Category did not receive its first actual mouse click"
                stage, age = "loading", 0
            elif stage == "loading" and age == 6:
                release.set()
            elif stage == "loading" and age > 6 and not editor._group_futures:
                stage, age = "loaded", 0
            elif stage == "loaded" and age >= 5:
                assert editor._latest_error is None, editor._latest_error
                if args.expect_collapse:
                    assert not opened and len(ids) > 1, "Expected original async-label collapse was not reproduced"
                else:
                    assert opened and len(ids) == 1, "Category collapsed or changed identity after asynchronous completion"
                if args.category_only or args.expect_collapse:
                    params.app_shall_exit = True
                else:
                    iterator = workflow(editor)
                    stage, action_age = "workflow", 0
                    action = next(iterator)
            elif stage == "workflow":
                action_age += 1
                done = (action[0] == "click" and action_age >= 5 or action[0] == "type" and action_age >= 10
                        or action[0] == "frames" and action_age >= action[1]
                        or action[0] == "until" and action_age >= 2 and action[1]())
                if done:
                    actions.append({"frame": frames, "action": action[0], "widget": action[1] if isinstance(action[1], str) else "condition"})
                    try:
                        action = next(iterator)
                        action_age = 0
                    except StopIteration:
                        assert editor._latest_error is None, editor._latest_error
                        params.app_shall_exit = True
                        stage = "complete"
                if action_age > 120:
                    analysis_release.set()
                    raise RuntimeError(f"UI action stalled: {action}; search={editor._filter_text!r}, field={editor._search_field}, rows={visible_ids}, open={opened}, selection={editor.selection.record}, overview={editor._show_overview}, error={editor._latest_error}")
            if frames > 1200:
                release.set()
                analysis_release.set()
                raise RuntimeError(f"Navigation test stalled in {stage}: {trace[-5:]}")
        params.callbacks.pre_new_frame = events
        params.callbacks.post_render_dockable_windows = finish
        runner(runner_params=params, **kwargs)
    with patch("ui.toolkit.app.immapp.run", render), patch.object(imgui, "tree_node", tracked_tree), \
         patch.object(editor_module, "_plugin_handle_group_record_summaries", delayed_summaries), \
         patch.object(editor_module, "plugin_handle_record_counts", delayed_counts), \
         patch.object(imgui, "selectable", selectable), patch.object(imgui, "begin_combo", track(original_combo)), \
         patch.object(imgui, "button", track(original_button)), patch.object(imgui, "small_button", track(original_small)), \
         patch.object(imgui, "checkbox", track(original_checkbox)), patch.object(imgui, "input_text_with_hint", track(original_input)), \
         patch.object(imgui, "collapsing_header", track(original_header)), \
         patch.object(imgui, "text_disabled", disabled_text):
        app.run()
    output.write_text(json.dumps({"passed": True, "collapse_reproduced": args.expect_collapse,
                                  "frames": frames, "checks": checks, "actions": actions, "category_trace": trace}, indent=2), encoding="utf-8")
    Image.fromarray(hello_imgui.final_app_window_screenshot()).save(output.with_suffix(".png"))
    print(f"Actual category-click {'reproduction' if args.expect_collapse else 'verification'} passed: {output}")


if __name__ == "__main__":
    main()
