"""Previs (Umbra visibility) generation for converted FO4 plugins.

Planning, scene construction, the visibility solve, tome encoding and CELL
metadata all run in the Rust ``previs_native`` extension; nothing from the
Creation Kit is loaded. Each cluster is solved as soon as it is planned, on
a pool of ``workers`` threads, and only its tome is written.

Only the record/asset subset the planner verifies is generated. Clusters
outside it are skipped and reported, never approximated.
"""

from __future__ import annotations

import datetime as _dt
import json
import shutil
from dataclasses import dataclass, field
from pathlib import Path

from bacup_lib.stage_progress import stage_progress

@dataclass
class PrevisRequest:
    plugin_path: Path
    mod_data_dir: Path
    master_dirs: list[Path]
    loose_roots: list[Path]
    archives: list[Path]
    work_dir: Path
    worlds: list[str] = field(default_factory=list)
    interiors: bool = True
    # None: the native default of half the logical CPUs.
    workers: int | None = None
    # Leave exterior CELLs out when previs rewrites the index (see
    # PrecombineRequest.interior_cdx_only).
    interior_cdx_only: bool = False


@dataclass
class PrevisResult:
    generated: int = 0
    stamped: int = 0
    failed: list[tuple[str, str]] = field(default_factory=list)
    skipped_unsupported: int = 0
    skipped_invalid: int = 0
    patched_cells: int = 0
    report_path: Path | None = None


def _native():
    from bacup_lib._native import previs_native

    return previs_native


def remove_existing_vis(mod_data_dir: Path, plugin_name: str) -> int:
    removed = 0
    for vis_dir in mod_data_dir.glob("[Vv][Ii][Ss]"):
        for candidate in vis_dir.iterdir():
            if candidate.is_dir() and candidate.name.casefold() == plugin_name.casefold():
                shutil.rmtree(candidate)
                removed += 1
    return removed


def generate_previs(request: PrevisRequest, *, log, progress=None) -> PrevisResult:
    """Plans, solves and stamps previs for every verified cluster.

    ``log(level, message)`` receives diagnostics; ``progress(done, total, item)``
    receives per-cluster progress.
    """
    native = _native()
    removed = remove_existing_vis(request.mod_data_dir, request.plugin_path.name)
    if removed:
        log("INFO", f"previs: removed {removed} stale Vis tree(s)")
    scene_dir = request.work_dir / "scenes"
    plan_request = {
        "plugin": str(request.plugin_path),
        "master_dirs": [str(p) for p in request.master_dirs],
        "loose_roots": [str(p) for p in request.loose_roots],
        "archives": [str(p) for p in request.archives],
        "scene_dir": str(scene_dir),
        "worlds": request.worlds,
        "interiors": request.interiors,
        "accept_reference_layer": False,
        "solve_into": str(request.mod_data_dir),
        "interior_cdx_only": request.interior_cdx_only,
    }
    if progress:
        progress(0, 1, "Planning and solving visibility clusters")
    log("INFO", "previs: loading the plugin and solving visibility clusters")
    prepared = native.prepare_previs(
        json.dumps(plan_request),
        str(request.work_dir / "previs_plan.json"),
        str(request.work_dir / "previs_timing.json"),
        workers=request.workers,
        progress=stage_progress(log, progress, stage="previs", noun="clusters", item="Solving visibility clusters"),
    )
    if progress:
        progress(1, 1, "Stamping previs cells")
    log("INFO", "previs: stamping CELLs and rewriting the precombine index")
    today = _dt.date.today()
    summary = json.loads(prepared.finalize(today.year, today.month, today.day))
    result = PrevisResult(
        generated=summary["generated"],
        stamped=summary["stamped"],
        failed=[tuple(failure) for failure in summary["failed"]],
        skipped_unsupported=summary["skipped_unsupported"],
        skipped_invalid=summary["skipped_invalid"],
        patched_cells=summary["patched_cells"],
        report_path=Path(summary["report_path"]),
    )
    for warning in summary["warnings"]:
        log("WARN", warning)
    log(
        "INFO",
        f"previs: {result.generated} cluster(s) solved, {result.stamped} stamp-only, "
        f"{result.skipped_unsupported} outside the verified subset "
        f"(details: {result.report_path})",
    )
    log("INFO", f"previs: timings: {summary['timing_path']}")
    index = summary["index"]
    if index:
        log(
            "INFO",
            f"previs: precombine index {index['cdx_relative_path']} lists "
            f"{index['cells_with_visibility']}/{index['cells']} CELL(s) with a UVD",
        )
        if index.get("exterior_cdx_relative_path"):
            log("INFO", f"previs: exterior rows in {index['exterior_cdx_relative_path']} (read by Tales)")
    shutil.rmtree(scene_dir, ignore_errors=True)
    log(
        "INFO",
        f"previs: generated {result.generated} UVD(s), stamped {result.patched_cells} CELL(s), "
        f"{len(result.failed)} solve failure(s)",
    )
    return result
