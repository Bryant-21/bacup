"""Conversion pipeline orchestrator."""

from __future__ import annotations

import dataclasses
import glob as glob_mod
import json
import logging
import os
import re
import shutil
import subprocess
import tempfile
import threading
import time
from pathlib import Path

from bacup_lib.behavior import (
    phase_scaffold as _phase_scaffold_python,
)
from bacup_lib.asset_paths import normalize_asset_source_path
from bacup_lib.base_asset_dedupe import (
    asset_owner_signature,
    resolve_base_asset_namespace,
    resolve_base_asset_relocation_mesh_roots,
)
from bacup_lib.models import (
    AssetRef,
    ConversionDecision,
    ConversionDecisionKind,
    ConversionSummary,
    DependencyGraph,
    PhaseProgress,
    RecordNode,
)
from bacup_lib.native_maps import native_translation_maps_dir
from bacup_lib.record import phase_resolve
from bacup_lib.record.canonical_keys import (
    TARGET_RECORD_TYPE_FIELD,
    _interleave_body_part_fields,
    find_unknown_fields,
)
from bacup_lib.record.schema_surface import (
    SchemaRecordError,
    get_schema_surface,
)
from bacup_lib.omod_property_codec import (
    VT_BOOL,
    VT_ENUM,
    VT_FLOAT,
    VT_INT,
    encode_property,
    property_name_to_id,
)
from bacup_lib.runner import ConversionRunner
from bacup_lib.yaml_helpers import (
    field_all,
    field_at,
    from_ref,
    remove_field,
    set_field,
    to_ref,
)
from creation_lib.esp.record_types import record_type_signature

_log = logging.getLogger("conversion.orchestrator")

_ASSET_PHASE_GATES = {
    "Convert Terrain BTOs": "convert_btos",
    "Convert NIFs": "convert_nifs",
    "Convert Textures": "convert_textures",
    "Convert Materials": "convert_materials",
    "Convert Havok": "convert_havok",
    "Postprocess Havok Assets": "convert_havok",
}
_ASSET_PHASE_DEFAULTS = {
    "Convert Terrain BTOs": False,
}


def _gated_asset_phases(orchestrator, phases):
    """Drop asset phases whose per-asset gate attr is explicitly False.
    Gates default True via getattr, so callers that never set them
    (whole-plugin graph/bounded runs) keep every phase."""
    return [
        (num, name, fn)
        for (num, name, fn) in phases
        if getattr(
            orchestrator,
            _ASSET_PHASE_GATES.get(name, ""),
            _ASSET_PHASE_DEFAULTS.get(name, True),
        )
    ]


def _append_unique_strings(target: list[str], values: list[str]) -> None:
    seen = set(target)
    for value in values:
        if value in seen:
            continue
        target.append(value)
        seen.add(value)


def _is_precombined_nif_asset(asset) -> bool:
    if getattr(asset, "asset_type", "") != "nif":
        return False
    for value in (
        getattr(asset, "source_path", ""),
        getattr(asset, "resolved_path", ""),
    ):
        normalized = normalize_asset_source_path(str(value or "")).lower().lstrip("/")
        if normalized == "meshes/precombined" or normalized.startswith(
            "meshes/precombined/"
        ):
            return True
        if normalized == "precombined" or normalized.startswith("precombined/"):
            return True
    return False


def apply_material_overrides(
    bgsm_paths,
    overrides,
) -> None:
    """Apply field overrides to BGSM binary files (post-write patch)."""
    if not overrides:
        return
    from creation_lib.material_tools.mass_edit_materials import (
        apply_field_override,
        detect_binary_type,
    )

    for bgsm_path in bgsm_paths:
        path = Path(bgsm_path)
        if not path.is_file() or detect_binary_type(str(path)) != "BGSM":
            continue
        for field, value in overrides.items():
            apply_field_override(path, str(field), value)


_WINDOWS_ABSOLUTE_PATH_RE = re.compile(r"^[A-Za-z]:[\\/]")


def _normalize_material_source_override(value) -> str:
    path = str(value or "").strip().replace("\\", "/")
    if (
        not path
        or os.path.isabs(path)
        or _WINDOWS_ABSOLUTE_PATH_RE.match(path)
        or path.startswith("//")
    ):
        return ""
    parts = [part for part in path.split("/") if part and part != "."]
    if not parts or any(part == ".." for part in parts):
        return ""
    if parts[0].lower() == "data":
        parts = parts[1:]
    return "/".join(parts)


def _source_root_from_resolved_asset(source_path: str, resolved_path: str) -> Path | None:
    if not source_path or not resolved_path:
        return None
    resolved = str(resolved_path).replace("\\", "/")
    source = str(source_path).replace("\\", "/").lstrip("/")
    idx = resolved.lower().find(source.lower())
    if idx <= 0:
        return None
    root = resolved[:idx].rstrip("/")
    return Path(root) if root else None


def _resolve_material_source_override(
    source_path: str,
    resolved_path: str,
    override_value: str,
) -> str:
    relative_path = _normalize_material_source_override(override_value)
    if not relative_path:
        return ""
    root = _source_root_from_resolved_asset(source_path, resolved_path)
    if root is None:
        return ""
    parts = relative_path.split("/")
    for candidate in (root.joinpath(*parts), root.joinpath("Data", *parts)):
        if candidate.is_file():
            return str(candidate)
    return ""


def _is_havok_or_collision_warning(message: str) -> bool:
    lowered = message.lower()
    return any(token in lowered for token in ("havok", "bhk", "hknp", "collision"))


def _drain_native_phase_events(
    rust_run,
    runner: ConversionRunner,
    phase_name: str,
) -> None:
    drain_events = getattr(rust_run, "drain_events", None)
    if not callable(drain_events):
        return

    while True:
        events = drain_events(256)
        if not events:
            return
        for event in events:
            if event.get("phase") != phase_name:
                continue
            kind = event.get("kind")
            if kind == "log":
                runner.emit_log(
                    event.get("level", "INFO"),
                    event.get("message", ""),
                )
            elif kind == "progress":
                phase = event.get("phase", "")
                current = event.get("current", 0)
                total = event.get("total", 0)
                item = event.get("item", "")
                runner.emit_log("INFO", f"[{phase}] {current}/{total} {item}".rstrip())


def _run_phase_with_live_drain(
    rust_run,
    runner: ConversionRunner,
    phase_name: str,
    *,
    drain_interval: float = 0.25,
    **run_phase_kwargs,
) -> dict:
    """Run ``rust_run.run_phase`` and drain events every *drain_interval* seconds
    via a background thread so progress reaches the caller while the Rust phase
    is still executing.

    Returns the phase report dict (same as ``rust_run.run_phase`` return value).
    """
    result: dict = {}
    exc_holder: list[BaseException] = []

    def _worker() -> None:
        try:
            result.update(rust_run.run_phase(phase_name, **run_phase_kwargs) or {})
        except BaseException as e:  # noqa: BLE001
            exc_holder.append(e)

    t = threading.Thread(target=_worker, daemon=True)
    t.start()
    while t.is_alive():
        time.sleep(drain_interval)
        _drain_native_phase_events(rust_run, runner, phase_name)
    t.join()
    # Final drain after the thread exits.
    _drain_native_phase_events(rust_run, runner, phase_name)
    if exc_holder:
        raise exc_holder[0]
    return result


def _params_for_convert_nifs(orchestrator, nif_assets=None) -> dict:
    """Build the params dict for the native convert_nifs phase."""
    from bacup_lib.weapon_attachment_surgery import is_fnv_source

    if nif_assets is None:
        nif_assets = [a for a in orchestrator.graph.all_assets if a.asset_type == "nif"]
    source_profile = getattr(orchestrator, "_source_profile", None)
    asset_prefix = getattr(source_profile, "asset_prefix", "") or ""

    is_fnv = is_fnv_source(source_profile)

    nif_entries = []
    for asset in nif_assets:
        entry: dict = {
            "source_path": asset.source_path,
            "resolved_path": asset.resolved_path or "",
        }
        output_subpath = getattr(asset, "output_subpath", None) or getattr(
            orchestrator, "_forced_asset_output_subpath", lambda _asset: None
        )(asset)
        material_namespace = _nif_material_namespace(orchestrator, asset)
        if output_subpath:
            entry["output_subpath"] = str(output_subpath).replace("\\", "/")
            entry["asset_namespace"] = getattr(orchestrator, "base_asset_namespace", "")
            if not material_namespace:
                material_namespace = getattr(orchestrator, "base_asset_namespace", "")
        if material_namespace:
            entry["material_namespace"] = material_namespace
        if is_fnv:
            role = _weapon_role_for_asset(orchestrator, asset.source_path)
            if role is not None:
                entry["weapon_role"] = role
        nif_entries.append(entry)

    params: dict = {
        "source_game": orchestrator.source_game,
        "target_game": orchestrator.target_game,
        "asset_prefix": asset_prefix,
        "nif_paths": nif_entries,
        "skip_existing": not bool(getattr(orchestrator, "overwrite_existing", False)),
        "addon_index_map": {
            str(k): v for k, v in (orchestrator._addon_index_map or {}).items()
        },
    }
    conversion_workers = getattr(orchestrator, "conversion_workers", None)
    if conversion_workers is not None:
        params["conversion_workers"] = int(conversion_workers)
    if bool(getattr(orchestrator, "disable_nif_collision_memo", False)):
        params["disable_collision_memo"] = True
    params.update(resolve_nif_conversion_options(orchestrator))
    return params


def _nif_material_namespace(orchestrator, asset) -> str:
    namespace = _fo76_to_fo4_asset_namespace(orchestrator)
    if not namespace:
        return ""
    if getattr(orchestrator, "_is_forced_base_asset_conversion", lambda _asset: False)(
        asset
    ):
        return namespace

    provenance = getattr(asset, "provenance", None)
    owner_fk = str(getattr(provenance, "added_by_record_fk", "") or "")
    if not owner_fk:
        return ""
    graph = getattr(orchestrator, "graph", None)
    for record in getattr(graph, "all_records", []) or []:
        if str(getattr(record, "form_key", "") or "") != owner_fk:
            continue
        for candidate in getattr(record, "assets", []) or []:
            if candidate is asset:
                continue
            if getattr(candidate, "asset_type", "") not in {"material", "texture"}:
                continue
            if _asset_has_namespaced_output(orchestrator, candidate, namespace):
                return namespace
        return ""
    return ""


def _fo76_to_fo4_asset_namespace(orchestrator) -> str:
    pair = (
        str(getattr(orchestrator, "source_game", "")).lower(),
        str(getattr(orchestrator, "target_game", "")).lower(),
    )
    if pair != ("fo76", "fo4"):
        return ""
    return str(getattr(orchestrator, "base_asset_namespace", "") or "").strip()


def _is_scol_collection_nif_asset(asset) -> bool:
    if getattr(asset, "asset_type", "") != "nif":
        return False
    parts = [
        part
        for part in normalize_asset_source_path(str(getattr(asset, "source_path", "") or ""))
        .replace("\\", "/")
        .strip("/")
        .split("/")
        if part
    ]
    if parts and parts[0].lower() == "meshes":
        parts = parts[1:]
    if len(parts) < 3 or parts[0].lower() != "scol":
        return False
    if Path(parts[1]).suffix.lower() not in {".esm", ".esp", ".esl"}:
        return False
    name = Path(parts[-1])
    return name.suffix.lower() == ".nif" and name.name.lower().startswith("cm")


def _is_scol_owned_asset(asset) -> bool:
    provenance = getattr(asset, "provenance", None)
    return str(getattr(provenance, "added_by_record_sig", "") or "").upper() == "SCOL"


def _scol_namespaced_asset_output_subpath(orchestrator, asset) -> str:
    namespace = _fo76_to_fo4_asset_namespace(orchestrator)
    if not namespace or not _asset_is_claimed_by_scol_record(orchestrator, asset):
        return ""
    roots = {
        "material": "Materials",
        "texture": "Textures",
    }
    root = roots.get(str(getattr(asset, "asset_type", "") or "").lower())
    if root is None:
        return ""
    subpath = _data_relative_asset_subpath(asset, root)
    if not subpath or not _scol_record_uses_asset_namespace(orchestrator, asset):
        return ""
    return _insert_namespace_after_data_root(subpath, namespace)


def _asset_is_claimed_by_scol_record(orchestrator, asset) -> bool:
    return any(True for _record in _scol_records_claiming_asset(orchestrator, asset))


def _scol_records_claiming_asset(orchestrator, asset):
    provenance = getattr(asset, "provenance", None)
    owner_fk = str(getattr(provenance, "added_by_record_fk", "") or "")
    owner_sig = str(getattr(provenance, "added_by_record_sig", "") or "").upper()
    graph = getattr(orchestrator, "graph", None)
    if graph is None:
        if owner_sig == "SCOL":
            yield None
        return
    for record in getattr(graph, "all_records", []) or []:
        if str(getattr(record, "record_type", "") or "").upper() != "SCOL":
            continue
        if asset in (getattr(record, "assets", []) or []):
            yield record
            continue
        if owner_sig == "SCOL" and str(getattr(record, "form_key", "") or "") == owner_fk:
            yield record


def _scol_record_uses_asset_namespace(orchestrator, asset) -> bool:
    records = list(_scol_records_claiming_asset(orchestrator, asset))
    for record in records:
        if record is None:
            continue
        for candidate in getattr(record, "assets", []) or []:
            if getattr(candidate, "asset_type", "") not in {"material", "texture"}:
                continue
            root = (
                "Materials"
                if getattr(candidate, "asset_type", "") == "material"
                else "Textures"
            )
            subpath = _data_relative_asset_subpath(candidate, root)
            if subpath and _target_has_data_relative_asset(orchestrator, subpath):
                return True
    if records:
        return False
    root = "Materials" if getattr(asset, "asset_type", "") == "material" else "Textures"
    subpath = _data_relative_asset_subpath(asset, root)
    return bool(subpath and _target_has_data_relative_asset(orchestrator, subpath))


def _data_relative_asset_subpath(asset, root: str) -> str:
    subpath = normalize_asset_source_path(str(getattr(asset, "source_path", "") or ""))
    subpath = subpath.replace("\\", "/").strip("/")
    lower = subpath.lower()
    if not lower.startswith(root.lower() + "/"):
        return f"{root}/{subpath}"
    return f"{root}/{subpath.split('/', 1)[1]}"


def _target_has_data_relative_asset(orchestrator, subpath: str) -> bool:
    target_asset_store = getattr(orchestrator, "target_asset_store", None)
    if target_asset_store is not None and target_asset_store.has_asset(subpath):
        return True
    rel = Path(*[part for part in subpath.replace("\\", "/").split("/") if part])
    for attr in ("target_extracted_dir", "target_data_dir"):
        value = getattr(orchestrator, attr, None)
        if not value:
            continue
        root = Path(value)
        if (root / rel).is_file() or (root / "Data" / rel).is_file():
            return True
    return False


def _insert_namespace_after_data_root(subpath: str, namespace: str) -> str:
    parts = [part for part in subpath.replace("\\", "/").split("/") if part]
    if not parts:
        return ""
    if len(parts) >= 2 and parts[1].lower() == namespace.lower():
        parts[1] = namespace
        return "/".join(parts)
    return "/".join([parts[0], namespace, *parts[1:]])


def _asset_has_namespaced_output(orchestrator, asset, namespace: str) -> bool:
    output_subpath = getattr(asset, "output_subpath", None) or getattr(
        orchestrator, "_forced_asset_output_subpath", lambda _asset: None
    )(asset) or _scol_namespaced_asset_output_subpath(orchestrator, asset)
    if not output_subpath:
        return False
    parts = [
        part.lower()
        for part in str(output_subpath).replace("\\", "/").split("/")
        if part
    ]
    return len(parts) >= 2 and parts[1] == namespace.lower()


def _iter_bto_source_roots(orchestrator) -> list[Path]:
    roots: list[Path] = []
    seen: set[str] = set()
    for attr in ("source_data_dir", "source_extracted_dir", "extracted_dir"):
        value = getattr(orchestrator, attr, None)
        if not value:
            continue
        root = Path(value)
        for candidate in (root, root / "Data"):
            if not candidate.is_dir():
                continue
            key = os.path.normcase(str(candidate.resolve()))
            if key in seen:
                continue
            seen.add(key)
            roots.append(candidate)
    return roots


def _normalize_bto_source_path(path: Path, root: Path) -> str:
    rel_path = path.relative_to(root).as_posix()
    parts = rel_path.split("/")
    if parts and parts[0].lower() == "data":
        parts = parts[1:]
    for index, part in enumerate(parts):
        if part.lower() == "meshes":
            return "/".join(parts[index:])
    return "/".join(parts)


def _case_insensitive_child_dir(root: Path, name: str) -> Path | None:
    direct = root / name
    if direct.is_dir():
        return direct
    try:
        for child in root.iterdir():
            if child.is_dir() and child.name.casefold() == name.casefold():
                return child
    except OSError:
        return None
    return None


def _iter_bto_search_roots(root: Path) -> list[Path]:
    meshes_root = _case_insensitive_child_dir(root, "Meshes")
    if meshes_root is None:
        return []
    terrain_root = _case_insensitive_child_dir(meshes_root, "Terrain")
    if terrain_root is None:
        return []
    return [terrain_root]


def discover_terrain_bto_assets(orchestrator) -> list[AssetRef]:
    """Discover terrain/LOD BTO files from source roots."""
    assets: list[tuple[int, str, AssetRef]] = []
    seen_sources: set[str] = set()
    seen_resolved: set[str] = set()
    for root in _iter_bto_source_roots(orchestrator):
        for search_root in _iter_bto_search_roots(root):
            for path in search_root.rglob("*"):
                if not path.is_file() or path.suffix.lower() != ".bto":
                    continue
                rel_parts = path.relative_to(root).parts
                data_rank = 1 if rel_parts and rel_parts[0].lower() == "data" else 0
                source_path = _normalize_bto_source_path(path, root)
                source_key = source_path.casefold()
                resolved_key = os.path.normcase(str(path.resolve()))
                if source_key in seen_sources or resolved_key in seen_resolved:
                    continue
                seen_sources.add(source_key)
                seen_resolved.add(resolved_key)
                assets.append(
                    (
                        data_rank,
                        source_path.lower(),
                        AssetRef(
                            asset_type="bto",
                            source_path=source_path,
                            resolved_path=str(path),
                        ),
                    )
                )
    return [
        asset
        for _data_rank, _source_path, asset in sorted(
            assets, key=lambda item: (item[0], item[1])
        )
    ]


def _params_for_convert_btos(orchestrator, bto_assets=None) -> dict:
    """Build the params dict for the native convert_btos phase."""
    if bto_assets is None:
        bto_assets = discover_terrain_bto_assets(orchestrator)
    params: dict = {
        "source_game": orchestrator.source_game,
        "target_game": orchestrator.target_game,
        "bto_paths": [
            {
                "source_path": asset.source_path,
                "resolved_path": asset.resolved_path or "",
            }
            for asset in bto_assets
        ],
        "skip_existing": not bool(getattr(orchestrator, "overwrite_existing", False)),
    }
    conversion_workers = getattr(orchestrator, "conversion_workers", None)
    if conversion_workers is not None:
        params["conversion_workers"] = int(conversion_workers)
    if bool(getattr(orchestrator, "disable_nif_collision_memo", False)):
        params["disable_collision_memo"] = True
    return params


def phase_convert_btos_native(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    _phase_convert_btos_native_impl(
        orchestrator, runner, progress, phase_name="convert_btos", single_call=False
    )


def phase_convert_btos_native_v2(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    """One run_phase call with the full BTO work list, dispatching the
    memoized convert_btos_v2 native phase."""
    _phase_convert_btos_native_impl(
        orchestrator, runner, progress, phase_name="convert_btos_v2", single_call=True
    )


def _phase_convert_btos_native_impl(
    orchestrator,
    runner: ConversionRunner,
    progress: PhaseProgress,
    *,
    phase_name: str,
    single_call: bool,
) -> None:
    """Convert terrain/LOD BTO meshes via the native convert_btos phase."""
    discovery_started = time.perf_counter()
    bto_assets = discover_terrain_bto_assets(orchestrator)
    discovery_elapsed_ms = int((time.perf_counter() - discovery_started) * 1000)
    total_btos = len(bto_assets)
    orchestrator._summary.btos_total = total_btos
    progress.total_items = total_btos
    progress.completed_items = 0
    runner.emit_item_progress(progress)
    runner.emit_log(
        "INFO",
        f"[BTO] total discovered={total_btos} discovery_elapsed_ms={discovery_elapsed_ms}",
    )

    if not bto_assets:
        runner.emit_log("INFO", "[BTO] no terrain BTO files discovered")
        return

    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is None:
        orchestrator._summary.btos_failed += total_btos
        progress.completed_items = total_btos
        progress.current_item = bto_assets[-1].source_path
        runner.emit_item_progress(progress)
        runner.emit_log("WARN", "convert_btos requires a native ConversionRun")
        return

    if single_call:
        batch_size = max(1, total_btos)
    else:
        batch_size = max(
            1,
            int(getattr(orchestrator, "bto_native_batch_size", _BTO_NATIVE_BATCH_SIZE)),
        )
    batch_count = (total_btos + batch_size - 1) // batch_size
    assets_written = 0
    warnings = 0
    for batch_index, start in enumerate(range(0, total_btos, batch_size), start=1):
        if runner.is_cancelled():
            break
        batch = bto_assets[start : start + batch_size]
        params = _params_for_convert_btos(orchestrator, batch)
        if single_call:
            report = _run_phase_with_live_drain(
                rust_run,
                runner,
                phase_name,
                mod_path=str(orchestrator.mod_path),
                source_extracted_dir="",
                params=params,
            )
        else:
            report = rust_run.run_phase(
                phase_name,
                mod_path=str(orchestrator.mod_path),
                source_extracted_dir="",
                params=params,
            )
            _drain_native_phase_events(rust_run, runner, phase_name)

        batch_written = int(report.get("assets_written", 0))
        batch_warnings = int(report.get("warnings", 0))
        assets_written += batch_written
        warnings += batch_warnings
        orchestrator._summary.btos_converted += batch_written
        orchestrator._summary.btos_failed += batch_warnings

        completed = min(start + len(batch), total_btos)
        progress.completed_items = completed
        progress.current_item = batch[-1].source_path if batch else ""
        runner.emit_item_progress(progress)
        runner.emit_log(
            "INFO",
            f"[BTO] batch {batch_index}/{batch_count}: "
            f"queued={completed}/{total_btos}, "
            f"written_or_existing={assets_written}, "
            f"failed={warnings}, "
            f"elapsed_ms={report.get('elapsed_ms', 0)}",
        )


def phase_convert_nifs_native(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    _phase_convert_nifs_native_impl(
        orchestrator, runner, progress, phase_name="convert_nifs_v2", single_call=True
    )


def phase_convert_nifs_native_v2(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    """Compatibility wrapper for callers that already named the v2 helper."""
    phase_convert_nifs_native(orchestrator, runner, progress)


def _phase_convert_nifs_native_impl(
    orchestrator,
    runner: ConversionRunner,
    progress: PhaseProgress,
    *,
    phase_name: str,
    single_call: bool,
) -> None:
    """Phase 3: Convert NIF meshes via the native convert_nifs_v2 phase.

    Uses the Rust ConversionRun dispatcher. Python does not run a fallback
    NIF conversion loop when a Rust run is unavailable.
    """
    _finalize_weapon_surgery = finalize_weapon_surgery

    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    nif_assets = [a for a in orchestrator.graph.all_assets if a.asset_type == "nif"]
    if not getattr(orchestrator, "convert_precombined_nifs", True):
        kept_assets = []
        precombined_count = 0
        for asset in nif_assets:
            if _is_precombined_nif_asset(asset):
                precombined_count += 1
            else:
                kept_assets.append(asset)
        if precombined_count:
            nif_assets = kept_assets
            runner.emit_log(
                "INFO",
                f"[NIF] skipped {precombined_count} precombined NIFs",
            )
    nif_count = len(nif_assets)
    orchestrator._summary.nifs_total = nif_count
    progress.total_items = nif_count
    runner.emit_item_progress(progress)

    if rust_run is None:
        runner.emit_log("WARN", "convert_nifs: no Rust run; skipping")
        return

    convert_assets = []
    base_game_skipped = 0
    stale_outputs_removed = 0
    for asset in nif_assets:
        if orchestrator._target_has_asset(asset):
            base_game_skipped += 1
            if orchestrator._remove_stale_asset_output(asset):
                stale_outputs_removed += 1
            orchestrator._track_asset(asset, "base_game_skip", "exists in target game")
        else:
            if not asset.resolved_path:
                orchestrator._summary.nifs_failed += 1
                message = asset.resolution_error or "source path did not resolve"
                runner.emit_log(
                    "WARN", f"NIF not found: {asset.source_path}: {message}"
                )
                orchestrator._log_lines.append(
                    f"[WARN] NIF not found: {asset.source_path}: {message}"
                )
                continue
            convert_assets.append(asset)
    if base_game_skipped:
        orchestrator._summary.nifs_base_game_skipped += base_game_skipped
        runner.emit_log(
            "INFO",
            f"[NIF] skipped {base_game_skipped} NIFs already present in target game",
        )
    if stale_outputs_removed:
        runner.emit_log(
            "INFO",
            f"[NIF] removed {stale_outputs_removed} stale NIF output(s) for target-game skips",
        )
    total_to_convert = len(convert_assets)
    if single_call:
        batch_size = max(1, total_to_convert)
    else:
        batch_size = max(
            1,
            int(getattr(orchestrator, "nif_native_batch_size", _NIF_NATIVE_BATCH_SIZE)),
        )
    workers = getattr(orchestrator, "conversion_workers", None)
    worker_label = str(workers) if workers is not None else "rayon-default"
    runner.emit_log(
        "INFO",
        f"[NIF] total referenced={nif_count}, queued={total_to_convert}, "
        f"target-game skipped={base_game_skipped}, workers={worker_label}, "
        f"batch_size={batch_size}",
    )

    assets_written = 0
    warnings = 0
    if total_to_convert:
        batch_count = (total_to_convert + batch_size - 1) // batch_size
        for batch_index, start in enumerate(
            range(0, total_to_convert, batch_size), start=1
        ):
            if runner.is_cancelled():
                break
            batch = convert_assets[start : start + batch_size]
            params = _params_for_convert_nifs(orchestrator, batch)
            if single_call:
                # One call for the whole work list: drain concurrently so the
                # bounded native event channel cannot overflow and drop the
                # trailing timings/memo-stats log events.
                report = _run_phase_with_live_drain(
                    rust_run,
                    runner,
                    phase_name,
                    mod_path=str(orchestrator.mod_path),
                    source_extracted_dir=str(
                        getattr(orchestrator, "target_extracted_dir", "") or ""
                    ),
                    params=params,
                )
            else:
                report = rust_run.run_phase(
                    phase_name,
                    mod_path=str(orchestrator.mod_path),
                    source_extracted_dir=str(
                        getattr(orchestrator, "target_extracted_dir", "") or ""
                    ),
                    params=params,
                )
                _drain_native_phase_events(rust_run, runner, phase_name)

            batch_written = int(report.get("assets_written", 0))
            batch_warnings = int(report.get("warnings", 0))
            assets_written += batch_written
            warnings += batch_warnings
            orchestrator._summary.nifs_converted += batch_written
            orchestrator._summary.nifs_failed += batch_warnings

            completed = min(start + len(batch), total_to_convert)
            progress.completed_items = base_game_skipped + completed
            progress.current_item = batch[-1].source_path if batch else ""
            runner.emit_item_progress(progress)
            runner.emit_log(
                "INFO",
                f"[NIF] batch {batch_index}/{batch_count}: "
                f"queued={completed}/{total_to_convert}, "
                f"written_or_existing={assets_written}, "
                f"failed={warnings}, "
                f"elapsed_ms={report.get('elapsed_ms', 0)}",
            )
    else:
        progress.completed_items = base_game_skipped
        runner.emit_item_progress(progress)
    runner.emit_log(
        "INFO",
        f"[NIF] native phase done: converted_or_existing={assets_written}, "
        f"failed={warnings}, target-game skipped={base_game_skipped}",
    )
    if not runner.is_cancelled():
        _finalize_weapon_surgery(orchestrator, runner)


def _params_for_convert_material_assets(orchestrator, mat_assets) -> dict:
    """Build the params dict for the native convert_materials phase."""
    source_profile = getattr(orchestrator, "_source_profile", None)
    asset_prefix = getattr(source_profile, "asset_prefix", "") or ""
    material_overrides = getattr(orchestrator, "_material_overrides", {}) or {}
    bgsm_default_overrides = material_overrides.get("bgsm_default") or {}

    # Resolve source MaterialsDB path from any resolved asset — mirrors _get_materials_cdb heuristic.
    source_materialsdb = ""
    for a in mat_assets:
        if not a.resolved_path:
            continue
        rp = a.resolved_path.replace("\\", "/")
        sp = a.source_path.replace("\\", "/")
        idx = rp.lower().find(sp.lower())
        if idx > 0:
            root = rp[:idx].rstrip("/")
            import os

            for candidate in [
                os.path.join(
                    root, "Data", "SeventySix - Materials.ba2", "MaterialsDB.cdb"
                ),
                os.path.join(root, "SeventySix - Materials.ba2", "MaterialsDB.cdb"),
                os.path.join(root, "Data", "MaterialsDB.cdb"),
                os.path.join(root, "MaterialsDB.cdb"),
            ]:
                if os.path.isfile(candidate):
                    source_materialsdb = candidate
                    break
        if source_materialsdb:
            break

    load_source_overrides = getattr(
        orchestrator, "_load_material_source_overrides", None
    )
    source_overrides = (
        load_source_overrides() if callable(load_source_overrides) else {}
    )
    entries = []
    for a in mat_assets:
        resolved = a.resolved_path or ""
        if source_overrides:
            sp_key = a.source_path.lower().replace("\\", "/")
            override_path = _resolve_material_source_override(
                a.source_path,
                resolved,
                source_overrides.get(sp_key, ""),
            )
            if override_path:
                resolved = override_path
        entry = {
            "source_path": a.source_path,
            "resolved_path": resolved,
            "is_cdb_ref": bool(getattr(a, "is_cdb_ref", False)),
        }
        output_subpath = getattr(a, "output_subpath", None) or getattr(
            orchestrator, "_forced_asset_output_subpath", lambda _asset: None
        )(a) or _scol_namespaced_asset_output_subpath(orchestrator, a)
        if output_subpath:
            entry["output_subpath"] = str(output_subpath).replace("\\", "/")
            entry["texture_namespace"] = getattr(
                orchestrator, "base_asset_namespace", ""
            )
        entries.append(entry)
    return {
        "materials": entries,
        "source_game": orchestrator.source_game,
        "target_game": orchestrator.target_game,
        "asset_prefix": asset_prefix,
        "source_materialsdb": source_materialsdb,
        "overwrite_existing": bool(orchestrator.overwrite_existing),
        "pbr_carry": bool(getattr(orchestrator, "pbr_carry", False)),
        "bgsm_default_overrides": bgsm_default_overrides,
    }


def _params_for_convert_materials(orchestrator) -> dict:
    """Build the params dict for the native convert_materials phase."""
    mat_assets = [
        a for a in orchestrator.graph.all_assets if a.asset_type == "material"
    ]
    return _params_for_convert_material_assets(orchestrator, mat_assets)


def _is_bgsm_or_bgem_asset(asset: AssetRef) -> bool:
    return Path(str(asset.source_path)).suffix.lower() in {".bgsm", ".bgem"}


_RAW_MATERIAL_TEXTURE_FIELDS: tuple[str, ...] = (
    "DiffuseTexture",
    "NormalTexture",
    "SmoothSpecTexture",
    "GreyscaleTexture",
    "GlowTexture",
    "WrinklesTexture",
    "EnvmapTexture",
    "InnerLayerTexture",
    "DisplacementTexture",
    "SpecularTexture",
    "LightingTexture",
    "FlowTexture",
    "DistanceFieldAlphaTexture",
    "BaseTexture",
    "GrayscaleTexture",
    "EnvmapMaskTexture",
    "GlassRoughnessScratch",
    "GlassDirtOverlay",
)


def _read_raw_material_texture_refs(material_path: str) -> list[tuple[str, str]]:
    suffix = Path(material_path).suffix.lower()
    try:
        if suffix == ".bgsm":
            from creation_lib.material_tools.bgsm_bin import read_bgsm

            with open(material_path, "rb") as handle:
                material = read_bgsm(handle)
        elif suffix == ".bgem":
            from creation_lib.material_tools.bgem_bin import read_bgem

            with open(material_path, "rb") as handle:
                material = read_bgem(handle)
        else:
            return []
    except Exception:
        return []

    refs: list[tuple[str, str]] = []
    for field_name in _RAW_MATERIAL_TEXTURE_FIELDS:
        value = getattr(material, field_name, None)
        if not isinstance(value, str):
            continue
        cleaned = value.replace("\x00", "").strip()
        if cleaned:
            refs.append((field_name, cleaned))
    return refs


def _texture_source_path_from_material_ref(texture_ref: str) -> str:
    from bacup_lib.asset_paths import normalize_asset_source_path

    rel_path = normalize_asset_source_path(texture_ref)
    if rel_path and not rel_path.lower().startswith("textures/"):
        rel_path = f"Textures/{rel_path}"
    return rel_path


def _texture_asset_key(source_path: str) -> str:
    return _texture_source_path_from_material_ref(source_path).lower()


def _resolve_material_texture_ref(
    source_root: Path,
    texture_ref: str,
) -> AssetRef | None:
    source_path = _texture_source_path_from_material_ref(texture_ref)
    if not source_path:
        return None

    for root in (source_root, source_root / "Data"):
        candidate = root.joinpath(*source_path.split("/"))
        if candidate.is_file():
            return AssetRef(
                asset_type="texture",
                source_path=source_path,
                resolved_path=str(candidate),
            )

    return AssetRef(
        asset_type="texture",
        source_path=source_path,
        resolved_path=None,
        resolution_error="Texture referenced by source material was not found",
    )


def _source_material_root(orchestrator) -> Path | None:
    for attr in ("source_data_dir", "source_extracted_dir", "extracted_dir"):
        value = getattr(orchestrator, attr, None)
        if not value:
            continue
        root = Path(value)
        if (root / "materials").is_dir():
            return root
        if root.name.lower() == "materials" and root.is_dir():
            return root.parent
    return None


def _iter_raw_bgsm_assets(orchestrator) -> list[AssetRef]:
    root = _source_material_root(orchestrator)
    if root is None:
        return []

    material_dir = root / "materials"
    graph_materials = _graph_material_assets_by_key(orchestrator)
    assets: list[AssetRef] = []
    for path in material_dir.rglob("*"):
        if not path.is_file() or path.suffix.lower() not in {".bgsm", ".bgem"}:
            continue
        rel_path = path.relative_to(root).as_posix()
        asset = AssetRef(
            asset_type="material",
            source_path=rel_path,
            resolved_path=str(path),
        )
        graph_asset = graph_materials.get(_material_asset_key(rel_path))
        if graph_asset is not None:
            asset.source_path = str(getattr(graph_asset, "source_path", "") or rel_path)
            asset.provenance = getattr(graph_asset, "provenance", None)
            asset.force_convert = bool(getattr(graph_asset, "force_convert", False))
            asset.force_reason = str(getattr(graph_asset, "force_reason", "") or "")
            asset.output_subpath = getattr(graph_asset, "output_subpath", None)
        assets.append(asset)
    assets.sort(key=lambda asset: asset.source_path.lower())
    return assets


def _material_asset_key(source_path: str) -> str:
    rel_path = normalize_asset_source_path(str(source_path or "")).replace("\\", "/")
    lower = rel_path.lower().lstrip("/")
    if lower.startswith("materials/"):
        lower = lower[len("materials/") :]
    return lower


def _graph_material_assets_by_key(orchestrator) -> dict[str, AssetRef]:
    graph = getattr(orchestrator, "graph", None)
    graph_assets = getattr(graph, "all_assets", []) or []
    selected: dict[str, AssetRef] = {}
    for asset in graph_assets:
        if getattr(asset, "asset_type", "") != "material":
            continue
        key = _material_asset_key(getattr(asset, "source_path", ""))
        if not key:
            continue
        existing = selected.get(key)
        if existing is None or _asset_force_priority(asset) > _asset_force_priority(
            existing
        ):
            selected[key] = asset
    return selected


def _asset_force_priority(asset: AssetRef) -> int:
    return 1 if asset_owner_signature(asset) else 0


def _write_raw_material_unresolved_texture_log(
    orchestrator,
    unresolved_entries: list[dict[str, str]],
) -> Path | None:
    log_path = (
        Path(str(getattr(orchestrator, "diagnostics_root", orchestrator.mod_path)))
        / "debug"
        / "raw_material_unresolved_textures.json"
    )
    if not unresolved_entries:
        try:
            log_path.unlink()
        except FileNotFoundError:
            pass
        return None

    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(
        json.dumps(unresolved_entries, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return log_path


def _augment_raw_material_texture_assets(
    orchestrator,
    runner: ConversionRunner,
) -> int:
    if getattr(orchestrator, "_raw_material_texture_assets_augmented", False):
        return 0
    source_game = str(getattr(orchestrator, "source_game", "")).lower()
    target_game = str(getattr(orchestrator, "target_game", "")).lower()
    if (source_game, target_game) != ("fo76", "fo4"):
        return 0

    root = _source_material_root(orchestrator)
    if root is None:
        return 0

    material_assets = _iter_raw_bgsm_assets(orchestrator)
    if not material_assets:
        return 0

    seen = {
        _texture_asset_key(asset.source_path): asset
        for asset in orchestrator.graph.all_assets
        if asset.asset_type == "texture"
    }
    added = 0
    unresolved = 0
    unresolved_entries: list[dict[str, str]] = []
    for material_asset in material_assets:
        if not material_asset.resolved_path:
            continue
        for field_name, texture_ref in _read_raw_material_texture_refs(
            material_asset.resolved_path
        ):
            texture_asset = _resolve_material_texture_ref(root, texture_ref)
            if texture_asset is None:
                continue
            texture_asset.provenance = getattr(material_asset, "provenance", None)
            key = _texture_asset_key(texture_asset.source_path)
            existing_asset = seen.get(key)
            if existing_asset is not None:
                continue
            seen[key] = texture_asset
            orchestrator.graph.all_assets.append(texture_asset)
            added += 1
            if not texture_asset.resolved_path:
                unresolved += 1
                unresolved_entries.append(
                    {
                        "material": material_asset.source_path,
                        "field": field_name,
                        "texture_ref": texture_ref,
                        "source_path": texture_asset.source_path,
                        "reason": texture_asset.resolution_error or "",
                    }
                )

    orchestrator._raw_material_texture_assets_augmented = True
    unresolved_log_path = _write_raw_material_unresolved_texture_log(
        orchestrator,
        unresolved_entries,
    )
    if added:
        unresolved_log_suffix = (
            f"; unresolved_log={unresolved_log_path}" if unresolved_log_path else ""
        )
        runner.emit_log(
            "INFO",
            f"[Texture] raw material texture refs: +{added} texture files "
            f"from {len(material_assets)} BGSM/BGEM files; unresolved={unresolved}"
            f"{unresolved_log_suffix}",
        )
    return added


def _raw_material_output_path(orchestrator, asset: AssetRef) -> Path:
    output_subpath = getattr(orchestrator, "_asset_output_subpath", None)
    if callable(output_subpath):
        subpath = output_subpath(asset)
    else:
        subpath = asset.source_path.replace("\\", "/")
        if not subpath.lower().startswith("materials/"):
            subpath = "materials/" + subpath
    return Path(str(orchestrator.mod_path)) / "data" / Path(*subpath.split("/"))


def _remove_stale_raw_material_output(
    orchestrator,
    runner: ConversionRunner,
    asset: AssetRef,
) -> bool:
    output_path = _raw_material_output_path(orchestrator, asset)
    try:
        if output_path.is_file():
            output_path.unlink()
            return True
    except OSError as exc:
        runner.emit_log(
            "WARNING",
            f"[Texture] failed removing stale material collision {output_path}: {exc}",
        )
    return False


def _filter_raw_bgsm_assets_for_texture_phase(
    orchestrator,
    runner: ConversionRunner,
    material_assets: list[AssetRef],
) -> list[AssetRef]:
    target_has_asset = getattr(orchestrator, "_target_has_asset", None)
    if not callable(target_has_asset):
        return material_assets

    queued: list[AssetRef] = []
    skipped = 0
    removed = 0
    track_asset = getattr(orchestrator, "_track_asset", None)
    for asset in material_assets:
        try:
            target_exists = bool(target_has_asset(asset))
        except Exception as exc:
            runner.emit_log(
                "WARNING",
                f"[Texture] target material collision check failed for {asset.source_path}: {exc}",
            )
            target_exists = False
        if not target_exists:
            queued.append(asset)
            continue

        skipped += 1
        if callable(track_asset):
            track_asset(asset, "base_game_skip", "exists in target game")
        if _remove_stale_raw_material_output(orchestrator, runner, asset):
            removed += 1

    if skipped:
        orchestrator._summary.materials_base_game_skipped += skipped
        message = (
            f"[Texture] raw material pass skipped {skipped} BGSM/BGEM files "
            "already present in target game"
        )
        if removed:
            message += f"; removed {removed} stale generated file(s)"
        runner.emit_log("INFO", message)
    return queued


def _convert_raw_bgsm_materials_from_texture_phase(
    orchestrator,
    runner: ConversionRunner,
) -> None:
    if getattr(orchestrator, "_raw_material_files_converted", False):
        return
    source_game = str(getattr(orchestrator, "source_game", "")).lower()
    target_game = str(getattr(orchestrator, "target_game", "")).lower()
    if (source_game, target_game) != ("fo76", "fo4"):
        return

    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is None:
        return

    all_material_assets = _iter_raw_bgsm_assets(orchestrator)
    if not all_material_assets:
        runner.emit_log(
            "INFO", "[Texture] raw material pass skipped: no source BGSM/BGEM files"
        )
        return

    material_assets = _filter_raw_bgsm_assets_for_texture_phase(
        orchestrator,
        runner,
        all_material_assets,
    )
    orchestrator._summary.materials_total += len(all_material_assets)
    if not material_assets:
        orchestrator._raw_material_files_converted = True
        runner.emit_log(
            "INFO",
            "[Texture] raw material pass skipped: all BGSM/BGEM files already exist in target game",
        )
        return

    runner.emit_log(
        "INFO",
        f"[Texture] raw material pass queued {len(material_assets)} BGSM/BGEM files",
    )
    params = _params_for_convert_material_assets(orchestrator, material_assets)
    report = rust_run.run_phase(
        "convert_materials_v2",
        mod_path=str(orchestrator.mod_path),
        source_extracted_dir=str(_source_material_root(orchestrator) or ""),
        params=params,
    )
    _drain_native_phase_events(rust_run, runner, "convert_materials_v2")
    converted = int(report.get("assets_written", 0) or 0)
    warnings = int(report.get("warnings", 0) or 0)
    orchestrator._summary.materials_converted += converted
    orchestrator._summary.materials_failed += warnings
    orchestrator._raw_material_files_converted = True
    runner.emit_log(
        "INFO",
        f"[Texture] raw material pass done: converted_or_existing={converted}, "
        f"failed={warnings}, elapsed_ms={report.get('elapsed_ms', 0)}",
    )


def phase_convert_materials_native(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    """Phase 5: Convert materials (BGSM/BGEM) via the native convert_materials_v2 phase.

    Uses the Rust ConversionRun dispatcher. Python does not run a fallback
    material conversion loop when a Rust run is unavailable.
    """
    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is not None and _whole_plugin_convert_all(orchestrator):
        mat_assets = [
            a for a in orchestrator.graph.all_assets if a.asset_type == "material"
        ]
        graph_cdb_mats = [a for a in mat_assets if not _is_bgsm_or_bgem_asset(a)]
        params = _params_for_convert_material_assets(orchestrator, graph_cdb_mats)
        params["convert_all"] = True
        source_extracted = str(getattr(orchestrator, "source_data_dir", "") or "")
        report = _run_phase_with_live_drain(
            rust_run,
            runner,
            "convert_materials_v2",
            mod_path=str(orchestrator.mod_path),
            source_extracted_dir=source_extracted,
            target_extracted_dir=str(
                getattr(orchestrator, "target_extracted_dir", "") or ""
            )
            or None,
            target_data_dir=str(getattr(orchestrator, "target_data_dir", "") or "")
            or None,
            params=params,
        )
        orchestrator._summary.materials_converted += report.get("assets_written", 0)
        orchestrator._summary.materials_failed += report.get("warnings", 0)
        runner.emit_log(
            "INFO",
            f"[Material] convert-all native phase done: "
            f"converted={report.get('assets_written', 0)}, "
            f"failed={report.get('warnings', 0)}, elapsed_ms={report.get('elapsed_ms', 0)}",
        )
        return

    orchestrator._register_heuristic_cubemap_assets()

    mat_assets = [
        a for a in orchestrator.graph.all_assets if a.asset_type == "material"
    ]
    if getattr(orchestrator, "_raw_material_files_converted", False):
        skipped = sum(1 for asset in mat_assets if _is_bgsm_or_bgem_asset(asset))
        if skipped:
            runner.emit_log(
                "INFO",
                f"[Material] skipped {skipped} BGSM/BGEM refs already covered by texture phase",
            )
        mat_assets = [
            asset for asset in mat_assets if not _is_bgsm_or_bgem_asset(asset)
        ]
    mat_count = len(mat_assets)
    progress.total_items = mat_count
    if getattr(orchestrator, "_raw_material_files_converted", False):
        orchestrator._summary.materials_total += mat_count
    else:
        orchestrator._summary.materials_total = mat_count

    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is None:
        runner.emit_log("WARN", "convert_materials: no Rust run; skipping")
        progress.completed_items = mat_count
        progress.current_item = ""
        runner.emit_item_progress(progress)
        return
    if not mat_assets:
        progress.completed_items = 0
        progress.current_item = ""
        runner.emit_item_progress(progress)
        return
    params = _params_for_convert_material_assets(orchestrator, mat_assets)
    report = _run_phase_with_live_drain(
        rust_run,
        runner,
        "convert_materials_v2",
        mod_path=str(orchestrator.mod_path),
        source_extracted_dir=str(
            getattr(orchestrator, "target_extracted_dir", "") or ""
        ),
        params=params,
    )
    orchestrator._summary.materials_converted += report.get("assets_written", 0)
    orchestrator._summary.materials_failed += report.get("warnings", 0)
    runner.emit_log(
        "INFO",
        f"[Material] native phase done: converted={report.get('assets_written', 0)}, "
        f"failed={report.get('warnings', 0)}, elapsed_ms={report.get('elapsed_ms', 0)}",
    )
    progress.completed_items = mat_count
    progress.current_item = ""
    runner.emit_item_progress(progress)


def _whole_plugin_convert_all(orchestrator) -> bool:
    """Whole-plugin fo76->fo4 runs enumerate-and-convert every source asset in Rust."""
    if not bool(getattr(orchestrator, "is_whole_plugin", False)):
        return False
    pair = (
        str(getattr(orchestrator, "source_game", "")).lower(),
        str(getattr(orchestrator, "target_game", "")).lower(),
    )
    return pair == ("fo76", "fo4")


def _native_texture_phase_owns_target_dedupe(orchestrator) -> bool:
    pair = (
        str(getattr(orchestrator, "source_game", "")).lower(),
        str(getattr(orchestrator, "target_game", "")).lower(),
    )
    namespace = str(getattr(orchestrator, "base_asset_namespace", "") or "").strip()
    return pair == ("fo76", "fo4") and bool(namespace)


def _params_for_convert_textures(orchestrator, tex_assets=None) -> dict:
    """Build the params dict for the native convert_textures phase."""
    if tex_assets is None:
        tex_assets = [
            a for a in orchestrator.graph.all_assets if a.asset_type == "texture"
        ]
    source_profile = getattr(orchestrator, "_source_profile", None)
    remix = {}
    if source_profile is not None:
        from bacup_lib.texture.native import texture_params

        remix = texture_params(source_profile)
    textures = []
    for asset in tex_assets:
        texture_path = asset.resolved_path or asset.source_path
        if not texture_path:
            continue
        output_subpath = getattr(asset, "output_subpath", None) or getattr(
            orchestrator, "_forced_asset_output_subpath", lambda _asset: None
        )(asset) or _scol_namespaced_asset_output_subpath(orchestrator, asset)
        if output_subpath:
            textures.append(
                {
                    "source_path": texture_path,
                    "output_subpath": str(output_subpath).replace("\\", "/"),
                }
            )
        else:
            textures.append(texture_path)
    source_extracted = (
        getattr(orchestrator, "source_data_dir", "")
        or getattr(orchestrator, "target_extracted_dir", "")
        or ""
    )
    params = {
        "textures": textures,
        "source_extracted": str(source_extracted),
        "skip_existing": not bool(getattr(orchestrator, "overwrite_existing", False)),
        "target_format_overrides": {},
        "use_gpu": not bool(getattr(orchestrator, "force_cpu_textures", False)),
        "pbr_carry": bool(getattr(orchestrator, "pbr_carry", False)),
        "landscape_mip_flooding": bool(
            getattr(orchestrator, "texture_landscape_mip_flooding", False)
        ),
        "gpu_min_pixels": 512 * 512,
        **remix,
    }
    conversion_workers = getattr(orchestrator, "conversion_workers", None)
    if conversion_workers is not None:
        params["conversion_workers"] = int(conversion_workers)
    return params


def phase_convert_textures_native(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    """Phase 4: Convert textures via the native convert_textures_v2 phase.

    Uses the Rust ConversionRun dispatcher. Python does not run a fallback
    texture conversion loop when a Rust run is unavailable.
    """
    texture_phase = "convert_textures_v2"
    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is not None and _whole_plugin_convert_all(orchestrator):
        source_extracted = str(getattr(orchestrator, "source_data_dir", "") or "")
        params = _params_for_convert_textures(orchestrator, [])
        params["convert_all"] = True
        params["source_extracted"] = source_extracted
        report = _run_phase_with_live_drain(
            rust_run,
            runner,
            texture_phase,
            mod_path=str(orchestrator.mod_path),
            source_extracted_dir=source_extracted,
            target_extracted_dir=str(
                getattr(orchestrator, "target_extracted_dir", "") or ""
            )
            or None,
            target_data_dir=str(getattr(orchestrator, "target_data_dir", "") or "")
            or None,
            params=params,
        )
        written = report.get("assets_written", 0)
        failed = report.get("warnings", 0)
        target_skipped = report.get("records_dropped", 0)
        total = written + failed + target_skipped
        orchestrator._summary.textures_total += total
        orchestrator._summary.textures_converted += written
        orchestrator._summary.textures_failed += failed
        orchestrator._summary.textures_base_game_skipped += target_skipped
        progress.total_items = total
        progress.completed_items = total
        progress.current_item = ""
        runner.emit_item_progress(progress)
        runner.emit_log(
            "INFO",
            f"[Texture] convert-all native phase done: "
            f"written_or_existing={written}, "
            f"target-game skipped={target_skipped}, "
            f"failed={failed}, elapsed_ms={report.get('elapsed_ms', 0)}",
        )
        return
    _augment_raw_material_texture_assets(orchestrator, runner)
    tex_assets = [a for a in orchestrator.graph.all_assets if a.asset_type == "texture"]
    tex_count = len(tex_assets)
    progress.total_items = tex_count
    orchestrator._summary.textures_total = tex_count

    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is None:
        runner.emit_log("WARN", "convert_textures: no Rust run; skipping")
        progress.completed_items = tex_count
        progress.current_item = ""
        runner.emit_item_progress(progress)
        return

    convert_assets = []
    base_game_skipped = 0
    native_owns_target_dedupe = _native_texture_phase_owns_target_dedupe(orchestrator)
    for asset in tex_assets:
        if not native_owns_target_dedupe and orchestrator._target_has_asset(asset):
            base_game_skipped += 1
            orchestrator._track_asset(asset, "base_game_skip", "exists in target game")
        else:
            convert_assets.append(asset)
    if base_game_skipped:
        orchestrator._summary.textures_base_game_skipped += base_game_skipped
        runner.emit_log(
            "INFO",
            f"[Texture] skipped {base_game_skipped} textures already present in target game",
        )
    unresolved_count = sum(1 for asset in convert_assets if not asset.resolved_path)
    runner.emit_log(
        "INFO",
        f"[Texture] total referenced={tex_count}, queued={len(convert_assets)}, "
        f"target-game skipped={base_game_skipped}, unresolved={unresolved_count}",
    )
    params = _params_for_convert_textures(orchestrator, convert_assets)
    report = _run_phase_with_live_drain(
        rust_run,
        runner,
        texture_phase,
        mod_path=str(orchestrator.mod_path),
        source_extracted_dir=str(
            getattr(orchestrator, "source_data_dir", "")
            or getattr(orchestrator, "target_extracted_dir", "")
            or ""
        ),
        target_extracted_dir=str(
            getattr(orchestrator, "target_extracted_dir", "") or ""
        )
        or None,
        target_data_dir=str(getattr(orchestrator, "target_data_dir", "") or "") or None,
        params=params,
    )
    orchestrator._summary.textures_converted += report.get("assets_written", 0)
    orchestrator._summary.textures_failed += unresolved_count + report.get(
        "warnings", 0
    )
    native_target_skipped = int(report.get("records_dropped", 0) or 0)
    orchestrator._summary.textures_base_game_skipped += native_target_skipped
    native_warnings = int(report.get("warnings", 0) or 0)
    total_failed = unresolved_count + native_warnings
    runner.emit_log(
        "INFO",
        f"[Texture] native phase done: written_or_existing={report.get('assets_written', 0)}, "
        f"native_target_skipped={native_target_skipped}, "
        f"native_failed={native_warnings}, unresolved={unresolved_count}, "
        f"total_failed={total_failed}, elapsed_ms={report.get('elapsed_ms', 0)}",
    )
    _convert_raw_bgsm_materials_from_texture_phase(orchestrator, runner)

    progress.completed_items = tex_count
    progress.current_item = ""
    runner.emit_item_progress(progress)


def _params_for_extract_atx(orchestrator) -> dict | None:
    """Collect ATX slugs from the dependency graph.

    Returns ``None`` when no FNV-style ATX entries are present (skip phase).
    """
    slugs: list[str] = []
    seen: set[str] = set()
    for asset in orchestrator.graph.all_assets:
        sp = (asset.source_path or "").replace("\\", "/").lower()
        # Match Materials/ATX/Weapons/<slug>/...
        marker = "materials/atx/weapons/"
        idx = sp.find(marker)
        if idx < 0:
            continue
        rest = sp[idx + len(marker) :]
        slug = rest.split("/", 1)[0]
        if slug and slug not in seen:
            seen.add(slug)
            slugs.append(slug)
    if not slugs:
        return None
    return {
        "atx_slugs": slugs,
        "source_extracted": str(
            getattr(orchestrator, "target_extracted_dir", "") or ""
        ),
        "mod_name": os.path.basename(orchestrator.mod_path),
    }


def phase_extract_atx_native(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    """Phase: Walk extracted ATX skin BGSMs via the native extract_atx phase.

    Observability only — the actual MaterialSwap record synthesis is owned by
    ``_augment_graph_with_atx_and_sounds`` (Python). No-op when no Rust run
    is live or no ATX slugs are present in the graph.
    """
    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is None:
        return
    params = _params_for_extract_atx(orchestrator)
    if params is None:
        return
    try:
        report = rust_run.run_phase(
            "extract_atx",
            mod_path=str(orchestrator.mod_path),
            source_extracted_dir=str(
                getattr(orchestrator, "target_extracted_dir", "") or ""
            ),
            params=params,
        )
    except Exception as exc:
        runner.emit_log("WARN", f"extract_atx phase failed (non-fatal): {exc}")
        return
    runner.emit_log(
        "INFO",
        f"[ATX] native phase done: bgsm_found={report.get('assets_written', 0)}, "
        f"warnings={report.get('warnings', 0)}, elapsed_ms={report.get('elapsed_ms', 0)}",
    )


def _params_for_convert_havok(orchestrator) -> dict:
    """Build the params dict for the native convert_havok phase."""
    source_profile = getattr(orchestrator, "_source_profile", None)
    target_profile = getattr(orchestrator, "_target_profile", None)
    asset_prefix = getattr(source_profile, "asset_prefix", "") or ""

    target_version_id = getattr(target_profile, "havok_version_id", None)

    behavior_assets = [
        a for a in orchestrator.graph.all_assets if a.asset_type == "behavior"
    ]
    animation_assets = [
        a for a in orchestrator.graph.all_assets if a.asset_type == "animation"
    ]
    nif_assets = [a for a in orchestrator.graph.all_assets if a.asset_type == "nif"]

    # Expand animation_dir assets before dispatching the native HKX phase.
    extracted_dir = None
    for a in orchestrator.graph.all_assets:
        if a.resolved_path:
            rp = a.resolved_path.replace("\\", "/")
            sp = a.source_path.replace("\\", "/")
            idx = rp.lower().find(sp.lower())
            if idx > 0:
                extracted_dir = rp[:idx].rstrip("/")
                break
    expanded_anims = orchestrator._expand_animation_dirs(extracted_dir)
    animation_assets = animation_assets + expanded_anims

    all_hkx = behavior_assets + animation_assets

    target_behaviors = list(orchestrator._load_target_behavior_paths())

    hkx_entries = [
        {
            "source_path": a.source_path,
            "resolved_path": a.resolved_path or "",
            "asset_type": a.asset_type,
        }
        for a in all_hkx
    ]
    nif_entries = [
        {
            "source_path": a.source_path,
            "resolved_path": a.resolved_path or "",
        }
        for a in nif_assets
    ]

    return {
        "source_game": orchestrator.source_game,
        "target_game": orchestrator.target_game,
        "additional_source_asset_roots": [
            str(root)
            for root in getattr(orchestrator, "additional_source_asset_roots", ()) or ()
        ],
        "target_version_id": str(target_version_id)
        if target_version_id is not None
        else "",
        "hkx_assets": hkx_entries,
        "nif_assets": nif_entries,
        "target_behaviors": target_behaviors,
        "asset_prefix": asset_prefix,
        "overwrite_existing": bool(orchestrator.overwrite_existing),
    }


def phase_convert_havok_native(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    """Phase 6: Convert Havok HKX assets via the native convert_havok phase.

    Uses the Rust ConversionRun dispatcher.
    """
    behavior_assets = [
        a for a in orchestrator.graph.all_assets if a.asset_type == "behavior"
    ]
    animation_assets = [
        a for a in orchestrator.graph.all_assets if a.asset_type == "animation"
    ]
    hkx_count = len(behavior_assets) + len(animation_assets)
    progress.total_items = hkx_count
    orchestrator._summary.havok_total = len(behavior_assets)
    orchestrator._summary.animations_total = len(animation_assets)

    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is None:
        runner.emit_log("WARN", "convert_havok: no Rust run; skipping")
        return

    params = _params_for_convert_havok(orchestrator)
    report = rust_run.run_phase(
        "convert_havok",
        mod_path=str(orchestrator.mod_path),
        source_extracted_dir=str(
            getattr(orchestrator, "target_extracted_dir", "") or ""
        ),
        params=params,
    )
    orchestrator._summary.havok_converted += report.get("assets_written", 0)
    orchestrator._summary.havok_base_game_skipped += report.get("records_dropped", 0)
    orchestrator._summary.havok_failed += report.get("warnings", 0)
    runner.emit_log(
        "INFO",
        f"[HKX] native phase done: converted={report.get('assets_written', 0)}, "
        f"remapped={report.get('records_dropped', 0)}, "
        f"failed={report.get('warnings', 0)}, elapsed_ms={report.get('elapsed_ms', 0)}",
    )


def phase_postprocess_havok_native(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    """Post-process converted HKX files in mod_path (strip events, inject
    animation names, fix rig paths, filter unreferenced behaviors).

    Mirrors the postprocess_havok_assets WaveStage in the whole-plugin path
    (unified.py build_wave_a4) — runs after convert_havok and all driver/
    animation synthesis phases so every output HKX is final before the
    post-processors touch them.
    """
    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is None:
        runner.emit_log("WARN", "postprocess_havok_assets: no Rust run; skipping")
        return
    report = rust_run.run_phase(
        "postprocess_havok_assets",
        mod_path=str(orchestrator.mod_path),
        source_extracted_dir=str(
            getattr(orchestrator, "target_extracted_dir", "") or ""
        ),
        params={},
    )
    orchestrator._summary.havok_converted += int(report.get("assets_written", 0) or 0)
    orchestrator._summary.havok_failed += int(report.get("warnings", 0) or 0)
    runner.emit_log(
        "INFO",
        f"[HKX] postprocess done: written={report.get('assets_written', 0)}, "
        f"warnings={report.get('warnings', 0)}, elapsed_ms={report.get('elapsed_ms', 0)}",
    )


def _params_for_convert_animations(orchestrator) -> dict:
    """Build the params dict for the native convert_animations phase."""
    source_profile = getattr(orchestrator, "_source_profile", None)
    asset_prefix = getattr(source_profile, "asset_prefix", "") or ""

    kf_assets = [
        a
        for a in orchestrator.graph.all_assets
        if a.asset_type in ("animation", "kf_animation")
    ]

    target_behaviors = list(orchestrator._load_target_behavior_paths())

    return {
        "animations": [
            {
                "source_path": a.source_path,
                "resolved_path": a.resolved_path or "",
                "asset_type": a.asset_type,
            }
            for a in kf_assets
        ],
        "target_behaviors": target_behaviors,
        "asset_prefix": asset_prefix,
        "source_game": orchestrator.source_game,
        "target_game": orchestrator.target_game,
        "overwrite_existing": bool(orchestrator.overwrite_existing),
    }


def phase_convert_animations_native(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    """Convert KF animations to HKX via the native convert_animations phase.

    Only runs on gamebryo source profiles (FO3/FNV) targeting creation1 (FO4).
    Same-engine HKX conversion (FO76→FO4) is handled by phase_convert_havok_native.
    """
    src_engine = (
        getattr(orchestrator._source_profile, "engine", None)
        if orchestrator._source_profile
        else None
    )
    tgt_engine = (
        getattr(orchestrator._target_profile, "engine", None)
        if orchestrator._target_profile
        else None
    )
    if src_engine != "gamebryo" or tgt_engine != "creation1":
        return

    kf_assets = [
        a
        for a in orchestrator.graph.all_assets
        if a.asset_type in ("animation", "kf_animation")
    ]
    progress.total_items = len(kf_assets)

    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is None:
        runner.emit_log("WARN", "convert_animations: no Rust run; skipping")
        return

    params = _params_for_convert_animations(orchestrator)
    report = rust_run.run_phase(
        "convert_animations",
        mod_path=str(orchestrator.mod_path),
        source_extracted_dir=str(
            getattr(orchestrator, "target_extracted_dir", "") or ""
        ),
        params=params,
    )
    orchestrator._summary.animations_converted += report.get("assets_written", 0)
    orchestrator._summary.animations_failed += report.get("warnings", 0)
    runner.emit_log(
        "INFO",
        f"[Animations] native phase done: converted={report.get('assets_written', 0)}, "
        f"skipped={report.get('records_dropped', 0)}, "
        f"failed={report.get('warnings', 0)}, elapsed_ms={report.get('elapsed_ms', 0)}",
    )


def _params_for_convert_skeleton(orchestrator) -> dict | None:
    """Build params for the native convert_skeleton phase, or None if no skeleton NIF is present."""
    skeleton_assets = [
        a
        for a in orchestrator.graph.all_assets
        if a.asset_type == "nif" and "skeleton" in a.source_path.lower()
    ]
    if not skeleton_assets:
        return None
    asset = skeleton_assets[0]
    return {
        "skeleton_nif": asset.source_path,
        "resolved_path": asset.resolved_path or "",
        "source_game": orchestrator.source_game,
        "target_game": orchestrator.target_game,
        "creature_type": None,
        "skeleton_name": None,
        "bone_name_map": None,
    }


def phase_convert_skeleton_native(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    """Convert a creature skeleton NIF → HKX via the native convert_skeleton phase.

    Only runs on gamebryo source profiles (FO3/FNV) targeting creation1 (FO4)
    when the dependency graph contains a skeleton NIF asset.
    """
    src_engine = (
        getattr(orchestrator._source_profile, "engine", None)
        if orchestrator._source_profile
        else None
    )
    tgt_engine = (
        getattr(orchestrator._target_profile, "engine", None)
        if orchestrator._target_profile
        else None
    )
    if src_engine != "gamebryo" or tgt_engine != "creation1":
        return

    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is None:
        runner.emit_log("WARN", "convert_skeleton: no Rust run; skipping")
        return

    params = _params_for_convert_skeleton(orchestrator)
    if params is None:
        runner.emit_log("INFO", "[Skeleton] no skeleton NIF in graph; skipping")
        return

    report = rust_run.run_phase(
        "convert_skeleton",
        mod_path=str(orchestrator.mod_path),
        source_extracted_dir=str(
            getattr(orchestrator, "target_extracted_dir", "") or ""
        ),
        params=params,
    )
    runner.emit_log(
        "INFO",
        f"[Skeleton] native phase done: written={report.get('assets_written', 0)}, "
        f"warnings={report.get('warnings', 0)}, elapsed_ms={report.get('elapsed_ms', 0)}",
    )


def phase_synthesize_drivers_native(
    orchestrator, runner: ConversionRunner, progress: PhaseProgress
) -> None:
    """Phase 7: Synthesize behavior driver chains via the native synthesize_drivers phase.

    Uses the Rust ConversionRun dispatcher. Python does not mutate translated
    records after this native asset phase.
    """
    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is None:
        runner.emit_log("WARN", "synthesize_drivers: no Rust run; skipping")
        return

    params: dict = {}
    report = rust_run.run_phase(
        "synthesize_drivers",
        mod_path=str(orchestrator.mod_path),
        source_extracted_dir=str(
            getattr(orchestrator, "target_extracted_dir", "") or ""
        ),
        params=params,
    )
    runner.emit_log(
        "INFO",
        f"[BehaviorDriver] native phase done: files_patched={report.get('assets_written', 0)}, "
        f"chains={report.get('records_dropped', 0)}, "
        f"skipped={report.get('warnings', 0)}, elapsed_ms={report.get('elapsed_ms', 0)}",
    )


def _conversion_form_key_map_sources(orchestrator) -> list[str]:
    sources: list[str] = []
    graph = getattr(orchestrator, "graph", None)
    _append_unique_strings(
        sources,
        [
            str(getattr(node, "form_key", "") or "")
            for node in list(getattr(graph, "all_records", []) or [])
            if str(getattr(node, "form_key", "") or "")
        ],
    )
    _append_unique_strings(
        sources,
        [
            str(value)
            for value in list(
                getattr(orchestrator, "_conversion_form_key_map_sources", []) or []
            )
            if str(value or "")
        ],
    )
    return sources


def _capture_source_to_target_form_key_map(
    orchestrator, run, runner: ConversionRunner
) -> None:
    sources = _conversion_form_key_map_sources(orchestrator)
    orchestrator._source_to_target_form_key_map = {}
    if not sources:
        return

    try:
        from bacup_lib.native_runtime import (
            load_native_module as _conv_native,
        )

        native_module = _conv_native()
        get_form_key_map = getattr(native_module, "conversion_run_form_key_map", None)
        if get_form_key_map is None:
            return
        raw_map = dict(get_form_key_map(int(run.id), sources))
    except Exception as exc:
        runner.emit_log("WARN", f"form-key map capture failed: {exc}")
        return

    orchestrator._source_to_target_form_key_map = {
        str(source): str(target)
        for source, target in raw_map.items()
        if str(source) and str(target)
    }
    if orchestrator._source_to_target_form_key_map:
        runner.emit_log(
            "INFO",
            "captured source->target form-key remaps: "
            f"{len(orchestrator._source_to_target_form_key_map)}",
        )


def _drain_and_drop_rust_run(orchestrator, runner: ConversionRunner) -> None:
    """Drain decisions/warnings from a stashed Rust ConversionRun and drop it.

    Mirrors plugin_port._drain_and_drop_rust_run. Idempotent — safe when no
    run was created (legacy backend or no source handle).
    """
    run = getattr(orchestrator, "_rust_conversion_run", None)
    if run is None:
        return
    _capture_source_to_target_form_key_map(orchestrator, run, runner)
    try:
        decisions = run.drain_decisions()
        warnings = run.drain_warnings()
    except Exception:
        decisions = []
        warnings = []
    if decisions:
        existing = list(getattr(orchestrator, "_conversion_decisions", []) or [])
        # Older Python decisions may be `ConversionDecision` instances; Rust
        # returns dicts. Downstream consumers handle either shape.
        existing.extend(decisions)
        orchestrator._conversion_decisions = existing
    if warnings:
        existing_logs = list(getattr(orchestrator, "_log_lines", []) or [])
        existing_logs.extend(f"[WARN] {w}" for w in warnings)
        orchestrator._log_lines = existing_logs
        orchestrator._summary.records_warnings += len(warnings)
    try:
        run.__exit__(None, None, None)
    except Exception:
        pass
    orchestrator._rust_conversion_run = None


def _candidate_creature_output_names(actors_root: Path) -> list[str]:
    candidate_names: set[str] = set()

    for actor_dir in actors_root.iterdir():
        if not actor_dir.is_dir():
            continue
        target_name = actor_dir.name
        if _has_creature_signature(actor_dir, target_name):
            candidate_names.add(target_name)

    return sorted(candidate_names)


def _has_creature_signature(actor_dir: Path, target_name: str) -> bool:
    return any(
        path.exists()
        for path in _required_creature_output_paths(actor_dir, target_name)
    )


def _required_creature_output_paths(
    actor_dir: Path, target_name: str
) -> tuple[Path, ...]:
    return (
        actor_dir / "CharacterAssets" / "skeleton.hkx",
        actor_dir / f"{target_name}Project.hkx",
        actor_dir / "Characters" / f"{target_name}.hkx",
        actor_dir / "Behaviors" / f"{target_name}RootBehavior.hkx",
        actor_dir / "Behaviors" / f"{target_name}Everything.hkx",
    )


@dataclasses.dataclass
class AssetConversionWorkerResult:
    logs: list[tuple[str, str]] = dataclasses.field(default_factory=list)
    log_lines: list[str] = dataclasses.field(default_factory=list)
    emitted_bgsms: list[str] = dataclasses.field(default_factory=list)


class _QuotedYamlString(str):
    pass


def _unpack_hkx_to_temp_xml(hkx_path: str) -> str:
    """Adapt native XML-string unpacking for callers that still need a path."""
    from creation_lib._native.havok_native import unpack_hkx_to_xml

    xml_text = unpack_hkx_to_xml(hkx_path)
    tmp_dir = tempfile.mkdtemp(prefix="hkxunpack_")
    xml_path = os.path.join(tmp_dir, Path(hkx_path).stem + ".xml")
    Path(xml_path).write_text(xml_text, encoding="utf-8")
    return xml_path


# Record types that should be NULLED (not stub-allocated) when reached
# only via the orphan sweep on a weapon/armor root. These are leaf-ish
# support records (animation/dialogue keywords, impact sets, etc.) that
# tend to come in via reverse-injected Race subgraphs and have no FO4
# meaning. Cloning them produces hundreds of orphaned records that
# break the output ESP. NPC roots are exempt â€” creature conversions
# legitimately need clones of their support records.
_SWEEP_NULL_TYPES_FOR_WEAPON_ROOTS: frozenset[str] = frozenset(
    {
        "Keywords",
        "ImpactDataSets",
        "MaterialTypes",
        "EquipTypes",
        "VoiceTypes",
        "BodyParts",
        "AnimationSoundTagSets",
        "MovementTypes",
        "AimModels",
        "Zooms",
        "InstanceNamingRules",
        "AttachParentSlots",
        "MagicEffects",
        "Spells",
        "ObjectEffects",
        "Perks",
    }
)

_DROPPED_TRANSLATED_RECORD_KEY = "__conversion_dropped__"


def _is_dropped_translated_record(translated: Any) -> bool:
    return (
        isinstance(translated, dict)
        and translated.get(_DROPPED_TRANSLATED_RECORD_KEY) is True
    )


def _patch_hkt_to_hkx(hkx_path: str) -> None:
    """Replace .hkt references with .hkx inside a converted HKX binary.

    FO76 behavior/character HKX files reference skeletons and animations
    using the .hkt extension.  FO4 expects .hkx.  Both are 4 bytes so
    this is a safe same-length binary substitution.
    """
    p = Path(hkx_path)
    data = p.read_bytes()
    patched = data.replace(b".hkt", b".hkx")
    if patched is not data:  # only write if changed
        p.write_bytes(patched)


def _inject_hitframe_events(anim_dir: str) -> list[str]:
    """Inject missing HitFrame annotation events into attack animations.

    FO76 attack animations have preHitFrame and weaponSwing but no HitFrame.
    FO4 requires HitFrame for damage timing.  This scans converted attack
    animations and injects HitFrame at the time of WeaponSweepAttackStart
    (the impact moment), or midway between weaponSwing and the next event
    if WeaponSweepAttackStart is absent.

    Returns list of filenames that were patched.
    """
    from creation_lib._native.havok_native import load_hkx, write_hkx

    patched_files: list[str] = []
    if not os.path.isdir(anim_dir):
        return patched_files

    for fname in os.listdir(anim_dir):
        if not fname.lower().startswith("attack") or not fname.lower().endswith(".hkx"):
            continue
        fpath = os.path.join(anim_dir, fname)
        try:
            hkx, reg = load_hkx(fpath)
        except Exception:
            continue

        modified = False
        for obj in hkx.objects:
            if (
                "SplineCompressed" not in obj.class_name
                and "Interleaved" not in obj.class_name
            ):
                continue
            for member in obj.members:
                if not (hasattr(member, "name") and member.name == "annotationTracks"):
                    continue
                if not member.contents:
                    continue
                # Only check track 0 (root bone track where events live)
                track = member.contents[0]
                ann_member = None
                for tm in track.members:
                    if hasattr(tm, "name") and tm.name == "annotations":
                        ann_member = tm
                        break
                if ann_member is None:
                    continue

                # Collect existing events
                events: list[tuple[float, str]] = []
                for ann in ann_member.contents:
                    time_val = text_val = None
                    for am in ann.members:
                        if hasattr(am, "name") and am.name == "time":
                            time_val = am.value
                        if hasattr(am, "name") and am.name == "text":
                            text_val = am.value
                    if time_val is not None and text_val is not None:
                        events.append((time_val, text_val))

                has_hitframe = any(str(t).lower() == "hitframe" for _, t in events)
                has_prehitframe = any(
                    str(t).lower() == "prehitframe" for _, t in events
                )

                if has_hitframe or not has_prehitframe:
                    continue  # Already has HitFrame or no preHitFrame to anchor on

                # Determine HitFrame time:
                # Prefer WeaponSweepAttackStart time (actual impact)
                # Fallback: midpoint between weaponSwing and next event after it
                hitframe_time = None
                sweep_times = [
                    t
                    for t, txt in events
                    if str(txt).lower() == "weaponsweepattackstart"
                ]
                if sweep_times:
                    hitframe_time = sweep_times[0]
                else:
                    # Find weaponSwing time and compute midpoint to next event
                    ws_times = [
                        t for t, txt in events if str(txt).lower() == "weaponswing"
                    ]
                    if ws_times:
                        ws_time = ws_times[0]
                        later = [t for t, _ in events if t > ws_time]
                        if later:
                            hitframe_time = (ws_time + min(later)) / 2.0
                        else:
                            # weaponSwing is last event; put HitFrame slightly after
                            hitframe_time = ws_time + 0.1
                    else:
                        # No weaponSwing either; put HitFrame slightly after preHitFrame
                        phf_times = [
                            t for t, txt in events if str(txt).lower() == "prehitframe"
                        ]
                        hitframe_time = phf_times[0] + 0.1

                # Build the HitFrame annotation object matching existing structure
                from creation_lib._native.havok_native import (
                    HKXObject,
                    HKXDirectMember,
                    HKXStringMember,
                    HKXType,
                )

                hitframe_ann = HKXObject(
                    name="",
                    class_name="hkaAnnotationTrackAnnotation",
                    schema_version=0,
                    members=[
                        HKXDirectMember(
                            name="time", type=HKXType.REAL, value=hitframe_time
                        ),
                        HKXStringMember(name="text", value="HitFrame", is_null=False),
                    ],
                )

                # Insert in sorted time order
                insert_idx = 0
                for i, ann in enumerate(ann_member.contents):
                    for am in ann.members:
                        if (
                            hasattr(am, "name")
                            and am.name == "time"
                            and am.value <= hitframe_time
                        ):
                            insert_idx = i + 1
                ann_member.contents.insert(insert_idx, hitframe_ann)
                modified = True

        if modified:
            out_bytes = write_hkx(hkx, reg)
            Path(fpath).write_bytes(out_bytes)
            patched_files.append(fname)

    return patched_files


def _strip_source_game_events_from_hkx(
    hkx_path: str,
    source_game: str,
    target_game: str = "fo4",
) -> tuple[int, int, list[str]]:
    """Strip source-game-only annotation events from a converted .hkx file.

    Uses EventMapper to decide per-annotation whether to drop, rewrite, or
    pass through.  Only touches `hkaSplineCompressedAnimation` /
    `hkaInterleavedUncompressedAnimation` -> `annotationTracks[*].annotations`.
    `hkbBehaviorGraphStringData.eventNames` is left untouched on purpose:
    those entries are referenced by index elsewhere in the behavior graph
    (hkbStateMachineTransitionInfo.eventId, hkbEventDrivenModifier.activateEventId,
    etc.) and removing one would silently shift every downstream index.
    CK's sync-data-gen crash is driven by annotation-track text matching,
    so stripping animation-side annotations is sufficient.

    Args:
        hkx_path: Path to the already-version-converted .hkx file.
        source_game: Source game key recognized by EventMapper (e.g. "fo76").
        target_game: Target game key (default "fo4").

    Returns:
        ``(dropped_count, renamed_count, warnings)``.  Returns ``(0, 0, [])``
        if the file contains no matching annotation tracks, if the mapping
        YAML doesn't exist, or if the file can't be read.  The file is only
        rewritten when at least one annotation was dropped or renamed.
    """
    from bacup_lib.animation.event_mapper import EventMapper
    from bacup_lib.models import AnimationEvent
    from creation_lib._native.havok_native import HKXStringMember, load_hkx, write_hkx

    try:
        mapper = EventMapper(source_game, target_game)
    except FileNotFoundError:
        return 0, 0, []

    try:
        hkx, reg = load_hkx(hkx_path)
    except Exception as exc:  # noqa: BLE001 â€” tolerate bad inputs
        _log.warning("Event-strip: failed to load %s: %s", hkx_path, exc)
        return 0, 0, []

    dropped = 0
    renamed = 0
    warnings: list[str] = []
    changed = False

    for obj in hkx.objects:
        if (
            "SplineCompressed" not in obj.class_name
            and "Interleaved" not in obj.class_name
        ):
            continue
        for member in obj.members:
            if not (hasattr(member, "name") and member.name == "annotationTracks"):
                continue
            if not member.contents:
                continue
            for track in member.contents:
                ann_member = None
                for tm in track.members:
                    if hasattr(tm, "name") and tm.name == "annotations":
                        ann_member = tm
                        break
                if ann_member is None or not ann_member.contents:
                    continue

                # Build a filtered list.  Index-order iteration is safe
                # because we reassign `contents` after â€” we don't mutate
                # during iteration.
                new_contents = []
                for ann in ann_member.contents:
                    time_val = None
                    text_val = None
                    text_slot = None  # reference to the HKXStringMember
                    for am in ann.members:
                        if hasattr(am, "name") and am.name == "time":
                            time_val = am.value
                        if hasattr(am, "name") and am.name == "text":
                            text_val = am.value
                            text_slot = am
                    if time_val is None or text_val is None:
                        # Malformed annotation â€” keep as-is
                        new_contents.append(ann)
                        continue

                    ev = AnimationEvent(time=time_val, text=text_val)
                    result, warning = mapper.map_event(ev)
                    if warning is not None:
                        warnings.append(warning)
                    if result is None:
                        # Dropped
                        dropped += 1
                        changed = True
                        continue
                    if result.text != text_val:
                        # Rewrite in place
                        if isinstance(text_slot, HKXStringMember):
                            text_slot.value = result.text
                            text_slot.is_null = False
                        else:
                            text_slot.value = result.text
                        renamed += 1
                        changed = True
                    new_contents.append(ann)

                if len(new_contents) != len(ann_member.contents):
                    ann_member.contents = new_contents

    if changed:
        try:
            out_bytes = write_hkx(hkx, reg)
            Path(hkx_path).write_bytes(out_bytes)
        except Exception as exc:  # noqa: BLE001
            _log.error("Event-strip: failed to write %s: %s", hkx_path, exc)
            return 0, 0, warnings

    return dropped, renamed, warnings


def _collect_behavior_clip_names(behavior_dir: str) -> set[str]:
    """Extract unique animation names from hkbClipGenerator objects in behavior HKX files.

    Parses all .hkx files in the behavior directory, finds hkbClipGenerator
    objects, and returns their animationName values (e.g. "Animations\\Idle.hkt").

    These are the animations the behavior graph actually references â€” only these
    should appear in character.hkx's animationBundleNameData.assetNames.
    """
    from creation_lib._native.havok_native import (
        HKXDirectMember,
        HKXStringMember,
        load_hkx_bytes,
    )

    clip_names: set[str] = set()
    if not os.path.isdir(behavior_dir):
        return clip_names

    for fname in os.listdir(behavior_dir):
        if not fname.lower().endswith(".hkx"):
            continue
        fpath = os.path.join(behavior_dir, fname)
        try:
            data = Path(fpath).read_bytes()
            hkx, _ = load_hkx_bytes(data)
        except Exception:
            _log.warning("Failed to parse behavior HKX: %s", fpath)
            continue

        for obj in hkx.objects:
            if obj.class_name != "hkbClipGenerator":
                continue
            for m in obj.members:
                if m.name == "animationName":
                    val = None
                    if isinstance(m, HKXStringMember):
                        val = m.value
                    elif isinstance(m, HKXDirectMember) and isinstance(m.value, str):
                        val = m.value
                    if val:
                        clip_names.add(val)

    return clip_names


# FO76-only generic behavior files that have no FO4 equivalent.
# These are shared across multiple FO76 creatures but aren't used by FO4's
# behavior graph system.  FO4 loads ALL .hkx files in a creature's Behaviors/
# directory (via BSBehaviorGraphSwapGenerator), so extra files can confuse
# the graph.  Creature-specific behaviors (named after the creature) are kept.
_FO76_ONLY_BEHAVIORS: frozenset[str] = frozenset(
    {
        "ambushbehavior.hkx",
        "dialoguebehavior.hkx",
        "furniturebed.hkx",
        "furniturebehavior.hkx",
        "furniturefishingbehavior.hkx",
        "furniturenomirrorbehavior.hkx",
        "sharedcorebehavior.hkx",
        "sharedrootbehavior.hkx",
        "sharedcorewrappingbehavior.hkx",
    }
)


def _filter_unreferenced_behaviors(
    behavior_dir: str,
) -> list[str]:
    """Remove known FO76-only generic behavior files from the output.

    FO4 loads ALL .hkx files in a creature's Behaviors/ directory via
    BSBehaviorGraphSwapGenerator (runtime graph swapping).  There is no
    explicit reference chain we can follow â€” the swap generator's
    pDefaultGenerator pointer is null in packfiles and resolved at runtime.

    Instead of reference-based filtering, we remove a known set of FO76-only
    generic behaviors (ambush, dialogue, furniture, shared*) that have no
    FO4 equivalent.  Creature-specific behaviors (named after the creature)
    are always kept.

    Returns list of removed filenames.
    """
    if not os.path.isdir(behavior_dir):
        return []

    removed = []
    for fname in os.listdir(behavior_dir):
        if not fname.lower().endswith(".hkx"):
            continue
        if fname.lower() in _FO76_ONLY_BEHAVIORS:
            fpath = os.path.join(behavior_dir, fname)
            os.remove(fpath)
            removed.append(fname)

    return removed


def _inject_animation_names_into_character_hkx(
    character_hkx_path: str,
    animation_dir: str,
    behavior_clip_names: set[str] | None = None,
) -> int:
    """Populate animationBundleNameData[0].assetNames in a converted character.hkx.

    FO76 character.hkx files have an empty assetNames array because FO76 resolves
    animations differently.  FO4 needs the character.hkx to list every animation
    file the behavior graph references, otherwise the creature T-poses.

    Also cleans up conversion artifacts:
    - Removes orphan hkbBoneIndexArray objects with empty boneIndices that are
      unreferenced (TAG0 reader artifact)
    - Removes the empty trailing variantVariableValues entry that points at
      the orphan object

    Returns the number of animation names injected (0 if nothing to do).
    """
    from creation_lib._native.havok_native import (
        HKXArrayMember,
        HKXStringMember,
        HKXObject,
        HKXPointerMember,
        HKXType,
        load_hkx_bytes,
        write_hkx,
    )

    if not os.path.isfile(character_hkx_path):
        return 0
    if not os.path.isdir(animation_dir):
        return 0

    # Collect animation filenames relative to the Animations/ directory.
    # FO4 character.hkx uses .hkt extension in assetNames â€” the engine
    # patches .hktâ†’.hkx at load time, and CK expects .hkt for AnimTextData
    # generation.  Confirmed against vanilla FO4 Deathclaw character.hkx.
    if behavior_clip_names:
        # Use behavior-referenced clips only (already in Animations\Name.hkt format)
        anim_files = sorted(behavior_clip_names)
    else:
        # Fallback: scan disk (less accurate â€” includes unreferenced animations)
        anim_files = []
        for fname in sorted(os.listdir(animation_dir)):
            if fname.lower().endswith(".hkx"):
                name_no_ext = os.path.splitext(fname)[0]
                anim_files.append(f"Animations\\{name_no_ext}.hkt")
        for root, _dirs, files in os.walk(animation_dir):
            rel = os.path.relpath(root, animation_dir)
            if rel == ".":
                continue
            for fname in sorted(files):
                if fname.lower().endswith(".hkx"):
                    name_no_ext = os.path.splitext(fname)[0]
                    subpath = os.path.join(
                        "Animations", rel, name_no_ext + ".hkt"
                    ).replace("/", "\\")
                    anim_files.append(subpath)

    if not anim_files:
        return 0

    # Read the character.hkx packfile
    data = Path(character_hkx_path).read_bytes()
    try:
        hkx, registry = load_hkx_bytes(data)
    except Exception:
        return 0

    # Find hkbCharacterStringData and inject animation names
    injected = 0
    for obj in hkx.objects:
        if obj.class_name != "hkbCharacterStringData":
            continue
        for m in obj.members:
            if not (
                isinstance(m, HKXArrayMember) and m.name == "animationBundleNameData"
            ):
                continue
            if not m.contents:
                continue
            bundle_obj = m.contents[0]
            if not hasattr(bundle_obj, "members"):
                continue
            bundle_members = list(bundle_obj.members)
            for bm_idx, bm in enumerate(bundle_members):
                if isinstance(bm, HKXArrayMember) and bm.name == "assetNames":
                    if not bm.contents:
                        bundle_members[bm_idx] = HKXArrayMember(
                            "assetNames",
                            HKXType.STRINGPTR,
                            anim_files,
                        )
                        bundle_obj.members = bundle_members
                        m.contents[0] = bundle_obj
                        injected = len(anim_files)
                    break
            break
        break

    if not injected:
        return 0

    # Clean up orphan hkbBoneIndexArray objects (empty boneIndices, unreferenced)
    # Collect all pointer targets
    referenced: set[str] = set()
    for obj in hkx.objects:
        for m in obj.members:
            if isinstance(m, HKXPointerMember) and m.target:
                referenced.add(m.target)
            elif isinstance(m, HKXArrayMember):
                for item in m.contents:
                    if isinstance(item, str) and item.startswith("#"):
                        referenced.add(item)

    # Find orphan bone index arrays
    orphan_names: set[str] = set()
    for obj in hkx.objects:
        if obj.class_name != "hkbBoneIndexArray":
            continue
        if not obj.name or obj.name in referenced:
            continue  # Still referenced, keep it
        has_data = False
        for m in obj.members:
            if isinstance(m, HKXArrayMember) and m.name == "boneIndices" and m.contents:
                has_data = True
                break
        if not has_data:
            orphan_names.add(obj.name)

    if orphan_names:

        def keep_object(*args) -> bool:
            obj = next((arg for arg in args if hasattr(arg, "name")), None)
            return obj is None or obj.name not in orphan_names

        hkx.objects.retain(keep_object)
        # Clean up variantVariableValues entries pointing at orphans
        for obj in hkx.objects:
            if obj.class_name != "hkbVariableValueSet":
                continue
            for m in obj.members:
                if isinstance(m, HKXArrayMember) and m.name == "variantVariableValues":
                    m.contents = [
                        v for v in m.contents if v not in orphan_names and v != ""
                    ]
                    break

    # Write back
    out_data = write_hkx(hkx, registry)
    Path(character_hkx_path).write_bytes(out_data)
    return injected


def _fix_character_rig_path_fo4(character_hkx_path: str) -> str | None:
    """Rewrite FO76-style skeleton paths in a converted character HKX to FO4 layout.

    FO76 ships `SingleBoneSkeleton.hkt` at
    `Meshes\\UniqueBehaviors\\zSingleBoneSkeleton\\` (one level up from
    `UniqueBehaviors\\<name>\\Characters\\`) so FO76 character files use
    `..\\zSingleBoneSkeleton\\SingleBoneSkeleton.hkt`.  FO4 ships the same
    skeleton at `Meshes\\GenericBehaviors\\zSingleBoneSkeleton\\` (two levels
    up), so FO4 character files use
    `..\\..\\GenericBehaviors\\zSingleBoneSkeleton\\SingleBoneSkeleton.hkt`.

    Without this rewrite, loading a converted weapon FX character file fails
    to locate the skeleton and the behavior graph never drives the NIF's
    controller sequences â€” observed on Meltdown (C-2).

    Returns the new path if rewritten, else ``None``.
    """
    from creation_lib._native.havok_native import (
        HKXStringMember,
        load_hkx_bytes,
        write_hkx,
    )

    if not os.path.isfile(character_hkx_path):
        return None

    data = Path(character_hkx_path).read_bytes()
    try:
        hkx, registry = load_hkx_bytes(data)
    except Exception:
        return None

    # FO76 sibling-folder prefix (case-insensitive, backslash-normalized).
    # FO76 ships: "..\zSingleBoneSkeleton\SingleBoneSkeleton.hkt"
    fo76_prefix_lc = "..\\zsingleboneskeleton\\"

    changed = False
    new_path: str | None = None
    for obj in hkx.objects:
        if obj.class_name != "hkbCharacterStringData":
            continue
        for m in obj.members:
            if not isinstance(m, HKXStringMember) or m.name != "rigName":
                continue
            old = m.value or ""
            low = old.lower().replace("/", "\\")
            if low.startswith(fo76_prefix_lc):
                # Extract trailing filename (e.g. "SingleBoneSkeleton.hkt")
                # from the original string so case is preserved.
                remainder = old.replace("/", "\\").split("\\", 2)[2]
                m.value = "..\\..\\GenericBehaviors\\zSingleBoneSkeleton\\" + remainder
                changed = True
                new_path = m.value
            break
        break

    if not changed:
        return None

    out_data = write_hkx(hkx, registry)
    Path(character_hkx_path).write_bytes(out_data)
    return new_path


def _try_get_profile(game_id: str):
    """Try to load a game profile, returning None if not found."""
    try:
        from creation_lib.core.game_profiles import get_profile

        return get_profile(game_id)
    except (KeyError, ImportError):
        return None


# ---------------------------------------------------------------------------
# NIF conversion helpers
# ---------------------------------------------------------------------------

_NIF_AUTO_SKIN_REFERENCE_BODY = Path(
    "meshes/actors/character/characterassets/malebody.nif"
)
_NIF_FIRST_PERSON_REFERENCE = Path(
    "meshes/actors/character/characterassets/1stpersonmalebody.nif"
)


def _nif_normalize_asset_key(path: str) -> str:
    return path.replace("\\", "/").lower()


def _nif_resolve_target_extract_nif(orchestrator, relative_path: Path) -> Path | None:
    target_asset_store = getattr(orchestrator, "target_asset_store", None)
    if target_asset_store is not None:
        return target_asset_store.materialize(relative_path)
    target_extracted_dir = getattr(orchestrator, "target_extracted_dir", "")
    if not target_extracted_dir:
        return None
    candidate = Path(target_extracted_dir) / relative_path
    return candidate if candidate.is_file() else None


def _weapon_role_for_asset(orchestrator, asset_source_path: str) -> str | None:
    cache = getattr(orchestrator, "_weapon_role_cache", None)
    if cache is None:
        from bacup_lib.weapon_report import weapon_metadata_index

        cache = {}
        seen_rows: set[int] = set()
        for row in weapon_metadata_index(orchestrator).values():
            row_id = id(row)
            if row_id in seen_rows:
                continue
            seen_rows.add(row_id)
            role = str(row.get("weapon_role") or "")
            if not role:
                continue
            for key in ("base_model", "model_mod1", "model_mod2", "model_mod3"):
                value = row.get(key)
                if isinstance(value, str) and value.strip():
                    cache[_nif_normalize_asset_key(value)] = role
        orchestrator._weapon_role_cache = cache
    return cache.get(_nif_normalize_asset_key(asset_source_path))


def resolve_nif_conversion_options(orchestrator) -> dict[str, object]:
    source_game = getattr(orchestrator, "source_game", None)
    target_game = getattr(orchestrator, "target_game", None)
    translation_maps_dir = getattr(orchestrator, "translation_maps_dir", None)
    auto_skin_reference_body = getattr(orchestrator, "auto_skin_reference_body", None)
    first_person_reference = getattr(orchestrator, "first_person_reference", None)
    emit_first_person = bool(getattr(orchestrator, "emit_first_person", False))
    morph_weight_cap = float(getattr(orchestrator, "morph_weight_cap", 0.5))
    skin_options_requested = (
        translation_maps_dir is not None
        or auto_skin_reference_body is not None
        or first_person_reference is not None
        or emit_first_person
        or morph_weight_cap != 0.5
    )
    default_legacy_skin_conversion = target_game == "fo4" and source_game in {
        "fnv",
        "fo3",
    }
    if not skin_options_requested and not default_legacy_skin_conversion:
        return {}

    translation_maps_dir = translation_maps_dir or native_translation_maps_dir()

    if target_game == "fo4" and skin_options_requested:
        if auto_skin_reference_body is None:
            auto_skin_reference_body = _nif_resolve_target_extract_nif(
                orchestrator,
                _NIF_AUTO_SKIN_REFERENCE_BODY,
            )
        if first_person_reference is None:
            first_person_reference = _nif_resolve_target_extract_nif(
                orchestrator,
                _NIF_FIRST_PERSON_REFERENCE,
            )

    options: dict[str, object] = {
        "translation_maps_dir": str(Path(translation_maps_dir)),
        "emit_first_person": emit_first_person,
        "morph_weight_cap": morph_weight_cap,
    }
    if auto_skin_reference_body is not None:
        options["auto_skin_reference_body"] = str(Path(auto_skin_reference_body))
    if first_person_reference is not None:
        options["first_person_reference"] = str(Path(first_person_reference))
    return options


def finalize_weapon_surgery(orchestrator, runner: "ConversionRunner") -> None:
    """Run FNV weapon-attachment surgery after NIF conversion."""
    from bacup_lib.weapon_attachment_surgery import is_fnv_source

    if not is_fnv_source(getattr(orchestrator, "_source_profile", None)):
        return
    from bacup_lib.weapon_attachment_surgery import (
        run_weapon_attachment_surgery,
    )

    result = run_weapon_attachment_surgery(orchestrator)
    for level, message in result.logs:
        runner.emit_log(level, message)
        orchestrator._log_lines.append(f"[{level}] {message}")


class ConversionFixups:
    """Helper methods used by conversion phase modules."""


    _LOCAL_FORMKEY_STRATEGIES = {"new_allocation", "source_id_preserved"}

    def __init__(self, orchestrator):
        self.__dict__["_orchestrator"] = orchestrator

    def __getattr__(self, name: str):
        orchestrator = self.__dict__["_orchestrator"]
        if name in getattr(orchestrator, "__dict__", {}):
            return orchestrator.__dict__[name]
        ns = getattr(orchestrator, "__dict__", {}).get("_ns")
        if ns is not None and hasattr(ns, name):
            return getattr(ns, name)
        for cls in type(orchestrator).__mro__:
            if name not in cls.__dict__:
                continue
            value = cls.__dict__[name]
            if hasattr(value, "__get__"):
                return value.__get__(orchestrator, type(orchestrator))
            return value
        raise AttributeError(name)

    def __setattr__(self, name: str, value):
        if name == "_orchestrator":
            self.__dict__[name] = value
            return
        orchestrator = self.__dict__["_orchestrator"]
        ns = getattr(orchestrator, "__dict__", {}).get("_ns")
        if ns is not None:
            setattr(ns, name, value)
            return
        setattr(orchestrator, name, value)

    def _output_plugin_extension(self) -> str:
        configured = getattr(self, "output_plugin_extension", "")
        if configured:
            ext = str(configured).strip().lower()
            if not ext.startswith("."):
                ext = f".{ext}"
            return ext if ext in {".esm", ".esp", ".esl"} else ".esp"

        root_form_key = getattr(getattr(self, "graph", None), "root", None)
        form_key = getattr(root_form_key, "form_key", "")
        plugin = form_key.split(":", 1)[1] if ":" in form_key else ""
        ext = os.path.splitext(plugin)[1].lower()
        return ext if ext in {".esm", ".esp", ".esl"} else ".esp"

    def _output_plugin_name(self) -> str:
        return f"{os.path.basename(self.mod_path)}{self._output_plugin_extension()}"

    @staticmethod
    def _is_creature_root_type(record_type: str) -> bool:
        return record_type in {"Npcs", "NPC_", "LeveledNpcs", "LVLN"}

    @staticmethod
    def _is_npc_type(record_type: str) -> bool:
        return record_type in {"Npcs", "NPC_"}

    @staticmethod
    def _is_race_type(record_type: str) -> bool:
        return record_type in {"Races", "RACE"}

    @staticmethod
    def _is_bodypart_type(record_type: str) -> bool:
        return record_type in {"BodyParts", "BPTD"}

    @staticmethod
    def _is_faction_type(record_type: str) -> bool:
        return record_type in {"Factions", "FACT"}

    @staticmethod
    def _is_constructible_object_type(record_type: str) -> bool:
        return record_type in {"ConstructibleObjects", "COBJ"}

    def _load_target_behavior_paths(self) -> set[str]:
        """Set of target-game ``.hkx`` paths to skip during HKX conversion.

        Enumerates target ``.hkx`` membership from the catalog (or legacy
        loose override) and returns each path relative to ``Meshes/`` (lower-cased,
        forward-slash) — the same ``meshes/``-stripped key the native havok
        phase matches against. Shipping a converted FO76 ``.hkx`` at a path the
        base game already owns (skeleton.hkx, the default behaviors, the shared
        animation set) overwrites the game's defaults and breaks 3rd-person
        behavior, so every base-game-path ``.hkx`` is skipped; FO76-unique paths
        absent from the base game survive.

        The legacy havok-DB lookup is gone on purpose: it indexed only
        behaviors/projects (no animations, no skeleton), so base animations and
        skeleton.hkx leaked through and clobbered FO4.
        """
        if self._target_behavior_paths is not None:
            return self._target_behavior_paths
        self._target_behavior_paths = set()
        if not self.use_base_game_assets:
            return self._target_behavior_paths
        target_asset_store = getattr(self, "target_asset_store", None)
        if target_asset_store is not None:
            self._target_behavior_paths = {
                path.removeprefix("meshes/")
                for path in target_asset_store.list_assets(
                    prefix="meshes/", suffix=".hkx"
                )
            }
            return self._target_behavior_paths
        meshes_root = os.path.join(self.target_extracted_dir or "", "Meshes")
        if not self.target_extracted_dir or not os.path.isdir(meshes_root):
            _log.warning(
                "No target extracted Meshes dir for %s; HKX base-game dedup skipped",
                self.target_game,
            )
            return self._target_behavior_paths
        paths: set[str] = set()
        for dirpath, _dirnames, filenames in os.walk(meshes_root):
            for fn in filenames:
                if fn.lower().endswith(".hkx"):
                    rel = os.path.relpath(os.path.join(dirpath, fn), meshes_root)
                    paths.add(rel.replace("\\", "/").lower())
        self._target_behavior_paths = paths
        return self._target_behavior_paths

    def _load_target_nif_paths(self) -> set[str]:
        """Set of target-game ``.nif`` paths to skip during NIF conversion.

        Enumerates target ``.nif`` membership from the catalog (or legacy
        loose override) and returns each path relative to ``Meshes/`` (lower-cased,
        forward-slash) — the ``meshes/``-stripped key ``_target_has_asset``
        matches against. Shipping a converted FO76 ``.nif`` at a path the base
        game already owns would clobber the vanilla mesh, so base-game-path
        NIFs are skipped; FO76-unique paths survive.

        The legacy ``data/<game>_nifs.db`` lookup remains unnecessary because
        the packaged target-asset catalog is the distributable membership index.
        """
        cached = getattr(self, "_target_nif_paths", None)
        if cached is not None:
            return cached
        self._target_nif_paths = set()
        if not self.use_base_game_assets:
            return self._target_nif_paths
        target_asset_store = getattr(self, "target_asset_store", None)
        if target_asset_store is not None:
            self._target_nif_paths = {
                path.removeprefix("meshes/")
                for path in target_asset_store.list_assets(
                    prefix="meshes/", suffix=".nif"
                )
            }
            return self._target_nif_paths
        meshes_root = os.path.join(self.target_extracted_dir or "", "Meshes")
        if not self.target_extracted_dir or not os.path.isdir(meshes_root):
            _log.warning(
                "No target extracted Meshes dir for %s; NIF base-game dedup skipped",
                self.target_game,
            )
            return self._target_nif_paths
        paths: set[str] = set()
        for dirpath, _dirnames, filenames in os.walk(meshes_root):
            for fn in filenames:
                if fn.lower().endswith(".nif"):
                    rel = os.path.relpath(os.path.join(dirpath, fn), meshes_root)
                    paths.add(rel.replace("\\", "/").lower())
        self._target_nif_paths = paths
        return self._target_nif_paths

    def _target_has_asset(self, asset: AssetRef) -> bool:
        """Check if the target game already has this asset at the same relative path."""
        if not self.use_base_game_assets:
            return False

        target_asset_index = getattr(self, "target_asset_index", None)
        if target_asset_index is not None and target_asset_index.has_asset(asset):
            return True

        source_path_lower = asset.source_path.lower().replace("\\", "/")
        # Assets expanded from animation_dir arrive with a ``Meshes/`` prefix
        # (relative to the extracted root), but the FO4 asset DBs store paths
        # without it. Strip so base-game lookups hit.
        if source_path_lower.startswith("meshes/"):
            source_path_lower = source_path_lower[len("meshes/") :]

        if asset.asset_type == "nif":
            if target_asset_index is not None:
                return False
            return source_path_lower in self._load_target_nif_paths()

        elif asset.asset_type in ("animation", "animation_dir", "behavior"):
            if target_asset_index is not None:
                return False
            # All .hkx assets dedup against target catalog membership
            # (animations + skeleton + behaviors), not a havok DB. The walker
            # sometimes classifies animation files (e.g. Actors/X/Animations/*.hkx
            # picked up via a subgraph AnimationPaths entry) as "behavior", so
            # the shared base-game .hkx skip-set covers every incoming type.
            return source_path_lower in self._load_target_behavior_paths()

        elif asset.asset_type in ("texture", "material", "sound"):
            if asset.asset_type == "texture" and self._is_known_target_texture_ref(
                asset.source_path
            ):
                return True
            if asset.asset_type == "texture" and self._target_has_fo76_texture_bundle(
                asset
            ):
                return True
            if self.target_extracted_dir:
                # Record-field paths for textures/materials/sounds are stored
                # without their top-level Data subdir prefix (e.g. the walker
                # gives "Actors/Molerat/broodmother_d.dds" for a texture, not
                # "Textures/Actors/Molerat/..."). Normalize via
                # _asset_data_subpath so the filesystem check lands on the
                # right Textures/Materials/Sound folder. NTFS is case-
                # insensitive so `broodmother_d.dds` matches `broodmother_d.DDS`.
                subpath = self._asset_data_subpath(asset)
                full_path = os.path.join(self.target_extracted_dir, subpath)
                return os.path.isfile(full_path)

        return False

    _FO76_BUNDLE_SUFFIXES: tuple[str, ...] = (
        "_diffuse",
        "_albedo",
        "_color",
        "_normal",
        "_roughness",
        "_rough",
        "_lighting",
        "_flow",
        "_d",
        "_n",
        "_r",
        "_l",
        "_f",
    )

    def _fo76_texture_bundle_stem(self, asset: AssetRef) -> tuple[str, str, str] | None:
        """Return (relative_dir, base_stem, extension) for FO76 shader bundles.

        FO76 weapon/material bundles commonly ship as ``_d/_n/_r/_l`` and may
        also carry a ``FlowTexture`` reference (``_f``). During FO76 -> FO4
        conversion we want to treat any of these files as members of the same
        bundle so we can reuse an existing FO4 DLC/base texture set when the
        target already provides ``_d/_n/_s``.
        """
        if (
            asset.asset_type != "texture"
            or self.source_game != "fo76"
            or self.target_game != "fo4"
        ):
            return None

        rel_path = asset.source_path.replace("\\", "/")
        if rel_path.lower().startswith("textures/"):
            rel_path = rel_path[len("textures/") :]
        rel_dir = os.path.dirname(rel_path)
        stem, ext = os.path.splitext(os.path.basename(rel_path))
        stem_lower = stem.lower()

        for suffix in self._FO76_BUNDLE_SUFFIXES:
            if stem_lower.endswith(suffix):
                return rel_dir, stem[: -len(suffix)], ext
        return None

    def _target_has_fo76_texture_bundle(self, asset: AssetRef) -> bool:
        """Check whether the target already provides the FO4 texture set.

        This covers installed DLC assets because the store filters the packaged
        catalog by the official archives present in FO4 Data.
        """
        bundle = self._fo76_texture_bundle_stem(asset)
        if bundle is None:
            return False

        rel_dir, stem, ext = bundle
        target_asset_store = getattr(self, "target_asset_store", None)
        for suffix in ("_d", "_n", "_s"):
            target_name = f"{stem}{suffix}{ext}"
            subpath = (
                os.path.join("Textures", rel_dir, target_name)
                if rel_dir
                else os.path.join("Textures", target_name)
            )
            if target_asset_store is not None:
                if not target_asset_store.has_asset(subpath):
                    return False
                continue
            if not self.target_extracted_dir:
                return False
            full_path = os.path.join(self.target_extracted_dir, subpath)
            if not os.path.isfile(full_path):
                return False
        return True

    def _asset_already_in_mod(self, asset: AssetRef) -> bool:
        """Check if an asset file already exists in the mod output directory."""
        out_path = self._asset_output_path(asset)
        return os.path.isfile(out_path)

    def _remove_stale_asset_output(self, asset: AssetRef) -> bool:
        out_path = self._asset_output_path(asset)
        if not os.path.isfile(out_path):
            return False
        os.remove(out_path)
        return True

    def _track_asset(self, asset: AssetRef, strategy: str, reason: str = "") -> None:
        """Record an asset dedup decision in the asset map."""
        key = asset.source_path.replace("\\", "/")
        entry = {"strategy": strategy, "source_path": key}
        if reason:
            entry["reason"] = reason
        self._asset_map[key] = entry

    def _expand_animation_dirs(self, extracted_dir: str | None) -> list[AssetRef]:
        """Expand animation_dir assets into individual animation AssetRefs.

        Skips generic/fallback animation directories (e.g., Animations/,
        Paired/, Common/) to avoid pulling in hundreds of shared animations
        that don't belong to the weapon being converted. Native dependency
        walking handles keyword/subgraph discovery; this expansion is for
        creature/race-specific dirs that come from Race Subgraph data.
        """
        from bacup_lib.animation.lookup import _is_fallback_path

        expanded: list[AssetRef] = []
        for asset in self.graph.all_assets:
            if asset.asset_type != "animation_dir":
                continue
            if not extracted_dir:
                continue
            # Skip generic fallback animation directories
            if not _is_fallback_path(asset.source_path):
                continue
            # Try to find the directory on disk (case-insensitive search)
            dir_path = os.path.join(extracted_dir, asset.source_path)
            if not os.path.isdir(dir_path):
                # Try with Meshes prefix
                dir_path = os.path.join(extracted_dir, "Meshes", asset.source_path)
            if not os.path.isdir(dir_path):
                continue
            for hkx in glob_mod.glob(
                os.path.join(dir_path, "**", "*.hkx"), recursive=True
            ):
                rel = os.path.relpath(hkx, extracted_dir).replace("\\", "/")
                expanded.append(AssetRef("animation", rel, resolved_path=hkx))
        return expanded

    _WILDLIFE_FACTION_FK = "022B31:Fallout4.esm"

    _SNDR_RECORD_TYPES = {
        "SNDR",
        "SoundDescriptor",
        "SoundDescriptors",
        "SoundDescriptorCompound",
    }

    _TEMPLATE_FLAG_BITS: dict[str, int] = {
        "TraitTemplate": 1 << 0,  # 1
        "StatsTemplate": 1 << 1,  # 2
        "FactionsTemplate": 1 << 2,  # 4
        "SpellListTemplate": 1 << 3,  # 8
        "AiDataTemplate": 1 << 4,  # 16
        "AiPackagesTemplate": 1 << 5,  # 32
        "BaseDataTemplate": 1 << 7,  # 128
        "InventoryTemplate": 1 << 8,  # 256
        "ScriptTemplate": 1 << 9,  # 512
        "DefPackListTemplate": 1 << 10,  # 1024
        "AttackDataTemplate": 1 << 11,  # 2048
        "KeywordsTemplate": 1 << 12,  # 4096
    }

    _CANONICAL_TEMPLATE_FLAG_NAMES: dict[str, str] = {
        "Traits": "Traits",
        "Stats": "Stats",
        "Factions": "Factions",
        "AIData": "AIData",
        "AIPackages": "AIPackages",
        "BaseData": "BaseData",
        "Inventory": "Inventory",
        "Script": "Script",
    }

    _CANONICAL_TEMPLATE_FLAG_EXCLUDE: set[str] = {"Traits", "Stats"}

    _CREATURE_DEFAULT_ACTOR_VALUES: dict[str, float] = {
        "0002C2:Fallout4.esm": 0,  # Strength
        "0002C3:Fallout4.esm": 0,  # Perception (source may override)
        "0002C4:Fallout4.esm": 0,  # Endurance
        "0002C5:Fallout4.esm": 0,  # Charisma
        "0002C6:Fallout4.esm": 0,  # Intelligence
        "0002C7:Fallout4.esm": 0,  # Agility
        "0002C8:Fallout4.esm": 0,  # Luck
        "0002D4:Fallout4.esm": 500,  # Health
        "0002D5:Fallout4.esm": 50,  # ActionPoints
        "0002DA:Fallout4.esm": 100,  # SpeedMult
        "0002DF:Fallout4.esm": 55,  # UnarmedDamage
        "0002E3:Fallout4.esm": 60,  # DamageResist
        "0002E4:Fallout4.esm": 100,  # PoisonResist
        "0002EB:Fallout4.esm": 100,  # EnergyResist
    }

    _ORPHAN_PRUNABLE_TYPES: frozenset[str] = frozenset(
        {
            "LeveledItems",
            "LVLI",
            "Ammunitions",
            "AMMO",
            "Ingestibles",
            "ALCH",
        }
    )

    _FO4_ADDITIVE_PARENTS: dict[str, str] = {
        "HumanRace": "166729:Fallout4.esm",  # HumanRaceSubGraphData
        "PowerArmorRace": "01D31E:Fallout4.esm",  # PowerArmorRace itself
    }

    _RACE_EID_NORMALIZE: dict[str, str] = {
        "HumanRaceSubGraphData": "HumanRace",
    }

    _SUBGRAPH_DATA_LABELS: frozenset[str] = frozenset(
        {"BehaviourGraph", "Path", "SAKD", "STKD", "SRAF"}
    )

    _DN_COMMON_GUN_FO4_FK = "2377CF:Fallout4.esm"

    _FO4_MELEE_SOUND_DEFAULTS: dict[str, str] = {
        "AttackSound": "094307:Fallout4.esm",  # WPNSwingBaseballBat
        "EquipSound": "2498AE:Fallout4.esm",  # WPNGenericMeleeLargeEquipUp
        "UnequipSound": "1526AC:Fallout4.esm",  # WPNEquipDown
    }

    _FK_RE = re.compile(r"[0-9A-Fa-f]{2,6}:.+\.es[mpl]")

    _REMOVE_SENTINEL = object()

    _MATERIAL_TEXTURE_FIELDS: tuple[str, ...] = (
        "DiffuseTexture",
        "NormalTexture",
        "SmoothSpecTexture",
        "GreyscaleTexture",
        "GlowTexture",
        "WrinklesTexture",
        "EnvmapTexture",
        "InnerLayerTexture",
        "DisplacementTexture",
        "SpecularTexture",
        "LightingTexture",
        "FlowTexture",
        "DistanceFieldAlphaTexture",
        "BaseTexture",
        "GrayscaleTexture",
        "EnvmapMaskTexture",
        "GlassRoughnessScratch",
        "GlassDirtOverlay",
    )

    def _prefixed_texture_reference(
        self,
        texture_path: object,
        owner_asset: AssetRef | None = None,
    ) -> object:
        if not isinstance(texture_path, str):
            return texture_path
        clean = texture_path.rstrip("\x00").strip()
        if not clean:
            return texture_path

        rel_path = clean.replace("\\", "/").lstrip("/")
        data_idx = rel_path.lower().rfind("/data/")
        if data_idx != -1:
            rel_path = rel_path[data_idx + 6 :]
        if rel_path.lower().startswith("data/"):
            rel_path = rel_path[5:]
        lookup_path = rel_path
        if not lookup_path.lower().startswith("textures/"):
            lookup_path = f"Textures/{lookup_path}"

        def material_slot_path(data_relative_path: str) -> str:
            normalized = data_relative_path.replace("\\", "/").lstrip("/")
            if normalized.lower().startswith("textures/"):
                normalized = normalized.split("/", 1)[1]
            return normalized

        source_profile = getattr(self, "_source_profile", None)
        if source_profile is None:
            return material_slot_path(lookup_path)

        from bacup_lib.paths import apply_asset_prefix

        normalized_lookup_path = apply_asset_prefix(lookup_path, source_profile)
        return material_slot_path(normalized_lookup_path)

    def _prefix_material_texture_references(
        self,
        mat,
        owner_asset: AssetRef | None = None,
    ) -> int:
        updated_count = 0
        for field_name in self._MATERIAL_TEXTURE_FIELDS:
            old_value = getattr(mat, field_name, None)
            new_value = self._prefixed_texture_reference(old_value, owner_asset)
            if new_value == old_value:
                continue
            setattr(mat, field_name, new_value)
            updated_count += 1
        return updated_count

    def _prefix_bgsm_file_texture_references(
        self,
        bgsm_path: os.PathLike[str] | str,
        owner_asset: AssetRef | None = None,
    ) -> None:
        from creation_lib.material_tools.bgsm_bin import read_bgsm

        with open(bgsm_path, "rb") as fh:
            bgsm = read_bgsm(fh)
        if not self._prefix_material_texture_references(bgsm, owner_asset):
            return

        import io

        buf = io.BytesIO()
        bgsm.write(buf)
        with open(bgsm_path, "wb") as fh:
            fh.write(buf.getvalue())

    def _convert_single_nif(
        self,
        asset: AssetRef,
        runner: ConversionRunner | None,
        *,
        bgsm_output_dir: str | None = None,
        emit_events: bool = True,
        progress_label: str | None = None,
    ) -> AssetConversionWorkerResult:
        """Convert a single NIF file."""
        worker_result = AssetConversionWorkerResult()

        def emit_log(level: str, message: str) -> None:
            worker_result.logs.append((level, message))
            if emit_events and runner is not None:
                runner.emit_log(level, message)

        def append_log_line(line: str) -> None:
            worker_result.log_lines.append(line)
            if emit_events:
                self._log_lines.append(line)

        if bgsm_output_dir is None:
            bgsm_output_dir = os.path.join(
                self.mod_path,
                "data",
                self._asset_output_root_subpath("Materials"),
            )
        out_path = self._asset_output_path(asset)
        os.makedirs(os.path.dirname(out_path), exist_ok=True)

        bgsm_default_overrides = (getattr(self, "_material_overrides", {}) or {}).get(
            "bgsm_default"
        ) or {}

        def process_emitted_bgsms(emitted_bgsms: list[str]) -> None:
            worker_result.emitted_bgsms.extend(emitted_bgsms)
            for bgsm_path in emitted_bgsms:
                self._prefix_bgsm_file_texture_references(bgsm_path, asset)
            if not bgsm_default_overrides or not emitted_bgsms:
                return

            try:
                apply_material_overrides(emitted_bgsms, bgsm_default_overrides)
            except Exception as e:
                emit_log(
                    "WARN",
                    f"Material override failed for NIF-emitted BGSMs from {asset.source_path}: {e}",
                )
                append_log_line(
                    f"[WARN] Material override failed: {asset.source_path}: {e}"
                )

        try:
            from creation_lib.nif import native_runtime as nif_native_runtime
            from bacup_lib.weapon_attachment_surgery import is_fnv_source

            options = {
                "source_path": asset.source_path,
                "progress_label": progress_label,
                "asset_prefix": getattr(
                    getattr(self, "_source_profile", None),
                    "asset_prefix",
                    self.source_game,
                ),
                "addon_index_map": dict(self._addon_index_map),
            }
            material_namespace = _nif_material_namespace(self, asset)
            if material_namespace:
                options["material_namespace"] = material_namespace
            options.update(resolve_nif_conversion_options(self))
            if is_fnv_source(getattr(self, "_source_profile", None)):
                weapon_role = _weapon_role_for_asset(self, asset.source_path)
                if weapon_role is not None:
                    options["weapon_role"] = weapon_role

            native_report = nif_native_runtime.convert_nif_file_raw(
                asset.resolved_path,
                out_path,
                self.source_game,
                self.target_game,
                bgsm_output_dir,
                options,
            )
        except Exception as e:
            emit_log(
                "ERROR", f"  NIF: native conversion failed for {asset.source_path}: {e}"
            )
            append_log_line(
                f"[ERROR] NIF native conversion failed: {asset.source_path}: {e}"
            )
            raise

        if not native_report.get("supported"):
            errors = native_report.get("errors", []) or [
                f"native NIF conversion does not support {self.source_game} -> {self.target_game}"
            ]
            for error in errors:
                emit_log("ERROR", f"  NIF: {error}")
                append_log_line(f"[ERROR] NIF: {asset.source_path}: {error}")
            raise RuntimeError("; ".join(str(error) for error in errors))

        process_emitted_bgsms(
            [str(path) for path in native_report.get("emitted_bgsms", []) or []]
        )
        for change in native_report.get("changes", []) or []:
            emit_log("INFO", f"  NIF: {change}")
        for warn in native_report.get("warnings", []) or []:
            if _is_havok_or_collision_warning(str(warn)):
                emit_log(
                    "WARN",
                    f"  NIF Havok/collision warning: {asset.source_path}: {warn}",
                )
            else:
                emit_log("WARN", f"  NIF warning: {asset.source_path}: {warn}")
            append_log_line(f"[WARN] NIF: {asset.source_path}: {warn}")
        return worker_result

    def _convert_texture_asset(
        self,
        asset: AssetRef,
        runner: ConversionRunner | None,
        *,
        emit_events: bool = True,
    ) -> AssetConversionWorkerResult:
        """Convert a single texture file."""
        worker_result = AssetConversionWorkerResult()

        def emit_log(level: str, message: str) -> None:
            worker_result.logs.append((level, message))
            if emit_events and runner is not None:
                runner.emit_log(level, message)

        if not self._source_profile or not self._target_profile:
            self._copy_asset_as_is(asset)
            return worker_result

        from creation_lib.textures.naming import (
            convert_texture_name,
            detect_texture_role,
        )
        from bacup_lib.texture.native import convert_texture_paths

        role = detect_texture_role(asset.source_path, self._source_profile)

        if self._source_profile.id == "fo76" and self._target_profile.id == "fo4":
            if self._convert_fo76_texture_bundle(asset, role):
                return worker_result

        if role is None:
            self._copy_asset_as_is(asset)
            emit_log(
                "WARN", f"Unknown texture role: {asset.source_path} -- copied as-is"
            )
            return worker_result

        out_dir = os.path.join(
            self.mod_path,
            "data",
            os.path.dirname(self._asset_output_subpath(asset)),
        )
        os.makedirs(out_dir, exist_ok=True)

        result = convert_texture_paths(
            [(Path(asset.resolved_path), role)],
            Path(out_dir),
            self._source_profile,
            self._target_profile,
        )
        if not result.get("converted"):
            self._copy_asset_as_is(asset)
            emit_log(
                "WARN",
                f"No converted texture outputs for {asset.source_path} -- copied as-is",
            )
            return worker_result

        expected_name = convert_texture_name(
            os.path.basename(asset.source_path),
            self._source_profile,
            self._target_profile,
        )
        expected_path = os.path.join(out_dir, expected_name)
        for item in result.get("converted", []):
            converted_path = item.get("path")
            if (
                converted_path
                and os.path.abspath(converted_path) != os.path.abspath(expected_path)
                and os.path.isfile(converted_path)
            ):
                os.replace(converted_path, expected_path)

        return worker_result

    def _convert_fo76_texture_bundle(self, asset: AssetRef, role: str | None) -> bool:
        """Handle FO76 shader texture bundles using the reference d/r/l flow."""
        if role not in {"diffuse", "reflectivity", "lighting"}:
            return False

        diffuse_asset = (
            asset
            if role == "diffuse"
            else self._find_fo76_texture_sibling(asset, "diffuse")
        )
        if (
            diffuse_asset is None
            or not diffuse_asset.resolved_path
            or not os.path.isfile(diffuse_asset.resolved_path)
        ):
            if role == "lighting":
                reflectivity_asset = self._find_fo76_texture_sibling(
                    asset, "reflectivity"
                )
                if (
                    reflectivity_asset is not None
                    and reflectivity_asset.resolved_path
                    and os.path.isfile(reflectivity_asset.resolved_path)
                ):
                    return True
                return False

            if role != "reflectivity":
                return False

            lighting_asset = self._find_fo76_texture_sibling(asset, "lighting")
            if (
                not asset.resolved_path
                or lighting_asset is None
                or not lighting_asset.resolved_path
                or not os.path.isfile(asset.resolved_path)
                or not os.path.isfile(lighting_asset.resolved_path)
            ):
                return False

            from bacup_lib.texture.native import convert_texture_paths

            out_dir = os.path.join(
                self.mod_path,
                "data",
                os.path.dirname(self._asset_output_subpath(asset)),
            )
            os.makedirs(out_dir, exist_ok=True)

            result = convert_texture_paths(
                [
                    (Path(asset.resolved_path), "reflectivity"),
                    (Path(lighting_asset.resolved_path), "lighting"),
                    *self._fo76_glow_sibling_input(asset),
                ],
                Path(out_dir),
                self._source_profile,
                self._target_profile,
            )
            if not result.get("converted"):
                raise RuntimeError(
                    f"native FO76 specular emission remix failed for {asset.source_path}"
                )
            return True

        if role in {"reflectivity", "lighting"}:
            # The diffuse asset owns bundle emission; sibling visits are no-ops.
            return True

        reflectivity_asset = self._find_fo76_texture_sibling(
            diffuse_asset, "reflectivity"
        )
        lighting_asset = self._find_fo76_texture_sibling(diffuse_asset, "lighting")
        if reflectivity_asset is None or lighting_asset is None:
            return False
        if not reflectivity_asset.resolved_path or not lighting_asset.resolved_path:
            return False
        if not os.path.isfile(reflectivity_asset.resolved_path) or not os.path.isfile(
            lighting_asset.resolved_path
        ):
            return False

        from bacup_lib.texture.native import convert_texture_paths

        out_dir = os.path.join(
            self.mod_path,
            "data",
            os.path.dirname(self._asset_output_subpath(diffuse_asset)),
        )
        os.makedirs(out_dir, exist_ok=True)

        result = convert_texture_paths(
            [
                (Path(diffuse_asset.resolved_path), "diffuse"),
                (Path(reflectivity_asset.resolved_path), "reflectivity"),
                (Path(lighting_asset.resolved_path), "lighting"),
                *self._fo76_glow_sibling_input(diffuse_asset),
            ],
            Path(out_dir),
            self._source_profile,
            self._target_profile,
        )
        if not result.get("converted"):
            raise RuntimeError(
                f"native FO76 texture bundle remix failed for {diffuse_asset.source_path}"
            )
        return True

    def _fo76_glow_sibling_input(self, asset: AssetRef) -> list[tuple[Path, str]]:
        glow_asset = self._find_fo76_texture_sibling(asset, "glow")
        if (
            glow_asset is None
            or not glow_asset.resolved_path
            or not os.path.isfile(glow_asset.resolved_path)
        ):
            return []
        return [(Path(glow_asset.resolved_path), "glow")]

    def _find_fo76_texture_sibling(
        self, asset: AssetRef, target_role: str
    ) -> AssetRef | None:
        """Resolve a sibling FO76 texture by swapping the filename suffix."""
        if not self._source_profile:
            return None

        from creation_lib.textures.naming import detect_texture_role

        current_role = detect_texture_role(asset.source_path, self._source_profile)
        if current_role is None:
            return None

        current_suffix = self._source_profile.texture_suffixes.get(current_role)
        target_suffix = self._source_profile.texture_suffixes.get(target_role)
        if not current_suffix or not target_suffix:
            return None

        rel_path = asset.source_path.replace("\\", "/")
        rel_dir = os.path.dirname(rel_path)
        stem, ext = os.path.splitext(os.path.basename(rel_path))
        idx = stem.lower().rfind(current_suffix.lower())
        if idx < 0:
            return None
        sibling_name = stem[:idx] + target_suffix + ext
        sibling_rel = (
            os.path.join(rel_dir, sibling_name).replace("\\", "/")
            if rel_dir
            else sibling_name
        )
        sibling_rel_lower = sibling_rel.lower()

        for candidate in self.graph.all_assets:
            if candidate.asset_type != "texture":
                continue
            if candidate.source_path.replace("\\", "/").lower() == sibling_rel_lower:
                return candidate

        if asset.resolved_path:
            sibling_disk = os.path.join(
                os.path.dirname(asset.resolved_path), sibling_name
            )
            if os.path.isfile(sibling_disk):
                return AssetRef("texture", sibling_rel, sibling_disk)
        return None

    _HEURISTIC_CUBEMAPS: tuple[str, ...] = (
        "Shared/Cubemaps/mipblur_DefaultOutside1.dds",
        "Shared/Cubemaps/mipblur_DefaultOutside1_dielectric.dds",
        "Shared/Cubemaps/mipblur_DefaultOutside1_Copper.dds",
        "Shared/Cubemaps/mipblur_DefaultOutside1_bronze.dds",
        "Shared/Cubemaps/MetalChrome01Cube_e.dds",
        "Shared/Cubemaps/MetalCopperShine01Cube_e.dds",
        "Shared/Cubemaps/MetalBronzeCube_e.dds",
        "Shared/Cubemaps/MetalBrushedGold_e.dds",
        "Shared/Cubemaps/MetalBrushed01Cube_e.dds",
        "Shared/Cubemaps/EyeCubeMap.dds",
        "Shared/Cubemaps/Oil_e.dds",
    )

    def _is_known_target_texture_ref(self, texture_path: str) -> bool:
        """Return True for narrow FO4 base-game texture refs we preserve without extraction."""
        target_profile = getattr(self, "_target_profile", None)
        if target_profile is None or target_profile.id != "fo4":
            return False

        rel_path = texture_path.replace("\\", "/").lstrip("/")
        data_idx = rel_path.lower().rfind("/data/")
        if data_idx != -1:
            rel_path = rel_path[data_idx + 6 :]
        if rel_path.lower().startswith("data/"):
            rel_path = rel_path[5:]
        if rel_path.lower().startswith("textures/"):
            rel_path = rel_path[9:]

        return rel_path.lower() in {
            cubemap.lower() for cubemap in self._HEURISTIC_CUBEMAPS
        }

    def _register_heuristic_cubemap_assets(self) -> None:
        """Inject placeholder texture AssetRefs for the heuristic cubemap
        catalog into ``graph.all_assets``.

        These are FO4 base-game cubemaps that the FO76->FO4 BGSM/BGEM/NIF
        downgrade may reference. Adding them as ``texture`` AssetRefs lets
        ``_target_has_asset`` treat them as base-game assets so the BA2
        packer skips them â€” the FO4 install resolves the reference at
        runtime, no copy needed.

        Idempotent: skipped when source isn't FO76 or target isn't FO4 (no
        heuristic injection happens) and when an entry for the same path
        already exists in the graph.
        """
        if (
            not self._source_profile
            or not self._target_profile
            or self._source_profile.id != "fo76"
            or self._target_profile.id != "fo4"
        ):
            return
        existing = {
            a.source_path.replace("\\", "/").lower()
            for a in self.graph.all_assets
            if a.asset_type == "texture"
        }
        for cube in self._HEURISTIC_CUBEMAPS:
            key = cube.lower()
            if key in existing:
                continue
            # Cubemap textures live under Textures/Shared/Cubemaps in the
            # extracted FO4 install; the source_path is the data-relative
            # form without the Textures/ prefix (matches walker convention
            # for texture refs from material extractors).
            self.graph.all_assets.append(
                AssetRef(
                    asset_type="texture",
                    source_path=cube,
                    resolved_path=None,
                )
            )

    _MATERIAL_SOURCE_OVERRIDES: dict[str, str] | None = None

    @classmethod
    def _load_material_source_overrides(cls) -> dict[str, str]:
        if cls._MATERIAL_SOURCE_OVERRIDES is not None:
            return cls._MATERIAL_SOURCE_OVERRIDES

        cfg_path = os.path.join(
            os.path.dirname(os.path.dirname(__file__)),
            "record",
            "material_source_overrides.yaml",
        )
        overrides: dict[str, str] = {}
        if os.path.isfile(cfg_path):
            import yaml

            with open(cfg_path, encoding="utf-8") as f:
                raw = yaml.safe_load(f) or {}
            for key, value in raw.items():
                override_path = _normalize_material_source_override(value)
                if not override_path:
                    raise ValueError(
                        "Material source override values must be data-relative "
                        f"paths: {key!r}"
                    )
                overrides[str(key).lower().replace("\\", "/")] = override_path
        cls._MATERIAL_SOURCE_OVERRIDES = overrides
        return overrides

    def _convert_single_material(
        self, asset: AssetRef, runner: ConversionRunner
    ) -> None:
        """Convert a single BGSM/BGEM/.mat material.

        Dispatch:
            .bgsm  -> _convert_bgsm       (convert.downgrade_bgsm)
            .bgem  -> _convert_bgem       (convert.downgrade_bgem)
            .mat OR is_cdb_ref=True
                   -> _convert_mat_via_cdb
                      (MaterialsCDB.lookup_by_path -> cdb_to_bgsm ->
                       convert.downgrade_bgsm)

        On CDB translation failure (lookup miss, missing CDB file, or any
        translation exception) the material falls back to copy-as-is via
        the outer _phase_materials exception handler.
        """
        if not self._source_profile or not self._target_profile:
            self._copy_asset_as_is(asset)
            return

        # Apply material source path overrides before dispatch.
        overrides = self._load_material_source_overrides()
        sp_key = asset.source_path.lower().replace("\\", "/")
        if sp_key in overrides:
            override_value = overrides[sp_key]
            override_path = _resolve_material_source_override(
                asset.source_path,
                asset.resolved_path or "",
                override_value,
            )
            if override_path:
                runner.emit_log(
                    "INFO",
                    f"[Material] Source override: {asset.source_path} -> {override_path}",
                )
                asset.resolved_path = override_path
            else:
                runner.emit_log(
                    "WARN",
                    f"[Material] Override path not found, using original: {override_value}",
                )

        lower = asset.source_path.lower()
        if asset.is_cdb_ref or lower.endswith(".mat"):
            self._convert_mat_via_cdb(asset, runner)
            return
        if lower.endswith(".bgsm"):
            self._convert_bgsm(asset, runner)
        elif lower.endswith(".bgem"):
            self._convert_bgem(asset, runner)
        else:
            self._copy_asset_as_is(asset)
            runner.emit_log(
                "INFO", f"Material copied as-is (unknown format): {asset.source_path}"
            )

    def _get_materials_cdb(self, runner: ConversionRunner):
        """Return the cached FO76 MaterialsDB.cdb, loading on first use.

        Returns ``None`` if no CDB file is available -- callers should
        fall back to copy-as-is in that case. The cache slot holds a
        sentinel ``False`` after a failed load so we don't retry on
        every asset.
        """
        if self._materials_cdb is False:
            return None
        if self._materials_cdb is not None:
            return self._materials_cdb

        from creation_lib.material_tools.materials_cdb import MaterialsCDB

        # FO76's MaterialsDB.cdb lives under the source game's extracted
        # dir. We probe a few canonical subpaths; if none resolve we
        # stash the sentinel False and warn once.
        extracted_dirs: list[str] = []
        # The orchestrator doesn't carry an explicit source extracted dir,
        # but we can recover it from any resolved asset path: the shared
        # prefix before the first source-relative path segment is the
        # root. Fall back to an empty list when no assets are resolved.
        for a in self.graph.all_assets:
            if not a.resolved_path:
                continue
            rp = a.resolved_path.replace("\\", "/")
            sp = a.source_path.replace("\\", "/")
            idx = rp.lower().find(sp.lower())
            if idx > 0:
                extracted_dirs.append(rp[:idx].rstrip("/"))
                break

        candidates = []
        for root in extracted_dirs:
            candidates.extend(
                [
                    os.path.join(
                        root, "Data", "SeventySix - Materials.ba2", "MaterialsDB.cdb"
                    ),
                    os.path.join(root, "SeventySix - Materials.ba2", "MaterialsDB.cdb"),
                    os.path.join(root, "Data", "MaterialsDB.cdb"),
                    os.path.join(root, "MaterialsDB.cdb"),
                ]
            )

        for candidate in candidates:
            if os.path.isfile(candidate):
                try:
                    self._materials_cdb = MaterialsCDB.from_file(candidate)
                    runner.emit_log(
                        "INFO", f"[Material] Loaded FO76 MaterialsDB: {candidate}"
                    )
                    return self._materials_cdb
                except Exception as e:
                    runner.emit_log(
                        "WARN",
                        f"[Material] Failed to load MaterialsDB at {candidate}: {e}",
                    )
                    break

        # No CDB available -- cache the miss.
        self._materials_cdb = False
        return None

    def _convert_mat_via_cdb(self, asset: AssetRef, runner: ConversionRunner) -> None:
        """Translate a .mat / is_cdb_ref asset into a downgraded BGSM.

        Pipeline:
            1. Look up the source path in the cached FO76 MaterialsCDB.
            2. Flatten the resulting CE2Material via cdb_to_bgsm() at the
               source profile's BGSM version.
            3. Downgrade to the target profile's BGSM version via
               creation_lib.material_tools.convert.downgrade_bgsm (with source_path=
               so RootMaterialPath synthesis fires).
            4. Write the result via _write_material.

        On any failure (missing CDB, lookup miss, translation exception)
        raises RuntimeError so the outer _phase_materials exception
        handler falls back to copy-as-is with a WARN log entry.
        """
        from creation_lib.material_tools.cdb_to_bgsm import cdb_to_bgsm
        from creation_lib.material_tools.convert import downgrade_bgsm, BGSM_VERSION_FO4
        from creation_lib.material_tools import convert as mat_convert_mod

        cdb = self._get_materials_cdb(runner)
        if cdb is None:
            runner.emit_log(
                "WARN",
                f"[Material] No FO76 MaterialsDB available for .mat: {asset.source_path} -- copying as-is",
            )
            self._copy_asset_as_is(asset)
            return

        ce2 = cdb.lookup_by_path(asset.source_path)
        if ce2 is None:
            runner.emit_log(
                "WARN",
                f"[Material] CDB lookup miss for {asset.source_path} -- copying as-is",
            )
            self._copy_asset_as_is(asset)
            return

        try:
            # Flatten the CE2Material at the source profile's native BGSM
            # version (typically FO76 v22). cdb_to_bgsm does not downgrade;
            # the subsequent convert.downgrade_bgsm call below handles
            # that with the battle-tested texture-slot remap +
            # RootMaterialPath synthesis path.
            source_bgsm_version = getattr(self._source_profile, "bgsm_version", 22)
            remix_profile = getattr(self._source_profile, "texture_remix", None)
            if remix_profile is None:
                from creation_lib.core.game_profiles import RemixProfile

                remix_profile = RemixProfile()
            bgsm = cdb_to_bgsm(
                ce2, target_version=source_bgsm_version, remix_profile=remix_profile
            )

            # Downgrade to the target profile. When crossing FO76 -> FO4
            # the convert module re-routes texture slots, converts the
            # Translucency -> RimLighting block, and synthesizes a
            # RootMaterialPath from the source path + shader flags.
            if (
                self._target_profile.material_model == "spec-gloss"
                and bgsm.header.version > BGSM_VERSION_FO4
            ):
                # Re-resolve downgrade_bgsm through the module each call so
                # monkey-patching in tests works.
                bgsm = mat_convert_mod.downgrade_bgsm(
                    bgsm, BGSM_VERSION_FO4, source_path=asset.source_path
                )

            self._write_material(bgsm, asset)
            runner.emit_log(
                "INFO",
                f"[Material] Translated .mat via CDB: {asset.source_path}",
            )
        except Exception as e:
            runner.emit_log(
                "WARN",
                f"[Material] CDB translation failed for {asset.source_path}: {e} -- copying as-is",
            )
            self._copy_asset_as_is(asset)

    def _convert_bgsm(self, asset: AssetRef, runner: ConversionRunner) -> None:
        """Read BGSM, downgrade version for target game, rewrite texture paths, write output."""
        import io
        from creation_lib.material_tools.bgsm_bin import read_bgsm
        from creation_lib.material_tools.convert import downgrade_bgsm, BGSM_VERSION_FO4

        with open(asset.resolved_path, "rb") as f:
            bgsm = read_bgsm(f)

        src_version = bgsm.header.version
        changes = []

        # Downgrade version for spec-gloss targets (FO4/SkyrimSE use v2; FO76 uses v>2)
        if (
            self._target_profile.material_model == "spec-gloss"
            and bgsm.header.version > BGSM_VERSION_FO4
        ):
            bgsm = downgrade_bgsm(bgsm, BGSM_VERSION_FO4, source_path=asset.source_path)
            changes.append(
                f"Version downgraded: {src_version} -> {BGSM_VERSION_FO4} (FO4 format)"
            )

        # Rewrite texture paths
        try:
            from creation_lib.textures.naming import convert_texture_name
        except ImportError:
            self._write_material(bgsm, asset)
            return

        src = self._source_profile
        tgt = self._target_profile

        # All texture fields that may be populated at this point
        for field_name in (
            "DiffuseTexture",
            "NormalTexture",
            "SmoothSpecTexture",
            "GreyscaleTexture",
            "GlowTexture",
            "WrinklesTexture",
            "EnvmapTexture",
            "InnerLayerTexture",
            "DisplacementTexture",
            "SpecularTexture",
            "LightingTexture",
            "FlowTexture",
            "DistanceFieldAlphaTexture",
        ):
            old_val = getattr(bgsm, field_name, None)
            if not old_val:
                continue
            filename = old_val.replace("\\", "/").split("/")[-1]
            new_name = convert_texture_name(filename, src, tgt)
            if new_name != filename:
                new_path = old_val[: old_val.rfind(filename)] + new_name
                setattr(bgsm, field_name, new_path)
                changes.append(f"{field_name}: {filename} -> {new_name}")

        self._write_material(bgsm, asset)
        for c in changes:
            runner.emit_log("INFO", f"  BGSM: {c}")

    def _convert_bgem(self, asset: AssetRef, runner: ConversionRunner) -> None:
        """Read BGEM, downgrade version for target game, rewrite texture paths, write output."""
        import io
        from creation_lib.material_tools.bgem_bin import read_bgem
        from creation_lib.material_tools.convert import downgrade_bgem, BGEM_VERSION_FO4

        with open(asset.resolved_path, "rb") as f:
            bgem = read_bgem(f)

        src_version = bgem.header.version
        changes = []

        # Downgrade version for spec-gloss targets (FO4/SkyrimSE use v20; FO76 adds Glass fields at v>=21)
        if (
            self._target_profile.material_model == "spec-gloss"
            and bgem.header.version > BGEM_VERSION_FO4
        ):
            bgem = downgrade_bgem(bgem, BGEM_VERSION_FO4, source_path=asset.source_path)
            changes.append(
                f"Version downgraded: {src_version} -> {BGEM_VERSION_FO4} (FO4 format)"
            )

        # Rewrite texture paths
        try:
            from creation_lib.textures.naming import convert_texture_name
        except ImportError:
            self._write_material(bgem, asset)
            return

        src = self._source_profile
        tgt = self._target_profile

        for field_name in (
            "BaseTexture",
            "GrayscaleTexture",
            "EnvmapTexture",
            "NormalTexture",
            "EnvmapMaskTexture",
            "SpecularTexture",
            "LightingTexture",
            "GlowTexture",
        ):
            old_val = getattr(bgem, field_name, None)
            if not old_val:
                continue
            filename = old_val.replace("\\", "/").split("/")[-1]
            new_name = convert_texture_name(filename, src, tgt)
            if new_name != filename:
                new_path = old_val[: old_val.rfind(filename)] + new_name
                setattr(bgem, field_name, new_path)
                changes.append(f"{field_name}: {filename} -> {new_name}")

        self._write_material(bgem, asset)
        for c in changes:
            runner.emit_log("INFO", f"  BGEM: {c}")

    def _write_material(self, mat, asset: AssetRef) -> None:
        """Serialize a BGSMData or BGEMData to the output mod path."""
        import io

        self._prefix_material_texture_references(mat, asset)
        out_path = self._asset_output_path(asset)
        os.makedirs(os.path.dirname(out_path), exist_ok=True)
        buf = io.BytesIO()
        mat.write(buf)
        with open(out_path, "wb") as f:
            f.write(buf.getvalue())

    def _strip_source_game_events(self, runner: ConversionRunner) -> None:
        """Strip FO76-only annotation events from converted animations.

        Walks the mod's meshes/ output tree, finds every .hkx under an
        animations/ directory, and passes it through
        :func:`_strip_source_game_events_from_hkx`.  Behavior .hkx files are
        NOT touched â€” behavior eventName index references are index-sensitive
        and the CK crash is driven by animation-track annotations.  Log is
        rate-limited to one DROP/RENAME line per distinct event text to
        avoid flooding the conversion log.
        """
        data_dir = os.path.join(self.mod_path, "data", "meshes")
        if not os.path.isdir(data_dir):
            return
        total_files = 0
        modified_files = 0
        total_dropped = 0
        total_renamed = 0
        seen_warnings: set[str] = set()
        for root, _dirs, files in os.walk(data_dir):
            root_lower = root.replace("\\", "/").lower()
            if "/animations" not in root_lower:
                continue
            for fname in files:
                if not fname.lower().endswith(".hkx"):
                    continue
                fpath = os.path.join(root, fname)
                total_files += 1
                dropped, renamed, warnings = _strip_source_game_events_from_hkx(
                    fpath,
                    self.source_game,
                    self.target_game,
                )
                if dropped or renamed:
                    modified_files += 1
                    total_dropped += dropped
                    total_renamed += renamed
                    rel = os.path.relpath(fpath, self.mod_path)
                    runner.emit_log(
                        "INFO",
                        f"[event-strip] {rel}: dropped={dropped} renamed={renamed}",
                    )
                # Rate-limit warnings: one per distinct message
                for w in warnings:
                    if w not in seen_warnings:
                        seen_warnings.add(w)
                        runner.emit_log("INFO", f"[event-strip] {w}")
        if modified_files:
            runner.emit_log(
                "INFO",
                f"[event-strip] Filtered {modified_files}/{total_files} anim(s): "
                f"{total_dropped} dropped, {total_renamed} renamed",
            )

    def _inject_hitframe_events(self, runner: ConversionRunner) -> None:
        """Inject missing HitFrame events into converted attack animations."""
        data_dir = os.path.join(self.mod_path, "data", "meshes")
        if not os.path.isdir(data_dir):
            return
        for root, _dirs, files in os.walk(data_dir):
            root_lower = root.replace("\\", "/").lower()
            if "/animations" not in root_lower:
                continue
            # Check if any attack*.hkx files exist
            has_attacks = any(
                f.lower().startswith("attack") and f.lower().endswith(".hkx")
                for f in files
            )
            if not has_attacks:
                continue
            patched = _inject_hitframe_events(root)
            if patched:
                runner.emit_log(
                    "INFO",
                    f"Injected HitFrame into {len(patched)} attack animation(s) in "
                    f"{os.path.relpath(root, self.mod_path)}: {', '.join(patched)}",
                )

    def _inject_character_animation_names(self, runner: ConversionRunner) -> None:
        """Find converted character.hkx files and inject animation names."""
        data_dir = os.path.join(self.mod_path, "data", "meshes")
        if not os.path.isdir(data_dir):
            return
        # Walk the output looking for */characters/*.hkx files
        for root, _dirs, files in os.walk(data_dir):
            root_lower = root.replace("\\", "/").lower()
            if "/characters" not in root_lower:
                continue
            for fname in files:
                if not fname.lower().endswith(".hkx"):
                    continue
                char_path = os.path.join(root, fname)
                # Find the actor root directory (parent of characters/)
                # e.g. .../meshes/Actors/Snallygaster/characters/ -> .../meshes/Actors/Snallygaster/
                parts = root.replace("\\", "/").split("/")
                try:
                    char_idx = next(
                        i
                        for i in range(len(parts) - 1, -1, -1)
                        if parts[i].lower() == "characters"
                    )
                except StopIteration:
                    continue
                actor_root = "/".join(parts[:char_idx])

                # Rewrite FO76-style rigName regardless of whether this
                # character has animations alongside â€” weapon-FX characters
                # (MeltdownFX, MinigunFX, etc.) have no Animations/ dir but
                # still need their skeleton path corrected.
                rewritten = _fix_character_rig_path_fo4(char_path)
                if rewritten is not None:
                    runner.emit_log(
                        "INFO",
                        f"Rewrote FO76 rigName -> {rewritten} in "
                        f"{os.path.relpath(char_path, self.mod_path)}",
                    )

                anim_dir = os.path.join(actor_root, "animations")
                if not os.path.isdir(anim_dir):
                    # Try capitalized variant
                    anim_dir = os.path.join(actor_root, "Animations")
                if not os.path.isdir(anim_dir):
                    continue
                # Collect behavior-referenced clip names
                behavior_clip_names: set[str] | None = None
                for bdir_name in ("behaviors", "Behaviors"):
                    bdir = os.path.join(actor_root, bdir_name)
                    if os.path.isdir(bdir):
                        behavior_clip_names = _collect_behavior_clip_names(bdir)
                        if behavior_clip_names:
                            runner.emit_log(
                                "INFO",
                                f"Found {len(behavior_clip_names)} behavior-referenced animations "
                                f"in {os.path.relpath(bdir, self.mod_path)}",
                            )
                        break

                count = _inject_animation_names_into_character_hkx(
                    char_path,
                    anim_dir,
                    behavior_clip_names,
                )
                if count > 0:
                    runner.emit_log(
                        "INFO",
                        f"Injected {count} animation names into {os.path.relpath(char_path, self.mod_path)}",
                    )

                # Filter out unreferenced behavior files
                for bdir_name in ("behaviors", "Behaviors"):
                    bdir = os.path.join(actor_root, bdir_name)
                    if os.path.isdir(bdir):
                        removed = _filter_unreferenced_behaviors(bdir)
                        if removed:
                            runner.emit_log(
                                "INFO",
                                f"Removed {len(removed)} unreferenced behavior file(s): "
                                f"{', '.join(removed)}",
                            )
                        break

    def _asset_data_subpath(self, asset: AssetRef) -> str:
        """Return source_path with the correct game Data subdirectory prefix.

        Bethesda record fields store paths relative to their asset directory:
        - Model.File â†’ relative to Meshes/ (no prefix in record)
        - Texture paths in NIFs/BGSMs â†’ already prefixed with textures/
        - Material paths in NIFs â†’ already prefixed with materials/

        This helper ensures the right prefix is present for each type so
        files land under data/Meshes/, data/Textures/, etc.
        """
        path = asset.source_path.replace("\\", "/")
        lower = path.lower()

        if asset.asset_type in ("nif", "behavior", "animation", "support"):
            if not lower.startswith("meshes/"):
                path = "meshes/" + path
        elif asset.asset_type == "texture":
            if not lower.startswith("textures/"):
                path = "textures/" + path
        elif asset.asset_type == "material":
            if not lower.startswith("materials/"):
                path = "materials/" + path
        elif asset.asset_type in ("sound", "audio"):
            if not lower.startswith("sound/"):
                path = "sound/" + path

        return path

    def _asset_output_root_subpath(self, root: str) -> str:
        """Return a Data root normalized for converted asset output."""
        subpath = root.replace("\\", "/").strip().strip("/")
        source_profile = getattr(self, "_source_profile", None)
        if source_profile is None:
            return subpath
        from bacup_lib.paths import apply_asset_prefix

        return apply_asset_prefix(subpath, source_profile)

    def _asset_output_subpath(self, asset: AssetRef) -> str:
        """Return the Data-relative output path for a converted/copied asset."""
        explicit = getattr(asset, "output_subpath", None)
        if explicit:
            return str(explicit).replace("\\", os.sep).replace("/", os.sep)
        subpath = self._asset_data_subpath(asset)
        source_profile = getattr(self, "_source_profile", None)
        if source_profile is None:
            return subpath
        from bacup_lib.paths import apply_asset_prefix

        return apply_asset_prefix(subpath, source_profile)

    def _asset_output_path(self, asset: AssetRef) -> str:
        return os.path.join(self.mod_path, "data", self._asset_output_subpath(asset))

    def _copy_asset_as_is(self, asset: AssetRef) -> None:
        """Copy an asset file to the mod output without conversion."""
        if not asset.resolved_path:
            return
        out_path = self._asset_output_path(asset)
        os.makedirs(os.path.dirname(out_path), exist_ok=True)
        shutil.copy2(asset.resolved_path, out_path)

    def _write_log_file(self, runner: ConversionRunner) -> None:
        """Write conversion_log.txt to the diagnostics folder."""
        import datetime

        log_path = self._diagnostics_path("conversion_log.txt")
        mode = "a" if os.path.exists(log_path) else "w"

        # Write provenance files and collect ancestor summary
        ancestor_summary = self.graph.write_provenance_files(self._diagnostics_dir())

        with open(log_path, mode, encoding="utf-8") as f:
            f.write(f"\n{'=' * 60}\n")
            f.write(
                f"Conversion: {self.graph.root.editor_id} ({self.source_game} -> {self.target_game})\n"
            )
            f.write(f"Date: {datetime.datetime.now().isoformat()}\n")
            f.write(f"{'=' * 60}\n\n")

            for line in self._log_lines:
                f.write(line + "\n")

            f.write(f"\nSummary:\n")
            s = self._summary
            f.write(
                f"  Records:  {s.records_translated} translated ({s.records_warnings} warnings)\n"
            )
            f.write(f"  NIFs:     {s.nifs_converted}/{s.nifs_total} converted\n")
            f.write(f"  BTOs:     {s.btos_converted}/{s.btos_total} converted\n")
            f.write(
                f"  Textures: {s.textures_converted}/{s.textures_total} converted\n"
            )
            f.write(
                f"  Materials:{s.materials_converted}/{s.materials_total} converted\n"
            )
            f.write(f"  Havok:    {s.havok_converted}/{s.havok_total} converted\n")
            f.write(
                f"  Anims:    {s.animations_converted}/{s.animations_total} converted\n"
            )
            f.write(f"  Audio:    {s.audio_copied}/{s.audio_total} copied\n")
            f.write(f"  Scripts:  {s.scripts_flagged} flagged for manual porting\n")
            f.write(f"  ESP:      {'built' if s.esp_built else 'not built'}\n")
            f.write(f"\n")

            # Asset ancestor summary â€” flags over-greedy edges (>10 assets from
            # a record not directly related to the root WEAP).
            if ancestor_summary:
                f.write(
                    "Asset provenance summary (assets grouped by ancestor record):\n"
                )
                for label, count in sorted(
                    ancestor_summary.items(), key=lambda kv: -kv[1]
                ):
                    flag = "  [REVIEW]" if count > 10 else ""
                    f.write(f"  {count:4d} assets  via {label}{flag}\n")
                f.write(
                    "  (Records marked [REVIEW] contributed >10 assets - "
                    "investigate walker edge policy if unrelated to root weapon)\n"
                )
                f.write(f"\n")

    _LVLI_HINT_PATTERNS: list[tuple[str, str]] = [
        ("DeathItem", "inject into NPC_.DeathItem or use as LeveledNpc death loot"),
        (
            "VendorList",
            "inject into Merchant container via FactionVendorList or VendorChest NPC_",
        ),
        (
            "Vendor",
            "inject into Merchant container via FactionVendorList or VendorChest NPC_",
        ),
        ("MerchantList", "inject into Merchant container via FactionVendorList"),
        ("LootBag", "inject via Container (looting) or NPC_.DeathItem"),
        ("AmmoList", "inject into appropriate ammo LeveledList or vendor chest"),
        ("Ammo", "inject into appropriate ammo LeveledList or vendor chest"),
        ("WeaponList", "inject into weapon leveled list (e.g. LLI_Weapons_Tier*)"),
        ("WeaponPack", "inject into weapon leveled list (e.g. LLI_Weapons_Tier*)"),
        ("ArmorList", "inject into armor leveled list or vendor chest"),
        ("Armor", "inject into armor leveled list or vendor chest"),
        ("EncounterZone", "inject via EncounterZone loot table"),
        ("Spawn", "inject via EncounterZone or leveled NPC spawn list"),
        ("SubList", "sub-list â€” injected transitively via parent LeveledList"),
    ]

    def _lvli_injection_hint(self, editor_id: str) -> str:
        """Return a human-readable injection hint based on the EditorID."""
        lower = editor_id.lower()
        for keyword, hint in self._LVLI_HINT_PATTERNS:
            if keyword.lower() in lower:
                return hint
        return "candidate for injection â€” review EditorID and entries to determine target list"

    def _build_injection_notes_lines(self) -> list[str]:
        """Collect all LVLI records and build Injection Notes section lines."""
        lvli_entries: list[tuple[str, str]] = []  # (formkey, editor_id)

        for node in self.graph.all_records:
            if node.record_type != "LeveledItems":
                continue
            eid = node.editor_id
            if self._formkey_mapper and node.form_key in self._formkey_mapper.mappings:
                mapping = self._formkey_mapper.mappings[node.form_key]
                fk = mapping["new_formkey"]
                eid = str(mapping.get("editor_id") or eid)
            else:
                fk = node.form_key

            lvli_entries.append((fk, eid))

        if not lvli_entries:
            return []

        lines: list[str] = [
            "=== Injection Notes ===",
            "The converter does NOT auto-inject leveled lists into the base game.",
            "You must wire these LVLI records into the world yourself using one of:",
            "  - FO4Edit: find the target list and add an entry pointing to the LVLI FormKey",
            "  - A hand-written quest/script that calls AddItem or modifies a Container",
            "  - A patch .esp that edits the target NPC_, Container, or LeveledList record",
            "",
            f"Emitted LVLI records ({len(lvli_entries)}):",
        ]
        for fk, eid in sorted(lvli_entries, key=lambda t: t[1].lower()):
            hint = self._lvli_injection_hint(eid)
            lines.append(f"  {fk}  {eid}")
            lines.append(f"      -> {hint}")
        lines.append("")
        return lines

    def _write_conversion_report(self, runner: ConversionRunner) -> None:
        """Write conversion_report.txt and asset_map.json to diagnostics."""
        import datetime

        mod_name = os.path.basename(self.mod_path)
        s = self._summary

        lines = [
            f"Conversion Report: {mod_name} ({self.source_game} -> {self.target_game})",
            f"Date: {datetime.datetime.now().isoformat()}",
            "",
            "=== Records ===",
            f"Vanilla remapped:  {s.records_vanilla_remapped}  (matched by EditorID + type in base game)",
            f"New allocations:   {s.records_new_allocated}  (assigned to {self._output_plugin_name()})",
            f"Total:             {s.records_translated}",
            "",
        ]

        # Detail lines
        if self._formkey_mapper:
            for src_fk, m in self._formkey_mapper.mappings.items():
                tag = "REMAP" if m["strategy"] == "vanilla_remap" else "NEW"
                lines.append(
                    f"  [{tag}] {m['editor_id']} ({m['record_type']}) -> {m['new_formkey']}"
                )
        lines.append("")

        lines.extend(
            [
                "=== Assets ===",
                f"NIFs:       {s.nifs_converted} converted, {s.nifs_base_game_skipped} skipped (base game), {s.nifs_failed} failed",
                f"BTOs:       {s.btos_converted} converted, {s.btos_base_game_skipped} skipped (base game), {s.btos_failed} failed",
                f"Textures:   {s.textures_converted} converted, {s.textures_base_game_skipped} skipped, {s.textures_failed} failed",
                f"Materials:  {s.materials_converted} converted, {s.materials_base_game_skipped} skipped, {s.materials_failed} failed",
                f"Animations: {s.animations_converted} converted, {s.animations_base_game_skipped} skipped, {s.animations_failed} failed",
                f"Havok:      {s.havok_converted} converted, {s.havok_base_game_skipped} skipped, {s.havok_failed} failed",
                f"Audio:      {s.audio_copied} copied, {s.audio_base_game_skipped} skipped, {s.audio_failed} failed",
                "",
                "FormKey map: formkey_map.json",
                "Asset map:   asset_map.json",
                "",
            ]
        )

        lines.append("=== Conversion Decisions ===")
        by_kind: dict[str, int] = {}
        for decision in self._conversion_decisions:
            by_kind[decision.kind.value] = by_kind.get(decision.kind.value, 0) + 1
        for kind, count in sorted(by_kind.items()):
            lines.append(f"{kind}: {count}")
        lines.append("")

        lines.extend(self._build_injection_notes_lines())

        report_path = self._diagnostics_path("conversion_report.txt")
        with open(report_path, "w", encoding="utf-8") as f:
            f.write("\n".join(lines))

        runner.emit_log("INFO", f"Conversion report written to {report_path}")
        self._log_lines.append(f"[INFO] Report: {report_path}")

    def _write_asset_map(self) -> None:
        """Write asset_map.json to diagnostics."""
        if not self._asset_map:
            return
        map_path = self._diagnostics_path("asset_map.json")
        with open(map_path, "w", encoding="utf-8") as f:
            json.dump(self._asset_map, f, indent=2, ensure_ascii=False)
        _log.info("Saved asset_map.json: %d entries", len(self._asset_map))

    _SCHEMA_ORDER_CACHE: dict[str, dict[str, list[str]]] = {}

    @staticmethod
    def _record_type_signature(record_type: str) -> str:
        return record_type_signature(record_type)


