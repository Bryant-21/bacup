"""Phase: copy sounds."""
from __future__ import annotations

import shutil
from collections import defaultdict
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from bacup_lib.models import AssetRef, ConversionContext, PhaseProgress
    from bacup_lib.runner import ConversionRunner


def _sound_data_subpath(asset: "AssetRef") -> str:
    path = asset.source_path.replace("\\", "/").lstrip("/")
    lower = path.lower()
    if lower.startswith("data/music/"):
        return "Music/" + path[11:]
    if lower.startswith("music/"):
        return "Music/" + path[6:]
    if not path.lower().startswith("sound/"):
        path = "sound/" + path
    return path


def _sound_output_subpath(asset: "AssetRef", ctx: "ConversionContext") -> str:
    subpath = _sound_data_subpath(asset)
    try:
        from creation_lib.core.game_profiles import get_profile
        from bacup_lib.paths import apply_asset_prefix

        return apply_asset_prefix(subpath, get_profile(ctx.source_game))
    except Exception:
        return subpath


_AUDIO_EXTS = (".wav", ".xwm", ".fuz")


def _apply_resolved_extension(subpath: str, resolved_path: str | None) -> str:
    """Preserve the real source format: re-extension the output after the file
    that actually resolved on disk (a SNDR naming `.wav` may only exist as
    `.xwm`/`.fuz`), so byte-copies never land XWM/FUZ data in a `.wav`."""
    if not resolved_path:
        return subpath
    resolved = Path(resolved_path)
    if resolved.is_dir():
        return subpath
    resolved_ext = resolved.suffix.lower()
    if resolved_ext not in _AUDIO_EXTS:
        return subpath
    current = Path(subpath)
    cur_ext = current.suffix.lower()
    if cur_ext not in _AUDIO_EXTS or cur_ext == resolved_ext:
        return subpath
    return current.with_suffix(resolved_ext).as_posix()


def _sound_output_path(asset: "AssetRef", ctx: "ConversionContext") -> Path:
    subpath = _apply_resolved_extension(
        _sound_output_subpath(asset, ctx), asset.resolved_path
    )
    return Path(ctx.mod_path) / "data" / Path(subpath)


def _target_has_sound(asset: "AssetRef", ctx: "ConversionContext") -> bool:
    target_root = getattr(ctx, "target_extracted_dir", None)
    if not target_root:
        return False
    return (Path(target_root) / Path(_sound_data_subpath(asset))).exists()


def _copy_sound_asset(asset: "AssetRef", ctx: "ConversionContext") -> None:
    if not asset.resolved_path:
        return
    source_path = Path(asset.resolved_path)
    output_path = _sound_output_path(asset, ctx)
    if source_path.is_dir():
        shutil.copytree(source_path, output_path, dirs_exist_ok=True)
        return
    output_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source_path, output_path)


class _WarningExamples:
    def __init__(self, limit: int = 25) -> None:
        self.limit = limit
        self.examples: list[str] = []
        self.total = 0

    def add(self, message: str) -> None:
        self.total += 1
        if len(self.examples) < self.limit:
            self.examples.append(message)

    def emit(
        self,
        runner: "ConversionRunner",
        *,
        level: str,
        omitted_label: str,
    ) -> None:
        for message in self.examples:
            runner.emit_log(level, message)
        omitted = self.total - len(self.examples)
        if omitted > 0:
            runner.emit_log(level, f"{omitted} additional {omitted_label}")


def _emit_sound_output_collision_warnings(
    sound_assets: list["AssetRef"],
    ctx: "ConversionContext",
    runner: "ConversionRunner",
) -> None:
    by_output: dict[str, list["AssetRef"]] = defaultdict(list)
    display_path: dict[str, str] = {}
    for asset in sound_assets:
        subpath = _sound_output_subpath(asset, ctx).replace("\\", "/")
        key = subpath.lower()
        by_output[key].append(asset)
        display_path.setdefault(key, subpath)

    collision_count = 0
    for key, assets in by_output.items():
        source_paths = []
        seen_sources: set[str] = set()
        for asset in assets:
            source = asset.source_path.replace("\\", "/")
            source_key = source.lower()
            if source_key in seen_sources:
                continue
            seen_sources.add(source_key)
            source_paths.append(source)
        if len(source_paths) <= 1:
            continue
        collision_count += 1
        if collision_count <= 25:
            runner.emit_log(
                "WARN",
                "copy_sounds: duplicate output path "
                f"{display_path[key]} from {len(source_paths)} sources: "
                + "; ".join(source_paths[:8])
                + ("; ..." if len(source_paths) > 8 else ""),
            )
    if collision_count > 25:
        runner.emit_log(
            "WARN",
            f"copy_sounds: {collision_count - 25} additional duplicate output paths",
        )


def copy_sounds(
    assets: list["AssetRef"],
    ctx: "ConversionContext",
    runner: "ConversionRunner",
    progress: "PhaseProgress",
) -> None:
    """Copy resolved sound assets into the converted mod output."""
    sound_assets = [asset for asset in assets if asset.asset_type in {"sound", "audio"}]
    summary = ctx.summary
    summary.audio_total = len(sound_assets)
    progress.total_items = len(sound_assets)
    progress.completed_items = 0
    missing_audio_examples = _WarningExamples()
    _emit_sound_output_collision_warnings(sound_assets, ctx, runner)

    for index, asset in enumerate(sound_assets, start=1):
        if runner.is_cancelled():
            break
        progress.current_item = asset.source_path

        if _target_has_sound(asset, ctx):
            summary.audio_base_game_skipped += 1
        elif not asset.resolved_path:
            summary.audio_failed += 1
            missing_audio_examples.add(f"Audio not found: {asset.source_path}")
        else:
            source_path = Path(asset.resolved_path)
            output_path = _sound_output_path(asset, ctx)
            if not source_path.exists():
                summary.audio_failed += 1
                missing_audio_examples.add(f"Audio not found: {asset.source_path}")
            elif output_path.exists():
                pass
            else:
                try:
                    _copy_sound_asset(asset, ctx)
                except Exception as exc:
                    summary.audio_failed += 1
                    runner.emit_log("ERROR", f"Audio failed: {asset.source_path}: {exc}")
                else:
                    summary.audio_copied += 1

        progress.completed_items = index
        runner.emit_item_progress(progress)

    progress.current_item = ""
    missing_audio_examples.emit(
        runner,
        level="ERROR",
        omitted_label="audio files were not found",
    )
    runner.emit_log(
        "INFO",
        "copy_sounds: "
        f"copied={summary.audio_copied}, "
        f"base_game_skipped={summary.audio_base_game_skipped}, "
        f"failed={summary.audio_failed}",
    )


def copy_sounds_native(
    assets: list["AssetRef"],
    ctx: "ConversionContext",
    runner: "ConversionRunner",
    progress: "PhaseProgress",
) -> None:
    """Native copy_sounds phase (parity with copy_sounds above)."""
    from bacup_lib.workflows.asset_phases import _drain_native_phase_events

    sound_assets = [a for a in assets if a.asset_type in {"sound", "audio"}]
    summary = ctx.summary
    summary.audio_total = len(sound_assets)
    progress.total_items = len(sound_assets)
    _emit_sound_output_collision_warnings(sound_assets, ctx, runner)
    rust_run = getattr(ctx, "_rust_conversion_run", None)
    if rust_run is None:
        runner.emit_log("WARN", "copy_sounds: no Rust run; falling back to Python copy")
        copy_sounds(assets, ctx, runner, progress)
        return
    params = {
        "sound_paths": [
            {"source_path": a.source_path, "resolved_path": a.resolved_path or ""}
            for a in sound_assets
        ],
    }
    report = rust_run.run_phase(
        "copy_sounds",
        mod_path=str(ctx.mod_path),
        target_extracted_dir=str(ctx.target_extracted_dir or ""),
        params=params,
    )
    _drain_native_phase_events(rust_run, runner, "copy_sounds")
    summary.audio_copied += int(report.get("assets_written", 0))
    summary.audio_base_game_skipped += int(report.get("records_dropped", 0))
    summary.audio_failed += int(report.get("warnings", 0))
    progress.completed_items = len(sound_assets)
    runner.emit_item_progress(progress)
