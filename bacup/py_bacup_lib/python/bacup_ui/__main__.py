"""Standalone launcher for the B.A.C.U.P. desktop application."""

from __future__ import annotations

import multiprocessing
import sys
from pathlib import Path

if not getattr(sys, "frozen", False):
    repo_root = str(Path(__file__).resolve().parents[4])
    if repo_root not in sys.path:
        sys.path.insert(0, repo_root)

from ui.core.logging_utils import setup_logging

setup_logging("bacup")

from bacup_ui.appalachia.appalachia_workspace import AppalachiaWorkspace  # noqa: E402
from bacup_ui.variant import BACUP_VARIANT  # noqa: E402
from ui.toolkit.app import ToolkitApp  # noqa: E402
from ui.toolkit.settings import ToolkitSettings  # noqa: E402


def _set_taskbar_identity() -> None:
    try:
        import ctypes

        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID("ModBox21.BACUP")
    except Exception:
        pass


def _run_bacup_project_setup(settings: ToolkitSettings) -> tuple[bool, bool]:
    from bacup_ui.setup import (
        AppalachiaSetup,
        BacupProjectSetup,
        appalachia_setup_needed,
        clear_pending_project_setup,
        get_pending_project_setup,
    )

    pending_project_id = get_pending_project_setup(settings)
    if pending_project_id is not None:
        setup = BacupProjectSetup(settings, pending_project_id)
    elif not getattr(settings, "setup_complete", False) and appalachia_setup_needed(
        settings
    ):
        setup = AppalachiaSetup(settings)
    else:
        return False, True

    completed = setup.run()
    if pending_project_id is not None:
        clear_pending_project_setup(settings)
    return True, completed


def run_bacup(launch_path: str | None = None) -> None:
    multiprocessing.freeze_support()
    _set_taskbar_identity()

    settings = ToolkitSettings(variant_id=BACUP_VARIANT.id)
    settings.active_workspace = BACUP_VARIANT.default_workspace
    ran_setup, completed = _run_bacup_project_setup(settings)
    if ran_setup:
        if not completed:
            return
        settings = ToolkitSettings(variant_id=BACUP_VARIANT.id)
        settings.active_workspace = BACUP_VARIANT.default_workspace

    workspace = AppalachiaWorkspace(toolkit_settings=settings)
    app = ToolkitApp(
        [workspace],
        settings,
        launch_path=launch_path,
        app_variant=BACUP_VARIANT,
    )
    app.run()


def main(argv: list[str] | None = None) -> None:
    from bacup_ui.crash_diagnostics import start_crash_diagnostics

    stop_diagnostics = start_crash_diagnostics()
    try:
        args = list(sys.argv[1:] if argv is None else argv)
        run_bacup(args[0] if args else None)
    finally:
        stop_diagnostics.set()


if __name__ == "__main__":
    main()
