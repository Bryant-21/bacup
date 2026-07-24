"""Unified FO76→FO4 regen driver: record track + concurrent asset waves, the
sink join (loose-tree reconcile + BA2 plan/finalize), and the cache manifest."""
from __future__ import annotations

import hashlib
import contextlib
import gc
import json
import logging
import os
import re
import shutil
import subprocess
import tempfile
import threading
import time
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import TYPE_CHECKING, Any, Callable, Iterable, Mapping

from creation_lib.build.archive_plan import (
    DEFAULT_ARCHIVE_MAX_BYTES,
    PlannedArchive,
    discover_mod_archives,
    plan_archive_outputs,
)
from creation_lib.build.packer import (
    _inventory_data_entries,
    _inventory_root_strings_entries,
    _run_native_pack_plans,
    _validate_archive_size,
)
from bacup_lib import pipeline
from bacup_lib.asset_paths import normalize_asset_source_path
from bacup_lib.base_asset_dedupe import (
    resolve_base_asset_namespace,
    resolve_base_asset_relocation_mesh_roots,
)
from bacup_lib.formkey.formkey_mapper import FormKeyMapper
from bacup_lib.generated_ids import (
    generated_object_id_floor as _generated_object_id_floor,
)
from bacup_lib.models import (
    AssetProvenance,
    AssetRef,
    ConversionContext,
    ConversionDecisionKind,
    ConversionSummary,
    ConvertedPluginRegistry,
    PhaseProgress,
    PluginPortOptions,
    PluginPortRequest,
    RecordNode,
    RunResult,
    WorldspaceCellBounds,
)
from bacup_lib.native_maps import native_face_resources_dir
from bacup_lib.native_runtime import load_native_module
from bacup_lib.record.translation_map_data import (
    load_translation_map_overrides,
)
from bacup_lib.runner import Drainer, emit_runner_status
from bacup_lib.target_masters import (
    close_plugin_handles,
    resolve_required_target_master_path,
    resolve_target_master_paths,
    resolve_target_master_plugin_paths,
)
from bacup_lib.target_preflight import (
    build_target_asset_index,
    build_target_record_preflight,
)
from bacup_lib.target_assets import build_target_asset_store
from bacup_lib.timing_report import TimingReport
from creation_lib.esp import native_runtime
from creation_lib.esp.plugin import Plugin

if TYPE_CHECKING:
    from bacup_lib.runner import ConversionRunner

_FO76_PIPBOY_MAP_REL = Path("textures/interface/pip-boy/papermap_city_d.dds")
_FO76_WEB_MAP_PNG_REL = Path(
    "PrismaUI_F4/views/B21_FullScreenMap/maps/appalachia/map.png"
)
_FO4_PIPBOY_MAP_SIZE = (2048, 2048)
_FO76_VAULTBOY_SWF_TREES = (
    (
        Path("interface/components/vaultboys"),
        Path("Interface/Components/VaultBoys"),
    ),
    (
        Path("interface/components/quest vault boys"),
        Path("Interface/Components/Quest Vault Boys"),
    ),
)


def _conversion_target_master_inputs(request: PluginPortRequest) -> list[Path]:
    paths = [Path(path) for path in request.target_master_paths]
    if (
        request.source_game == "fo76"
        and request.target_game == "fo4"
        and request.options.translate_records
    ):
        try:
            xdi_master = resolve_required_target_master_path(
                "XDI.esm",
                target_master_paths=paths,
                target_data_dir=request.target_data_dir,
                target_extracted_dir=None,
            )
        except FileNotFoundError as error:
            raise FileNotFoundError(
                "FO76→FO4 record conversion requires XDI.esm as a master; "
                "install Extended Dialogue Interface in Fallout 4 Data or "
                "another mod in the selected mod manager's mods directory"
            ) from error
        known = {str(path.resolve()).casefold() for path in paths if path.is_file()}
        if str(xdi_master.resolve()).casefold() not in known:
            paths.append(xdi_master)
    return paths


def _terrain_graft_source(opts: "PluginPortOptions", mod_path: str) -> Path:
    """Prior-handle source for the terrain graft.

    Upgrade mode points ``terrain_graft_esm`` at the live deployed ``SeventySix.esm``;
    otherwise the legacy ``--re-use-land`` run-local ``.regen_land_cache.esm`` is used.
    """
    graft_esm = getattr(opts, "terrain_graft_esm", None)
    if graft_esm is not None:
        return Path(graft_esm)
    return Path(mod_path) / ".regen_land_cache.esm"


def _stamp_target_plugin_version(request, ctx, runner) -> None:
    """Stamp the release version (``request.mod_version``) into the target
    plugin's TES4 SNAM in-memory, before build_esp serializes it.

    No-op when no version or conversion run is set.
    """
    version = getattr(request, "mod_version", None)
    if not version:
        return
    run = getattr(ctx, "_rust_conversion_run", None)
    if run is None:
        return
    from bacup_lib.version_stamp import stamp_plugin_version

    stamp_plugin_version(run, str(version))


def _safe_emit_log(
    runner: "ConversionRunner | None",
    level: str,
    message: str,
) -> None:
    if runner is not None:
        runner.emit_log(level, message)


def _resolve_fo76_translate_tokens(
    tables: dict[str, dict[int, str]],
    extracted_dir: str | Path | None,
) -> int:
    """Resolve whole-string FO76 UI tokens through interface/translate_*.txt."""
    import re

    if not extracted_dir:
        return 0
    root = Path(extracted_dir)
    token_re = re.compile(r"^\$[A-Z0-9_]+$")
    cache: dict[str, tuple[dict[str, str], dict[str, str]]] = {}

    def _maps_for_language(lang: str) -> tuple[dict[str, str], dict[str, str]]:
        if lang in cache:
            return cache[lang]
        path = root / "interface" / f"translate_{lang}.txt"
        direct: dict[str, str] = {}
        normalized: dict[str, str] = {}
        if path.is_file():
            for line in path.read_bytes().decode("utf-16").splitlines():
                key, sep, value = line.partition("\t")
                if not sep:
                    continue
                direct[key] = value
                normalized[key.replace("_", "")] = value
        cache[lang] = (direct, normalized)
        return cache[lang]

    rewritten = 0
    for lang, table in tables.items():
        direct, normalized = _maps_for_language(str(lang))
        if not direct and not normalized:
            continue
        for sid, value in list(table.items()):
            if not isinstance(value, str) or not token_re.match(value):
                continue
            resolved = direct.get(value) or normalized.get(value.replace("_", ""))
            if resolved:
                table[sid] = resolved.title()
                rewritten += 1
    return rewritten


def _finalize_fo76_pipboy_map_texture(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None",
) -> None:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return
    if not request.options.convert_textures:
        return

    source_root = getattr(ctx, "source_data_dir", None) or getattr(
        ctx, "source_extracted_dir", None
    )
    if not source_root:
        _safe_emit_log(
            runner,
            "WARN",
            "FO76 Appalachia Pip-Boy map texture skipped: source_data_dir is not set",
        )
        return

    source_path = Path(source_root) / _FO76_PIPBOY_MAP_REL
    if not source_path.is_file():
        _safe_emit_log(
            runner,
            "WARN",
            f"FO76 Appalachia Pip-Boy map texture not found: {source_path}",
        )
        return

    from PIL import Image

    from creation_lib.dds.io import load_image, save_image

    output_path = Path(ctx.mod_path) / "data" / _FO76_PIPBOY_MAP_REL
    web_map_path = Path(ctx.mod_path) / _FO76_WEB_MAP_PNG_REL
    image = load_image(str(source_path), mode="RGBA")
    web_image = image.copy()
    try:
        web_map_path.parent.mkdir(parents=True, exist_ok=True)
        web_image.save(web_map_path, format="PNG", optimize=True)

        # FO4's Pip-Boy map UI expects vanilla-style legacy DXT map textures.
        if image.size != _FO4_PIPBOY_MAP_SIZE:
            resized = image.resize(_FO4_PIPBOY_MAP_SIZE, Image.Resampling.LANCZOS)
            image.close()
            image = resized
        output_path.parent.mkdir(parents=True, exist_ok=True)
        save_image(
            image,
            str(output_path),
            format="DXT1",
            generate_mips=True,
            use_gpu=False,
        )
    finally:
        web_image.close()
        image.close()
    _safe_emit_log(
        runner,
        "INFO",
        f"Wrote FO4-compatible Appalachia Pip-Boy map texture: {output_path}",
    )
    _safe_emit_log(
        runner,
        "INFO",
        f"Wrote generated Appalachia fullscreen map image: {web_map_path}",
    )


def _copy_fo76_vaultboy_swfs(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (
        request.source_game.lower(),
        request.target_game.lower(),
    ) != ("fo76", "fo4"):
        return 0

    source_root = Path(ctx.source_data_dir)
    output_root = Path(ctx.mod_path) / "data"
    copied = 0
    for source_relative, output_relative in _FO76_VAULTBOY_SWF_TREES:
        source_tree = source_root / source_relative
        if not source_tree.is_dir():
            _safe_emit_log(
                runner,
                "WARN",
                f"FO76 VaultBoy SWF tree not found: {source_tree}",
            )
            continue
        for source_file in sorted(source_tree.rglob("*.swf")):
            if not source_file.is_file():
                continue
            output_file = output_root / output_relative / source_file.relative_to(
                source_tree
            )
            output_file.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source_file, output_file)
            copied += 1

    _safe_emit_log(runner, "INFO", f"Copied {copied} FO76 VaultBoy SWF asset(s)")
    return copied


def _normalize_streamed_rel(rel: str) -> str:
    """The Rust spill writers key entries by lowercase forward-slash rels."""
    return rel.replace("\\", "/").lstrip("/").lower()


def _cleanup_temp_save_strings(temp_plugin_path: Path) -> None:
    """The native plugin save derives localized-strings filenames from the
    output plugin's stem, so saving to a mkstemp path emits orphaned
    ``Strings/<temp stem>_<lang>.*`` tables that ``os.replace`` never moves."""
    strings_dir = temp_plugin_path.parent / "Strings"
    if not strings_dir.is_dir():
        return
    for orphan in strings_dir.glob(f"{temp_plugin_path.stem}_*"):
        try:
            orphan.unlink()
        except OSError:
            pass


def _resolve_source_strings_dir(
    source_plugin: Path,
    *configured_roots: Path | str | None,
) -> str | None:
    candidates = [source_plugin.parent / "Strings"]
    for configured_root in configured_roots:
        if not configured_root:
            continue
        root = Path(configured_root)
        candidates.append(
            root if root.name.casefold() == "strings" else root / "Strings"
        )

    for candidate in candidates:
        if candidate.is_dir():
            return str(candidate)
    return None


def _apply_placed_record_position_offset(
    *,
    run_id: int,
    offset: tuple[float, float, float],
) -> int:
    if offset == (0.0, 0.0, 0.0):
        return 0
    return int(
        load_native_module().conversion_run_apply_placed_record_position_offset(
            run_id,
            float(offset[0]),
            float(offset[1]),
            float(offset[2]),
        )
    )


def _sync_cell_regions_from_source(
    run_id: int,
    *,
    source_worldspace_editor_id: str,
    target_worldspace_editor_id: str,
) -> dict[str, Any]:
    return load_native_module().conversion_run_sync_cell_regions_from_source(
        run_id,
        source_worldspace_editor_id,
        target_worldspace_editor_id,
    )


def inventory_packable_entries(mod_root: Path) -> list:
    """The inventory pack_mod archives: ``data/**`` (textures included) + root
    ``Strings/``. ``Terrain/`` sits at the mod root and is therefore never
    inventoried (sidecars stay loose)."""
    mod_root = Path(mod_root)
    entries = _inventory_data_entries(mod_root / "data", include_textures=True)
    entries.extend(_inventory_root_strings_entries(mod_root / "Strings"))
    return entries


def _archive_label_selected(label: str, labels: tuple[str, ...]) -> bool:
    return any(
        label == wanted
        or (label.startswith(wanted) and label[len(wanted) :].isdigit())
        for wanted in labels
    )


def _archive_file_label(path: Path, mod_name: str) -> str:
    label = path.stem[len(f"{mod_name} - ") :]
    return label.removesuffix("_xbox")


def finalize_sinks_for_mod(
    sink_id: int,
    mod_root: Path,
    *,
    mod_name: str,
    archive_max_bytes: int = DEFAULT_ARCHIVE_MAX_BYTES,
    game: str = "fo4",
    reconcile_workers: int | None = None,
    direct_pack_textures: bool = True,
    direct_pack_all: bool = False,
    texture_pack_workers: int | None = None,
    expanded_archives: bool = True,
    archive_output_dir: Path | None = None,
    fo4_ba2_target: str = "nextgen",
    archive_labels: tuple[str, ...] | None = None,
    pack_progress: Callable[[dict], bool | None] | None = None,
) -> list[PlannedArchive]:
    """The sink join: reconcile the loose tree into the spills, plan shards with
    the legacy planner, finalize non-texture archives from the spills, and
    direct-pack texture archives from the loose tree. Texture direct-pack
    preserves the legacy parallel archive shape; the incremental DX10 spill is
    slower on the full FO76 corpus.

    On any error: spills are aborted and every archive written by THIS call
    (including the partial in-flight one) is deleted.

    ``archive_labels`` limits reconciliation, packing, and stale-output cleanup
    to the selected BA2 labels. Unselected archives remain untouched.
    """
    native = load_native_module()
    mod_root = Path(mod_root)
    archive_root = Path(archive_output_dir) if archive_output_dir is not None else mod_root
    archive_root.mkdir(parents=True, exist_ok=True)
    use_temp_outputs = not os.path.samefile(archive_root, mod_root)
    entries = inventory_packable_entries(mod_root)

    # The shard plan uses the shared archive planner so the sink join and
    # pack_mod agree on classification, ordering, and size checks.
    plans = plan_archive_outputs(
        mod_name,
        entries,
        "ba2",
        "",
        archive_max_bytes,
        game=game,
        expanded_archives=expanded_archives,
    )
    if archive_labels is not None:
        plans = [
            planned
            for planned in plans
            if _archive_label_selected(planned.label, archive_labels)
        ]
    selected_rels = {
        _normalize_streamed_rel(entry.relative_path)
        for planned in plans
        for entry in planned.entries
    }
    direct_pack_rels = {
        _normalize_streamed_rel(entry.relative_path)
        for planned in plans
        if direct_pack_all or (direct_pack_textures and planned.texture_archive)
        for entry in planned.entries
    }

    # Reconcile every packable output that exists in the tree. Conversion flags
    # decide what gets generated; final BA2 packing should not drop existing
    # assets just because the phase that usually owns them was skipped.
    streamed = set(native.sinks_streamed(sink_id))
    missing: list[tuple[str, str]] = []
    for entry in entries:
        normalized_rel = _normalize_streamed_rel(entry.relative_path)
        if archive_labels is not None and normalized_rel not in selected_rels:
            continue
        if normalized_rel in streamed or normalized_rel in direct_pack_rels:
            continue
        missing.append((str(entry.source_path), entry.relative_path))
    if missing:
        native.sinks_add_files(sink_id, missing, reconcile_workers)

    written: list[Path] = []

    def output_paths(name: str) -> tuple[Path, Path]:
        final = archive_root / name
        if not use_temp_outputs:
            return final, final
        temp = final.with_name(f"{final.name}.tmp")
        temp.unlink(missing_ok=True)
        return temp, final

    def commit_output(written_path: Path, final_path: Path) -> None:
        if written_path == final_path:
            return
        os.replace(written_path, final_path)
        written.append(final_path)

    try:
        direct_pack_plans = [
            planned
            for planned in plans
            if direct_pack_all or (direct_pack_textures and planned.texture_archive)
        ]
        spill_plans = [
            planned
            for planned in plans
            if not (direct_pack_all or (direct_pack_textures and planned.texture_archive))
        ]
        og_target = fo4_ba2_target == "og"
        if og_target and spill_plans:
            raise RuntimeError(
                "OG BA2 target requires the direct-pack path, but spill plans were "
                "produced; refusing to emit next-gen v8 archives for an OG target."
            )
        for planned in spill_plans:
            output, final_output = output_paths(planned.output_name)
            written.append(output)
            native.sinks_finalize_archive(
                sink_id,
                str(output),
                planned.texture_archive,
                json.dumps([entry.relative_path for entry in planned.entries]),
            )
            commit_output(output, final_output)

        requested_pack_workers = max(
            1,
            int(texture_pack_workers if texture_pack_workers is not None else 3),
        )
        direct_outputs: list[tuple[PlannedArchive, Path, Path]] = []
        for planned in direct_pack_plans:
            output, final_output = output_paths(planned.output_name)
            written.append(output)
            direct_outputs.append((planned, output, final_output))
        if direct_pack_plans:
            _run_native_pack_plans(
                [(planned, output) for planned, output, _final in direct_outputs],
                game,
                og=og_target,
                total_workers=requested_pack_workers,
                progress=pack_progress,
            )
        for _planned, output, final_output in direct_outputs:
            _validate_archive_size(output, archive_max_bytes)
            commit_output(output, final_output)
    except Exception:
        native.sinks_abort(sink_id)
        for path in written:
            try:
                path.unlink(missing_ok=True)
            except OSError:
                pass
        raise

    expected_names = {planned.output_name for planned in plans}
    for archive in discover_mod_archives(archive_root, mod_name, extensions=(".ba2",)):
        selected = archive_labels is None or _archive_label_selected(
            _archive_file_label(archive, mod_name), archive_labels
        )
        if selected and archive.name not in expected_names:
            archive.unlink()
    if use_temp_outputs:
        for archive in discover_mod_archives(mod_root, mod_name, extensions=(".ba2",)):
            if archive_labels is not None and not _archive_label_selected(
                _archive_file_label(archive, mod_name), archive_labels
            ):
                continue
            try:
                archive.unlink()
            except OSError as exc:
                _log.warning(
                    "Could not remove local archive duplicate %s: %s", archive, exc
                )

    native.sinks_cleanup_spills(sink_id)
    return plans


# ---------------------------------------------------------------------------
# Cache manifest
# ---------------------------------------------------------------------------

# Bump a phase's version on ANY output-affecting converter change — this is a
# manual contract: --cache trusts (source blake3, converter version, params
# digest, outputs-exist) as the full reuse key.
CONVERTER_VERSIONS: dict[str, str] = {
    "textures": "1",
    "materials": "1",
    "nifs": "1",
    "btos": "1",
    "sounds": "1",
    "havok": "1",
    "animations": "1",
    "drivers": "1",
    "scripts": "3",
}

MANIFEST_NAME = "manifest.json"

# Entry-list keys excluded from params digests: the digest captures
# conversion KNOBS, not the input set.
_ENTRY_LIST_PARAM_KEYS = (
    "textures",
    "nifs",
    "btos",
    "materials",
    "sound_paths",
    "havok_paths",
    "animations",
    "entries",
)


@dataclass(frozen=True)
class CacheAssetEntry:
    """One convertible source asset for the cache manifest."""

    source_path: str
    phase: str
    params_digest: str
    outputs: tuple[str, ...]  # data-relative outputs


def params_digest(params: Mapping) -> str:
    """sha256 of the canonical JSON of the phase params MINUS the entry
    lists, so the digest captures conversion knobs, not the input set."""
    filtered = {k: v for k, v in params.items() if k not in _ENTRY_LIST_PARAM_KEYS}
    canonical = json.dumps(filtered, sort_keys=True, separators=(",", ":"), default=str)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def write_cache_manifest(
    mod_root: Path,
    entries: Iterable[CacheAssetEntry],
    *,
    hash_workers: int | None = None,
) -> Path:
    """Write ``mods/<mod>/manifest.json`` (EVERY run, at join). Sources are
    batch-hashed via the native blake3 hasher; missing sources are recorded
    with a null hash (they can never cache-hit)."""
    native = load_native_module()
    mod_root = Path(mod_root)
    entries = list(entries)

    hashable = [e for e in entries if os.path.isfile(e.source_path)]
    hashes = (
        native.conversion_hash_files_blake3([e.source_path for e in hashable], hash_workers)
        if hashable
        else []
    )
    digest_by_source = dict(zip((e.source_path for e in hashable), hashes, strict=True))

    manifest = {
        "version": 1,
        "written_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "converters": dict(CONVERTER_VERSIONS),
        "assets": {
            entry.source_path: {
                "blake3": digest_by_source.get(entry.source_path),
                "phase": entry.phase,
                "params_digest": entry.params_digest,
                "outputs": list(entry.outputs),
            }
            for entry in entries
        },
    }
    path = mod_root / MANIFEST_NAME
    tmp = path.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(manifest, indent=1), encoding="utf-8")
    os.replace(tmp, path)
    return path


def consult_cache(
    manifest_path: Path,
    candidates: Iterable[CacheAssetEntry],
    *,
    mod_root: Path,
    hash_workers: int | None = None,
) -> set[str]:
    """Return the skip-set (source_path values) for ``--cache``: candidates
    whose recorded hash + converter version + params digest all match AND
    whose recorded outputs all exist under ``mod_root``. A changed source
    byte, a converter bump, a knob change, or a missing output re-converts."""
    manifest_path = Path(manifest_path)
    if not manifest_path.is_file():
        return set()
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return set()
    if manifest.get("version") != 1:
        return set()
    assets = manifest.get("assets", {})
    recorded_converters = manifest.get("converters", {})
    mod_root = Path(mod_root)
    native = load_native_module()

    cheap_pass: list[CacheAssetEntry] = []
    for entry in candidates:
        rec = assets.get(entry.source_path)
        if not rec or not rec.get("blake3"):
            continue
        if recorded_converters.get(entry.phase) != CONVERTER_VERSIONS.get(entry.phase):
            continue
        if rec.get("params_digest") != entry.params_digest:
            continue
        outputs = rec.get("outputs") or []
        if not outputs:
            continue
        if not all((mod_root / "data" / out).is_file() for out in outputs):
            continue
        if not os.path.isfile(entry.source_path):
            continue
        cheap_pass.append(entry)

    if not cheap_pass:
        return set()
    hashes = native.conversion_hash_files_blake3(
        [e.source_path for e in cheap_pass], hash_workers
    )
    skip: set[str] = set()
    for entry, digest in zip(cheap_pass, hashes, strict=True):
        if assets[entry.source_path]["blake3"] == digest:
            skip.add(entry.source_path)
    return skip


# ---------------------------------------------------------------------------
# UnifiedDriver — record track + signals
# ---------------------------------------------------------------------------


if TYPE_CHECKING:
    from bacup_lib.runner import ConversionRunner

_log = logging.getLogger("conversion.unified")
_FULL_WORLDSPACE_CELL_MIN = -32768
_FULL_WORLDSPACE_CELL_MAX = 32767
# Placed-record signatures dropped when PluginPortOptions.convert_placed_records
# is False. NAVM is intentionally excluded so navmeshes/terrain still convert.
_PLACED_RECORD_SIGNATURES = (
    "REFR",
    "ACHR",
    "PHZD",
    "PGRE",
    "PMIS",
    "PARW",
    "PBAR",
    "PBEA",
    "PCON",
)


def _emit_best_effort_runner_log(
    runner: "ConversionRunner",
    level: str,
    message: str,
) -> None:
    try:
        runner.emit_log(level, message)
    except Exception:
        log_level = logging.WARNING if level == "WARN" else logging.INFO
        try:
            _log.log(log_level, message)
        except Exception:
            pass


def _elapsed_seconds(started_at: float) -> float:
    return max(time.perf_counter() - started_at, 0.0)


def _format_elapsed_seconds(elapsed_seconds: float) -> str:
    return f"{elapsed_seconds:.3f}s"


def _timing_report(ctx: ConversionContext) -> TimingReport:
    existing = getattr(ctx, "timing_report", None)
    if isinstance(existing, TimingReport):
        return existing
    report = TimingReport()
    setattr(ctx, "timing_report", report)
    return report


def _record_timing(
    ctx: ConversionContext,
    name: str,
    started_at: float,
    **fields: object,
) -> None:
    _timing_report(ctx).record(name, time.perf_counter() - started_at, **fields)


def _memory_stage(ctx: ConversionContext | None, stage: str, **fields: object):
    if ctx is None:
        return contextlib.nullcontext()
    memory_report = getattr(_timing_report(ctx), "memory_report", None)
    scoped_stage = getattr(memory_report, "scoped_stage", None)
    if callable(scoped_stage):
        return scoped_stage(stage, **fields)
    return contextlib.nullcontext()


# Phase boundaries that free gigabytes: mimalloc retains freed pages in its
# segment cache, so these drops don't show in RSS until decommitted. Trim here
# so each free actually lands in peak RSS. Production-relevant, not just
# profiling — the snapshot below no-ops without a MemoryReport, but the trim
# always runs.
_TRIM_AFTER_MARKS = frozenset(
    {
        "after:masters_open",
        "after:translate",
        "after:early_source_close",
        "after:repair",
        "after:masters_early_close",
        "after:masters_close",
    }
)


def _memory_mark(ctx: ConversionContext | None, label: str) -> None:
    """Snapshot RSS with a named label if a MemoryReport is attached to ctx.

    Also returns mimalloc's cached free pages to the OS at the heavy-free
    boundaries in ``_TRIM_AFTER_MARKS``.
    """
    if ctx is None:
        return
    memory_report = getattr(_timing_report(ctx), "memory_report", None)
    mark = getattr(memory_report, "mark", None)
    if callable(mark):
        mark(label)
    if label in _TRIM_AFTER_MARKS:
        try:
            from creation_lib.esp import native_runtime as _esp_native

            _esp_native.trim_allocator()
        except Exception:
            pass


def _append_unique_strings(target: list[str], values: list[str]) -> None:
    seen = set(target)
    for value in values:
        if value in seen:
            continue
        target.append(value)
        seen.add(value)


def _validation_issue_severity(issue: object) -> str:
    severity = getattr(issue, "severity", "")
    return str(getattr(severity, "value", severity))


def _validation_issue_text(issue: object) -> str:
    form_id = getattr(issue, "form_id", None)
    message = str(getattr(issue, "message", issue))
    if isinstance(form_id, int):
        return f"{form_id:08X}: {message}"
    return message


_SCRIPT_CONDITION_FUNCTION_IDS = {629, 659, 660}
_SCRIPT_REF_SUBRECORD_SIGNATURES = ["VMAD", "CTDA"]
_SCRIPT_WARNING_LIMIT = 75


@dataclass(frozen=True, slots=True)
class _ScriptReference:
    script_name: str
    variable_name: str | None
    form_key: str
    form_id: int
    record_sig: str
    editor_id: str
    kind: str
    condition_inferred: bool = False


@dataclass(slots=True)
class _ScriptResolution:
    script_name: str
    status: str
    pex_path: Path | None = None
    message: str = ""

    @property
    def ok(self) -> bool:
        return self.status in {"target", "compiled"}


def _script_key(script_name: str) -> str:
    return script_name.replace("\\", ":").replace("/", ":").strip().lower()


def _variable_key(variable_name: str | None) -> str:
    return (variable_name or "").strip().lower()


def _script_relative_path(script_name: str, suffix: str) -> Path:
    normalized = script_name.replace("\\", ":").replace("/", ":")
    parts = [part for part in normalized.split(":") if part]
    if not parts:
        parts = [script_name]
    return Path(*parts).with_suffix(suffix)


# Fix folder for hand-written / AI-generated fills of hollow FO76 scripts.
# FO76 ships many activator/furniture scripts with their bodies stripped
# server-side, so the decompiled stub keeps the correct Scriptname / Extends /
# property list but has no logic. A patch here supplies ONLY the method/event
# bodies — keyed by the script's namespaced path (e.g.
# `WindChimesActivatorScript.psc`, `Fragments/Quests/Foo.psc`). The pipeline
# splices those members into the decompiled skeleton (replacing a same-named stub
# or appending a missing one), so the game-derived declarations stay intact and
# this directory holds ONLY original code — never Papyrus decompiled from the game.
_SCRIPT_PATCH_DIR = Path(__file__).resolve().parents[1] / "script_patches"

_FO76_TO_FO4_SCRIPT_VARIABLE_ADDITIONS = {
    _script_key("DefaultAliasInventoryManagement"): (
        ("ObjectReference", "ShutdownReferenceCache"),
    ),
}

_FO76_TO_FO4_SCRIPT_PROPERTY_ADDITIONS = {
    _script_key("RadioGeneral_MasterScript"): (
        ("Int[]", "songFormIDs", "Int[] Property songFormIDs Auto Const Mandatory"),
    ),
}

_PAPYRUS_SCRIPT_HEADER = re.compile(r"^\s*Scriptname\s+", re.IGNORECASE)
_PAPYRUS_TOP_LEVEL_VARIABLE = re.compile(
    r"^\s*(?P<type>[A-Za-z_][\w:]*(?:\[\])*)\s+(?P<name>[A-Za-z_]\w*)\b",
    re.IGNORECASE,
)
_PAPYRUS_TOP_LEVEL_PROPERTY = re.compile(
    r"^\s*(?P<type>[A-Za-z_][\w:]*(?:\[\])*)\s+Property\s+"
    r"(?P<name>[A-Za-z_]\w*)\b",
    re.IGNORECASE,
)

_PAPYRUS_MEMBER_START = re.compile(
    r"^\s*(?:(?P<ret>[A-Za-z_][\w\[\]]*)\s+)?(?P<kind>Function|Event)\s+"
    r"(?P<name>[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)?)\s*\(",
    re.IGNORECASE,
)
_PAPYRUS_MEMBER_END = re.compile(r"^\s*End(?:Function|Event)\b", re.IGNORECASE)
_PAPYRUS_STATE_START = re.compile(
    r"^(?P<indent>\s*)(?P<auto>Auto\s+)?State\s+(?P<name>[A-Za-z_]\w*)\b",
    re.IGNORECASE,
)
_PAPYRUS_STATE_END = re.compile(r"^\s*EndState\b", re.IGNORECASE)
_PAPYRUS_STATE_RENAME = re.compile(
    r"^\s*;\s*@state-rename\s+(?P<old>[A-Za-z_]\w*)\s+"
    r"(?P<new>[A-Za-z_]\w*)\s*$",
    re.IGNORECASE,
)


def _script_patch_source(script_name: str) -> str | None:
    """Return the fix-folder patch text for `script_name`, or None if none exists."""
    patch_path = _SCRIPT_PATCH_DIR / _script_relative_path(script_name, ".psc")
    if patch_path.is_file():
        return patch_path.read_text(encoding="utf-8")
    return None


def _augment_fo76_to_fo4_script_skeleton(script_name: str, skeleton: str) -> str:
    script_key = _script_key(script_name)
    variable_additions = _FO76_TO_FO4_SCRIPT_VARIABLE_ADDITIONS.get(script_key, ())
    property_additions = _FO76_TO_FO4_SCRIPT_PROPERTY_ADDITIONS.get(script_key, ())
    if not variable_additions and not property_additions:
        return skeleton

    lines = skeleton.splitlines()
    header_indexes = [
        index for index, line in enumerate(lines) if _PAPYRUS_SCRIPT_HEADER.match(line)
    ]
    if len(header_indexes) != 1:
        raise ValueError(
            f"Papyrus skeleton augmentation requires exactly one Scriptname header; "
            f"found {len(header_indexes)} for {script_name}"
        )

    insert_at = header_indexes[0] + 1
    for type_name, variable_name in variable_additions:
        variable_key = variable_name.lower()
        top_level_members = _iter_top_level_papyrus_members(lines)
        member_collisions = [
            lines[start]
            for _kind, name, start, _end in top_level_members
            if name == variable_key
        ]
        if member_collisions:
            found = ", ".join(member_collisions)
            raise ValueError(
                f"conflicting Papyrus member for {variable_name} in "
                f"{script_name}: {found}"
            )

        excluded_lines: set[int] = set()
        for _name, start, end in _iter_papyrus_states(lines):
            excluded_lines.update(range(start, end + 1))
        for _kind, _name, start, end in top_level_members:
            excluded_lines.update(range(start, end + 1))

        declarations: list[tuple[str, str]] = []
        properties: list[str] = []
        for index, line in enumerate(lines):
            if index in excluded_lines:
                continue
            property_match = _PAPYRUS_TOP_LEVEL_PROPERTY.match(line)
            if (
                property_match is not None
                and property_match.group("name").lower() == variable_key
            ):
                properties.append(line)
                continue
            match = _PAPYRUS_TOP_LEVEL_VARIABLE.match(line)
            if match is None or match.group("name").lower() != variable_key:
                continue
            declarations.append((match.group("type"), line))

        if properties:
            found = ", ".join(properties)
            raise ValueError(
                f"conflicting Papyrus property for {variable_name} in "
                f"{script_name}: {found}"
            )

        if declarations:
            if len(declarations) != 1 or declarations[0][0].lower() != type_name.lower():
                found = ", ".join(line for _found_type, line in declarations)
                raise ValueError(
                    f"conflicting Papyrus declaration for {variable_name} in "
                    f"{script_name}: {found}"
                )
            continue

        lines.insert(insert_at, f"{type_name} {variable_name}")
        insert_at += 1

    for type_name, property_name, declaration in property_additions:
        property_key = property_name.lower()
        top_level_members = _iter_top_level_papyrus_members(lines)
        member_collisions = [
            lines[start]
            for _kind, name, start, _end in top_level_members
            if name == property_key
        ]
        if member_collisions:
            found = ", ".join(member_collisions)
            raise ValueError(
                f"conflicting Papyrus member for {property_name} in "
                f"{script_name}: {found}"
            )

        excluded_lines: set[int] = set()
        for _name, start, end in _iter_papyrus_states(lines):
            excluded_lines.update(range(start, end + 1))
        for _kind, _name, start, end in top_level_members:
            excluded_lines.update(range(start, end + 1))

        variables: list[str] = []
        properties: list[tuple[str, str]] = []
        for index, line in enumerate(lines):
            if index in excluded_lines:
                continue
            property_match = _PAPYRUS_TOP_LEVEL_PROPERTY.match(line)
            if (
                property_match is not None
                and property_match.group("name").lower() == property_key
            ):
                properties.append((property_match.group("type"), line))
                continue
            variable_match = _PAPYRUS_TOP_LEVEL_VARIABLE.match(line)
            if (
                variable_match is not None
                and variable_match.group("name").lower() == property_key
            ):
                variables.append(line)

        if variables:
            found = ", ".join(variables)
            raise ValueError(
                f"conflicting Papyrus declaration for {property_name} in "
                f"{script_name}: {found}"
            )
        if properties:
            if len(properties) != 1 or properties[0][0].lower() != type_name.lower():
                found = ", ".join(line for _found_type, line in properties)
                raise ValueError(
                    f"conflicting Papyrus property for {property_name} in "
                    f"{script_name}: {found}"
                )
            continue

        lines.insert(insert_at, declaration)
        insert_at += 1

    merged = "\n".join(lines)
    return f"{merged}\n" if skeleton.endswith(("\n", "\r")) else merged


def _script_body_is_hollow(psc_source: str) -> bool:
    """True when a decompiled script has no event handlers — the tell-tale of an
    FO76 script whose body was stripped server-side (states only, no logic)."""
    return not any(
        line.lstrip().lower().startswith("event ") for line in psc_source.splitlines()
    )


def _iter_top_level_papyrus_members(
    lines: list[str],
) -> list[tuple[str, str, int, int]]:
    """Yield ``(kind, name, start, end)`` for each top-level Function/Event block —
    those in the script's empty (default) state, not nested inside a named State.
    ``kind``/``name`` are lower-cased; ``start``/``end`` are inclusive line indices."""
    members: list[tuple[str, str, int, int]] = []
    depth = 0
    i, n = 0, len(lines)
    while i < n:
        if _PAPYRUS_STATE_END.match(lines[i]):
            depth = max(0, depth - 1)
            i += 1
            continue
        start_match = _PAPYRUS_MEMBER_START.match(lines[i])
        if start_match:
            end = i + 1
            while end < n and not _PAPYRUS_MEMBER_END.match(lines[end]):
                end += 1
            if depth == 0:
                members.append(
                    (
                        start_match.group("kind").lower(),
                        start_match.group("name").lower(),
                        i,
                        min(end, n - 1),
                    )
                )
            i = end + 1
            continue
        if _PAPYRUS_STATE_START.match(lines[i]):
            depth += 1
        i += 1
    return members


def _iter_papyrus_states(lines: list[str]) -> list[tuple[str, int, int]]:
    """Return ``(lower_name, declaration, endstate)`` spans.

    Papyrus does not allow nested states, so each span can be scanned independently
    for member replacements while preserving its declaration and untouched members.
    """
    states: list[tuple[str, int, int]] = []
    i, n = 0, len(lines)
    while i < n:
        state_match = _PAPYRUS_STATE_START.match(lines[i])
        if state_match is None:
            i += 1
            continue
        end = i + 1
        while end < n and not _PAPYRUS_STATE_END.match(lines[end]):
            end += 1
        if end >= n:
            raise ValueError(f"unterminated Papyrus state {state_match.group('name')}")
        states.append((state_match.group("name").lower(), i, end))
        i = end + 1
    return states


def _iter_papyrus_members_in_state(
    lines: list[str], state_start: int, state_end: int
) -> list[tuple[str, str, int, int]]:
    """Return direct Function/Event members within one state span."""
    members: list[tuple[str, str, int, int]] = []
    i = state_start + 1
    while i < state_end:
        member_match = _PAPYRUS_MEMBER_START.match(lines[i])
        if member_match is None:
            i += 1
            continue
        end = i + 1
        while end < state_end and not _PAPYRUS_MEMBER_END.match(lines[end]):
            end += 1
        if end >= state_end:
            raise ValueError(
                f"unterminated Papyrus {member_match.group('kind')} "
                f"{member_match.group('name')}"
            )
        members.append(
            (
                member_match.group("kind").lower(),
                member_match.group("name").lower(),
                i,
                end,
            )
        )
        i = end + 1
    return members


def _state_rename_directives(patch_lines: list[str]) -> list[tuple[str, str]]:
    directives: list[tuple[str, str]] = []
    for line in patch_lines:
        match = _PAPYRUS_STATE_RENAME.match(line)
        if match is not None:
            directives.append((match.group("old"), match.group("new")))
    return directives


def _apply_papyrus_state_renames(
    skeleton_lines: list[str], directives: list[tuple[str, str]]
) -> dict[str, str]:
    """Rename declared states and their exact literal ``GoToState`` targets."""
    rename_map: dict[str, str] = {}
    for old, new in directives:
        old_key = old.lower()
        new_key = new.lower()
        if old_key in rename_map:
            raise ValueError(f"duplicate Papyrus state rename for {old}")

        states = _iter_papyrus_states(skeleton_lines)
        old_states = [span for span in states if span[0] == old_key]
        if len(old_states) != 1:
            raise ValueError(
                f"Papyrus state rename requires exactly one {old!r} state; "
                f"found {len(old_states)}"
            )
        if new_key != old_key and any(name == new_key for name, _start, _end in states):
            raise ValueError(f"Papyrus state rename target already exists: {new}")

        declaration = old_states[0][1]
        state_match = _PAPYRUS_STATE_START.match(skeleton_lines[declaration])
        assert state_match is not None
        skeleton_lines[declaration] = (
            skeleton_lines[declaration][: state_match.start("name")]
            + new
            + skeleton_lines[declaration][state_match.end("name") :]
        )

        goto_pattern = re.compile(
            rf'(?P<prefix>\b(?:Self\.)?GoToState\s*\(\s*")'
            rf'{re.escape(old)}(?P<suffix>"\s*\))',
            re.IGNORECASE,
        )
        for index, line in enumerate(skeleton_lines):
            skeleton_lines[index] = goto_pattern.sub(
                lambda match: match.group("prefix") + new + match.group("suffix"),
                line,
            )
        rename_map[old_key] = new_key
    return rename_map


def _merge_named_state_method_patches(
    skeleton_lines: list[str],
    patch_lines: list[str],
    rename_map: Mapping[str, str],
) -> None:
    """Replace or append members inside named states without copying declarations."""
    seen_patch_states: set[str] = set()
    for patch_state_name, patch_start, patch_end in _iter_papyrus_states(patch_lines):
        target_name = rename_map.get(patch_state_name, patch_state_name)
        if target_name in seen_patch_states:
            raise ValueError(f"duplicate Papyrus state patch for {target_name}")
        seen_patch_states.add(target_name)

        patch_members = _iter_papyrus_members_in_state(
            patch_lines, patch_start, patch_end
        )
        if not patch_members:
            continue

        target_states = [
            span for span in _iter_papyrus_states(skeleton_lines) if span[0] == target_name
        ]
        if len(target_states) != 1:
            raise ValueError(
                f"Papyrus named-state patch requires exactly one {target_name!r} "
                f"state; found {len(target_states)}"
            )

        blocks: dict[tuple[str, str], list[str]] = {}
        for kind, name, start, end in patch_members:
            key = (kind, name)
            if key in blocks:
                raise ValueError(f"duplicate Papyrus state member patch: {kind} {name}")
            blocks[key] = patch_lines[start : end + 1]

        _state_name, target_start, target_end = target_states[0]
        replacements: list[tuple[int, int, list[str]]] = []
        matched: set[tuple[str, str]] = set()
        for kind, name, start, end in _iter_papyrus_members_in_state(
            skeleton_lines, target_start, target_end
        ):
            key = (kind, name)
            if key in blocks:
                replacements.append((start, end, blocks[key]))
                matched.add(key)
        for start, end, block in sorted(replacements, reverse=True):
            skeleton_lines[start : end + 1] = block

        target_states = [
            span for span in _iter_papyrus_states(skeleton_lines) if span[0] == target_name
        ]
        insert_at = target_states[0][2]
        additions: list[str] = []
        for key, block in blocks.items():
            if key not in matched:
                if additions:
                    additions.append("")
                additions.extend(block)
        if additions:
            skeleton_lines[insert_at:insert_at] = additions + [""]


def _merge_script_method_patches(skeleton: str, patch: str) -> str:
    """Splice method and explicit state edits into a decompiled skeleton.

    Top-level behavior remains backward compatible. Members inside patch ``State``
    blocks replace or append only inside the matching skeleton state. Explicit
    ``; @state-rename old new`` directives rename one declaration and exact
    ``GoToState`` string targets. Script/property/state declarations otherwise come
    exclusively from the skeleton.
    """
    patch_lines = patch.splitlines()
    patch_members = _iter_top_level_papyrus_members(patch_lines)
    patch_states = _iter_papyrus_states(patch_lines)
    rename_directives = _state_rename_directives(patch_lines)
    if not patch_members and not patch_states and not rename_directives:
        return skeleton

    skel_lines = skeleton.splitlines()
    rename_map = _apply_papyrus_state_renames(skel_lines, rename_directives)
    _merge_named_state_method_patches(skel_lines, patch_lines, rename_map)

    blocks: dict[tuple[str, str], str] = {}
    for kind, name, start, end in patch_members:
        blocks[(kind, name)] = "\n".join(patch_lines[start : end + 1])

    to_remove = [
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(skel_lines)
        if (kind, name) in blocks
    ]
    for start, end in sorted(to_remove, reverse=True):
        del skel_lines[start : end + 1]

    merged = "\n".join(skel_lines).rstrip()
    if blocks:
        injected = "\n\n".join(blocks.values())
        return f"{merged}\n\n{injected}\n"
    return f"{merged}\n"


def _native_papyrus_diagnostics_message(diagnostics: Iterable[object]) -> str:
    messages: list[str] = []
    for diagnostic in diagnostics:
        if isinstance(diagnostic, Mapping):
            message = str(diagnostic.get("message", diagnostic))
            line = diagnostic.get("line")
            col = diagnostic.get("col")
            if line is not None and col is not None:
                message = f"{line}:{col}: {message}"
            messages.append(message)
        else:
            messages.append(str(diagnostic))
    return "; ".join(messages[:3]) if messages else "native compiler returned no output"


def _read_zstring(data: bytes | bytearray) -> str:
    raw = bytes(data)
    raw = raw.split(b"\x00", 1)[0]
    return raw.decode("cp1252", errors="replace").strip()


def _ctda_function_id(data: bytes | bytearray) -> int | None:
    raw = bytes(data)
    if len(raw) < 12:
        return None
    return int.from_bytes(raw[8:12], byteorder="little", signed=False)


def _ctda_param1_form_id(data: bytes | bytearray) -> int | None:
    raw = bytes(data)
    if len(raw) < 16:
        return None
    return int.from_bytes(raw[12:16], byteorder="little", signed=False)


def _condition_script_data_at(
    subrecords: list[Any],
    index: int,
) -> tuple[str | None, str | None, int, int | None] | None:
    sub = subrecords[index]
    if getattr(sub, "signature", "") != "CTDA":
        return None
    data = getattr(sub, "data", b"")
    function_id = _ctda_function_id(data)
    if function_id not in _SCRIPT_CONDITION_FUNCTION_IDS:
        return None

    script_name: str | None = None
    variable_name: str | None = None
    cursor = index + 1
    while cursor < len(subrecords):
        sig = getattr(subrecords[cursor], "signature", "")
        if sig == "CIS1":
            script_name = _read_zstring(getattr(subrecords[cursor], "data", b""))
            cursor += 1
            continue
        if sig == "CIS2":
            variable_name = _read_zstring(getattr(subrecords[cursor], "data", b""))
            cursor += 1
            continue
        break
    return script_name, variable_name, cursor, _ctda_param1_form_id(data)


def _form_id_lookup_keys(form_id: int | None) -> tuple[int, ...]:
    if form_id is None:
        return ()
    raw = int(form_id) & 0xFFFFFFFF
    object_id = raw & 0x00FFFFFF
    if raw == object_id:
        return (raw,)
    return (raw, object_id)


def _condition_script_names(
    script_name: str | None,
    target_form_id: int | None,
    scripts_by_form_id: Mapping[int, list[str]] | None,
) -> list[str]:
    if script_name:
        return [script_name]
    if scripts_by_form_id is None:
        return []
    for key in _form_id_lookup_keys(target_form_id):
        scripts = scripts_by_form_id.get(key)
        if scripts:
            return scripts
    return []


def _condition_script_ref_at(
    subrecords: list[Any],
    index: int,
) -> tuple[str, str | None, int] | None:
    found = _condition_script_data_at(subrecords, index)
    if found is None:
        return None
    script_name, variable_name, cursor, _target_form_id = found
    if script_name is None:
        return None
    return script_name, variable_name, cursor


def _iter_condition_script_refs(
    record: Any,
    *,
    form_key: str,
    scripts_by_form_id: Mapping[int, list[str]] | None = None,
) -> list[_ScriptReference]:
    refs: list[_ScriptReference] = []
    subrecords = list(getattr(record, "subrecords", []) or [])
    editor_id = str(getattr(record, "editor_id", None) or "")
    record_sig = str(getattr(record, "signature", ""))
    form_id = int(getattr(record, "form_id", 0)) & 0xFFFFFFFF
    for index, sub in enumerate(subrecords):
        if getattr(sub, "signature", "") != "CTDA":
            continue
        found = _condition_script_data_at(subrecords, index)
        if found is None:
            continue
        script_name, variable_name, _cursor, target_form_id = found
        condition_inferred = script_name is None
        for resolved_script_name in _condition_script_names(
            script_name,
            target_form_id,
            scripts_by_form_id,
        ):
            refs.append(
                _ScriptReference(
                    script_name=resolved_script_name,
                    variable_name=variable_name,
                    form_key=form_key,
                    form_id=form_id,
                    record_sig=record_sig,
                    editor_id=editor_id,
                    kind="condition",
                    condition_inferred=condition_inferred,
                )
            )
    return refs


def _iter_script_names(value: Any) -> Iterable[str]:
    if isinstance(value, Mapping):
        for key, item in value.items():
            if key == "ScriptName" and isinstance(item, str) and item.strip():
                yield item.strip()
                continue
            yield from _iter_script_names(item)
        return
    if isinstance(value, list):
        for item in value:
            yield from _iter_script_names(item)


def _iter_vmad_script_refs(
    record_payload: Mapping[str, Any],
    record: Any,
    *,
    form_key: str,
) -> list[_ScriptReference]:
    refs: list[_ScriptReference] = []
    fields = record_payload.get("fields")
    if not isinstance(fields, list):
        return refs
    editor_id = str(getattr(record, "editor_id", None) or "")
    record_sig = str(getattr(record, "signature", ""))
    form_id = int(getattr(record, "form_id", 0)) & 0xFFFFFFFF
    seen: set[str] = set()
    for field in fields:
        if not isinstance(field, Mapping):
            continue
        vmad = field.get("VirtualMachineAdapter")
        if not isinstance(vmad, Mapping):
            continue
        for script_name in _iter_script_names(vmad):
            key = _script_key(script_name)
            if key in seen:
                continue
            seen.add(key)
            refs.append(
                _ScriptReference(
                    script_name=script_name,
                    variable_name=None,
                    form_key=form_key,
                    form_id=form_id,
                    record_sig=record_sig,
                    editor_id=editor_id,
                    kind="vmad",
                )
            )
    return refs


def _record_from_native_subrecords(
    *,
    form_id: int,
    signature: str,
    editor_id: str | None,
    subrecords: list[tuple[str, bytes, str | None]],
) -> Any:
    from creation_lib.esp.model import Record, Subrecord

    record = Record(
        signature=str(signature),
        form_id=int(form_id) & 0xFFFFFFFF,
        subrecords=[
            Subrecord(str(sig), bytes(data), semantic_type)
            for sig, data, semantic_type in subrecords
        ],
    )
    setattr(record, "editor_id", editor_id or "")
    return record


def _script_ref_form_key(
    form_id: int,
    *,
    plugin_name: str,
) -> str:
    object_id = int(form_id) & 0x00FFFFFF
    if plugin_name:
        return f"{object_id:06X}:{plugin_name}"
    return f"{int(form_id) & 0xFFFFFFFF:08X}"


def _subrecord_payloads_after_script_strip(
    record: Any,
    *,
    failed_script_keys: set[str],
    invalid_condition_keys: set[tuple[str, str]],
    strip_vmad: bool,
    scripts_by_form_id: Mapping[int, list[str]] | None = None,
) -> tuple[list[dict[str, bytes]], int, int]:
    subrecords = list(getattr(record, "subrecords", []) or [])
    payloads: list[dict[str, bytes]] = []
    stripped_conditions = 0
    stripped_vmad = 0
    index = 0
    while index < len(subrecords):
        sub = subrecords[index]
        sig = str(getattr(sub, "signature", ""))
        if strip_vmad and sig == "VMAD":
            stripped_vmad += 1
            index += 1
            continue
        if sig == "CTDA":
            found = _condition_script_data_at(subrecords, index)
            if found is not None:
                script_name, variable_name, cursor, target_form_id = found
                resolved_script_names = _condition_script_names(
                    script_name,
                    target_form_id,
                    scripts_by_form_id,
                )
                unresolved_script_names = []
                for resolved_script_name in resolved_script_names:
                    script_key = _script_key(resolved_script_name)
                    condition_key = (script_key, _variable_key(variable_name))
                    if (
                        script_key in failed_script_keys
                        or condition_key in invalid_condition_keys
                    ):
                        unresolved_script_names.append(resolved_script_name)
                should_strip = bool(unresolved_script_names) and (
                    script_name is not None
                    or len(unresolved_script_names) == len(resolved_script_names)
                )
                if should_strip:
                    stripped_conditions += 1
                    index = cursor
                    continue
        payloads.append({"signature": sig, "data": bytes(getattr(sub, "data", b""))})
        index += 1
    if stripped_conditions:
        _sync_single_citc_condition_count(payloads)
    return payloads, stripped_conditions, stripped_vmad


def _sync_single_citc_condition_count(payloads: list[dict[str, bytes]]) -> None:
    citc_indexes = [
        index for index, item in enumerate(payloads) if item["signature"] == "CITC"
    ]
    if len(citc_indexes) != 1:
        return
    data = bytearray(payloads[citc_indexes[0]]["data"])
    if len(data) < 4:
        return
    condition_count = sum(1 for item in payloads if item["signature"] == "CTDA")
    data[0:4] = int(condition_count).to_bytes(4, byteorder="little", signed=False)
    payloads[citc_indexes[0]]["data"] = bytes(data)


def _dedupe_paths(paths: list[Path]) -> list[Path]:
    seen: set[str] = set()
    out: list[Path] = []
    for path in paths:
        key = str(path).replace("\\", "/").lower()
        if key in seen:
            continue
        seen.add(key)
        out.append(path)
    return out


def _source_script_roots(
    source_data_dir: Path | None,
    additional_source_asset_roots: tuple[Path, ...] = (),
) -> list[Path]:
    asset_roots = [
        Path(root)
        for root in (source_data_dir, *additional_source_asset_roots)
        if root is not None
    ]
    return _dedupe_paths(
        [
            candidate
            for root in asset_roots
            for candidate in (
                root / "scripts" / "client",
                root / "Scripts" / "Client",
                root / "Scripts",
                root / "Data" / "Scripts",
            )
        ]
    )


def _target_script_roots(
    *,
    target_data_dir: Path | None,
    target_extracted_dir: Path | None,
) -> list[Path]:
    roots: list[Path] = []
    if target_data_dir is not None:
        roots.append(target_data_dir / "Scripts")
    if target_extracted_dir is not None:
        roots.extend(
            [target_extracted_dir / "Scripts", target_extracted_dir / "scripts"]
        )
    return _dedupe_paths(roots)


def _script_name_from_relative_pex(path: Path, root: Path) -> str:
    rel = path.relative_to(root).with_suffix("")
    parts = list(rel.parts)
    if parts and parts[0].lower() in {"client", "server"}:
        parts = parts[1:]
    return ":".join(parts)


def _build_pex_index(roots: list[Path]) -> dict[str, Path]:
    index: dict[str, Path] = {}
    for root in roots:
        if not root.is_dir():
            continue
        for path in root.rglob("*.pex"):
            key = _script_key(_script_name_from_relative_pex(path, root))
            index.setdefault(key, path)
    return index


def _build_target_pex_index(
    *,
    target_data_dir: Path | None,
    target_extracted_dir: Path | None,
    target_asset_store: object | None,
) -> dict[str, Path | None]:
    roots = _target_script_roots(
        target_data_dir=target_data_dir,
        target_extracted_dir=target_extracted_dir,
    )
    index: dict[str, Path | None] = dict(_build_pex_index(roots))
    if target_asset_store is None:
        return index
    for asset_path in target_asset_store.list_assets(
        prefix="scripts/", suffix=".pex"
    ):
        script_name = _script_name_from_relative_pex(
            Path(asset_path), Path("scripts")
        )
        index.setdefault(_script_key(script_name), None)
    return index


def _script_name_from_indexed_pex(path: Path, roots: list[Path]) -> str:
    for root in roots:
        try:
            path.relative_to(root)
        except ValueError:
            continue
        return _script_name_from_relative_pex(path, root)
    return path.with_suffix("").name


def _include_all_source_scripts(source_game: str, target_game: str) -> bool:
    return source_game.lower() == "fo76" and target_game.lower() == "fo4"


_FO76_TO_FO4_SCRIPT_TYPE_ALIASES = {
    "player": "Actor",
    "questinstance": "Quest",
    "region": "Form",
}

_FO76_TO_FO4_SOURCE_SCRIPT_SKIP_KEYS = {
    "player",
    "creatures:mirelurkqueenracescript",
    "creatures:radscorpionracescript",
    "creatures:smbehemothracescript",
}


def _fo76_to_fo4_script_type(type_name: str) -> str:
    suffix = ""
    base = str(type_name or "")
    while base.endswith("[]"):
        suffix += "[]"
        base = base[:-2]
    return f"{_FO76_TO_FO4_SCRIPT_TYPE_ALIASES.get(base.lower(), base)}{suffix}"


def _skip_fo76_to_fo4_source_script(
    script_key: str,
    *,
    source_game: str,
    target_game: str,
) -> bool:
    return (
        source_game.lower() == "fo76"
        and target_game.lower() == "fo4"
        and _script_key(script_key) in _FO76_TO_FO4_SOURCE_SCRIPT_SKIP_KEYS
    )


def _script_parent_name(pex_path: Path) -> str | None:
    """Return the immediate ``Extends`` parent declared on the compiled script at
    `pex_path`, or None if it has none (a native root) or the PEX can't be parsed."""
    from creation_lib.pex import parse_pex

    try:
        pex = parse_pex(pex_path)
    except Exception:
        return None
    if not pex.objects:
        return None
    parent = getattr(pex.objects[0], "parent", None)
    return str(parent) if parent else None


def _extend_script_names_with_ancestor_closure(
    script_names_by_key: dict[str, str],
    *,
    source_index: dict[str, Path],
    target_index: dict[str, Path | None],
    runner: "ConversionRunner",
) -> None:
    """Walk the `Extends` chain of every script already in `script_names_by_key`
    and add record-unbound ancestors so children that extend them can compile.

    An ancestor already resolvable in `target_index` is FO4-native (or already
    shadowed) and is left alone -- never emitted. An ancestor found in
    `source_index` is added and its own ancestors are queued in turn. An ancestor
    with no PEX in either index is logged and left unemitted.
    """
    queue: list[str] = list(script_names_by_key.values())
    warned_missing: set[str] = set()
    while queue:
        script_name = queue.pop()
        pex_path = source_index.get(_script_key(script_name)) or target_index.get(
            _script_key(script_name)
        )
        if pex_path is None:
            continue
        parent_name = _script_parent_name(pex_path)
        if not parent_name:
            continue
        parent_key = _script_key(parent_name)
        if parent_key in target_index or parent_key in script_names_by_key:
            continue
        parent_pex = source_index.get(parent_key)
        if parent_pex is None:
            if parent_key not in warned_missing:
                warned_missing.add(parent_key)
                runner.emit_log(
                    "WARN",
                    f"[Scripts] {script_name} extends {parent_name}, which has no "
                    "PEX in the source tree; leaving unemitted",
                )
            continue
        script_names_by_key[parent_key] = parent_name
        queue.append(parent_name)


def _pex_member_names(path: Path) -> set[str]:
    from creation_lib.pex import parse_pex

    pex = parse_pex(path)
    names: set[str] = set()
    for obj in pex.objects:
        for variable in obj.variables:
            names.add(str(variable.name).lower())
        for prop in obj.properties:
            names.add(str(prop.name).lower())
            if prop.auto_var:
                names.add(str(prop.auto_var).lower())
    return names


def _script_has_variable(path: Path, variable_name: str | None) -> bool:
    if not variable_name:
        return True
    return _variable_key(variable_name) in _pex_member_names(path)


def _script_worker_count(requested_workers: int | None, script_count: int) -> int:
    if script_count <= 1:
        return 1
    if requested_workers is None:
        return 1
    return max(1, min(int(requested_workers), script_count))


def _remove_generated_script_outputs(mod_path: Path, script_name: str) -> list[str]:
    errors: list[str] = []
    for root, suffix in (
        (mod_path / "Scripts" / "Source" / "User", ".psc"),
        (mod_path / "data" / "Scripts", ".pex"),
    ):
        path = root / _script_relative_path(script_name, suffix)
        if not path.is_file():
            continue
        try:
            path.unlink()
        except OSError as exc:
            errors.append(f"{path}: {exc}")
    return errors


def _skip_record_signatures_payload(
    *, convert_placed_records: bool, exclude_signatures: "frozenset[str]"
) -> list[str]:
    base: set[str] = set()
    if not convert_placed_records:
        base.update(_PLACED_RECORD_SIGNATURES)
    base.update(s.upper() for s in (exclude_signatures or frozenset()))
    return sorted(base)


def _path_before_error(text: str) -> str:
    return text.split(" -> ", 1)[0]


def _is_bgsm_path(path: str) -> bool:
    return ".bgsm" in path.lower()


class _UnifiedRecordRuntime:
    def __init__(self, request: PluginPortRequest) -> None:
        self._req = request
        self._registry = ConvertedPluginRegistry()
        self._aggregate_summary = ConversionSummary(mod_path=str(request.output_root))
        self._run_result = RunResult()

    @property

    def run_result(self) -> RunResult:
        return self._run_result


    def _topo_sort(self, plugins: list[Path], runner: "ConversionRunner") -> list[Path]:
        canonical_names = {path: self._plugin_name(path) for path in plugins}
        path_by_name = {name: path for path, name in canonical_names.items()}
        alias_to_name: dict[str, str] = {}
        for path, name in canonical_names.items():
            for alias in self._source_name_candidates(name):
                alias_to_name[alias] = name

        masters_by_name = {
            canonical_names[path]: [
                alias_to_name.get(master, master)
                for master in self._read_master_chain(path)
            ]
            for path in plugins
        }

        for plugin_name, masters in masters_by_name.items():
            for master in masters:
                if master not in path_by_name and not self._is_target_master(master):
                    runner.emit_log(
                        "WARN",
                        f"{plugin_name} requires master {master} which is not selected; "
                        "cross-plugin references may fall back to vanilla-by-EID lookup",
                    )

        ordered: list[str] = []
        visiting: set[str] = set()
        visited: set[str] = set()

        def visit(name: str) -> None:
            if name in visited or name in visiting:
                return
            visiting.add(name)
            for master in masters_by_name.get(name, []):
                if master in path_by_name:
                    visit(master)
            visiting.remove(name)
            visited.add(name)
            ordered.append(name)

        for path in plugins:
            visit(canonical_names[path])
        return [path_by_name[name] for name in ordered]


    def _read_master_chain(self, source_plugin: Path) -> list[str]:
        if source_plugin.is_file():
            try:
                plugin = Plugin.load(source_plugin, game=self._req.source_game)
            except Exception as exc:
                _log.debug(
                    "Could not read native master chain for %s: %s", source_plugin, exc
                )
            else:
                try:
                    return list(plugin.header.masters)
                finally:
                    plugin.close()
        return []

    @staticmethod

    def _is_target_master(name: str) -> bool:
        lowered = name.lower()
        return (
            lowered == "fallout4.esm"
            or lowered.startswith("dlc")
            or lowered.startswith("cc")
        )

    @staticmethod

    def _source_name_candidates(plugin_name: str) -> list[str]:
        path = Path(plugin_name)
        candidates = [plugin_name]
        if path.suffix:
            candidates.append(path.stem)
        else:
            candidates.extend([f"{plugin_name}.esm", f"{plugin_name}.esp"])

        seen: set[str] = set()
        result: list[str] = []
        for candidate in candidates:
            key = candidate.lower()
            if key in seen:
                continue
            seen.add(key)
            result.append(candidate)
        return result


    def _merge_addon_index_map(
        self, ctx: ConversionContext, decisions: list
    ) -> None:
        """Fold `addon_node_index_remap` decisions (message ``"old->new"``)
        emitted by the ESP ADDN fixup into ``ctx.addon_index_map``. The NIF
        phase consumes this map to repoint `BSValueNode` addon-node blocks to
        their reconciled indices.
        """
        if not decisions:
            return
        current = dict(getattr(ctx, "addon_index_map", {}) or {})
        for decision in decisions:
            try:
                if decision.get("kind") != "addon_node_index_remap":
                    continue
                old_str, new_str = str(decision.get("message", "")).split("->", 1)
                current[int(old_str)] = int(new_str)
            except (ValueError, AttributeError):
                continue
        if current:
            ctx.addon_index_map = current


    def _emit_run_warnings_inline(
        self,
        run,
        runner: "ConversionRunner",
        ctx: ConversionContext,
    ) -> None:
        """Drain decisions + warnings from the run and emit them via the runner
        so they print inline as phases complete.
        """
        try:
            decisions = run.drain_decisions()
            warnings = run.drain_warnings()
        except Exception:
            return
        if decisions:
            existing = list(getattr(ctx, "conversion_decisions", []) or [])
            existing.extend(decisions)
            ctx.conversion_decisions = existing
            self._merge_addon_index_map(ctx, decisions)
        for w in warnings:
            runner.emit_log("WARN", w)
        if warnings:
            ctx.summary.records_warnings += len(warnings)


    def _drain_and_drop_rust_run(self, ctx: ConversionContext) -> None:
        """Drain decisions/warnings from a stashed Rust ConversionRun, then
        drop it. Idempotent — safe to call when no run was created.
        """
        run = getattr(ctx, "_rust_conversion_run", None)
        if run is None:
            return
        drainer = getattr(ctx, "_event_drainer", None)
        if drainer is not None:
            drainer.stop()
            ctx._event_drainer = None  # type: ignore[attr-defined]
        try:
            decisions = run.drain_decisions()
            warnings = run.drain_warnings()
        except Exception:
            decisions = []
            warnings = []
        if decisions:
            existing = list(getattr(ctx, "conversion_decisions", []) or [])
            existing.extend(decisions)
            ctx.conversion_decisions = existing
            self._merge_addon_index_map(ctx, decisions)
        if warnings:
            existing_logs = list(getattr(ctx, "log_lines", []) or [])
            existing_logs.extend(f"[WARN] {w}" for w in warnings)
            ctx.log_lines = existing_logs
            ctx.summary.records_warnings += len(warnings)
        try:
            run.close()
        except Exception:
            pass
        ctx._rust_conversion_run = None  # type: ignore[attr-defined]
        try:
            native_runtime.trim_allocator()
        except Exception:
            pass

    @staticmethod

    def _clean_stale_authoring_for_direct_esp(mod_path: Path) -> None:
        yaml_dir = mod_path / "yaml"
        records_dir = yaml_dir / "records"
        plugin_yaml = yaml_dir / "plugin.yaml"
        if records_dir.exists():
            shutil.rmtree(records_dir)
        if plugin_yaml.exists():
            plugin_yaml.unlink()
        yaml_dir.mkdir(parents=True, exist_ok=True)


    def _emit_authoring_yaml_for_build(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> bool:
        emit_authoring_yaml = bool(getattr(ctx, "emit_authoring_yaml", True))
        if emit_authoring_yaml and bool(getattr(ctx, "is_whole_plugin", False)):
            runner.emit_log(
                "INFO",
                "whole-plugin build: skipping authoring YAML export",
            )
            return False
        return emit_authoring_yaml


    def _validate_output_plugin(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> None:
        output_path = ctx.mod_path / ctx.output_plugin_name
        if not output_path.is_file():
            raise FileNotFoundError(
                f"missing output plugin for validation: {output_path}"
            )

        from creation_lib.esp.editor import EditorSession, validate

        started_at = time.perf_counter()
        runner.emit_log("INFO", f"Validating output plugin {ctx.output_plugin_name}")
        session = EditorSession(
            default_game=self._req.target_game,
            auto_scan_conflicts=False,
            master_search_paths=self._validation_master_search_paths(output_path),
        )
        try:
            loaded = session.load(output_path, game=self._req.target_game)
            issues = list(validate(session, handle=loaded.handle))
        finally:
            session.close_all()

        errors = [
            issue for issue in issues if _validation_issue_severity(issue) == "error"
        ]
        warnings = [
            issue for issue in issues if _validation_issue_severity(issue) == "warning"
        ]
        ctx.summary.validation_errors += len(errors)
        ctx.summary.validation_warnings += len(warnings)
        runner.emit_log(
            "INFO",
            f"Validation finished for {ctx.output_plugin_name}: "
            f"errors={len(errors)} warnings={len(warnings)} "
            f"elapsed={_format_elapsed_seconds(_elapsed_seconds(started_at))}",
        )
        for issue in errors[:25]:
            runner.emit_log("ERROR", f"validation: {_validation_issue_text(issue)}")
        if len(errors) > 25:
            runner.emit_log(
                "ERROR", f"validation: additional errors omitted: {len(errors) - 25}"
            )

        if errors and self._req.options.validation_fail_on_error:
            raise RuntimeError(
                f"output validation failed for {ctx.output_plugin_name}: "
                f"{len(errors)} error(s), {len(warnings)} warning(s)"
            )


    def _validation_master_search_paths(self, output_path: Path) -> list[Path]:
        paths: list[Path] = []
        seen: set[str] = set()

        def add(path: Path | None) -> None:
            if path is None:
                return
            candidate = (
                path.parent
                if path.is_file() or path.suffix.lower() in {".esm", ".esp", ".esl"}
                else path
            )
            try:
                key = str(candidate.resolve()).casefold()
            except OSError:
                key = str(candidate).casefold()
            if key in seen:
                return
            seen.add(key)
            paths.append(candidate)

        add(output_path.parent)
        for path in self._req.target_master_paths:
            add(path)
        add(self._req.target_data_dir)
        return paths


    def _build_context(
        self,
        source_plugin: Path,
        plugin_name: str,
        mod_path: Path,
        runner: "ConversionRunner | None" = None,
    ) -> ConversionContext:
        timing_report = getattr(self._req, "timing_report", None)
        target_master_paths = _conversion_target_master_inputs(self._req)

        def record_setup_timing(
            name: str,
            started_at: float,
            **fields: object,
        ) -> None:
            if isinstance(timing_report, TimingReport):
                timing_report.record(
                    name,
                    time.perf_counter() - started_at,
                    plugin=plugin_name,
                    **fields,
                )

        material_overrides = load_translation_map_overrides(
            self._req.source_game, self._req.target_game
        )
        target_asset_store = None
        if self._req.target_game == "fo4" and self._req.target_data_dir is not None:
            target_asset_store = build_target_asset_store(
                target_data_dir=self._req.target_data_dir,
                catalog_path=self._req.target_asset_catalog_path,
                cache_dir=self._req.target_asset_cache_dir,
                overlay_dir=self._req.target_extracted_dir,
            )
        target_asset_index = None
        if self._has_asset_phases(self._req.options):
            asset_roots = [self._req.target_extracted_dir]
            asset_index_workers = max(
                1,
                int(getattr(self._req.options, "conversion_workers", 1) or 1),
            )
            asset_started = time.perf_counter()
            _safe_emit_log(
                runner,
                "INFO",
                "Indexing target asset catalog/overlay: "
                + ", ".join(str(root) for root in asset_roots if root is not None)
                + f" (workers={asset_index_workers})",
            )
            target_asset_index = build_target_asset_index(
                asset_roots,
                workers=asset_index_workers,
                store=target_asset_store,
            )
            asset_count = int(
                getattr(target_asset_index, "asset_count", len(target_asset_index.keys))
            )
            files_scanned = int(getattr(target_asset_index, "files_scanned", 0))
            warning_count = len(target_asset_index.warnings)
            elapsed = _elapsed_seconds(asset_started)
            _safe_emit_log(
                runner,
                "INFO",
                f"Indexed target asset preflight: files={files_scanned} "
                f"assets={asset_count} "
                f"warnings={warning_count} elapsed={_format_elapsed_seconds(elapsed)}",
            )
            record_setup_timing(
                "setup_target_asset_preflight",
                asset_started,
                files=files_scanned,
                assets=asset_count,
                warnings=warning_count,
                workers=asset_index_workers,
            )

        # The native ConversionRun is the only long-lived owner of master
        # parse trees. Python passes no open handles: the record preflight
        # opens transient index-only handles internally and closes them.
        target_loader = None
        target_master_handles: list = []
        try:
            preflight_rows = []
            preflight_master_names = []
            preflight_missing_masters = []
            preflight_warnings = []
            if (
                self._req.options.translate_records
                and self._req.options.cell_bounds is None
            ):
                preflight_started = time.perf_counter()
                _safe_emit_log(
                    runner,
                    "INFO",
                    "Building target record preflight from target masters",
                )
                record_preflight = build_target_record_preflight(
                    self._req.target_game,
                    target_master_paths=target_master_paths,
                    target_data_dir=self._req.target_data_dir,
                    target_extracted_dir=None,
                    target_master_handles=target_master_handles,
                )
                preflight_rows = record_preflight.rows_for_native()
                preflight_master_names = list(record_preflight.master_names)
                preflight_missing_masters = list(record_preflight.missing_masters)
                preflight_warnings = list(record_preflight.warnings)
                _safe_emit_log(
                    runner,
                    "INFO",
                    f"Built target record preflight: rows={len(preflight_rows)} "
                    f"masters={len(preflight_master_names)} "
                    f"missing={len(preflight_missing_masters)} "
                    f"warnings={len(preflight_warnings)} "
                    f"elapsed={_format_elapsed_seconds(_elapsed_seconds(preflight_started))}",
                )
                record_setup_timing(
                    "setup_target_record_preflight",
                    preflight_started,
                    rows=len(preflight_rows),
                    masters=len(preflight_master_names),
                    missing=len(preflight_missing_masters),
                    warnings=len(preflight_warnings),
                )

            formkey_mapper = FormKeyMapper(
                mod_name=source_plugin.stem,
                target_game=self._req.target_game,
                target_loader=target_loader,
                target_master_handles=target_master_handles,
                mod_path=str(mod_path),
                use_base_game_assets=True,
                preserve_source_ids=True,
                output_plugin_extension=Path(plugin_name).suffix or ".esp",
            )

            ctx = ConversionContext(
                source_game=self._req.source_game,
                target_game=self._req.target_game,
                mod_path=mod_path,
                output_plugin_name=plugin_name,
                target_extracted_dir=self._req.target_extracted_dir,
                target_data_dir=self._req.target_data_dir,
                source_data_dir=self._req.source_data_dir,
                additional_source_asset_roots=tuple(
                    Path(root)
                    for root in self._req.additional_source_asset_roots
                ),
                formkey_mapper=formkey_mapper,
                fixups=None,
                material_overrides=material_overrides,
                summary=ConversionSummary(mod_path=str(mod_path)),
                target_asset_store=target_asset_store,
                target_asset_catalog_path=(
                    target_asset_store.catalog_path
                    if target_asset_store is not None
                    else self._req.target_asset_catalog_path
                ),
                target_asset_cache_dir=(
                    target_asset_store.cache_dir
                    if target_asset_store is not None
                    else self._req.target_asset_cache_dir
                ),
                converted_plugin_registry=self._registry,
                source_plugin_handle=None,
                source_master_handles=[],
                target_master_handles=target_master_handles,
                terrain_options=self._req.options.terrain,
                is_whole_plugin=True,
                emit_authoring_yaml=self._req.emit_authoring_yaml,
                conversion_workers=self._req.options.conversion_workers,
                disable_nif_collision_memo=self._req.options.disable_nif_collision_memo,
                records_limit=self._req.options.records_limit,
                overwrite_existing=self._req.options.overwrite_existing,
                force_cpu_textures=self._req.options.force_cpu_textures,
                pbr_carry=self._req.options.pbr_carry,
                texture_landscape_mip_flooding=(
                    self._req.options.texture_landscape_mip_flooding
                ),
                convert_precombined_nifs=self._req.options.convert_precombined_nifs,
                base_asset_relocation_mesh_roots=resolve_base_asset_relocation_mesh_roots(
                    self._req.source_game,
                    self._req.target_game,
                    self._req.options.base_asset_relocation_mesh_roots,
                ),
                base_asset_namespace=resolve_base_asset_namespace(
                    self._req.source_game,
                    self._req.target_game,
                    self._req.options.base_asset_namespace,
                ),
                diagnostics_root=self._req.diagnostics_root,
            )
            ctx.target_record_preflight_rows = preflight_rows
            ctx.source_plugin_path = source_plugin
            # The native ConversionRun loads its masters from these paths (and
            # seeds config.target_master_names from them at creation), so this
            # must be the full resolved official + fallback set — the request's
            # explicit paths alone are usually empty.
            target_master_plugin_paths, _ = resolve_target_master_plugin_paths(
                self._req.target_game,
                target_master_paths=target_master_paths,
                target_data_dir=self._req.target_data_dir,
                target_extracted_dir=None,
            )
            ctx.target_master_plugin_paths = target_master_plugin_paths
            ctx.target_record_preflight_master_names = preflight_master_names
            ctx.target_record_preflight_missing_masters = preflight_missing_masters
            ctx.target_record_preflight_warnings = preflight_warnings
            ctx.anim_text_data_base_race_path = next(
                (
                    path
                    for path in target_master_plugin_paths
                    if path.name.casefold() == "fallout4.esm"
                ),
                None,
            )
            ctx.target_asset_index = target_asset_index
            if isinstance(timing_report, TimingReport):
                setattr(ctx, "timing_report", timing_report)
            return ctx
        except Exception:
            close_plugin_handles(target_master_handles)
            raise


    @staticmethod

    def _close_source_handle(ctx: ConversionContext) -> None:
        source_handle = getattr(ctx, "source_plugin_handle", None)
        if source_handle is not None:
            source_handle.close()
            setattr(ctx, "source_plugin_handle", None)

    @staticmethod

    def _close_target_master_handles(ctx: ConversionContext) -> None:
        target_master_handles = getattr(ctx, "target_master_handles", [])
        close_plugin_handles(target_master_handles)
        setattr(ctx, "target_master_handles", [])

    @staticmethod

    def _has_asset_phases(opts) -> bool:
        return (
            opts.convert_nifs
            or opts.convert_textures
            or opts.convert_materials
            or opts.convert_havok
            or opts.convert_animations
            or opts.copy_sounds
        )


    def _collect_assets_native(
        self,
        source_plugin: Path,
        ctx: ConversionContext,
        runner: "ConversionRunner | None" = None,
    ) -> list[AssetRef]:
        from bacup_lib.record.extractors import signatures_for_asset_kind
        from creation_lib.esp.plugin import Plugin

        opts = self._req.options
        enabled_kinds: list[str] = []
        if opts.convert_nifs:
            enabled_kinds.append("nif")
        if opts.convert_textures:
            enabled_kinds.append("texture")
        if opts.convert_materials:
            enabled_kinds.append("material")
        if opts.convert_havok or opts.convert_animations:
            enabled_kinds.append("behavior")
        if opts.copy_sounds:
            enabled_kinds.append("sound")

        signature_sets = [signatures_for_asset_kind(kind) for kind in enabled_kinds]
        signatures = (
            sorted({sig for sigs in signature_sets for sig in sigs})
            if signature_sets and all(signature_sets)
            else None
        )

        source_handle = getattr(ctx, "source_plugin_handle", None)
        close_source_handle = False
        if source_handle is None:
            if not source_plugin.is_file():
                raise ValueError(
                    f"Native asset collection requires a binary plugin file, got {source_plugin}"
                )
            # Lazy compressed load: defer decompressing record bodies until
            # first access rather than eagerly loading the full 8+ GB FO76
            # source into RAM for a single collection pass.
            source_handle = Plugin.load(
                source_plugin,
                game=self._req.source_game,
                eager_compressed=False,
            )
            close_source_handle = True
        raw_started = time.perf_counter()
        _safe_emit_log(
            runner,
            "INFO",
            "Collecting native asset references: "
            f"kinds={','.join(enabled_kinds) if enabled_kinds else 'all'}",
        )
        try:
            raw_assets = source_handle.collect_assets(
                asset_kinds=enabled_kinds or None,
                signatures=signatures,
            )
        finally:
            if close_source_handle:
                source_handle.close()
        _safe_emit_log(
            runner,
            "INFO",
            f"Collected native asset references: raw={len(raw_assets)} "
            f"elapsed={_format_elapsed_seconds(_elapsed_seconds(raw_started))}",
        )
        _record_timing(
            ctx,
            "collect_assets_native_raw",
            raw_started,
            raw=len(raw_assets),
            kinds=",".join(enabled_kinds),
        )

        resolve_started = time.perf_counter()
        assets = [
            self._asset_ref_from_native_item(item, source_plugin, ctx)
            for item in raw_assets
        ]
        if opts.copy_sounds:
            assets = self._augment_fo76_audio_tree_assets(
                assets,
                source_plugin,
                ctx,
                runner,
            )
        _record_timing(
            ctx,
            "collect_assets_resolve_paths",
            resolve_started,
            assets=len(assets),
        )
        if opts.convert_nifs:
            assets = self._augment_full_plugin_nif_inventory(
                assets,
                source_plugin,
                ctx,
                runner,
            )
            assets = self._augment_character_asset_companion_nifs(
                assets,
                runner,
            )
        if getattr(opts, "synthesize_object_lod", False):
            assets = self._augment_lod_convention_assets(assets, ctx, runner)
        if opts.convert_havok or opts.convert_animations:
            assets = self._augment_havok_behavior_bundles(
                assets,
                source_plugin,
                ctx,
                runner,
            )
        assets = self._augment_fo76_pipboy_map_asset(assets, source_plugin, ctx, runner)
        return assets

    def _augment_fo76_audio_tree_assets(
        self,
        assets: list[AssetRef],
        source_plugin: Path,
        ctx: ConversionContext,
        runner: "ConversionRunner | None" = None,
    ) -> list[AssetRef]:
        if (
            self._req.source_game.lower(),
            self._req.target_game.lower(),
        ) != ("fo76", "fo4"):
            return assets

        output_plugin_name = Path(
            str(getattr(ctx, "output_plugin_name", "") or source_plugin.name)
        ).name
        if not output_plugin_name:
            return assets

        existing = {
            (asset.asset_type, self._sound_asset_copy_key(asset.source_path))
            for asset in assets
        }
        expanded = list(assets)

        voice_added = self._append_fo76_audio_tree_assets(
            expanded,
            existing,
            roots=self._source_audio_roots(
                source_plugin,
                ctx,
                ("Sound", "Voice", source_plugin.name),
            ),
            output_prefix=f"Sound/Voice/{output_plugin_name}",
            extensions={".fuz", ".wav", ".xwm"},
            walker_pass="voice_asset_tree",
            added_by_record_sig="INFO",
        )
        if voice_added:
            _safe_emit_log(
                runner,
                "INFO",
                f"Expanded {voice_added} FO76 voice asset(s) from Sound/Voice/{output_plugin_name}",
            )

        music_added = self._append_fo76_audio_tree_assets(
            expanded,
            existing,
            roots=self._source_audio_roots(source_plugin, ctx, ("Music",)),
            output_prefix="Music",
            extensions={".xwm", ".wav"},
            walker_pass="music_asset_tree",
            added_by_record_sig="MUSC",
        )
        if music_added:
            _safe_emit_log(
                runner,
                "INFO",
                f"Expanded {music_added} FO76 music asset(s) from Music",
            )

        fx_added = self._append_fo76_audio_tree_assets(
            expanded,
            existing,
            roots=self._source_audio_roots(source_plugin, ctx, ("Sound", "FX")),
            output_prefix="Sound/FX",
            extensions={".xwm", ".wav"},
            walker_pass="sound_fx_asset_tree",
            added_by_record_sig="SNDR",
        )
        if fx_added:
            _safe_emit_log(
                runner,
                "INFO",
                f"Expanded {fx_added} FO76 sound FX asset(s) from Sound/FX",
            )
        return expanded

    def _append_fo76_audio_tree_assets(
        self,
        expanded: list[AssetRef],
        existing: set[tuple[str, str]],
        *,
        roots: list[Path],
        output_prefix: str,
        extensions: set[str],
        walker_pass: str,
        added_by_record_sig: str,
    ) -> int:
        added = 0
        for root in roots:
            for source_file in sorted(root.rglob("*")):
                if not source_file.is_file():
                    continue
                if source_file.suffix.lower() not in extensions:
                    continue
                rel = source_file.relative_to(root).as_posix()
                source_path = f"{output_prefix}/{rel}"
                key = ("sound", self._sound_asset_copy_key(source_path))
                if key in existing:
                    continue
                existing.add(key)
                expanded.append(
                    AssetRef(
                        asset_type="sound",
                        source_path=source_path,
                        resolved_path=str(source_file),
                        provenance=AssetProvenance(
                            added_by_record_fk="",
                            added_by_record_eid="",
                            added_by_field="",
                            walk_depth=0,
                            walker_pass=walker_pass,
                            added_by_record_sig=added_by_record_sig,
                        ),
                    )
                )
                added += 1
        return added

    def _source_audio_roots(
        self,
        source_plugin: Path,
        ctx: ConversionContext,
        parts: tuple[str, ...],
    ) -> list[Path]:
        roots: list[Path] = []
        seen: set[str] = set()
        for root in self._native_asset_source_roots(source_plugin, ctx):
            for base in (root, root / "Data"):
                candidate: Path | None = base
                for part in parts:
                    candidate = self._case_insensitive_child(candidate, part)
                    if candidate is None:
                        break
                if candidate is None or not candidate.is_dir():
                    continue
                key = str(candidate.resolve()).replace("\\", "/").lower()
                if key in seen:
                    continue
                seen.add(key)
                roots.append(candidate)
        return roots

    @staticmethod
    def _sound_asset_copy_key(source_path: str) -> str:
        path = source_path.replace("\\", "/").strip().lstrip("/").lower()
        if path.startswith("data/music/"):
            return "music/" + path[11:]
        if path.startswith("music/"):
            return path
        if not path.startswith("sound/"):
            return "sound/" + path
        return path

    @staticmethod
    def _case_insensitive_child(parent: Path | None, name: str) -> Path | None:
        if parent is None or not parent.is_dir():
            return None
        direct = parent / name
        wanted = name.lower()
        for child in parent.iterdir():
            if child.name.lower() == wanted:
                return child
        if direct.exists():
            return direct
        return None

    def _augment_fo76_pipboy_map_asset(
        self,
        assets: list[AssetRef],
        source_plugin: Path,
        ctx: ConversionContext,
        runner: "ConversionRunner | None" = None,
    ) -> list[AssetRef]:
        if (
            self._req.source_game.lower(),
            self._req.target_game.lower(),
        ) != ("fo76", "fo4"):
            return assets
        if not self._req.options.convert_textures:
            return assets

        source_path = "Interface/Pip-Boy/papermap_city_d.dds"
        output_subpath = "textures/interface/pip-boy/papermap_city_d.dds"

        def _texture_output_key(asset: AssetRef) -> str:
            explicit = str(getattr(asset, "output_subpath", "") or "").replace("\\", "/")
            if explicit:
                return explicit.lower()
            rel = self._normalize_asset_source_path(asset.source_path).replace("\\", "/")
            if not rel.lower().startswith("textures/"):
                rel = f"textures/{rel}"
            return rel.lower()

        if any(
            str(asset.asset_type).lower() == "texture"
            and _texture_output_key(asset) == output_subpath
            for asset in assets
        ):
            return assets

        resolved_path, resolution_error = self._resolve_native_asset_path(
            "texture",
            source_path,
            source_plugin,
            ctx,
        )
        expanded = list(assets)
        expanded.append(
            AssetRef(
                asset_type="texture",
                source_path=source_path,
                resolved_path=resolved_path,
                resolution_error=resolution_error,
                provenance=AssetProvenance(
                    added_by_record_fk="",
                    added_by_record_eid="APPALACHIA",
                    added_by_field="WRLD.ICON",
                    walk_depth=0,
                    walker_pass="fo76_pipboy_map",
                    added_by_record_sig="WRLD",
                ),
                force_convert=True,
                force_reason="FO76 Appalachia Pip-Boy map image",
                output_subpath=output_subpath,
            )
        )
        if runner is not None:
            if resolved_path:
                runner.emit_log("INFO", "Queued FO76 Appalachia Pip-Boy map texture")
            else:
                runner.emit_log(
                    "WARN",
                    f"FO76 Appalachia Pip-Boy map texture not found: {resolution_error}",
                )
        return expanded


    def _augment_full_plugin_nif_inventory(
        self,
        assets: list[AssetRef],
        source_plugin: Path,
        ctx: ConversionContext,
        runner: "ConversionRunner | None" = None,
    ) -> list[AssetRef]:
        if not self._should_collect_full_plugin_nif_inventory(ctx):
            return assets

        mesh_roots = self._full_plugin_nif_mesh_roots(source_plugin, ctx)
        if not mesh_roots:
            return assets

        started_at = time.perf_counter()
        workers = self._asset_scan_worker_count(
            getattr(ctx, "conversion_workers", None)
        )
        if runner is not None:
            runner.emit_log(
                "INFO",
                f"Expanding full-plugin NIF inventory: roots={len(mesh_roots)} "
                f"workers={workers}",
            )
        expanded: list[AssetRef] = list(assets)
        seen_paths = {
            self._nif_source_path_key(asset.source_path)
            for asset in expanded
            if asset.asset_type == "nif"
        }
        nifs_seen = 0
        nifs_added = 0

        for mesh_root in mesh_roots:
            nif_files = self._iter_nif_files(mesh_root, workers=workers)
            nifs_seen += len(nif_files)
            for child in nif_files:
                source_path = self._mesh_root_relative_nif_source_path(mesh_root, child)
                key = self._nif_source_path_key(source_path)
                if key in seen_paths:
                    continue
                seen_paths.add(key)
                expanded.append(
                    AssetRef(
                        asset_type="nif",
                        source_path=source_path,
                        resolved_path=str(child),
                        provenance=self._full_plugin_nif_provenance(),
                    )
                )
                nifs_added += 1

        if runner is not None:
            runner.emit_log(
                "INFO",
                f"Expanded {nifs_added} full-plugin filesystem NIF(s) "
                f"from {nifs_seen} candidate(s) "
                f"elapsed={_format_elapsed_seconds(_elapsed_seconds(started_at))}",
            )
        _record_timing(
            ctx,
            "collect_assets_full_plugin_nifs",
            started_at,
            roots=len(mesh_roots),
            candidates=nifs_seen,
            added=nifs_added,
            workers=workers,
        )
        return expanded

    def _augment_lod_convention_assets(
        self,
        assets: list[AssetRef],
        ctx: ConversionContext,
        runner: "ConversionRunner | None" = None,
    ) -> list[AssetRef]:
        """Append FO76 LOD-by-convention meshes (`_lod.nif` + material/texture
        closure) for every LOD-capable base. FO76 ships LOD meshes by folder
        convention with NO record reference, so the native collector never sees
        them (only the 469 record-referenced ones ship today). This MUST ship
        exactly the meshes `synthesize_object_lod` writes MNAM for — both use the
        same native `lod_paths` rule + FO76-source existence check over the same
        LOD-capable bases (conversion `lod_assets`)."""
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        source_dir = str(
            getattr(ctx, "source_data_dir", None) or self._req.source_data_dir or ""
        )
        if rust_run is None or not source_dir:
            return assets

        started_at = time.perf_counter()
        try:
            rows = rust_run.collect_lod_closures()
        except Exception as exc:
            if runner is not None:
                runner.emit_log(
                    "WARN", f"LOD-convention asset collection skipped: {exc}"
                )
            return assets

        expanded = list(assets)
        seen = {
            (a.asset_type, self._normalize_asset_source_path(a.source_path).lower())
            for a in expanded
        }
        provenance = AssetProvenance(
            added_by_record_fk="",
            added_by_record_eid="",
            added_by_field="MNAM",
            walk_depth=0,
            walker_pass="lod_convention",
        )
        added = 0
        for asset_type, source_path, resolved_path in rows:
            norm = self._normalize_asset_source_path(source_path)
            key = (asset_type, norm.lower())
            if key in seen:
                continue
            seen.add(key)
            expanded.append(
                AssetRef(
                    asset_type=asset_type,
                    source_path=norm,
                    resolved_path=(resolved_path or None),
                    provenance=provenance,
                )
            )
            added += 1

        if runner is not None:
            runner.emit_log(
                "INFO",
                f"Expanded {added} LOD-convention asset(s) from {len(rows)} "
                f"candidate(s) elapsed={_format_elapsed_seconds(_elapsed_seconds(started_at))}",
            )
        _record_timing(
            ctx,
            "collect_assets_lod_convention",
            started_at,
            candidates=len(rows),
            added=added,
        )
        return expanded

    def _should_collect_full_plugin_nif_inventory(self, ctx: ConversionContext) -> bool:
        return bool(getattr(ctx, "is_whole_plugin", False)) and (
            self._req.options.cell_bounds is None
        )

    def _full_plugin_nif_mesh_roots(
        self,
        source_plugin: Path,
        ctx: ConversionContext,
    ) -> list[Path]:
        mesh_roots: list[Path] = []
        seen: set[str] = set()
        for root in self._native_asset_source_roots(source_plugin, ctx):
            for candidate in self._mesh_root_candidates(root):
                key = self._path_key(candidate)
                if key in seen:
                    continue
                seen.add(key)
                mesh_roots.append(candidate)
        return mesh_roots

    @classmethod
    def _mesh_root_candidates(cls, root: Path) -> list[Path]:
        candidates: list[Path] = []
        for base in (root, root / "Data"):
            if not base.is_dir():
                continue
            if base.name.lower() == "meshes":
                candidates.append(base)
                continue
            try:
                children = list(base.iterdir())
            except OSError:
                continue
            candidates.extend(
                child
                for child in children
                if child.is_dir() and child.name.lower() == "meshes"
            )
        return candidates

    @staticmethod
    def _asset_scan_worker_count(workers: int | None) -> int:
        try:
            return max(1, int(workers or 1))
        except (TypeError, ValueError):
            return 1

    @classmethod
    def _iter_nif_files(cls, mesh_root: Path, workers: int | None = None) -> list[Path]:
        worker_count = cls._asset_scan_worker_count(workers)
        tasks = cls._nif_scan_tasks(mesh_root, workers=worker_count)
        if not tasks:
            return []

        paths: list[Path] = []
        if worker_count <= 1 or len(tasks) == 1:
            for task in tasks:
                paths.extend(cls._scan_nif_task(task))
        else:
            max_workers = min(worker_count, len(tasks))
            with ThreadPoolExecutor(max_workers=max_workers) as executor:
                for result in executor.map(cls._scan_nif_task, tasks):
                    paths.extend(result)
        return sorted(
            paths,
            key=lambda path: str(path).replace("\\", "/").lower(),
        )

    @staticmethod
    def _nif_scan_tasks(
        mesh_root: Path,
        *,
        workers: int,
    ) -> list[tuple[Path, bool]]:
        if workers <= 1:
            return [(mesh_root, True)] if mesh_root.is_dir() else []
        tasks: list[tuple[Path, bool]] = []
        try:
            with os.scandir(mesh_root) as entries:
                direct_files = False
                for entry in entries:
                    try:
                        if entry.is_dir(follow_symlinks=False):
                            tasks.append((Path(entry.path), True))
                        elif entry.is_file(follow_symlinks=False):
                            direct_files = True
                    except OSError:
                        continue
                if direct_files:
                    tasks.insert(0, (mesh_root, False))
        except OSError:
            return []
        return tasks

    @staticmethod
    def _scan_nif_task(task: tuple[Path, bool]) -> list[Path]:
        base, recursive = task
        paths: list[Path] = []
        stack = [base]
        while stack:
            current = stack.pop()
            try:
                with os.scandir(current) as entries:
                    for entry in entries:
                        try:
                            if entry.is_dir(follow_symlinks=False):
                                if recursive:
                                    stack.append(Path(entry.path))
                            elif (
                                entry.is_file(follow_symlinks=False)
                                and entry.name.lower().endswith(".nif")
                            ):
                                paths.append(Path(entry.path))
                        except OSError:
                            continue
            except OSError:
                continue
            if not recursive:
                break
        return paths

    @staticmethod
    def _mesh_root_relative_nif_source_path(mesh_root: Path, child: Path) -> str:
        rel = child.relative_to(mesh_root).as_posix()
        return f"Meshes/{rel}"

    @staticmethod
    def _full_plugin_nif_provenance() -> AssetProvenance:
        return AssetProvenance(
            added_by_record_fk="",
            added_by_record_eid="",
            added_by_field="filesystem",
            walk_depth=0,
            walker_pass="full_plugin_nif_sweep",
            added_by_record_sig="",
        )

    @staticmethod
    def _path_key(path: Path) -> str:
        try:
            return str(path.resolve()).replace("\\", "/").lower()
        except OSError:
            return str(path).replace("\\", "/").lower()

    def _augment_character_asset_companion_nifs(
        self,
        assets: list[AssetRef],
        runner: "ConversionRunner | None" = None,
    ) -> list[AssetRef]:
        expanded: list[AssetRef] = list(assets)
        seen_paths = {
            self._nif_source_path_key(asset.source_path)
            for asset in expanded
            if asset.asset_type == "nif"
        }
        scanned_dirs: set[str] = set()
        companions_added = 0

        for asset in assets:
            if asset.asset_type != "nif" or not asset.resolved_path:
                continue
            source_dir = self._character_assets_source_dir(asset.source_path)
            if source_dir is None:
                continue
            disk_dir = self._character_assets_disk_dir(asset.resolved_path)
            if disk_dir is None or not disk_dir.is_dir():
                continue
            disk_key = str(disk_dir).replace("\\", "/").lower()
            if disk_key in scanned_dirs:
                continue
            scanned_dirs.add(disk_key)

            for child in sorted(disk_dir.iterdir(), key=lambda path: path.name.lower()):
                if not child.is_file() or child.suffix.lower() != ".nif":
                    continue
                source_path = f"{source_dir}/{child.name}".replace("\\", "/")
                key = self._nif_source_path_key(source_path)
                if key in seen_paths:
                    continue
                seen_paths.add(key)
                expanded.append(
                    AssetRef(
                        asset_type="nif",
                        source_path=source_path,
                        resolved_path=str(child),
                        provenance=self._character_asset_provenance(asset),
                    )
                )
                companions_added += 1

        if companions_added and runner is not None:
            runner.emit_log(
                "INFO",
                f"Expanded {companions_added} CharacterAssets companion NIF(s)",
            )
        return expanded

    @classmethod
    def _character_assets_source_dir(cls, source_path: str) -> str | None:
        parts = [
            part
            for part in cls._normalize_asset_source_path(source_path).replace("\\", "/").split("/")
            if part
        ]
        for index, part in enumerate(parts):
            if part.lower() == "characterassets":
                return "/".join(parts[: index + 1])
        return None

    @staticmethod
    def _character_assets_disk_dir(resolved_path: str) -> Path | None:
        path = Path(resolved_path)
        for candidate in (path.parent, *path.parent.parents):
            if candidate.name.lower() == "characterassets":
                return candidate
        return None

    @classmethod
    def _nif_source_path_key(cls, source_path: str) -> str:
        return cls._strip_meshes_prefix(source_path).lower()

    @staticmethod
    def _character_asset_provenance(asset: AssetRef) -> AssetProvenance | None:
        provenance = asset.provenance
        if provenance is None:
            return None
        return AssetProvenance(
            added_by_record_fk=provenance.added_by_record_fk,
            added_by_record_eid=provenance.added_by_record_eid,
            added_by_field=provenance.added_by_field,
            walk_depth=provenance.walk_depth,
            walker_pass="character_assets",
            added_by_record_sig=provenance.added_by_record_sig,
        )


    def _augment_havok_behavior_bundles(
        self,
        assets: list[AssetRef],
        source_plugin: Path,
        ctx: ConversionContext,
        runner: "ConversionRunner | None" = None,
    ) -> list[AssetRef]:
        from bacup_lib.behavior.deps import expand_behavior_bundle

        roots = self._native_asset_source_roots(source_plugin, ctx)
        if not roots:
            return assets

        seen_paths = {
            self._havok_source_path_key(asset.source_path)
            for asset in assets
        }
        expanded: list[AssetRef] = list(assets)
        project_assets = [
            asset for asset in assets if self._is_havok_project_asset(asset)
        ]
        if not project_assets:
            return expanded

        started_at = time.perf_counter()
        workers = self._asset_scan_worker_count(
            getattr(ctx, "conversion_workers", None)
        )
        if runner is not None:
            runner.emit_log(
                "INFO",
                f"Expanding Havok behavior bundles: projects={len(project_assets)} "
                f"roots={len(roots)} workers={workers}",
            )
        projects_expanded = 0
        companions_added = 0
        companions_seen = 0

        def expand_project(asset: AssetRef) -> tuple[AssetRef, list[AssetRef]]:
            project_asset = AssetRef(
                asset.asset_type,
                self._strip_meshes_prefix(asset.source_path),
                asset.resolved_path,
                asset.resolution_error,
                asset.is_cdb_ref,
                asset.provenance,
            )
            companions: list[AssetRef] = []
            for root in roots:
                companions.extend(expand_behavior_bundle(project_asset, str(root)))
            return asset, companions

        if workers <= 1 or len(project_assets) == 1:
            project_results = [expand_project(asset) for asset in project_assets]
        else:
            max_workers = min(workers, len(project_assets))
            with ThreadPoolExecutor(max_workers=max_workers) as executor:
                project_results = list(executor.map(expand_project, project_assets))

        for _asset, companions in project_results:
            project_added = 0
            companions_seen += len(companions)
            for companion in companions:
                source_path = self._normalize_asset_source_path(companion.source_path)
                key = self._havok_source_path_key(source_path)
                if key in seen_paths:
                    continue
                seen_paths.add(key)
                asset_type = self._havok_companion_asset_type(source_path)
                resolved_path, resolution_error = self._resolve_native_asset_path(
                    asset_type,
                    source_path,
                    source_plugin,
                    ctx,
                )
                expanded.append(
                    AssetRef(
                        asset_type=asset_type,
                        source_path=source_path,
                        resolved_path=resolved_path,
                        resolution_error=resolution_error,
                    )
                )
                project_added += 1

            if project_added:
                projects_expanded += 1
                companions_added += project_added

        if runner is not None:
            runner.emit_log(
                "INFO",
                "Expanded "
                f"{companions_added} Havok companion assets from "
                f"{projects_expanded} behavior project(s) "
                f"({companions_seen} candidate(s)) "
                f"elapsed={_format_elapsed_seconds(_elapsed_seconds(started_at))}",
            )
        _record_timing(
            ctx,
            "collect_assets_havok_bundles",
            started_at,
            projects=len(project_assets),
            projects_expanded=projects_expanded,
            candidates=companions_seen,
            added=companions_added,
            workers=workers,
        )
        return expanded

    @classmethod

    def _is_havok_project_asset(cls, asset: AssetRef) -> bool:
        if asset.asset_type != "behavior":
            return False
        rel_path = cls._strip_meshes_prefix(asset.source_path).lower()
        if not rel_path.endswith(".hkx"):
            return False
        path_parts = rel_path.split("/")
        companion_dirs = {"animations", "behaviors", "characterassets", "characters"}
        return not any(part in companion_dirs for part in path_parts[:-1])

    @classmethod

    def _havok_companion_asset_type(cls, source_path: str) -> str:
        path_parts = cls._strip_meshes_prefix(source_path).lower().split("/")
        return "animation" if "animations" in path_parts[:-1] else "behavior"

    @classmethod

    def _havok_source_path_key(cls, source_path: str) -> str:
        return cls._strip_meshes_prefix(source_path).lower()

    @classmethod

    def _strip_meshes_prefix(cls, source_path: str) -> str:
        rel_path = cls._normalize_asset_source_path(source_path)
        if rel_path.lower().startswith("meshes/"):
            return rel_path[7:]
        return rel_path

    _ASSET_PREFIX_FALLBACKS: dict[str, tuple[str, ...]] = {
        "nif": ("Meshes",),
        "texture": ("Textures",),
        "material": ("Materials",),
        "sound": ("Sound",),
        "behavior": ("Meshes",),
        "animation": ("Meshes",),
        "support": ("Meshes",),
    }
    _SOUND_EXTS = (".wav", ".xwm", ".fuz")


    def _asset_ref_from_native_item(
        self,
        item: object,
        source_plugin: Path,
        ctx: ConversionContext,
    ) -> AssetRef:
        asset_type = str(item["asset_type"])
        source_path = self._normalize_asset_source_path(str(item["source_path"]))
        source_plugin_names = self._source_plugin_names_from_native_item(item)
        resolved_path, resolution_error = self._resolve_native_asset_path(
            asset_type,
            source_path,
            source_plugin,
            ctx,
            source_plugin_names=source_plugin_names,
        )
        return AssetRef(
            asset_type=asset_type,
            source_path=source_path,
            resolved_path=resolved_path,
            resolution_error=resolution_error,
            provenance=AssetProvenance(
                added_by_record_fk=str(item.get("source_form_key") or ""),
                added_by_record_eid="",
                added_by_field=str(item.get("source_subrecord_sig") or ""),
                walk_depth=0,
                walker_pass="native_asset_collect",
                added_by_record_sig=str(item.get("source_record_signature") or ""),
            ),
        )

    @staticmethod

    def _normalize_asset_source_path(source_path: str) -> str:
        return normalize_asset_source_path(source_path)

    @classmethod

    def _source_plugin_names_from_native_item(cls, item: object) -> tuple[str, ...]:
        get = getattr(item, "get", None)
        if not callable(get):
            return ()
        source_form_key = str(get("source_form_key") or "")
        plugin_name = cls._plugin_name_from_form_key(source_form_key)
        return (plugin_name,) if plugin_name else ()

    @staticmethod
    def _plugin_name_from_form_key(form_key: str) -> str | None:
        for part in form_key.replace("\\", "/").split(":"):
            candidate = part.strip()
            if Path(candidate).suffix.lower() in {".esm", ".esp", ".esl"}:
                return candidate
        return None

    @classmethod
    def _scol_alias_asset_paths(
        cls,
        rel_path: str,
        source_plugin: Path,
        source_plugin_names: tuple[str, ...],
    ) -> list[str]:
        parts = rel_path.replace("\\", "/").strip().lstrip("/").split("/")
        if not parts:
            return []

        prefix: list[str] = []
        if parts[0].lower() == "meshes":
            prefix = ["Meshes"]
            parts = parts[1:]

        if len(parts) < 3 or parts[0].lower() != "scol":
            return []
        if Path(parts[1]).suffix.lower() not in {".esm", ".esp", ".esl"}:
            return []

        names: list[str] = []
        for name in [
            *source_plugin_names,
            *cls._source_name_candidates(source_plugin.name),
        ]:
            for candidate in (name, name.lower()):
                if candidate and candidate.lower() != parts[1].lower():
                    names.append(candidate)

        seen: set[str] = set()
        aliases: list[str] = []
        for name in names:
            key = name.lower()
            if key in seen:
                continue
            seen.add(key)
            aliases.append("/".join([*prefix, "SCOL", name, *parts[2:]]))
        return aliases


    def _resolve_native_asset_path(
        self,
        asset_type: str,
        source_path: str,
        source_plugin: Path,
        ctx: ConversionContext,
        *,
        source_plugin_names: tuple[str, ...] = (),
    ) -> tuple[str | None, str | None]:
        rel_path = source_path.replace("\\", "/")
        direct_path = Path(rel_path)
        if direct_path.is_absolute():
            if direct_path.is_file() or (
                asset_type == "sound" and direct_path.is_dir()
            ):
                return str(direct_path), None
            return None, f"File not found: {direct_path}"

        roots = self._native_asset_source_roots(source_plugin, ctx)
        raw_candidates = [
            rel_path,
            *self._scol_alias_asset_paths(rel_path, source_plugin, source_plugin_names),
        ]
        candidates: list[str] = []
        seen_candidates: set[str] = set()
        for raw_candidate in raw_candidates:
            key = raw_candidate.lower()
            if key not in seen_candidates:
                seen_candidates.add(key)
                candidates.append(raw_candidate)
            for prefix in self._ASSET_PREFIX_FALLBACKS.get(asset_type, ()):
                if raw_candidate.lower().startswith(prefix.lower() + "/"):
                    continue
                prefixed = f"{prefix}/{raw_candidate}"
                key = prefixed.lower()
                if key not in seen_candidates:
                    seen_candidates.add(key)
                    candidates.append(prefixed)

        tried: list[str] = []
        for root in roots:
            for base in (root, root / "Data"):
                for candidate in candidates:
                    full = base.joinpath(*candidate.split("/"))
                    tried.append(str(full))
                    if full.is_file() or (asset_type == "sound" and full.is_dir()):
                        return str(full), None
                    if asset_type == "sound":
                        stem = full.with_suffix("")
                        ext = full.suffix.lower()
                        if ext in self._SOUND_EXTS:
                            for alt_ext in self._SOUND_EXTS:
                                if alt_ext == ext:
                                    continue
                                alt = stem.with_suffix(alt_ext)
                                tried.append(str(alt))
                                if alt.is_file():
                                    return str(alt), None

        if not roots:
            return None, "Source asset root not configured."
        tried_preview = ", ".join(tried[:6])
        if len(tried) > 6:
            tried_preview += f" (+{len(tried) - 6} more)"
        return None, f"File not found in source roots. Searched: {tried_preview}"

    @staticmethod

    def _native_asset_source_roots(
        source_plugin: Path,
        ctx: ConversionContext,
    ) -> list[Path]:
        roots: list[Path] = []
        for attr in ("source_extracted_dir", "source_data_dir", "extracted_dir"):
            value = getattr(ctx, attr, None)
            if value:
                roots.append(Path(value))
        roots.extend(
            Path(root)
            for root in getattr(ctx, "additional_source_asset_roots", ()) or ()
        )
        if source_plugin.parent:
            roots.append(source_plugin.parent)

        deduped: list[Path] = []
        seen: set[str] = set()
        for root in roots:
            key = str(root).replace("\\", "/").lower()
            if key in seen:
                continue
            seen.add(key)
            deduped.append(root)
        return deduped


    def _defers_placed_child_ref_class(self, ctx: ConversionContext) -> bool:
        """True only for whole-plugin FO76->FO4 worldspace runs, where the phase-6
        cell-slice copy re-inserts exterior placed children (ACHR/REFR/...) AFTER
        the phase-2 fixups.

        When set, the pre-copy null_dangling_own_plugin_refs pass DEFERS the
        placed-ref-target class (LCTN LCUN/LCEP/ACEP) — leaving those refs intact —
        and the authoritative resolution runs post-copy via
        _repair_placed_child_refs (conversion_run_repair_placed_child_refs). Gated
        identically to the phase-6 copy so the deferral and repair are paired.
        """
        opts = self._req.options
        return bool(
            opts.translate_records
            and opts.convert_terrain
            and bool(getattr(ctx, "is_whole_plugin", False))
            and self._req.source_game == "fo76"
            and self._req.target_game == "fo4"
        )


    def _native_run_config(self, ctx: ConversionContext) -> dict:
        opts = self._req.options
        source_extracted = (
            getattr(ctx, "source_data_dir", None) or self._req.source_data_dir
        )
        target_extracted = (
            getattr(ctx, "target_extracted_dir", None) or self._req.target_extracted_dir
        )
        target_data = getattr(ctx, "target_data_dir", None) or self._req.target_data_dir
        return {
            "output_plugin_name": getattr(ctx, "output_plugin_name", "Output.esm"),
            "preserve_source_ids": bool(getattr(ctx, "preserve_source_ids", True)),
            "use_base_game_assets": True,
            "is_whole_plugin": bool(getattr(ctx, "is_whole_plugin", False)),
            "root_sig": None,
            "mod_path": str(getattr(ctx, "mod_path", "") or ""),
            "source_extracted_dir": str(source_extracted) if source_extracted else None,
            "additional_source_asset_roots": [
                str(root)
                for root in getattr(ctx, "additional_source_asset_roots", ()) or ()
            ],
            "target_extracted_dir": str(target_extracted) if target_extracted else None,
            "target_data_dir": str(target_data) if target_data else None,
            "target_asset_catalog_path": (
                str(getattr(ctx, "target_asset_catalog_path", "") or "")
                if getattr(ctx, "target_asset_catalog_path", None)
                else None
            ),
            "target_asset_cache_dir": (
                str(getattr(ctx, "target_asset_cache_dir", "") or "")
                if getattr(ctx, "target_asset_cache_dir", None)
                else None
            ),
            "conversion_workers": getattr(ctx, "conversion_workers", None),
            "records_limit": getattr(ctx, "records_limit", None),
            "generated_object_id_floor": int(
                getattr(ctx, "generated_object_id_floor", 0) or 0
            ),
            "target_record_preflight": list(
                getattr(ctx, "target_record_preflight_rows", []) or []
            ),
            "target_master_names": list(
                getattr(ctx, "target_record_preflight_master_names", []) or []
            ),
            "base_asset_relocation_mesh_roots": list(
                getattr(ctx, "base_asset_relocation_mesh_roots", ()) or []
            ),
            "base_asset_namespace": str(getattr(ctx, "base_asset_namespace", "") or ""),
            "warning_policy": "warn_playable",
            "skip_record_signatures": _skip_record_signatures_payload(
                convert_placed_records=opts.convert_placed_records,
                exclude_signatures=getattr(opts, "exclude_signatures", frozenset()),
            ),
            "defer_placed_child_ref_class": self._defers_placed_child_ref_class(ctx),
            "legacy_pack_origins": [
                row.to_dict() for row in self._req.legacy_pack_origins
            ],
            "legacy_pack_raw_source_counts": (
                self._req.legacy_pack_raw_source_counts.to_dict()
                if self._req.legacy_pack_raw_source_counts is not None
                else None
            ),
            "legacy_pack_expected_counts": (
                self._req.legacy_pack_expected_counts.to_dict()
                if self._req.legacy_pack_expected_counts is not None
                else None
            ),
            "legacy_pack_provenance_required": self._req.legacy_pack_provenance_required,
            "asset_phases": {
                "terrain": bool(opts.convert_terrain),
                "nifs": bool(opts.convert_nifs),
                "textures": bool(opts.convert_textures),
                "materials": bool(opts.convert_materials),
                "havok": bool(opts.convert_havok),
                "animations": bool(opts.convert_animations),
                "sounds": bool(opts.copy_sounds),
            },
        }


    def _translate_records_rust(
        self,
        records: list[RecordNode],
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        """Run the translate-records phase via the native Rust ConversionRun pipeline.

        The ConversionRun is stashed on ``ctx._rust_conversion_run`` and kept
        alive past this phase so the optional FNV-legacy phase can reuse the
        same mapper state. The outer ``_convert_one_plugin``
        ``finally`` block is responsible for draining decisions/warnings and
        dropping the run. Decisions/warnings emitted by this translate phase
        accumulate on the Rust side and are drained together with any
        FNV-phase warnings at scope end.
        """
        from bacup_lib.run import ConversionRun

        source_plugin_path = getattr(ctx, "source_plugin_path", None)
        if source_plugin_path is None:
            raise RuntimeError(
                "Native backend requires ctx.source_plugin_path to be set"
            )
        target_plugin_name = getattr(ctx, "output_plugin_name", "Output.esm")
        target_game = ctx.target_game
        config = self._native_run_config(ctx)

        runner.emit_log("INFO", "Native backend: creating ConversionRun")
        # NOTE: deliberately NOT using `with ConversionRun(...)` here. The run
        # must outlive this phase so the FNV-legacy phase can re-enter the
        # same mapper state. `_convert_one_plugin`'s finally block drops it.
        run = ConversionRun.create_new(
            ctx.source_game,
            target_game,
            str(getattr(ctx, "source_plugin_path")),
            target_plugin_name,
            master_plugin_paths=[
                str(path) for path in getattr(ctx, "target_master_plugin_paths", [])
            ],
            source_strings_dir=_resolve_source_strings_dir(
                Path(source_plugin_path),
                getattr(ctx, "source_extracted_dir", None),
                getattr(ctx, "source_data_dir", None),
                getattr(ctx, "extracted_dir", None),
            ),
            config=config,
        )
        ctx._rust_conversion_run = run  # type: ignore[attr-defined]
        ctx._event_drainer = Drainer(run, runner)  # type: ignore[attr-defined]
        ctx._event_drainer.start()
        terrain_graft_source = getattr(ctx, "_terrain_graft_source", None)
        if terrain_graft_source is not None:
            prepare_report = run.run_phase(
                "prepare_graft_terrain",
                mod_path="",
                params={"prior_plugin_path": str(terrain_graft_source)},
            )
            runner.emit_log(
                "INFO",
                "Reserved "
                f"{int(prepare_report.get('records_deferred', 0))} "
                "reused terrain/navmesh FormIDs from generated allocation",
            )
        maps_started = time.perf_counter()
        run.run_phase(
            "record_translation_maps", mod_path="", params={"validate_only": False}
        )
        _record_timing(ctx, "record_translation_maps", maps_started)
        runner.emit_log(
            "INFO",
            f"Native backend: ConversionRun created (id={run.id}); calling translate_all",
        )
        translate_started = time.perf_counter()
        report = run.run_phase("translate_v2", mod_path="", params={})
        stats = self._translate_stats_from_report(report)
        translate_elapsed = _elapsed_seconds(translate_started)
        ctx.native_translate_stats = dict(stats)
        _timing_report(ctx).record(
            "translate_all",
            translate_elapsed,
            translated=stats.get("records_translated", 0),
            dropped=stats.get("records_dropped", 0),
            deferred=stats.get("records_deferred", 0),
            failed=stats.get("records_failed", 0),
        )
        runner.emit_log(
            "INFO",
            f"Native backend: translate_all done; "
            f"translated={stats.get('records_translated', 0)}, "
            f"dropped={stats.get('records_dropped', 0)}, "
            f"deferred={stats.get('records_deferred', 0)}, "
            f"failed={stats.get('records_failed', 0)}; calling Story Manager subset",
        )
        story_manager_started = time.perf_counter()
        story_manager_report = run.run_phase(
            "emit_story_manager_subset", mod_path="", params={}
        )
        story_manager_elapsed = _elapsed_seconds(story_manager_started)
        story_manager_added = int(story_manager_report.get("records_added", 0))
        story_manager_changed = int(story_manager_report.get("records_changed", 0))
        story_manager_skipped = int(story_manager_report.get("records_dropped", 0))
        ctx.native_story_manager_stats = dict(story_manager_report)
        stats["records_translated"] = int(stats.get("records_translated", 0)) + story_manager_added
        _timing_report(ctx).record(
            "emit_story_manager_subset",
            story_manager_elapsed,
            added=story_manager_added,
            changed=story_manager_changed,
            skipped=story_manager_skipped,
        )
        runner.emit_log(
            "INFO",
            "Native backend: Story Manager subset done "
            f"(added={story_manager_added}, "
            f"quest_autostarts={story_manager_changed}, "
            f"skipped={story_manager_skipped}); calling apply_fixups",
        )
        # Emit translate-phase warnings/decisions inline so they're visible as
        # they happen (the run-handle's `finally` drain at the end of
        # _convert_one_plugin batches anything emitted after this).
        self._emit_run_warnings_inline(run, runner, ctx)
        fixups_started = time.perf_counter()
        fixups_v2_report = run.run_phase("fixups_v2", mod_path="", params={})
        fixups_elapsed = _elapsed_seconds(fixups_started)
        ctx.native_fixup_reports = []
        total_changed = int(fixups_v2_report.get("records_changed", 0))
        total_dropped = int(fixups_v2_report.get("records_dropped", 0))
        total_added = int(fixups_v2_report.get("records_added", 0))
        total_warnings = int(fixups_v2_report.get("warnings", 0))
        _timing_report(ctx).record(
            "apply_fixups",
            fixups_elapsed,
            changed=total_changed,
            dropped=total_dropped,
            added=total_added,
            warnings=total_warnings,
        )
        runner.emit_log(
            "INFO",
            "Native backend: apply_fixups done "
            f"(changed={total_changed}, "
            f"added={total_added}, "
            f"dropped={total_dropped}, "
            f"warnings={total_warnings})",
        )
        runner.emit_log(
            "INFO",
            f"Native backend timing: translate_all={translate_elapsed:.3f}s "
            f"apply_fixups={fixups_elapsed:.3f}s",
        )
        mswp_started = time.perf_counter()
        mswp_report = run.run_phase(
            "rewrite_mswp_material_paths", mod_path="", params={}
        )
        _timing_report(ctx).record(
            "rewrite_mswp_material_paths",
            _elapsed_seconds(mswp_started),
            changed=int(mswp_report.get("records_changed", 0)),
        )
        self._emit_run_warnings_inline(run, runner, ctx)

        self._apply_placed_record_position_offset(ctx, runner)

        runner.emit_log(
            "INFO",
            f"Native backend: translated={stats.get('records_translated', 0)}, "
            f"dropped={stats.get('records_dropped', 0)}, "
            f"deferred={stats.get('records_deferred', 0)}, "
            f"failed={stats.get('records_failed', 0)}",
        )

        ctx.summary.records_translated += stats.get("records_translated", 0)
        ctx.summary.records_vanilla_remapped += stats.get("records_vanilla_remapped", 0)
        ctx.summary.records_warnings += stats.get("records_failed", 0)

    @staticmethod
    def _translate_stats_from_report(report: dict[str, object]) -> dict[str, int]:
        return {
            "records_translated": int(report.get("records_changed", 0) or 0),
            "records_vanilla_remapped": int(
                report.get("records_vanilla_remapped", 0) or 0
            ),
            "records_dropped": int(report.get("records_dropped", 0) or 0),
            "records_deferred": int(report.get("records_deferred", 0) or 0),
            "records_failed": int(report.get("warnings", 0) or 0),
        }

    def _run_synthesize_object_lod_existing_output(
        self,
        source_plugin: Path,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> None:
        output_path = Path(ctx.mod_path) / ctx.output_plugin_name
        if not output_path.is_file():
            raise RuntimeError(
                "synthesize_object_lod with --no-translate-records requires an "
                f"existing output plugin at {output_path}"
            )

        from bacup_lib.run import ConversionRun

        temp_output_path: Path | None = None
        try:
            source_extracted = str(
                getattr(ctx, "source_data_dir", None)
                or self._req.source_data_dir
                or ""
            )
            with ConversionRun.open_existing(
                str(ctx.source_game),
                str(ctx.target_game),
                str(source_plugin),
                str(output_path),
                master_plugin_paths=[
                    str(path)
                    for path in getattr(ctx, "target_master_plugin_paths", [])
                ],
                source_strings_dir=_resolve_source_strings_dir(
                    source_plugin,
                    getattr(ctx, "source_data_dir", None),
                    self._req.source_data_dir,
                ),
                config=self._native_run_config(ctx),
            ) as run:
                report = run.run_phase(
                    "synthesize_object_lod",
                    mod_path=str(ctx.mod_path),
                    source_extracted_dir=source_extracted,
                    params={
                        "conversion_workers": getattr(
                            ctx, "conversion_workers", None
                        ),
                    },
                )
                self._emit_run_warnings_inline(run, runner, ctx)
                temp_fd, temp_name = tempfile.mkstemp(
                    dir=output_path.parent,
                    prefix=f".{output_path.name}.",
                    suffix=".tmp",
                )
                temp_output_path = Path(temp_name)
                os.close(temp_fd)
                run.save_target(str(temp_output_path), run_nvnm_validator=False)
            os.replace(temp_output_path, output_path)
            _cleanup_temp_save_strings(temp_output_path)
            temp_output_path = None
            runner.emit_log(
                "INFO",
                "synthesize_object_lod: updated existing output plugin "
                f"{output_path.name}; changed={report.get('records_changed', 0)} "
                f"assets={report.get('assets_written', 0)} "
                f"warnings={report.get('warnings', 0)}",
            )
        finally:
            if temp_output_path is not None:
                _cleanup_temp_save_strings(temp_output_path)
                try:
                    temp_output_path.unlink(missing_ok=True)
                except OSError:
                    pass

    def _apply_placed_record_position_offset(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> None:
        offset = tuple(
            float(value) for value in self._req.options.placed_record_position_offset
        )
        if offset == (0.0, 0.0, 0.0):
            return
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log(
                "WARN",
                "placed record position offset skipped: no target plugin handle",
            )
            return
        changed = _apply_placed_record_position_offset(
            run_id=int(getattr(ctx, "_rust_conversion_run").id),
            offset=offset,
        )
        runner.emit_log(
            "INFO",
            "placed record position offset: "
            f"records={changed} x={offset[0]} y={offset[1]} z={offset[2]}",
        )

    @staticmethod

    def _terrain_copy_bounds(
        terrain, worldspace_editor_id: str
    ) -> WorldspaceCellBounds:
        coords = (
            getattr(terrain, "source_min_x", None),
            getattr(terrain, "source_min_y", None),
            getattr(terrain, "source_max_x", None),
            getattr(terrain, "source_max_y", None),
        )
        if any(value is None for value in coords):
            min_x = min_y = _FULL_WORLDSPACE_CELL_MIN
            max_x = max_y = _FULL_WORLDSPACE_CELL_MAX
        else:
            min_x, min_y, max_x, max_y = (int(value) for value in coords)
            if max_x < min_x or max_y < min_y:
                min_x = min_y = _FULL_WORLDSPACE_CELL_MIN
                max_x = max_y = _FULL_WORLDSPACE_CELL_MAX

        return WorldspaceCellBounds(
            worldspace_editor_id=worldspace_editor_id,
            min_x=min_x,
            min_y=min_y,
            max_x=max_x,
            max_y=max_y,
            include_worldspace_persistent_cell=False,
        )


    def _copy_fo76_projected_placed_children(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log(
                "WARN",
                "projected placed children copy skipped: no target plugin handle",
            )
            progress.total_items = 0
            progress.completed_items = 0
            return

        from bacup_lib.pipeline.terrain import fo76_btd_work_items

        _btd_started = time.perf_counter()
        try:
            terrain_items = fo76_btd_work_items(ctx)
        except FileNotFoundError as exc:
            runner.emit_log("ERROR", str(exc))
            progress.status = "error"
            raise
        btd_secs = time.perf_counter() - _btd_started
        if not terrain_items:
            runner.emit_log(
                "INFO",
                "projected placed children copy skipped: no FO76 terrain worldspaces",
            )
            progress.total_items = 0
            progress.completed_items = 0
            progress.status = "completed"
            runner.emit_item_progress(progress)
            return

        total_placed = 0
        total_inserted = 0
        for terrain, _btd_path, _worldspace_eid in terrain_items:
            placed, inserted = self._copy_fo76_projected_placed_children_for_terrain(
                ctx,
                runner,
                terrain,
            )
            total_placed += placed
            total_inserted += inserted

        # btd timing here; per-worldspace sub-steps logged by
        # _..._for_terrain below.
        runner.emit_log(
            "INFO",
            f"placed-children phase timing: btd_work_items={btd_secs:.3f}s "
            f"worldspaces={len(terrain_items)} placed={total_placed} inserted={total_inserted}",
        )
        progress.total_items = total_placed
        progress.completed_items = total_inserted
        progress.status = "completed"
        runner.emit_item_progress(progress)


    def _copy_fo76_projected_placed_children_for_terrain(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        terrain,
    ) -> tuple[int, int]:
        target_worldspace = (
            getattr(terrain, "worldspace_editor_id", "")
            or Path(ctx.output_plugin_name).stem
        )
        source_worldspace = (
            getattr(terrain, "source_worldspace_editor_id", "") or target_worldspace
        )
        source_bounds = self._terrain_copy_bounds(terrain, source_worldspace)

        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log(
                "WARN",
                "projected placed children copy skipped: no native conversion run",
            )
            return 0, 0

        from bacup_lib.native_runtime import (
            load_native_module as _conv_native,
        )

        offset = tuple(
            float(value) for value in self._req.options.placed_record_position_offset
        )
        # Native orchestration: topology collection, grid routing, mapper
        # lookup, and the production copy kernel all run inside the conversion
        # crate — the multi-million-key topology never enters Python.
        report = _conv_native().conversion_run_copy_projected_placed_children(
            int(rust_run.id),
            source_worldspace,
            source_bounds.min_x,
            source_bounds.min_y,
            source_bounds.max_x,
            source_bounds.max_y,
            offset[0],
            offset[1],
            offset[2],
            worker_count=getattr(ctx, "conversion_workers", None),
        )
        timings = {
            str(name).removesuffix("_ms"): int(value) / 1000.0
            for name, value in dict(report.get("timing") or {}).items()
        }
        placed = int(report.get("placed", 0) or 0)
        for warning in [str(v) for v in report.get("collect_warnings") or []][:25]:
            runner.emit_log("WARN", f"projected placed children source: {warning}")

        result = report.get("copy") or None
        if result is None:
            runner.emit_log(
                "INFO",
                f"projected placed children copy: no placed children found in {source_worldspace}",
            )
            return placed, 0

        runner.emit_log(
            "INFO",
            "projected placed children copy start: "
            f"worldspace={source_worldspace}->{target_worldspace} "
            f"source_cells={int(report.get('source_cells', 0) or 0)} "
            f"matched_cells={int(report.get('matched_cells', 0) or 0)} "
            f"placed={placed}",
        )
        inserted = int(result.get("children_inserted", 0))
        skipped = [str(value) for value in result.get("skipped_children") or []]
        warnings = [str(value) for value in result.get("warnings") or []]
        runner.emit_log(
            "INFO",
            "projected placed children copy done: "
            f"worldspace={source_worldspace}->{target_worldspace} "
            f"cells={int(result.get('cells_touched', 0))} "
            f"inserted={inserted} skipped={len(skipped)} "
            f"rebucketed={int(result.get('children_rebucketed', 0))} "
            f"mapped_refs={int(result.get('mapped_form_refs', 0))} "
            f"reallocated_children={int(result.get('child_form_ids_reallocated', 0))} "
            f"leveled_bases={int(result.get('leveled_bases_resolved', 0))} "
            f"dropped_subrecords={int(result.get('schema_subrecords_dropped', 0))} "
            f"missing_bases={int(result.get('missing_base_children', 0))} "
            f"region_refs_rewritten={int(result.get('cell_region_refs_rewritten', 0))}",
        )
        for warning in warnings[:25]:
            runner.emit_log("WARN", f"projected placed children copy: {warning}")
        if skipped:
            runner.emit_log(
                "WARN",
                f"projected placed children skipped records: count={len(skipped)} first={skipped[0]}",
            )
        _t = time.perf_counter()
        region_sync = _sync_cell_regions_from_source(
            int(rust_run.id),
            source_worldspace_editor_id=source_worldspace,
            target_worldspace_editor_id=target_worldspace,
        )
        timings["region_sync"] = time.perf_counter() - _t
        runner.emit_log(
            "INFO",
            "projected cell region sync done: "
            f"worldspace={source_worldspace}->{target_worldspace} "
            f"cells_changed={int(region_sync.get('cells_changed', 0))} "
            f"region_refs_written={int(region_sync.get('region_refs_written', 0))} "
            f"missing_target_regions={int(region_sync.get('missing_target_regions', 0))} "
            f"warnings={len(region_sync.get('warnings') or [])}",
        )
        for warning in [str(value) for value in region_sync.get("warnings") or []][:25]:
            runner.emit_log("WARN", f"projected cell region sync: {warning}")
        runner.emit_log(
            "INFO",
            "placed-children timing: "
            f"worldspace={source_worldspace}->{target_worldspace} "
            + " ".join(f"{name}={secs:.3f}s" for name, secs in timings.items()),
        )
        return placed, inserted


    def _synthesize_worldspace_persistent_cells(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        """Emit the FO4 worldspace-persistent CELL and route the source
        worldspace-persistent refs (REFR/ACHR/PHZD) under it, converted FO76→FO4.

        The translator skips top-level CELL/REFR/ACHR and the terrain phase only
        synthesizes grid cells, so the output otherwise has no worldspace
        persistent cell — dropping ~130k persistent refs and dangling every QUST
        ALFR / LCTN MNAM that targets them. Runs after the grid-cell copy (so the
        WRLD + grid cells exist) and before ``_repair_placed_child_refs`` (so the
        deferred ALFR/MNAM resolve against the now-present refs).
        """
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log(
                "WARN",
                "worldspace persistent cell synthesis skipped: no plugin handle",
            )
            progress.total_items = 0
            progress.completed_items = 0
            progress.status = "completed"
            runner.emit_item_progress(progress)
            return

        from bacup_lib.pipeline.terrain import fo76_btd_work_items

        _btd_started = time.perf_counter()
        try:
            terrain_items = fo76_btd_work_items(ctx)
        except FileNotFoundError as exc:
            runner.emit_log("ERROR", str(exc))
            progress.status = "error"
            raise
        btd_secs = time.perf_counter() - _btd_started

        if not terrain_items:
            runner.emit_log(
                "INFO",
                "worldspace persistent cell synthesis skipped: no FO76 terrain worldspaces",
            )
            progress.total_items = 0
            progress.completed_items = 0
            progress.status = "completed"
            runner.emit_item_progress(progress)
            return

        from bacup_lib.native_runtime import (
            load_native_module as _conv_native,
        )

        offset = tuple(
            float(value) for value in self._req.options.placed_record_position_offset
        )
        native_module = _conv_native()

        total_converted = 0
        for terrain, _btd_path, _worldspace_eid in terrain_items:
            source_worldspace = (
                getattr(terrain, "source_worldspace_editor_id", "")
                or getattr(terrain, "worldspace_editor_id", "")
                or Path(ctx.output_plugin_name).stem
            )
            # Native orchestration: seed collection (reusing the copy phase's
            # scan when it covered the full worldspace), all-signature
            # persistent base keys, the FO76->FO4 FormKey map, and the
            # synthesis kernel all run inside the conversion crate.
            result = native_module.conversion_run_synthesize_worldspace_persistent_cell(
                int(rust_run.id),
                source_worldspace,
                _FULL_WORLDSPACE_CELL_MIN,
                _FULL_WORLDSPACE_CELL_MIN,
                _FULL_WORLDSPACE_CELL_MAX,
                _FULL_WORLDSPACE_CELL_MAX,
                offset[0],
                offset[1],
                offset[2],
                worker_count=getattr(ctx, "conversion_workers", None),
            )
            timings = {
                str(name).removesuffix("_ms"): int(value) / 1000.0
                for name, value in dict(
                    result.get("orchestration_timing") or {}
                ).items()
            }
            seed_cache_hit = bool(result.get("seed_cache_hit"))
            converted = int(result.get("persistent_refs_converted", 0) or 0)
            skipped = int(result.get("persistent_refs_skipped", 0) or 0)
            total_converted += converted
            skip_reasons = dict(result.get("skip_reasons") or {})
            skip_reasons_str = ", ".join(
                f"{reason}={count}"
                for reason, count in sorted(
                    skip_reasons.items(), key=lambda kv: -int(kv[1])
                )
            )
            runner.emit_log(
                "INFO",
                "worldspace persistent cell synthesis: "
                f"worldspace={source_worldspace} "
                f"cell={result.get('persistent_cell_form_key', '')} "
                f"synthesized={bool(result.get('cell_synthesized'))} "
                f"converted={converted} skipped={skipped} "
                f"skip_reasons=[{skip_reasons_str}] "
                f"mapped_refs={int(result.get('mapped_form_refs', 0) or 0)} "
                f"reallocated={int(result.get('child_form_ids_reallocated', 0) or 0)} "
                f"dropped_subrecords={int(result.get('schema_subrecords_dropped', 0) or 0)}",
            )
            # Surface WHICH base each dropped persistent ref resolves to, so the
            # missing map markers (base 000010) can be identified. The histogram
            # is keyed on (base FormKey, reason) over the COMPLETE skip set;
            # log it sorted by count, bounded so a 130k-ref run stays readable.
            base_histogram = dict(result.get("skip_base_histogram") or {})
            if base_histogram:
                for key, count in sorted(
                    base_histogram.items(), key=lambda kv: -int(kv[1])
                )[:40]:
                    runner.emit_log(
                        "WARN",
                        "worldspace persistent cell skipped-ref base: "
                        f"worldspace={source_worldspace} {key} count={count}",
                    )
            # A bounded per-ref sample (source FormKey | base | reason) for the
            # exact dropped refs — pairs with the histogram for spot-checking.
            sample_children = [str(value) for value in result.get("skipped_children") or []]
            for entry in sample_children[:25]:
                runner.emit_log(
                    "WARN",
                    f"worldspace persistent cell skipped-ref: worldspace={source_worldspace} {entry}",
                )
            for warning in [str(value) for value in result.get("warnings") or []][:25]:
                runner.emit_log(
                    "WARN", f"worldspace persistent cell synthesis: {warning}"
                )
            runner.emit_log(
                "INFO",
                "persistent-cell timing: "
                f"worldspace={source_worldspace} "
                + " ".join(f"{name}={secs:.3f}s" for name, secs in timings.items())
                + f" seed_cache_hit={seed_cache_hit}",
            )

        runner.emit_log(
            "INFO",
            f"persistent-cell phase timing: btd_work_items={btd_secs:.3f}s "
            f"worldspaces={len(terrain_items)} converted={total_converted}",
        )
        progress.total_items = max(total_converted, 1)
        progress.completed_items = total_converted
        progress.status = "completed"
        runner.emit_item_progress(progress)


    def _sync_fo76_projected_cell_locations(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            raise RuntimeError("projected cell location sync requires a conversion run")
        result = load_native_module().conversion_run_sync_cell_locations_from_lctn(rust_run.id)
        locations = int(result.get("locations_indexed", 0) or 0)
        conflicts = int(result.get("location_conflicts", 0) or 0)
        changed = int(result.get("cells_changed", 0) or 0)
        retagged = int(result.get("cells_retagged", 0) or 0)
        already = int(result.get("cells_already_tagged", 0) or 0)
        progress.total_items = locations
        progress.completed_items = changed
        progress.status = "completed"
        runner.emit_item_progress(progress)
        runner.emit_log(
            "INFO",
            "projected cell location sync: "
            f"locations={locations} conflicts={conflicts} "
            f"changed={changed} retagged={retagged} already_tagged={already}",
        )
        for warning in [str(value) for value in result.get("warnings") or []][:25]:
            runner.emit_log("WARN", f"projected cell location sync: {warning}")


    def _repair_placed_child_refs(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        """Authoritatively resolve the deferred placed-ref-target class (LCTN
        LCUN/LCEP/ACEP) against the now-complete output plugin.

        Pairs with the pre-copy deferral (``defer_placed_child_ref_class``): the
        phase-2 fixups left those refs intact because their targets — exterior
        placed children — were not inserted until the phase-6 copy + cell-location
        sync. This runs after both, keeping refs whose target is now present and
        nulling (LCUN: dropping the row in lockstep) any still absent.
        """
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log(
                "WARN",
                "repair_placed_child_refs: no rust run; skipping",
            )
            progress.total_items = 0
            progress.completed_items = 0
            progress.status = "completed"
            runner.emit_item_progress(progress)
            return

        from bacup_lib.native_runtime import (
            load_native_module as _conv_native,
        )

        m = _conv_native()
        report = m.conversion_run_repair_placed_child_refs(rust_run.id)
        records_changed = int(report.get("records_changed", 0) or 0)
        ctx.summary.records_warnings += records_changed
        runner.emit_log(
            "INFO",
            f"repair_placed_child_refs: records_changed={records_changed}",
        )
        self._emit_run_warnings_inline(rust_run, runner, ctx)
        progress.total_items = max(records_changed, 1)
        progress.completed_items = progress.total_items
        progress.status = "completed"
        runner.emit_item_progress(progress)


    def _synthesize_encounter_zones(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        """Synthesize FO4 ECZN records from FO76 LCTN data, stamp CELL.XEZN on
        each zone's footprint cells, and normalize workshop LCTN keywords.

        FO76 has no ECZN; the encounter-zone role lives on LCTN (band, parent,
        LCEC footprint, keywords). Runs after the worldspace persistent-cell
        synthesis (so exterior grid cells exist as XEZN targets) and BEFORE the
        source handle closes (the native pass reads source LCTN). FO76->FO4 only;
        the native method no-ops otherwise.
        """
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log(
                "WARN",
                "synthesize_encounter_zones: no rust run; skipping",
            )
            progress.total_items = 0
            progress.completed_items = 0
            progress.status = "completed"
            runner.emit_item_progress(progress)
            return

        from bacup_lib.native_runtime import (
            load_native_module as _conv_native,
        )

        m = _conv_native()
        report = m.conversion_run_synthesize_encounter_zones(rust_run.id)
        records_changed = int(report.get("records_changed", 0) or 0)
        runner.emit_log(
            "INFO",
            f"synthesize_encounter_zones: records_changed={records_changed}",
        )
        self._emit_run_warnings_inline(rust_run, runner, ctx)
        progress.total_items = max(records_changed, 1)
        progress.completed_items = progress.total_items
        progress.status = "completed"
        runner.emit_item_progress(progress)

    def _synthesize_sky_regions(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        """Stamp CELL.XCCM (Sky/Weather from Region) on interior cells flagged
        Show-Sky that lost their sky source in translation.

        FO4 drives interior sky only from XCCM->REGN; FO76's FO76-only XISR
        (Interior Sky Override -> Weather) has no FO4 equivalent and is dropped.
        The native pass reads the dropped XISR weather back from the source CELL,
        so this MUST run before the early source close (right after encounter-zone
        synthesis). FO76->FO4 only; the native method no-ops otherwise.
        """
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log("WARN", "synthesize_sky_regions: no rust run; skipping")
            progress.total_items = 0
            progress.completed_items = 0
            progress.status = "completed"
            runner.emit_item_progress(progress)
            return

        from bacup_lib.native_runtime import (
            load_native_module as _conv_native,
        )

        m = _conv_native()
        report = m.conversion_run_synthesize_sky_regions(rust_run.id)
        records_changed = int(report.get("records_changed", 0) or 0)
        runner.emit_log(
            "INFO",
            f"synthesize_sky_regions: records_changed={records_changed}",
        )
        progress.total_items = max(records_changed, 1)
        progress.completed_items = progress.total_items
        progress.status = "completed"
        runner.emit_item_progress(progress)

    def _synthesize_vendor_dialogue(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        """Synthesize the B21_VendorDialogueFaction gate faction and enroll every
        NPC that belongs to a vendor faction (a FACT carrying VENC) into it.

        FO76/Skyrim vendors trade from the faction's Vendor flag alone; FO4 needs
        a "Let's trade" dialogue topic running the vanilla VendorInfoScript. That
        topic ships in the companion B21_VendorDialogue.esp gated on
        GetInFaction(B21_VendorDialogueFaction); this seeds that faction + its
        members into the output. Target-only (FACT/NPC_), so it runs AFTER the
        early source close and AFTER repair (which finalizes FACT.VENC), but
        BEFORE the mapper remap state is released. FO76->FO4 only.
        """
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log(
                "WARN", "synthesize_vendor_dialogue: no rust run; skipping"
            )
            progress.total_items = 0
            progress.completed_items = 0
            progress.status = "completed"
            runner.emit_item_progress(progress)
            return

        from bacup_lib.native_runtime import (
            load_native_module as _conv_native,
        )

        m = _conv_native()
        # Non-fatal: this is a vendor-trade enhancement, not core conversion. A
        # failure here must NOT abort an otherwise-complete (expensive) regen —
        # log loudly and continue; converted vendors just won't get trade dialogue.
        try:
            report = m.conversion_run_synthesize_vendor_dialogue(rust_run.id)
            records_changed = int(report.get("records_changed", 0) or 0)
            runner.emit_log(
                "INFO",
                f"synthesize_vendor_dialogue: records_changed={records_changed}",
            )
        except Exception as exc:  # noqa: BLE001 - degrade gracefully
            records_changed = 0
            runner.emit_log(
                "WARN",
                "synthesize_vendor_dialogue FAILED (non-fatal; converted vendors "
                f"will lack trade dialogue this run): {exc}",
            )
        progress.total_items = max(records_changed, 1)
        progress.completed_items = progress.total_items
        progress.status = "completed"
        runner.emit_item_progress(progress)

    def _patch_projected_worldspace_subrecords(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        source_plugin: Path,
    ) -> None:
        """Carry self-contained + remappable authored WRLD service subrecords
        from the FO76 source onto the built projected worldspace.

        The projected-children copy + scaffold/build_esp synthesize a bare WRLD
        (EDID/NAMA/DATA/NAM0/NAM9 only); without DNAM (land/water height) the FO4
        terrain renderer never activates the worldspace and without CNAM there is
        no climate — the worldspace-entry blocker. Called from the cleanup
        ``finally`` AFTER the rust run (+ its target handle) is dropped so the
        ESM is no longer memory-mapped (Windows os error 1224), and BEFORE the
        source handle closes since the carry reads the source WRLD. Best-effort:
        a failure here is logged, never raised, so it can't mask the real result.
        """
        opts = self._req.options
        if not (opts.build_esp and opts.convert_terrain):
            return
        reopened_source = None
        try:
            # The carry reads the source WRLD header subrecords. On the
            # memory-reduced terrain path the source handle was already freed
            # after Phase 7, so re-open the source lazily here.
            source_handle = getattr(ctx, "source_plugin_handle", None)
            if source_handle is None:
                if not source_plugin.is_file():
                    return
                from creation_lib.esp.plugin import Plugin

                source_handle = Plugin.load(
                    source_plugin,
                    game=self._req.source_game,
                    eager_compressed=False,
                )
                reopened_source = source_handle

            from bacup_lib.pipeline.terrain import fo76_btd_work_items
            from bacup_lib.worldspace_services import (
                patch_target_worldspace_subrecords,
            )

            try:
                terrain_items = fo76_btd_work_items(ctx)
            except FileNotFoundError:
                terrain_items = []
            if not terrain_items:
                return

            output_esp_path = Path(ctx.mod_path) / ctx.output_plugin_name
            for terrain, _btd_path, _worldspace_eid in terrain_items:
                source_worldspace = (
                    getattr(terrain, "source_worldspace_editor_id", "")
                    or getattr(terrain, "worldspace_editor_id", "")
                )
                if not source_worldspace:
                    continue
                copied = patch_target_worldspace_subrecords(
                    source_plugin=source_handle,
                    target_plugin_path=output_esp_path,
                    worldspace_editor_id=source_worldspace,
                    target_game=self._req.target_game,
                )
                _emit_best_effort_runner_log(
                    runner,
                    "INFO",
                    "projected worldspace subrecord carry: "
                    f"worldspace={source_worldspace} copied={copied}",
                )
        except Exception as exc:  # best-effort; never break the cleanup chain
            _emit_best_effort_runner_log(
                runner,
                "WARN",
                f"projected worldspace subrecord carry failed: {exc}",
            )
        finally:
            if reopened_source is not None:
                try:
                    reopened_source.close()
                except Exception:
                    pass

    def _repair_term_marker_parameters_final(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        source_plugin: Path,
    ) -> None:
        if not (
            self._req.source_game == "fo76"
            and self._req.target_game == "fo4"
            and self._req.options.build_esp
        ):
            return

        output_path = Path(ctx.mod_path) / ctx.output_plugin_name
        if not output_path.is_file():
            raise FileNotFoundError(
                f"TERM marker post-build repair could not find {output_path}"
            )

        source_handle = getattr(ctx, "source_plugin_handle", None)
        reopened_source = None
        if source_handle is None:
            source_handle = Plugin.load(
                source_plugin,
                game=self._req.source_game,
                eager_compressed=False,
            )
            reopened_source = source_handle

        from creation_lib.esp.plugin import replace_plugin_with_localized_sidecars

        temp_path = output_path.with_name(f"{output_path.name}.termrepair.tmp")
        target_handle = None
        audit_handle = None
        changes: list[dict[str, Any]] = []
        try:
            temp_path.unlink(missing_ok=True)
            target_handle = Plugin.load(output_path, game=self._req.target_game)
            changes = target_handle.repair_term_marker_parameters_from_source(source_handle)
            if changes:
                target_handle.save(temp_path, backend="native")
            target_handle.close()
            target_handle = None

            if changes:
                replace_plugin_with_localized_sidecars(temp_path, output_path)

            audit_handle = Plugin.load(output_path, game=self._req.target_game)
            remaining = audit_handle.repair_term_marker_parameters_from_source(
                source_handle, dry_run=True
            )
            if remaining:
                raise RuntimeError(
                    "TERM marker post-build audit found "
                    f"{len(remaining)} unrepaired record(s)"
                )
        finally:
            if target_handle is not None:
                target_handle.close()
            if audit_handle is not None:
                audit_handle.close()
            temp_path.unlink(missing_ok=True)
            if reopened_source is not None:
                reopened_source.close()

        runner.emit_log(
            "INFO",
            "TERM marker post-build repair: "
            f"modified={len(changes)} "
            f"removed={sum(change['removed'] for change in changes)} "
            f"inserted={sum(change['inserted'] for change in changes)} "
            "audit_modified=0",
        )


    def _run_convert_scripts_phase(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> None:
        include_all_source = _include_all_source_scripts(
            self._req.source_game, self._req.target_game
        )
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            if not include_all_source:
                runner.emit_log("WARN", "[Scripts] no target plugin handle; skipping")
                return
            refs: list[_ScriptReference] = []
            candidate_records: dict[int, object] = {}
            runner.emit_log(
                "INFO", "[Scripts] source-only run; no target plugin will be modified"
            )
        else:
            refs, candidate_records = self._collect_script_references(
                rust_run.id, runner
            )

        started = time.perf_counter()
        if not refs and not include_all_source:
            runner.emit_log("INFO", "[Scripts] no Papyrus script references found")
            return
        if not refs:
            runner.emit_log("INFO", "[Scripts] no Papyrus script references found")

        target_index = _build_target_pex_index(
            target_data_dir=self._req.target_data_dir,
            target_extracted_dir=self._req.target_extracted_dir,
            target_asset_store=getattr(ctx, "target_asset_store", None),
        )
        source_roots = _source_script_roots(
            self._req.source_data_dir,
            self._req.additional_source_asset_roots,
        )
        source_index = _build_pex_index(source_roots)

        resolutions: dict[str, _ScriptResolution] = {}
        source_script_names: list[str] = []
        script_names_by_key = {
            _script_key(ref.script_name): ref.script_name for ref in refs
        }
        if include_all_source:
            for key, path in source_index.items():
                if _skip_fo76_to_fo4_source_script(
                    key,
                    source_game=self._req.source_game,
                    target_game=self._req.target_game,
                ):
                    continue
                script_names_by_key.setdefault(
                    key, _script_name_from_indexed_pex(path, source_roots)
                )
            runner.emit_log(
                "INFO",
                f"[Scripts] including all {len(source_index)} indexed source script(s)",
            )
        _extend_script_names_with_ancestor_closure(
            script_names_by_key,
            source_index=source_index,
            target_index=target_index,
            runner=runner,
        )
        for script_name in sorted(script_names_by_key.values(), key=str.lower):
            key = _script_key(script_name)
            if _skip_fo76_to_fo4_source_script(
                key,
                source_game=self._req.source_game,
                target_game=self._req.target_game,
            ):
                for error in _remove_generated_script_outputs(
                    Path(ctx.mod_path), script_name
                ):
                    runner.emit_log(
                        "WARN",
                        f"[Scripts] failed to remove stale script output {error}",
                    )
                resolutions[key] = _ScriptResolution(script_name, "target")
                continue
            if key in target_index:
                target_pex = target_index[key]
                for error in _remove_generated_script_outputs(
                    Path(ctx.mod_path), script_name
                ):
                    runner.emit_log(
                        "WARN",
                        f"[Scripts] failed to remove stale script output {error}",
                    )
                resolutions[key] = _ScriptResolution(script_name, "target", target_pex)
                continue

            source_pex = source_index.get(key)
            if source_pex is None:
                for error in _remove_generated_script_outputs(
                    Path(ctx.mod_path), script_name
                ):
                    runner.emit_log(
                        "WARN",
                        f"[Scripts] failed to remove stale script output {error}",
                    )
                resolutions[key] = _ScriptResolution(script_name, "source_missing")
                continue

            source_script_names.append(script_name)

        script_workers = _script_worker_count(
            self._req.options.conversion_workers,
            len(source_script_names),
        )
        if source_script_names:
            runner.emit_log(
                "INFO",
                f"[Scripts] porting {len(source_script_names)} source script(s) "
                f"with {script_workers} worker(s)",
            )

        decompiled_script_names: list[str] = []
        for script_name, resolution in self._decompile_source_scripts_for_fo4(
            source_script_names,
            source_index=source_index,
            ctx=ctx,
            runner=runner,
            workers=script_workers,
        ):
            key = _script_key(script_name)
            if resolution is None:
                decompiled_script_names.append(script_name)
            else:
                resolutions[key] = resolution

        for script_name, resolution in self._compile_decompiled_scripts_for_fo4(
            decompiled_script_names,
            source_index=source_index,
            ctx=ctx,
            runner=runner,
            workers=script_workers,
        ):
            resolutions[_script_key(script_name)] = resolution

        for resolution in resolutions.values():
            if resolution.status in {
                "compile_failed",
                "compile_timeout",
                "compiler_unavailable",
            }:
                for error in _remove_generated_script_outputs(
                    Path(ctx.mod_path),
                    resolution.script_name,
                ):
                    runner.emit_log(
                        "WARN",
                        f"[Scripts] failed to remove stale script output {error}",
                    )

        failed_script_keys = {
            key for key, resolution in resolutions.items() if not resolution.ok
        }
        invalid_condition_keys: set[tuple[str, str]] = set()
        invalid_condition_notes: list[str] = []
        inferred_condition_checks: dict[
            tuple[int, str], list[tuple[_ScriptReference, bool, tuple[str, str] | None]]
        ] = {}
        for ref in refs:
            if ref.kind != "condition" or not ref.variable_name:
                continue
            script_key = _script_key(ref.script_name)
            resolution = resolutions.get(script_key)
            if resolution is not None and resolution.status == "target":
                if ref.condition_inferred:
                    inferred_condition_checks.setdefault(
                        (ref.form_id, _variable_key(ref.variable_name)),
                        [],
                    ).append((ref, True, None))
                continue
            if resolution is None or not resolution.ok or resolution.pex_path is None:
                if ref.condition_inferred:
                    inferred_condition_checks.setdefault(
                        (ref.form_id, _variable_key(ref.variable_name)),
                        [],
                    ).append((ref, False, None))
                continue
            try:
                if _script_has_variable(resolution.pex_path, ref.variable_name):
                    if ref.condition_inferred:
                        inferred_condition_checks.setdefault(
                            (ref.form_id, _variable_key(ref.variable_name)),
                            [],
                        ).append((ref, True, None))
                    continue
            except Exception as exc:
                failed_script_keys.add(script_key)
                resolutions[script_key] = _ScriptResolution(
                    script_name=ref.script_name,
                    status="parse_failed",
                    pex_path=resolution.pex_path,
                    message=str(exc),
                )
                if ref.condition_inferred:
                    inferred_condition_checks.setdefault(
                        (ref.form_id, _variable_key(ref.variable_name)),
                        [],
                    ).append((ref, False, None))
                continue
            condition_key = (script_key, _variable_key(ref.variable_name))
            if ref.condition_inferred:
                inferred_condition_checks.setdefault(
                    (ref.form_id, _variable_key(ref.variable_name)),
                    [],
                ).append((ref, False, condition_key))
                continue
            invalid_condition_keys.add(condition_key)
            invalid_condition_notes.append(
                f"{ref.record_sig} {ref.editor_id or ref.form_key}: "
                f"{ref.script_name}.{ref.variable_name}"
            )

        for checks in inferred_condition_checks.values():
            if any(has_variable for _ref, has_variable, _condition_key in checks):
                continue
            note_ref: _ScriptReference | None = None
            for ref, _has_variable, condition_key in checks:
                note_ref = note_ref or ref
                if condition_key is not None:
                    invalid_condition_keys.add(condition_key)
            if note_ref is not None:
                invalid_condition_notes.append(
                    f"{note_ref.record_sig} {note_ref.editor_id or note_ref.form_key}: "
                    f"{note_ref.variable_name} not found on any script for condition form"
                )

        vmad_strip_form_ids = {
            ref.form_id
            for ref in refs
            if ref.kind == "vmad" and _script_key(ref.script_name) in failed_script_keys
        }
        if rust_run is None:
            stripped_conditions = 0
            stripped_vmad = 0
            changed_records = 0
        else:
            stripped_conditions, stripped_vmad, changed_records = (
                self._strip_failed_script_refs(
                    rust_run.id,
                    candidate_records,
                    failed_script_keys=failed_script_keys,
                    invalid_condition_keys=invalid_condition_keys,
                    vmad_strip_form_ids=vmad_strip_form_ids,
                )
            )

        for note in invalid_condition_notes[:_SCRIPT_WARNING_LIMIT]:
            runner.emit_log(
                "WARN", f"[Scripts] target script variable missing; stripped {note}"
            )
        extra_invalid = len(invalid_condition_notes) - _SCRIPT_WARNING_LIMIT
        if extra_invalid > 0:
            runner.emit_log(
                "WARN",
                f"[Scripts] suppressed {extra_invalid} additional missing-variable logs",
            )

        failed = [r for r in resolutions.values() if not r.ok]
        for resolution in failed[:_SCRIPT_WARNING_LIMIT]:
            detail = f": {resolution.message}" if resolution.message else ""
            runner.emit_log(
                "WARN",
                f"[Scripts] {resolution.status} for {resolution.script_name}{detail}",
            )
        extra_failed = len(failed) - _SCRIPT_WARNING_LIMIT
        if extra_failed > 0:
            runner.emit_log(
                "WARN",
                f"[Scripts] suppressed {extra_failed} additional script failure logs",
            )

        compiled = sum(1 for r in resolutions.values() if r.status == "compiled")
        target_existing = sum(1 for r in resolutions.values() if r.status == "target")
        ctx.summary.scripts_flagged += len(failed) + len(invalid_condition_keys)
        self._write_script_port_report(
            ctx,
            resolutions=resolutions,
            invalid_condition_notes=invalid_condition_notes,
            stripped_conditions=stripped_conditions,
            stripped_vmad=stripped_vmad,
            changed_records=changed_records,
        )
        _record_timing(
            ctx,
            "convert_scripts",
            started,
            references=len(refs),
            scripts=len(resolutions),
            compiled=compiled,
            failed=len(failed),
            stripped_conditions=stripped_conditions,
            stripped_vmad=stripped_vmad,
        )
        runner.emit_log(
            "INFO",
            "[Scripts] done: "
            f"refs={len(refs)} scripts={len(resolutions)} "
            f"target={target_existing} compiled={compiled} failed={len(failed)} "
            f"stripped_conditions={stripped_conditions} stripped_vmad={stripped_vmad}",
        )


    def _collect_script_references(
        self,
        run_id: int,
        runner: "ConversionRunner",
    ) -> tuple[list[_ScriptReference], dict[int, Any]]:
        plugin_name, native_rows = (
            load_native_module().conversion_run_script_reference_records(
                run_id, list(_SCRIPT_REF_SUBRECORD_SIGNATURES)
            )
        )
        plugin_name = str(plugin_name or "")
        refs: list[_ScriptReference] = []
        candidate_records: dict[int, Any] = {}
        seen_refs: set[tuple[str, str, int, str]] = set()
        records: list[tuple[Any, list[tuple[str, bytes, str | None]], str]] = []
        vmad_refs_by_form_id: dict[int, list[_ScriptReference]] = {}
        scripts_by_form_id: dict[int, list[str]] = {}

        for form_id, signature, editor_id, subrecords, authoring_text in native_rows:
            record = _record_from_native_subrecords(
                form_id=int(form_id) & 0xFFFFFFFF,
                signature=str(signature),
                editor_id=editor_id,
                subrecords=subrecords,
            )
            form_key = _script_ref_form_key(record.form_id, plugin_name=plugin_name)
            records.append((record, subrecords, form_key))
            script_refs_for_record: list[_ScriptReference] = []

            if any(sig == "VMAD" for sig, _data, _semantic_type in subrecords):
                if authoring_text:
                    try:
                        script_refs_for_record.extend(
                            _iter_vmad_script_refs(
                                json.loads(authoring_text),
                                record,
                                form_key=form_key,
                            )
                        )
                    except (TypeError, json.JSONDecodeError) as exc:
                        runner.emit_log(
                            "WARN",
                            f"[Scripts] failed to inspect VMAD for {form_key}: {exc}",
                        )

            if script_refs_for_record:
                vmad_refs_by_form_id[record.form_id] = script_refs_for_record
                script_names = [ref.script_name for ref in script_refs_for_record]
                for key in _form_id_lookup_keys(record.form_id):
                    scripts_by_form_id[key] = script_names

        self._script_condition_form_scripts = scripts_by_form_id

        for record, _subrecords, form_key in records:
            script_refs_for_record = _iter_condition_script_refs(
                record,
                form_key=form_key,
                scripts_by_form_id=scripts_by_form_id,
            )
            script_refs_for_record.extend(vmad_refs_by_form_id.get(record.form_id, []))
            if not script_refs_for_record:
                continue
            candidate_records[record.form_id] = record
            for ref in script_refs_for_record:
                key = (
                    _script_key(ref.script_name),
                    _variable_key(ref.variable_name),
                    ref.form_id,
                    ref.kind,
                )
                if key in seen_refs:
                    continue
                seen_refs.add(key)
                refs.append(ref)

        runner.emit_log(
            "INFO",
            f"[Scripts] discovered {len(refs)} Papyrus script reference(s) "
            f"across {len(candidate_records)} record(s)",
        )
        return refs, candidate_records


    def _decompile_source_scripts_for_fo4(
        self,
        script_names: list[str],
        *,
        source_index: dict[str, Path],
        ctx: ConversionContext,
        runner: "ConversionRunner",
        workers: int,
    ) -> list[tuple[str, _ScriptResolution | None]]:
        if not script_names:
            return []

        def run_one(script_name: str) -> tuple[str, _ScriptResolution | None]:
            try:
                source_pex = source_index[_script_key(script_name)]
                return script_name, self._decompile_script_source_for_fo4(
                    script_name,
                    source_pex,
                    ctx,
                    runner,
                )
            except Exception as exc:
                return script_name, _ScriptResolution(
                    script_name,
                    "decompile_failed",
                    message=str(exc),
                )

        if workers <= 1:
            return [run_one(script_name) for script_name in script_names]
        with ThreadPoolExecutor(
            max_workers=workers, thread_name_prefix="script-decompile"
        ) as pool:
            return list(pool.map(run_one, script_names))


    def _compile_decompiled_scripts_for_fo4(
        self,
        script_names: list[str],
        *,
        source_index: dict[str, Path],
        ctx: ConversionContext,
        runner: "ConversionRunner",
        workers: int,
    ) -> list[tuple[str, _ScriptResolution]]:
        if not script_names:
            return []

        selector = getattr(self._req.options, "papyrus_compiler", "native")
        if selector == "exe-batch":
            return self._compile_decompiled_scripts_batch_for_fo4(
                script_names, ctx=ctx, runner=runner
            )
        if selector == "native":
            return self._compile_decompiled_scripts_native_for_fo4(
                script_names, ctx=ctx, runner=runner, workers=workers
            )

        def run_one(script_name: str) -> tuple[str, _ScriptResolution]:
            try:
                source_pex = source_index.get(
                    _script_key(script_name),
                    Path(ctx.mod_path)
                    / "Scripts"
                    / "Source"
                    / "User"
                    / _script_relative_path(script_name, ".psc"),
                )
                return script_name, self._compile_decompiled_script_for_fo4(
                    script_name,
                    source_pex,
                    ctx,
                    runner,
                    cleanup_on_failure=False,
                )
            except Exception as exc:
                return script_name, _ScriptResolution(
                    script_name,
                    "compile_failed",
                    message=str(exc),
                )

        if workers <= 1:
            return [run_one(script_name) for script_name in script_names]
        with ThreadPoolExecutor(
            max_workers=workers, thread_name_prefix="script-compile"
        ) as pool:
            return list(pool.map(run_one, script_names))


    def _compile_decompiled_scripts_native_for_fo4(
        self,
        script_names: list[str],
        *,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        workers: int,
    ) -> list[tuple[str, _ScriptResolution]]:
        from creation_lib.core.game_profiles import get_profile
        from creation_lib.pex.native_runtime import compile_psc

        psc_root = Path(ctx.mod_path) / "Scripts" / "Source" / "User"
        output_root = Path(ctx.mod_path) / "data" / "Scripts"
        output_root.mkdir(parents=True, exist_ok=True)

        def _all(status: str, message: str) -> list[tuple[str, _ScriptResolution]]:
            return [(n, _ScriptResolution(n, status, message=message)) for n in script_names]

        profile = get_profile(self._req.target_game)
        imports = [psc_root]
        base_dirs: list[Path] = []
        for root in (self._req.target_data_dir,):
            if root is None:
                continue
            for source_root in (
                root / "Scripts" / "Source",
                root / "scripts" / "source",
            ):
                game_user = source_root / "User"
                game_base = source_root / "Base"
                if game_user.is_dir():
                    imports.append(game_user)
                if game_base.is_dir():
                    imports.append(game_base)
                    base_dirs.append(game_base)
        if len(imports) == 1:
            return _all("compiler_unavailable", "target script source dir not configured")
        imports = _dedupe_paths(imports)
        import_args = [str(path) for path in imports]
        flags_arg = None
        if profile.papyrus_flags:
            flags_arg = profile.papyrus_flags
            for game_base in base_dirs:
                flags_path = game_base / profile.papyrus_flags
                if flags_path.is_file():
                    flags_arg = str(flags_path)
                    break

        runner.emit_log(
            "INFO",
            f"[Scripts] native-compiling {len(script_names)} script(s)",
        )

        def run_one(script_name: str) -> tuple[str, _ScriptResolution]:
            psc_path = psc_root / _script_relative_path(script_name, ".psc")
            output_pex = output_root / _script_relative_path(script_name, ".pex")
            if output_pex.is_file():
                try:
                    output_pex.unlink()
                except OSError as exc:
                    return script_name, _ScriptResolution(
                        script_name,
                        "compile_failed",
                        message=f"could not remove stale output {output_pex}: {exc}",
                    )
            if not psc_path.is_file():
                return script_name, _ScriptResolution(
                    script_name,
                    "compile_failed",
                    message=f"missing source {psc_path}",
                )
            try:
                result = compile_psc(
                    psc_path.read_text(encoding="utf-8"),
                    imports=import_args,
                    game=self._req.target_game.lower(),
                    flags=flags_arg,
                    source_path=str(psc_path),
                )
            except Exception as exc:
                return script_name, _ScriptResolution(
                    script_name,
                    "compile_failed",
                    message=str(exc),
                )
            if not result.ok or result.pex_bytes is None:
                return script_name, _ScriptResolution(
                    script_name,
                    "compile_failed",
                    message=_native_papyrus_diagnostics_message(result.diagnostics),
                )
            try:
                output_pex.parent.mkdir(parents=True, exist_ok=True)
                output_pex.write_bytes(result.pex_bytes)
            except OSError as exc:
                return script_name, _ScriptResolution(
                    script_name,
                    "compile_failed",
                    message=str(exc),
                )
            return script_name, _ScriptResolution(script_name, "compiled", output_pex)

        if workers <= 1:
            return [run_one(script_name) for script_name in script_names]
        with ThreadPoolExecutor(
            max_workers=workers, thread_name_prefix="script-compile-native"
        ) as pool:
            return list(pool.map(run_one, script_names))


    def _compile_decompiled_scripts_batch_for_fo4(
        self,
        script_names: list[str],
        *,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> list[tuple[str, _ScriptResolution]]:
        """Compile every decompiled .psc in one PapyrusCompiler.exe -all process.

        One CLR instead of W concurrent ones bounds RAM.
        Per-script _ScriptResolution is reconstructed from which output .pex the
        batch run produced, preserving the downstream failed-ref stripping in
        _run_convert_scripts_phase.

        -all compiles every .psc under the User root, so the caller must ensure
        that tree contains only this run's decompiled scripts (stray .psc would be
        recompiled; the per-script .pex pre-delete above bounds the correctness
        impact to wasted work).
        """
        from creation_lib.core.game_profiles import get_profile

        psc_root = Path(ctx.mod_path) / "Scripts" / "Source" / "User"
        output_root = Path(ctx.mod_path) / "data" / "Scripts"
        output_root.mkdir(parents=True, exist_ok=True)

        def _all(status: str, message: str) -> list[tuple[str, _ScriptResolution]]:
            return [(n, _ScriptResolution(n, status, message=message)) for n in script_names]

        target_data_dir = self._req.target_data_dir
        if target_data_dir is None:
            return _all("compiler_unavailable", "target data dir not configured")
        profile = get_profile(self._req.target_game)
        if not profile.papyrus_compiler_dir:
            return _all("compiler_unavailable", "target game has no Papyrus compiler")
        compiler = (
            target_data_dir.parent / profile.papyrus_compiler_dir / "PapyrusCompiler.exe"
        )
        if not compiler.is_file():
            return _all("compiler_unavailable", str(compiler))

        # Pre-delete each target .pex so post-compile presence is a true success
        # signal. mod_path is reused across runs; without this a stale .pex from a
        # prior run would mask a real compile failure as "compiled" (the per-script
        # path is guarded by the subprocess returncode, which -all cannot give us).
        for script_name in script_names:
            stale_pex = output_root / _script_relative_path(script_name, ".pex")
            if stale_pex.is_file():
                try:
                    stale_pex.unlink()
                except OSError:
                    pass

        imports = [psc_root]
        game_user = target_data_dir / "Scripts" / "Source" / "User"
        game_base = target_data_dir / "Scripts" / "Source" / "Base"
        if game_user.is_dir():
            imports.append(game_user)
        if game_base.is_dir():
            imports.append(game_base)
        import_arg = ";".join(str(path) for path in imports)

        cmd = [
            str(compiler),
            str(psc_root),
            "-all",
            f"-import={import_arg}",
            f"-output={output_root}",
            "-quiet",
        ]
        if profile.papyrus_flags:
            flags_path = game_base / profile.papyrus_flags
            flags_value = flags_path if flags_path.is_file() else profile.papyrus_flags
            cmd.append(f"-flags={flags_value}")

        runner.emit_log(
            "INFO",
            f"[Scripts] batch-compiling {len(script_names)} script(s) in one process",
        )
        # Ceiling scales with batch size; a hung compiler still cannot block forever.
        timeout_seconds = 1800 + len(script_names) * 2
        try:
            completed = subprocess.run(
                cmd,
                cwd=str(psc_root),
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=timeout_seconds,
            )
        except subprocess.TimeoutExpired:
            return _all("compile_timeout", "batch compile timeout")

        out_text = (completed.stdout or "").strip()
        if out_text:
            for line in out_text.splitlines():
                if line.strip():
                    runner.emit_log("INFO", f"[compile] {line.rstrip()}")

        results: list[tuple[str, _ScriptResolution]] = []
        for script_name in script_names:
            output_pex = output_root / _script_relative_path(script_name, ".pex")
            if output_pex.is_file():
                results.append(
                    (script_name, _ScriptResolution(script_name, "compiled", output_pex))
                )
            else:
                results.append(
                    (
                        script_name,
                        _ScriptResolution(
                            script_name,
                            "compile_failed",
                            message="no output (see batch compile log)",
                        ),
                    )
                )
        return results


    def _decompile_script_source_for_fo4(
        self,
        script_name: str,
        source_pex: Path,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> _ScriptResolution | None:
        from creation_lib.pex import decompile_pex

        psc_root = Path(ctx.mod_path) / "Scripts" / "Source" / "User"
        psc_path = psc_root / _script_relative_path(script_name, ".psc")
        psc_path.parent.mkdir(parents=True, exist_ok=True)
        type_adapter = None
        drop_script_const = False
        skip_internal_functions = False
        fo4_api_compat = False
        if (
            self._req.source_game.lower() == "fo76"
            and self._req.target_game.lower() == "fo4"
        ):
            type_adapter = _fo76_to_fo4_script_type
            drop_script_const = True
            skip_internal_functions = True
            fo4_api_compat = True
        try:
            decompiled = decompile_pex(
                source_pex,
                type_adapter=type_adapter,
                drop_script_const=drop_script_const,
                skip_internal_functions=skip_internal_functions,
                fo4_api_compat=fo4_api_compat,
            )
            if fo4_api_compat:
                decompiled = _augment_fo76_to_fo4_script_skeleton(
                    script_name, decompiled
                )
        except Exception as exc:
            for error in _remove_generated_script_outputs(
                Path(ctx.mod_path), script_name
            ):
                runner.emit_log(
                    "WARN", f"[Scripts] failed to remove stale script output {error}"
                )
            return _ScriptResolution(script_name, "decompile_failed", message=str(exc))

        # A fix-folder patch supplies just the method/event bodies; splice them
        # into the decompiled skeleton, keeping the game-derived declarations. If
        # there is no patch, warn when the FO76 script decompiled with no logic so
        # a modder (or AI) knows it needs one.
        patch_source = _script_patch_source(script_name)
        if patch_source is not None:
            decompiled = _merge_script_method_patches(decompiled, patch_source)
            runner.emit_log(
                "INFO", f"[Scripts] merged fix-folder method patch into {script_name}"
            )
        elif self._req.source_game.lower() == "fo76" and _script_body_is_hollow(
            decompiled
        ):
            runner.emit_log(
                "WARN",
                f"[Scripts] {script_name} decompiled with no logic (FO76 server-side "
                f"stub); add an original {_script_relative_path(script_name, '.psc')} "
                f"to conversion/script_patches/ to implement it",
            )
        psc_path.write_text(decompiled, encoding="utf-8")
        return None


    def _compile_decompiled_script_for_fo4(
        self,
        script_name: str,
        source_pex: Path,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        *,
        cleanup_on_failure: bool,
    ) -> _ScriptResolution:
        from creation_lib.core.game_profiles import get_profile

        psc_root = Path(ctx.mod_path) / "Scripts" / "Source" / "User"
        output_root = Path(ctx.mod_path) / "data" / "Scripts"
        output_pex = output_root / _script_relative_path(script_name, ".pex")
        output_pex.parent.mkdir(parents=True, exist_ok=True)
        target_data_dir = self._req.target_data_dir
        if target_data_dir is None:
            if cleanup_on_failure:
                for error in _remove_generated_script_outputs(
                    Path(ctx.mod_path), script_name
                ):
                    runner.emit_log(
                        "WARN",
                        f"[Scripts] failed to remove stale script output {error}",
                    )
            return _ScriptResolution(
                script_name,
                "compiler_unavailable",
                message="target data dir not configured",
            )
        profile = get_profile(self._req.target_game)
        if not profile.papyrus_compiler_dir:
            if cleanup_on_failure:
                for error in _remove_generated_script_outputs(
                    Path(ctx.mod_path), script_name
                ):
                    runner.emit_log(
                        "WARN",
                        f"[Scripts] failed to remove stale script output {error}",
                    )
            return _ScriptResolution(
                script_name,
                "compiler_unavailable",
                message="target game has no Papyrus compiler",
            )
        compiler = (
            target_data_dir.parent
            / profile.papyrus_compiler_dir
            / "PapyrusCompiler.exe"
        )
        if not compiler.is_file():
            if cleanup_on_failure:
                for error in _remove_generated_script_outputs(
                    Path(ctx.mod_path), script_name
                ):
                    runner.emit_log(
                        "WARN",
                        f"[Scripts] failed to remove stale script output {error}",
                    )
            return _ScriptResolution(
                script_name, "compiler_unavailable", message=str(compiler)
            )

        imports = [psc_root]
        game_user = target_data_dir / "Scripts" / "Source" / "User"
        game_base = target_data_dir / "Scripts" / "Source" / "Base"
        if game_user.is_dir():
            imports.append(game_user)
        if game_base.is_dir():
            imports.append(game_base)
        import_arg = ";".join(str(path) for path in imports)

        rel_arg = _script_relative_path(script_name, "").as_posix()
        cmd = [
            str(compiler),
            rel_arg,
            f"-import={import_arg}",
            f"-output={output_root}",
            "-quiet",
        ]
        if profile.papyrus_flags:
            flags_path = game_base / profile.papyrus_flags
            flags_value = flags_path if flags_path.is_file() else profile.papyrus_flags
            cmd.append(f"-flags={flags_value}")

        try:
            completed = subprocess.run(
                cmd,
                cwd=str(psc_root),
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=60,
            )
        except subprocess.TimeoutExpired:
            if cleanup_on_failure:
                for error in _remove_generated_script_outputs(
                    Path(ctx.mod_path), script_name
                ):
                    runner.emit_log(
                        "WARN",
                        f"[Scripts] failed to remove stale script output {error}",
                    )
            return _ScriptResolution(
                script_name, "compile_timeout", message=str(source_pex)
            )

        out_text = (completed.stdout or "").strip()
        if completed.returncode != 0 or not output_pex.is_file():
            if cleanup_on_failure:
                for error in _remove_generated_script_outputs(
                    Path(ctx.mod_path), script_name
                ):
                    runner.emit_log(
                        "WARN",
                        f"[Scripts] failed to remove stale script output {error}",
                    )
            return _ScriptResolution(
                script_name,
                "compile_failed",
                message=out_text.splitlines()[-1]
                if out_text
                else f"exit={completed.returncode}",
            )
        return _ScriptResolution(script_name, "compiled", output_pex)


    def _strip_failed_script_refs(
        self,
        run_id: int,
        candidate_records: dict[int, Any],
        *,
        failed_script_keys: set[str],
        invalid_condition_keys: set[tuple[str, str]],
        vmad_strip_form_ids: set[int],
    ) -> tuple[int, int, int]:
        stripped_conditions = 0
        stripped_vmad = 0
        changed_records = 0
        for form_id, record in candidate_records.items():
            payloads, condition_count, vmad_count = (
                _subrecord_payloads_after_script_strip(
                    record,
                    failed_script_keys=failed_script_keys,
                    invalid_condition_keys=invalid_condition_keys,
                    strip_vmad=form_id in vmad_strip_form_ids,
                    scripts_by_form_id=getattr(
                        self,
                        "_script_condition_form_scripts",
                        None,
                    ),
                )
            )
            if condition_count == 0 and vmad_count == 0:
                continue
            subrecords = [
                (payload["signature"], payload["data"], None) for payload in payloads
            ]
            updated = load_native_module().conversion_run_set_record_subrecords(
                run_id,
                form_id,
                subrecords,
            )
            if not updated:
                continue
            changed_records += 1
            stripped_conditions += condition_count
            stripped_vmad += vmad_count
        return stripped_conditions, stripped_vmad, changed_records

    @staticmethod

    def _write_script_port_report(
        ctx: ConversionContext,
        *,
        resolutions: dict[str, _ScriptResolution],
        invalid_condition_notes: list[str],
        stripped_conditions: int,
        stripped_vmad: int,
        changed_records: int,
    ) -> None:
        report_root = Path(getattr(ctx, "diagnostics_root", None) or ctx.mod_path)
        report_root.mkdir(parents=True, exist_ok=True)
        report_path = report_root / "script_port_report.json"
        payload = {
            "scripts": [
                {
                    "script_name": resolution.script_name,
                    "status": resolution.status,
                    "pex_path": str(resolution.pex_path)
                    if resolution.pex_path
                    else None,
                    "message": resolution.message,
                }
                for resolution in sorted(
                    resolutions.values(),
                    key=lambda item: item.script_name.lower(),
                )
            ],
            "invalid_conditions": invalid_condition_notes,
            "stripped_conditions": stripped_conditions,
            "stripped_vmad": stripped_vmad,
            "changed_records": changed_records,
        }
        report_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


    def _run_optional_fnv_legacy_phase(
        self,
        ctx: ConversionContext,
        source_plugin: Path,
        runner: "ConversionRunner",
    ) -> bool:
        if self._req.source_game != "fnv" or self._req.target_game != "fo4":
            return False
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            return False
        self._run_phase(
            3,
            "Translate FNV Legacy Scripting",
            lambda p: self._run_fnv_legacy_phase_from_rust_run(
                ctx, source_plugin, runner, rust_run
            ),
            runner,
            timing_ctx=ctx,
            raise_on_error=(self._req.options.fnv_unmapped_function_policy == "halt"),
        )
        return True


    def _apply_fnv_legacy_result_dict(
        self,
        result_dict: dict,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> None:
        translated_scripts = int(result_dict.get("translated_scripts", 0))
        translated_quests = int(result_dict.get("translated_quests", 0))
        translated_infos = int(result_dict.get("translated_infos", 0))
        translated_scenes = int(result_dict.get("translated_scenes", 0))
        skipped_records = result_dict.get("skipped_records", []) or []
        records_written = int(result_dict.get("records_written", 0))
        records_failed = int(result_dict.get("records_failed", 0))
        psc_files_written = int(result_dict.get("psc_files_written", 0))
        psc_files_skipped = int(result_dict.get("psc_files_skipped", 0))
        lip_paths = result_dict.get("lip_regeneration_needed", []) or []

        vmad_intents = result_dict.get("vmad_intents") or []
        if vmad_intents and not bool(result_dict.get("vmad_attached_in_rust", False)):
            runner.emit_log(
                "WARN",
                "FNV legacy scripting returned VMAD intents without native attachment; "
                "Python record mutation fallback has been removed",
            )

        ctx.summary.records_translated += records_written
        ctx.summary.records_warnings += records_failed + len(skipped_records)
        _append_unique_strings(ctx.summary.lip_regeneration_needed, list(lip_paths))

        runner.emit_log(
            "INFO",
            "FNV legacy scripting: translated "
            f"{translated_scripts} SCPT, "
            f"{translated_quests} QUST, "
            f"{translated_infos} INFO, "
            f"{translated_scenes} SCEN; "
            f"written={records_written} failed={records_failed}; "
            f"psc={psc_files_written}/{psc_files_written + psc_files_skipped}; "
            f"skipped {len(skipped_records)}",
        )


    def _run_fnv_legacy_phase_from_rust_run(
        self,
        ctx: ConversionContext,
        source_plugin: Path,
        runner: "ConversionRunner",
        rust_run,
    ) -> None:
        mod_prefix = Path(ctx.mod_path).name if getattr(ctx, "mod_path", None) else ""
        mod_path_str = str(getattr(ctx, "mod_path", "") or "")
        from bacup_lib.native_runtime import (
            load_native_module as _conv_native,
        )

        result_dict = _conv_native().conversion_run_fnv_legacy_scripting_from_run(
            rust_run.id,
            mod_prefix,
            source_plugin.name,
            mod_path_str,
        )
        self._apply_fnv_legacy_result_dict(result_dict, ctx, runner)

    @staticmethod

    def _params_for_convert_creatures(ctx: ConversionContext) -> dict | None:
        return {"target_creature_archetype": ""}


    def _run_convert_creatures_phase(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> None:
        """Dispatch the Rust convert_creatures phase (observability only).

        No-op when no Rust run is live or the source plugin lacks NPC_ records.
        Reports the catalog-classified creature count via runner.emit_log.
        """
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            return
        params = self._params_for_convert_creatures(ctx)
        if params is None:
            return
        try:
            report = rust_run.run_phase(
                "convert_creatures",
                mod_path=str(ctx.mod_path),
                params=params,
            )
        except Exception as exc:
            runner.emit_log(
                "WARN", f"convert_creatures phase failed (non-fatal): {exc}"
            )
            return
        _timing_report(ctx).record(
            "native_phase",
            float(report.get("elapsed_ms", 0) or 0) / 1000.0,
            phase_name="convert_creatures",
            changed=report.get("records_changed", 0),
            warnings=report.get("warnings", 0),
        )
        runner.emit_log(
            "INFO",
            f"[Creatures] native phase done: classified={report.get('records_changed', 0)}, "
            f"warnings={report.get('warnings', 0)}, elapsed_ms={report.get('elapsed_ms', 0)}",
        )

    @staticmethod

    def _params_for_convert_equipment(ctx: ConversionContext) -> dict | None:
        mod_prefix = ctx.mod_path.name if getattr(ctx, "mod_path", None) else ""
        if not mod_prefix:
            return None
        return {
            "addon_index_start": 20000,
            "mod_prefix": mod_prefix,
        }


    def _run_convert_equipment_phase(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> None:
        """Dispatch the Rust convert_equipment phase.

        The current native phase reports synthetic equipment counts only; any
        future equipment record synthesis must be implemented in Rust.
        """
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            return
        params = self._params_for_convert_equipment(ctx)
        if params is None:
            return
        try:
            report = rust_run.run_phase(
                "convert_equipment",
                mod_path=str(ctx.mod_path),
                params=params,
            )
        except Exception as exc:
            runner.emit_log(
                "WARN", f"convert_equipment phase failed (non-fatal): {exc}"
            )
            return
        _timing_report(ctx).record(
            "native_phase",
            float(report.get("elapsed_ms", 0) or 0) / 1000.0,
            phase_name="convert_equipment",
            added=report.get("records_added", 0),
            warnings=report.get("warnings", 0),
        )
        runner.emit_log(
            "INFO",
            f"[Equipment] native phase done: synthetic_records={report.get('records_added', 0)}, "
            f"warnings={report.get('warnings', 0)}, elapsed_ms={report.get('elapsed_ms', 0)}",
        )


    def _run_convert_face_phase(
        self,
        ctx: "ConversionContext",
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        """Dispatch the Rust convert_face phase."""
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log("WARN", "convert_face: no rust run; skipping")
            return

        _face_res = native_face_resources_dir()
        source_extracted = str(
            getattr(ctx, "source_extracted_dir", None)
            or getattr(ctx, "extracted_dir", None)
            or ""
        )
        target_extracted = str(self._req.target_extracted_dir or "")
        target_face_params: dict[str, str] = {}
        target_asset_store = getattr(ctx, "target_asset_store", None)
        if target_asset_store is not None:
            head_candidates = {
                "male": (
                    "Meshes/Actors/Character/CharacterAssets/BaseMaleHead.nif",
                    "Meshes/Actors/Character/CharacterAssets/FaceParts/MaleHead.nif",
                ),
                "female": (
                    "Meshes/Actors/Character/CharacterAssets/BaseFemaleHead.nif",
                    "Meshes/Actors/Character/CharacterAssets/FaceParts/FemaleHead.nif",
                ),
            }
            bones_candidates = {
                "male": (
                    "Meshes/Actors/Character/CharacterAssets/BaseMaleHead_faceBones.nif",
                ),
                "female": (
                    "Meshes/Actors/Character/CharacterAssets/BaseFemaleHead_faceBones.nif",
                ),
            }
            for sex, candidates in head_candidates.items():
                for candidate in candidates:
                    if not target_asset_store.has_asset(candidate):
                        continue
                    head = target_asset_store.materialize(candidate)
                    tri = target_asset_store.materialize(str(Path(candidate).with_suffix(".tri")))
                    if head is not None and tri is not None:
                        target_face_params[f"target_base_head_{sex}"] = str(head)
                        break
            for sex, candidates in bones_candidates.items():
                for candidate in candidates:
                    bones = target_asset_store.materialize(candidate)
                    if bones is not None:
                        target_face_params[f"target_face_bones_{sex}"] = str(bones)
                        break
        output_plugin_name = (
            getattr(ctx, "output_plugin_name", "Output.esp") or "Output.esp"
        )
        mod_path = str(getattr(ctx, "mod_path", "") or "")

        rust_run.run_phase(
            "convert_face",
            mod_path=mod_path,
            source_extracted_dir=source_extracted,
            target_extracted_dir=target_extracted,
            params={
                "correspondence_path_male": str(
                    _face_res / "fnv_to_fo4_correspondence_male.npz"
                ),
                "correspondence_path_female": str(
                    _face_res / "fnv_to_fo4_correspondence_female.npz"
                ),
                "uv_lut_path_male": str(
                    _face_res / "fnv_to_fo4_facetint_uv_lut_male.npz"
                ),
                "uv_lut_path_female": str(
                    _face_res / "fnv_to_fo4_facetint_uv_lut_female.npz"
                ),
                "output_plugin_name": output_plugin_name,
                **target_face_params,
            },
        )


    def _run_phase(
        self,
        phase_num: int,
        phase_name: str,
        phase_fn,
        runner: "ConversionRunner",
        *,
        timing_ctx: ConversionContext | None = None,
        raise_on_error: bool = False,
    ) -> None:
        plugin_name = (
            getattr(timing_ctx, "output_plugin_name", None)
            if timing_ctx is not None
            else None
        )
        with _memory_stage(
            timing_ctx,
            f"phase:{phase_name}",
            plugin=plugin_name,
            phase=phase_num,
            phase_name=phase_name,
        ):
            phase_started = time.perf_counter()
            progress = PhaseProgress(
                phase=phase_num,
                phase_name=phase_name,
                status="running",
            )
            runner.emit_phase_start(progress)
            phase_error: Exception | None = None
            try:
                phase_fn(progress)
                if progress.status != "error":
                    progress.status = "completed"
            except Exception as exc:
                progress.status = "error"
                progress.error = str(exc)
                runner.emit_log("ERROR", f"{phase_name} failed: {exc}")
                _log.exception("Phase %s failed", phase_name)
                phase_error = exc
            progress.elapsed_seconds = _elapsed_seconds(phase_started)
            if timing_ctx is not None:
                _timing_report(timing_ctx).record(
                    "phase",
                    progress.elapsed_seconds,
                    phase=phase_num,
                    phase_name=phase_name,
                    status=progress.status,
                )
            runner.emit_phase_complete(progress)
            if phase_error is not None and raise_on_error:
                raise phase_error


    def _update_registry(self, ctx: ConversionContext) -> None:
        if (
            not ctx.summary.esp_built
            or not (ctx.mod_path / ctx.output_plugin_name).is_file()
        ):
            return
        mappings = getattr(ctx.formkey_mapper, "mappings", {}) or {}
        for source_fk, mapping in mappings.items():
            self._registry.resolutions[source_fk] = mapping.get("new_formkey")


    def _apply_registry_mappings(self, ctx: ConversionContext) -> None:
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            return
        mappings = {
            source_formkey: resolved_formkey
            for source_formkey, resolved_formkey in self._registry.resolutions.items()
            if resolved_formkey
        }
        if not mappings:
            return
        # The apply_registry_mappings phase expects the params dict to BE the
        # mappings (deserialized via serde_json::from_value).
        rust_run.run_phase(
            "apply_registry_mappings",
            mod_path=str(ctx.mod_path),
            params=mappings,
        )


    def _emit_projected_navmeshes(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log(
                "WARN",
                "emit_projected_navmeshes: no rust run; skipping",
            )
            progress.total_items = 0
            progress.completed_items = 0
            return

        report = rust_run.run_phase(
            "emit_projected_navmeshes",
            mod_path=str(ctx.mod_path),
            params={},
        )
        records_added = int(report.get("records_added", 0) or 0)
        records_dropped = int(report.get("records_dropped", 0) or 0)
        warnings = int(report.get("warnings", 0) or 0)
        ctx.summary.records_translated += records_added
        ctx.summary.records_warnings += warnings + records_dropped
        runner.emit_log(
            "INFO",
            "emit_projected_navmeshes: "
            f"records_added={records_added} "
            f"records_dropped={records_dropped} warnings={warnings}",
        )
        progress.total_items = max(records_added + records_dropped + warnings, 1)
        progress.completed_items = progress.total_items


    def _graft_terrain_navmesh(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
        prior_plugin_path: str | Path,
    ) -> None:
        """--re-use-land: graft exterior LAND/NAVM + terrain-texture records from
        the prior output cache instead of regenerating them.
        Replaces both Convert Terrain and Emit Projected NavMeshes; NAVI is still
        rebuilt downstream from the grafted exterior NAVM + fresh interior NAVM."""
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log("WARN", "graft_terrain: no rust run; skipping")
            progress.total_items = 0
            progress.completed_items = 0
            return

        report = rust_run.run_phase(
            "graft_terrain",
            mod_path=str(ctx.mod_path),
            params={"prior_plugin_path": str(prior_plugin_path)},
        )
        records_added = int(report.get("records_added", 0) or 0)
        records_dropped = int(report.get("records_dropped", 0) or 0)
        warnings = int(report.get("warnings", 0) or 0)
        ctx.summary.records_translated += records_added
        ctx.summary.records_warnings += warnings + records_dropped
        runner.emit_log(
            "INFO",
            "graft_terrain (reuse): "
            f"records_added={records_added} "
            f"records_dropped={records_dropped} warnings={warnings}",
        )
        progress.total_items = max(records_added + records_dropped + warnings, 1)
        progress.completed_items = progress.total_items


    def _convert_interior_cells(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
        carry_previs: bool = False,
    ) -> None:
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log(
                "WARN",
                "convert_interior_cells: no rust run; skipping",
            )
            progress.total_items = 0
            progress.completed_items = 0
            return

        report = rust_run.run_phase(
            "convert_interior_cells",
            mod_path="",
            params={"carry_previs": carry_previs},
        )
        records_added = int(report.get("records_added", 0) or 0)
        records_dropped = int(report.get("records_dropped", 0) or 0)
        warnings = int(report.get("warnings", 0) or 0)
        ctx.summary.records_translated += records_added
        ctx.summary.records_warnings += warnings + records_dropped
        runner.emit_log(
            "INFO",
            "convert_interior_cells: "
            f"records_added={records_added} "
            f"records_dropped={records_dropped} warnings={warnings}",
        )
        progress.total_items = max(records_added + records_dropped + warnings, 1)
        progress.completed_items = progress.total_items


    def _rebuild_projected_navi(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log(
                "WARN",
                "rebuild_projected_navi: no rust run; skipping",
            )
            progress.total_items = 0
            progress.completed_items = 0
            return

        report = rust_run.run_phase(
            "rebuild_projected_navi",
            mod_path=str(ctx.mod_path),
            params={},
        )
        records_added = int(report.get("records_added", 0) or 0)
        records_dropped = int(report.get("records_dropped", 0) or 0)
        warnings = int(report.get("warnings", 0) or 0)
        ctx.summary.records_translated += records_added
        ctx.summary.records_warnings += warnings + records_dropped
        try:
            phase_warnings = [str(value) for value in rust_run.drain_warnings()]
        except Exception:
            phase_warnings = []
        phase_messages = [
            warning
            for warning in phase_warnings
            if warning.startswith("rebuild_projected_navi: ")
        ]
        non_stats_warnings = [
            warning
            for warning in phase_warnings
            if not warning.startswith("rebuild_projected_navi: ")
        ]
        for warning in non_stats_warnings:
            runner.emit_log("WARN", warning)
        if non_stats_warnings:
            ctx.summary.records_warnings += len(non_stats_warnings)
        navmesh_report_keys = (
            "navmesh_bad_internal_links",
            "navmesh_linked_edge_vertex_mismatches",
            "navmesh_missing_internal_links",
            "navmesh_external_links_added",
            "navmesh_missing_external_links",
            "navmesh_ambiguous_external_edges",
            "navmesh_winding_conflicts",
        )
        if all(key in report for key in navmesh_report_keys):
            runner.emit_log(
                "INFO",
                "rebuild_projected_navi: "
                f"records_added={records_added} "
                f"records_dropped={records_dropped} warnings={warnings} "
                f"navmesh_bad_internal_links={report.get('navmesh_bad_internal_links', 0) or 0} "
                f"navmesh_linked_edge_vertex_mismatches={report.get('navmesh_linked_edge_vertex_mismatches', 0) or 0} "
                f"navmesh_missing_internal_links={report.get('navmesh_missing_internal_links', 0) or 0} "
                f"navmesh_external_links_added={report.get('navmesh_external_links_added', 0) or 0} "
                f"navmesh_missing_external_links={report.get('navmesh_missing_external_links', 0) or 0} "
                f"navmesh_ambiguous_external_edges={report.get('navmesh_ambiguous_external_edges', 0) or 0} "
                f"navmesh_winding_conflicts={report.get('navmesh_winding_conflicts', 0) or 0}",
            )
        elif phase_messages:
            runner.emit_log("INFO", phase_messages[-1])
        else:
            runner.emit_log(
                "INFO",
                "rebuild_projected_navi: "
                f"records_added={records_added} "
                f"records_dropped={records_dropped} warnings={warnings}",
            )
        progress.total_items = max(records_added + records_dropped + warnings, 1)
        progress.completed_items = progress.total_items


    def _merge_summary(self, plugin_summary: ConversionSummary) -> None:
        aggregate = self._aggregate_summary
        for field_name, value in vars(plugin_summary).items():
            if isinstance(value, bool):
                setattr(aggregate, field_name, getattr(aggregate, field_name) or value)
            elif isinstance(value, int):
                setattr(aggregate, field_name, getattr(aggregate, field_name) + value)
            elif isinstance(value, list):
                getattr(aggregate, field_name).extend(value)


    def _merge_run_result(self, ctx: ConversionContext) -> None:
        decisions = list(getattr(ctx, "conversion_decisions", []) or [])
        self._run_result.decisions.extend(decisions)
        self._merge_counts(
            self._run_result.skipped_counts, self._count_v2_skips(ctx, decisions)
        )
        self._merge_counts(
            self._run_result.translated_counts, self._translated_counts(ctx)
        )

        log_lines = list(getattr(ctx, "log_lines", []) or [])
        self._extend_unique(
            self._run_result.failed_nifs, self._failed_paths(log_lines, "NIF")
        )
        self._extend_unique(
            self._run_result.failed_textures, self._failed_paths(log_lines, "Texture")
        )
        self._extend_unique(
            self._run_result.failed_bgsms, self._failed_material_paths(log_lines)
        )
        self._extend_unique(
            self._run_result.failed_faces,
            list(getattr(ctx, "face_failed_form_keys", []) or []),
        )
        self._extend_unique(
            self._run_result.degraded_faces,
            list(getattr(ctx, "face_degraded_form_keys", []) or []),
        )
        self._run_result.fnv_translation_gaps.extend(
            list(getattr(ctx.summary, "fnv_translation_gaps", []) or [])
        )
        self._extend_unique(
            self._run_result.lip_regeneration_needed,
            list(getattr(ctx.summary, "lip_regeneration_needed", []) or []),
        )

    @staticmethod

    def _count_v2_skips(
        ctx: ConversionContext,
        decisions: list[object],
    ) -> dict[str, int]:
        counts: Counter[str] = Counter()
        for decision in decisions:
            if isinstance(decision, dict):
                kind = decision.get("kind")
                reason = decision.get("reason")
                record_type = decision.get("record_type")
            else:
                kind = getattr(decision, "kind", None)
                reason = getattr(decision, "reason", None)
                record_type = getattr(decision, "record_type", None)
            if (
                kind == ConversionDecisionKind.EXPLICIT_DROP
                and reason in {"v2_deferred", "fnv_legacy_scripting_deferred"}
                and record_type
            ):
                counts[str(record_type)] += 1
        return dict(counts)

    @staticmethod

    def _translated_counts(
        ctx: ConversionContext,
    ) -> dict[str, int]:
        native_stats = getattr(ctx, "native_translate_stats", {}) or {}
        by_signature = (
            native_stats.get("by_signature", {})
            if isinstance(native_stats, dict)
            else {}
        )
        return {
            str(record_type): int(stats.get("translated", 0))
            for record_type, stats in by_signature.items()
            if isinstance(stats, dict) and int(stats.get("translated", 0)) > 0
        }

    @staticmethod

    def _failed_paths(log_lines: list[str], label: str) -> list[str]:
        failed: list[str] = []
        not_found_prefixes = (f"[WARN] {label} not found: ", f"[ERROR] {label} not found: ")
        failed_prefix = f"[ERROR] {label} failed: "
        for line in log_lines:
            matched = next((p for p in not_found_prefixes if line.startswith(p)), None)
            if matched is not None:
                failed.append(line.removeprefix(matched))
            elif line.startswith(failed_prefix):
                failed.append(_path_before_error(line.removeprefix(failed_prefix)))
        return failed

    @staticmethod

    def _failed_material_paths(log_lines: list[str]) -> list[str]:
        failed: list[str] = []
        not_found_prefixes = ("[WARN] Material not found: ", "[ERROR] Material not found: ")
        for line in log_lines:
            matched = next((p for p in not_found_prefixes if line.startswith(p)), None)
            if matched is not None:
                path = line.removeprefix(matched)
                if _is_bgsm_path(path):
                    failed.append(path)
            elif line.startswith("[ERROR] Material failed: "):
                path = _path_before_error(
                    line.removeprefix("[ERROR] Material failed: ")
                )
                if _is_bgsm_path(path):
                    failed.append(path)
        return failed

    @staticmethod

    def _merge_counts(target: dict[str, int], counts: dict[str, int]) -> None:
        for key, count in counts.items():
            target[key] = target.get(key, 0) + count

    @staticmethod

    def _extend_unique(target: list[str], paths: list[str]) -> None:
        seen = set(target)
        for path in paths:
            if path in seen:
                continue
            target.append(path)
            seen.add(path)

    @staticmethod

    def _plugin_name(source_plugin: Path) -> str:
        return source_plugin.name




@dataclass
class TrackSignals:
    """Cross-track launch gates. On a record-track failure ``record_failed`` is
    set FIRST, then every other event is released — waiters must re-check
    ``record_failed`` after each wait."""

    assets_ready: threading.Event = field(default_factory=threading.Event)
    fixups_done: threading.Event = field(default_factory=threading.Event)
    asset_a2_done: threading.Event = field(default_factory=threading.Event)
    terrain_done: threading.Event = field(default_factory=threading.Event)
    record_done: threading.Event = field(default_factory=threading.Event)
    record_failed: threading.Event = field(default_factory=threading.Event)


def _resolved_mod_root(request: PluginPortRequest) -> Path:
    mod_name = request.output_mod_name or Path(request.source_plugins[0].name).stem
    return Path(request.output_root) / mod_name


def _preflight_legacy_packs(
    request: PluginPortRequest,
    runner: "ConversionRunner",
) -> None:
    if (
        not request.options.translate_records
        or request.source_game.lower() not in {"fnv", "fo3"}
        or request.target_game.lower() != "fo4"
    ):
        return

    from bacup_lib.run import ConversionRun

    source_plugin = Path(request.source_plugins[0])
    config = {
        "output_plugin_name": source_plugin.name,
        "is_whole_plugin": True,
        "legacy_pack_origins": [
            row.to_dict() for row in request.legacy_pack_origins
        ],
        "legacy_pack_raw_source_counts": (
            request.legacy_pack_raw_source_counts.to_dict()
            if request.legacy_pack_raw_source_counts is not None
            else None
        ),
        "legacy_pack_expected_counts": (
            request.legacy_pack_expected_counts.to_dict()
            if request.legacy_pack_expected_counts is not None
            else None
        ),
        "legacy_pack_provenance_required": request.legacy_pack_provenance_required,
        "skip_record_signatures": _skip_record_signatures_payload(
            convert_placed_records=request.options.convert_placed_records,
            exclude_signatures=request.options.exclude_signatures,
        ),
    }
    runner.emit_log("INFO", "Running fatal legacy PACK record preflight")
    with ConversionRun.create_new(
        request.source_game,
        request.target_game,
        str(source_plugin),
        source_plugin.name,
        source_strings_dir=_resolve_source_strings_dir(
            source_plugin,
            request.source_data_dir,
        ),
        config=config,
    ) as run:
        try:
            run.preflight_legacy_packs()
        finally:
            for event in run.drain_events(256):
                if event.get("kind") == "log":
                    runner.emit_log(
                        str(event.get("level", "INFO")).upper(),
                        str(event.get("message", "")),
                    )


class UnifiedDriver:
    """Unified regen driver.

    Record conversion runs through _UnifiedRecordRuntime; asset phases run in
    concurrent pipeline waves.
    """
    def __init__(
        self,
        request: PluginPortRequest,
        *,
        sink_id: int | None = None,
        signals: TrackSignals | None = None,
        defer_asset_a2_until_record_done: bool = False,
    ) -> None:
        # Keep asset toggles on in `request.options`. The record track does not
        # execute the asset blocks; wave scheduling owns them, while config
        # consumers still see the full-run phase shape.
        self._req = request
        self._record_runtime = _UnifiedRecordRuntime(request)
        self.sink_id = sink_id
        self.signals = signals if signals is not None else TrackSignals()
        self.defer_asset_a2_until_record_done = defer_asset_a2_until_record_done
        # Record-track products consumed by the asset waves:
        self.assets: list = []  # the LIVE ctx.assets list (terrain appends grass)
        self.addon_index_map: dict[int, int] = {}
        self.terrain_texture_jobs: list[dict[str, str]] = []
        self.ctx = None
        self.record_run_id: int | None = None
        self.summary: ConversionSummary | None = None
        # Live record-phase label for the run_state mirror.
        self.current_record_label: str | None = None
        self.on_terrain_done: Callable[[], None] | None = None
        self.on_land_cache_ready: Callable[[ConversionContext], bool] | None = None
        self._terrain_done_notified = False
        # Set by run_asset_track on a wave failure; the record track checks
        # it at every phase boundary. (The conversion_run_cancel half only
        # reaches the record run once the rust run exists — this flag covers
        # the pre-translate window too.)
        self.asset_track_failed = threading.Event()

    @property
    def record_runtime(self) -> _UnifiedRecordRuntime:
        return self._record_runtime

    def emit_complete(self, runner: "ConversionRunner") -> None:
        runner.emit_complete(str(self._req.output_root), self._record_runtime._aggregate_summary)

    def _record_phase(self, phase_no, label, body, runner, **kwargs) -> None:
        """Delegates to _UnifiedRecordRuntime._run_phase, tracking the live
        label for the run_state mirror and honouring the asset-track failure
        flag at every phase boundary."""
        if self.asset_track_failed.is_set():
            raise RuntimeError(
                f"asset track failed — aborting record track before {label}"
            )
        self.current_record_label = label
        try:
            self._record_runtime._run_phase(phase_no, label, body, runner, **kwargs)
        finally:
            self.current_record_label = None

    def _mark_terrain_done(self) -> None:
        self.signals.terrain_done.set()
        if self._terrain_done_notified:
            return
        self._terrain_done_notified = True
        if self.on_terrain_done is not None:
            self.on_terrain_done()

    def _mark_land_cache_ready(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> None:
        if self.on_land_cache_ready is None:
            return
        try:
            self.on_land_cache_ready(ctx)
        except Exception as exc:  # noqa: BLE001 - cache is an optimization
            runner.emit_log("WARN", f"land cache: early snapshot failed: {exc}")

    # -- record track ------------------------------------------------------

    def run_record_track(self, runner: "ConversionRunner") -> ConversionSummary:
        """Run the record track for the single regen plugin."""
        try:
            return self._run_record_track_inner(runner)
        except Exception:
            self.signals.record_failed.set()
            self.signals.assets_ready.set()
            self.signals.fixups_done.set()
            self.signals.asset_a2_done.set()
            self.signals.terrain_done.set()
            self.signals.record_done.set()
            raise

    def _run_record_track_inner(self, runner: "ConversionRunner") -> ConversionSummary:
        orch = self._record_runtime
        batch_started = time.perf_counter()
        started_plugins = 0
        cancelled = False
        batch_error: Exception | None = None
        try:
            order_started = time.perf_counter()
            ordered_plugins = orch._topo_sort(self._req.source_plugins, runner)
            runner.emit_log(
                "INFO",
                "Resolved plugin order for "
                f"{len(ordered_plugins)} plugin(s) in "
                f"{_format_elapsed_seconds(_elapsed_seconds(order_started))}",
            )
            if len(ordered_plugins) != 1:
                raise ValueError(
                    f"unified driver expects exactly one source plugin, got {len(ordered_plugins)}"
                )
            source_plugin = ordered_plugins[0]
            if runner.is_cancelled():
                runner.emit_log(
                    "WARN", f"Cancelled before converting {source_plugin.name}"
                )
                cancelled = True
            else:
                started_plugins = 1
                self._convert_record_track(source_plugin, runner)
            self.summary = orch._aggregate_summary
            return orch._aggregate_summary
        except Exception as exc:
            batch_error = exc
            raise
        finally:
            elapsed = _format_elapsed_seconds(_elapsed_seconds(batch_started))
            plugin_count = len(self._req.source_plugins)
            if batch_error is not None:
                runner.emit_log(
                    "ERROR",
                    f"Whole-plugin batch failed after {elapsed} "
                    f"({started_plugins}/{plugin_count} plugin(s) started)",
                )
            elif cancelled:
                runner.emit_log(
                    "WARN",
                    f"Whole-plugin batch cancelled after {elapsed} "
                    f"({started_plugins}/{plugin_count} plugin(s) started)",
                )
            else:
                runner.emit_log(
                    "INFO",
                    f"Whole-plugin batch completed in {elapsed} "
                    f"({started_plugins}/{plugin_count} plugin(s) started)",
                )

    def _convert_record_track(self, source_plugin: Path, runner: "ConversionRunner") -> None:
        """Convert records for the unified FO76 whole-plugin run."""
        orch = self._record_runtime
        opts = self._req.options
        plugin_name = orch._plugin_name(source_plugin)
        runner.emit_log("INFO", f"Converting {plugin_name}")
        plugin_started = time.perf_counter()
        plugin_error: Exception | None = None

        mod_path = _resolved_mod_root(self._req)
        ctx = None
        try:
            # setup/ctx/mod scaffolding
            setup_started = time.perf_counter()
            mod_path.mkdir(parents=True, exist_ok=True)
            (mod_path / ".game").write_text(self._req.target_game, encoding="utf-8")
            (mod_path / ".source_game").write_text(
                self._req.source_game, encoding="utf-8"
            )
            (mod_path / ".source_plugin").write_text(plugin_name, encoding="utf-8")
            (mod_path / "yaml").mkdir(exist_ok=True)

            ctx = orch._build_context(source_plugin, plugin_name, mod_path, runner)
            self.ctx = ctx
            _memory_mark(ctx, "after:masters_open")
            for missing_master in getattr(
                ctx, "target_record_preflight_missing_masters", []
            ):
                runner.emit_log(
                    "WARN", f"target record preflight: missing {missing_master}"
                )
            for warning in getattr(ctx, "target_record_preflight_warnings", []):
                runner.emit_log("WARN", f"target record preflight: {warning}")
            for warning in (
                getattr(getattr(ctx, "target_asset_index", None), "warnings", []) or []
            ):
                runner.emit_log("WARN", warning)
            if (
                bool(getattr(ctx, "is_whole_plugin", False))
                and opts.cell_bounds is None
            ):
                orch._clean_stale_authoring_for_direct_esp(mod_path)
            ctx.conversion_decisions = []
            ctx.log_lines = []
            ctx.fnv_legacy_links = {}
            ctx.fnv_legacy_result = None
            ctx._worldspace_patched = False
            ctx._source_closed = False
            runner.emit_log(
                "INFO",
                f"Prepared output and context for {plugin_name} in "
                f"{_format_elapsed_seconds(_elapsed_seconds(setup_started))}",
            )
            _record_timing(
                ctx, "setup", setup_started, plugin=plugin_name, mod_path=mod_path
            )
            if opts.cell_bounds is not None:
                raise ValueError(
                    "cell-bounds runs are not supported in unified driver"
                )

            phase_offset = 0
            # No persistent Python source handle: the native ConversionRun owns
            # the only long-lived source parse tree. Asset collection below and
            # the post-run repairs open short-lived handles as needed.

            # collect native asset refs before translate
            # UNCONDITIONALLY: the DRIVER has asset phases even though THIS
            # options object disables them.
            runner.emit_log(
                "INFO", f"Collecting asset references natively for {plugin_name}"
            )
            assets_started = time.perf_counter()
            assets = orch._collect_assets_native(source_plugin, ctx, runner)
            runner.emit_log(
                "INFO",
                f"Gathered {len(assets)} asset references for {plugin_name} in "
                f"{_format_elapsed_seconds(_elapsed_seconds(assets_started))}",
            )
            _record_timing(
                ctx,
                "collect_assets",
                assets_started,
                source_plugin=source_plugin,
                count=len(assets),
            )
            _memory_mark(ctx, "after:collect_assets")
            ctx.assets = assets
            ctx.shared_asset_conversion_enabled = bool(
                opts.convert_nifs or opts.convert_textures or opts.convert_materials
            )
            self.assets = assets
            self.signals.assets_ready.set()

            if opts.convert_terrain and opts.reuse_terrain_navmesh:
                ctx._terrain_graft_source = _terrain_graft_source(opts, mod_path)

            # translate records + registry maps
            if opts.translate_records:
                self._record_phase(
                    2,
                    "Translate Records",
                    lambda p: orch._translate_records_rust([], ctx, runner, p),
                    runner,
                    timing_ctx=ctx,
                    raise_on_error=True,
                )
                _memory_mark(ctx, "after:translate")
                orch._apply_registry_mappings(ctx)
                if orch._run_optional_fnv_legacy_phase(ctx, source_plugin, runner):
                    phase_offset = 1
                orch._run_convert_creatures_phase(ctx, runner)
                orch._run_convert_equipment_phase(ctx, runner)

            # Attach the sink to the record run (terrain sidecars + any
            # future record-side asset writes).
            rust_run = getattr(ctx, "_rust_conversion_run", None)
            if rust_run is not None:
                self.record_run_id = rust_run.id
                if self.sink_id is not None:
                    load_native_module().sinks_attach_run(rust_run.id, self.sink_id)

            # addon_index_map harvest happened inside _translate_records_rust
            # emit record-run warnings inline.
            self.addon_index_map = dict(getattr(ctx, "addon_index_map", {}) or {})
            self.signals.fixups_done.set()

            post_terrain_phase_offset = 0
            if opts.convert_terrain:
                if opts.reuse_terrain_navmesh:
                    # Graft LAND/NAVM + terrain-texture records from a prior FO4
                    # output instead of regenerating them; replaces both Convert
                    # Terrain and Emit Projected NavMeshes (below). Source is the
                    # live deployed ESM in upgrade mode, else the --re-use-land
                    # run-local .regen_land_cache.esm.
                    cache_path = ctx._terrain_graft_source
                    self._record_phase(
                        3 + phase_offset,
                        "Graft Terrain + Navmesh (reuse)",
                        lambda p, path=cache_path: orch._graft_terrain_navmesh(
                            ctx, runner, p, path
                        ),
                        runner,
                        timing_ctx=ctx,
                    )
                else:
                    # convert terrain
                    self._record_phase(
                        3 + phase_offset,
                        "Convert Terrain",
                        lambda p: pipeline.convert_terrain(ctx, runner, p),
                        runner,
                        timing_ctx=ctx,
                    )
                # Cross-run transfer for wave A3 + the sidecar registry.
                self._harvest_terrain_products(mod_path, runner)
                self._mark_terrain_done()

                if (
                    opts.translate_records
                    and self._req.source_game == "fo76"
                    and self._req.target_game == "fo4"
                ):
                    # Emit NavMeshes / NAVI / Copy children / Persistent /
                    # emit projected ECZN records. On --re-use-land the exterior
                    # NAVM were grafted in the graft phase above, so skip the emit.
                    if not opts.reuse_terrain_navmesh:
                        self._record_phase(
                            4 + phase_offset,
                            "Emit Projected NavMeshes",
                            lambda p: orch._emit_projected_navmeshes(ctx, runner, p),
                            runner,
                            timing_ctx=ctx,
                        )
                        self._mark_land_cache_ready(ctx, runner)
                    if self._req.options.include_interior:
                        _carry = self._req.options.carry_interior_previs
                        self._record_phase(
                            5 + phase_offset,
                            "Convert Interior Cells",
                            lambda p, _carry=_carry: orch._convert_interior_cells(
                                ctx, runner, p, carry_previs=_carry
                            ),
                            runner,
                            timing_ctx=ctx,
                        )
                    self._record_phase(
                        6 + phase_offset,
                        "Rebuild Projected NAVI",
                        lambda p: orch._rebuild_projected_navi(ctx, runner, p),
                        runner,
                        timing_ctx=ctx,
                    )
                    self._record_phase(
                        7 + phase_offset,
                        "Copy Projected Placed Children",
                        lambda p: orch._copy_fo76_projected_placed_children(
                            ctx, runner, p
                        ),
                        runner,
                        timing_ctx=ctx,
                    )
                    self._record_phase(
                        8 + phase_offset,
                        "Synthesize Worldspace Persistent Cell",
                        lambda p: orch._synthesize_worldspace_persistent_cells(
                            ctx, runner, p
                        ),
                        runner,
                        timing_ctx=ctx,
                    )
                    # Cell-location sync MUST precede encounter-zone synthesis: it
                    # writes each interior cell's Location (XLCN) from the LCTN
                    # ref-arrays, and the ECZN interior pull-model links a cell to
                    # its location through that XLCN. Run it first so interior-only
                    # locations get an ECZN and their cells' XEZN is stamped — else
                    # the synthesis sees no interior XLCN and the cell's XEZN is
                    # left dangling (e.g. WhitespringMall01 -> a terrain CELL).
                    self._record_phase(
                        9 + phase_offset,
                        "Sync Projected Cell Locations",
                        lambda p: orch._sync_fo76_projected_cell_locations(
                            ctx, runner, p
                        ),
                        runner,
                        timing_ctx=ctx,
                    )
                    self._record_phase(
                        10 + phase_offset,
                        "Synthesize Encounter Zones",
                        lambda p: orch._synthesize_encounter_zones(ctx, runner, p),
                        runner,
                        timing_ctx=ctx,
                    )
                    # Interior sky regions: reads the dropped XISR weather from the
                    # SOURCE cell, so it MUST run before the early source close.
                    self._record_phase(
                        11 + phase_offset,
                        "Synthesize Interior Sky Regions",
                        lambda p: orch._synthesize_sky_regions(ctx, runner, p),
                        runner,
                        timing_ctx=ctx,
                    )
                    # repair
                    self._record_phase(
                        12 + phase_offset,
                        "Repair Placed-Child Refs",
                        lambda p: orch._repair_placed_child_refs(ctx, runner, p),
                        runner,
                        timing_ctx=ctx,
                    )
                    _memory_mark(ctx, "after:repair")
                    # early source close
                    if not getattr(ctx, "_source_closed", False):
                        orch._close_source_handle(ctx)
                        ctx._source_closed = True
                        _memory_mark(ctx, "after:early_source_close")
                    # vendor dialogue: needs FACT.VENC finalized by repair above;
                    # target-only (FACT/NPC_), and must run before the mapper
                    # remap state is released below.
                    self._record_phase(
                        13 + phase_offset,
                        "Synthesize Vendor Dialogue",
                        lambda p: orch._synthesize_vendor_dialogue(ctx, runner, p),
                        runner,
                        timing_ctx=ctx,
                    )
                    _memory_mark(ctx, "after:vendor_dialogue")
                    # close masters and release remap state
                    orch._close_target_master_handles(ctx)
                    _memory_mark(ctx, "after:masters_early_close")
                    _rust_run = getattr(ctx, "_rust_conversion_run", None)
                    if _rust_run is not None:
                        _rust_run.release_remap_state()
                        _rust_run.release_master_handles()
                    post_terrain_phase_offset = 6
            else:
                self._mark_terrain_done()

            # Convert Terrain BTOs — wave A2 owns it: the native phase reads
            # source roots + its own run only, never the target handle
            # (discover_terrain_bto_assets walks source dirs).
            if opts.convert_btos:
                post_terrain_phase_offset += 1
            next_phase_no = 4 + phase_offset + post_terrain_phase_offset

            # NPC faces (off by default)
            if opts.convert_npc_faces:
                self._record_phase(
                    next_phase_no,
                    "Convert NPC Faces",
                    lambda p: orch._run_convert_face_phase(ctx, runner, p),
                    runner,
                    timing_ctx=ctx,
                )
                next_phase_no += 1

            # Synthesize object-LOD MNAM so native lodgen can emit .bto for
            # converted bases (FO76 ships LOD by folder convention, leaving the
            # FO4 records without MNAM). After translate+fixups (records carry
            # MODL), before build_esp (so MNAM serializes into the ESP). Uses the
            # FO76 source extracted dir (filesystem) to existence-check _lod.nif,
            # so it is safe even after the source handle's early close.
            if getattr(opts, "synthesize_object_lod", False):
                rust_run = getattr(ctx, "_rust_conversion_run", None)
                if rust_run is not None and opts.build_esp:
                    source_extracted = str(
                        getattr(ctx, "source_data_dir", None)
                        or self._req.source_data_dir
                        or ""
                    )
                    self._record_phase(
                        next_phase_no,
                        "Synthesize Object LOD",
                        lambda p, _se=source_extracted, _rr=rust_run: _rr.run_phase(
                            "synthesize_object_lod",
                            mod_path=str(ctx.mod_path),
                            source_extracted_dir=_se,
                            params={
                                "conversion_workers": getattr(ctx, "conversion_workers", None),
                            },
                        ),
                        runner,
                        timing_ctx=ctx,
                    )
                    next_phase_no += 1
                else:
                    self._record_phase(
                        next_phase_no,
                        "Synthesize Object LOD",
                        lambda p: orch._run_synthesize_object_lod_existing_output(
                            source_plugin, ctx, runner
                        ),
                        runner,
                        timing_ctx=ctx,
                    )
                    next_phase_no += 1

            # scaffold / convert scripts / build ESP
            if opts.build_esp:
                rust_run = getattr(ctx, "_rust_conversion_run", None)
                emit_authoring_yaml = orch._emit_authoring_yaml_for_build(ctx, runner)
                self._record_phase(
                    next_phase_no,
                    "Scaffold Mod",
                    lambda p: (
                        rust_run.run_phase(
                            "scaffold",
                            mod_path=str(ctx.mod_path),
                            params={
                                "mod_prefix": ctx.mod_path.name,
                                "output_plugin_name": ctx.output_plugin_name,
                                "emit_authoring_yaml": emit_authoring_yaml,
                            },
                        )
                        if rust_run is not None
                        else None
                    ),
                    runner,
                    timing_ctx=ctx,
                )
                next_phase_no += 1
                if (
                    opts.convert_scripts
                    and self._req.source_game == "fo76"
                    and self._req.target_game == "fo4"
                ):
                    self._record_phase(
                        next_phase_no,
                        "Convert Scripts",
                        lambda p: orch._run_convert_scripts_phase(ctx, runner),
                        runner,
                        timing_ctx=ctx,
                    )
                    _memory_mark(ctx, "after:convert_scripts")
                    next_phase_no += 1

                def _build_esp_phase(_progress) -> None:
                    if rust_run is None:
                        raise RuntimeError("build_esp: no rust run; cannot save ESP")
                    _stamp_target_plugin_version(self._req, ctx, runner)
                    output_path = ctx.mod_path / ctx.output_plugin_name
                    rust_run.save_target(
                        str(output_path),
                        emit_authoring_yaml=emit_authoring_yaml,
                        run_nvnm_validator=not bool(
                            getattr(ctx, "is_whole_plugin", False)
                        ),
                    )
                    if not output_path.is_file():
                        raise FileNotFoundError(
                            f"build_esp completed but did not write {output_path}"
                        )
                    ctx.summary.esp_built = True

                self._record_phase(
                    next_phase_no,
                    "Build ESP",
                    _build_esp_phase,
                    runner,
                    timing_ctx=ctx,
                    raise_on_error=True,
                )
                _memory_mark(ctx, "after:build_esp")
                next_phase_no += 1
                if rust_run is not None:
                    rust_run.release_source_handle()
                orch._drain_and_drop_rust_run(ctx)
                # worldspace patch + source-close guard
                if plugin_error is None:
                    orch._patch_projected_worldspace_subrecords(
                        ctx, runner, source_plugin
                    )
                    ctx._worldspace_patched = True
                if not getattr(ctx, "_source_closed", False):
                    orch._close_source_handle(ctx)
                    ctx._source_closed = True
                    _memory_mark(ctx, "after:early_source_close")

            # validate_output reads the on-disk
            # ESM only; independent of the asset waves).
            if opts.validate_output:
                if opts.build_esp:
                    self._record_phase(
                        next_phase_no,
                        "Validate Output",
                        lambda p: orch._validate_output_plugin(ctx, runner),
                        runner,
                        timing_ctx=ctx,
                        raise_on_error=True,
                    )
                    next_phase_no += 1
                else:
                    runner.emit_log(
                        "WARN",
                        "validate_output requested but build_esp is disabled; skipping validation",
                    )

            orch._update_registry(ctx)
            orch._merge_summary(ctx.summary)
            orch._merge_run_result(ctx)
        except Exception as exc:
            plugin_error = exc
            raise
        finally:
            # finally-block drains/closes. The record run shares nothing with
            # the asset waves' runs — drop it here.
            try:
                if ctx is not None:
                    orch._drain_and_drop_rust_run(ctx)
            finally:
                try:
                    if (
                        ctx is not None
                        and plugin_error is None
                        and not getattr(ctx, "_worldspace_patched", False)
                    ):
                        orch._patch_projected_worldspace_subrecords(
                            ctx, runner, source_plugin
                        )
                finally:
                    try:
                        if ctx is not None and not getattr(
                            ctx, "_source_closed", False
                        ):
                            orch._close_source_handle(ctx)
                            _memory_mark(ctx, "after:source_close")
                    finally:
                        if ctx is not None:
                            orch._close_target_master_handles(ctx)
                            _memory_mark(ctx, "after:masters_close")
            elapsed = _format_elapsed_seconds(_elapsed_seconds(plugin_started))
            if plugin_error is not None:
                runner.emit_log("ERROR", f"{plugin_name} failed after {elapsed}")
            else:
                runner.emit_log("INFO", f"Completed {plugin_name} in {elapsed}")

    def _harvest_terrain_products(
        self, mod_path: Path, runner: "ConversionRunner"
    ) -> None:
        """Transfer terrain texture jobs to wave A3 and register sidecars."""
        rust_run = getattr(self.ctx, "_rust_conversion_run", None) if self.ctx else None
        if rust_run is not None:
            try:
                self.terrain_texture_jobs = list(
                    json.loads(
                        load_native_module().conversion_run_terrain_texture_jobs_json(
                            rust_run.id
                        )
                    )
                )
            except Exception as exc:
                raise RuntimeError(
                    "Failed to transfer LAND texture jobs to textures_v2"
                ) from exc
            runner.emit_log(
                "INFO",
                f"Queued {len(self.terrain_texture_jobs)} LAND texture bundle(s) "
                "for textures_v2",
            )
        if self.sink_id is not None:
            native = load_native_module()
            terrain_dir = Path(mod_path) / "Terrain"
            if terrain_dir.is_dir():
                for sidecar in sorted(terrain_dir.glob("*.btd4")):
                    native.sinks_register_sidecar(
                        self.sink_id, f"Terrain/{sidecar.name}"
                    )


# ---------------------------------------------------------------------------
# Asset waves
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class AssetWaveToggles:
    """User-visible asset phase enables (the regen.py --no-* levers)."""

    nifs: bool = True
    btos: bool = True
    textures: bool = True
    materials: bool = True
    havok: bool = True
    drivers: bool = True
    sounds: bool = True
    # convert_animations is gamebryo->creation1 only and therefore inert for
    # fo76->fo4; the wave builder mirrors that guard.
    animations: bool = True

    @classmethod
    def from_options(cls, opts) -> "AssetWaveToggles":
        return cls(
            nifs=bool(opts.convert_nifs),
            btos=bool(opts.convert_btos),
            textures=bool(opts.convert_textures),
            materials=bool(opts.convert_materials),
            havok=bool(opts.convert_havok),
            drivers=bool(opts.synthesize_drivers),
            sounds=bool(opts.copy_sounds),
            animations=bool(opts.convert_animations),
        )


@dataclass(frozen=True)
class _AssetWavePlan:
    nif_phase: str = "convert_nifs_v2"
    bto_phase: str | None = "convert_btos_v2"
    texture_phase: str = "convert_textures_v2"
    material_phase: str | None = "convert_materials_v2"
    grass_topup: bool = True
    wave_a4: bool = True
    # FO76 runs enumerate the whole extracted tree; legacy sources have no
    # extracted tree to walk and convert the referenced set instead.
    textures_from_graph: bool = False


def _wave_plan_for(source_game: str) -> _AssetWavePlan:
    from creation_lib.core.game_profiles import get_profile

    if source_game.lower() == "skyrimse":
        return _AssetWavePlan(
            nif_phase="convert_nifs_v2",
            bto_phase=None,
            texture_phase="convert_textures_v2",
            material_phase=None,
            grass_topup=True,
            wave_a4=False,
            textures_from_graph=True,
        )
    if get_profile(source_game).engine == "gamebryo":
        return _AssetWavePlan(
            nif_phase="convert_gamebryo_nifs",
            bto_phase=None,
            texture_phase="convert_textures_v2",
            material_phase=None,
            grass_topup=False,
            wave_a4=False,
            textures_from_graph=True,
        )
    return _AssetWavePlan()


def _mvp_exclude_signatures(driver: "UnifiedDriver") -> frozenset[str]:
    from bacup_lib.source_pairs import MVP_EXCLUDE_SIGNATURES_BY_PAIR

    excluded = frozenset(
        signature.upper()
        for signature in getattr(driver._req.options, "exclude_signatures", ())
    )
    pair_id = {
        "fnv": "fnvfo3:fo4",
        "skyrimse": "skyrimse:fo4",
    }.get(str(driver.ctx.source_game).lower())
    expected = MVP_EXCLUDE_SIGNATURES_BY_PAIR.get(pair_id or "", frozenset())
    return expected if expected and expected <= excluded else frozenset()


def _is_skyrim_mvp(driver: "UnifiedDriver") -> bool:
    return (
        str(driver.ctx.source_game).lower() == "skyrimse"
        and bool(_mvp_exclude_signatures(driver))
    )


def _is_world_only_mvp_audio_path(path: str) -> bool:
    parts = [part for part in path.replace("\\", "/").lower().split("/") if part]
    if parts and parts[0] == "data":
        parts = parts[1:]
    if not parts:
        return False
    if parts[0] not in {"music", "sound"}:
        parts.insert(0, "sound")
    if parts[0] == "music":
        return True
    return (
        len(parts) >= 3
        and parts[0] == "sound"
        and parts[1] == "fx"
        and not _is_skyrim_mvp_loadscreen_asset_path(path, "sound")
    )


def _is_skyrim_mvp_audio_path(path: str) -> bool:
    return _is_world_only_mvp_audio_path(path)


def _is_skyrim_mvp_loadscreen_asset_path(
    path: str, asset_type: str = ""
) -> bool:
    normalized_type = str(asset_type).lower()
    parts = [part for part in path.replace("\\", "/").lower().split("/") if part]
    if parts and parts[0] == "data":
        parts = parts[1:]
    inferred_root = {
        "audio": "sound",
        "material": "materials",
        "nif": "meshes",
        "sound": "sound",
        "texture": "textures",
    }.get(normalized_type)
    if inferred_root and parts and parts[0] != inferred_root:
        parts.insert(0, inferred_root)
    if not parts:
        return False

    filename = parts[-1]
    if (
        parts[0] in {"materials", "meshes"}
        and parts[1:3] == ["_byoh", "loadsscreens"]
    ):
        return filename.endswith((".bgsm", ".nif"))
    if parts[:2] == ["textures", "clutter"]:
        return filename.startswith("loadscreen") and filename.endswith(".dds")
    if parts[:3] == ["sound", "fx", "ui"]:
        return filename.startswith(("loadscreen", "ui_loadscreen")) and filename.endswith(
            ".wav"
        )
    return (
        parts[0] in {"materials", "meshes"}
        and "loadscreenart" in parts[1:]
        and filename.endswith((".bgsm", ".nif"))
    )


def _is_skyrim_mvp_loadscreen_mesh_path(
    path: str, asset_type: str = ""
) -> bool:
    normalized_type = str(asset_type).lower()
    if normalized_type and normalized_type != "nif":
        return False
    if not normalized_type and not path.lower().endswith(".nif"):
        return False
    return _is_skyrim_mvp_loadscreen_asset_path(path, "nif")


def _remove_skyrim_mvp_loadscreen_material_outputs(mod_path: str) -> int:
    materials_root = Path(mod_path) / "data" / "Materials"
    if not materials_root.is_dir():
        return 0
    stale_outputs = [
        path
        for path in materials_root.rglob("*")
        if path.is_file()
        and path.suffix.lower() == ".bgsm"
        and _is_skyrim_mvp_loadscreen_asset_path(
            path.relative_to(materials_root).as_posix(), "material"
        )
    ]
    for path in stale_outputs:
        path.unlink()
    return len(stale_outputs)


def _remove_skyrim_mvp_loadscreen_asset_outputs(mod_path: str) -> int:
    data_root = Path(mod_path) / "data"
    removed = _remove_skyrim_mvp_loadscreen_material_outputs(mod_path)
    for relative_root in ("Meshes/_BYOH/LoadsScreens", "Sound/FX/UI", "Textures/Clutter"):
        root = data_root / relative_root
        if not root.is_dir():
            continue
        for path in root.rglob("*"):
            if path.is_file() and _is_skyrim_mvp_loadscreen_asset_path(
                path.relative_to(data_root).as_posix()
            ):
                path.unlink()
                removed += 1
    return removed


def _is_world_only_mvp_asset_path(path: str, asset_type: str = "") -> bool:
    if _is_skyrim_mvp_loadscreen_asset_path(path, asset_type):
        return False
    parts = [part for part in path.replace("\\", "/").lower().split("/") if part]
    if parts and parts[0] == "data":
        parts = parts[1:]
    inferred_root = {"nif": "meshes", "texture": "textures"}.get(
        str(asset_type).lower()
    )
    if inferred_root and parts and parts[0] not in {"meshes", "textures"}:
        parts.insert(0, inferred_root)
    if len(parts) < 2 or parts[0] not in {"meshes", "textures"}:
        return True
    semantic_parts = parts[1:]
    while semantic_parts:
        namespace = semantic_parts[0]
        if namespace == "creationclub":
            semantic_parts = semantic_parts[2:]
            continue
        if namespace not in {"dlc01", "dlc02", "_byoh", "_shared"}:
            break
        semantic_parts = semantic_parts[1:]
    return not semantic_parts or semantic_parts[0] not in {
        "actors",
        "armor",
        "characters",
        "clothes",
        "creatures",
        "weapons",
    }


def _is_world_only_mvp_asset(
    asset: AssetRef, exclude_signatures: frozenset[str]
) -> bool:
    from bacup_lib.base_asset_dedupe import asset_owner_signature

    if asset_owner_signature(asset) in exclude_signatures:
        return False
    return _is_world_only_mvp_asset_path(asset.source_path, asset.asset_type)


def _is_skyrim_mvp_world_asset_path(path: str, asset_type: str = "") -> bool:
    return _is_world_only_mvp_asset_path(path, asset_type)


def _is_skyrim_mvp_world_asset(asset: AssetRef) -> bool:
    from bacup_lib.source_pairs import SKYRIM_MVP_EXCLUDE_SIGNATURES

    return _is_world_only_mvp_asset(asset, SKYRIM_MVP_EXCLUDE_SIGNATURES)


@dataclass
class WaveStage:
    phase: str
    run_id: int
    mod_path: str
    source_extracted_dir: str
    params: dict
    after: tuple[str, ...] = ()
    target_extracted_dir: str | None = None
    target_data_dir: str | None = None

    def to_plan_stage(self) -> dict:
        stage = {
            "phase": self.phase,
            "run_id": self.run_id,
            "mod_path": self.mod_path,
            "source_extracted_dir": self.source_extracted_dir,
            "params": self.params,
            "after": list(self.after),
        }
        if self.target_extracted_dir is not None:
            stage["target_extracted_dir"] = self.target_extracted_dir
        if self.target_data_dir is not None:
            stage["target_data_dir"] = self.target_data_dir
        return stage


class AssetRuns:
    """Per-wave-group native runs: run_T textures+materials, run_N nifs+btos
    sharing the collision memo, run_H havok+drivers, run_S sounds. Stub plugin
    handles — asset phases never read them."""

    def __init__(
        self,
        ctx,
        toggles: "AssetWaveToggles",
        *,
        conversion_workers: int | None = None,
    ) -> None:
        from bacup_lib.run import ConversionRun

        def make(needs_relocation: bool):
            workers = (
                int(conversion_workers)
                if conversion_workers is not None
                else getattr(ctx, "conversion_workers", None)
            )
            config = {
                "output_plugin_name": str(
                    getattr(ctx, "output_plugin_name", "Output.esm")
                ),
                "is_whole_plugin": bool(getattr(ctx, "is_whole_plugin", False)),
                "conversion_workers": workers,
                "base_asset_namespace": str(
                    getattr(ctx, "base_asset_namespace", "") or ""
                ),
                "base_asset_relocation_mesh_roots": list(
                    getattr(ctx, "base_asset_relocation_mesh_roots", ()) or []
                ),
                "target_data_dir": str(getattr(ctx, "target_data_dir", "") or "")
                or None,
                "target_asset_catalog_path": str(
                    getattr(ctx, "target_asset_catalog_path", "") or ""
                )
                or None,
                "target_asset_cache_dir": str(
                    getattr(ctx, "target_asset_cache_dir", "") or ""
                )
                or None,
                "target_extracted_dir": str(
                    getattr(ctx, "target_extracted_dir", "") or ""
                )
                or None,
            }
            if needs_relocation:
                # The source extracted dir makes create_run build the
                # relocation member set (textures/nifs/materials read it).
                source = getattr(ctx, "source_data_dir", None)
                config["source_extracted_dir"] = str(source) if source else None
            config["mod_path"] = str(getattr(ctx, "mod_path", "") or "") or None
            return ConversionRun.create_new(
                str(ctx.source_game),
                str(ctx.target_game),
                None,
                str(config["output_plugin_name"]),
                config=config,
            )

        self.textures = make(True) if (toggles.textures or toggles.materials) else None
        self.nifs = make(True) if (toggles.nifs or toggles.btos) else None
        self.havok = make(False) if (toggles.havok or toggles.drivers) else None
        self.sounds = make(False) if toggles.sounds else None

    def all_runs(self) -> list:
        return [r for r in (self.textures, self.nifs, self.havok, self.sounds) if r]

    def attach_sink(self, sink_id: int) -> None:
        native = load_native_module()
        for run in self.all_runs():
            native.sinks_attach_run(run.id, sink_id)

    def cancel_all(self) -> None:
        for run in self.all_runs():
            try:
                run.cancel()
            except Exception:
                pass

    def drop_all(self) -> None:
        for run in self.all_runs():
            try:
                run.close()
            except Exception:
                pass


def _asset_key_pair(asset) -> tuple[str, str]:
    return (
        str(getattr(asset, "asset_type", "")).lower(),
        str(getattr(asset, "source_path", "")).replace("\\", "/").strip().lower(),
    )


class AssetWaveBuilder:
    """Builds the wave plans from the driver's live context via the same shim +
    param builders the pipeline wrappers use."""

    def __init__(
        self,
        driver: "UnifiedDriver",
        toggles: "AssetWaveToggles",
        runs: "AssetRuns",
        runner: "ConversionRunner",
        *,
        conversion_workers: int | None = None,
    ) -> None:
        self.driver = driver
        self.toggles = toggles
        self.runs = runs
        self.runner = runner
        self.conversion_workers = conversion_workers
        self._wave_plan = _wave_plan_for(str(driver.ctx.source_game))
        # NIF entries seen by wave A2 — the A3 grass top-up converts only the
        # delta appended by the terrain phase.
        self._a2_nif_keys: set[tuple[str, str]] = set()

    def _shim(self):
        from bacup_lib.pipeline._shim import build_orchestrator_shim

        shim = build_orchestrator_shim([], self.driver.ctx)
        shim.pbr_carry = bool(getattr(self.driver.ctx, "pbr_carry", False))
        shim.texture_landscape_mip_flooding = bool(
            getattr(self.driver.ctx, "texture_landscape_mip_flooding", False)
        )
        if self.conversion_workers is not None:
            shim.conversion_workers = int(self.conversion_workers)
        return shim

    # -- A1: immediately after collect_assets ------------------------------

    def build_wave_a1(self) -> list["WaveStage"]:
        shim = self._shim()
        stages: list[WaveStage] = []
        mvp_exclusions = _mvp_exclude_signatures(self.driver)
        if _is_skyrim_mvp(self.driver):
            removed = _remove_skyrim_mvp_loadscreen_asset_outputs(shim.mod_path)
            if removed:
                self.runner.emit_log(
                    "INFO",
                    f"[Assets] Skyrim MVP removed {removed} stale loadscreen asset(s)",
                )
        if self.toggles.sounds and self.runs.sounds:
            # Mirror of pipeline/sounds.py::copy_sounds_native.
            sound_assets = [
                a
                for a in list(self.driver.ctx.assets)
                if a.asset_type in {"sound", "audio"}
            ]
            if mvp_exclusions:
                all_sound_count = len(sound_assets)
                sound_assets = [
                    asset
                    for asset in sound_assets
                    if _is_world_only_mvp_audio_path(asset.source_path)
                ]
                self.runner.emit_log(
                    "INFO",
                    f"[Sound] {str(self.driver.ctx.source_game).upper()} MVP FX/music filter: "
                    f"kept={len(sound_assets)} omitted={all_sound_count - len(sound_assets)}",
                )
            shim._summary.audio_total = len(sound_assets)
            stages.append(
                WaveStage(
                    phase="copy_sounds",
                    run_id=self.runs.sounds.id,
                    mod_path=str(shim.mod_path),
                    source_extracted_dir="",
                    target_extracted_dir=str(shim.target_extracted_dir or ""),
                    params={
                        "sound_paths": [
                            {
                                "source_path": a.source_path,
                                "resolved_path": a.resolved_path or "",
                            }
                            for a in sound_assets
                        ]
                    },
                )
            )
        return stages

    # -- A2: after fixups-done (addon map harvested) ------------------------

    def build_wave_a2(self) -> list["WaveStage"]:
        shim = self._shim()
        stages: list[WaveStage] = []
        if self.toggles.nifs and self.runs.nifs:
            stages.append(self._build_nif_stage(shim, after=()))
        if (
            self._wave_plan.bto_phase is not None
            and self.toggles.btos
            and self.runs.nifs
        ):
            # Mirror of asset_phases.py::_phase_convert_btos_native_impl
            # (single_call shape; source_extracted_dir="").
            from bacup_lib.workflows.asset_phases import (
                _params_for_convert_btos,
                discover_terrain_bto_assets,
            )

            bto_assets = discover_terrain_bto_assets(shim)
            shim._summary.btos_total = len(bto_assets)
            self.runner.emit_log(
                "INFO", f"[BTO] total discovered={len(bto_assets)} (wave A2)"
            )
            if bto_assets:
                stages.append(
                    WaveStage(
                        phase=self._wave_plan.bto_phase,
                        run_id=self.runs.nifs.id,
                        mod_path=str(shim.mod_path),
                        source_extracted_dir="",
                        params=_params_for_convert_btos(shim, bto_assets),
                        # Same run as NIFs (shared collision memo) — chained
                        # behind them like the legacy serial order.
                        after=(self._wave_plan.nif_phase,)
                        if self.toggles.nifs
                        else (),
                    )
                )
        return stages

    def _build_nif_stage(self, shim, after: tuple[str, ...]) -> "WaveStage":
        # Mirror of asset_phases.py::_phase_convert_nifs_native_impl pre-work +
        # the single-call dispatch shape incl. the
        # source_extracted_dir=str(target_extracted_dir) oddity.
        from bacup_lib.workflows.asset_phases import (
            _is_precombined_nif_asset,
            _params_for_convert_nifs,
        )

        nif_assets = [a for a in shim.graph.all_assets if a.asset_type == "nif"]
        if self._wave_plan.nif_phase == "convert_gamebryo_nifs":
            nif_assets = [
                asset
                for asset in nif_assets
                if asset.source_path.lower().endswith(".nif")
            ]
        mvp_exclusions = _mvp_exclude_signatures(self.driver)
        skyrim_mvp = _is_skyrim_mvp(self.driver)
        if skyrim_mvp:
            stale_loadscreen_materials_removed = (
                _remove_skyrim_mvp_loadscreen_material_outputs(shim.mod_path)
            )
            loadscreen_assets = [
                asset
                for asset in nif_assets
                if _is_skyrim_mvp_loadscreen_mesh_path(
                    asset.source_path, asset.asset_type
                )
            ]
            stale_loadscreen_outputs_removed = 0
            for asset in loadscreen_assets:
                if shim._remove_stale_asset_output(asset):
                    stale_loadscreen_outputs_removed += 1
                shim._track_asset(
                    asset,
                    "mvp_runtime_skip",
                    "Skyrim LoadScreenArt meshes are not FO4-runtime compatible",
                )
            if loadscreen_assets or stale_loadscreen_materials_removed:
                self.runner.emit_log(
                    "INFO",
                    f"[NIF] Skyrim MVP skipped {len(loadscreen_assets)} "
                    "LoadScreenArt NIF(s); removed "
                    f"{stale_loadscreen_outputs_removed} stale NIF output(s) and "
                    f"{stale_loadscreen_materials_removed} generated BGSM output(s)",
                )
        if mvp_exclusions:
            nif_assets = [
                asset
                for asset in nif_assets
                if _is_world_only_mvp_asset(asset, mvp_exclusions)
            ]
        self._a2_nif_keys = {_asset_key_pair(a) for a in nif_assets}
        if not getattr(shim, "convert_precombined_nifs", True):
            kept = [a for a in nif_assets if not _is_precombined_nif_asset(a)]
            skipped = len(nif_assets) - len(kept)
            if skipped:
                nif_assets = kept
                self.runner.emit_log(
                    "INFO", f"[NIF] skipped {skipped} precombined NIFs"
                )
        shim._summary.nifs_total = len(nif_assets)
        convert_assets = []
        base_game_skipped = 0
        stale_outputs_removed = 0
        for asset in nif_assets:
            if shim._target_has_asset(asset):
                base_game_skipped += 1
                if shim._remove_stale_asset_output(asset):
                    stale_outputs_removed += 1
                shim._track_asset(asset, "base_game_skip", "exists in target game")
            elif not asset.resolved_path:
                shim._summary.nifs_failed += 1
                message = asset.resolution_error or "source path did not resolve"
                self.runner.emit_log(
                    "WARN", f"NIF not found: {asset.source_path}: {message}"
                )
            else:
                convert_assets.append(asset)
        if base_game_skipped:
            shim._summary.nifs_base_game_skipped += base_game_skipped
            self.runner.emit_log(
                "INFO",
                f"[NIF] skipped {base_game_skipped} NIFs already present in target game",
            )
        if stale_outputs_removed:
            self.runner.emit_log(
                "INFO",
                f"[NIF] removed {stale_outputs_removed} stale NIF output(s) for target-game skips",
            )
        self.runner.emit_log(
            "INFO",
            f"[NIF] total referenced={len(nif_assets)}, queued={len(convert_assets)}, "
            f"target-game skipped={base_game_skipped} (wave A2)",
        )
        params = _params_for_convert_nifs(shim, convert_assets)
        source_extracted_dir = str(shim.target_extracted_dir or "")
        if str(shim.source_game).lower() == "skyrimse":
            params["bgsm_output_dir"] = str(
                Path(str(shim.mod_path)) / "data" / "Materials"
            )
        if self._wave_plan.nif_phase == "convert_gamebryo_nifs":
            params = {
                "nif_paths": params["nif_paths"],
                "material_out_rel": (
                    f"materials/{Path(str(shim.mod_path)).name}/gamebryo"
                ),
            }
            source_extracted_dir = ""
        return WaveStage(
            phase=self._wave_plan.nif_phase,
            run_id=self.runs.nifs.id,
            mod_path=str(shim.mod_path),
            source_extracted_dir=source_extracted_dir,
            params=params,
            after=after,
        )

    # -- A3: after terrain-done ---------------------------------------------

    def build_wave_a3(self) -> list["WaveStage"]:
        shim = self._shim()
        stages: list[WaveStage] = []
        mvp_exclusions = _mvp_exclude_signatures(self.driver)
        if self.toggles.textures and self.runs.textures:
            if self._wave_plan.textures_from_graph:
                from bacup_lib.workflows.asset_phases import (
                    _is_precombined_nif_asset,
                    _params_for_convert_textures,
                )

                referenced_textures = [
                    asset
                    for asset in shim.graph.all_assets
                    if asset.asset_type == "texture"
                    and asset.source_path.lower().endswith(".dds")
                    and (
                        not mvp_exclusions
                        or _is_world_only_mvp_asset(asset, mvp_exclusions)
                    )
                ]
                texture_assets = []
                base_game_skipped = 0
                for asset in referenced_textures:
                    if shim._target_has_asset(asset):
                        base_game_skipped += 1
                        shim._remove_stale_asset_output(asset)
                        shim._track_asset(
                            asset, "base_game_skip", "exists in target game"
                        )
                    else:
                        texture_assets.append(asset)
                if base_game_skipped:
                    shim._summary.textures_total += base_game_skipped
                    shim._summary.textures_base_game_skipped += base_game_skipped
                nif_assets = [
                    asset
                    for asset in shim.graph.all_assets
                    if asset.asset_type == "nif"
                    and asset.source_path.lower().endswith(".nif")
                    and asset.resolved_path
                    and not shim._target_has_asset(asset)
                    and (
                        getattr(shim, "convert_precombined_nifs", True)
                        or not _is_precombined_nif_asset(asset)
                    )
                    and (
                        not mvp_exclusions
                        or _is_world_only_mvp_asset(asset, mvp_exclusions)
                    )
                ]
                # Grouping textures by role needs every member on disk, so an
                # unresolved path is dropped here rather than poisoning the
                # group it belongs to.
                unresolved = [a for a in texture_assets if not a.resolved_path]
                if unresolved:
                    self.runner.emit_log(
                        "WARN",
                        f"[Textures] {len(unresolved)} referenced texture(s) did not "
                        f"resolve and were skipped, first: {unresolved[0].source_path}",
                    )
                params = _params_for_convert_textures(
                    shim, [a for a in texture_assets if a.resolved_path]
                )
                params["convert_all"] = False
                params["nif_paths"] = [
                    {
                        "source_path": asset.source_path,
                        "resolved_path": asset.resolved_path,
                    }
                    for asset in nif_assets
                ]
                stages.append(
                    WaveStage(
                        phase=self._wave_plan.texture_phase,
                        run_id=self.runs.textures.id,
                        mod_path=str(shim.mod_path),
                        source_extracted_dir=str(
                            getattr(shim, "source_data_dir", "") or ""
                        ),
                        target_extracted_dir=str(shim.target_extracted_dir or "")
                        or None,
                        target_data_dir=str(shim.target_data_dir or "") or None,
                        params=params,
                    )
                )
            else:
                # Mirror of asset_phases.py::phase_convert_textures_native.
                from bacup_lib.workflows.asset_phases import (
                    _params_for_convert_textures,
                )

                source_extracted = str(getattr(shim, "source_data_dir", "") or "")
                params = _params_for_convert_textures(shim, [])
                params["convert_all"] = True
                params["source_extracted"] = source_extracted
                params["terrain_jobs"] = list(self.driver.terrain_texture_jobs)
                stages.append(
                    WaveStage(
                        phase="convert_textures_v2",
                        run_id=self.runs.textures.id,
                        mod_path=str(shim.mod_path),
                        source_extracted_dir=source_extracted,
                        target_extracted_dir=str(shim.target_extracted_dir or "")
                        or None,
                        target_data_dir=str(shim.target_data_dir or "") or None,
                        params=params,
                    )
                )
        if (
            self._wave_plan.material_phase is not None
            and self.toggles.materials
            and self.runs.textures
        ):
            # Mirror of asset_phases.py::phase_convert_materials_native
            # whole-plugin convert-all branch; routed to the materials engine
            # via convert_materials_v2.
            from bacup_lib.workflows.asset_phases import (
                _is_bgsm_or_bgem_asset,
                _params_for_convert_material_assets,
            )

            mat_assets = [
                a for a in shim.graph.all_assets if a.asset_type == "material"
            ]
            graph_cdb_mats = [a for a in mat_assets if not _is_bgsm_or_bgem_asset(a)]
            params = _params_for_convert_material_assets(shim, graph_cdb_mats)
            params["convert_all"] = True
            source_extracted = str(getattr(shim, "source_data_dir", "") or "")
            stages.append(
                WaveStage(
                    phase=self._wave_plan.material_phase,
                    run_id=self.runs.textures.id,
                    mod_path=str(shim.mod_path),
                    source_extracted_dir=source_extracted,
                    target_extracted_dir=str(shim.target_extracted_dir or "") or None,
                    target_data_dir=str(shim.target_data_dir or "") or None,
                    params=params,
                )
            )
        # Grass NIF top-up: the terrain phase appends grass assets to
        # ctx.assets AFTER wave A2 snapshotted the NIF list.
        # Textures/materials cover theirs via convert-all enumeration; NIFs
        # are entry-list driven and need the delta converted here.
        if (
            self._wave_plan.grass_topup
            and self.toggles.nifs
            and self.runs.nifs
            and self._a2_nif_keys
        ):
            delta = [
                a
                for a in list(self.driver.ctx.assets)
                if a.asset_type == "nif" and _asset_key_pair(a) not in self._a2_nif_keys
                and (
                    not mvp_exclusions
                    or _is_world_only_mvp_asset(a, mvp_exclusions)
                )
            ]
            if delta:
                from bacup_lib.workflows.asset_phases import (
                    _params_for_convert_nifs,
                )

                # Without this base-game filter the top-up re-converts
                # terrain-manifest spelling twins of base-game NIFs (the delta
                # key above does not collapse spelling variants) and overwrites
                # the loose tree AFTER the sink already banked wave A2's bytes.
                convert_assets = []
                base_game_skipped = 0
                stale_outputs_removed = 0
                for asset in delta:
                    if shim._target_has_asset(asset):
                        base_game_skipped += 1
                        if shim._remove_stale_asset_output(asset):
                            stale_outputs_removed += 1
                        shim._track_asset(
                            asset, "base_game_skip", "exists in target game"
                        )
                    elif not asset.resolved_path:
                        shim._summary.nifs_failed += 1
                    else:
                        convert_assets.append(asset)
                if base_game_skipped:
                    shim._summary.nifs_base_game_skipped += base_game_skipped
                    self.runner.emit_log(
                        "INFO",
                        f"[NIF] grass top-up: skipped {base_game_skipped} NIFs "
                        "already present in target game",
                    )
                if stale_outputs_removed:
                    self.runner.emit_log(
                        "INFO",
                        f"[NIF] grass top-up: removed {stale_outputs_removed} stale NIF output(s) for target-game skips",
                    )
                shim._summary.nifs_total += len(delta)
                self.runner.emit_log(
                    "INFO",
                    f"[NIF] grass top-up: {len(convert_assets)} terrain-appended NIF(s) (wave A3)",
                )
                if convert_assets:
                    params = _params_for_convert_nifs(shim, convert_assets)
                    if str(shim.source_game).lower() == "skyrimse":
                        params["bgsm_output_dir"] = str(
                            Path(str(shim.mod_path)) / "data" / "Materials"
                        )
                    stages.append(
                        WaveStage(
                            phase="convert_nifs_v2",
                            run_id=self.runs.nifs.id,
                            mod_path=str(shim.mod_path),
                            source_extracted_dir=str(shim.target_extracted_dir or ""),
                            params=params,
                        )
                    )
        return stages

    def build_wave_a4(self) -> list["WaveStage"]:
        if not self._wave_plan.wave_a4:
            return []
        shim = self._shim()
        stages: list[WaveStage] = []
        source_extracted = str(getattr(shim, "source_data_dir", "") or "")
        if self.toggles.havok and self.runs.havok:
            # Whole-plugin: enumerate-and-convert every source .hkx in Rust
            # (deduped against the FO4 base set), mirroring the convert-all
            # texture/material stages in wave A3. The dependency graph only
            # reaches actor behaviors via NPC/RACE refs, so a graph walk
            # silently drops UniqueBehaviors/GenericBehaviors/weapon behaviors
            # (e.g. BroZookaFX) — graph discovery is for bounded / cell-slice
            # runs only.
            from bacup_lib.workflows.asset_phases import (
                _params_for_convert_havok,
            )

            params = _params_for_convert_havok(shim)
            params["convert_all"] = True
            stages.append(
                WaveStage(
                    phase="convert_havok",
                    run_id=self.runs.havok.id,
                    mod_path=str(shim.mod_path),
                    source_extracted_dir=source_extracted,
                    target_extracted_dir=str(shim.target_extracted_dir or "") or None,
                    target_data_dir=str(shim.target_data_dir or "") or None,
                    params=params,
                )
            )
        if self.toggles.drivers and self.toggles.havok and self.runs.havok:
            stages.append(
                WaveStage(
                    phase="synthesize_drivers",
                    run_id=self.runs.havok.id,
                    mod_path=str(shim.mod_path),
                    source_extracted_dir=str(shim.target_extracted_dir or ""),
                    params={},
                    after=("convert_havok",),
                )
            )
        src_engine = getattr(shim._source_profile, "engine", None)
        tgt_engine = getattr(shim._target_profile, "engine", None)
        if (
            self.toggles.animations
            and src_engine == "gamebryo"
            and tgt_engine == "creation1"
        ):
            from bacup_lib.workflows.asset_phases import (
                _params_for_convert_animations,
            )

            after = ()
            if self.toggles.drivers and self.toggles.havok:
                after = ("synthesize_drivers",)
            elif self.toggles.havok:
                after = ("convert_havok",)
            stages.append(
                WaveStage(
                    phase="convert_animations",
                    run_id=(self.runs.havok or self.runs.nifs).id,
                    mod_path=str(shim.mod_path),
                    source_extracted_dir=str(shim.target_extracted_dir or ""),
                    params=_params_for_convert_animations(shim),
                    after=after,
                )
            )
        if self.toggles.havok and self.runs.havok:
            if (
                self.toggles.animations
                and src_engine == "gamebryo"
                and tgt_engine == "creation1"
            ):
                after = ("convert_animations",)
            elif self.toggles.drivers and self.toggles.havok:
                after = ("synthesize_drivers",)
            else:
                after = ("convert_havok",)
            stages.append(
                WaveStage(
                    phase="postprocess_havok_assets",
                    run_id=self.runs.havok.id,
                    mod_path=str(shim.mod_path),
                    source_extracted_dir=source_extracted,
                    params={},
                    after=after,
                )
            )
        facegen_run = self.runs.nifs or self.runs.textures or self.runs.havok
        if facegen_run:
            stages.append(
                WaveStage(
                    phase="copy_materialized_facegen",
                    run_id=facegen_run.id,
                    mod_path=str(shim.mod_path),
                    source_extracted_dir=source_extracted,
                    target_extracted_dir=str(shim.target_extracted_dir or "") or None,
                    target_data_dir=str(shim.target_data_dir or "") or None,
                    params={},
                    after=(stages[-1].phase,) if stages else (),
                )
            )
        return stages


# Phase -> ConversionSummary merge (the wrappers' report mappings).
def _merge_wave_report_into_summary(
    summary: ConversionSummary, phase: str, report: dict
) -> None:
    written = int(report.get("assets_written", 0) or 0)
    dropped = int(report.get("records_dropped", 0) or 0)
    warnings = int(report.get("warnings", 0) or 0)
    if phase == "copy_sounds":
        summary.audio_copied += written
        summary.audio_base_game_skipped += dropped
        summary.audio_failed += warnings
    elif phase == "convert_havok":
        summary.havok_total += written + warnings + dropped
        summary.havok_converted += written
        summary.havok_base_game_skipped += dropped
        summary.havok_failed += warnings
    elif phase in {"convert_nifs_v2", "convert_gamebryo_nifs"}:
        summary.nifs_converted += written
        summary.nifs_failed += warnings
    elif phase == "convert_btos_v2":
        summary.btos_converted += written
        summary.btos_failed += warnings
    elif phase in {"convert_textures_v2", "copy_textures"}:
        summary.textures_total += written + warnings + dropped
        summary.textures_converted += written
        summary.textures_failed += warnings
        summary.textures_base_game_skipped += dropped
    elif phase == "convert_materials_v2":
        summary.materials_converted += written
        summary.materials_failed += warnings
    elif phase == "convert_animations":
        summary.animations_converted += written
        summary.animations_failed += warnings
    elif phase == "postprocess_havok_assets":
        summary.havok_converted += written
        summary.havok_failed += warnings


class MultiRunDrainer:
    """Polls conversion_run_drain_events for every live asset run id every
    0.25 s, forwarding events to the runner (Drainer's dispatch shape) and
    capturing per-phase Completed reports for the summary merge + the live
    stage set for the run_state mirror."""

    def __init__(
        self, run_ids: list[int], runner: "ConversionRunner", hz: float = 4.0
    ) -> None:
        self._run_ids = list(run_ids)
        self._runner = runner
        self._period = 1.0 / hz
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._drain_lock = threading.Lock()
        self._lock = threading.Lock()
        self.completed_reports: list[tuple[str, dict]] = []
        self.active_stages: set[str] = set()
        self.stage_counters: dict[str, int] = {}
        self.stage_totals: dict[str, int] = {}
        self.stage_items: dict[str, str] = {}
        self.stage_errors: dict[str, str] = {}
        self.completed_stages: set[str] = set()

    def start(self) -> None:
        self._thread = threading.Thread(
            target=self._loop, daemon=True, name="unified-asset-drainer"
        )
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=2.0)
        self._drain_once()

    def drain_completed(self) -> list[tuple[str, dict]]:
        with self._lock:
            out = list(self.completed_reports)
            self.completed_reports.clear()
        return out

    def reconcile_pipeline_report(self, report: dict) -> None:
        for row in report.get("stages", []):
            if not isinstance(row, (list, tuple)) or len(row) < 5:
                continue
            stage, items_done, items_failed, _warnings, elapsed_ms = row[:5]
            self._complete_stage(
                str(stage),
                items_done=self._event_int(items_done),
                items_failed=self._event_int(items_failed),
                elapsed_ms=elapsed_ms,
            )

    def describe_active(self) -> str:
        with self._lock:
            stages = sorted(
                set(self.active_stages)
                | set(self.stage_counters)
                | set(self.stage_errors)
            )
            counters = dict(self.stage_counters)
            totals = dict(self.stage_totals)
            items = dict(self.stage_items)
            errors = dict(self.stage_errors)

        parts: list[str] = []
        for stage in stages:
            detail = stage
            current = counters.get(stage)
            total = totals.get(stage)
            if current is not None or total is not None:
                detail += f" {current or 0}/{total or 0}"
            item = items.get(stage)
            if item:
                detail += f" {item}"
            error = errors.get(stage)
            if error:
                detail += f" error={error}"
            parts.append(detail)
        return "; ".join(parts)

    @staticmethod
    def _event_int(value) -> int:
        try:
            return max(0, int(value or 0))
        except (TypeError, ValueError):
            return 0

    @staticmethod
    def _elapsed_seconds_from_ms(value) -> float | None:
        if not isinstance(value, (int, float)):
            return None
        return max(0.0, float(value) / 1000.0)

    def _emit_stage_progress(
        self,
        stage: str,
        *,
        status: str,
        completed: int = 0,
        total: int = 0,
        item: str = "",
        error: str | None = None,
        elapsed_ms=None,
    ) -> PhaseProgress:
        progress = PhaseProgress(
            phase=0,
            phase_name=stage,
            total_items=total,
            completed_items=completed,
            current_item=item,
            status=status,
            error=error,
            elapsed_seconds=self._elapsed_seconds_from_ms(elapsed_ms),
        )
        if status == "running" and completed == 0 and total == 0:
            self._runner.emit_phase_start(progress)
        elif status == "running":
            self._runner.emit_item_progress(progress)
        else:
            self._runner.emit_phase_complete(progress)
        return progress

    def _complete_stage(
        self,
        stage: str,
        *,
        items_done: int,
        items_failed: int,
        elapsed_ms=None,
    ) -> None:
        total = items_done + items_failed
        with self._lock:
            if stage in self.completed_stages:
                return
            self.completed_stages.add(stage)
            self.active_stages.discard(stage)
            self.stage_counters[stage] = total
            self.stage_totals[stage] = total
            self.stage_items.pop(stage, None)
            self.stage_errors.pop(stage, None)
        self._emit_stage_progress(
            stage,
            status="completed",
            completed=total,
            total=total,
            elapsed_ms=elapsed_ms,
        )

    def _loop(self) -> None:
        native = load_native_module()
        while not self._stop.is_set():
            self._drain_once(native)
            time.sleep(self._period)

    def _drain_once(self, native=None) -> None:
        if native is None:
            native = load_native_module()
        with self._drain_lock:
            for run_id in self._run_ids:
                try:
                    events = native.conversion_run_drain_events(run_id, 256)
                except Exception:
                    continue
                for ev in events:
                    self._dispatch(ev)

    def _dispatch(self, ev: dict) -> None:
        kind = ev.get("kind")
        if kind == "log":
            self._runner.emit_log(ev.get("level", "INFO"), ev.get("message", ""))
        elif kind == "progress":
            phase = ev.get("phase", "")
            current = self._event_int(ev.get("current", 0))
            total = self._event_int(ev.get("total", 0))
            item = str(ev.get("item", "") or "")
            stage = str(phase)
            with self._lock:
                if stage in self.completed_stages:
                    return
                self.stage_counters[stage] = current
                self.stage_totals[stage] = total
                if item:
                    self.stage_items[stage] = item
            self._emit_stage_progress(
                stage,
                status="running",
                completed=current,
                total=total,
                item=item,
            )
            self._runner.emit_log(
                "INFO", f"[{phase}] {current}/{total} {item}".rstrip()
            )
        elif kind == "started":
            phase = str(ev.get("phase", "") or "")
            with self._lock:
                self.completed_stages.discard(phase)
            self._emit_stage_progress(phase, status="running")
            self._runner.emit_log("INFO", f"phase started: {phase}")
        elif kind == "completed":
            phase = str(ev.get("phase", ""))
            report = dict(ev.get("report", {}) or {})
            with self._lock:
                self.completed_reports.append((phase, report))
            completed = self._event_int(report.get("assets_written", 0))
            failed = self._event_int(report.get("warnings", 0)) + self._event_int(
                report.get("items_failed", 0)
            )
            self._complete_stage(
                phase,
                items_done=completed,
                items_failed=failed,
                elapsed_ms=report.get("elapsed_ms"),
            )
            self._runner.emit_log(
                "INFO",
                f"phase completed: {phase} "
                f"written={report.get('assets_written', 0)} "
                f"warnings={report.get('warnings', 0)} "
                f"items_failed={report.get('items_failed', 0)} "
                f"elapsed_ms={report.get('elapsed_ms', 0)}",
            )
        elif kind == "stage_started":
            stage = str(ev.get("stage", ""))
            with self._lock:
                self.completed_stages.discard(stage)
                self.active_stages.add(stage)
                self.stage_errors.pop(stage, None)
            self._emit_stage_progress(stage, status="running")
            self._runner.emit_log("INFO", f"stage started: {stage}")
        elif kind == "stage_completed":
            stage = str(ev.get("stage", ""))
            items_done = self._event_int(ev.get("items_done", 0))
            self._complete_stage(
                stage,
                items_done=items_done,
                items_failed=self._event_int(ev.get("items_failed", 0)),
                elapsed_ms=ev.get("elapsed_ms"),
            )
            self._runner.emit_log(
                "INFO",
                f"stage completed: {stage} items_done={items_done} "
                f"items_failed={ev.get('items_failed', 0)} "
                f"elapsed_ms={ev.get('elapsed_ms', 0)}",
            )
        elif kind == "stage_failed":
            stage = str(ev.get("stage", ""))
            message = str(ev.get("message", "") or "")
            with self._lock:
                self.active_stages.discard(stage)
                self.stage_errors[stage] = message
            self._emit_stage_progress(
                stage,
                status="error",
                error=message,
            )
            self._runner.emit_log("ERROR", f"stage failed: {stage}: {message}")


class RunStateMirror:
    """Python mirror writing run_state.json every heartbeat.

    The native pipeline writes run_state.assets.json for the waves; this file
    joins both tracks.
    """

    def __init__(
        self, path: Path, driver: "UnifiedDriver", heartbeat_seconds: float = 30.0
    ) -> None:
        self._path = Path(path)
        self._driver = driver
        self._heartbeat = heartbeat_seconds
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._started_at = datetime.now(timezone.utc)
        self.drainer: MultiRunDrainer | None = None
        self.status = "running"

    def start(self) -> None:
        self.write_now()
        self._thread = threading.Thread(
            target=self._loop, daemon=True, name="unified-run-state"
        )
        self._thread.start()

    def finish(self, status: str) -> None:
        self.status = status
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=2.0)
        self.write_now()

    def _loop(self) -> None:
        while not self._stop.wait(self._heartbeat):
            self.write_now()

    def _rss_bytes(self) -> int:
        try:
            import psutil

            return int(psutil.Process().memory_info().rss)
        except Exception:
            return 0

    def write_now(self) -> None:
        record_label = self._driver.current_record_label or "-"
        asset_stages = "-"
        counters: dict[str, int] = {}
        if self.drainer is not None:
            with self.drainer._lock:
                active = sorted(self.drainer.active_stages)
                counters = dict(self.drainer.stage_counters)
            if active:
                asset_stages = "+".join(active)
        state = {
            "status": self.status,
            "stage": f"record:{record_label}+assets:{asset_stages}",
            "started_at": self._started_at.isoformat(timespec="milliseconds"),
            "updated_at": datetime.now(timezone.utc).isoformat(
                timespec="milliseconds"
            ),
            "counters": counters,
            "rss_bytes": self._rss_bytes(),
        }
        try:
            tmp = self._path.with_suffix(".json.tmp")
            tmp.write_text(json.dumps(state, indent=1), encoding="utf-8")
            os.replace(tmp, self._path)
        except OSError:
            pass  # write failures never abort (locked schema contract)


def run_asset_track(
    driver: "UnifiedDriver",
    runner: "ConversionRunner",
    toggles: "AssetWaveToggles",
    *,
    mod_root: Path,
    diagnostics_root: Path | None = None,
    max_asset_failures: int | None = None,
    mirror: "RunStateMirror | None" = None,
    asset_conversion_workers: int | None = None,
) -> "AssetRuns | None":
    """The asset thread body: wait for the launch gates, run the three wave
    plans via conversion_pipeline_run, merge summaries. Returns the AssetRuns
    (caller drops them after the join) or None when nothing ran."""
    native = load_native_module()
    signals = driver.signals

    def gate(event: threading.Event) -> bool:
        event.wait()
        return not signals.record_failed.is_set()

    if not gate(signals.assets_ready):
        signals.asset_a2_done.set()
        return None
    if not any(
        (
            toggles.nifs,
            toggles.btos,
            toggles.textures,
            toggles.materials,
            toggles.havok,
            toggles.drivers,
            toggles.sounds,
        )
    ):
        signals.asset_a2_done.set()
        return None

    runs: AssetRuns | None = None
    builder: AssetWaveBuilder | None = None
    drainer: MultiRunDrainer | None = None
    diagnostics_root = Path(diagnostics_root or mod_root)
    diagnostics_root.mkdir(parents=True, exist_ok=True)
    run_state_path = diagnostics_root / "run_state.assets.json"

    def ensure_runs() -> tuple["AssetRuns", "AssetWaveBuilder", "MultiRunDrainer"]:
        nonlocal runs, builder, drainer
        if runs is None:
            if asset_conversion_workers is not None:
                runner.emit_log(
                    "INFO",
                    f"asset waves: conversion_workers={int(asset_conversion_workers)}",
                )
            runs = AssetRuns(
                driver.ctx,
                toggles,
                conversion_workers=asset_conversion_workers,
            )
            if driver.sink_id is not None:
                runs.attach_sink(driver.sink_id)
            builder = AssetWaveBuilder(
                driver,
                toggles,
                runs,
                runner,
                conversion_workers=asset_conversion_workers,
            )
            drainer = MultiRunDrainer([r.id for r in runs.all_runs()], runner)
            if mirror is not None:
                mirror.drainer = drainer
            drainer.start()
        assert builder is not None
        assert drainer is not None
        return runs, builder, drainer

    def run_wave(name: str, stages: list["WaveStage"]) -> None:
        if not stages:
            return
        _, _, active_drainer = ensure_runs()
        plan = {
            "events_run_id": stages[0].run_id,
            "run_state_path": str(run_state_path),
            "stages": [s.to_plan_stage() for s in stages],
        }
        if max_asset_failures is not None:
            plan["max_asset_failures"] = int(max_asset_failures)
        runner.emit_log(
            "INFO",
            f"asset wave {name}: {len(stages)} stage(s): "
            + ", ".join(s.phase for s in stages),
        )
        started = time.perf_counter()
        try:
            pipeline_report = native.conversion_pipeline_run(json.dumps(plan))
        except Exception as exc:
            active_drainer._drain_once(native)
            context = active_drainer.describe_active()
            if context:
                raise RuntimeError(
                    f"asset wave {name} failed while {context}: {exc}"
                ) from exc
            raise
        active_drainer._drain_once(native)
        active_drainer.reconcile_pipeline_report(pipeline_report)
        runner.emit_log(
            "INFO",
            f"asset wave {name} completed in {time.perf_counter() - started:.1f}s",
        )
        for phase, report in active_drainer.drain_completed():
            _merge_wave_report_into_summary(driver.ctx.summary, phase, report)

    try:
        if driver.defer_asset_a2_until_record_done:
            if not gate(signals.terrain_done):
                signals.asset_a2_done.set()
                return runs
            _, active_builder, _ = ensure_runs()
            run_wave("A3", active_builder.build_wave_a3())
            if not gate(signals.record_done):
                signals.asset_a2_done.set()
                return runs
            run_wave("A1", active_builder.build_wave_a1())
            run_wave("A2", active_builder.build_wave_a2())
            signals.asset_a2_done.set()
            run_wave("A4", active_builder.build_wave_a4())
            return runs
        _, active_builder, _ = ensure_runs()
        run_wave("A1", active_builder.build_wave_a1())
        if not gate(signals.fixups_done):
            signals.asset_a2_done.set()
            return runs
        run_wave("A2", active_builder.build_wave_a2())
        signals.asset_a2_done.set()
        if not gate(signals.terrain_done):
            return runs
        run_wave("A3", active_builder.build_wave_a3())
        run_wave("A4", active_builder.build_wave_a4())
        return runs
    except Exception:
        # A wave failure stops the record track at its next phase boundary:
        # the flag covers the pre-translate window (no native run yet), the
        # native cancel reaches a running phase once the run exists.
        driver.asset_track_failed.set()
        signals.asset_a2_done.set()
        if driver.record_run_id is not None:
            try:
                native.conversion_run_cancel(driver.record_run_id)
            except Exception:
                pass
        raise
    finally:
        if drainer is not None:
            drainer.stop()
            for phase, report in drainer.drain_completed():
                _merge_wave_report_into_summary(driver.ctx.summary, phase, report)


@dataclass
class UnifiedRunResult:
    summary: ConversionSummary
    run_result: object
    planned_archives: list[PlannedArchive] = field(default_factory=list)


def _run_collision_validation(
    meshes_root: "Path",
    report_dir: "Path",
    runner: "ConversionRunner",
    workers: int | None = None,
) -> None:
    """Report-only collision gate. Logs findings; never raises."""
    try:
        from creation_lib.havok.collision_validation import validate_collision_root

        summary = validate_collision_root(
            meshes_root, report_dir=report_dir, workers=workers
        )
        runner.emit_log(
            "INFO",
            "collision validation: %d NIFs / %d blobs / %d errors / %d warnings (%d workers, report: %s)"
            % (
                summary["nifs"], summary["blobs"], summary["errors"],
                summary["warnings"], summary["workers"], report_dir,
            ),
        )
    except Exception as exc:  # never fail the regen on the gate
        runner.emit_log("WARN", "collision validation skipped: %s" % exc)


def _run_post_phase(label: str, body, runner: "ConversionRunner") -> None:
    progress = PhaseProgress(
        phase=0,
        phase_name=label,
        status="running",
    )
    started = time.perf_counter()
    runner.emit_phase_start(progress)
    try:
        body(progress)
    except Exception as exc:
        progress.status = "error"
        progress.error = str(exc)
        progress.elapsed_seconds = _elapsed_seconds(started)
        runner.emit_log("ERROR", f"{label} failed: {exc}")
        runner.emit_phase_complete(progress)
        raise
    progress.status = "completed"
    progress.elapsed_seconds = _elapsed_seconds(started)
    runner.emit_phase_complete(progress)


def _remove_existing_anim_text_data(mod_dir: Path) -> int:
    data_root = mod_dir / "data"
    if not data_root.is_dir():
        return 0

    removed = 0
    for meshes_root in data_root.iterdir():
        if not meshes_root.is_dir() or meshes_root.name.casefold() != "meshes":
            continue
        for candidate in meshes_root.iterdir():
            if candidate.is_dir() and candidate.name.casefold() == "animtextdata":
                shutil.rmtree(candidate)
                removed += 1
    return removed


def _run_anim_text_data_generation(
    ctx: ConversionContext,
    runner: "ConversionRunner",
    force_native: bool = False,
    *,
    progress: PhaseProgress | None = None,
) -> None:
    if ctx.target_game != "fo4":
        raise RuntimeError(
            "AnimTextData generation is only supported for FO4 targets; "
            f"got {ctx.target_game!r}"
        )

    mod_dir = Path(ctx.mod_path)
    plugin_name = str(ctx.output_plugin_name or "")
    if not plugin_name:
        raise RuntimeError("AnimTextData generation requires output_plugin_name")
    plugin_path = mod_dir / plugin_name
    if not plugin_path.is_file():
        raise FileNotFoundError(f"{plugin_path} not found. Build the mod first.")

    removed = _remove_existing_anim_text_data(mod_dir)
    if removed:
        runner.emit_log(
            "INFO",
            f"animtext: removed {removed} stale AnimTextData tree(s)",
        )

    # Prefer CK when it is present (full-fidelity, all buckets). End users who lack
    # CK get the native CK-free generator. `force_native` overrides CK detection to
    # exercise/ship the CK-free path even on a dev box that has CK installed. CK
    # availability is decided from the passed-in target_data_dir (no env read), per
    # the library env-read policy.
    game_data_dir = Path(ctx.target_data_dir) if ctx.target_data_dir else None
    ck_exe = (game_data_dir.parent / "CreationKit.exe") if game_data_dir else None
    if not force_native and ck_exe is not None and ck_exe.is_file():
        _run_anim_text_data_via_ck(
            ctx,
            runner,
            mod_dir,
            plugin_name,
            game_data_dir,
            progress=progress,
        )
    else:
        if force_native and ck_exe is not None and ck_exe.is_file():
            runner.emit_log(
                "INFO",
                "animtext: CreationKit.exe present but --anim-text-data-native set; "
                "using the CK-free native generator",
            )
        _run_anim_text_data_native(ctx, runner, plugin_path, progress=progress)


def _run_anim_text_data_via_ck(
    ctx: ConversionContext,
    runner: "ConversionRunner",
    mod_dir: Path,
    plugin_name: str,
    game_data_dir: Path,
    *,
    progress: PhaseProgress | None = None,
) -> None:
    from creation_lib.ck.automation import generate_anim_data

    def on_progress(message: str) -> None:
        runner.emit_log("INFO", f"animtext: {message}")
        if progress is not None:
            progress.current_item = message
            runner.emit_item_progress(progress)

    runner.emit_log(
        "INFO",
        f"animtext: staging {plugin_name} in {game_data_dir} and running CK -GenerateAnimInfo",
    )
    result = generate_anim_data(
        Path(plugin_name).stem,
        game=ctx.target_game,
        game_dir=game_data_dir.parent,
        game_data_dir=game_data_dir,
        mod_dir=mod_dir,
        plugin_name=plugin_name,
        deploy_loose_data=True,
        loose_data_roots=("Meshes",),
        on_progress=on_progress,
    )
    if result is None:
        runner.emit_log("WARN", "animtext: CK did not generate AnimTextData")


def _run_anim_text_data_native(
    ctx: ConversionContext,
    runner: "ConversionRunner",
    plugin_path: Path,
    *,
    progress: PhaseProgress | None = None,
) -> None:
    """CK-free AnimTextData generation: read the built plugin's RACE subgraphs and
    write the bucket files natively. Currently emits AnimationFileData; the other
    buckets fall back to CK when available."""
    from creation_lib.ck.anim_text_data import generate_anim_text_data
    from bacup_lib.native_runtime import load_native_module

    mod_dir = plugin_path.parent
    meshes_root = mod_dir / "data" / "Meshes"
    if not meshes_root.is_dir():
        alt = mod_dir / "data" / "meshes"
        if alt.is_dir():
            meshes_root = alt
    if not meshes_root.is_dir():
        runner.emit_log(
            "WARN",
            f"animtext: no loose Meshes at {meshes_root}; skipping CK-free generation",
        )
        return

    # Weapon/character subgraphs inject into the base-game character graph. The
    # target asset store stages only the RACE/SAPT behavior closure; the legacy
    # loose overlay remains a fallback for callers without a catalog.
    base_meshes_root: Path | None = None
    target_asset_store = getattr(ctx, "target_asset_store", None)
    if target_asset_store is None and ctx.target_extracted_dir:
        candidate = Path(ctx.target_extracted_dir) / "Meshes"
        if candidate.is_dir():
            base_meshes_root = candidate
        else:
            runner.emit_log(
                "WARN",
                f"animtext: base meshes {candidate} missing; weapon/character "
                "subgraphs will be skipped (3rd-person/NPC anims may not cache)",
            )
    elif target_asset_store is None:
        runner.emit_log(
            "WARN",
            "animtext: no target_extracted_dir; weapon/character subgraphs will be "
            "skipped (3rd-person/NPC anims may not cache)",
        )

    base_race_path = getattr(ctx, "anim_text_data_base_race_path", None)
    if base_race_path is None:
        base_race_path = next(
            (
                path
                for path in resolve_target_master_paths(
                    ctx.target_game,
                    target_data_dir=ctx.target_data_dir,
                    target_extracted_dir=ctx.target_extracted_dir,
                )
                if path.name.casefold() == "fallout4.esm"
            ),
            None,
        )

    base_race_paths = [Path(base_race_path)] if base_race_path is not None else []
    if target_asset_store is not None:
        native = load_native_module()
        base_meshes_root = Path(
            native.conversion_prepare_anim_text_data_assets(
                str(plugin_path),
                ctx.target_game,
                [str(path) for path in base_race_paths],
                str(meshes_root),
                str(ctx.target_data_dir),
                str(ctx.target_asset_catalog_path),
                str(ctx.target_asset_cache_dir),
                str(ctx.target_extracted_dir) if ctx.target_extracted_dir else None,
            )
        )

    runner.emit_log(
        "INFO",
        f"animtext: CK-free generation from {plugin_path.name} RACE subgraphs",
    )

    def on_progress(message: str) -> None:
        runner.emit_log("INFO", f"animtext: {message}")
        if progress is not None:
            progress.current_item = message
            runner.emit_item_progress(progress)

    count = generate_anim_text_data(
        plugin_path,
        game=ctx.target_game,
        source_meshes_root=meshes_root,
        output_meshes_root=meshes_root,
        base_meshes_root=base_meshes_root,
        base_plugin_paths=base_race_paths,
        mod_prefix=getattr(ctx, "mod_prefix", None),
        progress_callback=on_progress,
    )
    runner.emit_log("INFO", f"animtext: wrote {count} AnimTextData bucket file(s)")


def _regenerate_modt_after_asset_waves(
    driver: "UnifiedDriver",
    runner: "ConversionRunner",
    mod_root: Path,
    *,
    progress: PhaseProgress | None = None,
) -> None:
    ctx = driver.ctx
    if (
        ctx is None
        or driver._req.target_game.lower() != "fo4"
        or not driver._req.options.build_esp
    ):
        return

    output_path = mod_root / ctx.output_plugin_name
    if not output_path.is_file():
        raise FileNotFoundError(
            f"MODT regeneration requires the built output plugin: {output_path}"
        )

    from bacup_lib.run import ConversionRun

    manifest_path = mod_root / "debug" / "modt" / "mesh_manifest.json"
    temp_output_path: Path | None = None

    def update_progress(completed_items: int, current_item: str) -> None:
        if progress is None:
            return
        progress.total_items = 3
        progress.completed_items = completed_items
        progress.current_item = current_item
        runner.emit_item_progress(progress)

    update_progress(0, "Building mesh manifest")
    try:
        with ConversionRun.open_existing(
            driver._req.source_game,
            driver._req.target_game,
            None,
            str(output_path),
            config=driver.record_runtime._native_run_config(ctx),
        ) as run:
            manifest_report = run.run_phase(
                "emit_modt_manifest",
                mod_path=str(mod_root),
                params={"manifest_path": str(manifest_path)},
            )
            update_progress(1, "Regenerating MODT records")
            modt_report = run.run_phase(
                "regenerate_modt",
                mod_path=str(mod_root),
                params={
                    "manifest_path": str(manifest_path),
                    "is_upgrade": False,
                },
            )
            update_progress(2, "Saving updated plugin")
            temp_fd, temp_name = tempfile.mkstemp(
                dir=output_path.parent,
                prefix=f".{output_path.name}.",
                suffix=".tmp",
            )
            temp_output_path = Path(temp_name)
            os.close(temp_fd)
            run.save_target(str(temp_output_path), run_nvnm_validator=False)
        os.replace(temp_output_path, output_path)
        _cleanup_temp_save_strings(temp_output_path)
        temp_output_path = None
        update_progress(3, "")
    finally:
        if temp_output_path is not None:
            _cleanup_temp_save_strings(temp_output_path)
            try:
                temp_output_path.unlink(missing_ok=True)
            except OSError:
                pass

    runner.emit_log(
        "INFO",
        "post-asset MODT regeneration: "
        f"manifest_entries={manifest_report.get('records_changed', 0)} "
        f"records_changed={modt_report.get('records_changed', 0)}",
    )


def _generate_precombines_after_asset_waves(
    driver: "UnifiedDriver",
    runner: "ConversionRunner",
    mod_root: Path,
    *,
    progress: PhaseProgress | None = None,
) -> None:
    """EXPERIMENTAL post-asset precombine generation (gated off by default; see
    models.PhaseSelection.generate_precombines).

    Runs in the same post-asset stage as MODT regeneration, reopening the freshly
    built ESM to bake `*_OC.nif` and stamp CELL PCMB/XCRI + REFR VC. A phase
    failure is logged and swallowed â€” it must NOT abort the run, and the MODT
    results (which already saved in their own window) still stand.
    """
    ctx = driver.ctx
    if (
        ctx is None
        or driver._req.target_game.lower() != "fo4"
        or not driver._req.options.build_esp
        or not getattr(driver._req.options, "generate_precombines", False)
    ):
        return

    output_path = mod_root / ctx.output_plugin_name
    if not output_path.is_file():
        runner.emit_log(
            "WARN",
            f"generate_precombines skipped: built output plugin missing: {output_path}",
        )
        return

    from bacup_lib.run import ConversionRun

    temp_output_path: Path | None = None

    def update_progress(completed_items: int, current_item: str) -> None:
        if progress is None:
            return
        progress.total_items = 2
        progress.completed_items = completed_items
        progress.current_item = current_item
        runner.emit_item_progress(progress)

    update_progress(0, "Baking precombines")
    try:
        with ConversionRun.open_existing(
            driver._req.source_game,
            driver._req.target_game,
            None,
            str(output_path),
            config=driver.record_runtime._native_run_config(ctx),
        ) as run:
            try:
                report = run.run_phase(
                    "generate_precombines",
                    mod_path=str(mod_root),
                    params={
                        # TODO(precombine-v1): enrich phase params here. V1.6 makes an
                        # empty include_cells mean "all eligible interior cells"; the
                        # mesh source roots (mesh_extract_roots / mesh_archives) are
                        # supplied by the runner-path work stream and must NOT be
                        # handed to the phase from the pipeline yet.
                        "include_cells": [],
                    },
                )
            except Exception as exc:  # noqa: BLE001 â€” non-fatal by contract
                runner.emit_log("WARN", f"generate_precombines failed: {exc}")
                return
            if not report.get("assets_written"):
                runner.emit_log(
                    "INFO",
                    "generate_precombines: no precombines written "
                    f"(records_changed={report.get('records_changed', 0)}, "
                    f"warnings={report.get('warnings', 0)})",
                )
                return
            update_progress(1, "Saving updated plugin")
            temp_fd, temp_name = tempfile.mkstemp(
                dir=output_path.parent,
                prefix=f".{output_path.name}.",
                suffix=".tmp",
            )
            temp_output_path = Path(temp_name)
            os.close(temp_fd)
            run.save_target(str(temp_output_path), run_nvnm_validator=False)
        os.replace(temp_output_path, output_path)
        temp_output_path = None
        update_progress(2, "")
    finally:
        if temp_output_path is not None:
            try:
                temp_output_path.unlink(missing_ok=True)
            except OSError:
                pass

    runner.emit_log(
        "INFO",
        "post-asset precombine generation: "
        f"assets_written={report.get('assets_written', 0)} "
        f"records_changed={report.get('records_changed', 0)}",
    )


def _rebuild_cell_offsets_after_build(
    driver: "UnifiedDriver",
    runner: "ConversionRunner",
    mod_root: Path,
    *,
    progress: PhaseProgress | None = None,
) -> None:
    """Regenerate WRLD OFST/CLSZ cell seek tables on the built ESM.

    The tables encode the serialized byte layout of each worldspace group, so
    this must run after every other ESM-record mutation: precombine stamping
    (when enabled), MODT regeneration and the term-marker repair all rewrite the
    plugin before this point, so CELL sizes are final before the offsets rebuild.
    """
    ctx = driver.ctx
    if (
        ctx is None
        or driver._req.target_game.lower() != "fo4"
        or not driver._req.options.build_esp
        or not getattr(driver._req.options, "rebuild_cell_offsets", True)
    ):
        return

    output_path = mod_root / ctx.output_plugin_name
    if not output_path.is_file():
        raise FileNotFoundError(
            f"cell offset rebuild requires the built output plugin: {output_path}"
        )

    from bacup_lib.run import ConversionRun

    temp_output_path: Path | None = None

    def update_progress(completed_items: int, current_item: str) -> None:
        if progress is None:
            return
        progress.total_items = 2
        progress.completed_items = completed_items
        progress.current_item = current_item
        runner.emit_item_progress(progress)

    update_progress(0, "Rebuilding WRLD cell offset tables")
    try:
        with ConversionRun.open_existing(
            driver._req.source_game,
            driver._req.target_game,
            None,
            str(output_path),
            config=driver.record_runtime._native_run_config(ctx),
        ) as run:
            report = run.run_phase(
                "rebuild_cell_offsets",
                mod_path=str(mod_root),
            )
            for event in run.drain_events(256):
                if event.get("kind") == "log":
                    runner.emit_log(
                        str(event.get("level", "INFO")).upper(),
                        str(event.get("message", "")),
                    )
            update_progress(1, "Saving updated plugin")
            temp_fd, temp_name = tempfile.mkstemp(
                dir=output_path.parent,
                prefix=f".{output_path.name}.",
                suffix=".tmp",
            )
            temp_output_path = Path(temp_name)
            os.close(temp_fd)
            run.save_target(str(temp_output_path), run_nvnm_validator=False)
        os.replace(temp_output_path, output_path)
        _cleanup_temp_save_strings(temp_output_path)
        temp_output_path = None
        update_progress(2, "")
    finally:
        if temp_output_path is not None:
            _cleanup_temp_save_strings(temp_output_path)
            try:
                temp_output_path.unlink(missing_ok=True)
            except OSError:
                pass

    runner.emit_log(
        "INFO",
        "cell offset tables rebuilt: "
        f"worldspaces={report.get('records_changed', 0)} "
        f"warnings={report.get('warnings', 0)}",
    )


def run_unified(
    request: PluginPortRequest,
    runner: "ConversionRunner",
    *,
    enable_ba2: bool = True,
    archive_max_bytes: int = DEFAULT_ARCHIVE_MAX_BYTES,
    max_asset_failures: int | None = None,
    serialize_tracks: bool = True,
    asset_conversion_workers: int | None = None,
    expanded_archives: bool = True,
    archive_output_dir: Path | None = None,
    fo4_ba2_target: str = "nextgen",
    archive_labels: tuple[str, ...] | None = None,
    lod_hook: "Callable[[Path], None] | None" = None,
    land_cache_hook: "Callable[[ConversionContext], bool] | None" = None,
    record_preflight_complete: bool = False,
) -> "UnifiedRunResult":
    """The unified driver: record track + asset waves + sink join
    + cache manifest. `request.options` = the legacy full-run options (asset
    toggles ON; see UnifiedDriver.__init__)."""
    if not record_preflight_complete:
        _preflight_legacy_packs(request, runner)
    toggles = AssetWaveToggles.from_options(request.options)
    mod_root = _resolved_mod_root(request)
    mod_root.mkdir(parents=True, exist_ok=True)
    diagnostics_root = Path(getattr(request, "diagnostics_root", None) or mod_root)
    diagnostics_root.mkdir(parents=True, exist_ok=True)
    native = load_native_module()

    direct_pack_loose_archives = bool(enable_ba2)
    sink_id = native.sinks_create(
        json.dumps(
            {
                "mod_root": str(mod_root),
                "spill_dir": str(mod_root / "_sink_spills"),
                "emit_loose": True,
                "enable_ba2": bool(enable_ba2 and not direct_pack_loose_archives),
            }
        )
    )
    driver = UnifiedDriver(
        request,
        sink_id=sink_id,
        defer_asset_a2_until_record_done=not serialize_tracks,
    )
    driver.on_land_cache_ready = land_cache_hook
    mirror = RunStateMirror(diagnostics_root / "run_state.json", driver)
    mirror.start()

    asset_runs: "AssetRuns | None" = None
    asset_thread: threading.Thread | None = None
    asset_error: list[BaseException] = []
    cache_entries: list[CacheAssetEntry] | None = None

    def asset_thread_body() -> None:
        nonlocal asset_runs
        try:
            asset_runs = run_asset_track(
                driver,
                runner,
                toggles,
                mod_root=mod_root,
                diagnostics_root=diagnostics_root,
                max_asset_failures=max_asset_failures,
                mirror=mirror,
                asset_conversion_workers=asset_conversion_workers,
            )
        except BaseException as exc:  # noqa: BLE001 — re-raised at join
            asset_error.append(exc)

    def start_asset_thread() -> None:
        nonlocal asset_thread
        if asset_thread is not None:
            return
        asset_thread = threading.Thread(
            target=asset_thread_body, daemon=True, name="unified-asset-track"
        )
        asset_thread.start()

    def release_asset_state_before_lod() -> None:
        nonlocal asset_runs, cache_entries
        if cache_entries is None:
            cache_entries = collect_cache_entries(driver, toggles)
        if driver.ctx is not None:
            driver.ctx.assets = []
        driver.assets = []
        driver.terrain_texture_jobs = []
        if asset_runs is not None:
            asset_runs.drop_all()
            asset_runs = None
        gc.collect()

    try:
        if serialize_tracks:
            # RSS diagnostic lever: record track fully completes before any
            # wave launches.
            driver.run_record_track(runner)
            asset_thread_body()
        else:
            if driver.defer_asset_a2_until_record_done:
                driver.on_terrain_done = start_asset_thread
            else:
                start_asset_thread()
            try:
                driver.run_record_track(runner)
            finally:
                driver.signals.record_done.set()
                if asset_thread is not None:
                    asset_thread.join()
        if asset_error:
            raise asset_error[0]

        _run_post_phase(
            "Regenerate MODT",
            lambda progress: _regenerate_modt_after_asset_waves(
                driver,
                runner,
                mod_root,
                progress=progress,
            ),
            runner,
        )

        # EXPERIMENTAL, gated off: only schedule (and surface) the precombine phase
        # when explicitly enabled, so the default full build is byte-unchanged.
        if (
            request.target_game.lower() == "fo4"
            and getattr(request.options, "generate_precombines", False)
        ):
            _run_post_phase(
                "Generate precombines",
                lambda progress: _generate_precombines_after_asset_waves(
                    driver,
                    runner,
                    mod_root,
                    progress=progress,
                ),
                runner,
            )

        if driver.ctx is not None:
            driver.record_runtime._repair_term_marker_parameters_final(
                driver.ctx,
                runner,
                request.source_plugins[0],
            )

        # Last ESM-record mutation: OFST/CLSZ encode the serialized layout, so
        # every earlier step that rewrites the plugin must already have run.
        _run_post_phase(
            "Rebuild Cell Offsets",
            lambda progress: _rebuild_cell_offsets_after_build(
                driver,
                runner,
                mod_root,
                progress=progress,
            ),
            runner,
        )

        if driver.ctx is not None:
            _finalize_fo76_pipboy_map_texture(request, driver.ctx, runner)
            _run_post_phase(
                "Copy VaultBoy SWFs",
                lambda _progress: _copy_fo76_vaultboy_swfs(
                    request,
                    driver.ctx,
                    runner,
                ),
                runner,
            )

        if getattr(request.options, "generate_anim_text_data", False):
            if driver.ctx is None:
                raise RuntimeError(
                    "AnimTextData generation requires a built conversion context"
                )
            _run_post_phase(
                "Generate AnimTextData",
                lambda progress: _run_anim_text_data_generation(
                    driver.ctx,
                    runner,
                    force_native=getattr(
                        request.options,
                        "anim_text_data_native",
                        False,
                    ),
                    progress=progress,
                ),
                runner,
            )

        if lod_hook is not None:
            release_asset_state_before_lod()
            runner.emit_log("INFO", "lod: native LOD generation (pre-pack)")
            _run_post_phase(
                "Generate LOD",
                lambda _progress: lod_hook(mod_root),
                runner,
            )

        if getattr(request.options, "validate_collision", False):
            collision_meshes_root = mod_root / "data" / "Meshes"
            if not collision_meshes_root.is_dir():
                alt_meshes_root = mod_root / "data" / "meshes"
                if alt_meshes_root.is_dir():
                    collision_meshes_root = alt_meshes_root
            _run_collision_validation(
                collision_meshes_root,
                diagnostics_root / "collision_validation",
                runner,
                workers=asset_conversion_workers,
            )

        cache_manifest_started = time.perf_counter()
        emit_runner_status(runner, "Building conversion cache manifest")
        runner.emit_log("INFO", "postflight: hashing conversion cache manifest")
        write_cache_manifest(
            mod_root,
            cache_entries or collect_cache_entries(driver, toggles),
        )
        runner.emit_log(
            "INFO",
            "postflight: conversion cache manifest complete "
            f"elapsed={time.perf_counter() - cache_manifest_started:.1f}s",
        )
        target_asset_store = getattr(driver.ctx, "target_asset_store", None)
        if target_asset_store is not None:
            stats = target_asset_store.stats()
            runner.emit_log(
                "INFO",
                "Target asset cache: "
                f"hits={stats['cache_hits']} misses={stats['cache_misses']} "
                f"extracted={stats['files_extracted']} "
                f"bytes={stats['bytes_extracted']} "
                f"reindexed={stats['archives_reindexed']}",
            )

        plans: list[PlannedArchive] = []
        if enable_ba2:
            runner.emit_log("INFO", "join: reconcile + plan + finalize BA2 shards")

            def pack_body(progress: PhaseProgress) -> None:
                progress.current_item = "Reconciling and planning BA2 archives"
                runner.emit_item_progress(progress)

                def emit_pack_progress(event: dict) -> bool:
                    try:
                        total = max(0, int(event.get("total", 0) or 0))
                        completed = max(0, int(event.get("completed", 0) or 0))
                    except (TypeError, ValueError):
                        total, completed = 0, 0
                    message = str(event.get("message") or "")
                    if message.startswith("Packing archive "):
                        current_item = message.removeprefix("Packing archive ").split(
                            " files=", 1
                        )[0]
                    elif message.startswith("Archive packed native: name="):
                        archive_name = message.removeprefix(
                            "Archive packed native: name="
                        ).split(" files=", 1)[0]
                        current_item = f"Packed {archive_name}"
                    else:
                        current_item = f"Packing {total} BA2 archives"
                    if completed:
                        progress.total_items = total
                        progress.completed_items = (
                            min(completed, total) if total else completed
                        )
                    else:
                        progress.total_items = 0
                        progress.completed_items = 0
                    progress.current_item = current_item
                    runner.emit_item_progress(progress)
                    return True

                plans.extend(
                    finalize_sinks_for_mod(
                        sink_id,
                        mod_root,
                        mod_name=mod_root.name,
                        archive_max_bytes=archive_max_bytes,
                        reconcile_workers=getattr(
                            request.options, "conversion_workers", None
                        ),
                        direct_pack_all=direct_pack_loose_archives,
                        texture_pack_workers=getattr(
                            request.options, "conversion_workers", None
                        ),
                        expanded_archives=expanded_archives,
                        archive_output_dir=archive_output_dir,
                        fo4_ba2_target=fo4_ba2_target,
                        archive_labels=archive_labels,
                        pack_progress=emit_pack_progress,
                    )
                )

            _run_post_phase("Pack BA2", pack_body, runner)
            runner.emit_log(
                "INFO", "join: wrote " + ", ".join(p.output_name for p in plans)
            )
        driver.emit_complete(runner)
        mirror.finish("done")
        return UnifiedRunResult(
            summary=driver.record_runtime._aggregate_summary,
            run_result=driver.record_runtime.run_result,
            planned_archives=plans,
        )
    except BaseException:
        try:
            native.sinks_abort(sink_id)
        except Exception:
            pass
        mirror.finish("failed")
        raise
    finally:
        if asset_runs is not None:
            asset_runs.drop_all()
        try:
            native.sinks_drop(sink_id)
        except Exception:
            pass


def collect_cache_entries(
    driver: "UnifiedDriver", toggles: "AssetWaveToggles"
) -> list[CacheAssetEntry]:
    """Manifest rows for the entry-list phases. v1 fidelity (declared):
    BTO + sound entries record their planned outputs (cheap, exact path
    mapping); NIF/havok rows are recorded with empty outputs (consult_cache
    never skips them — conservative); convert-all phases (textures,
    materials) enumerate in Rust and have no per-entry rows."""
    if driver.ctx is None:
        return []
    entries: list[CacheAssetEntry] = []
    digest = params_digest({"overwrite": True})
    for asset in list(driver.ctx.assets):
        a_type = str(getattr(asset, "asset_type", "")).lower()
        resolved = getattr(asset, "resolved_path", None)
        if not resolved:
            continue
        source = str(getattr(asset, "source_path", "")).replace("\\", "/")
        if a_type == "bto" and toggles.btos:
            rel = source if source.lower().startswith("meshes/") else f"Meshes/{source}"
            entries.append(CacheAssetEntry(str(resolved), "btos", digest, (rel,)))
        elif a_type in {"sound", "audio"} and toggles.sounds:
            rel = source if source.lower().startswith("sound/") else f"Sound/{source}"
            entries.append(CacheAssetEntry(str(resolved), "sounds", digest, (rel,)))
        elif a_type == "nif" and toggles.nifs:
            entries.append(CacheAssetEntry(str(resolved), "nifs", digest, ()))
        elif a_type == "behavior" and toggles.havok:
            entries.append(CacheAssetEntry(str(resolved), "havok", digest, ()))
    return entries
