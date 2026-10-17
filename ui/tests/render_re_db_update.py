"""Render the RE Database "Game updated" flow against a simulated Steam update and save a screenshot.

    python -m ui.tests.render_re_db_update --output tmp/re_db_update.png [--added]

A tmp DB holds the old build (fixture a at 1.11.240.0) registered under a tmp manifest; the exe is
then replaced by fixture b at 1.11.244.0. Without --added the shot shows the check table and the
Add dialog; with --added it shows the dialog after Add, with a fake ingest's progress in Status.
Nothing touches binaries.yaml or db/re.
"""
from __future__ import annotations

import argparse
import sqlite3
import tempfile
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

import pytest
from imgui_bundle import hello_imgui, immapp
from PIL import Image

from tests.redb.test_game_update import NEW, OLD, game_dir, register, steam_update, write_manifest
from tools.re.redb import config, db, manifest
from ui.tests.test_re_db_update import FakeLauncher
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces
from ui.toolkit.workspaces.re_db_update import GameUpdates


def build(root: Path, mp: pytest.MonkeyPatch) -> tuple[Path, Path]:
    for env, sub in (("RE_ADDRLIB_DIR", "addrlib"), ("RE_COMMONLIB_DIR", "commonlib")):
        (root / sub).mkdir()
        mp.setenv(env, str(root / sub))
    (root / "addrlib" / "version-1-11-244-0.bin").write_bytes(b"")
    mp.setattr(config, "FINDINGS_ROOTS", [])
    mp.setattr(config, "RE_DIR", root / "db")
    folder = game_dir(root)
    man = write_manifest(root, folder, match_with="[fx-other]")
    mp.setattr(manifest, "_DEFAULT_PATH", man)
    con = db.open_db(root / "db" / "re.sqlite")
    register(con, man, OLD)
    con.close()
    steam_update(folder)
    return root / "db", man


def fake_ingest_progress(db_dir: Path) -> None:
    """What `ingest NEW` would have written a few minutes in: register done, pe running, a pe log."""
    con = sqlite3.connect(db_dir / "re.sqlite")
    con.execute("INSERT INTO binary(key,game,product,version,variant,path,sha256,size,image_base) "
                "VALUES(?,'fo4','game','1.11.244','retail','x','s',1,0)", (NEW,))
    for stage, state in (("register", "done"), ("unpack", "done"), ("pe", "running")):
        con.execute("INSERT INTO stage_run SELECT id,?,1,'h',?,NULL,NULL,NULL FROM binary WHERE key=?",
                    (stage, state, NEW))
    con.commit()
    con.close()
    log = db_dir / "logs" / NEW / "pe.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_text("2026-10-03 12:00:01 INFO pe: 412,880 functions from .pdata\n"
                   "2026-10-03 12:00:09 INFO pe: mhash 120,000 / 412,880\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--added", action="store_true")
    parser.add_argument("--width", type=int, default=1600)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="redb_update_ui_"))
    with pytest.MonkeyPatch.context() as mp:
        db_dir, man = build(root, mp)
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
                ws = app._active_ws
                if frames == 3:
                    ws.updates = GameUpdates(ws.session, manifest_path=man, launcher=FakeLauncher(),
                                             workspace_root=root)
                    ws.updates.check()
                    ws.session.wait()
                    ws.start_update(ws.updates.result.changes[0])
                if frames == 5 and args.added:
                    ws.confirm_update()
                    fake_ingest_progress(db_dir)
                    ws.session.refresh_overview()
                    ws.session.wait()
                if frames >= 14:
                    params.app_shall_exit = True

            params.callbacks.post_render_dockable_windows = finish
            run(runner_params=params, **kwargs)

        with patch("ui.toolkit.app.immapp.run", render):
            app.run()
        Image.fromarray(hello_imgui.final_app_window_screenshot()).save(args.output)


if __name__ == "__main__":
    main()
