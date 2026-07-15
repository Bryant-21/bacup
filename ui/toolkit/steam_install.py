"""Local Steam-install checks for release workflows."""
from __future__ import annotations

from dataclasses import dataclass
import os
from pathlib import Path
import re

from creation_lib.core.game_profiles import GAME_PROFILES

from ui.toolkit.path_detector import validate_game_path


_STEAM_API_DLL_32 = "steam_api.dll"
_STEAM_API_DLL_64 = "steam_api64.dll"
_STEAM_32_BIT_GAMES = frozenset({"fnv", "fo3"})


@dataclass(frozen=True)
class SteamInstallResult:
    ok: bool
    game_id: str
    app_id: int | None
    root_dir: str
    local_install_valid: bool
    steam_layout_valid: bool
    steam_api_present: bool
    appmanifest_present: bool
    appmanifest_matches: bool
    steam_library_dir: str
    appmanifest_path: str
    message: str


def validate_steam_install_for_game(
    game_id: str,
    root_or_data_dir: str,
) -> SteamInstallResult:
    profile = GAME_PROFILES.get(game_id)
    root_dir = _install_root_from_path(root_or_data_dir)
    root_text = str(root_dir) if root_dir is not None else str(root_or_data_dir or "")
    app_id = profile.steam_app_id if profile is not None else None

    if profile is None:
        return _result(False, game_id, None, root_text, "Unknown game profile.")
    if not app_id:
        return _result(False, game_id, None, root_text, "Game has no Steam AppID.")

    local_valid = root_dir is not None and validate_game_path(game_id, str(root_dir))
    if not local_valid:
        return _result(
            False,
            game_id,
            app_id,
            root_text,
            f"{profile.display_name} install is invalid: executable or Data archives not found.",
            local_install_valid=False,
        )

    assert root_dir is not None
    steam_api_dll = (
        _STEAM_API_DLL_32 if game_id in _STEAM_32_BIT_GAMES else _STEAM_API_DLL_64
    )
    steam_api_present = (root_dir / steam_api_dll).is_file()
    library_dir = _steam_library_dir_for_install(root_dir)
    steam_layout_valid = library_dir is not None
    manifest_path = (
        library_dir / "steamapps" / f"appmanifest_{app_id}.acf"
        if library_dir is not None
        else None
    )
    appmanifest_present = manifest_path is not None and manifest_path.is_file()
    appmanifest_matches = (
        _appmanifest_matches_install(manifest_path, app_id, root_dir)
        if manifest_path is not None and appmanifest_present
        else False
    )

    if not steam_layout_valid:
        message = (
            f"{profile.display_name} must be installed under a Steam library "
            r"steamapps\common folder."
        )
    elif not steam_api_present:
        message = f"{profile.display_name} install is missing {steam_api_dll}."
    elif not appmanifest_present:
        message = f"Steam app manifest appmanifest_{app_id}.acf was not found."
    elif not appmanifest_matches:
        message = f"Steam app manifest does not match the selected {profile.display_name} folder."
    else:
        message = f"{profile.display_name} Steam install verified."

    return SteamInstallResult(
        ok=bool(
            local_valid
            and steam_layout_valid
            and steam_api_present
            and appmanifest_present
            and appmanifest_matches
        ),
        game_id=game_id,
        app_id=app_id,
        root_dir=str(root_dir),
        local_install_valid=bool(local_valid),
        steam_layout_valid=steam_layout_valid,
        steam_api_present=steam_api_present,
        appmanifest_present=appmanifest_present,
        appmanifest_matches=appmanifest_matches,
        steam_library_dir=str(library_dir or ""),
        appmanifest_path=str(manifest_path or ""),
        message=message,
    )


def _result(
    ok: bool,
    game_id: str,
    app_id: int | None,
    root_dir: str,
    message: str,
    *,
    local_install_valid: bool = False,
) -> SteamInstallResult:
    return SteamInstallResult(
        ok=ok,
        game_id=game_id,
        app_id=app_id,
        root_dir=root_dir,
        local_install_valid=local_install_valid,
        steam_layout_valid=False,
        steam_api_present=False,
        appmanifest_present=False,
        appmanifest_matches=False,
        steam_library_dir="",
        appmanifest_path="",
        message=message,
    )


def _install_root_from_path(value: str) -> Path | None:
    text = str(value or "").strip()
    if not text:
        return None
    path = Path(text).expanduser()
    if path.name.lower() == "data":
        path = path.parent
    return path


def _steam_library_dir_for_install(root_dir: Path) -> Path | None:
    parts = [part.lower() for part in root_dir.parts]
    for index in range(len(parts) - 2):
        if parts[index] == "steamapps" and parts[index + 1] == "common":
            return Path(*root_dir.parts[:index])
    return None


def _appmanifest_matches_install(
    manifest_path: Path,
    app_id: int,
    root_dir: Path,
) -> bool:
    try:
        text = manifest_path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False

    manifest_app_id = _acf_value(text, "appid")
    install_dir = _acf_value(text, "installdir")
    if manifest_app_id != str(app_id) or not install_dir:
        return False

    manifest_root = manifest_path.parent / "common" / install_dir
    return _same_path(root_dir, manifest_root)


def _acf_value(text: str, key: str) -> str:
    match = re.search(rf'"{re.escape(key)}"\s+"([^"]*)"', text, flags=re.IGNORECASE)
    return match.group(1) if match else ""


def _same_path(left: Path, right: Path) -> bool:
    return os.path.normcase(os.path.abspath(left)) == os.path.normcase(os.path.abspath(right))
