"""Unified FO76→FO4 regen driver: record track, concurrent asset waves, and
sink join (loose-tree reconcile + BA2 plan/finalize)."""

from __future__ import annotations

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
from pathlib import Path, PurePosixPath
from queue import Empty, Queue
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, Callable, Iterable, Mapping

from creation_lib.build.archive_plan import (
    DEFAULT_ARCHIVE_MAX_BYTES,
    ArchiveEntry,
    PlannedArchive,
    discover_mod_archives,
    estimate_archive_size,
    plan_archive_outputs,
)
from creation_lib.build.packer import (
    _inventory_data_entries,
    _inventory_root_strings_entries,
    _run_native_pack_entries,
    _run_native_pack_plans,
    _validate_archive_size,
)
from bacup_lib import papyrus_header_cache, pipeline
from bacup_lib.asset_paths import normalize_asset_source_path
from bacup_lib.base_asset_dedupe import (
    resolve_base_asset_namespace,
    resolve_base_asset_relocation_mesh_roots,
)
from bacup_lib.formkey.formkey_mapper import FormKeyMapper
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
from bacup_lib.native_maps import native_face_resources_dir, native_translation_maps_dir
from bacup_lib.native_runtime import load_native_module
from bacup_lib.record.translation_map_data import (
    load_translation_map_overrides,
)
from bacup_lib.runner import Drainer
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

_ANIM_TEXT_PROGRESS_HEARTBEAT_SECONDS = 15.0
_CREATE_NO_WINDOW = 0x08000000 if os.name == "nt" else 0
_PAPYRUS_CLASS_NAME_MAX_LENGTH = 38
_MVP_MELEE_TARGET_PROFILES = frozenset(
    {
        "unarmed",
        "power_fist_unarmed",
        "machete_one_hand",
        "grognak_two_hand",
        "ripper_continuous",
    }
)
_PSC_SCRIPT_NAME_RE = re.compile(
    r"^\s*ScriptName\s+([A-Za-z_][A-Za-z0-9_]*)\b", re.IGNORECASE | re.MULTILINE
)
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
    requirements = load_native_module().conversion_setup_required_masters(
        request.source_game, request.target_game, request.options.translate_records
    )
    for master_name, error_message in requirements:
        try:
            required_master = resolve_required_target_master_path(
                master_name,
                target_master_paths=paths,
                target_data_dir=request.target_data_dir,
                target_extracted_dir=None,
            )
        except FileNotFoundError as error:
            raise FileNotFoundError(error_message) from error
        known = {str(path.resolve()).casefold() for path in paths if path.is_file()}
        if str(required_master.resolve()).casefold() not in known:
            paths.append(required_master)
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
    pair = (request.source_game.lower(), request.target_game.lower())
    if pair != ("fo76", "fo4"):
        from bacup_lib.fullscreen_map import finalize_converted_world_maps

        exported = finalize_converted_world_maps(request, ctx)
        if exported:
            _safe_emit_log(
                runner,
                "INFO",
                "Wrote FullScreenMap packs for converted worldspaces: "
                + ", ".join(exported),
            )
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

    from bacup_lib.run.run_handle import ConversionRun

    try:
        with ConversionRun.create_new("fo76", "fo4", None, "SeventySix.esm") as run:
            report = run.run_phase(
                "finalize_fo76_maps",
                mod_path=str(ctx.mod_path),
                source_extracted_dir=str(source_root),
            )
            for event in run.drain_events():
                if event.get("kind") == "log":
                    _safe_emit_log(runner, str(event.get("level", "INFO")).upper(), str(event.get("message", "")))
    finally:
        _trim_bacup_native_allocator()
    if not report.get("assets_written"):
        return
    try:
        from bacup_lib.fullscreen_map_regions import export_region_labels

        count = export_region_labels(
            source_plugin=Path(request.source_plugins[0]),
            source_root=Path(source_root),
            mod_root=Path(ctx.mod_path),
        )
        _safe_emit_log(runner, "INFO", f"Wrote {count} major region labels for the Appalachia map")
    except Exception as exc:
        _safe_emit_log(runner, "WARN", f"FO76 map region labels skipped: {exc}")
    try:
        from bacup_lib.fullscreen_map_markers import export_marker_icons

        icons = export_marker_icons(
            source_data_dir=Path(source_root),
            source_plugin=Path(request.source_plugins[0]),
            mod_root=Path(ctx.mod_path),
        )
        _safe_emit_log(runner, "INFO", f"Wrote FO76 map marker icons for the Appalachia pack: {icons}")
    except Exception as exc:  # the map falls back to its FO4 icons
        _safe_emit_log(runner, "WARN", f"FO76 map marker icons skipped: {exc}")


def _restore_fo76_ui_sources(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    if ctx.source_data_dir is None:
        return 0
    from bacup_lib.source_reextract import restore_missing_ui_sources

    result = restore_missing_ui_sources(Path(ctx.source_data_dir), ctx.source_archive_dirs)
    if result.restored:
        shown = ", ".join(result.restored[:10])
        more = f" (+{len(result.restored) - 10} more)" if len(result.restored) > 10 else ""
        _safe_emit_log(
            runner,
            "WARN",
            f"Re-extracted {len(result.restored)} missing FO76 UI source file(s) "
            f"from the game archives: {shown}{more}",
        )
    for rel, reason in result.failed.items():
        _safe_emit_log(runner, "ERROR", f"Could not re-extract {rel}: {reason}")
    if result.failed:
        raise RuntimeError(
            f"{len(result.failed)} FO76 UI source file(s) could not be re-extracted: "
            + ", ".join(list(result.failed)[:5])
        )
    return len(result.restored)


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
            output_file = (
                output_root / output_relative / source_file.relative_to(source_tree)
            )
            output_file.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source_file, output_file)
            copied += 1

    _safe_emit_log(runner, "INFO", f"Copied {copied} FO76 VaultBoy SWF asset(s)")
    return copied


def _convert_fo76_perk_vaultboys(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.perk_vaultboys import LIBRARY, convert_perk_vaultboys, emit_perk_vaultboys

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / LIBRARY).is_file():
        _safe_emit_log(runner, "WARN", "Perk Vault Boy HUD clips skipped: perks library has not been extracted")
        return 0
    mod = Path(ctx.mod_path)
    manifest = convert_perk_vaultboys(source_root, mod / "data")
    target_data = getattr(ctx, "target_data_dir", None) or request.target_data_dir
    catalog = emit_perk_vaultboys(Path(ctx.source_plugin_path), mod / ctx.output_plugin_name, mod,
                                  Path(target_data) if target_data else None)
    _safe_emit_log(runner, "INFO", f"Converted {len(manifest['files'])} perk Vault Boy HUD clips; "
                                   f"{len(catalog['perks'])} perk bindings")
    return len(manifest["files"])


def _merge_packaged_tales_translations(
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> None:
    """Top up the shared translation table with bacup_lib's packaged Tales keys.

    Each FO76 UI converter (legendary perks, daily ops, fishing, ...) writes
    into the same per-DLL Scaleform table, so any one of them running to
    completion should leave it whole. Idempotent: merging already-present
    keys is a no-op on the resulting file.

    Only these pipeline wrappers call this; a direct `convert_*_ui()` caller
    (e.g. a standalone staging script) skips the packaged merge.
    """
    from bacup_lib.translations import (
        merge_packaged_overlays, merge_ui_translations, packaged_tales_lines, translation_path,
    )

    output_data = Path(ctx.mod_path) / "data"
    accumulator = translation_path(output_data)
    preserved, written = merge_ui_translations(output_data, packaged_tales_lines())
    merge_packaged_overlays(accumulator)
    _safe_emit_log(
        runner, "INFO",
        f"translations: merged {written} Tales lines into {accumulator} (preserved {preserved})",
    )


def _convert_fo76_challenges(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.challenge_catalog import emit_challenge_catalog
    from bacup_lib.challenge_hud_ui import convert_challenge_hud_ui
    from bacup_lib.challenge_ui import convert_challenge_map_ui

    target_data = getattr(ctx, "target_data_dir", None) or request.target_data_dir
    if target_data is None:
        raise ValueError("Challenge conversion requires the target Fallout 4 Data directory")
    mod = Path(ctx.mod_path)
    catalog = emit_challenge_catalog(
        Path(ctx.source_plugin_path), mod, Path(target_data),
        output_plugin_name=ctx.output_plugin_name,
    )
    menu_manifest = convert_challenge_map_ui(Path(ctx.source_data_dir), mod)
    hud_manifest = convert_challenge_hud_ui(Path(ctx.source_data_dir), mod / "data")
    count = len(menu_manifest["files"]) + len(hud_manifest["files"])
    _safe_emit_log(runner, "INFO", f"Generated {len(catalog['challenges'])} challenges and {count} map/HUD assets")
    return count


def _convert_fo76_expedition_results(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.expedition_results_ui import MENU, convert_expedition_results_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "Expedition results UI skipped: source menu has not been extracted")
        return 0
    manifest = convert_expedition_results_ui(source_root, Path(ctx.mod_path) / "data")
    count = len(manifest["files"])
    _safe_emit_log(runner, "INFO", f"Converted Expedition results menu and {count - 1} dependencies")
    _merge_packaged_tales_translations(ctx, runner)
    return count


def _convert_fo76_raid_rewards_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib import raid_rewards_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / raid_rewards_ui.MENU).is_file():
        _safe_emit_log(runner, "WARN", "Raid rewards UI skipped: source menu has not been extracted")
        return 0
    manifest = raid_rewards_ui.convert_raid_rewards_ui(source_root, Path(ctx.mod_path) / "data")
    count = len(manifest["files"])
    _safe_emit_log(runner, "INFO", f"Converted raid rewards menu and {count - 1} dependencies")
    _merge_packaged_tales_translations(ctx, runner)
    return count


def _convert_fo76_player_ghoul(request, ctx, runner=None) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.player_ghoul import emit_player_ghoul

    mod = Path(ctx.mod_path)
    target_data = getattr(ctx, "target_data_dir", None) or request.target_data_dir
    catalog = emit_player_ghoul(Path(ctx.source_plugin_path), mod / ctx.output_plugin_name, mod,
                               Path(target_data) if target_data else None,
                               source_root=Path(getattr(ctx, "source_data_dir", None) or getattr(ctx, "source_extracted_dir", None) or Path(ctx.source_plugin_path).parent))
    _safe_emit_log(runner, "INFO", f"Player ghoul catalog: {len(catalog['cards'])} cards, {len(catalog['chems'])} chems; appearance fallback")
    return 1


def _convert_fo76_pets(request: PluginPortRequest, ctx: ConversionContext,
                       runner: "ConversionRunner | None" = None) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.pet_catalog import emit_pet_catalog
    mod = Path(ctx.mod_path)
    catalog = emit_pet_catalog(Path(ctx.source_plugin_path), mod / ctx.output_plugin_name, mod,
                              source_root=Path(ctx.source_data_dir))
    _safe_emit_log(runner, "INFO", f"Pet runtime catalog: {len(catalog['pets'])} variants, {len(catalog['apparel'])} apparel records")
    progression = catalog["progression"]
    _safe_emit_log(runner, "INFO" if progression["available"] else "WARN",
                   "Pet progression board converted" if progression["available"] else progression["reason"])
    return 1


def _convert_fo76_reputation_presentation(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.reputation_catalog import emit_reputation_catalog
    from bacup_lib.reputation_ui import convert_reputation_ui
    from bacup_lib.reputation_map_ui import convert_reputation_map_ui

    mod = Path(ctx.mod_path)
    catalog = emit_reputation_catalog(
        Path(ctx.source_plugin_path), mod / ctx.output_plugin_name, mod
    )
    missing = catalog.get("missing_converted", [])
    if missing:
        _safe_emit_log(
            runner, "WARN",
            f"Reputation presentation is missing {len(missing)} converted form(s): {', '.join(missing)}",
        )
    manifest = convert_reputation_ui(Path(ctx.source_data_dir), mod / "data")
    map_art = convert_reputation_map_ui(Path(ctx.source_data_dir), mod)
    count = len(manifest["files"]) + len(map_art["files"])
    _safe_emit_log(runner, "INFO", f"Converted reputation HUD, native map artwork and catalog for {len(catalog['factions'])} factions")
    _merge_packaged_tales_translations(ctx, runner)
    return count


def _convert_fo76_respawn_map(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.respawn_ui import convert_respawn_ui

    mod = Path(ctx.mod_path)
    manifest = convert_respawn_ui(
        Path(ctx.source_data_dir), mod,
        source_plugin=Path(ctx.source_plugin_path),
        converted_plugin=mod / ctx.output_plugin_name,
    )
    _safe_emit_log(runner, "INFO", "Generated the Tales death bag catalog and native map contract")
    return len(manifest["files"])


def _convert_fo76_infestations(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.infestations import CATALOG, OUTPUT, convert_infestations, remove_legacy_bundle, summarize

    mod = Path(ctx.mod_path)
    try:
        result = convert_infestations(
            Path(ctx.source_data_dir), mod,
            source_plugin=Path(ctx.source_plugin_path),
            converted_plugin=mod / ctx.output_plugin_name,
        )
    except (FileNotFoundError, KeyError, ValueError) as error:
        # Tales disables Infestations when these are absent; stale ones would describe another build.
        (mod / CATALOG).unlink(missing_ok=True)
        (mod / OUTPUT / "presentation.json").unlink(missing_ok=True)
        remove_legacy_bundle(mod)
        _safe_emit_log(runner, "WARN", f"Infestations skipped: {error}")
        return 0
    for line in summarize(result["catalog"]):
        _safe_emit_log(runner, "INFO", f"Infestations: {line}")
    return len(result["presentation"]["files"]) + 1


def _convert_fo76_quest_tabs(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.pipboy_quest_catalog import OUTPUT, emit_quest_catalog, inspect_presentation

    mod = Path(ctx.mod_path)
    try:
        inspect_presentation(Path(ctx.source_data_dir))
    except (FileNotFoundError, ValueError) as error:
        (mod / OUTPUT).unlink(missing_ok=True)
        _safe_emit_log(runner, "WARN", f"Pip-Boy quest tabs skipped: {error}")
        return 0
    catalog = emit_quest_catalog(
        Path(ctx.source_plugin_path), mod / ctx.output_plugin_name,
        Path(ctx.source_data_dir), mod,
    )
    _safe_emit_log(runner, "INFO", f"Pip-Boy quest catalog mapped {len(catalog['quests'])} quests")
    return 1


def _convert_fo76_pipboy2000_anim_text(request, ctx, runner=None) -> int:
    """Must run after AnimTextData generation, which deletes the whole AnimTextData tree."""
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.pipboy2000_anim_text import DataSource, base_source, emit_pipboy2000_anim_text

    target_data = getattr(ctx, "target_data_dir", None) or request.target_data_dir
    if target_data is None:
        _safe_emit_log(runner, "WARN", "Pip-Boy 2000 AnimTextData skipped: the Fallout 4 Data directory is unknown")
        return 0
    mod = Path(ctx.mod_path)
    converted, base = DataSource(mod / "data"), base_source(Path(target_data))
    try:
        written = emit_pipboy2000_anim_text(mod, converted, base)
    except FileNotFoundError as error:
        _safe_emit_log(runner, "WARN", f"Pip-Boy 2000 AnimTextData skipped: {error}")
        return 0
    finally:
        converted.close()
        base.close()
    _safe_emit_log(runner, "INFO", f"Pip-Boy 2000 AnimTextData: wrote {len(written)} file(s)")
    return len(written)


def _convert_fo76_casino_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.casino_ui import MENU, convert_casino_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "Casino UI skipped: source menu has not been extracted")
        return 0
    manifest = convert_casino_ui(source_root, Path(ctx.mod_path) / "data")
    count = len(manifest["files"])
    _safe_emit_log(runner, "INFO", f"Converted casino menu, HUD, fanfare and {count - 3} dependencies")
    _merge_packaged_tales_translations(ctx, runner)
    return count


def _copy_fo76_holotape_programs(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.holotape_programs import copy_fo76_holotape_programs

    copied = copy_fo76_holotape_programs(Path(ctx.source_data_dir), Path(ctx.mod_path) / "data")
    if not copied:
        _safe_emit_log(runner, "WARN", "Holotape games skipped: source programs have not been extracted")
    else:
        _safe_emit_log(runner, "INFO", f"Copied {len(copied)} FO76 holotape game files")
    return len(copied)


def _convert_fo76_combat_perks(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.combat_perk_catalog import emit_combat_perk_catalog
    from bacup_lib.combat_perk_ui import convert_combat_perk_ui

    mod = Path(ctx.mod_path)
    catalog = emit_combat_perk_catalog(
        Path(ctx.source_plugin_path), mod / ctx.output_plugin_name, mod
    )
    manifest = convert_combat_perk_ui(Path(ctx.source_data_dir), mod / "data")
    missing = catalog.get("missing_converted", [])
    if missing:
        _safe_emit_log(
            runner, "WARN",
            f"Combat perks are missing {len(missing)} converted form(s): {', '.join(missing)}",
        )
    missing_source = catalog.get("missing_source", [])
    if missing_source:
        _safe_emit_log(
            runner, "WARN",
            f"Combat perks are missing {len(missing_source)} source form(s): {', '.join(missing_source)}",
        )
    _safe_emit_log(runner, "INFO", f"Generated the combat perk catalog and HUD asset: {manifest['movie']}")
    return 2


def _convert_fo76_quick_boy_ui(request, ctx, runner=None) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.quick_boy_ui import convert_quick_boy_ui

    manifest = convert_quick_boy_ui(Path(ctx.source_data_dir), Path(ctx.mod_path) / "data")
    _safe_emit_log(runner, "INFO", "Converted Quick-Boy background from the installed FO76 Pip-Boy")
    return len(manifest["files"])


def _convert_fo76_status_hud_ui(request, ctx, runner=None) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.status_hud_ui import convert_status_hud_ui

    manifest = convert_status_hud_ui(Path(ctx.source_data_dir), Path(ctx.mod_path) / "data",
                                     source_plugin=Path(ctx.source_plugin_path))
    _safe_emit_log(runner, "INFO", "Converted isolated health/status and condition HUD artwork")
    return len(manifest["files"])


def _convert_fo76_legendary_perk_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.legendary_perks_ui import MENU, convert_legendary_perk_ui
    from bacup_lib.legendary_perk_bindings import emit_legendary_perk_bindings

    bindings = emit_legendary_perk_bindings(Path(ctx.mod_path) / ctx.output_plugin_name, Path(ctx.mod_path))
    _safe_emit_log(runner, "INFO", f"Generated {len(bindings['forms'])} legendary runtime bindings")

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "Legendary perk UI skipped: source menu has not been extracted")
        return 0
    manifest = convert_legendary_perk_ui(source_root, Path(ctx.mod_path) / "data")
    count = len(manifest["files"])
    _safe_emit_log(runner, "INFO", f"Converted legendary perk menu and {count - 1} dependencies")
    _merge_packaged_tales_translations(ctx, runner)
    return count


def _convert_fo76_perk_card_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.perk_cards_ui import MENU, convert_perk_card_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "Regular perk UI skipped: source menu has not been extracted")
        return 0
    manifest = convert_perk_card_ui(source_root, Path(ctx.mod_path) / "data")
    _safe_emit_log(runner, "INFO", f"Converted regular perk menu and {len(manifest['files']) - 1} dependencies")
    count = len(manifest["files"])
    from bacup_lib.special_builds_ui import MENU as SPECIAL_MENU, convert_special_builds_ui
    if (source_root / "interface" / SPECIAL_MENU).is_file():
        special = convert_special_builds_ui(source_root, Path(ctx.mod_path) / "data")
        count += len(special["files"])
        _safe_emit_log(runner, "INFO", f"Converted punch card SPECIAL menu and {len(special['files']) - 1} dependencies")
    else:
        _safe_emit_log(runner, "WARN", "Punch card UI skipped: source menu has not been extracted")
    _merge_packaged_tales_translations(ctx, runner)
    return count


def _convert_fo76_keypad_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.keypad_ui import MENU, convert_keypad_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "Keypad UI skipped: source menu has not been extracted")
        return 0
    manifest = convert_keypad_ui(source_root, Path(ctx.mod_path) / "data")
    count = len(manifest["files"])
    _safe_emit_log(runner, "INFO", f"Converted keypad menu and {count - 1} dependencies")
    return count


def _convert_fo76_daily_ops_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.daily_ops_ui import MENU, convert_daily_ops_ui
    from bacup_lib.daily_ops_voice import convert_daily_ops_voice

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "Daily Ops UI skipped: source menu has not been extracted")
        return 0
    manifest = convert_daily_ops_ui(source_root, Path(ctx.mod_path) / "data")
    # Native presentation reads its generated catalogue loose, outside the BA2s.
    voices = convert_daily_ops_voice(source_root, Path(ctx.mod_path))
    count = len(manifest["files"])
    _safe_emit_log(runner, "INFO", f"Converted Daily Ops report, HUD, radio portrait and {len(voices['topics'])} radio cues")
    _merge_packaged_tales_translations(ctx, runner)
    return count


def _convert_fo76_currency_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.currency_ui import convert_currency_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface/hudmenu.swf").is_file():
        _safe_emit_log(runner, "WARN", "Currency UI skipped: source HUD has not been extracted")
        return 0
    manifest = convert_currency_ui(source_root, Path(ctx.mod_path) / "data")
    _safe_emit_log(runner, "INFO", "Converted currency gain notifications, icons and sound cues")
    return len(manifest["files"])


def _convert_fo76_xp_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.xp_ui import SOURCE, convert_xp_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / SOURCE).is_file():
        _safe_emit_log(runner, "WARN", "Experience UI skipped: source HUD has not been extracted")
        return 0
    manifest = convert_xp_ui(source_root, Path(ctx.mod_path) / "data")
    count = len(manifest["files"]) + len(manifest["audio"])
    _safe_emit_log(runner, "INFO", f"Converted the experience HUD and {len(manifest['audio'])} level-up sounds")
    return count


def _convert_fo76_quest_area_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.quest_area_ui import SOURCE, convert_quest_area_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / SOURCE).is_file():
        _safe_emit_log(runner, "WARN", "Quest area UI skipped: source HUD has not been extracted")
        return 0
    manifest = convert_quest_area_ui(source_root, Path(ctx.mod_path) / "data")
    _safe_emit_log(runner, "INFO", "Converted the objective-area compass clip")
    _merge_packaged_tales_translations(ctx, runner)
    return len(manifest["files"])


def _convert_fo76_death_screen_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.death_screen_ui import MENU, convert_death_screen_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "Death screen UI skipped: source menu has not been extracted")
        return 0
    manifest = convert_death_screen_ui(source_root, Path(ctx.mod_path) / "data")
    _safe_emit_log(runner, "INFO", "Converted the FO76 death respawn screen")
    _merge_packaged_tales_translations(ctx, runner)
    return len(manifest["files"])


def _convert_fo76_revive_prompt_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.revive_prompt_ui import SOURCE, convert_revive_prompt_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / SOURCE).is_file():
        _safe_emit_log(runner, "WARN", "Revive prompt UI skipped: source HUD has not been extracted")
        return 0
    manifest = convert_revive_prompt_ui(source_root, Path(ctx.mod_path) / "data")
    _safe_emit_log(runner, "INFO", "Converted the FO76 revive prompt")
    _merge_packaged_tales_translations(ctx, runner)
    return len(manifest["files"])


def _convert_fo76_mission_map_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.mission_map_ui import convert_mission_map_ui

    if not (Path(ctx.source_data_dir) / "interface/dailyopsiconlibrary.swf").is_file():
        _safe_emit_log(runner, "WARN", "Mission map PNGs skipped: source UI has not been extracted")
        return 0
    # The native map reads loose files beside its DLL; the F4SE tree is deployed outside BA2s.
    manifest = convert_mission_map_ui(Path(ctx.source_data_dir), Path(ctx.mod_path),
                                     source_plugin=getattr(ctx, "source_plugin_path", None))
    count = len(manifest["files"])
    _safe_emit_log(runner, "INFO", f"Converted {count} Daily Ops and Expeditions map assets")
    return count


def _convert_fo76_camp_build_ui(request, ctx, runner=None) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.camp_build_ui import MENU, convert_camp_build_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "CAMP build UI skipped: source menu has not been extracted")
        return 0
    manifest = convert_camp_build_ui(source_root, Path(ctx.mod_path) / "data")
    _merge_packaged_tales_translations(ctx, runner)
    _safe_emit_log(runner, "INFO", f"Converted private CAMP build menu and {len(manifest['files']) - 1} dependencies")
    return len(manifest["files"])


def _prepare_fo76_camp_wallpapers(request, ctx, runner=None) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.camp_wallpapers import emit_camp_wallpapers

    mod = Path(ctx.mod_path)
    result = emit_camp_wallpapers(Path(ctx.source_plugin_path), mod / ctx.output_plugin_name, mod)
    for key, label in (("missing_converted", "unconverted forms"), ("missing_meshes", "missing meshes")):
        if result[key]:
            _safe_emit_log(runner, "WARN", f"CAMP wallpapers: {len(result[key])} {label}: {', '.join(result[key][:10])}")
    if result["unrepaired_meshes"]:
        meshes = [item["mesh"] for item in result["unrepaired_meshes"]]
        _safe_emit_log(runner, "WARN", f"CAMP wallpapers: {len(meshes)} meshes keep their own navmesh cutters: {', '.join(meshes[:10])}")
    _safe_emit_log(runner, "INFO", f"CAMP wallpapers: {len(result['wallpapers'])} wallpapers, "
                                   f"{len(result['repaired_meshes'])} meshes given their wall's connect points or cutters")
    return len(result["wallpapers"])


def _prepare_fo76_camp_snap(request, ctx, runner=None) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.camp_snap import emit_camp_snap

    result = emit_camp_snap(Path(ctx.source_plugin_path), Path(ctx.mod_path))
    _safe_emit_log(runner, "INFO", f"CAMP snapping: {result['node_types']} node types, {result['templates']} templates, "
                                   f"{result['forms']} forms ({result['skipped_forms']} without snap nodes)")
    return result["forms"]


def _prepare_fo76_camp_scrap(request, ctx, runner=None) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.camp_scrap import emit_camp_scrap

    target_data = getattr(ctx, "target_data_dir", None) or request.target_data_dir
    result = emit_camp_scrap(Path(ctx.source_plugin_path), Path(target_data), Path(ctx.mod_path))
    if result["missing_masters"]:
        _safe_emit_log(runner, "WARN", f"CAMP scrapping: FO4 masters not found: {', '.join(result['missing_masters'])}")
    tiers = ", ".join(f"{count} by {tier}" for tier, count in sorted(result["tiers"].items()))
    _safe_emit_log(runner, "INFO", f"CAMP scrapping: {result['objects']} FO76 deletable objects given FO4 scrap recipes ({tiers})")
    return result["objects"]


@contextlib.contextmanager
def _fo4_behavior_root(request, ctx):
    """A Data-shaped root holding the FO4 behavior graphs emotes and photo mode read,
    from the extracted overlay or the game's archives."""
    from bacup_lib.emotes import HUMAN_GRAPH, POWER_ARMOR_GRAPH, POWER_ARMOR_SKELETON

    store = getattr(ctx, "target_asset_store", None)
    extracted = getattr(ctx, "target_extracted_dir", None) or request.target_extracted_dir
    if store is None:
        if not extracted:
            raise ValueError("Fallout 4 behavior graphs need the Fallout 4 Data directory")
        yield Path(extracted)
        return
    with tempfile.TemporaryDirectory(prefix="bacup_fo4_behaviors_") as root:
        for asset in (HUMAN_GRAPH, POWER_ARMOR_GRAPH, POWER_ARMOR_SKELETON):
            source = store.materialize(asset)
            if source is None:
                raise ValueError(f"Fallout 4 {asset} is missing from the game's Data and archives")
            destination = Path(root) / asset
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
        yield Path(root)


def _convert_fo76_photo_mode_ui(request, ctx, runner=None) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.photo_mode_ui import convert_photo_mode_ui
    with _fo4_behavior_root(request, ctx) as target_root:
        result = convert_photo_mode_ui(Path(ctx.source_data_dir), Path(ctx.mod_path) / "data",
                                       source_plugin=Path(ctx.source_plugin_path), target_root=target_root)
    _merge_packaged_tales_translations(ctx, runner)
    return len(result["files"])


def _convert_fo76_menu_presentation(request, ctx, runner=None) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.menu_presentation_ui import MENUS, convert_menu_presentation

    source_root = Path(ctx.source_data_dir)
    if not all((source_root / "interface" / name).is_file()
               for name in MENUS):
        _safe_emit_log(runner, "WARN", "FO76 menu presentation skipped: source menus are missing")
        return 0
    source_data = next((path for path in ctx.source_archive_dirs
                        if (path.parent / "Fallout76.ini").is_file()), None)
    if source_data is None:
        _safe_emit_log(runner, "WARN", "FO76 menu presentation skipped: installed source media unavailable")
        return 0
    result = convert_menu_presentation(source_root, source_data.parent,
                                       Path(ctx.mod_path) / "data", ctx.target_extracted_dir,
                                       getattr(ctx, "target_data_dir", None) or request.target_data_dir)
    for missing in result.get("missing_media", []):
        _safe_emit_log(runner, "WARN", f"FO76 menu media missing, skipped: {missing}")
    from bacup_lib.character_creation_ui import convert_character_creation_ui
    character_creation = convert_character_creation_ui(source_root, Path(ctx.mod_path) / "data")
    _merge_packaged_tales_translations(ctx, runner)
    return len(result["files"]) + len(character_creation["files"])


def _convert_fo76_photo_gallery_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.photo_gallery_ui import MENU, convert_photo_gallery_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "Photo gallery UI skipped: source menu has not been extracted")
        return 0
    convert_photo_gallery_ui(source_root, Path(ctx.mod_path))
    _safe_emit_log(runner, "INFO", "Converted the source-preserved photo gallery menu and Prisma map presentation")
    _merge_packaged_tales_translations(ctx, runner)
    return 2


def _convert_fo76_fishing_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.fishing_ui import convert_fishing_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface/fishingmenu.swf").is_file():
        _safe_emit_log(runner, "WARN", "Fishing UI skipped: source menu has not been extracted")
        return 0
    manifest = convert_fishing_ui(source_root, Path(ctx.mod_path) / "data")
    count = len(manifest["files"])
    _safe_emit_log(runner, "INFO", f"Converted fishing menu and {count - 1} dependencies")
    _merge_packaged_tales_translations(ctx, runner)
    return count


def _convert_fo76_inspect_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.inspect_ui import MENU, convert_inspect_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "Inspect UI skipped: source menu has not been extracted")
        return 0
    manifest = convert_inspect_ui(source_root, Path(ctx.mod_path) / "data")
    count = len(manifest["files"])
    _safe_emit_log(runner, "INFO", f"Converted inspect item card and {count - 1} dependencies")
    _merge_packaged_tales_translations(ctx, runner)
    return count


def _convert_fo76_workbench_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.workbench_ui import convert_workbench_ui

    target_data = getattr(ctx, "target_data_dir", None) or request.target_data_dir
    if target_data is None:
        raise ValueError("Workbench conversion requires Fallout 4's target data directory")
    convert_workbench_ui(Path(ctx.mod_path) / "data", target_data_dir=Path(target_data))
    _safe_emit_log(runner, "INFO", "Connected condition repair to Fallout 4's workbench button bar")
    return 1


def _convert_fo76_barter_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.barter_ui import SOURCE, convert_barter_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / SOURCE).is_file():
        _safe_emit_log(runner, "WARN", "Barter UI skipped: source trade menu has not been extracted")
        return 0
    target_data = getattr(ctx, "target_data_dir", None) or request.target_data_dir
    if target_data is None:
        raise ValueError("Barter conversion requires Fallout 4's target data directory")
    manifest = convert_barter_ui(source_root, Path(ctx.mod_path) / "data", target_data_dir=Path(target_data))
    from bacup_lib.inventory_lists_ui import convert_inventory_lists
    inventory_files = convert_inventory_lists(Path(target_data), Path(ctx.mod_path) / "data")
    from bacup_lib.power_armor_icons import generate_power_armor_icons
    generate_power_armor_icons(source_root, Path(ctx.mod_path))
    _safe_emit_log(runner, "INFO", "Converted FO76 vendor panels and category strip around FO4's barter controller")
    return len(manifest["files"]) + len(inventory_files) + 1


def _generate_item_icons(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    """Per-item inventory icons for FO4's masters and the converted plugin, drawn from the models the game loads."""
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    target_data = getattr(ctx, "target_data_dir", None) or request.target_data_dir
    if target_data is None:
        _safe_emit_log(runner, "WARN", "Item icons skipped: Fallout 4's data directory is not set")
        return 0
    from bacup_lib.item_icons import FO4_MASTERS, GameData, generate_item_icons, master_strings

    mod_root, target = Path(ctx.mod_path), Path(target_data)
    # The converted assets are still loose; Fallout 4's come from its archives.
    data = GameData((str(mod_root / "data"), str(target)), (str(target),))
    masters = [target / name for name in FO4_MASTERS if (target / name).is_file()]
    with tempfile.TemporaryDirectory(prefix="bacup-item-icons-") as strings:
        master_strings(data, masters, Path(strings))
        plugins = [(master, Path(strings)) for master in masters]
        plugins.append((mod_root / ctx.output_plugin_name, mod_root / "Strings"))
        generate_item_icons(plugins, data, mod_root, workers=int(getattr(ctx, "conversion_workers", None) or 1))
    _safe_emit_log(runner, "INFO", "Rendered offline inventory icons for Fallout 4's and the converted items")
    return 1


def _fo76_legacy_leftover_materials(source_root: str | Path | None, mod_data: Path) -> list[str]:
    """The mod's pre-v22 BGSMs converted from FO4-era FO76 leftovers (an ``_s`` smooth/spec map).

    Legacy conversions of real FO76 materials also land pre-v22 but keep their FO76 ``_r``;
    their source names no ``_s``, so they stay out (and are never upgraded).
    """
    leftovers = []
    materials = mod_data / "materials"
    if not source_root or not materials.is_dir():
        return leftovers
    source_root = Path(source_root)
    for path in materials.rglob("*.bgsm"):
        with open(path, "rb") as handle:
            header = handle.read(8)
        if len(header) < 8 or int.from_bytes(header[4:8], "little") >= 20:
            continue
        key = path.relative_to(mod_data).as_posix().lower()
        if _fo76_source_names_smoothspec(source_root, key):
            leftovers.append(key)
    return leftovers


def _fo76_source_names_smoothspec(source_root: Path, key: str) -> bool:
    from creation_lib.material_tools.bgsm_bin import read_bgsm

    source = source_root / key
    if not source.is_file():
        return False
    try:
        with open(source, "rb") as handle:
            spec = read_bgsm(handle).SmoothSpecTexture
    except Exception:
        return False
    return Path(str(spec).replace("\x00", "").replace("\\", "/")).stem.lower().endswith("_s")


_UPGRADE_MANIFEST = "debug/fo4_material_upgrade.json"
_UPGRADE_ORIGINALS = ".bacup/upgrade_originals"


def _manifest_from_upgrade_report(
    mod_root: Path,
    source_data_dir: Path | None,
    target_extracted_dir: Path | None,
    target_data_dir: Path | None = None,
) -> dict:
    """Outputs predating the manifest: the upgrade's own report lists what it wrote and the
    native-overwrite marker is the exclude set. Leftovers upgraded in place lost their
    originals; FO4's copy stands in where it has one, the rest stay as they are."""
    report_path = Path(mod_root) / "data" / "materialupgrade" / "upgrade_report.json"
    if not report_path.is_file():
        raise FileNotFoundError(f"{report_path} is missing; run a full regen with --upgrade-fo4-materials once")
    written = json.loads(report_path.read_text(encoding="utf-8"))["result"]["written"]
    marker = Path(mod_root) / "debug" / "fo4_overwrite_outputs.txt"
    exclude = marker.read_text(encoding="utf-8").split() if marker.is_file() else []
    store = None

    def fo4_ships(key: str) -> bool:
        nonlocal store
        if target_extracted_dir is not None:
            return (Path(target_extracted_dir) / key).is_file()
        if target_data_dir is None:
            return False
        if store is None:
            from bacup_lib.target_assets import ensure_target_asset_catalog

            store = build_target_asset_store(
                target_data_dir=target_data_dir, catalog_path=ensure_target_asset_catalog(target_data_dir)
            )
        return store.has_asset(key)

    keep = {
        key
        for key in written
        if key.endswith(".bgsm")
        and source_data_dir is not None
        and _fo76_source_names_smoothspec(Path(source_data_dir), key)
        and not fo4_ships(key)
    }
    return {"written": [key for key in written if key not in keep], "leftovers": [], "exclude_paths": exclude}


def rerun_fo4_material_upgrade(
    mod_root: Path,
    target_data_dir: Path,
    source_data_dir: Path | None,
    workers: int,
    runner: "ConversionRunner | None" = None,
    target_extracted_dir: Path | None = None,
    source_extracted_dir: Path | None = None,
) -> int:
    """Run only the FO4 material upgrade again on an existing output: drop the last run's
    outputs, restore the leftover originals, then upgrade with the recorded exclude set.
    With ``source_extracted_dir`` it also re-runs the file-only head repairs."""
    manifest_path = Path(mod_root) / _UPGRADE_MANIFEST
    if manifest_path.is_file():
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    else:
        manifest = _manifest_from_upgrade_report(mod_root, source_data_dir, target_extracted_dir, target_data_dir)
    mod_data = Path(mod_root) / "data"
    leftovers = set(manifest["leftovers"])
    for key in manifest["written"]:
        if key.lower() not in leftovers:
            (mod_data / key).unlink(missing_ok=True)
    for key in leftovers:
        shutil.copy2(Path(mod_root) / _UPGRADE_ORIGINALS / key, mod_data / key)
    request = SimpleNamespace(
        options=SimpleNamespace(upgrade_fo4_materials=True),
        source_game="fo76",
        target_game="fo4",
        target_data_dir=target_data_dir,
    )
    ctx = SimpleNamespace(
        target_data_dir=target_data_dir,
        mod_path=mod_root,
        source_data_dir=source_data_dir,
        native_overwrite_paths=manifest["exclude_paths"],
        conversion_workers=workers,
    )
    upgraded = _upgrade_fo4_materials(request, ctx, runner)  # type: ignore[arg-type]
    if source_extracted_dir is not None:
        _repair_face_heads(mod_data, source_extracted_dir)
    return upgraded


def _repair_face_heads_for(ctx) -> None:
    # Regen hands the extracted FO76 tree over as source_data_dir.
    source_root = getattr(ctx, "source_extracted_dir", None) or getattr(ctx, "source_data_dir", None)
    if not source_root:
        _log.warning("face heads: no FO76 source directory on the run; head repair skipped")
        return
    _repair_face_heads(Path(ctx.mod_path) / "data", Path(source_root))


def _repair_face_heads(mod_data: Path, source_extracted_dir: Path) -> None:
    """Asset waves re-copy FO76's BC1 base heads and Customization/ head BGSMs; repair them
    after every one, not only in final_plugin."""
    heads, base_heads, warnings = load_native_module().repair_face_head_files(
        str(mod_data), str(source_extracted_dir)
    )
    for warning in warnings:
        _log.warning("face heads: %s", warning)
    _log.info("face heads: %d head materials repointed, %d base head textures re-encoded", heads, base_heads)


def _upgrade_fo4_materials(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
    phase: PhaseProgress | None = None,
) -> int:
    """Native Materials → Upgrade: FO4's own materials become BGSM v22+, except the replace set."""
    if not getattr(request.options, "upgrade_fo4_materials", False):
        return 0
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    target_data = getattr(ctx, "target_data_dir", None) or request.target_data_dir
    if target_data is None:
        _safe_emit_log(runner, "WARN", "FO4 material upgrade skipped: Fallout 4's data directory is not set")
        return 0
    from creation_lib.material_tools import upgrade_v22

    mod_root = Path(ctx.mod_path)
    mod_data = mod_root / "data"
    mod_data.mkdir(parents=True, exist_ok=True)
    leftovers = _fo76_legacy_leftover_materials(getattr(ctx, "source_data_dir", None), mod_data)
    exclude_paths = list(getattr(ctx, "native_overwrite_paths", None) or [])
    # Leftovers are upgraded in place; keep the converted originals so the upgrade can run again.
    for key in leftovers:
        original = mod_root / _UPGRADE_ORIGINALS / key
        original.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(mod_data / key, original)
    # The converted mod overlays FO4's data: its own materials win and are never upgraded,
    # whatever their version (pre-v22 FO76 conversions lost their real _r here, 2026-10-11).
    options = upgrade_v22.UpgradeOptions(
        sources=[str(target_data)],
        overlay_sources=[str(mod_data)],
        overlay_upgrade_paths=leftovers,
        output=str(mod_data),
        # FO4 metal colour lives only in the envmap cube; envmap mode carries it into `_r` F0.
        metal_mode="envmap",
        # FO4 meshes carry inline texture sets and envmap shader types that override the upgraded BGSM.
        include_nifs=True,
        exclude_paths=exclude_paths,
        workers=int(getattr(ctx, "conversion_workers", None) or 4),
    )
    logged = {"stage": None, "step": -1}

    def log_progress(done: int, total: int, stage: str) -> bool:
        step = done * 20 // max(total, 1)
        if stage != logged["stage"] or step != logged["step"]:
            logged.update(stage=stage, step=step)
            _safe_emit_log(runner, "INFO", f"FO4 material upgrade: {stage} {done}/{total}")
        if phase is not None and runner is not None:
            phase.total_items, phase.completed_items, phase.current_item = total, done, stage
            runner.emit_item_progress(phase)
        return not (runner is not None and getattr(runner, "is_cancelled", lambda: False)())

    _safe_emit_log(runner, "INFO", "FO4 material upgrade: planning (scans every FO4 material and mesh)")
    report = upgrade_v22.run(options, log_progress)
    manifest = {"written": report["result"].get("written", []), "leftovers": leftovers, "exclude_paths": exclude_paths}
    (mod_root / _UPGRADE_MANIFEST).parent.mkdir(parents=True, exist_ok=True)
    (mod_root / _UPGRADE_MANIFEST).write_text(json.dumps(manifest, indent=1), encoding="utf-8")
    summary = report["summary"]
    errors = report["result"]["errors"]
    _safe_emit_log(
        runner,
        "WARN" if errors else "INFO",
        f"Upgraded {summary['upgrade']} FO4 materials to BGSM v22+ ({summary['skipped']} skipped, {len(errors)} errors)",
    )
    return int(summary["upgrade"])


def _convert_fo76_favorites_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.favorites_ui import MENU, convert_favorites_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "Favorites wheel skipped: source radial menu has not been extracted")
        return 0
    target_data = getattr(ctx, "target_data_dir", None) or request.target_data_dir
    if target_data is None:
        raise ValueError("Favorites conversion requires Fallout 4's target data directory for the stock menu")
    manifest = convert_favorites_ui(source_root, Path(ctx.mod_path) / "data", source_plugin=Path(ctx.source_plugin_path),
                                    target_data_dir=Path(target_data))
    _safe_emit_log(runner, "INFO", "Converted the FO76 favorites wheel for FO4's twelve saved quickslots")
    return len(manifest["files"])


def _convert_fo76_emotes(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.emotes import convert_emotes

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface/radialmenu.swf").is_file():
        _safe_emit_log(runner, "WARN", "Emotes skipped: source radial menu has not been extracted")
        return 0
    with _fo4_behavior_root(request, ctx) as target_root:
        manifest = convert_emotes(source_root, Path(ctx.mod_path), source_plugin=Path(ctx.source_plugin_path),
                                  target_root=target_root)
    _safe_emit_log(runner, "INFO", f"Converted {len(manifest['entries'])} verified human emotes")
    return len(manifest["files"]) + 1


def _convert_fo76_realtime_vats_ui(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib.realtime_vats_ui import MENU, convert_realtime_vats_ui

    source_root = Path(ctx.source_data_dir)
    if not (source_root / "interface" / MENU).is_file():
        _safe_emit_log(runner, "WARN", "Real-time VATS UI skipped: source menu has not been extracted")
        return 0
    manifest = convert_realtime_vats_ui(source_root, Path(ctx.mod_path) / "data")
    _safe_emit_log(runner, "INFO", "Converted FO76 VATS presentation for Tales real-time VATS")
    return len(manifest["files"])


def _emit_fo76_realtime_vats_catalog(
    request: PluginPortRequest,
    ctx: ConversionContext,
    runner: "ConversionRunner | None" = None,
) -> int:
    if (request.source_game.lower(), request.target_game.lower()) != ("fo76", "fo4"):
        return 0
    from bacup_lib import realtime_vats_catalog

    catalog = realtime_vats_catalog.emit_from_plugin(Path(ctx.source_plugin_path), Path(ctx.mod_path),
                                                     Path(ctx.source_data_dir))
    _safe_emit_log(runner, "INFO", f"Real-time VATS catalog: {len(catalog['luck_critical_charge_curve'])} Luck curve points")
    return 1


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
    root_strings = _inventory_root_strings_entries(mod_root / "Strings")
    if root_strings:
        root_string_paths = {entry.relative_path.casefold() for entry in root_strings}
        entries = [
            entry
            for entry in entries
            if entry.relative_path.casefold() not in root_string_paths
        ]
        entries.extend(root_strings)
    return entries


def _archive_label_selected(label: str, labels: tuple[str, ...]) -> bool:
    return any(
        label == wanted or (label.startswith(wanted) and label[len(wanted) :].isdigit())
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
    ba2_compression_level: int | None = None,
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
    """The sink join: reconcile the loose tree into the spills, plan shards,
    finalize non-texture archives from the spills, and direct-pack texture
    archives from the loose tree (the incremental DX10 spill is slower on the
    full FO76 corpus).

    On any error: spills are aborted and every archive written by THIS call
    (including the partial in-flight one) is deleted.

    ``archive_labels`` limits reconciliation, packing, and stale-output cleanup
    to the selected BA2 labels. Unselected archives remain untouched.
    """
    native = load_native_module()
    mod_root = Path(mod_root)
    archive_root = (
        Path(archive_output_dir) if archive_output_dir is not None else mod_root
    )
    archive_root.mkdir(parents=True, exist_ok=True)
    use_temp_outputs = not os.path.samefile(archive_root, mod_root)
    inventory_started = time.perf_counter()
    entries = inventory_packable_entries(mod_root)
    inventory_seconds = time.perf_counter() - inventory_started

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
    inventory_message = (
        f"Archive inventory: {len(entries)} files, "
        f"{sum(entry.size for entry in entries) / 1024**3:.1f} GiB in "
        f"{inventory_seconds:.1f}s; packing {len(plans)} BA2 archive(s)"
    )
    _log.info("%s", inventory_message)
    if pack_progress is not None:
        pack_progress({"message": inventory_message, "completed": 0, "total": len(plans)})
    selected_rels = {
        _normalize_streamed_rel(entry.relative_path)
        for planned in plans
        for entry in planned.entries
    }
    custom_compression = ba2_compression_level is not None
    direct_pack_rels = {
        _normalize_streamed_rel(entry.relative_path)
        for planned in plans
        if custom_compression
        or direct_pack_all
        or (direct_pack_textures and planned.texture_archive)
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
            if custom_compression
            or direct_pack_all
            or (direct_pack_textures and planned.texture_archive)
        ]
        spill_plans = [
            planned
            for planned in plans
            if not (
                custom_compression
                or direct_pack_all
                or (direct_pack_textures and planned.texture_archive)
            )
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
                reconcile_workers,
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
        if direct_pack_plans and ba2_compression_level is None:
            _run_native_pack_plans(
                [(planned, output) for planned, output, _final in direct_outputs],
                game,
                og=og_target,
                total_workers=requested_pack_workers,
                progress=pack_progress,
            )
        elif direct_pack_plans:
            total_archives = len(direct_outputs)
            for index, (planned, output, _final) in enumerate(direct_outputs):
                if pack_progress is not None:
                    pack_progress(
                        {
                            "message": f"Packing archive {planned.output_name}",
                            "completed": index,
                            "total": total_archives,
                        }
                    )
                _run_native_pack_entries(
                    planned.entries,
                    str(output),
                    game,
                    texture_archive=planned.texture_archive,
                    og=og_target,
                    compression_level=ba2_compression_level,
                    jobs=requested_pack_workers,
                )
                if pack_progress is not None:
                    pack_progress(
                        {
                            "message": (
                                "Archive packed native: name="
                                f"{planned.output_name}"
                            ),
                            "completed": index + 1,
                            "total": total_archives,
                        }
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
# fo4:starfield packaging tail
#
# Starfield auto-loads ONLY `<plugin> - Main.ba2`, `- Textures.ba2`,
# `- Voices_<lang>.ba2`, `- Localization.ba2` for a mod plugin
# (bacup/docs/starfield_target/R1-esm-wrld-btd.md). The generic
# `finalize_sinks_for_mod`/`plan_archive_outputs` path above is not reused:
# (1) its default `expanded_archives=True` splits terrain/meshes/materials into
# per-family archives Starfield never opens for a mod (silent total content
# loss); (2) it has no per-archive compression override, and `terrain/*.btd`
# (and, like vanilla `SFBGS00D - Main.ba2`, the whole Main archive) must ship
# UNCOMPRESSED. `_run_native_pack_entries` (unlike `_run_native_pack_plans`)
# takes a per-archive `compress`, so this tail builds exactly the two archives
# Starfield opens.
# ---------------------------------------------------------------------------

FO4_STARFIELD_MAIN_LABEL = "Main"
FO4_STARFIELD_TEXTURES_LABEL = "Textures"
#: The only archive labels Starfield auto-loads for a mod plugin.
FO4_STARFIELD_PACKED_LABELS: tuple[str, ...] = (
    FO4_STARFIELD_MAIN_LABEL,
    FO4_STARFIELD_TEXTURES_LABEL,
)
#: `data/`-relative top-level dirs that pack into the general, uncompressed
#: Main archive: .mesh + .nif, terrain/*.btd, and sound/soundbanks/**, like
#: `SFBGS00D - Main.ba2`, whose meshes+geometries+terrain+sound carry zero
#: compressed chunks.
_FO4_STARFIELD_MAIN_DIRS = frozenset({"meshes", "geometries", "terrain", "sound"})


@dataclass(frozen=True)
class Fo4StarfieldPackFlags:
    """Runtime knobs for :func:`build_pack_plan`.

    Kept separate from :class:`PackJob` so the pure classifier can be
    unit-tested without constructing a full mod tree or a conversion request.
    """

    mod_name: str = "Fallout4_SF"


@dataclass(frozen=True)
class PackJob:
    """One file's fo4:starfield packaging destination (R1 SD-8 / R3 / R5).

    ``destination`` is either an output BA2 filename (e.g.
    ``"Fallout4_SF - Main.ba2"``) or the literal string ``"loose"`` for files
    that must never be archived.
    """

    relative_path: str  # forward-slash path relative to the mod's `data/` root
    source_path: Path
    destination: str
    pack_kind: str | None  # "starfield" | "starfielddds" | None when loose
    compress: bool


def _fo4_starfield_archive_name(mod_name: str, label: str) -> str:
    return f"{mod_name} - {label}.ba2"


def _classify_fo4_starfield_path(relative_path: str) -> tuple[str, str | None, bool]:
    """Returns ``(label, pack_kind, compress)`` for one `data/`-relative path.

    ``label`` is ``"Main"``, ``"Textures"``, or ``"loose"``. Raises
    ``ValueError`` for anything outside the R-doc-mandated destinations —
    SD-8's failure mode is silent misfiling, so an unrecognized asset class
    fails loud instead of guessing a bucket for it.
    """
    top = relative_path.split("/", 1)[0].lower()
    if top == "materials":
        # R5: zero of ~1.26M vanilla archived files are .mat; ship loose.
        return "loose", None, False
    if top == "textures":
        return FO4_STARFIELD_TEXTURES_LABEL, "starfielddds", True
    if top in _FO4_STARFIELD_MAIN_DIRS:
        return FO4_STARFIELD_MAIN_LABEL, "starfield", False
    raise ValueError(
        f"fo4:starfield packaging: {relative_path!r} is outside the R-doc "
        "packaging rules (materials/, textures/, meshes/, geometries/, "
        "terrain/, sound/) — extend _classify_fo4_starfield_path instead of "
        "silently bucketing it (SD-8)."
    )


def build_pack_plan(
    mod_dir: Path, r_flags: Fo4StarfieldPackFlags | None = None
) -> list[PackJob]:
    """Classify every file under an fo4:starfield mod's `data/` tree into its
    packaging destination (R1 SD-8 / R3 mesh packability / R5 loose .mat).

    Pure — does no packing and calls no native code; ``mod_dir`` is the mod's
    `data/` root (the same root ``pack_fo4_starfield_archives`` derives as
    ``mod_root / "data"``). Actual archive writes happen in
    :func:`pack_fo4_starfield_archives`.
    """
    mod_dir = Path(mod_dir)
    flags = r_flags or Fo4StarfieldPackFlags()
    jobs: list[PackJob] = []
    for path in sorted(mod_dir.rglob("*")):
        if not path.is_file():
            continue
        relative_path = path.relative_to(mod_dir).as_posix()
        label, pack_kind, compress = _classify_fo4_starfield_path(relative_path)
        destination = (
            "loose"
            if label == "loose"
            else _fo4_starfield_archive_name(flags.mod_name, label)
        )
        jobs.append(PackJob(relative_path, path, destination, pack_kind, compress))
    return jobs


def pack_fo4_starfield_archives(
    mod_root: Path,
    *,
    mod_name: str | None = None,
    archive_max_bytes: int = DEFAULT_ARCHIVE_MAX_BYTES,
    archive_output_dir: Path | None = None,
    expanded_archives: bool = False,
    ba2_compression_level: int | None = None,
    pack_progress: Callable[[dict], bool | None] | None = None,
) -> list[PlannedArchive]:
    """The fo4:starfield packaging tail.

    Packs ``terrain/*.btd`` (plus meshes/geometries/sound) uncompressed into
    ``<mod_name> - Main.ba2`` and textures (compressed) into
    ``<mod_name> - Textures.ba2``; ``materials/**.mat`` is left loose.
    Asserts the produced archive name set is EXACTLY
    ``{"<mod_name> - Main.ba2", "<mod_name> - Textures.ba2"}`` and fails
    loudly instead of sharding when an archive would exceed
    ``archive_max_bytes``: Starfield never auto-loads `-Main1`/`-Main2` or
    any per-family archive for a mod plugin.
    """
    if expanded_archives:
        raise RuntimeError(
            "fo4:starfield packaging refuses expanded_archives=True (R1 "
            "SD-8): Starfield auto-loads only '<plugin> - Main.ba2' and "
            "'- Textures.ba2' for a mod plugin. Per-family archives "
            "('- Terrain.ba2', '- Meshes.ba2', '- Materials.ba2', ...) are "
            "never opened by the engine for a mod plugin — only vanilla's "
            "Starfield.ini-registered archives are — so expanding would be "
            "silent total content loss. Pass expanded_archives=False (the "
            "pin for this pair)."
        )
    mod_root = Path(mod_root)
    mod_name = mod_name or mod_root.name
    data_root = mod_root / "data"
    archive_root = (
        Path(archive_output_dir) if archive_output_dir is not None else mod_root
    )
    archive_root.mkdir(parents=True, exist_ok=True)

    required_names = {
        _fo4_starfield_archive_name(mod_name, label)
        for label in FO4_STARFIELD_PACKED_LABELS
    }

    plan = build_pack_plan(data_root, Fo4StarfieldPackFlags(mod_name=mod_name))
    by_destination: dict[str, list[PackJob]] = {}
    for job in plan:
        by_destination.setdefault(job.destination, []).append(job)
    by_destination.pop("loose", None)

    unexpected = sorted(set(by_destination) - required_names)
    if unexpected:
        raise RuntimeError(
            "fo4:starfield packaging (R1 SD-8): build_pack_plan produced "
            f"unexpected archive destination(s) {unexpected}; the only "
            f"names Starfield auto-loads for this plugin are "
            f"{sorted(required_names)}"
        )

    planned: list[PlannedArchive] = []
    for label in FO4_STARFIELD_PACKED_LABELS:
        archive_name = _fo4_starfield_archive_name(mod_name, label)
        jobs = by_destination.get(archive_name, [])
        if not jobs:
            continue
        entries = tuple(
            ArchiveEntry(
                job.relative_path, job.source_path, job.source_path.stat().st_size
            )
            for job in jobs
        )
        estimated_size = estimate_archive_size(entries)
        if estimated_size > archive_max_bytes:
            raise RuntimeError(
                f"fo4:starfield packaging (R1 SD-8): {archive_name} would be "
                f"~{estimated_size / 1024**3:.2f} GiB, over the "
                f"{archive_max_bytes / 1024**3:.0f} GiB BA2 cap. Sharding to "
                f"'{mod_name} - {label}1.ba2'/'-{label}2.ba2' is refused: "
                "those names are never auto-loaded by Starfield for a mod "
                "plugin. Re-run with a smaller --terrain-extent-cells crop, "
                "or deploy this mod with loose files (--deploy-loose) "
                "instead of packed archives."
            )
        output_path = archive_root / archive_name
        texture_archive = label == FO4_STARFIELD_TEXTURES_LABEL
        _run_native_pack_entries(
            entries,
            str(output_path),
            "starfield",
            texture_archive=texture_archive,
            compress=jobs[0].compress,
            compression_level=(
                ba2_compression_level if jobs[0].compress else None
            ),
        )
        _validate_archive_size(output_path, archive_max_bytes)
        planned.append(
            PlannedArchive(
                family=label,
                label=label,
                output_name=archive_name,
                entries=entries,
                texture_archive=texture_archive,
            )
        )
        if pack_progress is not None:
            pack_progress(
                {
                    "message": f"Archive packed native: name={archive_name} files={len(entries)}",
                    "total": len(FO4_STARFIELD_PACKED_LABELS),
                    "completed": len(planned),
                }
            )

    produced_names = {p.output_name for p in planned}
    if produced_names != required_names:
        raise RuntimeError(
            "fo4:starfield packaging (R1 SD-8): produced archive set "
            f"{sorted(produced_names)} != the required "
            f"{sorted(required_names)} — Starfield only auto-loads "
            "'- Main.ba2'/'- Textures.ba2' for a mod plugin; a missing "
            "archive here means an asset class silently failed to convert."
        )

    # Stale-output cleanup: remove any leftover archive for this mod (e.g. a
    # per-family archive from a prior expanded_archives=True run) that isn't
    # one of the two names this pair is allowed to produce.
    for archive in discover_mod_archives(archive_root, mod_name, extensions=(".ba2",)):
        if archive.name not in produced_names:
            archive.unlink()

    return planned


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
    report = _timing_report(ctx)
    if fields.get("scope") == "detail" and "offset_seconds" not in fields:
        fields["offset_seconds"] = max(started_at - report.started_at, 0.0)
    report.record(name, time.perf_counter() - started_at, **fields)


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
        "after:collect_assets",
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


def _trim_bacup_native_allocator() -> None:
    try:
        from bacup_lib.esp_native_runtime import load_esp_native

        trim = getattr(load_esp_native(), "trim_allocator_native", None)
        if callable(trim):
            trim()
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


# Statuses meaning "the toolchain was unusable", not "this script is bad".
_INFRASTRUCTURE_FAILURE_STATUSES = frozenset({"compiler_unavailable"})

@dataclass(slots=True)
class _ScriptResolution:
    script_name: str
    status: str
    pex_path: Path | None = None
    message: str = ""

    @property
    def ok(self) -> bool:
        return self.status in {"target", "compiled"}

    @property
    def is_infrastructure_failure(self) -> bool:
        """The toolchain never ran, so this says nothing about the script.

        Stripping a record's VMAD is only defensible when *that script* failed
        to compile. A missing compiler fails every script at once, and treating
        that as 6742 individual script failures is what silently gutted a
        shipped ESM of 13624 bindings.
        """
        return self.status in _INFRASTRUCTURE_FAILURE_STATUSES


def _assert_script_toolchain_healthy(resolutions: Iterable[_ScriptResolution]) -> None:
    load_native_module().script_toolchain_assert_healthy(json.dumps([
        {"script_name": r.script_name, "status": r.status,
         "pex_path": str(r.pex_path) if r.pex_path is not None else None,
         "message": r.message or ""} for r in resolutions
    ]))


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
_SCRIPT_ADDITION_DIR = Path(__file__).resolve().parents[1] / "script_additions"
_QUEST_RUNTIME_UNSUPPORTED_MANIFEST = (
    Path(__file__).resolve().parents[1]
    / "resources"
    / "conversion"
    / "fo76_fo4_quest_runtime_support.yaml"
)
def _script_patch_source(script_name: str) -> str | None:
    return load_native_module().script_patch_source(str(_SCRIPT_PATCH_DIR), script_name)


def _script_addition_sources(
    source_game: str,
    target_game: str,
) -> dict[str, tuple[str, Path]]:
    return {
        key: (name, Path(path))
        for key, name, path in load_native_module().script_addition_sources(
            str(_SCRIPT_ADDITION_DIR), source_game, target_game
        )
    }


def _requires_script_addition(
    script_name: str,
    *,
    source_game: str,
    target_game: str,
) -> bool:
    return load_native_module().script_requires_addition(script_name, source_game, target_game)


def _install_script_addition(mod_path: Path, script_name: str, source_path: Path) -> Path:
    return Path(load_native_module().script_addition_install(str(mod_path), script_name, str(source_path)))


def _augment_fo76_to_fo4_script_skeleton(script_name: str, skeleton: str) -> str:
    return load_native_module().script_patch_augment(script_name, skeleton)


def _iter_top_level_papyrus_members(lines: list[str]) -> list[tuple[str, str, int, int]]:
    return list(load_native_module().script_patch_members("\n".join(lines)))


def _assert_patch_member_headers_recognized(patch_lines: list[str]) -> None:
    load_native_module().script_patch_analyze("\n".join(patch_lines))


def _state_rename_directives(patch_lines: list[str]) -> list[tuple[str, str]]:
    return [tuple(row) for row in json.loads(load_native_module().script_patch_analyze("\n".join(patch_lines)))["renames"]]


def _drop_member_directives(patch_lines: list[str]) -> set[str]:
    return set(json.loads(load_native_module().script_patch_analyze("\n".join(patch_lines)))["drop_members"])


def _drop_property_directives(patch_lines: list[str]) -> set[str]:
    return set(json.loads(load_native_module().script_patch_analyze("\n".join(patch_lines)))["drop_properties"])


def _script_patch_member_specs(patch_lines: list[str]) -> list[tuple[str, str, str, list[str]]]:
    return [tuple(row) for row in json.loads(load_native_module().script_patch_analyze("\n".join(patch_lines)))["members"]]


def _assert_script_patch_members_merged(patch: str, merged: str) -> None:
    load_native_module().script_patch_validate_merged(patch, merged)


def _merge_script_method_patches(skeleton: str, patch: str) -> str:
    return load_native_module().script_patch_merge(skeleton, patch)


def _assert_compiled_script_patch_members(pex_path: Path, patch: str) -> None:
    load_native_module().script_patch_validate_compiled(str(pex_path), patch)


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


def _path_through_extension(text: str, extension: str) -> str:
    end = text.lower().find(extension.lower())
    return text[: end + len(extension)] if end >= 0 else _path_before_error(text)


def _is_bgsm_path(path: str) -> bool:
    return ".bgsm" in path.lower()


def _exact_mvp_creature_profile(ctx) -> str | None:
    if getattr(ctx, "mvp_creature_corpus_policy", None) is not None:
        return None
    return getattr(ctx, "mvp_creature_profile", None)


class _UnifiedRecordRuntime:
    def __init__(self, request: PluginPortRequest) -> None:
        self._req = request
        self._registry = ConvertedPluginRegistry()
        self._aggregate_summary = ConversionSummary(mod_path=str(request.output_root))
        self._run_result = RunResult()
        self._source_asset_inventory = None

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

    def _merge_addon_index_map(self, ctx: ConversionContext, decisions: list) -> None:
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
        for warning, count in Counter(warnings).items():
            message = warning if count == 1 else f"{warning} ({count} occurrences)"
            runner.emit_log("WARN", message)
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

    def _check_output_runtime_hazards(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> None:
        output_path = ctx.mod_path / ctx.output_plugin_name
        if not output_path.is_file():
            raise FileNotFoundError(
                f"missing output plugin for runtime-hazard scan: {output_path}"
            )

        from creation_lib.esp.editor.runtime_hazards import (
            FO4_TARGET_SHAPE_PROFILE,
            scan_runtime_hazards_path,
        )

        started_at = time.perf_counter()
        runner.emit_log(
            "INFO", f"Checking FO4 target shapes in {ctx.output_plugin_name}"
        )
        report = scan_runtime_hazards_path(
            output_path,
            game=self._req.target_game,
            profile=FO4_TARGET_SHAPE_PROFILE,
        )

        runner.emit_log(
            "INFO",
            f"FO4 target-shape check finished for {ctx.output_plugin_name}: "
            f"hazards={len(report)} "
            f"elapsed={_format_elapsed_seconds(_elapsed_seconds(started_at))}",
        )
        for hazard in report.hazards[:25]:
            runner.emit_log("ERROR", f"runtime hazard: {hazard.message}")
        if len(report) > 25:
            runner.emit_log(
                "ERROR", f"runtime hazard: additional hazards omitted: {len(report) - 25}"
            )
        if report.hazards:
            raise RuntimeError(
                f"FO4 target-shape check failed for {ctx.output_plugin_name}: "
                f"{len(report)} runtime hazard(s)"
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

        master_started = time.perf_counter()
        target_master_paths = _conversion_target_master_inputs(self._req)
        record_setup_timing("setup_master_inputs", master_started)
        maps_started = time.perf_counter()
        material_overrides = load_translation_map_overrides(
            self._req.source_game, self._req.target_game
        )
        record_setup_timing("setup_translation_maps", maps_started)
        target_asset_store = None
        if self._req.target_game == "fo4" and self._req.target_data_dir is not None:
            store_started = time.perf_counter()
            target_asset_store = build_target_asset_store(
                target_data_dir=self._req.target_data_dir,
                catalog_path=self._req.target_asset_catalog_path,
                cache_dir=self._req.target_asset_cache_dir,
                overlay_dir=self._req.target_extracted_dir,
            )
            store_timings = target_asset_store.open_timings() if target_asset_store else {}
            record_setup_timing(
                "setup_target_asset_store", store_started, components=store_timings
            )
            _safe_emit_log(
                runner, "INFO", f"Target asset store timings: {store_timings}"
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
            asset_count = int(target_asset_index.asset_count)
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
                source_archive_dirs=tuple(dict.fromkeys(path.parent for path in self._req.source_plugins)),
                additional_source_asset_roots=tuple(
                    Path(root) for root in self._req.additional_source_asset_roots
                ),
                skyrim_quest_runtime_converted_asset_roots=tuple(
                    Path(root)
                    for root in self._req.skyrim_quest_runtime_converted_asset_roots
                ),
                legacy_music_tracks=tuple(self._req.legacy_music_tracks),
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
                native_assets=self._req.options.native_assets,
                replace_fo4_materials=self._req.options.replace_fo4_materials,
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
        from creation_lib.esp.plugin import Plugin

        opts = self._req.options
        setattr(ctx, "source_asset_inventory", None)
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

        signatures = load_native_module().SourceAssetInventory.asset_signatures(
            enabled_kinds
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

        inventory = load_native_module().SourceAssetInventory()
        self._source_asset_inventory = inventory
        try:
            assets = self._resolve_and_expand_native_assets(
                raw_assets, source_plugin, ctx, runner
            )
            if bool(getattr(ctx, "is_whole_plugin", False)) and (
                opts.convert_textures or opts.convert_materials or opts.convert_havok
            ):
                setattr(ctx, "source_asset_inventory", inventory)
            return assets
        except BaseException:
            setattr(ctx, "source_asset_inventory", None)
            raise
        finally:
            self._source_asset_inventory = None

    def _resolve_and_expand_native_assets(
        self,
        raw_assets: list,
        source_plugin: Path,
        ctx: ConversionContext,
        runner: "ConversionRunner | None" = None,
    ) -> list[AssetRef]:
        options = self._native_asset_collection_options(source_plugin, ctx)
        options["legacy_music_tracks"] = self._legacy_music_track_manifest(ctx)
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if self._req.options.synthesize_object_lod and rust_run is not None:
            started = time.perf_counter()
            try:
                options["lod_rows"] = rust_run.collect_lod_closures()
            except Exception as exc:
                _safe_emit_log(
                    runner, "WARN", f"LOD-convention asset collection skipped: {exc}"
                )
            _record_timing(ctx, "collect_assets_lod_convention", started)
        inventory = (
            self._source_asset_inventory or load_native_module().SourceAssetInventory()
        )
        started = time.perf_counter()
        result = json.loads(
            inventory.collect_assets_json(json.dumps(raw_assets), json.dumps(options))
        )
        report = _timing_report(ctx)
        for timing in result["timings"]:
            report.record(timing.pop("name"), timing.pop("elapsed_seconds"), **timing)
        for level, message in result["logs"]:
            _safe_emit_log(runner, level, message)
        assets = [self._asset_ref_from_collection_item(row) for row in result["assets"]]
        _record_timing(
            ctx, "collect_assets_native_expansion", started, assets=len(assets)
        )
        _safe_emit_log(runner, "INFO", f"Source asset inventory: {inventory.stats()}")
        return assets

    def _native_asset_collection_options(self, source_plugin: Path, ctx) -> dict:
        opts = self._req.options
        return {
            "source_game": self._req.source_game,
            "target_game": self._req.target_game,
            "source_plugin": str(source_plugin),
            "source_extracted_dir": (
                str(ctx.source_extracted_dir)
                if getattr(ctx, "source_extracted_dir", None) else None
            ),
            "source_data_dir": (
                str(ctx.source_data_dir)
                if getattr(ctx, "source_data_dir", None) else None
            ),
            "extracted_dir": (
                str(ctx.extracted_dir) if getattr(ctx, "extracted_dir", None) else None
            ),
            "additional_source_asset_roots": [
                str(root)
                for root in getattr(ctx, "additional_source_asset_roots", ()) or ()
            ],
            "output_plugin_name": str(getattr(ctx, "output_plugin_name", "") or ""),
            "full_plugin_nifs": (
                bool(getattr(ctx, "is_whole_plugin", False)) and opts.cell_bounds is None
            ),
            "nifs": bool(opts.convert_nifs),
            "sounds": bool(opts.copy_sounds),
            "havok": bool(opts.convert_havok or opts.convert_animations),
            "textures": bool(opts.convert_textures),
            "workers": int(getattr(ctx, "conversion_workers", None) or 1),
            "mvp_creature_profile": getattr(ctx, "mvp_creature_profile", None),
            "mvp_creature_corpus_policy": getattr(ctx, "mvp_creature_corpus_policy", None),
        }

    @staticmethod
    def _asset_ref_from_collection_item(row: dict) -> AssetRef:
        row["provenance"] = (
            AssetProvenance(**row["provenance"])
            if row["provenance"] is not None else None
        )
        point = row["workshop_wire_point"]
        row["workshop_wire_point"] = tuple(point) if point is not None else None
        return AssetRef(**row)

    def _augment_weapon_mvp_asset_closure(
        self,
        assets: list[AssetRef],
        source_plugin: Path,
        ctx: ConversionContext,
        runner: "ConversionRunner | None" = None,
    ) -> list[AssetRef]:
        from bacup_lib.weapon_mvp_assets import weapon_mvp_asset_closure

        closure = weapon_mvp_asset_closure(
            self._req.source_game,
            self._req.target_game,
            assets,
        )
        if closure is None:
            return assets
        activator = closure.activating_assets(assets)[0]
        provenance = getattr(activator, "provenance", None)
        expanded = list(assets)
        for asset in expanded:
            if not closure.allows(asset) or asset.resolved_path:
                continue
            asset.resolved_path, asset.resolution_error = (
                self._resolve_native_asset_path(
                    asset.asset_type,
                    asset.source_path,
                    source_plugin,
                    ctx,
                    source_plugin_names=(closure.activators[0].source_plugin,),
                )
            )
        for asset_type, source_path in closure.missing_entries(expanded):
            resolved_path, resolution_error = self._resolve_native_asset_path(
                asset_type,
                source_path,
                source_plugin,
                ctx,
                source_plugin_names=(closure.activators[0].source_plugin,),
            )
            expanded.append(
                AssetRef(
                    asset_type=asset_type,
                    source_path=source_path,
                    resolved_path=resolved_path,
                    resolution_error=resolution_error,
                    provenance=AssetProvenance(
                        added_by_record_fk=str(
                            getattr(provenance, "added_by_record_fk", "") or ""
                        ),
                        added_by_record_eid=str(
                            getattr(provenance, "added_by_record_eid", "") or ""
                        ),
                        added_by_field="NIF texture closure",
                        walk_depth=0,
                        walker_pass="weapon_mvp_asset_closure",
                        added_by_record_sig=str(
                            getattr(provenance, "added_by_record_sig", "") or ""
                        ),
                    ),
                )
            )
        added = len(expanded) - len(assets)
        if added:
            _safe_emit_log(
                runner,
                "INFO",
                f"Expanded {added} exact {closure.label} MVP asset(s)",
            )
        return expanded

    def _augment_fnv_weapon_mvp_asset_closure(
        self,
        assets: list[AssetRef],
        source_plugin: Path,
        ctx: ConversionContext,
        runner: "ConversionRunner | None" = None,
    ) -> list[AssetRef]:
        return self._augment_weapon_mvp_asset_closure(
            assets, source_plugin, ctx, runner
        )

    def _resolve_native_asset_path(
        self,
        asset_type: str,
        source_path: str,
        source_plugin: Path,
        ctx: ConversionContext,
        *,
        source_plugin_names: tuple[str, ...] = (),
    ) -> tuple[str | None, str | None]:
        inventory = (
            self._source_asset_inventory or load_native_module().SourceAssetInventory()
        )
        return inventory.resolve_asset(
            asset_type,
            source_path,
            json.dumps(self._native_asset_collection_options(source_plugin, ctx)),
            list(source_plugin_names),
        )

    def _legacy_music_track_manifest(
        self,
        ctx: ConversionContext,
    ) -> list[dict[str, str]]:
        cached = getattr(ctx, "_legacy_music_track_manifest", None)
        if cached is not None:
            return cached

        configured = list(getattr(ctx, "legacy_music_tracks", ()) or ())
        if configured:
            setattr(ctx, "_legacy_music_track_manifest", configured)
            return configured

        from bacup_lib.legacy_music import discover_legacy_music_tracks

        primary_roots: list[Path] = []
        for value in (
            getattr(ctx, "source_data_dir", None),
            getattr(ctx, "source_extracted_dir", None),
            self._req.source_data_dir,
        ):
            if value:
                primary_roots.append(Path(value))
        manifest = discover_legacy_music_tracks(
            source_game=self._req.source_game,
            target_game=self._req.target_game,
            output_plugin_name=str(
                getattr(ctx, "output_plugin_name", "") or "Converted.esm"
            ),
            primary_roots=primary_roots,
            additional_roots=(
                Path(root)
                for root in getattr(ctx, "additional_source_asset_roots", ()) or ()
            ),
        )
        setattr(ctx, "_legacy_music_track_manifest", manifest)
        return manifest

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
        fnv_quest_slice = self._is_fnv_quest_slice()
        exclude_signatures = getattr(opts, "exclude_signatures", frozenset())
        requested_exclude_signatures = frozenset(exclude_signatures)
        quest_slice_records: dict[str, list[int]] = {}
        if (
            self._req.source_game.lower() == "starfield"
            and self._req.target_game.lower() == "fo4"
        ):
            # starfield:fo4 has no non-world-only mode; enforce the
            # fence here so callers that forgot --mvp / the UI override still
            # get it, matching the fnv/skyrim quest-slice blocks' shape below.
            from bacup_lib.source_pairs import STARFIELD_MVP_EXCLUDE_SIGNATURES

            exclude_signatures = (
                frozenset(exclude_signatures) | STARFIELD_MVP_EXCLUDE_SIGNATURES
            )
        if fnv_quest_slice:
            if not opts.convert_placed_records:
                raise RuntimeError(
                    "strict FNV quest slice requires placed-record conversion"
                )
            from bacup_lib.source_pairs import (
                FNV_MVP_EXCLUDE_SIGNATURES,
                FNV_QUEST_SLICE_EXCLUDE_SIGNATURES,
                FNV_QUEST_SLICE_RECORD_FORM_IDS,
                fnv_quest_slice_record_form_ids,
            )

            normalized_excludes = frozenset(
                str(signature).strip().upper() for signature in exclude_signatures
            )
            conflicts = sorted(
                frozenset(FNV_QUEST_SLICE_RECORD_FORM_IDS)
                & (normalized_excludes - FNV_QUEST_SLICE_EXCLUDE_SIGNATURES)
            )
            if conflicts:
                raise RuntimeError(
                    "strict FNV quest slice excludes required record signatures: "
                    + ", ".join(conflicts)
                )
            exclude_signatures = (
                frozenset(exclude_signatures) | FNV_MVP_EXCLUDE_SIGNATURES
            )
            quest_slice_records = fnv_quest_slice_record_form_ids()
        from bacup_lib.source_pairs import (
            mvp_creature_corpus_policy_payload,
            mvp_creature_profile_payload,
            mvp_melee_policy_payload,
            mvp_record_exception_payload,
        )

        pair_id = {
            ("fnv", "fo4"): "fnvfo3:fo4",
            ("skyrimse", "fo4"): "skyrimse:fo4",
        }.get((self._req.source_game.lower(), self._req.target_game.lower()))
        mvp_melee_policy = (
            mvp_melee_policy_payload(pair_id, requested_exclude_signatures)
            if pair_id is not None and not fnv_quest_slice
            else None
        )
        mvp_record_exceptions = (
            []
            if mvp_melee_policy is not None
            else (
                mvp_record_exception_payload(pair_id, requested_exclude_signatures)
                if pair_id is not None and not fnv_quest_slice
                else []
            )
        )
        mvp_creature_corpus_policy = (
            mvp_creature_corpus_policy_payload(pair_id, requested_exclude_signatures)
            if pair_id is not None
            and not fnv_quest_slice
            and not getattr(opts, "mvp_melee_only", False)
            else None
        )
        mvp_creature_profile = (
            mvp_creature_profile_payload(pair_id, requested_exclude_signatures)
            if pair_id is not None
            and not fnv_quest_slice
            and not getattr(opts, "mvp_melee_only", False)
            and mvp_creature_corpus_policy is None
            else None
        )
        if getattr(opts, "mvp_melee_only", False) and mvp_melee_policy is None:
            raise RuntimeError(
                "melee-only conversion requires the complete bulk_melee_v1 MVP fence"
            )
        ctx.mvp_record_exceptions = mvp_record_exceptions
        ctx.mvp_melee_policy = mvp_melee_policy
        ctx.mvp_creature_corpus_policy = mvp_creature_corpus_policy
        ctx.mvp_creature_profile = mvp_creature_profile
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
            "overwrite_existing": bool(getattr(ctx, "overwrite_existing", False)),
            "use_base_game_assets": True,
            "is_whole_plugin": bool(getattr(ctx, "is_whole_plugin", False)),
            "root_sig": None,
            "mod_path": str(getattr(ctx, "mod_path", "") or ""),
            "source_extracted_dir": str(source_extracted) if source_extracted else None,
            "additional_source_asset_roots": [
                str(root)
                for root in getattr(ctx, "additional_source_asset_roots", ()) or ()
            ],
            "legacy_music_tracks": self._legacy_music_track_manifest(ctx),
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
            "native_assets": bool(getattr(opts, "native_assets", False)),
            "replace_fo4_materials": bool(getattr(opts, "replace_fo4_materials", False)),
            "warning_policy": "warn_playable",
            "skip_record_signatures": _skip_record_signatures_payload(
                convert_placed_records=opts.convert_placed_records,
                exclude_signatures=exclude_signatures,
            ),
            "fnv_quest_slice": fnv_quest_slice,
            "mvp_melee_only": bool(getattr(opts, "mvp_melee_only", False)),
            "fnv_quest_slice_records": quest_slice_records,
            "mvp_record_exceptions": mvp_record_exceptions,
            "fnv_force_greet_donor_form_key": (
                "05B307@Fallout4.esm" if fnv_quest_slice else None
            ),
            "defer_placed_child_ref_class": self._defers_placed_child_ref_class(ctx),
            "legacy_pack_origins": [
                row.to_dict() for row in self._req.legacy_pack_origins
            ],
            "legacy_runtime_origins": [
                dict(row)
                for row in getattr(self._req, "legacy_runtime_origins", ()) or ()
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

    @staticmethod
    def _mvp_melee_asset_receipt_rows(
        metadata_rows: Iterable[Mapping[str, object]],
        policy: Mapping[str, object],
    ) -> tuple[dict[str, object], ...]:
        policy_id = str(policy.get("policy_id") or "")
        receipt_rows: list[dict[str, object]] = []
        for metadata in metadata_rows:
            row = dict(metadata)
            native_status = str(row.get("status") or "").casefold()
            row["policy"] = str(row.get("policy") or policy_id)
            row["role"] = str(row.get("role") or row.get("weapon_role") or "unknown")

            if native_status not in {"admitted", "rejected"}:
                row["status"] = "rejected"
                row["reason_code"] = "missing_native_admission_receipt"
            elif native_status == "admitted":
                target_profile = str(row.get("target_profile") or "").casefold()
                first_person_resolution = str(
                    row.get("first_person_resolution") or ""
                ).casefold()
                if target_profile in _MVP_MELEE_TARGET_PROFILES:
                    row["role"] = "melee"
                record_only = (
                    target_profile != "unarmed"
                    and first_person_resolution == "none"
                    and not str(row.get("world_model") or "")
                    and not str(row.get("first_person_model") or "")
                )
                if not target_profile:
                    row["status"] = "rejected"
                    row["reason_code"] = "missing_target_profile"
                elif target_profile not in _MVP_MELEE_TARGET_PROFILES:
                    row["status"] = "rejected"
                    row["reason_code"] = "invalid_target_profile"
                elif record_only:
                    row["asset_disposition"] = "record_only"
                elif target_profile != "unarmed" and not str(
                    row.get("world_model") or ""
                ):
                    row["status"] = "rejected"
                    row["reason_code"] = "missing_world_model"
                elif target_profile != "unarmed" and not str(
                    row.get("first_person_model") or ""
                ):
                    row["status"] = "rejected"
                    row["reason_code"] = "missing_first_person_model"
            receipt_rows.append(row)
        return tuple(receipt_rows)

    def _run_mvp_melee_phase(
        self,
        run,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> dict[str, object] | None:
        policy = getattr(ctx, "mvp_melee_policy", None)
        if policy is None:
            return None
        if str(policy.get("policy_id") or "") != "bulk_melee_v1":
            raise RuntimeError("unsupported MVP melee policy")

        started = time.perf_counter()
        report = dict(
            run.run_phase(
                "mvp_melee",
                mod_path="",
                params={"policy": policy},
            )
            or {}
        )
        if "records_added" not in report or "records_dropped" not in report:
            raise RuntimeError("mvp_melee returned an unaccounted phase report")

        from bacup_lib.weapon_mvp_assets import build_weapon_mvp_asset_plan
        from bacup_lib.weapon_report import weapon_metadata_rows

        metadata_source = SimpleNamespace(
            graph=SimpleNamespace(all_records=[]),
            _rust_conversion_run=run,
            _record_type_signature=lambda record_type: record_type,
        )
        native_rows = weapon_metadata_rows(metadata_source)
        records_added = int(report.get("records_added", 0) or 0)
        records_dropped = int(report.get("records_dropped", 0) or 0)
        if records_added + records_dropped != len(native_rows):
            raise RuntimeError(
                "mvp_melee report does not account for its source corpus: "
                f"metadata={len(native_rows)} added={records_added} "
                f"dropped={records_dropped}"
            )

        native_receipt_complete = all(
            str(row.get("status") or "").casefold() in {"admitted", "rejected"}
            for row in native_rows
        )
        native_admitted = sum(
            str(row.get("status") or "").casefold() == "admitted" for row in native_rows
        )
        native_rejected = sum(
            str(row.get("status") or "").casefold() == "rejected" for row in native_rows
        )
        invalid_output_profiles = sorted(
            {
                str(row.get("target_profile") or "").casefold()
                for row in native_rows
                if str(row.get("status") or "").casefold() == "admitted"
                and str(row.get("target_profile") or "").casefold()
                not in _MVP_MELEE_TARGET_PROFILES
            }
        )
        ranged_output = sum(
            str(row.get("status") or "").casefold() == "admitted"
            and str(row.get("target_profile") or "").casefold()
            not in _MVP_MELEE_TARGET_PROFILES
            for row in native_rows
        )
        if ranged_output:
            raise RuntimeError(
                "mvp_melee violated ranged_output=0: "
                f"admitted={ranged_output} target_profiles={invalid_output_profiles}"
            )
        if native_receipt_complete and (
            native_admitted != records_added or native_rejected != records_dropped
        ):
            raise RuntimeError(
                "mvp_melee metadata receipt disagrees with its phase report: "
                f"receipt_admitted={native_admitted} report_added={records_added} "
                f"receipt_rejected={native_rejected} report_dropped={records_dropped}"
            )

        receipt_rows = self._mvp_melee_asset_receipt_rows(native_rows, policy)
        asset_plan = build_weapon_mvp_asset_plan(
            str(ctx.source_game),
            str(ctx.target_game),
            getattr(ctx, "assets", ()),
            receipt_rows,
        )
        ctx._weapon_metadata_rows = receipt_rows
        ctx._weapon_mvp_asset_plan = asset_plan
        ctx.native_mvp_melee_stats = {
            "candidates": len(native_rows),
            "records_added": records_added,
            "records_dropped": records_dropped,
            "receipt_complete": native_receipt_complete,
            "asset_admitted": asset_plan.admitted_count,
            "asset_rejected": asset_plan.rejected_count,
            "asset_conflicts": asset_plan.conflict_count,
            "ranged_output": 0,
        }
        _timing_report(ctx).record(
            "mvp_melee",
            _elapsed_seconds(started),
            **ctx.native_mvp_melee_stats,
        )
        runner.emit_log(
            "INFO",
            "Native backend: mvp_melee done "
            f"(candidates={len(native_rows)}, admitted={records_added}, "
            f"rejected={records_dropped}, asset_admitted={asset_plan.admitted_count}, "
            "ranged_output=0)",
        )
        return report

    def _run_mvp_creature_dependency_plan(
        self,
        run,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> dict[str, object] | None:
        policy = getattr(ctx, "mvp_creature_corpus_policy", None)
        if policy is None:
            return None
        if not isinstance(policy, Mapping):
            raise RuntimeError("creature corpus policy must be a typed mapping")
        if str(policy.get("policy_id") or "") != "all_creatures_v1":
            raise RuntimeError("unsupported creature corpus policy")
        missing_fields = [
            field
            for field in (
                "pair_id",
                "source_games",
                "source_plugins",
                "execution_mode",
                "debug_dir",
            )
            if not policy.get(field)
        ]
        if missing_fields:
            raise RuntimeError(
                "all_creatures_v1 policy is missing required fields: "
                + ", ".join(missing_fields)
            )
        if str(policy["execution_mode"]) != "strict":
            raise RuntimeError("all_creatures_v1 requires strict execution")
        debug_dir = str(policy["debug_dir"])
        if debug_dir != "debug/creature_corpus":
            raise RuntimeError("all_creatures_v1 has an invalid debug directory")
        source_extracted = (
            getattr(ctx, "source_data_dir", None) or self._req.source_data_dir
        )
        report = dict(
            run.run_phase(
                "plan_creature_dependencies",
                mod_path=str(ctx.mod_path),
                source_extracted_dir=(
                    str(source_extracted) if source_extracted else ""
                ),
                params={"debug_dir": debug_dir},
            )
            or {}
        )
        ctx.native_creature_dependency_stats = report
        runner.emit_log(
            "INFO",
            "Native backend: all_creatures_v1 recursive dependency allow-set "
            "planned from the retained source run",
        )
        return report

    def _run_mvp_creature_corpus_discovery(
        self,
        run,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> dict[str, object] | None:
        policy = getattr(ctx, "mvp_creature_corpus_policy", None)
        if policy is None:
            return None
        if not isinstance(policy, Mapping):
            raise RuntimeError("creature corpus policy must be a typed mapping")
        if str(policy.get("policy_id") or "") != "all_creatures_v1":
            raise RuntimeError("unsupported creature corpus policy")
        missing_fields = [
            field
            for field in (
                "pair_id",
                "source_games",
                "source_plugins",
                "execution_mode",
                "debug_dir",
            )
            if not policy.get(field)
        ]
        if missing_fields:
            raise RuntimeError(
                "all_creatures_v1 policy is missing required fields: "
                + ", ".join(missing_fields)
            )
        if str(policy["execution_mode"]) != "strict":
            raise RuntimeError("all_creatures_v1 requires strict execution")
        debug_dir = str(policy["debug_dir"])
        if debug_dir != "debug/creature_corpus":
            raise RuntimeError("all_creatures_v1 has an invalid debug directory")
        source_extracted = (
            getattr(ctx, "source_data_dir", None) or self._req.source_data_dir
        )
        from bacup_lib.workflows.asset_phases import fo4_behavior_contracts_root

        target_extracted = fo4_behavior_contracts_root(
            SimpleNamespace(
                target_extracted_dir=getattr(ctx, "target_extracted_dir", None) or self._req.target_extracted_dir,
                target_asset_store=getattr(ctx, "target_asset_store", None),
            ),
            runner,
        )
        target_data = getattr(ctx, "target_data_dir", None) or self._req.target_data_dir
        report = dict(
            run.run_phase(
                "discover_creature_corpus",
                mod_path=str(ctx.mod_path),
                source_extracted_dir=(
                    str(source_extracted) if source_extracted else ""
                ),
                target_extracted_dir=(
                    str(target_extracted) if target_extracted else None
                ),
                target_data_dir=str(target_data) if target_data else None,
                params={
                    "profile": "all_creatures_v1",
                    "debug_dir": debug_dir,
                },
            )
            or {}
        )
        ctx.native_creature_corpus_discovery_stats = report
        ctx._creature_corpus_discovered = True
        prepared_job_count = int(report.get("records_deferred", 0) or 0)
        ctx._creature_corpus_has_jobs = prepared_job_count > 0
        runner.emit_log(
            "INFO",
            "Native backend: all_creatures_v1 corpus discovery completed on the "
            f"retained source run ({prepared_job_count} native prepared jobs)",
        )
        return report

    def _run_mvp_creature_corpus_execution(
        self,
        run,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> dict[str, object] | None:
        policy = getattr(ctx, "mvp_creature_corpus_policy", None)
        if policy is None:
            return None
        if str(policy.get("policy_id") or "") != "all_creatures_v1":
            raise RuntimeError("unsupported creature corpus policy")
        if not getattr(ctx, "_creature_corpus_discovered", False):
            raise RuntimeError(
                "all_creatures_v1 corpus discovery did not complete on the retained "
                "source run"
            )
        if getattr(ctx, "_creature_corpus_executed", False):
            raise RuntimeError("all_creatures_v1 corpus execution already completed")
        if not getattr(ctx, "_creature_corpus_has_jobs", False):
            ctx._creature_corpus_executed = True
            return None

        source_extracted = (
            getattr(ctx, "source_data_dir", None) or self._req.source_data_dir
        )
        from bacup_lib.workflows.asset_phases import fo4_behavior_contracts_root

        target_extracted = fo4_behavior_contracts_root(
            SimpleNamespace(
                target_extracted_dir=getattr(ctx, "target_extracted_dir", None) or self._req.target_extracted_dir,
                target_asset_store=getattr(ctx, "target_asset_store", None),
            ),
            runner,
        )
        target_data = getattr(ctx, "target_data_dir", None) or self._req.target_data_dir
        report = dict(
            run.run_phase(
                "execute_creature_corpus",
                mod_path=str(ctx.mod_path),
                source_extracted_dir=(
                    str(source_extracted) if source_extracted else ""
                ),
                target_extracted_dir=(
                    str(target_extracted) if target_extracted else None
                ),
                target_data_dir=str(target_data) if target_data else None,
                params={
                    "plan_path": f"{policy['debug_dir']}/plan.json",
                    "mode": str(policy["execution_mode"]),
                },
            )
            or {}
        )
        ctx.native_creature_corpus_execution_stats = report
        ctx._creature_corpus_executed = True
        runner.emit_log(
            "INFO",
            "Native backend: all_creatures_v1 corpus executed on the retained "
            "record run",
        )
        return report

    def _translate_records_rust(
        self,
        records: list[RecordNode],
        ctx: ConversionContext,
        runner: "ConversionRunner",
        progress: "PhaseProgress",
    ) -> None:
        """Run the translate-records phase via the native Rust ConversionRun pipeline.

        The ConversionRun is stashed on ``ctx._rust_conversion_run`` and outlives
        this phase so the optional FNV-legacy phase can reuse its mapper state.
        The ``finally`` in ``_convert_one_plugin`` drains the decisions/warnings
        both phases accumulate on the Rust side and drops the run.
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
        # The run is gone by the post phases; the material upgrade must skip exactly this set.
        reader = getattr(run, "native_overwrite_paths", None)
        ctx.native_overwrite_paths = list(reader()) if callable(reader) else []  # type: ignore[attr-defined]
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
        self._prepare_skyrim_minimal_quest_actions(
            run=run,
            ctx=ctx,
            runner=runner,
        )
        if ctx.source_game.lower() == "starfield" and ctx.target_game.lower() == "fo4":
            terrain_dir = config.get("source_extracted_dir")
            ctx.native_starfield_weapon_plan_stats = dict(
                run.run_phase(
                    "plan_starfield_weapons",
                    mod_path=str(ctx.mod_path),
                    source_extracted_dir=str(terrain_dir or ""),
                    params={},
                )
                or {}
            )
            runner.emit_log(
                "INFO",
                "Starfield weapon dependency contracts saved; incomplete weapons remain deferred",
            )
            if not terrain_dir:
                raise RuntimeError(
                    "starfield:fo4 requires a resolved source_extracted_dir to "
                    "run the starfield_worldspace_gate phase"
                )
            gate_started = time.perf_counter()
            gate_report = run.run_phase(
                "starfield_worldspace_gate",
                mod_path="",
                params={"terrain_dir": str(Path(terrain_dir) / "terrain")},
            )
            _record_timing(ctx, "starfield_worldspace_gate", gate_started)
            runner.emit_log(
                "INFO",
                "Native backend: starfield_worldspace_gate dropped "
                f"{gate_report.get('records_dropped', 0)} record(s) "
                "(worldspace without baked terrain); calling translate_all",
            )
        else:
            runner.emit_log(
                "INFO",
                f"Native backend: ConversionRun created (id={run.id}); calling translate_all",
            )
        self._run_mvp_creature_dependency_plan(run, ctx, runner)
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
            f"failed={stats.get('records_failed', 0)}",
        )
        self._compile_and_attach_skyrim_minimal_quests(
            run=run,
            ctx=ctx,
            runner=runner,
        )
        self._copy_skyrim_quest_runtime_assets(
            run=run,
            ctx=ctx,
            runner=runner,
        )
        if ctx.source_game.lower() == "fnv" and ctx.target_game.lower() == "fo4":
            legacy_started = time.perf_counter()
            if self._is_fnv_world_only_mvp():
                runner.emit_log(
                    "INFO",
                    "FNV world-only MVP: skipping legacy scripting and PSC emission",
                )
            else:
                runner.emit_log(
                    "INFO",
                    "Native backend: translate_all done; calling FNV legacy scripting "
                    "before Story Manager and fixups",
                )
                self._run_fnv_legacy_phase_from_rust_run(
                    ctx,
                    Path(source_plugin_path),
                    runner,
                    run,
                )
            ctx._fnv_legacy_completed = True
            _timing_report(ctx).record(
                "fnv_legacy_scripting",
                _elapsed_seconds(legacy_started),
                phase_order="after_translate_v2_before_story_manager",
            )
        runner.emit_log("INFO", "Native backend: calling Story Manager subset")
        story_manager_started = time.perf_counter()
        story_manager_report = run.run_phase(
            "emit_story_manager_subset", mod_path="", params={}
        )
        story_manager_elapsed = _elapsed_seconds(story_manager_started)
        story_manager_added = int(story_manager_report.get("records_added", 0))
        story_manager_changed = int(story_manager_report.get("records_changed", 0))
        story_manager_skipped = int(story_manager_report.get("records_dropped", 0))
        ctx.native_story_manager_stats = dict(story_manager_report)
        stats["records_translated"] = (
            int(stats.get("records_translated", 0)) + story_manager_added
        )
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
        fixups_v2_report = run.run_phase("fixups_v2",
            mod_path="",
            params={
                "defer_havok_postprocess": _has_havok_postprocess(ctx, self._req.options),
            },
        )
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
        mvp_melee_report = self._run_mvp_melee_phase(run, ctx, runner)
        if mvp_melee_report is not None:
            stats["records_translated"] = int(stats.get("records_translated", 0)) + int(
                mvp_melee_report.get("records_added", 0) or 0
            )
            stats["records_dropped"] = int(stats.get("records_dropped", 0)) + int(
                mvp_melee_report.get("records_dropped", 0) or 0
            )
        if ctx.source_game.lower() == "fo76" and ctx.target_game.lower() == "fo4":
            areas_report = run.run_phase("emit_objective_areas", mod_path=str(ctx.mod_path), params={})
            runner.emit_log("INFO", f"Quest objective areas: {areas_report.get('assets_written', 0)} catalog(s), {areas_report.get('warnings', 0)} skipped entries")
            fishing_started = time.perf_counter()
            fishing_report = run.run_phase(
                "emit_fishing_catalog", mod_path=str(ctx.mod_path), params={}
            )
            bullion_report = run.run_phase("emit_gold_bullion", mod_path=str(ctx.mod_path), params={})
            runner.emit_log("INFO", f"Gold bullion: {bullion_report.get('assets_written', 0)} price catalog(s), {bullion_report.get('warnings', 0)} unmapped items")
            imagespace_report = run.run_phase("emit_imagespace_data", mod_path=str(ctx.mod_path), params={})
            runner.emit_log("INFO", f"Image-space data: {imagespace_report.get('warnings', 0)} unmapped image spaces")
            volume_report = run.run_phase("emit_reflection_volume_data", mod_path=str(ctx.mod_path), params={})
            runner.emit_log("INFO", f"Reflection volume data: {volume_report.get('records_changed', 0)} cube-map volume(s)")
            _record_timing(ctx, "emit_fishing_catalog", fishing_started)
            runner.emit_log(
                "INFO", "Fishing source catalog: "
                f"{int(fishing_report.get('assets_written', 0))} file(s) preserved",
            )
            perk_report = run.run_phase("emit_perk_cards", mod_path=str(ctx.mod_path), params={})
            runner.emit_log("INFO", f"Perk cards: {perk_report.get('assets_written', 0)} catalog(s), {perk_report.get('warnings', 0)} unresolved bindings/curves")
            model_swap_report = run.run_phase("emit_weapon_model_swaps", mod_path=str(ctx.mod_path), params={})
            runner.emit_log("INFO", f"Weapon model swaps: {model_swap_report.get('records_changed', 0)} skin(s), {model_swap_report.get('warnings', 0)} unmapped")
            # After the fixups: reads the curve-source markers and the flattened damage they wrote.
            npc_stats_report = run.run_phase("emit_npc_stats_catalog", mod_path=str(ctx.mod_path), params={})
            runner.emit_log("INFO", f"NPC stats catalog: {npc_stats_report.get('assets_written', 0)} file(s), {npc_stats_report.get('warnings', 0)} missing curves/unmapped forms")
        self._run_mvp_creature_corpus_discovery(run, ctx, runner)
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
        mvp_creature_profile = _exact_mvp_creature_profile(ctx)
        if mvp_creature_profile is not None:
            creature_started = time.perf_counter()
            creature_report = run.run_phase(
                "emit_mvp_creature",
                mod_path=str(ctx.mod_path),
                params={"profile": mvp_creature_profile},
            )
            added = int(creature_report.get("records_added", 0))
            if added != 6:
                raise RuntimeError(
                    f"{mvp_creature_profile} MVP emitted {added} records; expected 6"
                )
            total_added += added
            _timing_report(ctx).record(
                "emit_mvp_creature",
                _elapsed_seconds(creature_started),
                added=added,
                assets=int(creature_report.get("assets_written", 0)),
            )
            runner.emit_log(
                "INFO",
                f"MVP creature emitted: profile={mvp_creature_profile} "
                f"records={added} scaffold_hkx="
                f"{int(creature_report.get('assets_written', 0))}",
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
    def _emit_equipment_condition_catalog(run, ctx: ConversionContext, runner) -> None:
        report = run.run_phase("emit_equipment_condition", mod_path=str(ctx.mod_path), params={})
        catalog_path = Path(ctx.mod_path) / "F4SE/Plugins/B21_TalesFromAppalachia/Condition" / f"{Path(ctx.output_plugin_name).name}.json"
        catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
        if catalog.get("settings", {}).get("mapping_available") is not True:
            raise RuntimeError("Equipment condition catalog has no live source-to-target mappings; conversion cannot publish this catalog")
        runner.emit_log("INFO", f"Equipment condition catalog: {report.get('assets_written', 0)} file(s), {report.get('warnings', 0)} missing curve(s)")

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
                getattr(ctx, "source_data_dir", None) or self._req.source_data_dir or ""
            )
            with ConversionRun.open_existing(
                str(ctx.source_game),
                str(ctx.target_game),
                str(source_plugin),
                str(output_path),
                master_plugin_paths=[
                    str(path) for path in getattr(ctx, "target_master_plugin_paths", [])
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
                        "conversion_workers": getattr(ctx, "conversion_workers", None),
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
            sample_children = [
                str(value) for value in result.get("skipped_children") or []
            ]
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
        result = load_native_module().conversion_run_sync_cell_locations_from_lctn(
            rust_run.id
        )
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
        # Needs placed-ref repair done (ess_placed_locations) and the source still open.
        catalog_report = rust_run.run_phase(
            "emit_leveled_catalog", mod_path=str(ctx.mod_path), params={}
        )
        runner.emit_log(
            "INFO",
            "Leveled-list catalog: "
            f"{int(catalog_report.get('assets_written', 0))} file(s), "
            f"{int(catalog_report.get('warnings', 0))} unmapped form(s)",
        )
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
            runner.emit_log("WARN", "synthesize_vendor_dialogue: no rust run; skipping")
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
        if not (
            self._req.source_game.lower() == "fo76"
            and self._req.target_game.lower() == "fo4"
        ):
            return
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
                patch_target_worldspaces_subrecords,
            )

            try:
                terrain_items = fo76_btd_work_items(ctx)
            except FileNotFoundError:
                terrain_items = []
            if not terrain_items:
                return

            output_esp_path = Path(ctx.mod_path) / ctx.output_plugin_name
            source_worldspaces = []
            for terrain, _btd_path, _worldspace_eid in terrain_items:
                source_worldspace = getattr(
                    terrain, "source_worldspace_editor_id", ""
                ) or getattr(terrain, "worldspace_editor_id", "")
                if source_worldspace:
                    source_worldspaces.append(source_worldspace)

            copied_by_worldspace = patch_target_worldspaces_subrecords(
                source_plugin=source_handle,
                target_plugin_path=output_esp_path,
                worldspace_editor_ids=source_worldspaces,
                target_game=self._req.target_game,
            )
            for source_worldspace, copied in zip(
                source_worldspaces, copied_by_worldspace, strict=True
            ):
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
            changes = target_handle.repair_term_marker_parameters_from_source(
                source_handle
            )
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
        if self._req.source_game.lower() != "fo76" or self._req.target_game.lower() != "fo4":
            raise ValueError("Convert Scripts supports only fo76:fo4")
        self._run_native_convert_scripts_phase(ctx, runner)

    def _run_native_convert_scripts_phase(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> None:
        from bacup_lib.run import ConversionRun
        from creation_lib.pex.corpus import bundled_corpus_root

        run = getattr(ctx, "_rust_conversion_run", None)
        owned_run = run is None
        drainer = None
        if owned_run:
            run = ConversionRun.create_new(
                self._req.source_game,
                self._req.target_game,
                None,
                getattr(ctx, "output_plugin_name", "Output.esm"),
                config=self._native_run_config(ctx),
            )
            drainer = Drainer(run, runner)
            drainer.start()
        diagnostics = Path(getattr(ctx, "diagnostics_root", None) or ctx.mod_path)
        bundled = bundled_corpus_root(self._req.target_game)
        started = time.perf_counter()
        try:
            report = run.run_phase(
                "convert_scripts",
                mod_path=str(ctx.mod_path),
                source_extracted_dir=str(self._req.source_data_dir or ""),
                target_extracted_dir=str(self._req.target_extracted_dir) if self._req.target_extracted_dir is not None else None,
                target_data_dir=str(self._req.target_data_dir) if self._req.target_data_dir is not None else None,
                params={
                    "resources_root": str(Path(__file__).resolve().parents[1]),
                    "source_data_dir": str(self._req.source_data_dir or ""),
                    "additional_source_asset_roots": [str(path) for path in self._req.additional_source_asset_roots],
                    "diagnostics_root": str(diagnostics),
                    "output_root": str(self._req.output_root),
                    "bundled_corpus_root": str(bundled) if bundled is not None else None,
                    "workers": self._req.options.conversion_workers,
                    "compiler": getattr(self._req.options, "papyrus_compiler", "native"),
                },
            )
            ctx.summary.scripts_flagged += int(report.get("warnings", 0))
            timing = json.loads((diagnostics / "script_port_timing.json").read_text(encoding="utf-8"))
            for event in timing["events"]:
                fields = dict(event)
                name = fields.pop("name")
                elapsed = fields.pop("elapsed_seconds")
                fields["offset_seconds"] += max(started - _timing_report(ctx).started_at, 0.0)
                _timing_report(ctx).record(name, elapsed, **fields)
        finally:
            if drainer is not None:
                drainer.stop()
            if owned_run:
                run.close()

    def _collect_script_references(
        self,
        run_id: int,
        runner: "ConversionRunner",
    ) -> tuple[list[_ScriptReference], int]:
        native = load_native_module()
        native_started = time.perf_counter()
        (
            _plugin_name,
            native_refs,
            counters,
            candidate_count,
            alias_records_changed,
            alias_bindings_changed,
            warnings,
        ) = native.conversion_run_inspect_script_references(run_id)
        native_seconds = time.perf_counter() - native_started
        refs = [
            _ScriptReference(
                script_name=str(script_name),
                variable_name=(str(variable_name) if variable_name is not None else None),
                form_key=str(form_key),
                form_id=int(form_id) & 0xFFFFFFFF,
                record_sig=str(record_sig),
                editor_id=str(editor_id or ""),
                kind=str(kind),
                condition_inferred=bool(condition_inferred),
            )
            for (
                script_name,
                variable_name,
                form_key,
                form_id,
                record_sig,
                editor_id,
                kind,
                condition_inferred,
            ) in native_refs
        ]
        (
            native_row_count,
            vmad_row_count,
            ctda_row_count,
            subrecord_count,
            raw_payload_bytes,
            authoring_bytes,
        ) = (int(value) for value in counters)

        self._script_reference_profile = {
            "timings": (("convert_scripts_collect_native", native_seconds),),
            "counters": {
                "native_rows": native_row_count,
                "vmad_rows": vmad_row_count,
                "ctda_rows": ctda_row_count,
                "subrecords": subrecord_count,
                "raw_bytes": raw_payload_bytes,
                "authoring_bytes": authoring_bytes,
            },
        }

        runner.emit_log(
            "INFO",
            "[Scripts] reference profile: "
            f"native_ms={native_seconds * 1000:.3f} "
            f"native_rows={native_row_count} vmad_rows={vmad_row_count} "
            f"ctda_rows={ctda_row_count} subrecords={subrecord_count} "
            f"raw_bytes={raw_payload_bytes} authoring_bytes={authoring_bytes}",
        )

        runner.emit_log(
            "INFO",
            f"[Scripts] discovered {len(refs)} Papyrus script reference(s) "
            f"across {int(candidate_count)} record(s)",
        )
        for warning in warnings:
            runner.emit_log("WARN", f"[Scripts] {warning}")
        if alias_bindings_changed:
            runner.emit_log(
                "INFO",
                f"[Scripts] redirected {alias_bindings_changed} "
                f"Default2StateActivator binding(s) across "
                f"{alias_records_changed} FO76 record(s)",
            )
        return refs, int(candidate_count)

    @staticmethod
    def _reconcile_script_references(
        run_id: int,
        resolution_evidence: list[tuple[str, str, str, str | None]],
    ) -> tuple[
        int,
        int,
        int,
        int,
        list[str],
        list[str],
        list[str],
        list[tuple[str, str, str, str]],
        int,
        int,
        int,
    ]:
        return load_native_module().conversion_run_reconcile_script_references(
            run_id,
            resolution_evidence,
        )

    def _compile_decompiled_scripts_for_fo4(
        self,
        script_names: list[str],
        *,
        source_index: dict[str, Path],
        ctx: ConversionContext,
        runner: "ConversionRunner",
        workers: int,
        psc_paths: Mapping[str, Path] | None = None,
    ) -> list[tuple[str, _ScriptResolution]]:
        if not script_names:
            return []

        selector = getattr(self._req.options, "papyrus_compiler", "native")
        if selector == "exe-batch":
            return self._compile_decompiled_scripts_batch_for_fo4(
                script_names,
                ctx=ctx,
                runner=runner,
                psc_paths=psc_paths,
            )
        if selector == "native":
            return self._compile_decompiled_scripts_native_for_fo4(
                script_names,
                ctx=ctx,
                runner=runner,
                workers=workers,
                psc_paths=psc_paths,
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

    def _papyrus_type_universe(
        self, ctx: object, runner: "ConversionRunner"
    ) -> Path | None:
        """Header-only `.psc` synthesized from the target game's own `.pex`.

        Fallout 4 ships no `.psc` — those come with the Creation Kit — so
        importing the game's source directory made every conversion depend on a
        CK install, and silently compiled nothing without one. The `.pex` in the
        user's extracted game carry the same type information.
        """
        cached = getattr(self, "_papyrus_headers_root", None)
        if cached is not None:
            return cached or None

        corpus = papyrus_header_cache.find_pex_corpus(
            *(
                root / "scripts"
                for root in (self._req.target_extracted_dir, self._req.target_data_dir)
                if root is not None
            )
        )
        if corpus is None:
            # Nothing loose on disk is the normal case for a player who never
            # extracted the game — the .pex are still there, inside the BA2.
            corpus = papyrus_header_cache.materialize_pex_corpus(
                getattr(ctx, "target_asset_store", None)
            )
            if corpus is not None:
                runner.emit_log(
                    "INFO",
                    "[Scripts] building the Papyrus type universe from the game's "
                    f"archives ({corpus})",
                )
        if corpus is None:
            # No game to read at all. The shipped corpus carries the same type
            # surface, so conversion does not require an installed Fallout 4.
            from creation_lib.pex.corpus import bundled_corpus_root

            bundled = bundled_corpus_root(self._req.target_game)
            if bundled is not None:
                runner.emit_log(
                    "INFO",
                    f"[Scripts] using the bundled {self._req.target_game} Papyrus "
                    f"type universe ({bundled})",
                )
                self._papyrus_headers_root = bundled
                return bundled
            self._papyrus_headers_root = False
            return None

        # Never under target_data_dir: the game's Data directory is not ours to
        # write into, and the corpus may well have been found there.
        extracted = self._req.target_extracted_dir
        cache_base = Path(extracted) if extracted is not None else Path(
            self._req.output_root
        )
        out_root = cache_base / "Scripts" / "GeneratedHeaders"
        headers = papyrus_header_cache.ensure_headers(
            corpus, out_root, emit_log=runner.emit_log
        )
        self._papyrus_headers_root = headers
        return headers

    def _papyrus_import_roots(
        self, ctx: object, runner: "ConversionRunner"
    ) -> tuple[Path, ...]:
        from creation_lib.pex.corpus import bundled_corpus_root

        headers_root = self._papyrus_type_universe(ctx, runner)
        if headers_root is None:
            return ()
        roots = [headers_root]
        # Loose installs can contain the anchor PEX files without the full API.
        bundled = bundled_corpus_root(self._req.target_game)
        if bundled is not None:
            roots.append(bundled)
        return tuple(_dedupe_paths(roots))

    def _compile_decompiled_scripts_native_for_fo4(
        self,
        script_names: list[str],
        *,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        workers: int,
        psc_paths: Mapping[str, Path] | None = None,
    ) -> list[tuple[str, _ScriptResolution]]:
        from creation_lib.core.game_profiles import get_profile
        from creation_lib.pex.native_runtime import compile_psc

        psc_root = Path(ctx.mod_path) / "Scripts" / "Source" / "User"
        output_root = Path(ctx.mod_path) / "data" / "Scripts"
        output_root.mkdir(parents=True, exist_ok=True)

        def _all(status: str, message: str) -> list[tuple[str, _ScriptResolution]]:
            return [
                (n, _ScriptResolution(n, status, message=message)) for n in script_names
            ]

        profile = get_profile(self._req.target_game)

        import_roots = self._papyrus_import_roots(ctx, runner)
        if not import_roots:
            return _all(
                "compiler_unavailable",
                "no Fallout 4 script corpus found in the game's loose scripts "
                "or its archives; the Papyrus type universe is built from those "
                ".pex",
            )

        imports = _dedupe_paths([psc_root, *import_roots])
        import_args = [str(path) for path in imports]
        flags_arg = profile.papyrus_flags or None
        bundled_flags = papyrus_header_cache.flags_file(import_roots[0])
        if bundled_flags is not None:
            flags_arg = str(bundled_flags)

        runner.emit_log(
            "INFO",
            f"[Scripts] native-compiling {len(script_names)} script(s)",
        )

        def run_one(script_name: str) -> tuple[str, _ScriptResolution]:
            psc_path = (
                psc_paths.get(
                    script_name, psc_root / _script_relative_path(script_name, ".psc")
                )
                if psc_paths is not None
                else psc_root / _script_relative_path(script_name, ".psc")
            )
            try:
                relative_psc = psc_path.relative_to(psc_root)
            except ValueError:
                return script_name, _ScriptResolution(
                    script_name,
                    "compile_failed",
                    message=f"source is outside {psc_root}: {psc_path}",
                )
            output_pex = output_root / relative_psc.with_suffix(".pex")
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
        psc_paths: Mapping[str, Path] | None = None,
    ) -> list[tuple[str, _ScriptResolution]]:
        """Compile every decompiled .psc in one PapyrusCompiler.exe -all process.

        One CLR instead of W concurrent ones bounds RAM.
        Per-script _ScriptResolution is reconstructed from which output .pex the
        batch run produced, preserving the downstream failed-ref stripping in
        _run_convert_scripts_phase.

        A manifest caller supplies ``psc_paths``; those sources are staged into
        an isolated input root so ``-all`` cannot compile stale sibling sources.
        """
        from creation_lib.core.game_profiles import get_profile

        psc_root = Path(ctx.mod_path) / "Scripts" / "Source" / "User"
        output_root = Path(ctx.mod_path) / "data" / "Scripts"
        output_root.mkdir(parents=True, exist_ok=True)

        def _all(status: str, message: str) -> list[tuple[str, _ScriptResolution]]:
            return [
                (n, _ScriptResolution(n, status, message=message)) for n in script_names
            ]

        target_data_dir = self._req.target_data_dir
        if target_data_dir is None:
            return _all("compiler_unavailable", "target data dir not configured")
        profile = get_profile(self._req.target_game)
        if not profile.papyrus_compiler_dir:
            return _all("compiler_unavailable", "target game has no Papyrus compiler")
        compiler = (
            target_data_dir.parent
            / profile.papyrus_compiler_dir
            / "PapyrusCompiler.exe"
        )
        if not compiler.is_file():
            return _all("compiler_unavailable", str(compiler))

        batch_source_root = psc_root
        staged_source_root: Path | None = None
        if psc_paths is not None:
            try:
                staged_source_root = Path(
                    tempfile.mkdtemp(prefix="fnv-psc-manifest-", dir=ctx.mod_path)
                )
                for script_name in script_names:
                    source_path = psc_paths.get(script_name)
                    if source_path is None or not source_path.is_file():
                        raise FileNotFoundError(
                            f"missing current-run source for {script_name}"
                        )
                    staged_path = staged_source_root / _script_relative_path(
                        script_name, ".psc"
                    )
                    staged_path.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source_path, staged_path)
                batch_source_root = staged_source_root
            except (OSError, ValueError) as exc:
                if staged_source_root is not None:
                    shutil.rmtree(staged_source_root, ignore_errors=True)
                return _all("compile_failed", str(exc))

        # Pre-delete each target .pex so post-compile presence is a true success
        # signal. mod_path is reused across runs; without this a stale .pex from a
        # prior run would mask a real compile failure as "compiled" (the per-script
        # path is guarded by the subprocess returncode, which -all cannot give us).
        predelete_failures: dict[str, str] = {}
        for script_name in script_names:
            stale_pex = output_root / _script_relative_path(script_name, ".pex")
            if stale_pex.is_file():
                try:
                    stale_pex.unlink()
                except OSError as exc:
                    predelete_failures[_script_key(script_name)] = (
                        f"could not remove stale output {stale_pex}: {exc}"
                    )

        imports = [batch_source_root]
        if batch_source_root != psc_root:
            imports.append(psc_root)
        if self._req.source_game == "fo76" and self._req.target_game == "fo4":
            imports.append(Path(__file__).resolve().parents[5] / "mods/B21_TalesFromAppalachia/Scripts/Source/User")
        game_user = target_data_dir / "Scripts" / "Source" / "User"
        game_base = target_data_dir / "Scripts" / "Source" / "Base"
        if game_user.is_dir():
            imports.append(game_user)
        if game_base.is_dir():
            imports.append(game_base)
        import_arg = ";".join(str(path) for path in imports)

        cmd = [
            str(compiler),
            str(batch_source_root),
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
                cwd=str(batch_source_root),
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=timeout_seconds,
                creationflags=_CREATE_NO_WINDOW,
            )
        except subprocess.TimeoutExpired:
            return _all("compile_timeout", "batch compile timeout")
        finally:
            if staged_source_root is not None:
                shutil.rmtree(staged_source_root, ignore_errors=True)

        out_text = (completed.stdout or "").strip()
        if out_text:
            for line in out_text.splitlines():
                if line.strip():
                    runner.emit_log("INFO", f"[compile] {line.rstrip()}")

        results: list[tuple[str, _ScriptResolution]] = []
        for script_name in script_names:
            output_pex = output_root / _script_relative_path(script_name, ".pex")
            predelete_failure = predelete_failures.get(_script_key(script_name))
            if predelete_failure is not None:
                results.append(
                    (
                        script_name,
                        _ScriptResolution(
                            script_name,
                            "compile_failed",
                            message=predelete_failure,
                        ),
                    )
                )
            elif output_pex.is_file():
                results.append(
                    (
                        script_name,
                        _ScriptResolution(script_name, "compiled", output_pex),
                    )
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

        if output_pex.is_file():
            try:
                output_pex.unlink()
            except OSError as exc:
                return _ScriptResolution(
                    script_name,
                    "compile_failed",
                    message=f"could not remove stale output {output_pex}: {exc}",
                )

        imports = [psc_root]
        if self._req.source_game == "fo76" and self._req.target_game == "fo4":
            imports.append(Path(__file__).resolve().parents[5] / "mods/B21_TalesFromAppalachia/Scripts/Source/User")
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
                creationflags=_CREATE_NO_WINDOW,
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

    def _run_quest_runtime_inventory_phase(
        self,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> None:
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log(
                "WARN", "[Quest Runtime] no target plugin handle; inventory skipped"
            )
            return

        input_started = time.perf_counter()
        source_roots = _source_script_roots(
            self._req.source_data_dir,
            self._req.additional_source_asset_roots,
        )
        target_roots = _target_script_roots(
            target_data_dir=self._req.target_data_dir,
            target_extracted_dir=self._req.target_extracted_dir,
        )
        if self._req.options.convert_scripts:
            target_roots = _dedupe_paths(
                [Path(ctx.mod_path) / "data" / "Scripts", *target_roots]
            )
        else:
            runner.emit_log(
                "WARN",
                "[Quest Runtime] script conversion is disabled; persistent mod-output "
                "PEX files are not current-run producer evidence",
            )

        diagnostics_root = Path(
            getattr(ctx, "diagnostics_root", None) or ctx.mod_path / "diagnostics"
        )
        diagnostics_root.mkdir(parents=True, exist_ok=True)
        report_path = diagnostics_root / "quest_runtime_inventory.json"
        addition_scripts = [
            script_name
            for script_name, _path in _script_addition_sources(
                self._req.source_game,
                self._req.target_game,
            ).values()
        ]
        _record_timing(
            ctx,
            "quest_runtime_inventory_input_preparation",
            input_started,
            scope="detail",
            parent="Inventory Quest Runtime Routes",
            source_roots=len(source_roots),
            target_roots=len(target_roots),
            additions=len(addition_scripts),
        )
        inventory_stage = "quest_runtime_inventory_native"
        inventory_stage_started = time.perf_counter()
        try:
            report_json = (
                load_native_module().conversion_run_quest_runtime_inventory_json(
                    rust_run.id,
                    [str(path) for path in source_roots],
                    [str(path) for path in target_roots],
                    str(_SCRIPT_PATCH_DIR),
                    addition_scripts,
                    str(_QUEST_RUNTIME_UNSUPPORTED_MANIFEST),
                    self._req.options.convert_scripts,
                )
            )
            _record_timing(
                ctx,
                "quest_runtime_inventory_native",
                inventory_stage_started,
                scope="detail",
                parent="Inventory Quest Runtime Routes",
                status="ok",
            )
            inventory_stage = "quest_runtime_inventory_report_write"
            inventory_stage_started = time.perf_counter()
            report_path.write_text(f"{report_json.rstrip()}\n", encoding="utf-8")
            _record_timing(
                ctx,
                "quest_runtime_inventory_report_write",
                inventory_stage_started,
                scope="detail",
                parent="Inventory Quest Runtime Routes",
                status="ok",
                bytes=len(report_json.encode("utf-8")),
            )
            inventory_stage = "quest_runtime_inventory_json_decode"
            inventory_stage_started = time.perf_counter()
            report = json.loads(report_json)
            _record_timing(
                ctx,
                "quest_runtime_inventory_json_decode",
                inventory_stage_started,
                scope="detail",
                parent="Inventory Quest Runtime Routes",
                status="ok",
            )
        except Exception as exc:
            _record_timing(
                ctx,
                inventory_stage,
                inventory_stage_started,
                scope="detail",
                parent="Inventory Quest Runtime Routes",
                status="failed",
                error_type=type(exc).__name__,
            )
            runner.emit_log(
                "WARN",
                f"[Quest Runtime] native inventory could not complete: {exc}",
            )
            return

        if report.get("inventory_error"):
            runner.emit_log(
                "WARN",
                "[Quest Runtime] inventory could not complete; "
                f"report written before continuing: {report['inventory_error']}",
            )
            return
        summary_started = time.perf_counter()
        summary = report["summary"]
        level = "INFO" if report["coverage_complete"] else "WARN"
        runner.emit_log(
            level,
            "[Quest Runtime] "
            f"native={summary['native']} adapted={summary['adapted']} "
            f"unsupported={summary['explicitly_unsupported']} "
            f"unclassified={summary['unclassified']}; "
            "strict enforcement is disabled",
        )
        _record_timing(
            ctx,
            "quest_runtime_inventory_summary",
            summary_started,
            scope="detail",
            parent="Inventory Quest Runtime Routes",
            coverage_complete=bool(report["coverage_complete"]),
        )

    def _run_optional_fnv_legacy_phase(
        self,
        ctx: ConversionContext,
        source_plugin: Path,
        runner: "ConversionRunner",
    ) -> bool:
        if self._req.source_game != "fnv" or self._req.target_game != "fo4":
            return False
        if bool(getattr(ctx, "_fnv_legacy_completed", False)):
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

    def _is_fnv_quest_slice(self) -> bool:
        if (
            self._req.source_game.lower() != "fnv"
            or self._req.target_game.lower() != "fo4"
        ):
            return False
        return bool(getattr(self._req.options, "fnv_quest_slice", False))

    def _is_fnv_world_only_mvp(self) -> bool:
        if (
            self._req.source_game.lower() != "fnv"
            or self._req.target_game.lower() != "fo4"
        ):
            return False
        from bacup_lib.source_pairs import FNV_MVP_EXCLUDE_SIGNATURES

        excluded = frozenset(
            signature.upper()
            for signature in getattr(self._req.options, "exclude_signatures", ())
        )
        return (
            not self._is_fnv_quest_slice()
            and self._active_mvp_creature_corpus_policy() is None
            and FNV_MVP_EXCLUDE_SIGNATURES <= excluded
        )

    def _active_mvp_creature_profile(self) -> str | None:
        if self._is_fnv_quest_slice() or getattr(
            self._req.options, "mvp_melee_only", False
        ):
            return None
        if self._active_mvp_creature_corpus_policy() is not None:
            return None
        from bacup_lib.source_pairs import mvp_creature_profile_payload

        pair_id = {
            ("fnv", "fo4"): "fnvfo3:fo4",
            ("skyrimse", "fo4"): "skyrimse:fo4",
        }.get((self._req.source_game.lower(), self._req.target_game.lower()))
        if pair_id is None:
            return None
        return mvp_creature_profile_payload(
            pair_id,
            frozenset(getattr(self._req.options, "exclude_signatures", ())),
        )

    def _active_mvp_creature_corpus_policy(self) -> dict[str, object] | None:
        if self._is_fnv_quest_slice() or getattr(
            self._req.options, "mvp_melee_only", False
        ):
            return None
        from bacup_lib.source_pairs import mvp_creature_corpus_policy_payload

        pair_id = {
            ("fnv", "fo4"): "fnvfo3:fo4",
            ("skyrimse", "fo4"): "skyrimse:fo4",
        }.get((self._req.source_game.lower(), self._req.target_game.lower()))
        if pair_id is None:
            return None
        return mvp_creature_corpus_policy_payload(
            pair_id,
            frozenset(getattr(self._req.options, "exclude_signatures", ())),
        )

    def _active_mvp_melee_policy(self) -> dict[str, object] | None:
        if self._is_fnv_quest_slice():
            return None
        from bacup_lib.source_pairs import mvp_melee_policy_payload

        pair_id = {
            ("fnv", "fo4"): "fnvfo3:fo4",
            ("skyrimse", "fo4"): "skyrimse:fo4",
        }.get((self._req.source_game.lower(), self._req.target_game.lower()))
        if pair_id is None:
            return None
        return mvp_melee_policy_payload(
            pair_id,
            frozenset(getattr(self._req.options, "exclude_signatures", ())),
        )

    def _active_mvp_record_exceptions(self) -> list[dict[str, str | int]]:
        if self._is_fnv_quest_slice():
            return []
        if self._active_mvp_melee_policy() is not None:
            return []
        from bacup_lib.source_pairs import mvp_record_exception_payload

        pair_id = {
            ("fnv", "fo4"): "fnvfo3:fo4",
            ("skyrimse", "fo4"): "skyrimse:fo4",
        }.get((self._req.source_game.lower(), self._req.target_game.lower()))
        if pair_id is None:
            return []
        return mvp_record_exception_payload(
            pair_id,
            frozenset(getattr(self._req.options, "exclude_signatures", ())),
        )

    @staticmethod
    def _generated_psc_manifest(
        result_dict: Mapping[str, Any],
    ) -> list[dict[str, Any]]:
        entries = result_dict.get("generated_psc_classes", ()) or ()
        manifest: list[dict[str, Any]] = []
        seen: set[str] = set()
        for entry in entries:
            if isinstance(entry, Mapping):
                class_name = entry.get("class_name")
                relative_source_path = entry.get("relative_source_path")
                kind = entry.get("kind", "unknown")
                required = entry.get("required", True)
            else:
                class_name = entry
                relative_source_path = f"{class_name}.psc"
                kind = "unknown"
                required = True
            if not isinstance(class_name, str):
                raise ValueError(f"invalid generated PSC manifest entry: {entry!r}")
            normalized_class_name = class_name.strip().removesuffix(".psc")
            if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", normalized_class_name):
                raise ValueError(f"invalid generated PSC class name: {class_name!r}")
            if not isinstance(relative_source_path, str):
                raise ValueError(f"invalid PSC source path for {class_name!r}")
            relative_path = Path(relative_source_path.replace("\\", "/"))
            if (
                relative_path.is_absolute()
                or relative_path.suffix.casefold() != ".psc"
                or ".." in relative_path.parts
            ):
                raise ValueError(
                    f"invalid generated PSC relative source path: {relative_source_path!r}"
                )
            key = normalized_class_name.casefold()
            if key in seen:
                raise ValueError(f"duplicate generated PSC class name: {class_name!r}")
            seen.add(key)
            manifest.append(
                {
                    "class_name": normalized_class_name,
                    "relative_source_path": relative_path.as_posix(),
                    "kind": str(kind),
                    "required": bool(required),
                }
            )
        return manifest

    _fnv_generated_psc_manifest = _generated_psc_manifest

    @staticmethod
    def _generated_psc_validation_errors(
        manifest: list[dict[str, Any]],
        psc_paths: Mapping[str, Path],
        *,
        current_run_root: Path | None = None,
    ) -> dict[str, str]:
        errors: dict[str, str] = {}
        classes_by_key: dict[str, str] = {}
        stems_by_key: dict[str, str] = {}
        paths_by_key: dict[str, str] = {}

        for entry in manifest:
            class_name = str(entry["class_name"])
            class_key = class_name.casefold()
            previous_class = classes_by_key.get(class_key)
            if previous_class is not None:
                message = (
                    "case-insensitive duplicate generated PSC class name: "
                    f"{previous_class}, {class_name}"
                )
                errors[previous_class] = message
                errors[class_name] = message
            else:
                classes_by_key[class_key] = class_name
            if len(class_name) > _PAPYRUS_CLASS_NAME_MAX_LENGTH:
                errors[class_name] = (
                    "Papyrus class name exceeds "
                    f"{_PAPYRUS_CLASS_NAME_MAX_LENGTH} characters: {class_name}"
                )

            psc_path = psc_paths.get(class_name)
            if psc_path is None:
                errors[class_name] = "missing generated PSC source path"
                continue
            path_key = str(psc_path.resolve()).casefold()
            previous_path = paths_by_key.get(path_key)
            if previous_path is not None:
                message = (
                    f"case-insensitive duplicate generated PSC source path: {psc_path}"
                )
                errors[previous_path] = message
                errors[class_name] = message
            else:
                paths_by_key[path_key] = class_name
            if current_run_root is not None:
                try:
                    psc_path.resolve().relative_to(current_run_root.resolve())
                except ValueError:
                    errors[class_name] = (
                        f"generated PSC source is outside current run: {psc_path}"
                    )
                    continue
            stem = psc_path.stem
            stem_key = stem.casefold()
            previous_stem = stems_by_key.get(stem_key)
            if previous_stem is not None:
                message = (
                    "case-insensitive duplicate generated PSC filename stem: "
                    f"{psc_paths[previous_stem].stem}, {stem}"
                )
                errors[previous_stem] = message
                errors[class_name] = message
            else:
                stems_by_key[stem_key] = class_name
            if len(stem) > _PAPYRUS_CLASS_NAME_MAX_LENGTH:
                errors[class_name] = (
                    "Papyrus PSC filename stem exceeds "
                    f"{_PAPYRUS_CLASS_NAME_MAX_LENGTH} characters: {stem}"
                )
            if not psc_path.is_file():
                errors[class_name] = f"missing current-run source {psc_path}"
                continue
            try:
                source = psc_path.read_text(encoding="utf-8-sig")
            except OSError as exc:
                errors[class_name] = (
                    f"could not read current-run source {psc_path}: {exc}"
                )
                continue
            match = _PSC_SCRIPT_NAME_RE.search(source)
            if match is None:
                errors[class_name] = (
                    f"current-run source has no ScriptName declaration: {psc_path}"
                )
            elif match.group(1) != class_name:
                errors[class_name] = (
                    f"ScriptName {match.group(1)!r} does not match manifest "
                    f"class_name {class_name!r}"
                )
        return errors

    _fnv_generated_psc_validation_errors = _generated_psc_validation_errors

    def _prepare_skyrim_minimal_quest_actions(
        self,
        *,
        run,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> int:
        if not (
            ctx.source_game.lower() == "skyrimse"
            and ctx.target_game.lower() == "fo4"
        ):
            return 0
        candidates = json.loads(
            load_native_module().conversion_run_skyrim_minimal_quest_candidates_json(
                run.id
            )
        )
        if not candidates:
            return 0
        from bacup_lib.skyrim_papyrus import (
            SkyrimScriptIntent,
            discover_skyrim_psc_artifacts,
            extract_minimal_quest_fragment_actions,
        )

        import_roots = self._papyrus_import_roots(ctx, runner)
        if not import_roots:
            raise RuntimeError(
                "Skyrim minimal quest conversion requires the Fallout 4 Papyrus type universe"
            )
        flags_path = papyrus_header_cache.flags_file(import_roots[0])
        production = discover_skyrim_psc_artifacts(
            [
                SkyrimScriptIntent(
                    row["source_script_class"], "quest-fragment", required=True
                )
                for row in candidates
            ],
            source_data_dir=self._req.source_data_dir,
            additional_source_asset_roots=self._req.additional_source_asset_roots,
            fo4_import_dirs=import_roots,
            fo4_flags=str(flags_path) if flags_path is not None else None,
        )
        if not production.supported:
            raise RuntimeError(
                "Skyrim minimal quest source recovery failed: "
                + str(production.unsupported_reason or "unsupported source")
            )
        sources = {
            artifact.class_name.casefold(): artifact.source
            for artifact in production.artifacts
        }
        requests = []
        for row in candidates:
            source = sources.get(row["source_script_class"].casefold())
            if source is None:
                raise RuntimeError(
                    "Skyrim minimal quest source recovery omitted required class "
                    + row["source_script_class"]
                )
            requests.append(
                {
                    "component_id": row["component_id"],
                    "actions": extract_minimal_quest_fragment_actions(
                        source, row["source_fragment_function"]
                    ),
                }
            )
        raw_prefix = re.sub(r"[^A-Za-z0-9_]", "_", Path(ctx.mod_path).name)
        class_prefix = (raw_prefix or "Skyrim")[:24]
        if class_prefix[0].isdigit():
            class_prefix = f"S_{class_prefix}"[:24]
        load_native_module().conversion_run_install_skyrim_minimal_quest_actions(
            run.id,
            class_prefix,
            json.dumps(requests, sort_keys=True),
        )
        runner.emit_log(
            "INFO",
            f"[Skyrim Quest Runtime] admitted action lowering for {len(requests)} minimal quest(s)",
        )
        return len(requests)

    def _compile_and_attach_skyrim_minimal_quests(
        self,
        *,
        run,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> int:
        if not (
            ctx.source_game.lower() == "skyrimse"
            and ctx.target_game.lower() == "fo4"
        ):
            return 0
        manifest = json.loads(
            load_native_module().conversion_run_emit_skyrim_minimal_quest_psc_manifest_json(
                run.id, str(ctx.mod_path)
            )
        )
        if not manifest:
            return 0
        output_root = (Path(ctx.mod_path) / "data" / "Scripts").resolve()
        for row in manifest:
            stale_pex = (output_root / _script_relative_path(row["class_name"], ".pex")).resolve()
            if output_root not in stale_pex.parents:
                raise RuntimeError("Skyrim quest PEX path escaped the output root")
            if stale_pex.is_file():
                stale_pex.unlink()
        generic_evidence = self._compile_generated_psc_manifest(
            manifest=manifest,
            psc_files_written=len(manifest),
            ctx=ctx,
            runner=runner,
            workflow_label="Skyrim Quest Runtime",
            report_filename="skyrim_quest_script_compile_manifest.json",
            report_source="skyrim_minimal_quest_runtime",
            strict=True,
            failure_subject="Skyrim minimal quest",
        )
        result_by_class = {
            row["class_name"].casefold(): row for row in generic_evidence
        }
        pex_paths = [
            (output_root / _script_relative_path(row["class_name"], ".pex")).resolve()
            for row in manifest
        ]
        pex_hashes = load_native_module().conversion_hash_files_blake3(
            [str(path) for path in pex_paths], None
        )
        compile_run_id = f"skyrim-quest-{time.time_ns()}"
        evidence = []
        for row, pex_path, pex_hash in zip(manifest, pex_paths, pex_hashes, strict=True):
            result = result_by_class.get(row["class_name"].casefold())
            if result is None or not result.get("compiled_success"):
                raise RuntimeError(
                    f"required Skyrim minimal quest PSC {row['class_name']} was not compiled"
                )
            evidence.append(
                {
                    "manifest_id": row["manifest_id"],
                    "class_name": row["class_name"],
                    "relative_source_path": row["relative_source_path"],
                    "source_blake3": row["source_blake3"],
                    "relative_pex_path": f"data/Scripts/{row['class_name']}.pex",
                    "pex_artifact_path": str(pex_path),
                    "pex_blake3": pex_hash,
                    "target_game": "fo4",
                    "compile_run_id": compile_run_id,
                    "compiled_success": True,
                }
            )
        attached = int(
            load_native_module().conversion_run_reconcile_skyrim_minimal_quest_compiler_evidence(
                run.id, json.dumps(evidence, sort_keys=True)
            )
        )
        if attached != len(manifest):
            raise RuntimeError(
                "Skyrim minimal quest VMAD attachment count does not match the compile manifest"
            )
        runner.emit_log(
            "INFO",
            f"[Skyrim Quest Runtime] compiled and attached {attached} evidence-gated quest VMAD payload(s)",
        )
        return attached

    @staticmethod
    def _find_skyrim_quest_runtime_asset(
        roots: tuple[Path, ...],
        relative_candidates: tuple[str, ...],
        label: str,
    ) -> tuple[Path, str]:
        for candidate in relative_candidates:
            relative = PurePosixPath(candidate.replace("\\", "/"))
            for root in roots:
                path = root.joinpath(*relative.parts)
                if path.is_file():
                    return path, relative.as_posix()
                parent = path.parent
                if parent.is_dir():
                    match = next(
                        (
                            entry
                            for entry in parent.iterdir()
                            if entry.is_file()
                            and entry.name.casefold() == path.name.casefold()
                        ),
                        None,
                    )
                    if match is not None:
                        return match, relative.as_posix()
        raise FileNotFoundError(
            f"{label} is missing from declared roots: "
            + ", ".join(relative_candidates)
        )

    @staticmethod
    def _skyrim_quest_runtime_evidence(
        path: Path,
        relative_path: str,
        provenance: dict[str, str],
    ) -> dict[str, object]:
        digest = load_native_module().conversion_hash_files_blake3(
            [str(path.resolve())], None
        )
        if not isinstance(digest, list) or len(digest) != 1:
            raise RuntimeError("native BLAKE3 returned an invalid Skyrim quest asset digest")
        return {
            "relative_path": relative_path,
            "exists": True,
            "byte_len": path.stat().st_size,
            "blake3": digest[0],
            "provenance": provenance,
        }

    @staticmethod
    def _skyrim_quest_runtime_converted_roots(
        ctx: ConversionContext,
    ) -> tuple[Path, ...]:
        output_data = (Path(ctx.mod_path) / "data").resolve()
        resolved = (
            Path(root).resolve()
            for root in ctx.skyrim_quest_runtime_converted_asset_roots
            if Path(root).is_dir()
        )
        return tuple(
            dict.fromkeys(
                root
                for root in resolved
                if not (
                    root == output_data
                    or root.is_relative_to(output_data)
                    or output_data.is_relative_to(root)
                )
            )
        )

    def _copy_skyrim_quest_runtime_assets(
        self,
        *,
        run,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> int:
        if not (
            ctx.source_game.lower() == "skyrimse"
            and ctx.target_game.lower() == "fo4"
        ):
            return 0
        requirements_manifest = json.loads(
            load_native_module().conversion_run_skyrim_quest_runtime_asset_requirements_json(
                run.id
            )
        )
        requirements = requirements_manifest.get("requirements", [])
        if not requirements:
            return 0

        source_roots = tuple(
            dict.fromkeys(
                Path(root).resolve()
                for root in (
                    ctx.source_data_dir,
                    *ctx.additional_source_asset_roots,
                )
                if root is not None and Path(root).is_dir()
            )
        )
        output_data = (Path(ctx.mod_path) / "data").resolve()
        output_data.mkdir(parents=True, exist_ok=True)
        converted_roots = self._skyrim_quest_runtime_converted_roots(ctx)
        if not source_roots:
            raise RuntimeError(
                "Skyrim quest runtime voice assets require declared source asset roots"
            )
        if not converted_roots:
            raise RuntimeError(
                "Skyrim quest runtime voice assets require declared converted asset roots"
            )

        intents: list[dict[str, object]] = []
        for requirement in requirements:
            source_directory = str(requirement["source_voice_directory"])
            source_stems = tuple(str(stem) for stem in requirement["source_stems"])
            source_fuz_path, source_fuz_relative = self._find_skyrim_quest_runtime_asset(
                source_roots,
                tuple(f"{source_directory}/{stem}.fuz" for stem in source_stems),
                "Skyrim quest source FUZ",
            )
            source_lip_path, source_lip_relative = self._find_skyrim_quest_runtime_asset(
                source_roots,
                tuple(f"{source_directory}/{stem}.lip" for stem in source_stems),
                "Skyrim quest source LIP",
            )
            source_fuz = self._skyrim_quest_runtime_evidence(
                source_fuz_path,
                source_fuz_relative,
                {"kind": "loose_file"},
            )
            source_lip = self._skyrim_quest_runtime_evidence(
                source_lip_path,
                source_lip_relative,
                {"kind": "loose_file"},
            )
            target_local = int(str(requirement["target_info"]).split("@", 1)[0], 16)
            response_number = int(requirement["response_number"])
            target_stem = f"{target_local:06x}_{response_number:02}"
            target_directory = str(requirement["target_voice_path"])
            audio_path, audio_relative = self._find_skyrim_quest_runtime_asset(
                converted_roots,
                (
                    f"{target_directory}/{target_stem}.xwm",
                    f"{target_directory}/{target_stem}.wav",
                ),
                "converted Skyrim quest XWM/WAV",
            )
            target_lip_path, target_lip_relative = self._find_skyrim_quest_runtime_asset(
                converted_roots,
                (f"{target_directory}/{target_stem}.lip",),
                "converted Skyrim quest LIP",
            )
            audio = self._skyrim_quest_runtime_evidence(
                audio_path,
                audio_relative,
                {
                    "kind": "converted",
                    "source_blake3": str(source_fuz["blake3"]),
                    "converter": "declared-converted-artifact-root",
                },
            )
            target_lip = self._skyrim_quest_runtime_evidence(
                target_lip_path,
                target_lip_relative,
                {
                    "kind": "converted",
                    "source_blake3": str(source_lip["blake3"]),
                    "converter": "declared-converted-artifact-root",
                },
            )
            intents.append(
                {
                    "source_info": requirement["source_info"],
                    "target_info": requirement["target_info"],
                    "source_speaker": requirement["source_speaker"],
                    "target_speaker": requirement["target_speaker"],
                    "response_number": response_number,
                    "transcript": requirement["transcript"],
                    "voice_type": requirement["voice_type"],
                    "source_fuz": source_fuz,
                    "source_lip": source_lip,
                    "target_audio": {
                        "format": audio_path.suffix.removeprefix(".").casefold(),
                        "evidence": audio,
                    },
                    "target_lip": target_lip,
                    "sequence": None,
                }
            )
        manifest = {
            "output_plugin": requirements_manifest["output_plugin"],
            "intents": intents,
        }
        canonical_manifest = json.loads(
            load_native_module().conversion_run_install_skyrim_quest_runtime_asset_manifest_json(
                run.id, json.dumps(manifest, sort_keys=True)
            )
        )
        from bacup_lib.skyrim_runtime_assets import copy_skyrim_quest_runtime_assets

        receipt = copy_skyrim_quest_runtime_assets(
            canonical_manifest,
            source_roots,
            converted_roots,
            output_data,
        )
        receipt_json = {
            "rows": [
                {
                    "semantic_role": row.semantic_role,
                    "source_path": row.source_path,
                    "target_path": row.target_path,
                    "source_root": row.source_root,
                    "size": row.size,
                    "blake3": row.blake3,
                    "source_blake3": row.source_blake3,
                }
                for row in receipt.rows
            ]
        }
        reconciled = int(
            load_native_module().conversion_run_reconcile_skyrim_quest_runtime_asset_copy_receipt_json(
                run.id, json.dumps(receipt_json, sort_keys=True)
            )
        )
        if reconciled != receipt.files_copied:
            raise RuntimeError(
                "Skyrim quest runtime asset receipt count changed during native reconciliation"
            )
        runner.emit_log(
            "INFO",
            f"[Skyrim Quest Runtime] copied and reconciled {reconciled} voice/LIP/SEQ artifact(s)",
        )
        return reconciled

    def _compile_generated_psc_manifest(
        self,
        *,
        manifest: list[dict[str, Any]],
        psc_files_written: int,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        workflow_label: str,
        report_filename: str,
        report_source: str,
        strict: bool,
        failure_subject: str,
        exact_runtime_tree: bool = False,
    ) -> list[dict[str, Any]]:
        report_root = Path(getattr(ctx, "diagnostics_root", None) or ctx.mod_path)
        report_root.mkdir(parents=True, exist_ok=True)
        report_path = report_root / report_filename
        psc_root = Path(ctx.mod_path) / "Scripts" / "Source" / "User"
        resolutions: dict[str, _ScriptResolution] = {}
        entries_by_class = {entry["class_name"]: entry for entry in manifest}

        if psc_files_written and not manifest:
            message = (
                f"{workflow_label} wrote PSC files without a current-run "
                "generated_psc_classes manifest"
            )
            runner.emit_log("ERROR" if strict else "WARN", message)
            if strict:
                raise RuntimeError(message)
        if psc_files_written != len(manifest):
            message = (
                f"{workflow_label} PSC manifest count does not match files written: "
                f"manifest={len(manifest)} written={psc_files_written}"
            )
            runner.emit_log("ERROR" if strict else "WARN", message)
            if strict:
                raise RuntimeError(message)

        psc_paths = {}
        for class_name, entry in entries_by_class.items():
            relative_source = Path(entry["relative_source_path"])
            psc_paths[class_name] = (
                Path(ctx.mod_path) / relative_source
                if relative_source.parts
                and relative_source.parts[0].casefold() == "scripts"
                else psc_root / relative_source
            )
        validation_errors = self._generated_psc_validation_errors(
            manifest,
            psc_paths,
            current_run_root=Path(ctx.mod_path),
        )
        for class_name, message in validation_errors.items():
            resolutions[class_name] = _ScriptResolution(
                class_name,
                "compile_failed",
                message=message,
            )
        if strict and validation_errors:
            details = "; ".join(
                f"{class_name}: {message}"
                for class_name, message in sorted(validation_errors.items())
            )
            runner.emit_log(
                "ERROR", f"[{workflow_label} Scripts] invalid PSC manifest: {details}"
            )
            raise RuntimeError(
                f"invalid required {failure_subject} PSC manifest: {details}"
            )

        if exact_runtime_tree:
            expected_psc = {
                str(path.resolve()).casefold() for path in psc_paths.values()
            }
            actual_psc = {
                str(path.resolve()).casefold()
                for path in psc_root.rglob("*.psc")
                if path.is_file()
            }
            unexpected_psc = sorted(actual_psc - expected_psc)
            if unexpected_psc:
                raise RuntimeError(
                    f"{failure_subject} has stale PSC outside the current-run manifest: "
                    + ", ".join(unexpected_psc[:10])
                )

            output_root = Path(ctx.mod_path) / "data" / "Scripts"
            expected_pex = {
                str(
                    (output_root / _script_relative_path(name, ".pex")).resolve()
                ).casefold()
                for name in entries_by_class
            }
            actual_pex = (
                {
                    str(path.resolve()).casefold()
                    for path in output_root.rglob("*.pex")
                    if path.is_file()
                }
                if output_root.is_dir()
                else set()
            )
            unexpected_pex = sorted(actual_pex - expected_pex)
            if unexpected_pex:
                raise RuntimeError(
                    f"{failure_subject} has stale PEX outside the exact runtime inventory: "
                    + ", ".join(unexpected_pex[:10])
                )
            for class_name in entries_by_class:
                stale_pex = output_root / _script_relative_path(class_name, ".pex")
                if stale_pex.is_file():
                    try:
                        stale_pex.unlink()
                    except OSError as exc:
                        raise RuntimeError(
                            f"could not remove stale current-run PEX {stale_pex}: {exc}"
                        ) from exc

        compilable = [name for name in entries_by_class if name not in resolutions]
        if compilable and not self._req.options.convert_scripts:
            for class_name in compilable:
                resolutions[class_name] = _ScriptResolution(
                    class_name,
                    "compile_disabled",
                    message="convert_scripts is disabled",
                )
        elif compilable:
            workers = int(getattr(self._req.options, "conversion_workers", 1) or 1)
            source_index = {
                _script_key(class_name): psc_path
                for class_name, psc_path in psc_paths.items()
            }
            for class_name, resolution in self._compile_decompiled_scripts_for_fo4(
                compilable,
                source_index=source_index,
                ctx=ctx,
                runner=runner,
                workers=max(workers, 1),
                psc_paths=psc_paths,
            ):
                resolutions[class_name] = resolution
        for class_name in entries_by_class:
            resolutions.setdefault(
                class_name,
                _ScriptResolution(
                    class_name,
                    "compile_failed",
                    message="compiler returned no current-run result",
                ),
            )

        if exact_runtime_tree:
            output_root = (Path(ctx.mod_path) / "data" / "Scripts").resolve()
            for class_name, resolution in list(resolutions.items()):
                expected_output = (
                    output_root / _script_relative_path(class_name, ".pex")
                ).resolve()
                if resolution.status != "compiled":
                    continue
                if (
                    resolution.pex_path is None
                    or resolution.pex_path.resolve() != expected_output
                    or not expected_output.is_file()
                    or expected_output.stat().st_size <= 0
                ):
                    resolutions[class_name] = _ScriptResolution(
                        class_name,
                        "compile_failed",
                        message="compiler returned no fresh exact-manifest PEX",
                    )
            actual_pex = (
                {
                    str(path.resolve()).casefold()
                    for path in output_root.rglob("*.pex")
                    if path.is_file()
                }
                if output_root.is_dir()
                else set()
            )
            expected_pex = {
                str(
                    (output_root / _script_relative_path(name, ".pex")).resolve()
                ).casefold()
                for name in entries_by_class
            }
            if actual_pex - expected_pex:
                raise RuntimeError(
                    f"{failure_subject} compiler produced unexpected PEX output: "
                    + ", ".join(sorted(actual_pex - expected_pex)[:10])
                )

        payload = {
            "source": report_source,
            "psc_root": str(psc_root),
            "strict_quest_slice": strict,
            "psc_files_written": psc_files_written,
            "generated_psc_classes": manifest,
            "results": [
                {
                    "class_name": class_name,
                    "relative_source_path": entries_by_class[class_name][
                        "relative_source_path"
                    ],
                    "kind": entries_by_class[class_name]["kind"],
                    "required": entries_by_class[class_name]["required"],
                    "status": resolution.status,
                    "pex_path": str(resolution.pex_path)
                    if resolution.pex_path is not None
                    else None,
                    "message": resolution.message,
                }
                for class_name, resolution in sorted(
                    resolutions.items(), key=lambda item: item[0].casefold()
                )
            ],
        }
        report_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        all_failures = [
            resolution
            for resolution in resolutions.values()
            if resolution.status != "compiled"
        ]
        failures = [
            resolution
            for resolution in all_failures
            if (entries_by_class[resolution.script_name]["required"])
        ]
        runner.emit_log(
            "INFO" if not all_failures else "WARN",
            f"[{workflow_label} Scripts] current-run PSC manifest: "
            f"classes={len(manifest)} compiled={len(resolutions) - len(all_failures)} "
            f"failed={len(all_failures)} report={report_path}",
        )
        if strict and failures:
            details = "; ".join(
                f"{resolution.script_name}: {resolution.status}"
                for resolution in failures[:10]
            )
            raise RuntimeError(
                f"required {failure_subject} PSC compilation failed: {details}"
            )
        evidence: list[dict[str, Any]] = []
        for class_name in entries_by_class:
            resolution = resolutions[class_name]
            pex_path = resolution.pex_path
            if pex_path is not None:
                try:
                    relative_pex_path = (
                        Path(pex_path)
                        .resolve()
                        .relative_to(Path(ctx.mod_path).resolve())
                        .as_posix()
                    )
                except ValueError:
                    relative_pex_path = f"Scripts/{class_name}.pex"
            else:
                relative_pex_path = f"Scripts/{class_name}.pex"
            evidence.append(
                {
                    "class_name": class_name,
                    "relative_pex_path": relative_pex_path,
                    "compiled_success": resolution.status == "compiled",
                }
            )
        return evidence

    def _compile_fnv_generated_psc_manifest(
        self,
        *,
        manifest: list[dict[str, Any]],
        psc_files_written: int,
        ctx: ConversionContext,
        runner: "ConversionRunner",
    ) -> list[dict[str, Any]]:
        from bacup_lib.conversion_runtime import prepare_fnv_native_manifest, stage_fnv_native_runtime

        original_count = len(manifest)
        manifest, native_consumers = prepare_fnv_native_manifest(Path(ctx.mod_path), manifest)
        evidence = self._compile_generated_psc_manifest(
            manifest=manifest,
            psc_files_written=psc_files_written + len(manifest) - original_count,
            ctx=ctx,
            runner=runner,
            workflow_label="FNV",
            report_filename="fnv_script_compile_manifest.json",
            report_source="fnv_legacy_scripting",
            strict=self._is_fnv_quest_slice(),
            failure_subject="FNV quest-slice",
        )
        stage_fnv_native_runtime(Path(ctx.mod_path), native_consumers, evidence)
        return evidence

    def _apply_fnv_legacy_result_dict(
        self,
        result_dict: dict,
        ctx: ConversionContext,
        runner: "ConversionRunner",
        rust_run=None,
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
            message = (
                "FNV legacy scripting returned VMAD intents without native attachment; "
                "Python record mutation fallback has been removed"
            )
            runner.emit_log("ERROR" if self._is_fnv_quest_slice() else "WARN", message)
            if self._is_fnv_quest_slice():
                raise RuntimeError(message)

        ctx.summary.records_translated += records_written
        ctx.summary.records_warnings += records_failed + len(skipped_records)
        _append_unique_strings(ctx.summary.lip_regeneration_needed, list(lip_paths))

        if skipped_records:
            grouped: Counter[tuple[str, str]] = Counter()
            examples: dict[tuple[str, str], list[str]] = {}
            for skipped in skipped_records:
                try:
                    signature, editor_id, reason = skipped
                except (TypeError, ValueError):
                    signature, editor_id, reason = (
                        "<unknown>",
                        "<unknown>",
                        str(skipped),
                    )
                key = (str(signature).upper(), str(reason))
                grouped[key] += 1
                examples.setdefault(key, []).append(str(editor_id))
            ctx.fnv_legacy_skip_reasons = {
                f"{signature}:{reason}": count
                for (signature, reason), count in sorted(grouped.items())
            }
            for (signature, reason), count in sorted(grouped.items()):
                if signature != "SCPT":
                    continue
                sample = ", ".join(examples[(signature, reason)][:3])
                runner.emit_log(
                    "WARN",
                    f"FNV legacy SCPT skipped: reason={reason!r} count={count} "
                    f"examples={sample}",
                )

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

        if self._is_fnv_quest_slice() and (
            records_failed != 0 or skipped_records or psc_files_skipped != 0
        ):
            raise RuntimeError(
                "strict FNV quest slice produced incomplete accounting: "
                f"records_failed={records_failed} "
                f"skipped_records={len(skipped_records)} "
                f"psc_files_skipped={psc_files_skipped}"
            )

        manifest = self._fnv_generated_psc_manifest(result_dict)
        compile_evidence = self._compile_fnv_generated_psc_manifest(
            manifest=manifest,
            psc_files_written=psc_files_written,
            ctx=ctx,
            runner=runner,
        )
        if rust_run is not None:
            from bacup_lib.native_runtime import load_native_module as _conv_native

            reconciliation = (
                _conv_native().conversion_run_fnv_reconcile_compiled_scripts(
                    rust_run.id,
                    compile_evidence,
                )
            )
            required_fragments = int(
                reconciliation.get("fragment_bindings_expected", -1)
            )
            attached_fragments = int(
                reconciliation.get("fragment_bindings_attached", 0)
            )
            if self._is_fnv_quest_slice() and attached_fragments != required_fragments:
                raise RuntimeError(
                    "required FNV fragment VMAD reconciliation failed: "
                    f"expected={required_fragments} attached={attached_fragments}"
                )
            runner.emit_log(
                "INFO",
                "[FNV Scripts] reconciled compiled scripts before plugin save: "
                f"standalone={int(reconciliation.get('intents_built', 0))} "
                f"fragments={attached_fragments}",
            )

        voice_manifest = result_dict.get("voice_manifest") or {
            "version": 1,
            "entries": [],
        }
        voice_entries = voice_manifest.get("entries", [])
        if voice_entries:
            from bacup_lib.fnv_voice import (
                process_voice_manifest,
                write_voice_processing_report,
            )
            from creation_lib.paths import get_resource_dir

            report_root = Path(getattr(ctx, "diagnostics_root", None) or ctx.mod_path)
            manifest_path = report_root / "fnv_voice_manifest.json"
            manifest_path.write_text(
                json.dumps(voice_manifest, indent=2) + "\n", encoding="utf-8"
            )
            source_data_value = (
                getattr(ctx, "source_data_dir", None) or self._req.source_data_dir
            )
            if not source_data_value:
                raise RuntimeError(
                    "FNV voice manifest has entries but no source data directory"
                )
            source_data_roots = [Path(source_data_value)]
            source_data_roots.extend(
                Path(root)
                for root in (
                    getattr(ctx, "additional_source_asset_roots", ())
                    or getattr(self._req, "additional_source_asset_roots", ())
                    or ()
                )
            )
            source_data_roots = list(dict.fromkeys(source_data_roots))
            voice_results = process_voice_manifest(
                manifest_path,
                source_roots={
                    "fnv-base": tuple(source_data_roots),
                    "fo3-base": tuple(source_data_roots),
                },
                output_mod_dir=Path(ctx.mod_path),
                target_plugin=str(getattr(ctx, "output_plugin_name", "") or ""),
                resource_dir=get_resource_dir(),
                strict=self._is_fnv_quest_slice(),
                workers=int(
                    getattr(ctx, "conversion_workers", None)
                    or getattr(self._req.options, "conversion_workers", 1)
                    or 1
                ),
            )
            voice_report = report_root / "fnv_voice_processing_report.json"
            write_voice_processing_report(voice_results, voice_report)
            voice_failures = [
                result
                for result in voice_results
                if result.status not in {"written", "already_exists"}
            ]
            if self._is_fnv_quest_slice() and voice_failures:
                details = "; ".join(
                    f"{result.info_form_id}:{result.response_index}={result.status}"
                    for result in voice_failures[:10]
                )
                raise RuntimeError(f"required FNV voice processing failed: {details}")

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
        self._apply_fnv_legacy_result_dict(result_dict, ctx, runner, rust_run)

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
            or getattr(ctx, "source_data_dir", None)
            or self._req.source_data_dir
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
                "ghoul_male": (
                    "Meshes/Actors/Character/CharacterAssets/MaleGhoulHead.nif",
                ),
                "ghoul_female": (
                    "Meshes/Actors/Character/CharacterAssets/FemaleGhoulHead.nif",
                ),
                "child_male": (
                    "Meshes/Actors/Character/CharacterAssets/ChildMaleHead.nif",
                ),
                "child_female": (
                    "Meshes/Actors/Character/CharacterAssets/ChildFemaleHead.nif",
                ),
            }
            bones_candidates = {
                "male": (
                    "Meshes/Actors/Character/CharacterAssets/BaseMaleHead_faceBones.nif",
                ),
                "female": (
                    "Meshes/Actors/Character/CharacterAssets/BaseFemaleHead_faceBones.nif",
                ),
                "ghoul_male": (
                    "Meshes/Actors/Character/CharacterAssets/MaleGhoulHead_faceBones.nif",
                ),
                "ghoul_female": (
                    "Meshes/Actors/Character/CharacterAssets/FemaleGhoulHead_faceBones.nif",
                ),
            }
            for sex, candidates in head_candidates.items():
                for candidate in candidates:
                    if not target_asset_store.has_asset(candidate):
                        continue
                    head = target_asset_store.materialize(candidate)
                    if head is not None:
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
                "correspondence_path_ghoul_male": str(
                    _face_res / "fnv_to_fo4_correspondence_ghoul_male.npz"
                ),
                "correspondence_path_ghoul_female": str(
                    _face_res / "fnv_to_fo4_correspondence_ghoul_female.npz"
                ),
                "uv_lut_path_ghoul_male": str(
                    _face_res / "fnv_to_fo4_facetint_uv_lut_ghoul_male.npz"
                ),
                "uv_lut_path_ghoul_female": str(
                    _face_res / "fnv_to_fo4_facetint_uv_lut_ghoul_female.npz"
                ),
                "correspondence_path_child_male": str(
                    _face_res / "fnv_to_fo4_correspondence_child_male.npz"
                ),
                "correspondence_path_child_female": str(
                    _face_res / "fnv_to_fo4_correspondence_child_female.npz"
                ),
                "uv_lut_path_child_male": str(
                    _face_res / "fnv_to_fo4_facetint_uv_lut_child_male.npz"
                ),
                "uv_lut_path_child_female": str(
                    _face_res / "fnv_to_fo4_facetint_uv_lut_child_female.npz"
                ),
                "output_plugin_name": output_plugin_name,
                "translation_maps_dir": str(native_translation_maps_dir()),
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
            if phase_error is not None:
                _record_phase_failure(self._req, phase_name, phase_error)
                # Translation creates mapper state required by every dependent record phase.
                if raise_on_error and (
                    phase_name == "Translate Records" or not _continue_on_error(self._req)
                ):
                    raise phase_error
            elif progress.status == "error":
                _record_phase_failure(
                    self._req, phase_name, RuntimeError(progress.error or "reported an error")
                )

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
        not_found_prefixes = (
            f"[WARN] {label} not found: ",
            f"[ERROR] {label} not found: ",
        )
        failed_prefix = f"[ERROR] {label} failed: "
        for line in log_lines:
            matched = next((p for p in not_found_prefixes if line.startswith(p)), None)
            if matched is not None:
                failed.append(
                    _path_through_extension(
                        line.removeprefix(matched),
                        ".nif" if label == "NIF" else ".dds",
                    )
                )
            elif line.startswith(failed_prefix):
                failed.append(_path_before_error(line.removeprefix(failed_prefix)))
            elif label == "NIF" and line.startswith("[ERROR] convert_nifs failed "):
                failed.append(
                    _path_through_extension(
                        line.removeprefix("[ERROR] convert_nifs failed "), ".nif"
                    )
                )
            elif label == "Texture":
                for prefix in (
                    "[ERROR] texture task failed ",
                    "[ERROR] texture job failed ",
                    "[ERROR] texture group failed ",
                ):
                    if line.startswith(prefix):
                        failed.append(line.removeprefix(prefix))
                        break
        return failed

    @staticmethod
    def _failed_material_paths(log_lines: list[str]) -> list[str]:
        failed: list[str] = []
        not_found_prefixes = (
            "[WARN] Material not found: ",
            "[ERROR] Material not found: ",
        )
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
        "legacy_pack_origins": [row.to_dict() for row in request.legacy_pack_origins],
        "legacy_runtime_origins": [
            dict(row)
            for row in getattr(request, "legacy_runtime_origins", ()) or ()
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
        "fnv_quest_slice": bool(request.options.fnv_quest_slice),
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
        runner.emit_complete(
            str(self._req.output_root), self._record_runtime._aggregate_summary
        )

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

    def _run_fo4_starfield_record_tail(
        self,
        ctx: ConversionContext,
        orch: "_UnifiedRecordRuntime",
        runner: "ConversionRunner",
        opts,
        phase_offset: int,
    ) -> int:
        """The fo4:starfield record track between translate and build_esp.

        Order is load-bearing:

        * ``terrain_btd_write`` resolves the BTD's LTEX palette through the
          run's ``mapper_state``.
        * ``starfield_cells`` translates the ``CELL``/``REFR`` records
          ``fo4_to_starfield.yaml`` skip-lists (parentage is group topology,
          not a subrecord) and nests them under the ``WRLD`` / interior block
          trees. It resolves placed-ref bases and both XTEL endpoints through
          ``mapper_state`` and must precede ``audio_rewire``, which patches the
          CELL records it creates. It follows ``terrain_btd_write`` only so the
          asset track's A3 terrain gate is released before this long walk; the
          two have no data dependency.
        * ``wwise_audio`` allocates FormKeys for the ``WWED`` records it
          synthesizes through ``mapper_state`` (its ``__synth_wwise_audio__``
          symbol) and hard-errors when it is None.
        * ``audio_rewire`` joins on ``wwise_audio``'s
          ``debug/wwise/events_manifest.json`` and resolves each row's FO4
          FormKey through the mapper. Its REGN->``CELL.XCMO`` compensation and
          placeholder ``XCMO``/``XCAS`` rewire walk the CELL records
          ``starfield_cells`` creates.

        All four run after ``translate`` (which seeds ``mapper_state``) and
        before the master handles are released. Returns the number of phase
        slots consumed, for the caller's phase numbering.
        """
        rust_run = getattr(ctx, "_rust_conversion_run", None)
        if rust_run is None:
            runner.emit_log("WARN", "fo4:starfield record tail: no rust run; skipping")
            self._mark_terrain_done()
            return 0

        mod_path = str(ctx.mod_path)
        source_extracted = str(
            getattr(ctx, "source_data_dir", None) or self._req.source_data_dir or ""
        )
        used = 0

        if opts.convert_terrain:
            worldspace = (
                getattr(opts.terrain, "worldspace_editor_id", "")
                or FO4_STARFIELD_MVP_WORLDSPACE
            )
            btd_params: dict = {"worldspace_editor_id": worldspace}
            extent_cells = getattr(opts.terrain, "extent_cells", None)
            if extent_cells:
                btd_params["terrain_extent_cells"] = int(extent_cells)
            self._record_phase(
                3 + phase_offset + used,
                "Write Starfield BTD Terrain",
                lambda p, _p=btd_params: rust_run.run_phase(
                    "terrain_btd_write", mod_path=mod_path, params=_p
                ),
                runner,
                timing_ctx=ctx,
            )
            used += 1
        # The asset track's A3 gate waits on this; release it whether or not a
        # BTD was written so the asset waves never deadlock.
        self._mark_terrain_done()

        self._record_phase(
            3 + phase_offset + used,
            "Emit Starfield Cells + Placed Refs",
            lambda p: rust_run.run_phase(
                "starfield_cells", mod_path=mod_path, params={}
            ),
            runner,
            timing_ctx=ctx,
        )
        used += 1

        placeholder_audio = bool(getattr(opts, "placeholder_audio", False))
        wwise_params: dict = {"placeholder_audio": placeholder_audio}
        if not placeholder_audio:
            tools = _fo4_starfield_wwise_tools(ctx)
            if tools is None:
                raise RuntimeError(
                    "fo4:starfield real-audio mode needs the Wwise 2021.1.x toolchain: "
                    "set WWISEROOT (and FFMPEG_EXE / XWMAENCODE_EXE if they are not "
                    "beside it), or re-run with --placeholder-audio"
                )
            wwise_params["tools"] = tools
        self._record_phase(
            3 + phase_offset + used,
            "Wwise Audio"
            + (" (placeholder)" if placeholder_audio else " (bank build)"),
            lambda p, _p=wwise_params, _se=source_extracted: rust_run.run_phase(
                "wwise_audio",
                mod_path=mod_path,
                source_extracted_dir=_se,
                params=_p,
            ),
            runner,
            timing_ctx=ctx,
        )
        used += 1

        self._record_phase(
            3 + phase_offset + used,
            "Audio Rewire",
            lambda p: rust_run.run_phase("audio_rewire", mod_path=mod_path, params={}),
            runner,
            timing_ctx=ctx,
        )
        used += 1
        return used

    def _convert_record_track(
        self, source_plugin: Path, runner: "ConversionRunner"
    ) -> None:
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
            ctx.mvp_creature_corpus_policy = orch._active_mvp_creature_corpus_policy()
            ctx.mvp_creature_profile = orch._active_mvp_creature_profile()
            ctx.mvp_melee_policy = orch._active_mvp_melee_policy()
            ctx.mvp_record_exceptions = orch._active_mvp_record_exceptions()
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
                raise ValueError("cell-bounds runs are not supported in unified driver")

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
            asset_transition_started = time.perf_counter()
            ctx.assets = assets
            ctx.shared_asset_conversion_enabled = bool(
                opts.convert_nifs or opts.convert_textures or opts.convert_materials
            )
            self.assets = assets
            self.signals.assets_ready.set()
            _record_timing(
                ctx,
                "unified_asset_handoff",
                asset_transition_started,
                scope="detail",
                parent="record_track",
                assets=len(assets),
            )

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
                registry_apply_started = time.perf_counter()
                orch._apply_registry_mappings(ctx)
                _record_timing(
                    ctx,
                    "unified_registry_mappings_apply",
                    registry_apply_started,
                    scope="detail",
                    parent="record_track",
                )
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
                mvp_creature_profile = _exact_mvp_creature_profile(ctx)
                if mvp_creature_profile is not None:
                    rust_run.run_phase(
                        "emit_mvp_creature",
                        mod_path=str(ctx.mod_path),
                        params={
                            "profile": mvp_creature_profile,
                            "register_only": True,
                        },
                    )
                orch._run_mvp_creature_corpus_execution(rust_run, ctx, runner)
                _run_fnv_weapon_animation_phase(
                    self._req, ctx, rust_run, orch._aggregate_summary, mvp_creature_profile
                )
                _run_skyrim_weapon_animation_phase(
                    self._req, ctx, rust_run, orch._aggregate_summary, mvp_creature_profile
                )

            # addon_index_map harvest happened inside _translate_records_rust
            # emit record-run warnings inline.
            self.addon_index_map = dict(getattr(ctx, "addon_index_map", {}) or {})
            self.signals.fixups_done.set()

            post_terrain_phase_offset = 0
            condition_inputs_retained = False
            if _is_fo4_starfield(ctx):
                post_terrain_phase_offset = self._run_fo4_starfield_record_tail(
                    ctx, orch, runner, opts, phase_offset
                )
            elif opts.convert_terrain:
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
                        raise_on_error=True,
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
                        condition_inputs_retained = opts.build_esp and self._req.source_game == "fo76" and self._req.target_game == "fo4"
                        if not condition_inputs_retained:
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
                                "conversion_workers": getattr(
                                    ctx, "conversion_workers", None
                                ),
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

                if self._req.source_game == "fo76" and self._req.target_game == "fo4":
                    self._record_phase(
                        next_phase_no,
                        "Inventory Quest Runtime Routes",
                        lambda p: orch._run_quest_runtime_inventory_phase(ctx, runner),
                        runner,
                        timing_ctx=ctx,
                    )
                    next_phase_no += 1

                if rust_run is not None:
                    if self._req.source_game == "skyrimse" and self._req.target_game == "fo4":
                        def _prepare_skyrim_magic_runtime(_progress) -> None:
                            from bacup_lib.conversion_runtime import stage_conversion_runtime

                            rust_run.run_phase(
                                "prepare_skyrim_magic_runtime", mod_path=str(ctx.mod_path), params={}
                            )
                            stage_conversion_runtime(Path(ctx.mod_path))

                        self._record_phase(
                            next_phase_no,
                            "Skyrim Creature Magic Runtime",
                            _prepare_skyrim_magic_runtime,
                            runner,
                            timing_ctx=ctx,
                            raise_on_error=True,
                        )
                        next_phase_no += 1
                    if self._req.source_game in {"fnv", "fo3"} and self._req.target_game == "fo4":
                        self._record_phase(
                            next_phase_no,
                            "Legacy Creature Runtime",
                            lambda p: rust_run.run_phase(
                                "prepare_fnv_radiation_runtime", mod_path=str(ctx.mod_path), params={}
                            ),
                            runner,
                            timing_ctx=ctx,
                            raise_on_error=True,
                        )
                        next_phase_no += 1
                    if self._req.source_game == "fo76" and self._req.target_game == "fo4":
                        self._record_phase(
                            next_phase_no,
                            "Equipment Condition Catalog",
                            lambda p: orch._emit_equipment_condition_catalog(rust_run, ctx, runner),
                            runner,
                            timing_ctx=ctx,
                            raise_on_error=True,
                        )
                        next_phase_no += 1
                    if condition_inputs_retained:
                        rust_run.release_remap_state()
                        rust_run.release_master_handles()
                    _memory_mark(ctx, "before:native_source_release")
                    handle_cleanup_started = time.perf_counter()
                    source_released = rust_run.release_source_handle()
                    if source_released:
                        _trim_bacup_native_allocator()
                    _memory_mark(ctx, "after:native_source_release")
                    _record_timing(
                        ctx,
                        "unified_source_handle_release",
                        handle_cleanup_started,
                        scope="detail",
                        parent="record_track_cleanup",
                        released=source_released,
                    )

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
                    if self._req.source_game in {"fnv", "fo3"} and self._req.target_game == "fo4":
                        from bacup_lib.conversion_runtime import stage_fnv_catalog_runtime

                        stage_fnv_catalog_runtime(Path(ctx.mod_path), ctx.output_plugin_name)
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
                drain_started = time.perf_counter()
                orch._drain_and_drop_rust_run(ctx)
                _record_timing(
                    ctx,
                    "unified_record_run_drain_drop",
                    drain_started,
                    scope="detail",
                    parent="record_track_cleanup",
                )
                # worldspace patch + source-close guard
                if plugin_error is None:
                    worldspace_patch_started = time.perf_counter()
                    orch._patch_projected_worldspace_subrecords(
                        ctx, runner, source_plugin
                    )
                    ctx._worldspace_patched = True
                    _record_timing(
                        ctx,
                        "unified_worldspace_final_patch",
                        worldspace_patch_started,
                        scope="detail",
                        parent="record_track_cleanup",
                    )
                if not getattr(ctx, "_source_closed", False):
                    source_close_started = time.perf_counter()
                    orch._close_source_handle(ctx)
                    ctx._source_closed = True
                    _memory_mark(ctx, "after:early_source_close")
                    _record_timing(
                        ctx,
                        "unified_source_handle_close",
                        source_close_started,
                        scope="detail",
                        parent="record_track_cleanup",
                    )

                if self._req.target_game == "fo4":
                    self._record_phase(
                        next_phase_no,
                        "Check Runtime Hazards",
                        lambda p: orch._check_output_runtime_hazards(ctx, runner),
                        runner,
                        timing_ctx=ctx,
                        raise_on_error=True,
                    )
                    next_phase_no += 1

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

            registry_update_started = time.perf_counter()
            orch._update_registry(ctx)
            _record_timing(
                ctx,
                "unified_registry_update",
                registry_update_started,
                scope="detail",
                parent="record_track_finalize",
            )
            summary_merge_started = time.perf_counter()
            orch._merge_summary(ctx.summary)
            _record_timing(
                ctx,
                "unified_summary_merge",
                summary_merge_started,
                scope="detail",
                parent="record_track_finalize",
            )
            result_merge_started = time.perf_counter()
            orch._merge_run_result(ctx)
            _record_timing(
                ctx,
                "unified_result_merge",
                result_merge_started,
                scope="detail",
                parent="record_track_finalize",
            )
        except Exception as exc:
            plugin_error = exc
            raise
        finally:
            # finally-block drains/closes. The record run shares nothing with
            # the asset waves' runs — drop it here.
            try:
                if ctx is not None:
                    final_drain_started = time.perf_counter()
                    orch._drain_and_drop_rust_run(ctx)
                    _record_timing(
                        ctx,
                        "unified_final_drain_drop",
                        final_drain_started,
                        scope="detail",
                        parent="record_track_finally",
                    )
            finally:
                try:
                    if (
                        ctx is not None
                        and plugin_error is None
                        and not getattr(ctx, "_worldspace_patched", False)
                    ):
                        final_worldspace_started = time.perf_counter()
                        orch._patch_projected_worldspace_subrecords(
                            ctx, runner, source_plugin
                        )
                        _record_timing(
                            ctx,
                            "unified_final_worldspace_patch",
                            final_worldspace_started,
                            scope="detail",
                            parent="record_track_finally",
                        )
                finally:
                    try:
                        if ctx is not None and not getattr(
                            ctx, "_source_closed", False
                        ):
                            final_source_close_started = time.perf_counter()
                            orch._close_source_handle(ctx)
                            _memory_mark(ctx, "after:source_close")
                            _record_timing(
                                ctx,
                                "unified_final_source_close",
                                final_source_close_started,
                                scope="detail",
                                parent="record_track_finally",
                            )
                    finally:
                        if ctx is not None:
                            final_masters_close_started = time.perf_counter()
                            orch._close_target_master_handles(ctx)
                            _memory_mark(ctx, "after:masters_close")
                            _record_timing(
                                ctx,
                                "unified_final_master_handles_close",
                                final_masters_close_started,
                                scope="detail",
                                parent="record_track_finally",
                            )
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
    # fo4:starfield replaces the whole FO4-target asset track with the three
    # Starfield writers; the `*_phase` names above are then unused.
    starfield_target: bool = False


def _wave_plan_for(source_game: str, target_game: str = "fo4") -> _AssetWavePlan:
    from creation_lib.core.game_profiles import get_profile

    if target_game.lower() == "starfield":
        # FO4 -> Starfield: `starfield_meshes` (A2) then `starfield_materials`
        # + `starfield_textures` (A3). None of the FO4-target phases apply —
        # different mesh container, different material format, different
        # texture channel layout.
        return _AssetWavePlan(
            nif_phase="starfield_meshes",
            bto_phase=None,
            texture_phase="starfield_textures",
            material_phase="starfield_materials",
            grass_topup=False,
            wave_a4=False,
            textures_from_graph=True,
            starfield_target=True,
        )
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
    if get_profile(source_game).engine == "creation2":
        # No FO76-shaped .bto LOD format; hybrid collision + material
        # conversion are handled inside the NIF converter and
        # convert_materials_v2 (cdb_to_bgsm), so wave A4's Havok
        # behavior/animation/skeleton/facegen stages have nothing to do.
        return _AssetWavePlan(bto_phase=None, wave_a4=False)
    return _AssetWavePlan()


def _has_havok_postprocess(ctx, opts) -> bool:
    return (
        bool(opts.convert_havok)
        and _wave_plan_for(str(ctx.source_game), str(ctx.target_game)).wave_a4
        and getattr(ctx, "mvp_creature_corpus_policy", None) is None
        and _exact_mvp_creature_profile(ctx) is None
    )


def _requires_fo76_behavior_animtext(ctx, opts) -> bool:
    return (
        ctx is not None
        and ctx.source_game == "fo76"
        and ctx.target_game == "fo4"
        and _has_havok_postprocess(ctx, opts)
        and (Path(ctx.mod_path) / "debug/fo76_behaviors/plan.json").is_file()
    )


#: The one FO4 worldspace the fo4:starfield MVP ports. Used when
#: `TerrainOptions.worldspace_editor_id` is left unset.
FO4_STARFIELD_MVP_WORLDSPACE = "Commonwealth"


def _is_fo4_starfield(ctx) -> bool:
    """True for the FO4 -> Starfield pair.

    This pair inverts the repo's usual "everything targets FO4" assumption: its
    record tail, asset waves, and packaging are Starfield-shaped and share no
    branches with the FO4-target phases; this predicate is the whole gate.
    """
    return (
        str(getattr(ctx, "source_game", "")).lower() == "fo4"
        and str(getattr(ctx, "target_game", "")).lower() == "starfield"
    )


def _fo4_starfield_wwise_tools(ctx) -> dict | None:
    """Build `wwise_audio`'s `tools` params from the local Wwise install.

    Returns None when `WWISEROOT` is unset — the caller turns that into an
    actionable error pointing at `--placeholder-audio` (the 2021.1.x authoring
    install is licence-gated and can never exist in CI).
    """
    wwise_root = os.environ.get("WWISEROOT", "").strip()
    if not wwise_root:
        return None
    bin_dir = Path(wwise_root) / "Authoring" / "x64" / "Release" / "bin"
    ffmpeg = os.environ.get("FFMPEG_EXE", "").strip() or "ffmpeg"
    xwmaencode = os.environ.get("XWMAENCODE_EXE", "").strip() or "xwmaencode.exe"
    # The Wwise project Bethesda ships with Starfield's tools drop.
    target_data = getattr(ctx, "target_data_dir", None)
    project_src = (
        str(Path(target_data).parent / "Tools" / "wwise" / "Starfield")
        if target_data
        else ""
    )
    return {
        "xwm_decode_cmd": [xwmaencode, "{in}", "{out}"],
        "ffmpeg_cmd": [
            ffmpeg,
            "-y",
            "-i",
            "{in}",
            "-ar",
            "48000",
            "-ac",
            "2",
            "{out}",
        ],
        "wwise_console": str(bin_dir / "WwiseConsole.exe"),
        "copy_streamed_files": str(bin_dir / "CopyStreamedFiles.exe"),
        "wwise_project_src": project_src,
    }


def _strip_leading_dir(rel: str, folder: str) -> str:
    """`meshes/a/b.nif` -> `a/b.nif` (case-insensitive, `\\` tolerated)."""
    normalized = str(rel).replace("\\", "/").lstrip("/")
    prefix = f"{folder}/"
    if normalized[: len(prefix)].casefold() == prefix:
        return normalized[len(prefix) :]
    return normalized


def starfield_mesh_entries(nif_assets) -> list[dict]:
    """`starfield_meshes` `nifs` rows.

    `nif_rel` is relative to the mod's `data/meshes/` (the phase joins it under
    `meshes/` itself), so the graph's `meshes/` prefix comes off here.
    """
    return [
        {
            "nif_rel": _strip_leading_dir(asset.source_path, "meshes"),
            "resolved_path": asset.resolved_path or "",
        }
        for asset in nif_assets
    ]


def starfield_material_entries(material_assets) -> list[dict]:
    """`starfield_materials` `materials` rows.

    `bgsm_rel` KEEPS its `materials/` prefix: `mat_out_rel` maps it straight to
    `<mod>/data/materials/...mat`, and the same string (lowercased,
    backslashed) is the MaterialID CRC input the mesh writer stamps into each
    NIF, so stripping the prefix here would desync the two.
    """
    return [
        {
            "bgsm_rel": str(asset.source_path).replace("\\", "/").lstrip("/"),
            "resolved_path": asset.resolved_path or "",
        }
        for asset in material_assets
    ]


def starfield_texture_sets(
    material_assets, resolved_textures: dict[str, str]
) -> tuple[list[dict], list[str]]:
    sets, warnings = json.loads(load_native_module().conversion_starfield_texture_sets(
        [(str(asset.source_path), str(asset.resolved_path or "")) for asset in material_assets],
        resolved_textures,
    ))
    return sets, warnings

def _mvp_exclude_signatures(driver: "UnifiedDriver") -> frozenset[str]:
    from bacup_lib.source_pairs import MVP_EXCLUDE_SIGNATURES_BY_PAIR

    excluded = frozenset(
        signature.upper()
        for signature in getattr(driver._req.options, "exclude_signatures", ())
    )
    source_game = str(driver.ctx.source_game).lower()
    target_game = str(driver.ctx.target_game).lower()
    pair_id = {
        ("fnv", "fo4"): "fnvfo3:fo4",
        ("skyrimse", "fo4"): "skyrimse:fo4",
        ("starfield", "fo4"): "starfield:fo4",
    }.get((source_game, target_game))
    expected = MVP_EXCLUDE_SIGNATURES_BY_PAIR.get(pair_id or "", frozenset())
    if pair_id == "starfield:fo4":
        return expected
    return expected if expected and expected <= excluded else frozenset()


def _is_skyrim_mvp(driver: "UnifiedDriver") -> bool:
    return str(driver.ctx.source_game).lower() == "skyrimse" and bool(
        _mvp_exclude_signatures(driver)
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


def _is_skyrim_mvp_loadscreen_asset_path(path: str, asset_type: str = "") -> bool:
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
    if parts[0] in {"materials", "meshes"} and parts[1:3] == ["_byoh", "loadsscreens"]:
        return filename.endswith((".bgsm", ".nif"))
    if parts[:2] == ["textures", "clutter"]:
        return filename.startswith("loadscreen") and filename.endswith(".dds")
    if parts[:3] == ["sound", "fx", "ui"]:
        return filename.startswith(
            ("loadscreen", "ui_loadscreen")
        ) and filename.endswith(".wav")
    return (
        parts[0] in {"materials", "meshes"}
        and "loadscreenart" in parts[1:]
        and filename.endswith((".bgsm", ".nif"))
    )


def _is_skyrim_mvp_loadscreen_mesh_path(path: str, asset_type: str = "") -> bool:
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
    for relative_root in (
        "Meshes/_BYOH/LoadsScreens",
        "Sound/FX/UI",
        "Textures/Clutter",
    ):
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


def _is_world_only_mvp_asset_path(
    path: str,
    asset_type: str = "",
    exclude_signatures: frozenset[str] | None = None,
) -> bool:
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
    excluded_namespaces = {"actors", "characters", "creatures"}
    if exclude_signatures is None or {"ARMA", "ARMO"} & exclude_signatures:
        excluded_namespaces.update({"armor", "clothes"})
    if exclude_signatures is None or "WEAP" in exclude_signatures:
        excluded_namespaces.add("weapons")
    return not semantic_parts or semantic_parts[0] not in excluded_namespaces


def _is_world_only_mvp_asset(
    asset: AssetRef,
    exclude_signatures: frozenset[str],
    asset_closure=None,
) -> bool:
    from bacup_lib.base_asset_dedupe import asset_owner_signature

    if asset_closure is not None and asset_closure.allows(asset):
        return True
    if not _is_world_only_mvp_asset_path(
        asset.source_path, asset.asset_type, exclude_signatures
    ):
        return False
    owner_signature = asset_owner_signature(asset)
    if owner_signature:
        return owner_signature not in exclude_signatures
    return True


@dataclass(frozen=True)
class _MvpAssetClosureSet:
    closures: tuple[object, ...]

    def allows(self, asset: AssetRef) -> bool:
        return any(closure.allows(asset) for closure in self.closures)


def _mvp_asset_closure_set(driver: "UnifiedDriver", assets: list[AssetRef]):
    from bacup_lib.creature_mvp_assets import creature_mvp_asset_closure

    weapon_plan = getattr(driver.ctx, "_weapon_mvp_asset_plan", None)
    if getattr(driver._req.options, "mvp_melee_only", False):
        if weapon_plan is None:
            raise RuntimeError(
                "melee-only asset conversion requires an authoritative weapon receipt plan"
            )
        return _MvpAssetClosureSet((weapon_plan,))
    closures = tuple(
        closure
        for closure in (
            weapon_plan,
            creature_mvp_asset_closure(_exact_mvp_creature_profile(driver.ctx), assets),
        )
        if closure is not None
    )
    return _MvpAssetClosureSet(closures) if closures else None


def _is_selected_mvp_asset(
    driver: "UnifiedDriver",
    asset: AssetRef,
    exclude_signatures: frozenset[str],
    asset_closure,
) -> bool:
    if getattr(driver._req.options, "mvp_melee_only", False):
        return asset_closure is not None and asset_closure.allows(asset)
    return _is_world_only_mvp_asset(asset, exclude_signatures, asset_closure)


def _is_skyrim_mvp_world_asset_path(path: str, asset_type: str = "") -> bool:
    from bacup_lib.source_pairs import SKYRIM_MVP_EXCLUDE_SIGNATURES

    return _is_world_only_mvp_asset_path(
        path, asset_type, SKYRIM_MVP_EXCLUDE_SIGNATURES
    )


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


def _sequential_wave_plan(stages: list[WaveStage]) -> list[dict]:
    plan_stages: list[dict] = []
    previous_phase: str | None = None
    for stage in stages:
        plan_stage = stage.to_plan_stage()
        if previous_phase is not None and previous_phase not in plan_stage["after"]:
            plan_stage["after"].append(previous_phase)
        plan_stages.append(plan_stage)
        previous_phase = stage.phase
    return plan_stages


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

        self._ctx = ctx
        self._source_asset_inventory = getattr(ctx, "source_asset_inventory", None)
        created_runs = []

        def make(needs_relocation: bool, needs_inventory: bool):
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
                "native_assets": bool(getattr(ctx, "native_assets", False)),
                "replace_fo4_materials": bool(
                    getattr(ctx, "replace_fo4_materials", False)
                ),
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
            if _exact_mvp_creature_profile(ctx) is not None:
                config["skip_record_signatures"] = ["WEAP"]
                config["mvp_record_exceptions"] = list(
                    getattr(ctx, "mvp_record_exceptions", ()) or ()
                )
            run = ConversionRun.create_new(
                str(ctx.source_game),
                str(ctx.target_game),
                None,
                str(config["output_plugin_name"]),
                config=config,
            )
            if needs_inventory and self._source_asset_inventory is not None:
                try:
                    self._source_asset_inventory.attach_run(run.id)
                except BaseException:
                    run.close()
                    raise
            created_runs.append(run)
            return run

        try:
            self.textures = (
                make(True, True) if (toggles.textures or toggles.materials) else None
            )
            self.nifs = make(True, False) if (toggles.nifs or toggles.btos) else None
            self.havok = (
                make(False, toggles.havok)
                if (toggles.havok or toggles.drivers)
                else None
            )
            self.sounds = make(False, False) if toggles.sounds else None
        except BaseException:
            for run in created_runs:
                try:
                    run.close()
                except Exception:
                    pass
            setattr(ctx, "source_asset_inventory", None)
            self._source_asset_inventory = None
            raise

    def all_runs(self) -> list:
        return [
            r
            for r in (
                self.textures,
                self.nifs,
                self.havok,
                self.sounds,
            )
            if r
        ]

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
        setattr(self._ctx, "source_asset_inventory", None)
        self._source_asset_inventory = None


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
        planning_summary: ConversionSummary | None = None,
    ) -> None:
        self.driver = driver
        self.toggles = toggles
        self.runs = runs
        self.runner = runner
        self.conversion_workers = conversion_workers
        self.planning_summary = planning_summary
        self._wave_plan = _wave_plan_for(
            str(driver.ctx.source_game), str(driver.ctx.target_game)
        )
        # NIF entries seen by wave A2 — the A3 grass top-up converts only the
        # delta appended by the terrain phase.
        self._a2_nif_keys: set[tuple[str, str]] = set()

    def _shim(self):
        from bacup_lib.pipeline._shim import build_orchestrator_shim
        from bacup_lib.workflows.asset_phases import _weapon_model_key

        shim = build_orchestrator_shim([], self.driver.ctx)
        if self.planning_summary is not None:
            shim._summary = self.planning_summary
        shim.pbr_carry = bool(getattr(self.driver.ctx, "pbr_carry", False))
        shim.texture_landscape_mip_flooding = bool(
            getattr(self.driver.ctx, "texture_landscape_mip_flooding", False)
        )
        if self.conversion_workers is not None:
            shim.conversion_workers = int(self.conversion_workers)
        weapon_plan = getattr(self.driver.ctx, "_weapon_mvp_asset_plan", None)
        if weapon_plan is not None:
            role_claims: dict[str, set[str]] = {}
            for claim in weapon_plan.admitted_claims:
                if claim.asset[0] != "nif" or claim.model_kind not in {
                    "world",
                    "first_person",
                }:
                    continue
                key = _weapon_model_key(claim.asset[1])
                if key:
                    role_claims.setdefault(key, set()).add(claim.weapon_role)
            role_cache = {
                key: next(iter(roles))
                for key, roles in role_claims.items()
                if len(roles) == 1
            }
            shim._weapon_mvp_asset_plan = weapon_plan
            shim._weapon_metadata_rows = tuple(
                getattr(self.driver.ctx, "_weapon_metadata_rows", ()) or ()
            )
            shim._weapon_role_cache = role_cache
            shim._admitted_weapon_role_cache = role_cache
        return shim

    # -- A1: immediately after collect_assets ------------------------------

    def build_wave_a1(self) -> list["WaveStage"]:
        shim = self._shim()
        stages: list[WaveStage] = []
        if self._wave_plan.starfield_target:
            # No copy_sounds leg: Starfield plays Wwise banks, not loose
            # xwm/wav, so the record track's `wwise_audio` phase owns every
            # audio byte this pair emits (R6).
            self.runner.emit_log(
                "INFO",
                "[Sound] fo4:starfield audio is owned by the wwise_audio phase; "
                "no loose-sound copy",
            )
            return stages
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
            if (
                str(self.driver.ctx.source_game).lower() == "skyrimse"
                and str(self.driver.ctx.target_game).lower() == "fo4"
                and self.driver._req.options.translate_records
            ):
                receipt = getattr(self.driver.ctx, "skyrim_runtime_asset_receipt", None)
                allowed = {
                    normalize_asset_source_path(path).casefold()
                    for path in getattr(receipt, "output_paths", ())
                }
                removed = 0
                for asset in sound_assets:
                    if (
                        normalize_asset_source_path(asset.source_path).casefold()
                        in allowed
                    ):
                        continue
                    if shim._remove_stale_asset_output(asset):
                        removed += 1
                self.runner.emit_log(
                    "INFO",
                    "[Sound] Skyrim runtime assets are receipt-driven; skipped broad "
                    f"copy_sounds and removed {removed} stale output(s)",
                )
                return stages
            legacy_mp3_count = sum(
                1
                for asset in sound_assets
                if asset.resolved_path
                and Path(asset.resolved_path).suffix.casefold() == ".mp3"
            )
            if legacy_mp3_count:
                from bacup_lib.legacy_music import prepare_legacy_music_assets

                self.runner.emit_log(
                    "INFO",
                    f"[Sound] Encoding {legacy_mp3_count} legacy music track(s) to XWM",
                )
                sound_assets = prepare_legacy_music_assets(
                    sound_assets,
                    Path(shim.mod_path) / ".regen_music_cache",
                )
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
        if self._wave_plan.starfield_target:
            if self.toggles.nifs and self.runs.nifs:
                stages.append(self._build_starfield_meshes_stage(shim))
            return stages
        if self.toggles.nifs and self.runs.nifs:
            stages.append(self._build_nif_stage(shim, after=()))
            creature_stage = self._build_mvp_creature_nif_stage(shim)
            if creature_stage is not None:
                stages.append(creature_stage)
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
                        after=(self._wave_plan.nif_phase,) if self.toggles.nifs else (),
                    )
                )
        return stages

    def _build_mvp_creature_nif_stage(self, shim) -> "WaveStage | None":
        from bacup_lib.creature_mvp_assets import creature_mvp_asset_closure
        from bacup_lib.workflows.asset_phases import _params_for_convert_nifs

        closure = creature_mvp_asset_closure(
            _exact_mvp_creature_profile(self.driver.ctx),
            shim.graph.all_assets,
        )
        if closure is None:
            return None
        assets = [
            asset
            for asset in shim.graph.all_assets
            if asset.asset_type == "nif" and closure.allows(asset)
        ]
        unresolved = [asset.source_path for asset in assets if not asset.resolved_path]
        if len(assets) != 2 or unresolved:
            raise RuntimeError(
                f"{closure.label} MVP NIF closure is incomplete: "
                f"found={len(assets)} unresolved={unresolved}"
            )
        params = _params_for_convert_nifs(shim, assets)
        params["skin_policy"] = "preserve_source_rig"
        source_extracted_dir = str(shim.target_extracted_dir or "")
        if closure.profile == "skyrim_wolf":
            phase = "convert_mvp_creature_nifs_v2"
            params["bgsm_output_dir"] = str(
                Path(str(shim.mod_path)) / "data" / "Materials"
            )
        else:
            phase = "convert_mvp_creature_gamebryo_nifs"
            params = {
                "nif_paths": params["nif_paths"],
                "material_out_rel": (
                    f"materials/{Path(str(shim.mod_path)).name}/gamebryo"
                ),
                "skin_policy": "preserve_source_rig",
            }
            source_extracted_dir = ""
        return WaveStage(
            phase=phase,
            run_id=self.runs.nifs.id,
            mod_path=str(shim.mod_path),
            source_extracted_dir=source_extracted_dir,
            params=params,
            after=(self._wave_plan.nif_phase,),
        )

    # -- fo4:starfield asset track -----------------------------------------

    def _build_starfield_meshes_stage(self, shim) -> "WaveStage":
        nif_assets = [
            asset
            for asset in shim.graph.all_assets
            if asset.asset_type == "nif"
            and asset.source_path.lower().endswith(".nif")
            and asset.resolved_path
        ]
        shim._summary.nifs_total = len(nif_assets)
        entries = starfield_mesh_entries(nif_assets)
        self.runner.emit_log(
            "INFO",
            f"[NIF] fo4:starfield queued {len(entries)} NIF(s) for .mesh conversion "
            "(wave A2)",
        )
        return WaveStage(
            phase="starfield_meshes",
            run_id=self.runs.nifs.id,
            mod_path=str(shim.mod_path),
            source_extracted_dir=str(getattr(shim, "source_data_dir", "") or ""),
            params={"nifs": entries},
        )

    def _build_starfield_material_and_texture_stages(self, shim) -> list["WaveStage"]:
        """`starfield_materials` then `starfield_textures`.

        Both are driven from the FO4 BGSM set: the `.mat` writer derives its
        texture *paths* from each BGSM's Diffuse/Normal/SmoothSpec/Glow fields,
        so the texture writer must be handed the same set or the `.mat` files
        point at files nothing wrote. Textures run second only for log
        ordering — they share the run but touch disjoint outputs.
        """
        stages: list[WaveStage] = []
        source_extracted = str(getattr(shim, "source_data_dir", "") or "")
        material_assets = [
            asset
            for asset in shim.graph.all_assets
            if asset.asset_type == "material"
            and asset.source_path.lower().endswith(".bgsm")
            and asset.resolved_path
        ]
        bgem_skipped = sum(
            1
            for asset in shim.graph.all_assets
            if asset.asset_type == "material"
            and asset.source_path.lower().endswith(".bgem")
        )
        if bgem_skipped:
            self.runner.emit_log(
                "INFO",
                f"[Materials] fo4:starfield skipped {bgem_skipped} .bgem effect "
                "material(s): the Starfield .mat writer maps BGSM only (R5)",
            )
        if not (self.toggles.materials and self.runs.textures):
            return stages

        stages.append(
            WaveStage(
                phase="starfield_materials",
                run_id=self.runs.textures.id,
                mod_path=str(shim.mod_path),
                source_extracted_dir=source_extracted,
                params={"materials": starfield_material_entries(material_assets)},
            )
        )

        if self.toggles.textures:
            resolved_textures = {
                normalize_asset_source_path(
                    asset.source_path
                ).casefold(): asset.resolved_path
                for asset in shim.graph.all_assets
                if asset.asset_type == "texture" and asset.resolved_path
            }
            texture_sets, warnings = starfield_texture_sets(
                material_assets, resolved_textures
            )
            for warning in warnings:
                self.runner.emit_log("WARN", f"[Textures] {warning}")
            self.runner.emit_log(
                "INFO",
                f"[Textures] fo4:starfield built {len(texture_sets)} PBR texture "
                f"set(s) from {len(material_assets)} BGSM(s) (wave A3)",
            )
            stages.append(
                WaveStage(
                    phase="starfield_textures",
                    run_id=self.runs.textures.id,
                    mod_path=str(shim.mod_path),
                    source_extracted_dir=source_extracted,
                    params={"texture_sets": texture_sets},
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
        mvp_asset_closure = _mvp_asset_closure_set(self.driver, shim.graph.all_assets)
        from bacup_lib.creature_mvp_assets import creature_mvp_asset_closure

        creature_asset_closure = creature_mvp_asset_closure(
            _exact_mvp_creature_profile(self.driver.ctx),
            shim.graph.all_assets,
        )
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
                if _is_selected_mvp_asset(
                    self.driver, asset, mvp_exclusions, mvp_asset_closure
                )
            ]
        if creature_asset_closure is not None:
            nif_assets = [
                asset
                for asset in nif_assets
                if not creature_asset_closure.allows(asset)
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
            if not (
                mvp_asset_closure is not None and mvp_asset_closure.allows(asset)
            ) and shim._target_has_asset(asset):
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
            skin_option_keys = (
                "translation_maps_dir",
                "target_skeleton",
                "auto_skin_reference_body",
                "emit_first_person",
                "first_person_reference",
                "morph_weight_cap",
            )
            params = {
                "nif_paths": params["nif_paths"],
                "material_out_rel": (
                    f"materials/{Path(str(shim.mod_path)).name}/gamebryo"
                ),
                **{key: params[key] for key in skin_option_keys if key in params},
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
        if self._wave_plan.starfield_target:
            return self._build_starfield_material_and_texture_stages(shim)
        mvp_exclusions = _mvp_exclude_signatures(self.driver)
        mvp_asset_closure = _mvp_asset_closure_set(self.driver, shim.graph.all_assets)
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
                        or _is_selected_mvp_asset(
                            self.driver,
                            asset,
                            mvp_exclusions,
                            mvp_asset_closure,
                        )
                    )
                ]
                texture_assets = []
                base_game_skipped = 0
                for asset in referenced_textures:
                    if not (
                        mvp_asset_closure is not None
                        and mvp_asset_closure.allows(asset)
                    ) and shim._target_has_asset(asset):
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
                    and (
                        (
                            mvp_asset_closure is not None
                            and mvp_asset_closure.allows(asset)
                        )
                        or not shim._target_has_asset(asset)
                    )
                    and (
                        getattr(shim, "convert_precombined_nifs", True)
                        or not _is_precombined_nif_asset(asset)
                    )
                    and (
                        not mvp_exclusions
                        or _is_selected_mvp_asset(
                            self.driver,
                            asset,
                            mvp_exclusions,
                            mvp_asset_closure,
                        )
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
                if a.asset_type == "nif"
                and _asset_key_pair(a) not in self._a2_nif_keys
                and (
                    not mvp_exclusions
                    or _is_selected_mvp_asset(
                        self.driver, a, mvp_exclusions, mvp_asset_closure
                    )
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
        corpus_policy = getattr(self.driver.ctx, "mvp_creature_corpus_policy", None)
        if corpus_policy is not None:
            if not getattr(self.driver.ctx, "_creature_corpus_executed", False):
                raise RuntimeError(
                    "all_creatures_v1 corpus execution did not complete on the retained "
                    "record run"
                )
            return []
        profile = _exact_mvp_creature_profile(self.driver.ctx)
        if profile is not None:
            return self._build_mvp_creature_a4(profile)
        if not self._wave_plan.wave_a4:
            if not getattr(self.driver.ctx, "_fnv_weapon_animation_profiles", False):
                return []
            shim = self._shim()
            parts_run = self.runs.nifs or self.runs.havok
            if parts_run is None:
                raise RuntimeError("FNV weapon profiles require a weapon asset run")
            from bacup_lib.workflows.asset_phases import fo4_behavior_contracts_root

            return [
                WaveStage(
                    phase="convert_legacy_weapon_parts",
                    run_id=parts_run.id,
                    mod_path=str(shim.mod_path),
                    source_extracted_dir=str(getattr(shim, "source_data_dir", "") or ""),
                    target_extracted_dir=str(fo4_behavior_contracts_root(shim, self.runner) or "") or None,
                    params={},
                )
            ]
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
            if self.toggles.havok:
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
            from bacup_lib.workflows.asset_phases import _target_havok_contract_root

            if (
                self.toggles.animations
                and src_engine == "gamebryo"
                and tgt_engine == "creation1"
            ):
                after = ("convert_animations",)
            else:
                after = ("convert_havok",)
            stages.append(
                WaveStage(
                    phase="postprocess_havok_assets",
                    run_id=self.runs.havok.id,
                    mod_path=str(shim.mod_path),
                    source_extracted_dir=source_extracted,
                    target_extracted_dir=str(_target_havok_contract_root(shim, self.runner) or "") or None,
                    params={"source_archive_dirs": [str(path) for path in getattr(shim, "source_archive_dirs", ())]},
                    after=after,
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
                    after=("postprocess_havok_assets",),
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

    def _build_mvp_creature_a4(self, profile: str) -> list["WaveStage"]:
        shim = self._shim()
        run = self.runs.havok or self.runs.nifs
        if run is None or not self.toggles.havok:
            raise RuntimeError(
                f"{profile} MVP requires the Havok phase and an asset run"
            )
        if profile == "skyrim_wolf":
            from bacup_lib.creature_mvp_assets import creature_mvp_asset_closure

            closure = creature_mvp_asset_closure(profile, shim.graph.all_assets)
            if closure is None:
                raise RuntimeError("Skyrim Wolf MVP asset closure did not activate")
            assets = [
                asset
                for asset in shim.graph.all_assets
                if asset.asset_type == "behavior" and closure.allows(asset)
            ]
            unresolved = [
                asset.source_path for asset in assets if not asset.resolved_path
            ]
            if len(assets) != 6 or unresolved:
                raise RuntimeError(
                    "Skyrim Wolf MVP Havok closure is incomplete: "
                    f"found={len(assets)} unresolved={unresolved}"
                )
            return [
                WaveStage(
                    phase="convert_mvp_skyrim_wolf_havok",
                    run_id=run.id,
                    mod_path=str(shim.mod_path),
                    source_extracted_dir="",
                    params={
                        "assets": [
                            {
                                "source_path": asset.source_path,
                                "resolved_path": asset.resolved_path,
                            }
                            for asset in assets
                        ]
                    },
                )
            ]
        if profile != "fnv_gecko":
            raise RuntimeError(f"unknown MVP creature profile: {profile}")
        if not self.toggles.animations:
            raise RuntimeError("FNV Gecko MVP requires animation conversion")
        from bacup_lib.workflows.asset_phases import (
            _params_for_convert_animations,
            _params_for_convert_skeleton,
        )

        skeleton_params = _params_for_convert_skeleton(shim)
        animation_params = _params_for_convert_animations(shim)
        if skeleton_params is None or not skeleton_params.get("resolved_path"):
            raise RuntimeError("FNV Gecko MVP skeleton closure is incomplete")
        animations = animation_params.get("animations", [])
        if len(animations) != 5 or any(
            not animation.get("resolved_path") for animation in animations
        ):
            raise RuntimeError(
                f"FNV Gecko MVP animation closure is incomplete: found={len(animations)}"
            )
        return [
            WaveStage(
                phase="convert_skeleton",
                run_id=run.id,
                mod_path=str(shim.mod_path),
                source_extracted_dir="",
                params=skeleton_params,
            ),
            WaveStage(
                phase="convert_animations",
                run_id=run.id,
                mod_path=str(shim.mod_path),
                source_extracted_dir="",
                params=animation_params,
                after=("convert_skeleton",),
            ),
        ]


# Phase -> ConversionSummary merge (the wrappers' report mappings).
_ASSET_PLANNING_SUMMARY_FIELDS = (
    "nifs_total",
    "nifs_base_game_skipped",
    "nifs_failed",
    "btos_total",
    "btos_base_game_skipped",
    "btos_failed",
    "textures_total",
    "textures_base_game_skipped",
    "textures_failed",
    "materials_total",
    "materials_base_game_skipped",
    "materials_failed",
    "havok_total",
    "havok_base_game_skipped",
    "havok_failed",
    "animations_total",
    "animations_base_game_skipped",
    "animations_failed",
    "audio_total",
    "audio_base_game_skipped",
    "audio_failed",
)


def _merge_asset_planning_summary(
    aggregate: ConversionSummary, planning: ConversionSummary
) -> None:
    for field in _ASSET_PLANNING_SUMMARY_FIELDS:
        value = int(getattr(planning, field, 0) or 0)
        if value:
            setattr(aggregate, field, int(getattr(aggregate, field, 0)) + value)


def _run_fnv_weapon_animation_phase(request, ctx, native_run, summary, mvp_creature_profile=None):
    opts = request.options
    if (
        request.source_game != "fnv"
        or request.target_game != "fo4"
        or not opts.convert_animations
        or not opts.translate_records
        or mvp_creature_profile is not None
        or getattr(opts, "mvp_melee_only", False)
    ):
        return
    if not opts.convert_nifs:
        raise ValueError(
            "FNV weapon animation conversion requires NIF conversion; "
            "enable convert_nifs or disable convert_animations"
        )
    from bacup_lib.workflows.asset_phases import fo4_behavior_contracts_root

    report = native_run.run_phase(
        "convert_legacy_weapon_animations",
        mod_path=str(ctx.mod_path),
        source_extracted_dir=str(ctx.source_data_dir or ""),
        target_extracted_dir=str(fo4_behavior_contracts_root(ctx) or "") or None,
        params={"mod_prefix": getattr(ctx, "mod_prefix", None) or "B21_FNV"},
    )
    ctx._fnv_weapon_animation_profiles = bool(
        report.get("records_added", 0) or report.get("records_changed", 0)
    )
    _merge_wave_report_into_summary(summary, "convert_legacy_weapon_animations", report)


def _run_skyrim_weapon_animation_phase(request, ctx, native_run, summary, mvp_creature_profile=None):
    opts = request.options
    if (
        request.source_game != "skyrimse"
        or request.target_game != "fo4"
        or not opts.convert_animations
        or not opts.translate_records
        or mvp_creature_profile is not None
        or getattr(opts, "mvp_melee_only", False)
    ):
        return
    if not opts.convert_nifs:
        raise ValueError("Skyrim weapon animation conversion requires NIF conversion")
    from bacup_lib.workflows.asset_phases import fo4_behavior_contracts_root

    report = native_run.run_phase(
        "convert_skyrim_weapon_animations",
        mod_path=str(ctx.mod_path),
        source_extracted_dir=str(ctx.source_data_dir or ""),
        target_extracted_dir=str(fo4_behavior_contracts_root(ctx) or "") or None,
        params={"mod_prefix": getattr(ctx, "mod_prefix", None) or "B21_Skyrim"},
    )
    ctx._skyrim_weapon_animation_profiles = bool(
        report.get("records_added", 0) or report.get("records_changed", 0)
    )
    _merge_wave_report_into_summary(summary, "convert_skyrim_weapon_animations", report)


def _run_required_anim_text_data(request, ctx, runner):
    weapon_profiles = (
        getattr(ctx, "_fnv_weapon_animation_profiles", False)
        or getattr(ctx, "_skyrim_weapon_animation_profiles", False)
    )
    creature_policy = getattr(ctx, "mvp_creature_corpus_policy", None)
    if (
        isinstance(creature_policy, Mapping)
        and creature_policy.get("policy_id") == "all_creatures_v1"
    ):
        if getattr(ctx, "_creature_corpus_has_jobs", False):
            _run_post_phase(
                "Generate Creature AnimTextData",
                lambda progress: _run_creature_corpus_anim_text_closure(
                    ctx, runner, progress=progress
                ),
                runner,
                request=request,
            )
        if not weapon_profiles:
            return
    requested = getattr(request.options, "generate_anim_text_data", False)
    if not (requested or weapon_profiles or _requires_fo76_behavior_animtext(ctx, request.options)):
        return
    if not requested:
        runner.emit_log(
            "INFO",
            "animtext: converted behavior profiles require native "
            "metadata and clip bindings before packing",
        )

    def generate_anim_text_data(progress):
        if ctx is None:
            raise RuntimeError("AnimTextData generation requires a built conversion context")
        _run_anim_text_data_generation(
            ctx,
            runner,
            force_native=weapon_profiles or getattr(request.options, "anim_text_data_native", False),
            progress=progress,
        )

    _run_post_phase("Generate AnimTextData", generate_anim_text_data, runner, request=request)


def _merge_wave_report_into_summary(
    summary: ConversionSummary, phase: str, report: dict
) -> None:
    written = int(report.get("assets_written", 0) or 0)
    dropped = int(report.get("records_dropped", 0) or 0)
    failed = int(report.get("items_failed", 0) or 0)
    if phase == "copy_sounds":
        summary.audio_copied += written
        summary.audio_base_game_skipped += dropped
        summary.audio_failed += failed
    elif phase in {"convert_havok", "convert_mvp_skyrim_wolf_havok"}:
        summary.havok_total += written + failed + dropped
        summary.havok_converted += written
        summary.havok_base_game_skipped += dropped
        summary.havok_failed += failed
    elif phase in {
        "convert_nifs_v2",
        "convert_gamebryo_nifs",
        "convert_mvp_creature_nifs_v2",
        "convert_mvp_creature_gamebryo_nifs",
    }:
        summary.nifs_converted += written
        summary.nifs_failed += failed
    elif phase == "convert_btos_v2":
        summary.btos_converted += written
        summary.btos_failed += failed
    elif phase in {"convert_textures_v2", "copy_textures"}:
        summary.textures_total += written + failed + dropped
        summary.textures_converted += written
        summary.textures_failed += failed
        summary.textures_base_game_skipped += dropped
    elif phase == "convert_materials_v2":
        summary.materials_converted += written
        summary.materials_failed += failed
    elif phase in {"convert_animations", "convert_legacy_weapon_animations", "convert_legacy_weapon_parts", "convert_skyrim_weapon_animations"}:
        summary.animations_converted += written
        summary.animations_failed += failed
    elif phase == "postprocess_havok_assets":
        summary.havok_failed += failed


class MultiRunDrainer:
    """Polls conversion_run_drain_events for every live asset run id every
    0.25 s, forwarding events to the runner (Drainer's dispatch shape) and
    capturing per-phase Completed reports for the summary merge + the live
    stage set for the run_state mirror."""

    def __init__(
        self,
        run_ids: list[int],
        runner: "ConversionRunner",
        hz: float = 4.0,
        *,
        timing_report: TimingReport | None = None,
    ) -> None:
        self._run_ids = list(run_ids)
        self._runner = runner
        self._timing_report = timing_report
        self._period = 1.0 / hz
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._drain_lock = threading.Lock()
        self._lock = threading.Lock()
        self.completed_reports: list[tuple[str, dict]] = []
        self._reported_phases: set[str] = set()
        self.failure_log_lines: list[str] = []
        self.active_stages: set[str] = set()
        self.stage_counters: dict[str, int] = {}
        self.stage_totals: dict[str, int] = {}
        self.stage_items: dict[str, str] = {}
        self.stage_errors: dict[str, str] = {}
        self.completed_stages: set[str] = set()
        self._stage_started_at: dict[str, float] = {}

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

    def begin_wave(self, phases: Iterable[str]) -> None:
        phases = set(phases)
        with self._lock:
            self._reported_phases.difference_update(phases)
            self.completed_stages.difference_update(phases)

    def drain_failure_logs(self) -> list[str]:
        with self._lock:
            out = list(self.failure_log_lines)
            self.failure_log_lines.clear()
        return out

    def reconcile_pipeline_report(self, report: dict) -> None:
        for phase, phase_report in report.get("phase_reports", {}).items():
            self._capture_phase_report(str(phase), dict(phase_report))
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

    def _capture_phase_report(self, phase: str, report: dict) -> None:
        with self._lock:
            if phase in self._reported_phases:
                return
            self._reported_phases.add(phase)
            self.completed_reports.append((phase, report))

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
            started = self._stage_started_at.pop(stage, None)
        elapsed_seconds = self._elapsed_seconds_from_ms(elapsed_ms)
        if elapsed_seconds is None:
            elapsed_seconds = _elapsed_seconds(started) if started is not None else 0.0
        if self._timing_report is not None:
            self._timing_report.record(
                stage,
                elapsed_seconds,
                track="assets",
                status="completed",
                items_done=items_done,
                items_failed=items_failed,
            )
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

    def _drain_once(self, native=None, *, exhaust: bool = False) -> None:
        if native is None:
            native = load_native_module()
        with self._drain_lock:
            for run_id in self._run_ids:
                while True:
                    try:
                        events = native.conversion_run_drain_events(run_id, 256)
                    except Exception:
                        break
                    for ev in events:
                        self._dispatch(ev)
                    if not exhaust or len(events) < 256:
                        break

    def _dispatch(self, ev: dict) -> None:
        kind = ev.get("kind")
        if kind == "log":
            level = str(ev.get("level", "INFO") or "INFO").upper()
            message = str(ev.get("message", "") or "")
            if level in {"WARN", "ERROR"}:
                with self._lock:
                    self.failure_log_lines.append(f"[{level}] {message}")
            self._runner.emit_log(level, message)
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
            self._capture_phase_report(phase, report)
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
                self._stage_started_at[stage] = time.perf_counter()
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
                started = self._stage_started_at.pop(stage, None)
            if self._timing_report is not None and started is not None:
                self._timing_report.record(
                    stage,
                    _elapsed_seconds(started),
                    track="assets",
                    status="error",
                    error=message,
                )
            self._emit_stage_progress(
                stage,
                status="error",
                error=message,
            )
            self._runner.emit_log("ERROR", f"stage failed: {stage}: {message}")


def _merge_asset_failure_logs(
    runtime: _UnifiedRecordRuntime, drainer: MultiRunDrainer
) -> None:
    lines = drainer.drain_failure_logs()
    if not lines:
        return
    runtime._extend_unique(
        runtime._run_result.failed_nifs, runtime._failed_paths(lines, "NIF")
    )
    runtime._extend_unique(
        runtime._run_result.failed_textures,
        runtime._failed_paths(lines, "Texture"),
    )
    runtime._extend_unique(
        runtime._run_result.failed_bgsms, runtime._failed_material_paths(lines)
    )


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
            "updated_at": datetime.now(timezone.utc).isoformat(timespec="milliseconds"),
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
            getattr(driver.ctx, "mvp_creature_corpus_policy", None) is not None,
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
    planning_summary = ConversionSummary(mod_path=str(mod_root))

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
                planning_summary=planning_summary,
            )
            drainer = MultiRunDrainer(
                [r.id for r in runs.all_runs()],
                runner,
                timing_report=_timing_report(driver.ctx),
            )
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
        active_drainer.begin_wave([stage.phase for stage in stages])
        plan = {
            "events_run_id": stages[0].run_id,
            "run_state_path": str(run_state_path),
            "stages": _sequential_wave_plan(stages),
        }
        if max_asset_failures is not None:
            plan["max_asset_failures"] = int(max_asset_failures)
        runner.emit_log(
            "INFO",
            f"asset wave {name}: {len(stages)} stage(s): "
            + ", ".join(s.phase for s in stages),
        )
        if runner.is_cancelled():
            raise RuntimeError(f"conversion cancelled before asset wave {name}")
        started = time.perf_counter()
        wave_done = threading.Event()

        def forward_cancellation() -> None:
            while not wave_done.wait(0.1):
                if runner.is_cancelled():
                    runs.cancel_all()
                    return

        cancel_watcher = threading.Thread(target=forward_cancellation, daemon=True, name=f"asset-wave-{name}-cancel")
        cancel_watcher.start()
        try:
            pipeline_report = native.conversion_pipeline_run(json.dumps(plan))
        except Exception as exc:
            if runner.is_cancelled():
                raise RuntimeError(f"conversion cancelled during asset wave {name}") from exc
            active_drainer._drain_once(native, exhaust=True)
            context = active_drainer.describe_active()
            error = (
                RuntimeError(f"asset wave {name} failed while {context}: {exc}")
                if context
                else exc
            )
            if not _continue_on_error(driver._req):
                if error is exc:
                    raise
                raise error from exc
            _record_phase_failure(driver._req, f"Asset wave {name}", error)
            runner.emit_log("ERROR", f"asset wave {name} failed; continuing: {error}")
            _merge_asset_failure_logs(driver.record_runtime, active_drainer)
            return
        finally:
            wave_done.set()
        active_drainer._drain_once(native, exhaust=True)
        if runner.is_cancelled():
            raise RuntimeError(f"conversion cancelled during asset wave {name}")
        active_drainer.reconcile_pipeline_report(pipeline_report)
        runner.emit_log(
            "INFO",
            f"asset wave {name} completed in {time.perf_counter() - started:.1f}s",
        )
        for phase, report in active_drainer.drain_completed():
            _merge_wave_report_into_summary(
                driver.record_runtime._aggregate_summary, phase, report
            )
        _merge_asset_failure_logs(driver.record_runtime, active_drainer)

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
    except Exception as exc:
        if _continue_on_error(driver._req):
            _record_phase_failure(driver._req, "Asset track", exc)
            runner.emit_log("ERROR", f"asset track failed; continuing with records: {exc}")
            signals.asset_a2_done.set()
            return runs
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
                _merge_wave_report_into_summary(
                    driver.record_runtime._aggregate_summary, phase, report
                )
            _merge_asset_failure_logs(driver.record_runtime, drainer)
        aggregate = driver.record_runtime._aggregate_summary
        _merge_asset_planning_summary(aggregate, planning_summary)


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
                summary["nifs"],
                summary["blobs"],
                summary["errors"],
                summary["warnings"],
                summary["workers"],
                report_dir,
            ),
        )
    except Exception as exc:  # never fail the regen on the gate
        runner.emit_log("WARN", "collision validation skipped: %s" % exc)


def _continue_on_error(request) -> bool:
    return bool(getattr(getattr(request, "options", None), "continue_on_error", False))


def _record_phase_failure(request, label: str, exc: BaseException) -> None:
    failures = getattr(request, "phase_failures", None)
    if failures is not None:
        failures.append(f"{label}: {exc}")


def _run_post_phase(
    label: str,
    body,
    runner: "ConversionRunner",
    *,
    request: PluginPortRequest | None = None,
    timing_report: TimingReport | None = None,
) -> None:
    if timing_report is None:
        timing_report = getattr(request, "timing_report", None)
    only = getattr(getattr(request, "options", None), "post_phases_only", None)
    if only is not None and label not in only:
        return
    if label in getattr(getattr(request, "options", None), "post_phases_skip", ()):
        return
    if getattr(runner, "is_cancelled", lambda: False)():
        raise RuntimeError(f"conversion cancelled before {label}")
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
        _record_phase_failure(request, label, exc)
        continuing = _continue_on_error(request)
        runner.emit_log(
            "ERROR",
            f"{label} failed{'; continuing' if continuing else ''}: {exc}",
        )
        _log.exception("Post phase %s failed", label)
        runner.emit_phase_complete(progress)
        if not continuing:
            raise
    else:
        progress.status = "completed"
        progress.elapsed_seconds = _elapsed_seconds(started)
        runner.emit_phase_complete(progress)
    finally:
        if timing_report is not None:
            timing_report.record(
                label,
                _elapsed_seconds(started),
                track="post",
                status=progress.status,
                error=progress.error,
            )


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


def _read_creature_anim_text_ledger(path: Path, label: str) -> Mapping[str, object]:
    if not path.is_file():
        raise RuntimeError(f"all_creatures_v1 is missing its {label}: {path}")
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise RuntimeError(
            f"all_creatures_v1 could not read its {label}: {path}: {exc}"
        ) from exc
    if not isinstance(payload, Mapping):
        raise RuntimeError(f"all_creatures_v1 {label} must be a JSON object")
    return payload


def _creature_anim_text_contract_inputs(
    ctx: ConversionContext,
) -> tuple[Path, Path, Path, tuple[str, ...]]:
    policy = getattr(ctx, "mvp_creature_corpus_policy", None)
    if not isinstance(policy, Mapping) or policy.get("policy_id") != "all_creatures_v1":
        raise RuntimeError("creature AnimText closure requires all_creatures_v1")
    if str(policy.get("execution_mode") or "") != "strict":
        raise RuntimeError(
            "all_creatures_v1 AnimText closure requires strict execution"
        )

    debug_dir_value = str(policy.get("debug_dir") or "")
    debug_dir = PurePosixPath(debug_dir_value)
    if (
        not debug_dir_value
        or debug_dir.is_absolute()
        or "\\" in debug_dir_value
        or any(part in {"", ".", ".."} for part in debug_dir.parts)
    ):
        raise RuntimeError("all_creatures_v1 has an invalid debug_dir")
    debug_root = Path(ctx.mod_path).joinpath(*debug_dir.parts)
    plan_path = debug_root / "plan.json"
    execution_path = debug_root / "execution_ledger.json"
    record_commit_path = debug_root / "record_commit_ledger.json"

    plan = _read_creature_anim_text_ledger(plan_path, "corpus plan")
    execution = _read_creature_anim_text_ledger(
        execution_path, "corpus execution ledger"
    )
    record_commit = _read_creature_anim_text_ledger(
        record_commit_path, "record-commit ledger"
    )
    if "receipt" in record_commit:
        if record_commit.get("version") != 2:
            raise RuntimeError(
                "all_creatures_v1 record-commit ledger has an unsupported version"
            )
        receipt = record_commit.get("receipt")
        if not isinstance(receipt, Mapping):
            raise RuntimeError(
                "all_creatures_v1 record-commit ledger receipt must be an object"
            )
        record_commit = receipt
    for label, payload in (
        ("corpus plan", plan),
        ("corpus execution ledger", execution),
    ):
        if payload.get("version") != 1:
            raise RuntimeError(f"all_creatures_v1 {label} has an unsupported version")

    if (
        execution.get("mode") != "strict"
        or execution.get("strict_aborted") is not False
    ):
        raise RuntimeError("all_creatures_v1 corpus execution did not commit strictly")
    raw_execution_families = execution.get("families")
    if not isinstance(raw_execution_families, list) or not raw_execution_families:
        raise RuntimeError("all_creatures_v1 execution ledger has no families")
    family_ids: list[str] = []
    for family in raw_execution_families:
        if not isinstance(family, Mapping):
            raise RuntimeError("all_creatures_v1 execution family must be an object")
        family_id = str(family.get("family_id") or "").strip()
        if not family_id or family.get("disposition") != "published":
            raise RuntimeError(
                "all_creatures_v1 AnimText requires every family to be published"
            )
        records_deferred = family.get("records_deferred")
        if (
            isinstance(records_deferred, bool)
            or not isinstance(records_deferred, int)
            or records_deferred != 0
        ):
            raise RuntimeError(
                f"all_creatures_v1 family {family_id} did not commit its record closure"
            )
        family_ids.append(family_id)
    expected = tuple(family_ids)
    expected_folded = {family.casefold() for family in expected}
    if len(expected_folded) != len(expected):
        raise RuntimeError("all_creatures_v1 execution ledger has duplicate families")

    raw_jobs = plan.get("jobs")
    if not isinstance(raw_jobs, list) or not raw_jobs:
        raise RuntimeError("all_creatures_v1 corpus plan has no jobs")
    planned_family_graphs: dict[str, int] = {family: 0 for family in expected_folded}
    for job in raw_jobs:
        if not isinstance(job, Mapping):
            raise RuntimeError("all_creatures_v1 corpus job must be an object")
        family_id = str(job.get("family_id") or "").strip().casefold()
        if family_id not in planned_family_graphs:
            raise RuntimeError(
                "all_creatures_v1 corpus plan and execution families do not match"
            )
        graph_paths = job.get("graph_paths")
        if graph_paths is None:
            artifacts = job.get("artifacts")
            if not isinstance(artifacts, list):
                raise RuntimeError(
                    "all_creatures_v1 corpus job artifacts must be a list"
                )
            graph_paths = [
                artifact.get("target_path")
                for artifact in artifacts
                if isinstance(artifact, Mapping)
                and artifact.get("kind")
                in {"character_hkx", "root_behavior_hkx", "core_behavior_hkx"}
            ]
        if not isinstance(graph_paths, list):
            raise RuntimeError("all_creatures_v1 corpus graph_paths must be a list")
        for graph_path in graph_paths:
            target_path = str(graph_path or "").strip()
            if not target_path.lower().endswith(".hkx"):
                raise RuntimeError(
                    f"all_creatures_v1 family {family_id} has an invalid graph artifact"
                )
            planned_family_graphs[family_id] += 1
    if any(count < 1 for count in planned_family_graphs.values()):
        raise RuntimeError(
            "all_creatures_v1 plan does not provide a graph for every published family"
        )

    raw_record_families = record_commit.get("families")
    if not isinstance(raw_record_families, list) or not raw_record_families:
        raise RuntimeError("all_creatures_v1 record-commit ledger has no families")
    committed: set[str] = set()
    committed_race_keys: set[str] = set()
    record_total = 0
    mapping_total = 0
    for family in raw_record_families:
        if not isinstance(family, Mapping):
            raise RuntimeError("all_creatures_v1 record family must be an object")
        family_id = str(family.get("family_id") or "").strip()
        family_key = family_id.casefold()
        if (
            not family_id
            or family_key not in expected_folded
            or family_key in committed
        ):
            raise RuntimeError(
                "all_creatures_v1 record and execution families do not match"
            )
        committed.add(family_key)
        record_count = family.get("record_count")
        mapping_count = family.get("mapping_count")
        if (
            isinstance(record_count, bool)
            or not isinstance(record_count, int)
            or record_count < 1
            or isinstance(mapping_count, bool)
            or not isinstance(mapping_count, int)
            or mapping_count < 1
        ):
            raise RuntimeError(
                f"all_creatures_v1 family {family_id} has no committed record mapping"
            )
        record_total += record_count
        mapping_total += mapping_count
        primary_mappings = family.get("primary_mappings")
        if (
            not isinstance(primary_mappings, list)
            or len(primary_mappings) != mapping_count
        ):
            raise RuntimeError(
                f"all_creatures_v1 family {family_id} mapping receipt is incomplete"
            )
        races = family.get("races")
        if not isinstance(races, list) or not races:
            raise RuntimeError(
                f"all_creatures_v1 family {family_id} has no committed RACE"
            )
        for race in races:
            race_form_key = (
                str(race.get("form_key") or "").strip()
                if isinstance(race, Mapping)
                else ""
            )
            if not race_form_key or race_form_key.casefold() in committed_race_keys:
                raise RuntimeError(
                    f"all_creatures_v1 family {family_id} has an invalid RACE receipt"
                )
            committed_race_keys.add(race_form_key.casefold())
            attack_events = race.get("attack_events")
            attack_data_entries = race.get("attack_data_entries")
            if (
                not isinstance(attack_events, list)
                or any(not str(event).strip() for event in attack_events)
                or isinstance(attack_data_entries, bool)
                or not isinstance(attack_data_entries, int)
                or attack_data_entries != len(attack_events)
            ):
                raise RuntimeError(
                    f"all_creatures_v1 family {family_id} RACE ATKD/ATKE closure is incomplete"
                )
    if committed != expected_folded:
        raise RuntimeError(
            "all_creatures_v1 record receipt does not account for every published family"
        )
    raw_family_ids = record_commit.get("family_ids")
    if (
        not isinstance(raw_family_ids, list)
        or {str(family).strip().casefold() for family in raw_family_ids}
        != expected_folded
    ):
        raise RuntimeError("all_creatures_v1 record family IDs do not reconcile")
    if record_commit.get("record_count") != record_total:
        raise RuntimeError("all_creatures_v1 record receipt count does not reconcile")
    if record_commit.get("mapping_count") != mapping_total:
        raise RuntimeError("all_creatures_v1 mapping receipt count does not reconcile")
    reserved_form_keys = record_commit.get("reserved_form_keys")
    normalized_reserved = (
        {str(key).strip().casefold() for key in reserved_form_keys}
        if isinstance(reserved_form_keys, list)
        else set()
    )
    if (
        not isinstance(reserved_form_keys, list)
        or len(reserved_form_keys) != record_total
        or len(normalized_reserved) != record_total
        or not committed_race_keys.issubset(normalized_reserved)
    ):
        raise RuntimeError("all_creatures_v1 reserved FormKey receipt is incomplete")

    return plan_path, execution_path, record_commit_path, expected


def _run_creature_corpus_anim_text_closure(
    ctx: ConversionContext,
    runner: "ConversionRunner",
    *,
    progress: PhaseProgress | None = None,
) -> object:
    if ctx.target_game != "fo4":
        raise RuntimeError("all_creatures_v1 AnimText closure requires an FO4 target")
    plan_path, execution_path, record_commit_path, family_ids = (
        _creature_anim_text_contract_inputs(ctx)
    )
    plugin_path = Path(ctx.mod_path) / str(ctx.output_plugin_name or "")
    if not ctx.output_plugin_name or not plugin_path.is_file():
        raise RuntimeError(
            "all_creatures_v1 AnimText closure requires the committed target plugin"
        )
    removed = _remove_existing_anim_text_data(Path(ctx.mod_path))
    if removed:
        runner.emit_log(
            "INFO", f"animtext: removed {removed} stale AnimTextData tree(s)"
        )
    receipt = _run_anim_text_data_native(
        ctx,
        runner,
        plugin_path,
        progress=progress,
        creature_family_ids=family_ids,
        creature_corpus_plan=plan_path,
        creature_execution_ledger=execution_path,
        creature_record_commit_ledger=record_commit_path,
    )
    if receipt is None:
        raise RuntimeError("all_creatures_v1 AnimText closure returned no receipt")
    ctx.creature_anim_text_receipt = receipt
    receipt_path = execution_path.parent / "anim_text_receipt.json"
    receipt_path.write_text(
        json.dumps(receipt.to_dict(), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return receipt


def _run_anim_text_data_generation(
    ctx: ConversionContext,
    runner: "ConversionRunner",
    force_native: bool = False,
    *,
    progress: PhaseProgress | None = None,
) -> None:
    force_native = force_native or getattr(ctx, "source_game", None) == "fo76"
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
                "animtext: CreationKit.exe present but native generation required; "
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


def _marshal_creature_anim_text_receipt(raw: str) -> object:
    from creation_lib.ck.anim_text_data import (
        CreatureAnimTextClosureReceipt,
        CreatureAnimTextFamilyReceipt,
        CreatureAnimTextTarget,
        CreatureAttackEventReceipt,
    )

    payload = json.loads(raw)
    families = tuple(
        CreatureAnimTextFamilyReceipt(
            family_id=str(family["family_id"]),
            status=str(family["status"]),
            race_form_keys=tuple(str(value) for value in family["race_form_keys"]),
            graph_paths=tuple(str(value) for value in family["graph_paths"]),
            emitted_files=tuple(str(value) for value in family["emitted_files"]),
            attack_events=tuple(
                CreatureAttackEventReceipt(
                    event=str(event["event"]),
                    race_form_key=str(event["race_form_key"]),
                    atkd_index=int(event["atkd_index"]),
                    source=str(event["source"]),
                    targets=tuple(
                        CreatureAnimTextTarget(
                            graph_path=str(target["graph_path"]),
                            clip_name=str(target["clip_name"]),
                            annotation=str(target["annotation"]),
                        )
                        for target in event["targets"]
                    ),
                )
                for event in family["attack_events"]
            ),
        )
        for family in payload["families"]
    )
    return CreatureAnimTextClosureReceipt(
        version=int(payload["version"]),
        policy_id=str(payload["policy_id"]),
        written=int(payload["written"]),
        families=families,
    )


def _run_anim_text_data_native(
    ctx: ConversionContext,
    runner: "ConversionRunner",
    plugin_path: Path,
    *,
    progress: PhaseProgress | None = None,
    creature_family_ids: tuple[str, ...] = (),
    creature_corpus_plan: Path | None = None,
    creature_execution_ledger: Path | None = None,
    creature_record_commit_ledger: Path | None = None,
) -> object | None:
    """CK-free AnimTextData generation: read the built plugin's RACE subgraphs and
    write the bucket files natively. Currently emits AnimationFileData; the other
    buckets fall back to CK when available."""
    from bacup_lib.native_runtime import load_native_module

    mod_dir = plugin_path.parent
    meshes_root = mod_dir / "data" / "Meshes"
    if not meshes_root.is_dir():
        alt = mod_dir / "data" / "meshes"
        if alt.is_dir():
            meshes_root = alt
    if not meshes_root.is_dir():
        if creature_family_ids:
            raise RuntimeError(
                f"all_creatures_v1 has no emitted loose Meshes at {meshes_root}"
            )
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
    runner.emit_log(
        "INFO",
        f"animtext: CK-free generation from {plugin_path.name} RACE subgraphs",
    )

    def emit_progress(message: str) -> None:
        runner.emit_log("INFO", f"animtext: {message}")
        if progress is not None:
            progress.current_item = message
            runner.emit_item_progress(progress)

    progress_messages: Queue[str] = Queue()
    native_started = time.monotonic()
    last_update = native_started
    if creature_family_ids:
        if (
            creature_corpus_plan is None
            or creature_execution_ledger is None
            or creature_record_commit_ledger is None
        ):
            raise RuntimeError(
                "creature AnimText closure requires corpus plan, execution, and "
                "record-commit ledgers"
            )
        creature_contract_json = json.dumps(
            {
                "version": 1,
                "policy_id": "all_creatures_v1",
                "game": ctx.target_game,
                "plugin_path": str(plugin_path),
                "source_meshes_root": str(meshes_root),
                "output_meshes_root": str(meshes_root),
                "base_meshes_root": (
                    str(base_meshes_root) if base_meshes_root is not None else None
                ),
                "base_plugin_paths": [str(path) for path in base_race_paths],
                "corpus_plan_path": str(creature_corpus_plan),
                "execution_ledger_path": str(creature_execution_ledger),
                "record_commit_ledger_path": str(creature_record_commit_ledger),
                "expected_family_ids": list(creature_family_ids),
                "event_source": "emitted_race_atkd_atke",
                "resolution_source": "emitted_family_graphs",
                "mod_prefix": getattr(ctx, "mod_prefix", None),
            },
            sort_keys=True,
        )
    else:
        creature_contract_json = None

    native = load_native_module()

    def generate() -> tuple[int, str | None]:
        asset_paths = (
            {
                "target_data_dir": str(ctx.target_data_dir),
                "target_catalog_path": str(ctx.target_asset_catalog_path),
                "target_cache_dir": str(ctx.target_asset_cache_dir),
                "target_overlay_dir": (
                    str(ctx.target_extracted_dir) if ctx.target_extracted_dir else None
                ),
            }
            if target_asset_store is not None
            else {
                "target_data_dir": None,
                "target_catalog_path": None,
                "target_cache_dir": None,
                "target_overlay_dir": None,
            }
        )
        return native.conversion_generate_anim_text_data(
            str(plugin_path),
            ctx.target_game,
            [str(path) for path in base_race_paths],
            str(meshes_root),
            str(meshes_root),
            base_meshes_root=(
                str(base_meshes_root) if base_meshes_root is not None else None
            ),
            mod_prefix=getattr(ctx, "mod_prefix", None),
            creature_contract_json=creature_contract_json,
            progress_callback=progress_messages.put,
            workers=getattr(ctx, "conversion_workers", None),
            **asset_paths,
        )

    with ThreadPoolExecutor(max_workers=1, thread_name_prefix="animtext") as executor:
        future = executor.submit(generate)
        while not future.done():
            now = time.monotonic()
            heartbeat_due_in = max(
                0.0,
                _ANIM_TEXT_PROGRESS_HEARTBEAT_SECONDS - (now - last_update),
            )
            try:
                message = progress_messages.get(timeout=min(0.1, heartbeat_due_in))
            except Empty:
                now = time.monotonic()
                if (
                    not future.done()
                    and now - last_update >= _ANIM_TEXT_PROGRESS_HEARTBEAT_SECONDS
                ):
                    emit_progress(
                        "still working: native AnimTextData generation is active "
                        f"({now - native_started:.0f}s elapsed)"
                    )
                    last_update = now
            else:
                emit_progress(message)
                last_update = time.monotonic()

        while True:
            try:
                message = progress_messages.get_nowait()
            except Empty:
                break
            emit_progress(message)
        count, creature_receipt_json = future.result()
    runner.emit_log("INFO", f"animtext: wrote {count} AnimTextData bucket file(s)")
    if not creature_family_ids:
        return None
    if creature_receipt_json is None:
        raise RuntimeError("all_creatures_v1 AnimText closure returned no receipt")
    return _marshal_creature_anim_text_receipt(creature_receipt_json)


def _regenerate_modt_after_asset_waves(
    driver: "UnifiedDriver",
    runner: "ConversionRunner",
    mod_root: Path,
    *,
    progress: PhaseProgress | None = None,
    nif_run=None,
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
        open_started = time.perf_counter()
        with ConversionRun.open_existing(
            driver._req.source_game,
            driver._req.target_game,
            None,
            str(output_path),
            config=driver.record_runtime._native_run_config(ctx),
        ) as run:
            _record_timing(ctx, "modt_open_native", open_started)
            if nif_run is not None:
                run.share_output_nif_dependencies(nif_run)
            manifest_started = time.perf_counter()
            manifest_report = run.run_phase(
                "emit_modt_manifest",
                mod_path=str(mod_root),
                params={"manifest_path": str(manifest_path)},
            )
            _record_timing(ctx, "modt_manifest", manifest_started)
            update_progress(1, "Regenerating MODT records")
            apply_started = time.perf_counter()
            modt_report = run.run_phase(
                "regenerate_modt",
                mod_path=str(mod_root),
                params={
                    "manifest_path": str(manifest_path),
                    "is_upgrade": False,
                },
            )
            _record_timing(ctx, "modt_apply", apply_started)
            update_progress(2, "Saving updated plugin")
            save_started = time.perf_counter()
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
        _record_timing(ctx, "modt_save_plugin", save_started)
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


def _build_precombines_after_build(
    driver: "UnifiedDriver",
    runner: "ConversionRunner",
    mod_root: Path,
    *,
    progress: PhaseProgress | None = None,
) -> None:
    """Build precombined meshes (*_OC.NIF + geometry CSG/CDX + CELL PCMB/XCRI).

    Runs before previs, which reads the combined groups for its scenes.
    """
    from bacup_lib.precombine_generation import PrecombineRequest, generate_precombines

    ctx = driver.ctx
    options = driver._req.options
    driver._precombine_index_resources = None
    if (
        ctx is None
        or driver._req.target_game.lower() != "fo4"
        or not options.build_esp
        or not getattr(options, "build_precombines", False)
    ):
        return
    plugin_path = mod_root / ctx.output_plugin_name
    if not plugin_path.is_file():
        raise FileNotFoundError(f"precombine generation requires the built plugin: {plugin_path}")
    previous = getattr(options, "reuse_precombines_from", None)
    driver._reused_precombines = bool(previous) and _reuse_previous_precombines(
        driver, runner, mod_root, plugin_path, Path(previous)
    )
    if driver._reused_precombines:
        return
    target_data = getattr(ctx, "target_data_dir", None) or driver._req.target_data_dir
    target_data = Path(target_data) if target_data else None
    target_extracted = getattr(ctx, "target_extracted_dir", None) or driver._req.target_extracted_dir
    archives = (
        sorted(
            path
            for path in target_data.glob("*.ba2")
            if any(token in path.name.casefold() for token in (" - meshes", " - materials"))
        )
        if target_data
        else []
    )

    def report_progress(done: int, total: int, item: str) -> None:
        if progress is None:
            return
        progress.total_items = total
        progress.completed_items = done
        progress.current_item = item
        runner.emit_item_progress(progress)

    result = generate_precombines(
        PrecombineRequest(
            plugin_path=plugin_path,
            mod_data_dir=mod_root / "data",
            master_dirs=[mod_root, *([target_data] if target_data else [])],
            loose_roots=[mod_root / "data", *([Path(target_extracted)] if target_extracted else [])],
            archives=archives,
            work_dir=Path(getattr(ctx, "diagnostics_root", None) or mod_root) / "precombine",
            workers=getattr(options, "conversion_workers", None),
            interior_cdx_only=bool(getattr(options, "interior_cdx_only", False)),
        ),
        log=runner.emit_log,
        progress=report_progress,
    )
    if result.patched_cells:
        _rebuild_cell_offsets_after_build(driver, runner, mod_root)
    if getattr(options, "generate_previs", False):
        driver._precombine_index_resources = result.index_resources


def _reuse_previous_precombines(
    driver: "UnifiedDriver", runner: "ConversionRunner", mod_root: Path, plugin_path: Path, previous: Path
) -> bool:
    """Copies the previous build's precombine/previs stamps onto the rebuilt ESM
    and keeps its files (precombined NIFs, Vis UVDs, CSG/CDX). Any missing piece
    or mismatch returns False, and the caller regenerates."""
    from creation_lib._native import previs_native
    from bacup_lib.precombine_generation import precombine_sidecar_names

    options = driver._req.options
    data = mod_root / "data"

    def plugin_dir(*parts: str) -> bool:
        level = [data]
        for part in (*parts, plugin_path.name.casefold()):
            level = [child for parent in level if parent.is_dir() for child in parent.iterdir()
                     if child.is_dir() and child.name.casefold() == part]
        return bool(level)

    csg, cdx, _exterior = precombine_sidecar_names(plugin_path.name)
    missing = [
        label
        for label, present in (
            ("precombined meshes", plugin_dir("meshes", "precombined")),
            ("Vis UVDs", plugin_dir("vis") or not getattr(options, "generate_previs", False)),
            (csg, (mod_root / csg).is_file() or (data / csg).is_file()),
            (cdx, (mod_root / cdx).is_file() or (data / cdx).is_file()),
            ("previous ESM", previous.is_file()),
        )
        if not present
    ]
    if missing:
        runner.emit_log("INFO", f"precombine: regenerating; nothing to reuse for {', '.join(missing)}")
        return False
    grafted = plugin_path.with_name(plugin_path.name + ".grafted")
    try:
        cells = previs_native.graft_cell_stamps(str(previous), str(plugin_path), str(grafted))
    except ValueError as exc:
        grafted.unlink(missing_ok=True)
        runner.emit_log("WARN", f"precombine: regenerating; previous stamps do not fit the new ESM ({exc})")
        return False
    os.replace(grafted, plugin_path)
    runner.emit_log("INFO", f"precombine: reused the previous precombines and previs on {cells} CELL(s)")
    _rebuild_cell_offsets_after_build(driver, runner, mod_root)
    return True


def _generate_previs_after_build(
    driver: "UnifiedDriver",
    runner: "ConversionRunner",
    mod_root: Path,
    *,
    progress: PhaseProgress | None = None,
) -> None:
    """Generate Umbra previs (Vis/*.uvd + CELL VISI/RVIS/XPRI) on the built ESM.

    The visibility solve is the clean-room Rust one. Stamping changes CELL
    sizes, so the WRLD offset tables are rebuilt afterwards.
    """
    from bacup_lib.previs_generation import PrevisRequest, generate_previs

    ctx = driver.ctx
    options = driver._req.options
    if getattr(driver, "_reused_precombines", False):
        return
    index_resources = getattr(driver, "_precombine_index_resources", None)
    driver._precombine_index_resources = None
    if (
        ctx is None
        or driver._req.target_game.lower() != "fo4"
        or not options.build_esp
        or not getattr(options, "generate_previs", False)
    ):
        return
    plugin_path = mod_root / ctx.output_plugin_name
    if not plugin_path.is_file():
        raise FileNotFoundError(f"previs generation requires the built plugin: {plugin_path}")
    target_data = getattr(ctx, "target_data_dir", None) or driver._req.target_data_dir
    target_data = Path(target_data) if target_data else None
    target_extracted = getattr(ctx, "target_extracted_dir", None) or driver._req.target_extracted_dir
    archives = (
        sorted(
            path
            for path in target_data.glob("*.ba2")
            if any(token in path.name.casefold() for token in (" - meshes", " - materials"))
        )
        if target_data
        else []
    )

    def report_progress(done: int, total: int, item: str) -> None:
        if progress is None:
            return
        progress.total_items = total
        progress.completed_items = done
        progress.current_item = item
        runner.emit_item_progress(progress)

    result = generate_previs(
        PrevisRequest(
            plugin_path=plugin_path,
            mod_data_dir=mod_root / "data",
            master_dirs=[mod_root, *([target_data] if target_data else [])],
            loose_roots=[mod_root / "data", *([Path(target_extracted)] if target_extracted else [])],
            archives=archives,
            work_dir=Path(getattr(ctx, "diagnostics_root", None) or mod_root) / "previs",
            workers=getattr(options, "conversion_workers", None),
            interior_cdx_only=bool(getattr(options, "interior_cdx_only", False)),
            index_resources=index_resources,
        ),
        log=runner.emit_log,
        progress=report_progress,
    )
    if result.patched_cells:
        _rebuild_cell_offsets_after_build(driver, runner, mod_root)


def _finalize_plugin_records_after_asset_waves(
    driver: "UnifiedDriver",
    runner: "ConversionRunner",
    mod_root: Path,
    source_plugin: Path,
    *,
    progress: PhaseProgress | None = None,
    nif_run=None,
) -> None:
    """Publish MODT, TERM and final cell offsets as one atomic plugin update."""
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
            f"final plugin record update requires the built output plugin: {output_path}"
        )

    from bacup_lib.run import ConversionRun

    repair_term_markers = driver._req.source_game.lower() == "fo76"
    rebuild_cell_offsets = bool(
        getattr(driver._req.options, "rebuild_cell_offsets", True)
    )
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
        open_started = time.perf_counter()
        with ConversionRun.open_existing(
            driver._req.source_game,
            driver._req.target_game,
            None,
            str(output_path),
            config=driver.record_runtime._native_run_config(ctx),
        ) as run:
            _record_timing(ctx, "final_plugin_open_native", open_started)
            if runner.is_cancelled():
                raise RuntimeError("conversion cancelled before final plugin update")
            if nif_run is not None:
                run.share_output_nif_dependencies(nif_run)
            cancel_watch_stop = threading.Event()

            def forward_cancellation() -> None:
                while not cancel_watch_stop.wait(0.1):
                    if runner.is_cancelled():
                        run.cancel()
                        return

            cancel_watcher = threading.Thread(
                target=forward_cancellation,
                daemon=True,
                name="final-plugin-cancel",
            )
            cancel_watcher.start()
            try:
                manifest_started = time.perf_counter()
                manifest_report = run.run_phase(
                    "emit_modt_manifest",
                    mod_path=str(mod_root),
                    params={"manifest_path": str(manifest_path)},
                )
                _record_timing(ctx, "modt_manifest", manifest_started)
                update_progress(1, "Regenerating MODT and finalizing plugin records")
                mutations_started = time.perf_counter()
                final_report = run.run_phase(
                    "finalize_plugin_records",
                    mod_path=str(mod_root),
                    params={
                        "manifest_path": str(manifest_path),
                        "manifest_entries": manifest_report.get("records_changed", 0),
                        "is_upgrade": False,
                        "regenerate_modt": True,
                        "repair_term_markers": repair_term_markers,
                        "rebuild_cell_offsets": rebuild_cell_offsets,
                        "convert_legacy_landscape_specular": bool(
                            getattr(driver._req.options, "convert_textures", False)
                            and driver._req.source_game.lower() in {"fnv", "fo3", "skyrim", "skyrimse"}
                        ),
                        "fail_on_duplicate_form_ids": bool(
                            getattr(
                                driver._req.options, "fail_on_duplicate_form_ids", False
                            )
                        ),
                        "source_plugin_path": (
                            str(source_plugin) if repair_term_markers else None
                        ),
                    },
                )
                _record_timing(ctx, "final_plugin_mutations_native", mutations_started)
            finally:
                cancel_watch_stop.set()
                cancel_watcher.join(timeout=1.0)
            if runner.is_cancelled():
                raise RuntimeError("conversion cancelled before final plugin save")
            for event in run.drain_events(256):
                if event.get("kind") == "log":
                    runner.emit_log(
                        str(event.get("level", "INFO")).upper(),
                        str(event.get("message", "")),
                    )
            update_progress(2, "Saving updated plugin")
            save_started = time.perf_counter()
            temp_fd, temp_name = tempfile.mkstemp(
                dir=output_path.parent,
                prefix=f".{output_path.name}.",
                suffix=".tmp",
            )
            temp_output_path = Path(temp_name)
            os.close(temp_fd)
            run.save_target(str(temp_output_path), run_nvnm_validator=False)
        if runner.is_cancelled():
            raise RuntimeError("conversion cancelled before final plugin publication")
        os.replace(temp_output_path, output_path)
        _cleanup_temp_save_strings(temp_output_path)
        _record_timing(ctx, "final_plugin_save", save_started)
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
        "final plugin records published atomically: "
        f"records_changed={final_report.get('records_changed', 0)} "
        f"warnings={final_report.get('warnings', 0)}",
    )


def run_unified(
    request: PluginPortRequest,
    runner: "ConversionRunner",
    *,
    enable_ba2: bool = True,
    archive_max_bytes: int = DEFAULT_ARCHIVE_MAX_BYTES,
    ba2_compression_level: int | None = None,
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
    """The unified driver: record track + asset waves + sink join.

    `request.options` = the legacy full-run options (asset toggles ON; see
    UnifiedDriver.__init__).
    """
    preflight_started = time.perf_counter()
    if not record_preflight_complete:
        _preflight_legacy_packs(request, runner)
    _record_timing(
        request,
        "unified_initial_preflight",
        preflight_started,
        scope="detail",
        parent="run_unified",
        status="skipped" if record_preflight_complete else "ok",
    )
    toggles = AssetWaveToggles.from_options(request.options)
    mod_root = _resolved_mod_root(request)
    mod_root.mkdir(parents=True, exist_ok=True)
    diagnostics_root = Path(getattr(request, "diagnostics_root", None) or mod_root)
    diagnostics_root.mkdir(parents=True, exist_ok=True)
    sink_setup_started = time.perf_counter()
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
    _record_timing(
        request,
        "unified_sink_creation",
        sink_setup_started,
        scope="detail",
        parent="run_unified",
        status="ok",
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
        nonlocal asset_runs
        release_started = time.perf_counter()
        if driver.ctx is not None:
            driver.ctx.assets = []
        driver.assets = []
        driver.terrain_texture_jobs = []
        if asset_runs is not None:
            asset_runs.drop_all()
            asset_runs = None
        gc.collect()
        _record_timing(
            request,
            "unified_release_asset_state_before_lod",
            release_started,
            scope="detail",
            parent="run_unified",
        )

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
                    asset_join_started = time.perf_counter()
                    asset_thread.join()
                    _record_timing(
                        request,
                        "unified_asset_thread_join_wait",
                        asset_join_started,
                        scope="detail",
                        parent="run_unified",
                        overlaps="asset_track",
                    )
        if asset_error:
            raise asset_error[0]

        _run_post_phase(
            "Finalize Plugin Records",
            lambda progress: _finalize_plugin_records_after_asset_waves(
                driver,
                runner,
                mod_root,
                request.source_plugins[0],
                progress=progress,
                nif_run=getattr(asset_runs, "nifs", None),
            ),
            runner,
            request=request,
        )

        if getattr(request.options, "build_precombines", False):
            _run_post_phase(
                "Build Precombines",
                lambda progress: _build_precombines_after_build(
                    driver,
                    runner,
                    mod_root,
                    progress=progress,
                ),
                runner,
                request=request,
            )

        if getattr(request.options, "generate_previs", False):
            _run_post_phase(
                "Generate Previs",
                lambda progress: _generate_previs_after_build(
                    driver,
                    runner,
                    mod_root,
                    progress=progress,
                ),
                runner,
                request=request,
            )

        if driver.ctx is not None:
            pipboy_map_started = time.perf_counter()
            _run_post_phase(
                "Finalize Pip-Boy Map",
                lambda _progress: _finalize_fo76_pipboy_map_texture(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _record_timing(
                request,
                "unified_pipboy_map_finalization",
                pipboy_map_started,
                scope="detail",
                parent="run_unified",
            )
            _run_post_phase(
                "Restore Missing UI Sources",
                lambda _progress: _restore_fo76_ui_sources(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Copy VaultBoy SWFs",
                lambda _progress: _copy_fo76_vaultboy_swfs(
                    request,
                    driver.ctx,
                    runner,
                ),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Perk Vault Boys",
                lambda _progress: _convert_fo76_perk_vaultboys(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Legendary Perk UI",
                lambda _progress: _convert_fo76_legendary_perk_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Regular Perk UI",
                lambda _progress: _convert_fo76_perk_card_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Daily Ops UI",
                lambda _progress: _convert_fo76_daily_ops_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Keypad UI",
                lambda _progress: _convert_fo76_keypad_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Mission Map UI",
                lambda _progress: _convert_fo76_mission_map_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert CAMP Build UI",
                lambda _progress: _convert_fo76_camp_build_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Prepare CAMP Wallpapers",
                lambda _progress: _prepare_fo76_camp_wallpapers(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Prepare CAMP Snapping",
                lambda _progress: _prepare_fo76_camp_snap(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Prepare CAMP Scrapping",
                lambda _progress: _prepare_fo76_camp_scrap(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Photo Mode UI",
                lambda _progress: _convert_fo76_photo_mode_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Photo Gallery UI",
                lambda _progress: _convert_fo76_photo_gallery_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert FO76 Menu Presentation",
                lambda _progress: _convert_fo76_menu_presentation(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Quick-Boy UI",
                lambda _progress: _convert_fo76_quick_boy_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Currency UI",
                lambda _progress: _convert_fo76_currency_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Experience UI",
                lambda _progress: _convert_fo76_xp_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Quest Area UI",
                lambda _progress: _convert_fo76_quest_area_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Death Screen UI",
                lambda _progress: _convert_fo76_death_screen_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Revive Prompt UI",
                lambda _progress: _convert_fo76_revive_prompt_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Fishing UI",
                lambda _progress: _convert_fo76_fishing_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Inspect UI",
                lambda _progress: _convert_fo76_inspect_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Workbench Repair UI",
                lambda _progress: _convert_fo76_workbench_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Barter UI",
                lambda _progress: _convert_fo76_barter_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Generate Item Icons",
                lambda _progress: _generate_item_icons(request, driver.ctx, runner),
                runner,
                request=request,
            )
            if getattr(request.options, "upgrade_fo4_materials", False):
                _run_post_phase(
                    "Upgrade FO4 Materials",
                    lambda progress: _upgrade_fo4_materials(request, driver.ctx, runner, progress),
                    runner,
                    request=request,
                )
            if (request.source_game.lower(), request.target_game.lower()) == ("fo76", "fo4"):
                _run_post_phase(
                    "Repair Face Heads",
                    lambda _progress: _repair_face_heads_for(driver.ctx),
                    runner,
                    request=request,
                )
            _run_post_phase(
                "Convert Challenges",
                lambda _progress: _convert_fo76_challenges(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Expedition Results",
                lambda _progress: _convert_fo76_expedition_results(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Raid Rewards",
                lambda _progress: _convert_fo76_raid_rewards_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Reputation Presentation",
                lambda _progress: _convert_fo76_reputation_presentation(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Build Pet Runtime Catalog",
                lambda _progress: _convert_fo76_pets(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Build Player Ghoul Rules",
                lambda _progress: _convert_fo76_player_ghoul(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Respawn Map",
                lambda _progress: _convert_fo76_respawn_map(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Infestations",
                lambda _progress: _convert_fo76_infestations(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Build Pip-Boy Quest Categories",
                lambda _progress: _convert_fo76_quest_tabs(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Casino UI",
                lambda _progress: _convert_fo76_casino_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Copy Holotape Games",
                lambda _progress: _copy_fo76_holotape_programs(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Combat Perks",
                lambda _progress: _convert_fo76_combat_perks(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Favorites UI",
                lambda _progress: _convert_fo76_favorites_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Emotes",
                lambda _progress: _convert_fo76_emotes(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Status HUD UI",
                lambda _progress: _convert_fo76_status_hud_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Convert Real-time VATS UI",
                lambda _progress: _convert_fo76_realtime_vats_ui(request, driver.ctx, runner),
                runner,
                request=request,
            )
            _run_post_phase(
                "Emit Real-time VATS Catalog",
                lambda _progress: _emit_fo76_realtime_vats_catalog(request, driver.ctx, runner),
                runner,
                request=request,
            )

        from bacup_lib.legacy_tales_catalogs import remove_legacy_tales_catalogs

        remove_legacy_tales_catalogs(mod_root)

        _run_required_anim_text_data(request, driver.ctx, runner)

        if driver.ctx is not None:
            _run_post_phase(
                "Generate Pip-Boy 2000 AnimTextData",
                lambda _progress: _convert_fo76_pipboy2000_anim_text(request, driver.ctx, runner),
                runner,
                request=request,
            )

        if lod_hook is not None:
            release_asset_state_before_lod()
            runner.emit_log("INFO", "lod: native LOD generation (pre-pack)")
            _run_post_phase(
                "Generate LOD",
                lambda _progress: lod_hook(mod_root),
                runner,
                request=request,
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

                # The game mounts BA2s by plugin name, so archives are named
                # after the output plugin stem, which differs from the mod
                # folder on fnvfo3:fo4 (mods/FNV_FO3 -> FalloutNV.esm).
                archive_base_name = (
                    getattr(request, "archive_base_name", None) or mod_root.name
                )
                if request.target_game.lower() == "starfield":
                    # R1 SD-8: the fo4:starfield tail never goes through the
                    # generic multi-family planner in the `else` branch below
                    # — see pack_fo4_starfield_archives's docstring for why.
                    plans.extend(
                        pack_fo4_starfield_archives(
                            mod_root,
                            mod_name=archive_base_name,
                            archive_max_bytes=archive_max_bytes,
                            archive_output_dir=archive_output_dir,
                            expanded_archives=expanded_archives,
                            ba2_compression_level=ba2_compression_level,
                            pack_progress=emit_pack_progress,
                        )
                    )
                else:
                    plans.extend(
                        finalize_sinks_for_mod(
                            sink_id,
                            mod_root,
                            mod_name=archive_base_name,
                            archive_max_bytes=archive_max_bytes,
                            ba2_compression_level=ba2_compression_level,
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

            _run_post_phase(
                "Pack BA2", pack_body, runner,
                request=request,
            )
            runner.emit_log(
                "INFO", "join: wrote " + ", ".join(p.output_name for p in plans)
            )
        completion_started = time.perf_counter()
        driver.emit_complete(runner)
        mirror.finish("done")
        _record_timing(
            request,
            "unified_completion_emit_and_mirror_finish",
            completion_started,
            scope="detail",
            parent="run_unified",
            status="done",
        )
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
        failed_finish_started = time.perf_counter()
        mirror.finish("failed")
        _record_timing(
            request,
            "unified_mirror_finish_failed",
            failed_finish_started,
            scope="detail",
            parent="run_unified",
            status="failed",
        )
        raise
    finally:
        if asset_runs is not None:
            final_asset_drop_started = time.perf_counter()
            asset_runs.drop_all()
            _record_timing(
                request,
                "unified_final_asset_runs_drop",
                final_asset_drop_started,
                scope="detail",
                parent="run_unified",
            )
        if driver.ctx is not None:
            setattr(driver.ctx, "source_asset_inventory", None)
        sink_drop_started = time.perf_counter()
        sink_drop_status = "ok"
        try:
            native.sinks_drop(sink_id)
        except Exception:
            sink_drop_status = "failed"
        _record_timing(
            request,
            "unified_final_sink_drop",
            sink_drop_started,
            scope="detail",
            parent="run_unified",
            status=sink_drop_status,
        )
