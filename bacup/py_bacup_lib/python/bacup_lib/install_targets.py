"""Resolve a user-chosen install location to a deploy dir + FO4 archive-INI target.

Env-free: no os.environ reads. All inputs (paths, mode strings) are explicit params.
"""
from __future__ import annotations

import configparser
from dataclasses import dataclass
from pathlib import Path

_DEFAULT_PROFILE = "Default"


@dataclass(frozen=True)
class InstallTarget:
    deploy: bool
    deploy_data_dir: Path | None
    runtime_ini_path: Path | None
    warning: str | None = None


def resolve_mo2_profile_ini(mod_folder: Path) -> Path | None:
    """mod_folder is an MO2 mod folder laid out as <instance>/mods/<Name>.

    Return <instance>/profiles/<profile>/fallout4custom.ini, where <profile> is
    read from <instance>/ModOrganizer.ini ([General] selected_profile), falling
    back to 'Default'. Return None if mod_folder is not a .../mods/<Name> layout
    (i.e. its parent dir is not named 'mods').
    """
    if mod_folder.parent.name.lower() != "mods":
        return None

    instance = mod_folder.parent.parent
    profile = _read_selected_profile(instance / "ModOrganizer.ini")
    return instance / "profiles" / profile / "fallout4custom.ini"


def _read_selected_profile(ini_path: Path) -> str:
    try:
        parser = configparser.ConfigParser(strict=False)
        parser.read(ini_path, encoding="utf-8")
        raw = parser.get("General", "selected_profile")
    except Exception:
        return _DEFAULT_PROFILE

    value = raw.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
        value = value[1:-1].strip()

    if not value or value.startswith("@ByteArray("):
        return _DEFAULT_PROFILE
    return value


def resolve_deploy_and_ini(
    *,
    install_location: str,
    install_path: str,
    fo4_data_dir: Path,
    docs_custom_ini: Path,
    mo2_use_profile_ini: bool = True,
) -> InstallTarget:
    mode = install_location.strip().lower()

    if mode == "vortex":
        if not install_path.strip():
            return InstallTarget(False, None, None, warning="Vortex install folder not set")
        return InstallTarget(True, Path(install_path), docs_custom_ini)

    if mode == "mo2":
        if not install_path.strip():
            return InstallTarget(False, None, None, warning="MO2 mod folder not set")
        if not mo2_use_profile_ini:
            # Profile-specific game INI files are off; the game reads the global INI.
            return InstallTarget(True, Path(install_path), docs_custom_ini)
        ini = resolve_mo2_profile_ini(Path(install_path))
        warning = None if ini else "Could not derive MO2 profile INI: expected a .../mods/<Name> folder"
        return InstallTarget(True, Path(install_path), ini, warning=warning)

    if mode == "none":
        return InstallTarget(deploy=False, deploy_data_dir=None, runtime_ini_path=None)

    # "game" and anything unrecognized.
    return InstallTarget(deploy=True, deploy_data_dir=None, runtime_ini_path=docs_custom_ini)
