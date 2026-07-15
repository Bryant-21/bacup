"""Install/deploy debug audit: is a mod's ESM + BA2 shards deployed and
registered in the runtime INI's [Archive] lists?

Reuses the archive-INI logic already implemented in ``regen_pipeline`` rather
than duplicating it.
"""
from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from creation_lib.build.deployer import discover_mod_archives
from bacup_lib.regen_pipeline import (
    _FO4_ARCHIVE_ANIMATION_KEY,
    _FO4_ARCHIVE_MAIN_KEY,
    _FO4_ARCHIVE_TEXTURE_KEY,
    _archive_ini_values_by_key,
    _fo4_ini_archive_names_for_plugins,
    _reconcile_runtime_archive_ini_entries,
    _unique_archive_names,
)

_ARCHIVE_KEYS = (
    _FO4_ARCHIVE_MAIN_KEY,
    _FO4_ARCHIVE_ANIMATION_KEY,
    _FO4_ARCHIVE_TEXTURE_KEY,
)


@dataclass
class ArchiveAuditRow:
    name: str
    kind: str  # "esm" | "ba2"
    deployed: bool
    registered: bool | None  # None for the esm row, or when ini_path is None/missing


@dataclass
class ArchiveAuditReport:
    deploy_dir: Path
    ini_path: Path | None
    rows: list[ArchiveAuditRow]
    missing_registration: list[str]  # ba2 names deployed but NOT registered in the INI
    stale_registration: list[str]  # registered for this plugin but no longer deployed
    note: str | None = None


def audit_archive_ini(
    *,
    deploy_dir: Path,
    ini_path: Path | None,
    mod_name: str,
    plugin_name: str,
) -> ArchiveAuditReport:
    """Report whether the mod's ESM + BA2 shards are present in deploy_dir and
    (for BA2s) registered in the [Archive] lists of ini_path."""
    rows = [
        ArchiveAuditRow(
            name=plugin_name,
            kind="esm",
            deployed=(deploy_dir / plugin_name).is_file(),
            registered=None,
        )
    ]

    registered_names: set[str] | None = None
    stale_registration: list[str] = []
    note: str | None = None
    if ini_path is None:
        note = "No INI target for this install location."
    elif not ini_path.is_file():
        note = f"INI not found: {ini_path}"
    else:
        values_by_key = _archive_ini_values_by_key(ini_path, _ARCHIVE_KEYS)
        registered_names = {
            name.lower() for values in values_by_key.values() for name in values
        }

    archive_names = [
        archive_path.name for archive_path in discover_mod_archives(deploy_dir, mod_name)
    ]
    if registered_names is not None:
        deployed_names = {name.lower() for name in archive_names}
        stale_registration = [
            name
            for name in _fo4_ini_archive_names_for_plugins(
                [plugin_name],
                ini_path=ini_path,
            )
            if name.lower() not in deployed_names
        ]
    missing_registration: list[str] = []
    for name in archive_names:
        registered = None if registered_names is None else name.lower() in registered_names
        rows.append(ArchiveAuditRow(name=name, kind="ba2", deployed=True, registered=registered))
        if registered is False:
            missing_registration.append(name)

    return ArchiveAuditReport(
        deploy_dir=deploy_dir,
        ini_path=ini_path,
        rows=rows,
        missing_registration=missing_registration,
        stale_registration=stale_registration,
        note=note,
    )


def repair_archive_ini(
    *,
    ini_path: Path,
    base_ini_path: Path,
    archive_names: list[str],
    plugin_name: str,
) -> list[str]:
    """Replace this plugin's archive entries with its currently deployed names."""
    before_values = _archive_ini_values_by_key(ini_path, _ARCHIVE_KEYS)
    before = {name.lower() for values in before_values.values() for name in values}

    current_names = _unique_archive_names(archive_names)
    _reconcile_runtime_archive_ini_entries(
        current_names,
        plugin_names=[plugin_name],
        ini_path=ini_path,
        base_ini_path=base_ini_path,
    )

    return sorted(name for name in current_names if name.lower() not in before)
