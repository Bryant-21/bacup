"""Render the RE Database workspace against the redb fixture DB and save a screenshot.

    python -m ui.tests.render_re_db --output tmp/re_db.png
"""
from __future__ import annotations

import argparse
import tempfile
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from imgui_bundle import hello_imgui, immapp
from PIL import Image

from ui.tests.test_re_db_workspace import build_fixture_db
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces
from ui.toolkit.workspaces.re_db_session import RedbSession


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--query", default="sum_loop")
    parser.add_argument("--width", type=int, default=1600)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    db_dir = build_fixture_db(Path(tempfile.mkdtemp(prefix="redb_ui_")))

    settings = ToolkitSettings(path=args.output.parent / "fixture-settings.json",
                               editor_settings_path=args.output.parent / "no-editor-settings.json")
    settings.save = lambda: None
    settings.setup_complete = True
    settings.active_workspace = "re_db"
    settings.window_width, settings.window_height = args.width, args.height
    settings.set_workspace_settings("re_db", {"db_dir": str(db_dir)})
    variant = replace(get_variant("full"), workspace_ids=("re_db",), default_workspace="re_db",
                      include_ai_panel=False)
    app = ToolkitApp(create_workspaces(settings, workspace_ids=("re_db",)), settings, app_variant=variant)
    run = immapp.run

    def render(runner_params, **kwargs):
        params = runner_params
        params.ini_disable = True
        params.app_window_params.hidden = True
        params.app_window_params.window_geometry.size = (args.width, args.height)
        frames = 0

        def finish():
            nonlocal frames
            frames += 1
            if frames == 3:
                ws = app._active_ws
                assert ws.session.db_dir == db_dir
                ws._query = args.query
                ws.run_search()
                ws.session.wait()
                ws.select_result(next(i for i, r in enumerate(ws.session.results)
                                      if RedbSession.result_ref(r)))
                ws.session.wait()
            if frames >= 12:
                params.app_shall_exit = True

        params.callbacks.post_render_dockable_windows = finish
        run(runner_params=params, **kwargs)

    with patch("ui.toolkit.app.immapp.run", render):
        app.run()
    assert app._active_ws.id == "re_db" and app._active_ws.session.view.detail
    Image.fromarray(hello_imgui.final_app_window_screenshot()).save(args.output)


if __name__ == "__main__":
    main()
