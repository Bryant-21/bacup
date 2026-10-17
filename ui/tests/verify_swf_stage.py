"""Native HUD hierarchy/coordinate workflow on disposable copies only."""
from __future__ import annotations

import argparse
from collections import deque
from dataclasses import replace
import hashlib
import json
from pathlib import Path
import time
from unittest.mock import patch

from imgui_bundle import hello_imgui, imgui, immapp
from PIL import Image

from ui.swf_editor.movie import Movie
from ui.swf_editor.stage import CompiledStage, flatten
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--hud',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args = parser.parse_args()
    directory = args.output.resolve()
    directory.relative_to(Path.cwd().resolve())
    directory.mkdir(parents=True,exist_ok=True)
    original = args.hud.read_bytes()
    copy = directory/'hud-copy.swf'
    copy.write_bytes(original)
    initial = CompiledStage(original)
    target = next(n for n in flatten(initial.nodes()) if n.name == 'RightMeters_mc')
    settings = ToolkitSettings(path=directory/'settings.json',editor_settings_path=directory/'missing.json')
    settings.save = lambda:None
    settings.setup_complete = True
    settings.active_workspace = 'swf'
    for game in ('fo4','fo76','skyrimse','starfield','fo3','fnv'):
        settings.set_game_root_dir(game,'')
        settings.set_game_extracted_dir(game,'')
        settings._paths[game]['additional_paths'] = []
    fonts = args.hud.parent/'fonts_en.swf'
    font_bytes = fonts.read_bytes() if fonts.is_file() else None
    if font_bytes:
        interface = directory/'assets'/'Interface'
        interface.mkdir(parents=True,exist_ok=True)
        (interface/'fonts_en.swf').write_bytes(font_bytes)
        settings.set_game_extracted_dir('fo4',str(interface.parent))
    variant = replace(get_variant('full'),workspace_ids=('swf',),default_workspace='swf',include_ai_panel=False)
    app = ToolkitApp(create_workspaces(settings,workspace_ids=('swf',)),settings,app_variant=variant)
    run, begin = immapp.run,imgui.begin
    frames,phase,ticks = 0,0,0
    trace,events = [],deque()
    focus_label = 'Layers##swf'
    started = time.monotonic()
    saved = directory/'moved.swfproj'
    exported = directory/'moved.swf'
    def click(bounds):
        a,b,c,d = bounds
        events.append((frames,lambda:(imgui.get_io().add_mouse_pos_event((a+c)/2,(b+d)/2),imgui.get_io().add_mouse_button_event(0,True))))
        events.append((frames+1,lambda:imgui.get_io().add_mouse_button_event(0,False)))
    def focus(label,*a,**kw):
        if label == focus_label and phase not in {10,11}:
            imgui.set_next_window_focus()
        return begin(label,*a,**kw)
    changed_movie = None
    drag_baseline = None
    method_key = 'MeterBar::frame1'
    selected_tab = None
    begin_tab = imgui.begin_tab_item
    def select_tab(label,*args,**kwargs):
        if label == selected_tab:
            kwargs['flags'] = imgui.TabItemFlags_.set_selected
        return begin_tab(label,*args,**kwargs)
    def drag(panel,cancel=False):
        node = panel.selected()
        lo,scale,rect = panel._stage_geometry
        a,b,c,d = node.bounds
        x,y = lo[0]+((a+c)/2-rect[0])*scale,lo[1]+((b+d)/2-rect[1])*scale
        events.append((frames,lambda:(imgui.get_io().add_mouse_pos_event(x,y),imgui.get_io().add_mouse_button_event(0,True))))
        events.append((frames+1,lambda:imgui.get_io().add_mouse_pos_event(x+55,y-25)))
        if cancel:
            events.append((frames+2,lambda:imgui.get_io().add_key_event(imgui.Key.escape,True)))
            events.append((frames+3,lambda:imgui.get_io().add_key_event(imgui.Key.escape,False)))
        events.append((frames+4,lambda:imgui.get_io().add_mouse_button_event(0,False)))
    def runner(runner_params,**kwargs):
        params = runner_params
        params.ini_disable = True
        params.app_window_params.hidden = True
        params.app_window_params.window_geometry.size = (1700,1050)
        params.fps_idling.enable_idling = False
        def finish():
            nonlocal frames,phase,ticks,focus_label,changed_movie,drag_baseline,selected_tab
            frames += 1
            editor = app._ws_map['swf']._app
            if not editor:
                return
            panel = editor.stage_panel
            if phase == 0 and frames > 8:
                editor.open_document(copy)
                trace.append('Opened disposable stock HUD copy')
                phase = 1
            elif phase == 1 and 'node:RightMeters_mc' in panel._control_rects:
                click(panel._control_rects['node:RightMeters_mc'])
                phase = 2
            elif phase == 2 and panel.selection == target.key:
                focus_label = 'Properties##swf'
                ticks = frames
                phase = 3
            elif phase == 3 and frames-ticks > 8 and 'X' in panel._control_rects:
                click(panel._control_rects['X'])
                trace.append('Clicked actual hierarchy row and coordinate slider')
                ticks = frames
                phase = 4
            elif phase == 4 and frames-ticks > 8:
                assert editor.document.movie != original,editor.status_text
                changed_movie = editor.document.movie
                moved = next(n for n in flatten(CompiledStage(changed_movie).nodes()) if n.key == target.key)
                assert moved.matrix.translate_x != target.matrix.translate_x
                before,after = Movie(original),Movie(changed_movie)
                assert before.header == after.header
                assert all(a.encoded == b.encoded for i,(a,b) in enumerate(zip(before.tags,after.tags)) if i != target.address[0])
                trace.append('Changed only selected placement; all ABC and other tags stayed exact')
                editor.undo()
                assert editor.document.movie == original
                editor.redo()
                assert editor.document.movie == changed_movie
                editor.document.save(saved)
                editor.open_document(saved)
                assert editor.document.movie == changed_movie and not editor.document.dirty
                editor.export_swf(exported)
                assert exported.read_bytes() == changed_movie
                editor.push_undo('Unsaved fixture change')
                editor.document.sources['B21_Unsaved.as'] = '// unsaved'
                editor.new_document()
                assert editor._pending_action is not None
                editor.resolve_pending('cancel')
                assert editor.document.mode == 'binary'
                editor.document.sources.pop('B21_Unsaved.as')
                editor.document.mark_saved()
                trace.append('Undo/redo, save/reopen/export and cancelled unsaved replacement passed')
                focus_label = 'Viewport##swf'
                ticks = frames
                phase = 5
            elif phase == 5 and frames-ticks > 12 and panel._image_key == panel._requested_key and panel._texture:
                assert not panel.message,panel.message
                trace.append('Native HUD pixels and imported game fonts rendered without warnings')
                panel.selection = target.key
                drag_baseline = editor.document.movie
                drag(panel,cancel=True)
                ticks = frames
                phase = 6
            elif phase == 6 and frames-ticks > 9:
                assert editor.document.movie == drag_baseline and panel._drag is None
                trace.append('Actual viewport drag cancelled with Escape without changing the movie')
                drag(panel)
                ticks = frames
                phase = 7
            elif phase == 7 and frames-ticks > 9 and panel._image_key == panel._requested_key:
                assert editor.document.movie != drag_baseline
                changed_movie = editor.document.movie
                from creation_lib.swf import native_runtime as native
                before,after = Movie(drag_baseline),Movie(changed_movie)
                assert all(a.encoded == b.encoded for i,(a,b) in enumerate(zip(before.tags,after.tags)) if i != target.address[0])
                original_png = native.render_frame_image(drag_baseline,fonts=[font_bytes])['png']
                moved_png = native.render_frame_image(changed_movie,fonts=[font_bytes])['png']
                (directory/'before-drag.png').write_bytes(original_png)
                (directory/'after-drag.png').write_bytes(moved_png)
                assert original_png != moved_png
                trace.append('Dragged actual HUD pixels; only selected placement and its rendered position changed')
                editor.document_panel.class_name = 'MeterBar'
                editor.document_panel.method = 'frame1'
                focus_label = 'Document##swf'
                selected_tab = 'ABC Inspector'
                ticks = frames
                phase = 8
            elif phase == 8 and frames-ticks > 8 and 'recover_method' in panel._control_rects:
                click(panel._control_rects['recover_method'])
                phase = 9
            elif phase == 9 and method_key in editor.document.sources:
                assert 'stop()' in editor.document.sources[method_key]
                selected_tab = 'AS3 Sources'
                ticks = frames
                phase = 10
            elif phase == 10 and frames-ticks == 8:
                widget = editor.document_panel.editor
                widget.set_cursor(type(widget).DocPos(0,0))
                widget.set_focus()
            elif phase == 10 and frames-ticks == 10:
                imgui.get_io().add_input_characters_utf8('gotoAndStop(2);\n')
            elif phase == 10 and frames-ticks > 15:
                assert 'gotoAndStop(2)' in editor.document.sources[method_key]
                editor.document.save(saved)
                reopened = type(editor.document).open(saved)
                assert 'gotoAndStop(2)' in reopened.sources[method_key]
                editor.start_build(export_path=directory/'method-edited.swf')
                phase = 11
            elif phase == 11 and not editor.jobs.busy:
                assert editor.compiled_bytes,editor.status_text
                from creation_lib.swf import native_runtime as native
                report = native.abc_disassemble(editor.compiled_bytes,'MeterBar','frame1')
                assert 'gotoAndStop' in json.dumps(report)
                before,after = Movie(changed_movie),Movie(editor.compiled_bytes)
                assert all(a.encoded == b.encoded for a,b in zip(before.tags,after.tags) if a.code not in {72,82})
                trace.append('Clicked native method recovery; typed into real AS3 widget; saved/reopened and compiled/exported only that method')
                editor.document.mark_saved()
                focus_label = 'Viewport##swf'
                selected_tab = None
                ticks = frames
                phase = 12
            elif phase == 12 and frames-ticks > 12 and panel._image_key == panel._requested_key:
                drag_baseline = editor.document.movie
                drag(panel)
                ticks = frames
                phase = 13
            elif phase == 13 and frames-ticks == 2:
                assert panel._drag is not None
                focus_label = 'Document##swf'
                selected_tab = 'ABC Inspector'
            elif phase == 13 and frames-ticks > 9:
                assert editor.document.movie == drag_baseline and panel._drag is None
                trace.append('Hiding the viewport during an active drag cancelled it; release while hidden made no edit')
                saved_source = editor.document.sources[method_key]
                editor.document_panel.recover_method()
                assert editor.document.sources[method_key] == saved_source and not editor.jobs.busy
                trace.append('Opening the same existing method again preserved the edited source buffer')
                focus_label = 'Viewport##swf'
                selected_tab = None
                ticks = frames
                phase = 14
            elif phase == 14 and frames-ticks > 12 and panel._image_key == panel._requested_key:
                assert args.hud.read_bytes() == original
                trace.append('Original HUD unchanged; modern native UI rendered')
                params.app_shall_exit = True
            for event in list(events):
                if event[0] <= frames:
                    events.remove(event)
                    event[1]()
            if time.monotonic()-started > 35:
                raise AssertionError(f'Stage interaction stalled at {phase}: {editor.status_text}')
        params.callbacks.post_render_dockable_windows = finish
        run(runner_params=params,**kwargs)
    with patch('ui.toolkit.app.immapp.run',runner),patch.object(imgui,'begin',focus),patch.object(imgui,'begin_tab_item',select_tab):
        app.run()
    assert phase == 14
    evidence = dict(frames=frames,trace=trace,visible_placements=len(list(flatten(initial.nodes()))),
                    original_sha256=hashlib.sha256(original).hexdigest(),
                    edited_sha256=hashlib.sha256(changed_movie).hexdigest(),
                    rendering='Native authored HUD pixels with companion fonts; no external renderer',
                    warnings=app._ws_map['swf']._app.stage_panel.message)
    (directory/'interactions.json').write_text(json.dumps(evidence,indent=2),encoding='utf-8')
    Image.fromarray(hello_imgui.final_app_window_screenshot()).save(directory/'interactions.png')
    print(json.dumps(evidence))


if __name__ == '__main__':
    main()
