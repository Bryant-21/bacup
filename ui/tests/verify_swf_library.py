"""Native toolkit rendering and frame profiling of disposable/read-only libraries."""
from __future__ import annotations

import argparse
from dataclasses import replace
import importlib.util
import json
from pathlib import Path
import sqlite3
import statistics
import time
from types import SimpleNamespace
from unittest.mock import patch

from imgui_bundle import hello_imgui, imgui, immapp
from PIL import Image

from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--database', type=Path, required=True)
    parser.add_argument('--baseline', action='store_true')
    parser.add_argument('--frames', type=int, default=180)
    args = parser.parse_args()
    directory = args.output.resolve()
    directory.relative_to(Path(__file__).resolve().parents[2])
    directory.mkdir(parents=True, exist_ok=True)
    database = args.database.resolve()
    from ui.swf_editor.panels import library_panel as panel_module
    if args.baseline:
        spec = importlib.util.spec_from_file_location('library_baseline_render', Path('.codex/tmp/swf-library/baseline_library_panel.py'))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        module._db_dir = lambda: directory / 'baseline-cache-ui'
        connect = sqlite3.connect
        module.sqlite3 = SimpleNamespace(Row=sqlite3.Row, connect=lambda path: connect(Path(path).resolve().as_uri()+'?mode=ro', uri=True))
        original_panel = module.LibraryPanel
    else:
        original_panel = panel_module.LibraryPanel
    timings, search_timings, trace, frame_work = [], [], [], {}
    class MeasuredPanel(original_panel):
        def __init__(self, editor):
            if args.baseline:
                super().__init__(editor)
                self._db_path = lambda: database
            else:
                super().__init__(editor, db_paths={'fo4': database}, cache_dir=directory / 'new-cache-ui',
                                 user_db=directory / 'personal-ui.db')

        def draw(self):
            start = time.perf_counter_ns()
            super().draw()
            elapsed = (time.perf_counter_ns() - start) / 1e6
            timings.append(elapsed)
            frame = imgui.get_frame_count()
            frame_work[frame] = frame_work.get(frame, 0) + elapsed

        def background_tick(self):
            if args.baseline:
                return
            start = time.perf_counter_ns()
            super().background_tick()
            frame = imgui.get_frame_count()
            frame_work[frame] = frame_work.get(frame, 0) + (time.perf_counter_ns() - start) / 1e6

        def close(self):
            if args.baseline:
                if self._loader:
                    self._loader.shutdown()
                for texture in self._tex_objects:
                    texture.release()
                self._tex_objects.clear()
            else:
                super().close()

        def _search(self):
            start = time.perf_counter_ns()
            super()._search()
            search_timings.append((time.perf_counter_ns() - start) / 1e6)

    settings = ToolkitSettings(path=directory / 'isolated-settings.json', editor_settings_path=directory / 'missing.json')
    settings.save = lambda: None
    settings.setup_complete = True
    settings.active_workspace = 'swf'
    settings.window_width, settings.window_height = 1600, 1000
    for game in ('fo4','fo76','skyrimse','starfield','fo3','fnv'):
        settings.set_game_root_dir(game, '')
        settings.set_game_extracted_dir(game, '')
        settings._paths[game]['additional_paths'] = []
    variant = replace(get_variant('full'), workspace_ids=('swf',), default_workspace='swf', include_ai_panel=False)
    app = ToolkitApp(create_workspaces(settings, workspace_ids=('swf',)), settings, app_variant=variant)
    run, begin = immapp.run, imgui.begin
    frames, phase = 0, 0
    started = time.monotonic()
    def focus(label, *a, **kw):
        if label == 'Library##swf':
            imgui.set_next_window_focus()
        return begin(label, *a, **kw)
    def render(runner_params, **kwargs):
        nonlocal frames
        params = runner_params
        params.ini_disable = True
        params.app_window_params.hidden = True
        params.app_window_params.window_geometry.size = (1600,1000)
        params.fps_idling.enable_idling = False
        def finish():
            nonlocal frames, phase
            frames += 1
            panel = app._ws_map['swf']._app.library_panel
            ready = args.baseline or (panel.controller.total > 0 and len(panel._textures) >= 3)
            if phase == 0 and frames >= 70 and ready:
                panel._search_query = panel._results[0]['name'] if args.baseline else panel._last_visible[0].name
                if args.baseline:
                    panel._dirty = True
                else:
                    panel.search_now()
                trace.append('changed search after initial thumbnails')
                phase = 1
            elif phase == 1 and frames >= 110 and ready:
                panel._search_query = ''
                if args.baseline:
                    panel._dirty = True
                else:
                    panel.search_now()
                trace.append('returned to initial results')
                phase = 2
            elif (phase == 2 and frames >= args.frames and ready
                  and (args.baseline or (not panel.controller.loading and all(
                      (asset.key, (64,96,128)[panel._thumb_size_idx]) in panel._textures
                      or (asset.key, (64,96,128)[panel._thumb_size_idx]) in panel.controller.errors
                      for asset in panel._last_visible)))):
                params.app_shall_exit = True
            if time.monotonic() - started > 45:
                raise RuntimeError(f'Library did not finish: {panel.controller.status}, {panel.controller.error}, {dict(panel.controller.errors)}')
        params.callbacks.post_render_dockable_windows = finish
        run(runner_params=params, **kwargs)
    with patch.object(panel_module, 'LibraryPanel', MeasuredPanel), patch('ui.toolkit.app.immapp.run', render), patch.object(imgui, 'begin', focus):
        app.run()
    tag = 'baseline' if args.baseline else 'replacement'
    sorted_times = sorted(timings)
    evidence = {'mode':tag,'database':str(database),'frames':frames,'trace':trace,
                'library_draw_ms':{'median':statistics.median(timings),'p95':sorted_times[int(len(sorted_times)*.95)],
                                   'max':max(timings),'first':timings[:5]}, 'synchronous_search_ms':search_timings}
    total_work = sorted(frame_work.values())
    evidence['library_total_frame_ms'] = dict(median=statistics.median(total_work), p95=total_work[int(len(total_work)*.95)], max=max(total_work))
    if not args.baseline:
        panel = app._ws_map['swf']._app.library_panel
        evidence.update(metrics=panel.controller.metrics, source_total=panel.controller.total)
        names = {asset.key:asset.name for page in panel.controller.pages.values() for asset in page}
        names.update({asset.key:asset.name for asset in panel._last_visible})
        evidence['unavailable_previews'] = [{'asset':names.get(key, key), 'size':size, 'reason':reason}
                                          for (key,size), reason in panel.controller.errors.items()]
    (directory / f'{tag}-frames.json').write_text(json.dumps(evidence,indent=2))
    Image.fromarray(hello_imgui.final_app_window_screenshot()).save(directory / f'{tag}-library.png')
    print(json.dumps(evidence))


if __name__ == '__main__':
    main()
