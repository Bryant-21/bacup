"""Entry point for ModBox21 — Bethesda Modding Toolkit.

Usage:
  Dev mode:  uv run python -m ui.toolkit
  Frozen:    ModBox21.exe
"""

import multiprocessing
import sys
from pathlib import Path

# Ensure project root is on sys.path (dev mode only — frozen uses _MEIPASS)
if not getattr(sys, "frozen", False):
    project_root = str(Path(__file__).resolve().parents[2])
    if project_root not in sys.path:
        sys.path.insert(0, project_root)

from creation_lib.havok.native_runtime import configure_native_resources

configure_native_resources()

from ui.core.logging_utils import setup_logging

log = setup_logging("toolkit")

from ui.toolkit.settings import ToolkitSettings  # noqa: E402
from ui.toolkit.variants import get_variant, variant_id_from_exe_name  # noqa: E402
from ui.toolkit.workspaces import create_workspaces  # noqa: E402
from ui.toolkit.app import ToolkitApp  # noqa: E402


def _prepare_first_run_settings(settings: ToolkitSettings, variant) -> bool:
    if settings.setup_complete or variant.id != "nif":
        return False

    from creation_lib.core.game_profiles import GAME_PROFILES
    from ui.toolkit.path_detector import detect_game_path, validate_game_path

    for profile in GAME_PROFILES.values():
        game_id = profile.id
        if not profile.is_moddable or game_id not in settings._paths:
            continue
        if settings.get_game_paths(game_id).get("root_dir"):
            continue
        detected = detect_game_path(game_id)
        if detected and validate_game_path(game_id, detected):
            settings._paths[game_id]["root_dir"] = detected

    settings.setup_complete = True
    settings.save()
    return True


def _set_taskbar_identity(variant) -> None:
    """Give the process a stable AppUserModelID so Windows shows the window's
    own icon in the taskbar (otherwise dev launches group under python.exe)."""
    try:
        import ctypes

        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(
            f"ModBox21.{variant.id}"
        )
    except Exception:
        pass


def run_toolkit_variant(variant_id: str = "full", launch_path: str | None = None):
    multiprocessing.freeze_support()
    variant = get_variant(variant_id)
    _set_taskbar_identity(variant)

    settings = ToolkitSettings(variant_id=variant.id)
    if variant.is_standalone:
        settings.active_workspace = variant.default_workspace

    if not settings.setup_complete and not _prepare_first_run_settings(
        settings, variant
    ):
        from ui.toolkit.setup_wizard import SetupWizard

        wizard = SetupWizard(settings)
        if not wizard.run():
            return  # User cancelled or closed wizard

        # Reload settings after wizard saved them
        settings = ToolkitSettings(variant_id=variant.id)
        if variant.is_standalone:
            settings.active_workspace = variant.default_workspace

    workspaces = create_workspaces(
        toolkit_settings=settings,
        workspace_ids=variant.workspace_ids,
    )
    app = ToolkitApp(workspaces, settings, launch_path=launch_path, app_variant=variant)
    app.run()


def _parse_main_args(argv: list[str], executable_name: str) -> tuple[str, str | None]:
    variant_id = variant_id_from_exe_name(Path(executable_name).stem) or "full"
    args = list(argv)
    if args and args[0].startswith("--variant="):
        variant_id = args.pop(0).split("=", 1)[1]
    launch_path = args[0] if args else None
    return variant_id, launch_path


def main():
    executable_name = sys.executable if getattr(sys, "frozen", False) else sys.argv[0]
    variant_id, launch_path = _parse_main_args(sys.argv[1:], executable_name)
    run_toolkit_variant(variant_id, launch_path=launch_path)


if __name__ == "__main__":
    main()
