"""Precombined mesh generation for converted FO4 plugins.

Planning, group NIF construction, the shipped ``<stem> - Geometry.csg``
shared geometry, its ``<stem>.cdx`` index and the CELL metadata all run in the
Rust ``previs_native`` extension. The CSG and CDX are plugin sidecars written
beside the plugin, never into the archived ``data`` tree; deployment copies
them loose like the plugin itself. Only the interior STAT/SCOL subset verified
byte-for-byte against CK is combined; every other CELL is skipped and
reported, never approximated.
"""

from __future__ import annotations

import datetime as _dt
import json
import shutil
from dataclasses import dataclass, field
from pathlib import Path

from bacup_lib.stage_progress import stage_progress


@dataclass
class PrecombineRequest:
    plugin_path: Path
    mod_data_dir: Path
    master_dirs: list[Path]
    loose_roots: list[Path]
    archives: list[Path]
    work_dir: Path
    # None: the native default of half the logical CPUs.
    workers: int | None = None
    # Leave exterior CELLs out of the index. By default their rows go to
    # <stem> - Exterior.cdx, which Tales loads into an index of its own: FO4
    # loads every plugin's <stem>.cdx rows into one index of about 7.4M rows,
    # and Appalachia's exterior rows would overflow it and crash the game.
    interior_cdx_only: bool = False


@dataclass
class PrecombineResult:
    cells: int = 0
    groups: int = 0
    patched_cells: int = 0
    # Other stamped CELLs whose stale precombine/previs stamps were removed.
    cleared_cells: int = 0
    excluded_references: int = 0
    skipped_unsupported: int = 0
    skipped_invalid: list[str] = field(default_factory=list)
    report_path: Path | None = None
    timing_path: Path | None = None
    index: dict | None = None


def precombine_sidecar_names(plugin_name: str) -> tuple[str, str, str]:
    from creation_lib.build.archive_plan import precombine_sidecar_names as _names

    return _names(plugin_name)


def remove_existing_precombines(mod_data_dir: Path, plugin_path: Path) -> int:
    removed = 0
    plugin_name = plugin_path.name
    stem = plugin_path.stem.casefold()
    for meshes in mod_data_dir.glob("[Mm][Ee][Ss][Hh][Ee][Ss]"):
        for combined in meshes.glob("[Pp][Rr][Ee][Cc][Oo][Mm][Bb][Ii][Nn][Ee][Dd]"):
            for candidate in combined.iterdir():
                if candidate.is_dir() and candidate.name.casefold() == plugin_name.casefold():
                    shutil.rmtree(candidate)
                    removed += 1
    # Stamping clears every previs stamp, so earlier UVDs are orphaned.
    for vis in mod_data_dir.glob("[Vv][Ii][Ss]"):
        for candidate in vis.iterdir():
            if candidate.is_dir() and candidate.name.casefold() == plugin_name.casefold():
                shutil.rmtree(candidate)
                removed += 1
    stale ={f"{stem} - geometry.csg", f"{stem} - geometry.psg", f"{stem}.cdx", f"{stem} - exterior.cdx"}
    for directory in {mod_data_dir, plugin_path.parent}:
        for candidate in directory.iterdir() if directory.is_dir() else ():
            if candidate.is_file() and candidate.name.casefold() in stale:
                candidate.unlink()
                removed += 1
    return removed


def refresh_precombine_index(plugin_path: Path, mod_data_dir: Path) -> dict | None:
    """Rewrites ``<stem>.cdx`` from the stamped plugin; previs reruns it so
    CELLs with a UVD index it instead of the no-previs marker."""
    from bacup_lib._native import previs_native

    report = previs_native.write_precombine_index(str(plugin_path), str(mod_data_dir))
    return json.loads(report) if report else None


def generate_precombines(request: PrecombineRequest, *, log, progress=None) -> PrecombineResult:
    """Builds and stamps precombined meshes for every verified interior CELL.

    ``log(level, message)`` receives diagnostics; ``progress(done, total, item)``
    receives coarse progress.
    """
    from bacup_lib._native import previs_native

    removed = remove_existing_precombines(request.mod_data_dir, request.plugin_path)
    if removed:
        log("INFO", f"precombine: removed {removed} stale precombined output(s)")
    if progress:
        progress(0, 2, "Building precombined meshes")
    report_path = request.work_dir / "precombine_report.json"
    timing_path = request.work_dir / "precombine_timing.json"
    log("INFO", "precombine: loading the plugin and building precombined meshes")
    prepared = previs_native.prepare_precombines(
        json.dumps(
            {
                "plugin": str(request.plugin_path),
                "master_dirs": [str(p) for p in request.master_dirs],
                "loose_roots": [str(p) for p in request.loose_roots],
                "archives": [str(p) for p in request.archives],
                "output_dir": str(request.mod_data_dir),
                "interior_cdx_only": request.interior_cdx_only,
            }
        ),
        str(report_path),
        str(timing_path),
        workers=request.workers,
        progress=stage_progress(log, progress, stage="precombine", noun="CELLs", item="Building precombined meshes"),
    )
    if progress:
        progress(1, 2, "Stamping CELL precombine data")
    log("INFO", "precombine: stamping CELLs and writing the precombine index")
    today = _dt.date.today()
    summary = json.loads(prepared.finalize(today.year, today.month, today.day))
    result = PrecombineResult(**{
        **summary,
        "report_path": Path(summary["report_path"]),
        "timing_path": Path(summary["timing_path"]),
    })
    for reason in result.skipped_invalid:
        log("WARN", f"precombine: invalid input skipped: {reason}")
    if progress:
        progress(2, 2, "")
    log(
        "INFO",
        f"precombine: combined {result.cells} CELL(s) into {result.groups} group(s), "
        f"stamped {result.patched_cells} (cleared stale stamps on {result.cleared_cells}), "
        f"{result.skipped_unsupported} CELL(s) outside the "
        f"verified subset (details: {result.report_path})",
    )
    return result
