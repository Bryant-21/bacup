"""Auto-detect game installation paths.

Detection strategy per game (in order):
1. Windows Registry (Steam App {steam_app_id})
2. Steam libraryfolders.vdf parsing
3. Common path scan across drives

Uses GameProfile fields (steam_app_id, executable_name) so new games
are supported automatically once a profile is registered.
"""

import logging
import os
import re
import string
from pathlib import Path

from creation_lib.core.game_profiles import GAME_PROFILES, GameProfile

_log = logging.getLogger("toolkit.path_detector")

# Steam folder name overrides for games whose Steam folder differs from the
# display_name.  Key = game profile id, value = candidate folder names tried in
# order (the actual steamapps/common subfolder varies by depot — e.g. Fallout 76
# installs to "Fallout76" with no space).
_STEAM_FOLDER_NAMES: dict[str, list[str]] = {
    "fo3": ["Fallout 3 GOTY"],
    "fo4": ["Fallout 4"],
    "skyrimse": ["Skyrim Special Edition"],
    "fo76": ["Fallout76", "Fallout 76"],
    "starfield": ["Starfield"],
}


def _steam_folder_candidates(game_id: str, profile: GameProfile) -> list[str]:
    return _STEAM_FOLDER_NAMES.get(game_id, [profile.display_name])


# ------------------------------------------------------------------ #
#  Public API                                                         #
# ------------------------------------------------------------------ #


def detect_game_path(game_id: str) -> str | None:
    """Auto-detect the installation directory for *game_id*, or None if not found."""
    profile = GAME_PROFILES.get(game_id)
    if profile is None:
        _log.warning("Unknown game_id %r in detect_game_path", game_id)
        return None

    steam_folders = _steam_folder_candidates(game_id, profile)

    # Strategy 1: Windows Registry
    if profile.steam_app_id:
        path = _detect_from_registry(profile.steam_app_id, profile)
        if path:
            _log.info("%s detected via registry: %s", profile.display_name, path)
            return path

    # Strategy 2: Steam libraryfolders.vdf
    path = _detect_from_steam_vdf(steam_folders, profile)
    if path:
        _log.info("%s detected via Steam VDF: %s", profile.display_name, path)
        return path

    # Strategy 3: Common path scan
    path = _detect_from_common_paths(steam_folders, profile)
    if path:
        _log.info("%s detected via path scan: %s", profile.display_name, path)
        return path

    _log.info("%s installation not auto-detected", profile.display_name)
    return None


def validate_game_path(game_id: str, path: str) -> bool:
    """Check if *path* is a valid installation for *game_id*.

    Checks for the game executable and a Data/ directory with at least
    one archive file (.ba2 or .bsa depending on game).
    """
    profile = GAME_PROFILES.get(game_id)
    if profile is None:
        return False
    return _validate(path, profile)


# Keep legacy helpers so existing callers don't break.
def detect_fo4_path() -> str | None:
    return detect_game_path("fo4")


def validate_fo4_path(path: str) -> bool:
    return validate_game_path("fo4", path)


# ------------------------------------------------------------------ #
#  Internal helpers                                                   #
# ------------------------------------------------------------------ #


def _validate(path: str, profile: GameProfile) -> bool:
    """Core validation: exe exists + Data/ with archives."""
    if not path:
        return False
    p = Path(path)
    exe = profile.executable_name
    if exe and not (p / exe).is_file():
        return False
    data_dir = p / "Data"
    if not data_dir.is_dir():
        return False
    # Check for at least one archive file
    for ext in profile.archive_extensions:
        try:
            if any(data_dir.glob(f"*{ext}")):
                return True
        except (PermissionError, OSError):
            pass
    return False


def _detect_from_registry(steam_app_id: int, profile: GameProfile) -> str | None:
    """Try Windows Registry for a Steam App."""
    try:
        import winreg
        key_path = rf"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Steam App {steam_app_id}"
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, key_path) as key:
            install_loc, _ = winreg.QueryValueEx(key, "InstallLocation")
            if install_loc and _validate(install_loc, profile):
                return str(install_loc)
    except (ImportError, OSError, FileNotFoundError):
        pass
    return None


def _detect_from_steam_vdf(steam_folders: list[str], profile: GameProfile) -> str | None:
    """Parse Steam's libraryfolders.vdf to find all library paths."""
    steam_default = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"))
    vdf_path = steam_default / "Steam" / "config" / "libraryfolders.vdf"
    if not vdf_path.is_file():
        vdf_path = steam_default / "Steam" / "steamapps" / "libraryfolders.vdf"
    if not vdf_path.is_file():
        return None

    try:
        text = vdf_path.read_text(encoding="utf-8", errors="replace")
        paths = re.findall(r'"path"\s+"([^"]+)"', text)
        for lib_path in paths:
            lib_path = lib_path.replace("\\\\", "\\")
            for steam_folder in steam_folders:
                game_dir = Path(lib_path) / "steamapps" / "common" / steam_folder
                if game_dir.is_dir() and _validate(str(game_dir), profile):
                    return str(game_dir)
    except (OSError, UnicodeDecodeError) as e:
        _log.debug("VDF parse failed: %s", e)

    return None


def _detect_from_common_paths(steam_folders: list[str], profile: GameProfile) -> str | None:
    """Scan common Steam install locations across all drives."""
    suffixes = [
        rf"{base}\steamapps\common\{steam_folder}"
        for steam_folder in steam_folders
        for base in ("Steam", "SteamLibrary", r"Games\Steam", "Steam Games")
    ]
    prefixes = [
        "",
        "Program Files (x86)",
        "Program Files",
    ]

    for letter in _get_drive_letters():
        for prefix in prefixes:
            for suffix in suffixes:
                if prefix:
                    candidate = Path(f"{letter}:\\{prefix}\\{suffix}")
                else:
                    candidate = Path(f"{letter}:\\{suffix}")
                if candidate.is_dir() and _validate(str(candidate), profile):
                    return str(candidate)
    return None


def _get_drive_letters() -> list[str]:
    """Return available drive letters on Windows."""
    try:
        import ctypes
        bitmask = ctypes.windll.kernel32.GetLogicalDrives()
        return [
            letter for i, letter in enumerate(string.ascii_uppercase)
            if bitmask & (1 << i)
        ]
    except (AttributeError, OSError):
        return [
            letter for letter in "CDEFGHIJKLMNOPQRSTUVWXYZ"
            if Path(f"{letter}:\\").exists()
        ]
