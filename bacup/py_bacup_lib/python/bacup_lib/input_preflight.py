"""Pre-conversion scan of FO76 extracted inputs and the FO4 target corpus.

Catches incomplete extraction (e.g. missing APPALACHIA terrain-LOD BTOs) BEFORE
a run starts, instead of failing deep in LOD generation. Pure and path-explicit:
no os.environ, no ToolkitSettings.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

from bacup_lib.target_assets import default_target_asset_catalog
from creation_lib.db.native_runtime import Database


@dataclass(frozen=True)
class MissingInput:
    label: str
    checked_path: str
    fix_hint: str


@dataclass
class InputPreflightReport:
    required_missing: list[MissingInput] = field(default_factory=list)
    optional_missing: list[MissingInput] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return not self.required_missing


def _find_child_ci(base: Path, name: str) -> Path | None:
    if not base.is_dir():
        return None
    lowered = name.casefold()
    try:
        for entry in base.iterdir():
            if entry.name.casefold() == lowered:
                return entry
    except OSError:
        return None
    return None


def _resolve_ci_path(base: Path, *parts: str) -> Path:
    """Case-insensitive descent; returns the literal join for the first part
    that does not exist so the reported path is human-readable."""
    current = base
    for idx, part in enumerate(parts):
        child = _find_child_ci(current, part)
        if child is None:
            return current.joinpath(*parts[idx:])
        current = child
    return current


def _bto_objects_dir(fo76_extracted: Path, world: str) -> Path:
    return _resolve_ci_path(fo76_extracted, "Meshes", "Terrain", world, "Objects")


def _has_bto(objects_dir: Path) -> bool:
    if not objects_dir.is_dir():
        return False
    try:
        return any(
            entry.is_file() and entry.name.casefold().endswith(".bto")
            for entry in objects_dir.iterdir()
        )
    except OSError:
        return False


def scan_conversion_inputs(
    paths: object,
    worldspaces: Iterable[str] = ("Appalachia",),
) -> InputPreflightReport:
    report = InputPreflightReport()
    fo76_data = Path(getattr(paths, "source_data_dir", "") or "")
    fo76_ext = Path(getattr(paths, "source_extracted_dir", "") or "")
    fo4_data = Path(getattr(paths, "target_data_dir", "") or "")
    catalog = Path(
        getattr(paths, "target_asset_catalog_path", None)
        or default_target_asset_catalog()
    )

    source_plugin = _resolve_ci_path(fo76_data, "SeventySix.esm")
    if not source_plugin.is_file():
        report.required_missing.append(
            MissingInput(
                "FO76 source plugin",
                str(source_plugin),
                "Point the Fallout 76 install path at a valid Fallout 76 Data folder "
                "containing SeventySix.esm.",
            )
        )

    if not fo76_ext.is_dir():
        report.required_missing.append(
            MissingInput(
                "FO76 extracted directory",
                str(fo76_ext),
                "Re-run Setup / Re-extract Game Data and select the extracted "
                "Fallout 76 directory before converting.",
            )
        )
    else:
        for world in worldspaces:
            objects_dir = _bto_objects_dir(fo76_ext, world)
            if not _has_bto(objects_dir):
                report.required_missing.append(
                    MissingInput(
                        f"FO76 terrain-LOD BTOs ({world})",
                        str(objects_dir),
                        "Re-extract the Fallout 76 terrain-LOD archives into your FO76 "
                        f"extracted dir (expected *.bto under Meshes/Terrain/{world}/Objects).",
                    )
                )

    if not fo4_data.is_dir():
        report.required_missing.append(
            MissingInput(
                "FO4 Data directory",
                str(fo4_data),
                "Point the Fallout 4 install path at a valid Data directory containing "
                "Fallout4.esm and the official BA2 archives.",
            )
        )
    else:
        target_master = _resolve_ci_path(fo4_data, "Fallout4.esm")
        if not target_master.is_file():
            report.required_missing.append(
                MissingInput(
                    "FO4 base master",
                    str(target_master),
                    "Verify the Fallout 4 Data directory contains Fallout4.esm.",
                )
            )
    if catalog.is_file() and fo4_data.is_dir():
        try:
            with Database.open(str(catalog), mode="ro") as db:
                rows = db.query_all(
                    "SELECT name, content_pack, required FROM archives ORDER BY priority"
                )
        except (RuntimeError, OSError):
            pass
        else:
            installed = {
                entry.name.casefold(): entry
                for entry in fo4_data.iterdir()
                if entry.is_file() and entry.suffix.casefold() == ".ba2"
            }
            for row in rows:
                archive_name = row["name"]
                content_pack = row["content_pack"]
                required = row["required"]
                if str(archive_name).casefold() in installed:
                    continue
                missing = MissingInput(
                    f"FO4 archive ({content_pack})",
                    str(fo4_data / str(archive_name)),
                    "Verify the corresponding Fallout 4 base game or DLC files.",
                )
                if required:
                    report.required_missing.append(missing)
                else:
                    report.optional_missing.append(missing)

    return report
