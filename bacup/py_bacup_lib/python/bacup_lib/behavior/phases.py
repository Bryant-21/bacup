"""Scaffold phase helpers for the conversion pipeline."""
from __future__ import annotations

import os
import shutil
from pathlib import Path

from bacup_lib.paths import apply_asset_prefix
from bacup_lib.runner import ConversionRunner
from bacup_lib.models import (
    AssetRef,
    PhaseProgress,
    auto_conversion_worker_count,
)


def _conversion_worker_count(orchestrator) -> int:
    workers = getattr(orchestrator, "conversion_workers", None)
    if workers is None:
        return auto_conversion_worker_count()
    return int(workers)


def phase_scaffold(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    """Phase 7: Scaffold mod output -- copy audio and write logs."""
    audio_assets = [a for a in orchestrator.graph.all_assets if a.asset_type == "sound"]
    script_assets = [a for a in orchestrator.graph.all_assets if a.asset_type == "script"]
    total = len(audio_assets) + 1
    progress.total_items = total
    item_idx = 0

    data_dir = os.path.join(orchestrator.mod_path, "data")
    for subdir in ("Meshes", "Textures", "Materials", "Sound", "Scripts"):
        os.makedirs(os.path.join(data_dir, subdir), exist_ok=True)

    orchestrator._summary.audio_total = len(audio_assets)
    audio_copy_candidates: list[AssetRef] = []
    for asset in audio_assets:
        if runner.is_cancelled():
            break

        if orchestrator._target_has_asset(asset):
            orchestrator._summary.audio_base_game_skipped += 1
            orchestrator._track_asset(asset, "base_game_skip", "exists in target game")
            runner.emit_log("INFO", f"[Audio] Base game skip: {asset.source_path}")
            item_idx += 1
            progress.completed_items = item_idx
            progress.current_item = asset.source_path
            runner.emit_item_progress(progress)
            continue

        if (
            not orchestrator.overwrite_existing
            and _prefixed_sound_output_exists(orchestrator, asset)
        ):
            orchestrator._track_asset(asset, "already_exists", "already in mod output")
            item_idx += 1
            progress.completed_items = item_idx
            progress.current_item = asset.source_path
            runner.emit_item_progress(progress)
            continue

        if _resolved_sound_asset_exists(asset):
            audio_copy_candidates.append(asset)
        else:
            runner.emit_log("ERROR", f"Audio not found: {asset.source_path}")
            orchestrator._summary.audio_failed += 1
            item_idx += 1
            progress.completed_items = item_idx
            progress.current_item = asset.source_path
            runner.emit_item_progress(progress)

    workers = _conversion_worker_count(orchestrator)

    if workers <= 1 or len(audio_copy_candidates) <= 1:
        for asset in audio_copy_candidates:
            _copy_sound_asset_with_prefix(orchestrator, asset)
            orchestrator._summary.audio_copied += 1
            orchestrator._track_asset(asset, "copied")
            item_idx += 1
            progress.completed_items = item_idx
            progress.current_item = asset.source_path
            runner.emit_item_progress(progress)
    elif audio_copy_candidates:
        from concurrent.futures import ThreadPoolExecutor

        max_workers = min(workers, len(audio_copy_candidates))
        with ThreadPoolExecutor(
            max_workers=max_workers,
            thread_name_prefix="audio-copy",
        ) as executor:
            futures = [
                (
                    asset,
                    executor.submit(_copy_sound_asset_with_prefix, orchestrator, asset),
                )
                for asset in audio_copy_candidates
            ]
            for asset, future in futures:
                try:
                    future.result()
                except Exception as exc:
                    runner.emit_log("ERROR", f"Audio failed: {asset.source_path}: {exc}")
                    orchestrator._summary.audio_failed += 1
                else:
                    orchestrator._summary.audio_copied += 1
                    orchestrator._track_asset(asset, "copied")
                item_idx += 1
                progress.completed_items = item_idx
                progress.current_item = asset.source_path
                runner.emit_item_progress(progress)

    support_assets = [a for a in orchestrator.graph.all_assets if a.asset_type == "support"]
    for asset in support_assets:
        if runner.is_cancelled():
            break
        if orchestrator._target_has_asset(asset):
            continue
        if not orchestrator.overwrite_existing and orchestrator._asset_already_in_mod(asset):
            continue
        if asset.resolved_path and os.path.isfile(asset.resolved_path):
            orchestrator._copy_asset_as_is(asset)
            runner.emit_log("INFO", f"[Support] Copied: {asset.source_path}")
        else:
            runner.emit_log("WARN", f"Support file not found: {asset.source_path}")

    for asset in script_assets:
        orchestrator._summary.scripts_flagged += 1
        runner.emit_log(
            "WARN", f"Script requires manual porting: {asset.source_path}"
        )
        orchestrator._log_lines.append(
            f"[WARN] Script requires manual porting: {asset.source_path}"
        )

    item_idx += 1
    progress.completed_items = item_idx
    progress.current_item = "Write conversion log"
    runner.emit_item_progress(progress)
    orchestrator._write_log_file(runner)

    orchestrator._write_asset_map()
    orchestrator._write_conversion_report(runner)


def _prefixed_sound_output_path(orchestrator, asset: AssetRef) -> Path:
    from bacup_lib.pipeline.sounds import _apply_resolved_extension

    subpath = _prefixed_asset_output_subpath(orchestrator, asset)
    subpath = _apply_resolved_extension(subpath, asset.resolved_path)
    return Path(orchestrator.mod_path) / "data" / Path(subpath)


def _prefixed_sound_output_exists(orchestrator, asset: AssetRef) -> bool:
    out_path = _prefixed_sound_output_path(orchestrator, asset)
    if asset.resolved_path and Path(asset.resolved_path).is_dir():
        return out_path.is_dir()
    return out_path.is_file()


def _resolved_sound_asset_exists(asset: AssetRef) -> bool:
    if not asset.resolved_path:
        return False
    resolved = Path(asset.resolved_path)
    return resolved.is_file() or resolved.is_dir()


def _prefixed_asset_output_subpath(orchestrator, asset: AssetRef) -> str:
    output_subpath = getattr(orchestrator, "_asset_output_subpath", None)
    if output_subpath is not None:
        return output_subpath(asset)

    subpath = orchestrator._asset_data_subpath(asset)
    source_profile = getattr(orchestrator, "_source_profile", None)
    if source_profile is not None:
        subpath = apply_asset_prefix(subpath, source_profile)
    return subpath


def _copy_sound_asset_with_prefix(orchestrator, asset: AssetRef) -> None:
    if not asset.resolved_path:
        return
    out_path = _prefixed_sound_output_path(orchestrator, asset)
    source_path = Path(asset.resolved_path)
    if source_path.is_dir():
        shutil.copytree(source_path, out_path, dirs_exist_ok=True)
        return
    out_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(asset.resolved_path, out_path)
