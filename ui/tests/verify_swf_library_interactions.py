"""Drive real ImGui mouse/text/wheel events against a disposable large library."""
from __future__ import annotations

import argparse
from collections import deque
from dataclasses import replace
import json
from pathlib import Path
import time
from uuid import uuid4
from unittest.mock import patch

from imgui_bundle import hello_imgui, imgui, immapp
from PIL import Image

from ui.tests.benchmark_swf_library import make_fixture
from ui.tests.test_swf_editor_workflow import SVG
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    directory = args.output.resolve()
    directory.relative_to(Path(__file__).resolve().parents[2])
    directory.mkdir(parents=True, exist_ok=True)
    base, other = directory / 'large.db', directory / 'other.db'
    make_fixture(base, 25000)
    make_fixture(other, 700)
    svg, invalid = directory / 'B21_Import.svg', directory / 'invalid.svg'
    svg.write_text(SVG, encoding='utf-8')
    invalid.write_text('broken', encoding='utf-8')
    from creation_lib.swf.parser import SwfDocument, SwfHeader
    from creation_lib.swf.tags import DefineShapeTag, EndTag
    from creation_lib.swf.svg_io import svg_to_shapes
    from creation_lib.swf.types import RECT
    from creation_lib.swf.writer import write_swf
    import copy
    template = svg_to_shapes(SVG)[0]
    tags = []
    for index in range(2000):
        item = copy.deepcopy(template)
        item.shape_id = index + 1
        tags.append(DefineShapeTag(item, 32))
    slow = directory / 'B21_Cancel.swf'
    slow.write_bytes(write_swf(SwfDocument(SwfHeader('FWS', 10, 0, RECT(0, 800, 0, 600), 24, 1), tags=[*tags, EndTag()])))
    from ui.swf_editor.panels import library_panel as module
    original = module.LibraryPanel
    personal = directory / f'personal-{uuid4().hex}.db'
    class FixturePanel(original):
        def __init__(self, editor):
            super().__init__(editor, db_paths={'fo4':base, 'skyrimse':other}, cache_dir=directory / 'cache',
                             user_db=personal)
    settings = ToolkitSettings(path=directory / 'isolated-settings.json', editor_settings_path=directory / 'missing.json')
    settings.save = lambda: None
    settings.setup_complete = True
    settings.active_workspace = 'swf'
    for game in ('fo4','fo76','skyrimse','starfield','fo3','fnv'):
        settings.set_game_root_dir(game, '')
        settings.set_game_extracted_dir(game, '')
        settings._paths[game]['additional_paths'] = []
    variant = replace(get_variant('full'), workspace_ids=('swf',), default_workspace='swf', include_ai_panel=False)
    app = ToolkitApp(create_workspaces(settings, workspace_ids=('swf',)), settings, app_variant=variant)
    run, begin = immapp.run, imgui.begin
    frames, phase, ticks = 0, 0, 0
    trace, events = [], deque()
    started = time.monotonic()
    focus_label = 'Library##swf'
    pick = {'path': svg}
    resources = []
    baseline_total = None
    def click(bounds, delay=0):
        a, b, c, d = bounds
        events.append((frames + delay, lambda: (imgui.get_io().add_mouse_pos_event((a+c)/2, (b+d)/2),
                                               imgui.get_io().add_mouse_button_event(0, True))))
        events.append((frames + delay + 1, lambda: imgui.get_io().add_mouse_button_event(0, False)))
    def edit_search(panel, value):
        click(panel._control_rects['search'])
        def select_all():
            imgui.get_io().add_key_event(imgui.Key.mod_ctrl, True)
            imgui.get_io().add_key_event(imgui.Key.a, True)
        events.append((frames+2, select_all))
        def replace_text():
            io = imgui.get_io()
            io.add_key_event(imgui.Key.a, False)
            io.add_key_event(imgui.Key.mod_ctrl, False)
        events.append((frames+3, replace_text))
        events.append((frames+4, lambda: imgui.get_io().add_key_event(imgui.Key.backspace, True)))
        events.append((frames+5, lambda: imgui.get_io().add_key_event(imgui.Key.backspace, False)))
        if value:
            events.append((frames+6, lambda: imgui.get_io().add_input_characters_utf8(value)))
    def focus(label, *a, **kw):
        if label == focus_label:
            imgui.set_next_window_focus()
        return begin(label, *a, **kw)
    def runner(runner_params, **kwargs):
        params = runner_params
        params.ini_disable = True
        params.app_window_params.hidden = True
        params.app_window_params.window_geometry.size = (1600,1000)
        params.fps_idling.enable_idling = False
        def finish():
            nonlocal frames, phase, focus_label, ticks, baseline_total
            frames += 1
            editor = app._ws_map['swf']._app
            if editor is None:
                return
            panel = editor.library_panel
            c = panel.controller
            if c.error:
                raise AssertionError(c.error)
            resources.append((len(panel._textures), len(c.pixels), len(c.pages), len(c._work)))
            assert len(panel._textures) <= 128 and len(c.pixels) <= 128 and len(c.pages) <= 8 and len(c._work) <= 8
            if phase == 0 and c.total == 25000 and len(panel._textures) >= 3:
                edit_search(panel, 'B21_Asset024999')
                phase = 1
            elif phase == 1 and c.total == 1 and panel._last_visible and panel._last_visible[0].name == 'B21_Asset024999':
                asset = panel._last_visible[0]
                assert asset.user_added
                rect = panel._tile_rects[asset.key]
                click(rect)
                click(rect, 3)
                trace.append('Typed into native search; reached User asset beyond old 200-row cutoff; double-clicked its tile')
                phase = 2
            elif phase == 2 and len(editor.scene.library_symbols) == 1:
                assert c.selection.name == 'B21_Asset024999'
                saved_shape = editor.scene.library_symbols[1]
                editor.undo()
                assert not editor.scene.library_symbols
                editor.redo()
                assert editor.scene.library_symbols[1] == saved_shape
                project = directory / 'placed.swfproj'
                editor.document.save(project)
                editor.open_document(project)
                assert editor.scene.library_symbols[1] == saved_shape and not editor.document.dirty
                editor.document.export(directory / 'placed.swf')
                edit_search(panel, '')
                trace.append('Asynchronous placement committed once; undo/redo and save/reopen/export preserved geometry')
                phase = 3
            elif phase == 3 and c.total == 25000 and panel._search_query == '':
                a,b,x,y = panel._grid_bounds
                imgui.get_io().add_mouse_pos_event((a+x)/2, (b+y)/2)
                imgui.get_io().add_mouse_wheel_event(0, -100000)
                phase = 4
            elif phase == 4 and panel._last_visible and panel._last_visible[-1].name == 'B21_Asset024999':
                trace.append('Native wheel scrolled to final asset; paged metadata and visible previews remained bounded')
                panel.set_game(1)
                assert c.selection is None and c.shape is None
                phase = 5
            elif phase == 5 and c.total == 700:
                assert c.selection is None
                panel.set_game(0)
                trace.append('Changed games with overlapping row IDs; old selection and late jobs could not commit')
                phase = 6
            elif phase == 6 and c.total == 25000:
                click(panel._control_rects['import'])
                phase = 7
            elif phase == 7 and 'Added 1' in c.import_status and c.total == 25001:
                editor.scene.selection = [(0,0)]
                click(panel._control_rects['save_selection'])
                phase = 8
            elif phase == 8 and not c.importing and c.total == 25002:
                baseline_total = c.total
                c.request_import(slow)
                phase = 9
            elif phase == 9 and c.importing and 'cancel' in panel._control_rects:
                click(panel._control_rects['cancel'])
                phase = 10
            elif phase == 10 and not c.importing and not c.loading:
                assert c.total == baseline_total, (c.total, baseline_total, c.import_status)
                assert 'cancelled' in c.import_status.lower()
                pick['path'] = invalid
                ticks = frames
                phase = 10.5
            elif phase == 10.5 and frames - ticks >= 3:
                click(panel._control_rects['import'])
                trace.append('Imported SVG and saved canvas artwork via native buttons; cancelled large SWF import without adding rows')
                phase = 11
            elif phase == 11 and 'Import failed' in c.import_status:
                assert c.total == baseline_total
                focus_label = 'Properties##swf'
                ticks = frames
                phase = 12
            elif phase == 12 and frames - ticks > 30:
                assert not c._wanted
                focus_label = 'Library##swf'
                ticks = frames
                phase = 13
            elif phase == 13 and frames - ticks > 30:
                trace.append('Invalid import reported visibly; hid and reopened Library without losing document or resources')
                params.app_shall_exit = True
            due = [event for event in events if event[0] <= frames]
            for event in due:
                events.remove(event)
                event[1]()
            if time.monotonic() - started > 55:
                raise AssertionError(f'Native interaction stalled at phase {phase}: query={panel._search_query!r}, total={c.total}, import={c.import_status}, selection={c.selection_error}')
        params.callbacks.post_render_dockable_windows = finish
        run(runner_params=params, **kwargs)
    with patch.object(module, 'LibraryPanel', FixturePanel), patch('ui.toolkit.app.immapp.run', runner), patch.object(imgui, 'begin', focus), \
            patch('creation_lib.ui.widgets.pick_folder.pick_file', lambda *a, **kw: str(pick['path'])):
        app.run()
    assert phase == 13, trace
    panel = app._ws_map['swf']._app.library_panel
    assert panel._closed and not panel._textures and panel.controller._closed
    evidence = dict(frames=frames, trace=trace, metrics=panel.controller.metrics,
                    max_resources=dict(textures=max(r[0] for r in resources), pixel_tiles=max(r[1] for r in resources),
                                       pages=max(r[2] for r in resources), jobs=max(r[3] for r in resources)),
                    repeated_cleanup='All owned textures and worker released by ToolkitApp cleanup')
    (directory / 'interactions.json').write_text(json.dumps(evidence, indent=2), encoding='utf-8')
    Image.fromarray(hello_imgui.final_app_window_screenshot()).save(directory / 'interactions.png')
    print(json.dumps(evidence))


if __name__ == '__main__':
    main()
