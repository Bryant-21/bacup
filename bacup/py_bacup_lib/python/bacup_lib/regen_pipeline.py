"""Path-explicit FO76->FO4 full-regen pipeline core.

Env-free counterpart to ``scripts/regen.py``: the CLI resolves environment
into :class:`RegenPaths` / :class:`RegenOptions` and calls into here, and the
UI does the same from ``ToolkitSettings``. This module must never read
``os.environ`` for project config (see creation_lib env-read policy).
"""

from __future__ import annotations

import copy
from dataclasses import dataclass, field, replace
from pathlib import Path
from typing import NamedTuple

from bacup_lib.source_pairs import (
    DEFAULT_PAIR_ID,
    SOURCE_PAIRS,
    SourcePair,
    get_pair,
)

_OBJECT_LOD_OVERLAY_RELATIVE_PATH = Path(".modkit/object_lod_overlay.v1.json")


@dataclass
class RegenPaths:
    source_extracted_dir: Path
    source_data_dir: Path
    target_data_dir: Path
    target_ck_ini_path: Path
    target_custom_ini_path: Path
    target_game_ini_path: Path
    output_root: Path
    target_extracted_dir: Path | None = None
    mod_name: str = "SeventySix"
    # Tool-binary resource dir (texconv, etc.). Frozen builds must pass the
    # bundled location (get_resource_dir()); None falls back to
    # <project_root>/resource, correct for dev/CLI runs.
    resource_dir: Path | None = None
    # Optional deploy-only target. Conversion still resolves FO4 inputs from
    # target_data_dir; final deploy/undeploy copies use this virtual Data root.
    deploy_data_dir: Path | None = None
    # Explicit runtime archive-INI target override. When set, the deployed BA2s
    # are registered here even for a virtual (deploy_data_dir) target — the
    # caller has resolved the correct INI (e.g. an MO2 profile's fallout4custom.ini
    # or the global Fallout4Custom.ini). None = legacy behavior below.
    runtime_ini_path: Path | None = None
    # Optional run-specific diagnostics directory. When unset, reports stay in
    # output_root for compatibility with non-CLI callers.
    diagnostics_root: Path | None = None
    target_asset_catalog_path: Path | None = None
    target_asset_cache_dir: Path | None = None
    merge_primary_plugin_paths: tuple[Path, ...] = ()
    merge_grafted_plugin_paths: tuple[Path, ...] = ()
    # Ordered loose-asset fallbacks after source_extracted_dir. These are for
    # grafted plugin inputs, not terrain, whose source remains primary.
    additional_source_asset_roots: tuple[Path, ...] = ()


@dataclass
class RegenOptions:
    deploy: bool = True
    ba2_mode: str = "packed"  # "packed" | "expanded"
    archive_max_bytes: int = 16 * 1024**3
    workers: int | None = None
    asset_workers: int | None = None
    lod_mode: str = (
        "hybrid-atlas"  # "convert" | "generate" | "hybrid" | "hybrid-atlas" | "none"
    )
    pbr_carry: bool = False
    texture_landscape_mip_flooding: bool = False
    re_use_land: bool = False
    write_land_cache: bool = True
    include_interior: bool = True
    carry_interior_previs: bool = False
    records_limit: int | None = None
    emit_btd4: bool = False
    generate_anim_text_data: bool = False
    anim_text_data_native: bool = False
    validate_collision: bool = False
    validate_output: bool = False
    validation_warn_only: bool = False
    max_asset_failures: int | None = None
    max_seconds: int | None = None
    deep_invariants: bool = False
    export_yaml: bool = True
    cpu_textures: bool = False
    memory_report: bool = False
    direct_deploy_archives: bool = False
    update_runtime_ini: bool = True
    fo4_ba2_target: str = "nextgen"  # "og" | "nextgen"; resolved concrete by caller
    # Upgrade generation: regenerate only the families changed between the
    # deployed and target versions; reuse the rest from the live deployment.
    upgrade: bool = False
    mod_version: str | None = None  # stamped into SNAM; target for upgrades
    upgrade_from: str | None = None  # override; else read deployed ESM SNAM
    upgrade_manifest_path: Path | None = None


@dataclass
class RegenResult:
    exit_code: int
    output_root: Path
    elapsed_seconds: float
    deployed: bool
    failures: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)


import contextlib  # noqa: E402
import json  # noqa: E402
import logging  # noqa: E402
import os  # noqa: E402
import shutil  # noqa: E402
import subprocess  # noqa: E402
import sys  # noqa: E402
import time  # noqa: E402

LOG = logging.getLogger("regen")
_REPO_ROOT_FALLBACK = Path(__file__).resolve().parents[4]


def _remove_object_lod_overlay(path: Path) -> None:
    path.unlink(missing_ok=True)
    path.with_suffix(path.suffix + ".tmp").unlink(missing_ok=True)


@contextlib.contextmanager
def _object_lod_overlay_scope(path: Path):
    _remove_object_lod_overlay(path)
    try:
        yield
    finally:
        _remove_object_lod_overlay(path)


def _bind_object_lod_overlay_to_plugin(path: Path, plugin_path: Path) -> None:
    document = json.loads(path.read_text(encoding="utf-8"))
    plugin_name = document.get("plugin_name")
    if (
        not isinstance(plugin_name, str)
        or plugin_name.casefold() != plugin_path.name.casefold()
    ):
        raise RuntimeError(
            f"object LOD overlay plugin {plugin_name!r} does not match {plugin_path.name!r}"
        )
    metadata = plugin_path.stat()
    document["plugin_size"] = metadata.st_size
    document["plugin_mtime_ns"] = metadata.st_mtime_ns
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.unlink(missing_ok=True)
    try:
        temporary.write_text(
            json.dumps(document, indent=2, sort_keys=True),
            encoding="utf-8",
        )
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


OUTPUT_MOD_NAME = "SeventySix"


_FO4_ARCHIVE_INI_SECTION = "Archive"


_FO4_ARCHIVE_MAIN_KEY = "SResourceArchiveList"


_FO4_ARCHIVE_ANIMATION_KEY = "SResourceArchiveList2"


_FO4_ARCHIVE_TEXTURE_KEY = "sResourceIndexFileList"


_FO4_ARCHIVE_MEMORY_CACHE_KEY = "SResourceArchiveMemoryCacheList"


_FO4_ARCHIVE_STARTUP_KEY = "sResourceStartUpArchiveList"


_FO4_CK_ARCHIVE_AUTOLOAD_KEY = "bAutoloadTESFileArchives"


_FO4_CK_ARCHIVE_AUTOLOAD_VALUE = "0"


_FO4_CK_TEXTURE_ARCHIVES = (
    "CreationKit - Textures.ba2",
    "Fallout4 - TexturesPatch.ba2",
)


_FO4_MANAGED_ARCHIVE_KEYS = (
    _FO4_ARCHIVE_MAIN_KEY,
    _FO4_ARCHIVE_ANIMATION_KEY,
    _FO4_ARCHIVE_TEXTURE_KEY,
)


_FO4_SEEDED_ARCHIVE_KEYS = _FO4_MANAGED_ARCHIVE_KEYS


_FO4_CK_REMOVED_ARCHIVE_KEYS = (
    _FO4_ARCHIVE_MEMORY_CACHE_KEY,
    _FO4_ARCHIVE_STARTUP_KEY,
)


_RUNTIME_ARCHIVE_INI_STATE = ".regen_runtime_archive_ini_state.json"


_LAND_CACHE_PLUGIN = ".regen_land_cache.esm"


_LAND_CACHE_MARKER = ".regen_land_cache.json"


_LAND_CACHE_ASSET_DIR = ".regen_land_cache_assets"


_MEMORY_SAMPLE_INTERVAL_SECONDS = 2.0


FO76_PLUGINS = [
    "SeventySix.esm",
]

RESUME_PHASES = ("nifs", "textures", "havok", "lodgen", "pack", "deploy")

_RESUME_PHASE_ALIASES = {
    "nif": "nifs",
    "nifs": "nifs",
    "convert_nifs": "nifs",
    "convert_nifs_v2": "nifs",
    "texture": "textures",
    "textures": "textures",
    "convert_textures": "textures",
    "convert_textures_v2": "textures",
    "havok": "havok",
    "convert_havok": "havok",
    "lod": "lodgen",
    "lodgen": "lodgen",
    "generate_lod": "lodgen",
    "pack": "pack",
    "pack_ba2": "pack",
    "deploy": "deploy",
}

_RESUME_PHASE_LABELS = {
    "nifs": "Convert NIFs",
    "textures": "Convert Textures",
    "havok": "Convert Havok",
    "lodgen": "Generate LOD",
    "pack": "Pack BA2",
    "deploy": "Deploy Mod",
}


_WARNING_ONLY_INVARIANT_PREFIXES = ("BGSM with bCastShadows=False:",)


_APPALACHIA_CELL_CHECK_BOUNDS = (-32768, -32768, 32767, 32767)


_FO4_CK_MAX_SUBRECORD_LENGTHS = {
    ("FURN", "WBDT"): 2,
    ("TERM", "WBDT"): 2,
    ("MOVT", "SPED"): 112,
}


_FO4_CK_ROW_PROJECTIONS = {
    ("ARMO", "DAMA"): (12, 8),
    ("WEAP", "DAMA"): (12, 8),
}


def _write_conversion_reports(
    timing_report,
    memory_report,
    diagnostics_root: Path,
    elapsed: float,
) -> None:
    diagnostics_root.mkdir(parents=True, exist_ok=True)
    timing_report.write_json(
        diagnostics_root / "conversion_timing.json",
        total_elapsed_seconds=elapsed,
    )
    if memory_report is None:
        for stale_path in (
            diagnostics_root / "conversion_memory.json",
            diagnostics_root / "conversion_memory.md",
        ):
            with contextlib.suppress(FileNotFoundError):
                stale_path.unlink()
        LOG.info("memory report disabled")
        return

    memory_report.stop()
    memory_json_path = diagnostics_root / "conversion_memory.json"
    memory_markdown_path = diagnostics_root / "conversion_memory.md"
    memory_report.write_json(memory_json_path, total_elapsed_seconds=elapsed)
    memory_report.write_markdown(memory_markdown_path, total_elapsed_seconds=elapsed)
    peak = memory_report.summary(total_elapsed_seconds=elapsed).get("peak")
    if isinstance(peak, dict):
        LOG.info(
            "memory report written: %s peak_total_rss=%.3fGB stage=%s",
            memory_markdown_path,
            float(peak.get("total_rss_gb", 0.0)),
            peak.get("stage", ""),
        )
    else:
        LOG.info("memory report written: %s", memory_markdown_path)


def _ini_section_bounds(
    lines: list[str],
    section_name: str,
) -> tuple[int, int] | None:
    header = f"[{section_name.lower()}]"
    start = None
    for index, line in enumerate(lines):
        if line.strip().lower() == header:
            start = index
            break
    if start is None:
        return None

    end = len(lines)
    for index in range(start + 1, len(lines)):
        stripped = lines[index].strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            end = index
            break
    return start, end


def _ensure_ini_section(lines: list[str], section_name: str) -> tuple[int, int]:
    bounds = _ini_section_bounds(lines, section_name)
    if bounds is not None:
        return bounds

    if lines and not lines[-1].endswith(("\n", "\r")):
        lines[-1] = f"{lines[-1]}\n"
    if lines and lines[-1].strip():
        lines.append("\n")
    lines.append(f"[{section_name}]\n")
    start = len(lines) - 1
    return start, len(lines)


def _ini_key_value(line: str) -> tuple[str, str] | None:
    stripped = line.strip()
    if not stripped or stripped.startswith(("#", ";")) or "=" not in line:
        return None
    key, value = line.split("=", 1)
    return key.strip(), value.strip()


def _ini_line_newline(line: str) -> str:
    if line.endswith("\r\n"):
        return "\r\n"
    if line.endswith("\n"):
        return "\n"
    return ""


def _ini_list_values(value: str) -> list[str]:
    return [part.strip() for part in value.split(",") if part.strip()]


def _find_ini_key_line(
    lines: list[str],
    start: int,
    end: int,
    key: str,
) -> tuple[int, str, list[str]] | None:
    for index in range(start + 1, end):
        parsed = _ini_key_value(lines[index])
        if parsed is None:
            continue
        line_key, value = parsed
        if line_key.lower() == key.lower():
            return index, line_key, _ini_list_values(value)
    return None


def _ensure_ini_scalar_value(
    lines: list[str],
    start: int,
    end: int,
    key: str,
    value: str,
) -> tuple[bool, int]:
    for index in range(start + 1, end):
        parsed = _ini_key_value(lines[index])
        if parsed is None:
            continue
        line_key, existing_value = parsed
        if line_key.lower() != key.lower():
            continue
        if existing_value == value:
            return False, end
        newline = _ini_line_newline(lines[index])
        lines[index] = f"{line_key}={value}{newline}"
        return True, end

    lines.insert(end, f"{key}={value}\n")
    return True, end + 1


def _archive_ini_values_by_key(
    ini_path: Path | None,
    keys: tuple[str, ...],
) -> dict[str, list[str]]:
    if ini_path is None or not ini_path.is_file():
        return {}
    lines = ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
    bounds = _ini_section_bounds(lines, _FO4_ARCHIVE_INI_SECTION)
    if bounds is None:
        return {}

    start, end = bounds
    values_by_key: dict[str, list[str]] = {}
    for key in keys:
        existing_line = _find_ini_key_line(lines, start, end, key)
        if existing_line is None:
            continue
        _index, _line_key, existing_values = existing_line
        if existing_values:
            values_by_key[key] = _unique_archive_names(existing_values)
    return values_by_key


def _ensure_seeded_archive_ini_entries(
    lines: list[str],
    start: int,
    end: int,
    seed_values_by_key: dict[str, list[str]],
) -> tuple[bool, int]:
    changed = False
    for key in _FO4_SEEDED_ARCHIVE_KEYS:
        seed_values = seed_values_by_key.get(key, [])
        if not seed_values:
            continue
        existing_line = _find_ini_key_line(lines, start, end, key)
        if existing_line is None:
            lines.insert(end, f"{key}={', '.join(seed_values)}\n")
            end += 1
            changed = True
            continue

        index, line_key, existing_values = existing_line
        merged_values = _unique_archive_names([*seed_values, *existing_values])
        if merged_values == existing_values:
            continue
        newline = _ini_line_newline(lines[index])
        lines[index] = f"{line_key}={', '.join(merged_values)}{newline}"
        changed = True
    return changed, end


def _remove_ini_keys(
    lines: list[str],
    start: int,
    end: int,
    keys: tuple[str, ...],
) -> tuple[bool, int]:
    changed = False
    key_set = {key.lower() for key in keys}
    index = start + 1
    while index < end:
        parsed = _ini_key_value(lines[index])
        if parsed is None or parsed[0].lower() not in key_set:
            index += 1
            continue
        del lines[index]
        end -= 1
        changed = True
    return changed, end


def _remove_ini_archive_values(
    lines: list[str],
    start: int,
    end: int,
    key: str,
    archive_names: tuple[str, ...],
) -> bool:
    existing_line = _find_ini_key_line(lines, start, end, key)
    if existing_line is None:
        return False
    index, line_key, existing_values = existing_line
    removed = {name.lower() for name in archive_names}
    kept_values = [value for value in existing_values if value.lower() not in removed]
    if kept_values == existing_values:
        return False
    newline = _ini_line_newline(lines[index])
    lines[index] = f"{line_key}={', '.join(kept_values)}{newline}"
    return True


def _archive_label_base(archive_name: str) -> str:
    stem = Path(archive_name).stem
    if stem.lower().endswith("_xbox"):
        stem = stem[: -len("_xbox")]
    if " - " not in stem:
        return ""
    label = stem.rsplit(" - ", 1)[1]
    return label.rstrip("0123456789")


def _fo4_archive_ini_key_for_archive(archive_name: str) -> str | None:
    path = Path(archive_name)
    if path.suffix.lower() != ".ba2":
        return None
    label_base = _archive_label_base(path.name)
    if not label_base:
        return None
    if label_base in {"Textures", "LODTextures", "TerrainTextures"}:
        return _FO4_ARCHIVE_TEXTURE_KEY
    if label_base == "Animations":
        return _FO4_ARCHIVE_ANIMATION_KEY
    return _FO4_ARCHIVE_MAIN_KEY


def _unique_archive_names(archive_names: list[str]) -> list[str]:
    seen: set[str] = set()
    unique: list[str] = []
    for archive_name in archive_names:
        normalized = Path(archive_name).name
        key = normalized.lower()
        if key in seen:
            continue
        seen.add(key)
        unique.append(normalized)
    return unique


def _group_fo4_archive_ini_entries(
    archive_names: list[str],
) -> dict[str, list[str]]:
    grouped: dict[str, list[str]] = {key: [] for key in _FO4_MANAGED_ARCHIVE_KEYS}
    for archive_name in _unique_archive_names(archive_names):
        if Path(archive_name).stem.lower().endswith("_xbox"):
            continue
        key = _fo4_archive_ini_key_for_archive(archive_name)
        if key is None:
            continue
        grouped[key].append(archive_name)
    return {key: names for key, names in grouped.items() if names}


def _add_fo4_archive_ini_entries(
    archive_names: list[str],
    *,
    ini_path: Path | None = None,
    seed_ini_path: Path | None = None,
) -> list[str]:
    grouped = _group_fo4_archive_ini_entries(archive_names)
    if not grouped:
        return []

    if ini_path is None:
        raise ValueError("_add_fo4_archive_ini_entries requires an explicit ini_path")
    seed_values_by_key = _archive_ini_values_by_key(
        seed_ini_path,
        _FO4_SEEDED_ARCHIVE_KEYS,
    )
    lines = (
        ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
        if ini_path.is_file()
        else []
    )
    start, end = _ensure_ini_section(lines, _FO4_ARCHIVE_INI_SECTION)
    autoload_changed, end = _ensure_ini_scalar_value(
        lines,
        start,
        end,
        _FO4_CK_ARCHIVE_AUTOLOAD_KEY,
        _FO4_CK_ARCHIVE_AUTOLOAD_VALUE,
    )
    seeded_changed, end = _ensure_seeded_archive_ini_entries(
        lines,
        start,
        end,
        seed_values_by_key,
    )
    removed_keys_changed, end = _remove_ini_keys(
        lines,
        start,
        end,
        _FO4_CK_REMOVED_ARCHIVE_KEYS,
    )
    removed_archives_changed = _remove_ini_archive_values(
        lines,
        start,
        end,
        _FO4_ARCHIVE_TEXTURE_KEY,
        _FO4_CK_TEXTURE_ARCHIVES,
    )
    added: list[str] = []
    archive_entries_changed = False

    for key, names in grouped.items():
        seed_values = seed_values_by_key.get(key, [])
        existing_line = _find_ini_key_line(lines, start, end, key)
        if existing_line is None:
            values = _unique_archive_names([*seed_values, *names])
            lines.insert(end, f"{key}={', '.join(values)}\n")
            end += 1
            added.extend(names)
            archive_entries_changed = True
            continue

        index, line_key, existing_values = existing_line
        seen = {value.lower() for value in existing_values}
        missing = [name for name in names if name.lower() not in seen]
        updated_values = _unique_archive_names([*seed_values, *existing_values, *names])
        if not missing and updated_values == existing_values:
            continue
        newline = _ini_line_newline(lines[index])
        lines[index] = f"{line_key}={', '.join(updated_values)}{newline}"
        added.extend(missing)
        archive_entries_changed = True

    if (
        archive_entries_changed
        or autoload_changed
        or seeded_changed
        or removed_keys_changed
        or removed_archives_changed
    ):
        ini_path.parent.mkdir(parents=True, exist_ok=True)
        ini_path.write_text("".join(lines), encoding="utf-8")
    return _unique_archive_names(added)


def _remove_fo4_archive_ini_entries(
    archive_names: list[str],
    *,
    ini_path: Path | None = None,
) -> list[str]:
    names = _unique_archive_names(archive_names)
    if not names:
        return []

    if ini_path is None:
        raise ValueError(
            "_remove_fo4_archive_ini_entries requires an explicit ini_path"
        )
    if not ini_path.is_file():
        return []

    names_by_key = {name.lower(): name for name in names}
    lines = ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
    bounds = _ini_section_bounds(lines, _FO4_ARCHIVE_INI_SECTION)
    if bounds is None:
        return []
    start, end = bounds
    removed: list[str] = []

    for key in _FO4_MANAGED_ARCHIVE_KEYS:
        existing_line = _find_ini_key_line(lines, start, end, key)
        if existing_line is None:
            continue
        index, line_key, existing_values = existing_line
        kept_values: list[str] = []
        for value in existing_values:
            if value.lower() in names_by_key:
                removed.append(names_by_key[value.lower()])
                continue
            kept_values.append(value)
        if len(kept_values) == len(existing_values):
            continue
        newline = _ini_line_newline(lines[index])
        lines[index] = f"{line_key}={', '.join(kept_values)}{newline}"

    if removed:
        ini_path.write_text("".join(lines), encoding="utf-8")
    return _unique_archive_names(removed)


def _register_runtime_archive_ini_entries(
    archive_names: list[str],
    *,
    ini_path: Path | None = None,
    base_ini_path: Path | None = None,
) -> list[str]:
    """Register deployed BA2s in the *game-runtime* [Archive] lists.

    FO4 only auto-loads `<Plugin> - Main/Textures/Voices*.ba2`; any other
    generated archive names must be listed here, split by archive format:
    texture (DX10) -> sResourceIndexFileList, animations -> SResourceArchiveList2,
    everything else (GNRL) -> SResourceArchiveList.

    Targets Fallout4Custom.ini, whose [Archive] lists REPLACE (not merge with)
    Fallout4.ini's, so every touched key is seeded with the base-game values from
    Fallout4.ini before our archives are appended — otherwise vanilla content would
    drop out. Idempotent: only missing entries are added; returns the names added.
    """
    grouped = _group_fo4_archive_ini_entries(archive_names)
    if not grouped:
        return []

    if ini_path is None or base_ini_path is None:
        raise ValueError(
            "_register_runtime_archive_ini_entries requires explicit ini_path and base_ini_path"
        )
    base_values_by_key = _archive_ini_values_by_key(
        base_ini_path,
        _FO4_MANAGED_ARCHIVE_KEYS,
    )

    lines = (
        ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
        if ini_path.is_file()
        else []
    )
    start, end = _ensure_ini_section(lines, _FO4_ARCHIVE_INI_SECTION)

    added: list[str] = []
    changed = False
    for key in _FO4_MANAGED_ARCHIVE_KEYS:
        names = grouped.get(key, [])
        if not names:
            continue
        base_values = base_values_by_key.get(key, [])
        existing_line = _find_ini_key_line(lines, start, end, key)
        existing_values = existing_line[2] if existing_line is not None else []
        seen = {value.lower() for value in [*base_values, *existing_values]}
        missing = [name for name in names if name.lower() not in seen]
        merged_values = _unique_archive_names([*base_values, *existing_values, *names])
        if existing_line is None:
            lines.insert(end, f"{key}={', '.join(merged_values)}\n")
            end += 1
            added.extend(missing)
            changed = True
            continue
        index, line_key, _existing_values = existing_line
        if merged_values == existing_values:
            continue
        newline = _ini_line_newline(lines[index])
        lines[index] = f"{line_key}={', '.join(merged_values)}{newline}"
        added.extend(missing)
        changed = True

    if changed:
        ini_path.parent.mkdir(parents=True, exist_ok=True)
        ini_path.write_text("".join(lines), encoding="utf-8")
    return _unique_archive_names(added)


def _capture_runtime_archive_ini_state(ini_path: Path) -> dict:
    lines = (
        ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
        if ini_path.is_file()
        else []
    )
    bounds = _ini_section_bounds(lines, _FO4_ARCHIVE_INI_SECTION)
    keys: dict[str, dict[str, object]] = {}
    if bounds is not None:
        start, end = bounds
        for key in _FO4_MANAGED_ARCHIVE_KEYS:
            existing_line = _find_ini_key_line(lines, start, end, key)
            if existing_line is None:
                keys[key] = {"present": False}
                continue
            _index, line_key, values = existing_line
            keys[key] = {"present": True, "line_key": line_key, "values": values}
    else:
        keys = {key: {"present": False} for key in _FO4_MANAGED_ARCHIVE_KEYS}
    return {
        "exists": ini_path.is_file(),
        "archive_section_exists": bounds is not None,
        "keys": keys,
    }


def _write_runtime_archive_ini_state(
    state_path: Path, ini_path: Path | None = None
) -> bool:
    if state_path.exists():
        return False
    if ini_path is None:
        raise ValueError(
            "_write_runtime_archive_ini_state requires an explicit ini_path"
        )
    state_path.parent.mkdir(parents=True, exist_ok=True)
    state_path.write_text(
        json.dumps(_capture_runtime_archive_ini_state(ini_path), indent=2),
        encoding="utf-8",
    )
    return True


def _restore_runtime_archive_ini_state(
    state_path: Path, ini_path: Path | None = None
) -> bool:
    if not state_path.is_file():
        return False
    if ini_path is None:
        raise ValueError(
            "_restore_runtime_archive_ini_state requires an explicit ini_path"
        )
    state = json.loads(state_path.read_text(encoding="utf-8"))
    lines = (
        ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
        if ini_path.is_file()
        else []
    )
    changed = False
    bounds = _ini_section_bounds(lines, _FO4_ARCHIVE_INI_SECTION)
    if bounds is None and state.get("archive_section_exists"):
        start, end = _ensure_ini_section(lines, _FO4_ARCHIVE_INI_SECTION)
        changed = True
    elif bounds is None:
        state_path.unlink()
        return False
    else:
        start, end = bounds

    keys = state.get("keys", {})
    for key in _FO4_MANAGED_ARCHIVE_KEYS:
        desired = keys.get(key, {"present": False})
        existing_line = _find_ini_key_line(lines, start, end, key)
        if desired.get("present"):
            line_key = str(desired.get("line_key") or key)
            values = [str(value) for value in desired.get("values", [])]
            line = f"{line_key}={', '.join(values)}\n"
            if existing_line is None:
                lines.insert(end, line)
                end += 1
                changed = True
                continue
            index, _existing_key, _existing_values = existing_line
            newline = _ini_line_newline(lines[index])
            restored_newline = newline or "\n"
            restored = f"{line_key}={', '.join(values)}{restored_newline}"
            if lines[index] != restored:
                lines[index] = restored
                changed = True
            continue
        removed, end = _remove_ini_keys(lines, start, end, (key,))
        changed = changed or removed

    if changed:
        ini_path.parent.mkdir(parents=True, exist_ok=True)
        ini_path.write_text("".join(lines), encoding="utf-8")
    state_path.unlink()
    return changed


def _cleanup_fo4_archive_ini_overrides(
    *,
    ini_path: Path | None = None,
) -> bool:
    if ini_path is None:
        raise ValueError(
            "_cleanup_fo4_archive_ini_overrides requires an explicit ini_path"
        )
    if not ini_path.is_file():
        return False

    lines = ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
    bounds = _ini_section_bounds(lines, _FO4_ARCHIVE_INI_SECTION)
    if bounds is None:
        return False
    start, end = bounds
    removed_keys_changed, end = _remove_ini_keys(
        lines,
        start,
        end,
        _FO4_CK_REMOVED_ARCHIVE_KEYS,
    )
    removed_archives_changed = _remove_ini_archive_values(
        lines,
        start,
        end,
        _FO4_ARCHIVE_TEXTURE_KEY,
        _FO4_CK_TEXTURE_ARCHIVES,
    )
    if not removed_keys_changed and not removed_archives_changed:
        return False
    ini_path.write_text("".join(lines), encoding="utf-8")
    return True


def _fo4_ini_archive_names_for_plugins(
    plugin_names: list[str],
    *,
    ini_path: Path | None = None,
) -> list[str]:
    if ini_path is None:
        raise ValueError(
            "_fo4_ini_archive_names_for_plugins requires an explicit ini_path"
        )
    if not ini_path.is_file():
        return []
    plugin_stems = {Path(plugin_name).stem.lower() for plugin_name in plugin_names}
    lines = ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
    bounds = _ini_section_bounds(lines, _FO4_ARCHIVE_INI_SECTION)
    if bounds is None:
        return []

    start, end = bounds
    archive_names: list[str] = []
    for key in _FO4_MANAGED_ARCHIVE_KEYS:
        existing_line = _find_ini_key_line(lines, start, end, key)
        if existing_line is None:
            continue
        _index, _line_key, existing_values = existing_line
        for value in existing_values:
            path = Path(value)
            if path.suffix.lower() != ".ba2" or " - " not in path.stem:
                continue
            plugin_stem = path.stem.rsplit(" - ", 1)[0].lower()
            if plugin_stem in plugin_stems:
                archive_names.append(path.name)
    return _unique_archive_names(archive_names)


def _invariant_worker_count(conversion_workers: int | None) -> int:
    workers = (
        conversion_workers
        if conversion_workers is not None
        else ((os.cpu_count() or 2) // 2)
    )
    return max(1, min(workers, 16))


def _build_options(
    records_only: bool,
    conversion_workers: int | None,
    records_limit: int | None,
    validate_output: bool = False,
    validation_fail_on_error: bool = True,
    fo76_data_dir: Path | None = None,
    skip_placed_records: bool = False,
    skip_animations: bool = False,
    convert_lod: bool = True,
    reuse_terrain_navmesh: bool = False,
    terrain_graft_esm: Path | None = None,
    phases=None,
    overwrite_existing: bool = False,
    exclude_signatures: frozenset[str] = frozenset(),
    force_cpu_textures: bool = False,
    pbr_carry: bool = False,
    texture_landscape_mip_flooding: bool = False,
    convert_precombined_nifs: bool = False,
    disable_nif_collision_memo: bool = False,
    papyrus_compiler: str = "native",
    emit_btd4: bool = False,
    generate_anim_text_data: bool = False,
    anim_text_data_native: bool = False,
    validate_collision: bool = False,
    pair_id: str | None = None,
):
    from bacup_lib import PhaseSelection
    from bacup_lib.models import PluginPortOptions, TerrainOptions

    ps = phases or PhaseSelection()
    terrain = TerrainOptions(
        fo76_data_dir=""
        if records_only or fo76_data_dir is None
        else str(fo76_data_dir),
    )
    terrain.lod_mode = ps.lod_mode
    terrain.emit_btd4 = emit_btd4
    # Legacy skip_* params still force-off when passed; the PhaseSelection is the
    # primary source. convert_npc_faces is intentionally hardcoded False on BOTH
    # paths (the "npc-faces trap": PhaseSelection defaults it True but the
    # whole-plugin FO76->FO4 run never converts faces; sourcing it would change
    # output and break parity).
    placed = ps.convert_placed_records and not skip_placed_records
    animations = ps.convert_animations and not skip_animations
    # convert_btos (FO76 .bto -> FO4 .bto) must NOT run in generate/hybrid/hybrid-atlas/none mode:
    # lodgen produces fresh FO4 LODs there, and also converting the FO76 LODs
    # double-processes them (a malformed converted .bto null-derefs on load).
    lod = ps.convert_lod and convert_lod and ps.lod_mode == "convert"
    # Hybrid FO76 worlds without source BTO tiles fall back to record LOD, so
    # they need the same MNAM/proxy synthesis as generate mode.
    synthesize_object_lod = ps.lod_mode == "generate" or (
        pair_id in {None, DEFAULT_PAIR_ID} and ps.lod_mode in {"hybrid", "hybrid-atlas"}
    )

    if records_only:
        return PluginPortOptions(
            translate_records=ps.translate_records,
            convert_placed_records=placed,
            convert_npc_faces=False,
            convert_terrain=False,
            reuse_terrain_navmesh=reuse_terrain_navmesh,
            convert_nifs=False,
            convert_btos=False,
            convert_textures=False,
            convert_materials=False,
            convert_havok=False,
            synthesize_drivers=False,
            convert_animations=False,
            generate_anim_text_data=False,
            validate_collision=validate_collision,
            convert_scripts=ps.convert_scripts,
            copy_sounds=False,
            build_esp=ps.build_esp,
            validate_output=validate_output,
            validation_fail_on_error=validation_fail_on_error,
            conversion_workers=conversion_workers,
            records_limit=records_limit,
            terrain=terrain,
            placed_record_position_offset=(0.0, 0.0, 0.0),
            overwrite_existing=overwrite_existing,
            exclude_signatures=exclude_signatures,
            force_cpu_textures=force_cpu_textures,
            pbr_carry=pbr_carry,
            texture_landscape_mip_flooding=texture_landscape_mip_flooding,
            convert_precombined_nifs=convert_precombined_nifs,
            disable_nif_collision_memo=disable_nif_collision_memo,
            papyrus_compiler=papyrus_compiler,
        )

    return PluginPortOptions(
        translate_records=ps.translate_records,
        convert_placed_records=placed,
        convert_npc_faces=False,
        convert_terrain=ps.convert_terrain,
        reuse_terrain_navmesh=reuse_terrain_navmesh,
        terrain_graft_esm=terrain_graft_esm,
        convert_nifs=ps.convert_nifs,
        convert_btos=lod,
        synthesize_object_lod=synthesize_object_lod,
        convert_textures=ps.convert_textures,
        convert_materials=ps.convert_materials,
        convert_havok=ps.convert_havok,
        synthesize_drivers=ps.synthesize_drivers,
        convert_animations=animations,
        generate_anim_text_data=(
            generate_anim_text_data and ps.generate_anim_text_data
        ),
        anim_text_data_native=anim_text_data_native,
        validate_collision=validate_collision,
        convert_scripts=ps.convert_scripts,
        copy_sounds=ps.copy_sounds,
        build_esp=ps.build_esp,
        validate_output=validate_output,
        validation_fail_on_error=validation_fail_on_error,
        conversion_workers=conversion_workers,
        records_limit=records_limit,
        terrain=terrain,
        placed_record_position_offset=(0.0, 0.0, 0.0),
        overwrite_existing=overwrite_existing,
        exclude_signatures=exclude_signatures,
        force_cpu_textures=force_cpu_textures,
        pbr_carry=pbr_carry,
        texture_landscape_mip_flooding=texture_landscape_mip_flooding,
        convert_precombined_nifs=convert_precombined_nifs,
        disable_nif_collision_memo=disable_nif_collision_memo,
        papyrus_compiler=papyrus_compiler,
    )


def _run_generate_lod(
    mod_root: "Path",
    worldspaces: "list[str]",
    working_esm: "Path",
    asset_dirs: "list[Path]",
    settings: dict,
    runner_log,
    source_data_dir: "Path | None" = None,
    fo76_profile: bool = False,
    object_lod_overlay: "Path | None" = None,
) -> None:
    """Call the native lodgen wrapper for each worldspace and write into mod_root.

    The worldspace + every record lodgen reads (WRLD/CELL/LAND/REFR + base MNAM)
    come EXCLUSIVELY from ``working_esm`` (our freshly-built output ESM, which alone
    carries the synthesized DistantLOD/MNAM object LOD depends on). lodgen never
    discovers the plugin from the asset dirs, so a stale deployed copy in the live
    FO4 game install can't shadow it.

    LOD asset meshes/textures resolve from ``output_dir`` (our converted output)
    first, then ``asset_dirs`` (the extracted vanilla+DLC FO4 assets) — never the
    live game install.
    """
    from creation_lib.lod.native_runtime import generate_lod

    output_dir = Path(mod_root) / "data"
    lod_asset_dirs = [str(output_dir)] + [str(p) for p in asset_dirs]
    for world in worldspaces:
        world_settings = copy.deepcopy(settings)
        world_global = world_settings.setdefault("global", {})
        world_global["worldspaces"] = [world]
        objects_settings = world_settings.setdefault("objects", {})
        if fo76_profile and world.casefold() != "appalachia":
            world_global["stride"] = None
            world_global["southwest_cell"] = None
            world_global["bounds"] = None
        object_source = str(objects_settings.get("source", "records")).lower()
        if fo76_profile and object_source in {"fo76_bto", "fo76_bto_atlas"}:
            from creation_lib.lod.native_runtime import count_fo76_bto_tiles

            source_bto_count = (
                count_fo76_bto_tiles(source_data_dir, world)
                if source_data_dir is not None
                else 0
            )
            if source_bto_count == 0:
                world_global["generate_trees"] = True
                objects_settings["source"] = "records"
                objects_settings["fo76_bto_atlas_pages"] = False
                objects_settings["fo76_bto_merge_atlassed_shapes"] = False
                objects_settings["fo76_bto_tree_billboard_from_lod"] = None
                world_settings.setdefault("trees", {})["trees_3d"] = True
                runner_log(
                    "WARN",
                    f"lodgen {world}: no matching extracted source BTOs; "
                    "using record object LOD",
                )
            else:
                runner_log(
                    "INFO",
                    f"lodgen {world}: using {source_bto_count} extracted source "
                    f"BTO tile(s) for {object_source}",
                )
        if (
            str(objects_settings.get("source", "")).lower() == "fo76_bto_atlas"
            and objects_settings.get("fo76_bto_tree_billboard_from_lod") is not None
            and source_data_dir is not None
        ):
            from creation_lib.lod.billboards import (  # noqa: PLC0415
                BillboardRenderError,
                generate_fo76_bto_tree_billboards,
            )

            try:
                manifest_path, species_count = generate_fo76_bto_tree_billboards(
                    world,
                    world_settings,
                    source_data_dir=Path(source_data_dir),
                    data_dirs=[Path(source_data_dir), output_dir, *asset_dirs],
                    out_dir=output_dir,
                    progress=lambda m, f, _w=world: runner_log(
                        "INFO", f"lodgen {_w}: billboard {m} {f:.0%}"
                    ),
                )
            except BillboardRenderError as exc:
                runner_log(
                    "WARN",
                    f"lodgen {world}: billboard manifest not generated: {exc}",
                )
            except RuntimeError as exc:
                if "collect_fo76_bto_tree_billboard_species" not in str(exc):
                    raise
                runner_log(
                    "WARN",
                    f"lodgen {world}: billboard manifest not generated: {exc}",
                )
            else:
                if manifest_path is not None:
                    runner_log(
                        "INFO",
                        f"lodgen {world}: billboard manifest species={species_count} path={manifest_path}",
                    )
        runner_log("INFO", f"lodgen: generating LOD for {world}")
        result = generate_lod(
            world,
            world_settings,
            data_dirs=lod_asset_dirs,
            output_dir=str(output_dir),
            plugin_path=str(working_esm),
            source_data_dir=str(source_data_dir)
            if source_data_dir is not None
            else None,
            object_lod_overlay=(
                str(object_lod_overlay) if object_lod_overlay is not None else None
            ),
            progress=lambda m, f, _w=world: runner_log(
                "INFO", f"lodgen {_w}: {m} {f:.0%}"
            ),
        )
        runner_log(
            "INFO",
            f"lodgen {world}: btr={result.btr} bto={result.bto} dds={result.dds} "
            f"lod_written={result.lod_written} warnings={len(result.warnings)}",
        )
        if world_global.get("generate_terrain", True) and result.btr <= 0:
            raise RuntimeError(
                f"lodgen {world}: terrain LOD was enabled but generated no BTR files"
            )
        if world_global.get("write_lodsettings", True) and not result.lod_written:
            raise RuntimeError(
                f"lodgen {world}: LOD settings output was enabled but no .lod file was written"
            )
        for warning in result.warnings:
            runner_log("WARN", f"lodgen {world}: {warning}")
        # Remove atlas-map/cache intermediates (e.g. APPALACHIA.Objects.txt and
        # APPALACHIA.Objects.meta.json) that lodgen writes alongside atlas DDS.
        # The UV data is already baked into BTO meshes, so these files must not
        # ship under Textures/ where DX10 BA2 packing expects texture payloads.
        # Resolve the Objects/ dir case-insensitively so the cleanup works
        # regardless of on-disk casing produced by the native lodgen tool.
        obj_dir: Path | None = None
        terrain_root = output_dir / "Textures" / "Terrain"
        # Walk two levels: Terrain/<world_case>/Objects/
        if terrain_root.is_dir():
            for world_dir in terrain_root.iterdir():
                if world_dir.name.lower() == world.lower() and world_dir.is_dir():
                    for candidate in world_dir.iterdir():
                        if candidate.name.lower() == "objects" and candidate.is_dir():
                            obj_dir = candidate
                            break
                    if obj_dir is not None:
                        break
        if obj_dir is not None:
            for pattern, label in (
                ("*.txt", "atlas-map intermediate"),
                ("*.meta.json", "atlas cache metadata"),
            ):
                for path in obj_dir.glob(pattern):
                    path.unlink(missing_ok=True)
                    runner_log("INFO", f"lodgen: removed {label} {path.name}")


def _target_lod_asset_dirs(
    paths: "RegenPaths",
    lod_mode: str,
    *,
    working_esm: Path,
) -> list[Path]:
    from bacup_lib.target_assets import (
        build_target_asset_store,
        normalize_target_asset_path,
    )
    from creation_lib.esp.plugin import Plugin

    store = build_target_asset_store(
        target_data_dir=paths.target_data_dir,
        catalog_path=paths.target_asset_catalog_path,
        cache_dir=paths.target_asset_cache_dir,
        overlay_dir=paths.target_extracted_dir,
    )
    if store is None:
        return [paths.target_extracted_dir] if paths.target_extracted_dir else []

    plugin = Plugin.load(working_esm, game="fo4", eager_compressed=False)
    try:
        referenced = plugin.collect_assets()
    finally:
        plugin.close()
    roots: set[str] = set()
    for row in referenced:
        value = normalize_target_asset_path(str(row.get("source_path", "")))
        if not value:
            continue
        candidates = [value]
        if "/" not in value or value.split("/", 1)[0] not in {
            "meshes",
            "materials",
            "textures",
        }:
            suffix = Path(value).suffix.casefold()
            if suffix == ".nif":
                candidates.insert(0, f"meshes/{value}")
            elif suffix in {".bgsm", ".bgem"}:
                candidates.insert(0, f"materials/{value}")
            elif suffix == ".dds":
                candidates.insert(0, f"textures/{value}")
        roots.update(
            candidate for candidate in candidates if store.has_asset(candidate)
        )
    if roots:
        store.materialize_many(store.dependency_closure(roots))
    return [store.cache_data_root]


def _normalize_resume_phase(start_phase: str) -> str:
    key = str(start_phase or "").strip().lower().replace("-", "_").replace(" ", "_")
    phase = _RESUME_PHASE_ALIASES.get(key)
    if phase is None:
        raise ValueError(
            "unknown resume phase "
            f"{start_phase!r}; expected one of {', '.join(RESUME_PHASES)}"
        )
    return phase


def _existing_plugin_paths(paths: "RegenPaths", plugin_names: list[str]) -> list[Path]:
    return [paths.output_root / Path(name).name for name in plugin_names]


def _require_existing_plugins(
    paths: "RegenPaths",
    plugin_names: list[str],
    *,
    start_phase: str,
) -> None:
    missing = [
        p for p in _existing_plugin_paths(paths, plugin_names) if not p.is_file()
    ]
    if not missing:
        return
    label = _RESUME_PHASE_LABELS.get(start_phase, start_phase)
    joined = ", ".join(str(path) for path in missing)
    raise FileNotFoundError(
        f"Resume from {label} requires existing generated plugin(s): {joined}"
    )


def _resume_phase_selection(start_phase: str, phases):
    from bacup_lib import PhaseSelection

    selected = copy.copy(phases or PhaseSelection())
    for attr in (
        "translate_records",
        "convert_placed_records",
        "convert_npc_faces",
        "convert_terrain",
        "convert_lod",
        "convert_nifs",
        "convert_textures",
        "convert_materials",
        "convert_havok",
        "synthesize_drivers",
        "convert_animations",
        "convert_scripts",
        "copy_sounds",
        "build_esp",
    ):
        setattr(selected, attr, False)

    if start_phase == "nifs":
        selected.convert_nifs = True
        selected.convert_textures = True
        selected.convert_materials = True
        selected.convert_havok = True
        selected.synthesize_drivers = True
        selected.convert_animations = True
        selected.copy_sounds = True
    elif start_phase == "textures":
        selected.convert_textures = True
        selected.convert_materials = True
        selected.convert_havok = True
        selected.synthesize_drivers = True
        selected.convert_animations = True
        selected.copy_sounds = True
    elif start_phase == "havok":
        selected.convert_havok = True
        selected.synthesize_drivers = True
        selected.convert_animations = True
        selected.copy_sounds = True
    else:
        raise ValueError(f"{start_phase!r} is not an asset resume phase")
    return selected


def _prepare_lod_generation_settings(
    options: "RegenOptions",
    lod_settings: dict | None,
    lod_worldspaces: list[str] | None,
    resolved_workers: int,
    *,
    pair_id: str = DEFAULT_PAIR_ID,
) -> tuple[list[str], dict, bool, bool] | None:
    if options.lod_mode not in {"generate", "hybrid", "hybrid-atlas"}:
        return None
    if not lod_settings:
        raise RuntimeError(f"lod_mode={options.lod_mode} requires lod_settings")
    settings = copy.deepcopy(lod_settings)
    is_fo76_pair = pair_id == DEFAULT_PAIR_ID
    configured_worldspaces = list(settings.get("global", {}).get("worldspaces", []))
    discover_worldspaces = lod_worldspaces is None and not is_fo76_pair
    if lod_worldspaces is not None:
        wss = list(lod_worldspaces)
    elif is_fo76_pair:
        wss = configured_worldspaces
    else:
        wss = []
    if not discover_worldspaces and not wss:
        raise RuntimeError(
            f"lod_mode={options.lod_mode} requires at least one LOD worldspace"
        )
    global_settings = settings.setdefault("global", {})
    global_settings["workers"] = resolved_workers
    global_settings["worldspaces"] = list(wss)
    objects_settings = settings.setdefault("objects", {})
    if not is_fo76_pair:
        from creation_lib.lod.default_settings import fo4_default_settings

        default_objects = fo4_default_settings()["objects"]
        global_settings["stride"] = None
        global_settings["southwest_cell"] = None
        global_settings["bounds"] = None
        global_settings["generate_trees"] = True
        objects_settings["source"] = "records"
        for key, value in default_objects.items():
            if key.startswith("fo76_bto_"):
                objects_settings[key] = value
        settings.setdefault("trees", {})["trees_3d"] = True
    elif options.lod_mode == "hybrid":
        objects_settings["source"] = "fo76_bto"
    elif options.lod_mode == "hybrid-atlas":
        settings.setdefault("global", {}).setdefault("generate_trees", False)
        settings.setdefault("trees", {}).setdefault("trees_3d", False)
        objects_settings["source"] = "fo76_bto_atlas"
        objects_settings.setdefault("fo76_bto_atlas_pages", True)
        objects_settings.setdefault("fo76_bto_merge_atlassed_shapes", True)
        objects_settings.setdefault("fo76_bto_atlas_min_tile_size", 0)
        objects_settings.setdefault("fo76_bto_atlas_min_foliage_tile_size", 1024)
        objects_settings.setdefault("fo76_bto_atlas_foliage_page_size", 4096)
        objects_settings.setdefault("fo76_bto_atlas_foliage_max_tile_size", 1024)
        objects_settings.setdefault("fo76_bto_atlas_min_alpha_tested_tile_size", 1024)
        objects_settings.setdefault("fo76_bto_atlas_alpha_tested_page_size", 4096)
        objects_settings.setdefault("fo76_bto_atlas_alpha_tested_max_tile_size", 1024)
        objects_settings.setdefault("fo76_bto_atlas_from_lod", 8)
        objects_settings.setdefault("fo76_bto_tree_billboard_from_lod", None)
    lod_uses_fo76_bto = is_fo76_pair and (
        str(objects_settings.get("source", "records")).lower()
        in {"fo76_bto", "fo76_bto_atlas"}
    )
    return list(wss), settings, lod_uses_fo76_bto, discover_worldspaces


def _resolve_lod_worldspaces(
    configured_worldspaces: list[str],
    *,
    discover_from_plugin: bool,
    working_esm: Path,
    runner_log,
) -> list[str]:
    if discover_from_plugin:
        from creation_lib.lod.native_runtime import discover_worldspaces

        candidates = discover_worldspaces(working_esm, game="fo4")
        source = "discovered"
    else:
        candidates = configured_worldspaces
        source = "selected"

    worldspaces: list[str] = []
    seen: set[str] = set()
    for candidate in candidates:
        worldspace = str(candidate).strip()
        key = worldspace.casefold()
        if not worldspace or key in seen:
            continue
        seen.add(key)
        worldspaces.append(worldspace)

    if not worldspaces:
        if discover_from_plugin:
            raise RuntimeError(
                "LOD generation found no eligible worldspaces in freshly built "
                f"plugin: {working_esm}"
            )
        raise RuntimeError("LOD generation requires at least one worldspace")

    runner_log(
        "INFO",
        f"lodgen: {source} {len(worldspaces)} worldspace(s): " + ", ".join(worldspaces),
    )
    return worldspaces


def _run_runner_phase(
    runner,
    phase_no: int,
    label: str,
    body,
) -> None:
    from bacup_lib.models import PhaseProgress

    emit_start = getattr(runner, "emit_phase_start", None)
    emit_complete = getattr(runner, "emit_phase_complete", None)
    started = time.perf_counter()
    progress = PhaseProgress(
        phase=phase_no,
        phase_name=label,
        total_items=1,
        completed_items=0,
        status="running",
    )
    if callable(emit_start):
        emit_start(progress)
    try:
        body()
    except Exception as exc:
        progress.status = "error"
        progress.error = str(exc)
        progress.elapsed_seconds = time.perf_counter() - started
        if callable(emit_complete):
            emit_complete(progress)
        raise
    progress.status = "completed"
    progress.completed_items = 1
    progress.elapsed_seconds = time.perf_counter() - started
    if callable(emit_complete):
        emit_complete(progress)


def _run_existing_lodgen(
    paths: "RegenPaths",
    options: "RegenOptions",
    plugin_names: list[str],
    *,
    runner,
    lod_settings: dict | None,
    lod_worldspaces: list[str] | None,
    resolved_workers: int,
) -> None:
    prepared = _prepare_lod_generation_settings(
        options,
        lod_settings,
        lod_worldspaces,
        resolved_workers,
    )
    if prepared is None:
        raise RuntimeError(
            f"Resume from LODGen requires lod_mode generate/hybrid/hybrid-atlas, got {options.lod_mode!r}"
        )
    wss, settings, lod_uses_fo76_bto, discover_worldspaces = prepared
    working_esm = paths.output_root / Path(plugin_names[0]).name
    if not working_esm.is_file():
        raise FileNotFoundError(
            f"LOD generation requires the built plugin before lodgen: {working_esm}"
        )
    wss = _resolve_lod_worldspaces(
        wss,
        discover_from_plugin=discover_worldspaces,
        working_esm=working_esm,
        runner_log=runner.emit_log,
    )
    _run_generate_lod(
        mod_root=paths.output_root,
        worldspaces=wss,
        working_esm=working_esm,
        asset_dirs=_target_lod_asset_dirs(
            paths, options.lod_mode, working_esm=working_esm
        ),
        settings=settings,
        runner_log=runner.emit_log,
        source_data_dir=paths.source_extracted_dir if lod_uses_fo76_bto else None,
        fo76_profile=True,
    )


def _pack_existing_output(
    paths: "RegenPaths",
    options: "RegenOptions",
    *,
    resolved_workers: int,
) -> bool:
    from bacup_lib.native_runtime import load_native_module
    from bacup_lib.workflows.unified import finalize_sinks_for_mod

    archive_output_dir = None
    if options.deploy and options.direct_deploy_archives:
        archive_output_dir = paths.deploy_data_dir or paths.target_data_dir

    native = load_native_module()
    sink_id = native.sinks_create(
        json.dumps(
            {
                "mod_root": str(paths.output_root),
                "spill_dir": str(paths.output_root / "_sink_spills"),
                "emit_loose": True,
                "enable_ba2": False,
            }
        )
    )
    try:
        finalize_sinks_for_mod(
            sink_id,
            paths.output_root,
            mod_name=paths.output_root.name,
            archive_max_bytes=options.archive_max_bytes,
            reconcile_workers=resolved_workers,
            direct_pack_all=True,
            texture_pack_workers=resolved_workers,
            expanded_archives=(options.ba2_mode == "expanded"),
            archive_output_dir=archive_output_dir,
            fo4_ba2_target=options.fo4_ba2_target,
        )
    finally:
        try:
            native.sinks_drop(sink_id)
        except Exception:
            pass
    return archive_output_dir is not None


def _sanitize_existing_outputs(output_root: Path, plugin_names: list[str]) -> None:
    _sanitize_fo4_ck_payloads_for_outputs(output_root, plugin_names)
    _sanitize_fo4_ck_materials_for_outputs(output_root, plugin_names)


def _check_run_invariants(
    output_root: Path,
    records_only: bool,
    plugin_names: list[str],
    invariant_workers: int = 1,
    deep_invariants: bool = False,
    skip_pack: bool = False,
) -> tuple[list[str], list[str]]:
    from bacup_lib.invariants import check_run_invariants

    failures: list[str] = []
    warnings: list[str] = []
    if not output_root.is_dir():
        failures.append(f"missing output root: {output_root}")
        return failures, warnings

    for plugin_name in plugin_names:
        mod_dir = output_root
        if not mod_dir.is_dir():
            failures.append(f"missing mod dir: {mod_dir}")
            continue
        for relative in (".game", ".source_game", ".source_plugin"):
            if not (mod_dir / relative).exists():
                failures.append(f"missing {relative} in {mod_dir}")
        if records_only and not (mod_dir / "yaml").exists():
            failures.append(f"missing yaml in {mod_dir}")
        if not (mod_dir / f"{Path(plugin_name).stem}.esm").is_file():
            failures.append(f"missing output plugin in {mod_dir}")
        if not records_only and not skip_pack and not any(mod_dir.glob("*.ba2")):
            failures.append(f"missing BA2 archive in {mod_dir}")
        plugin_path = mod_dir / f"{Path(plugin_name).stem}.esm"

        tasks = [
            (
                "generic",
                lambda: check_run_invariants(
                    mod_dir,
                    expected_plugins=[plugin_name],
                    source_prefix="fo76",
                    allowed_asset_prefixes=("FO76",),
                    max_workers=invariant_workers,
                ),
            )
        ]
        if (
            deep_invariants
            and not records_only
            and plugin_name == "SeventySix.esm"
            and plugin_path.is_file()
        ):
            tasks.append(
                (
                    "appalachia",
                    lambda: _check_appalachia_cell_invariant(
                        plugin_path,
                        worker_count=invariant_workers,
                    ),
                )
            )
        elif (
            not records_only
            and plugin_name == "SeventySix.esm"
            and plugin_path.is_file()
        ):
            LOG.info(
                "invariant appalachia_cell_check: skipped; pass --deep-invariants to run"
            )

        invariant_results = [(name, task()) for name, task in tasks]

        result = next(value for name, value in invariant_results if name == "generic")
        for failure in result.failures:
            if failure.startswith(_WARNING_ONLY_INVARIANT_PREFIXES):
                warnings.append(failure)
            else:
                failures.append(failure)
        for name, value in invariant_results:
            if name != "appalachia":
                continue
            cell_failures, cell_warnings = value
            failures.extend(cell_failures)
            warnings.extend(cell_warnings)
    return failures, warnings


def _check_appalachia_cell_invariant(
    plugin_path: Path,
    *,
    worker_count: int | None = None,
) -> tuple[list[str], list[str]]:
    started = time.perf_counter()
    failures: list[str] = []
    warnings: list[str] = []
    try:
        appalachia_cells, cell_warnings = _count_appalachia_projected_cells(
            plugin_path,
            worker_count=worker_count,
        )
    except Exception as exc:
        failures.append(f"APPALACHIA cell check failed for {plugin_path}: {exc}")
        appalachia_cells = 0
    else:
        for warning in cell_warnings:
            warnings.append(f"APPALACHIA cell check: {warning}")
        if appalachia_cells <= 0:
            failures.append(f"missing projected APPALACHIA cells in {plugin_path}")
    LOG.info(
        "invariant appalachia_cell_check: plugin=%s cells=%d failures=%d warnings=%d elapsed=%.3fs",
        plugin_path,
        appalachia_cells,
        len(failures),
        len(warnings),
        time.perf_counter() - started,
    )
    return failures, warnings


def _bto_failure_count(result, options, records_only: bool) -> int:
    if records_only or not getattr(options, "convert_btos", False):
        return 0
    return int(getattr(result, "btos_failed", 0) or 0)


def _count_appalachia_projected_cells(
    plugin_path: Path,
    *,
    worker_count: int | None = None,
) -> tuple[int, list[str]]:
    from creation_lib.esp.plugin import Plugin

    min_x, min_y, max_x, max_y = _APPALACHIA_CELL_CHECK_BOUNDS
    _ = worker_count
    plugin = Plugin.load(
        plugin_path,
        game="fo4",
        eager_compressed=False,
        language="en",
    )
    try:
        terrain_ids = plugin.collect_worldspace_terrain_ids(
            worldspace_editor_id="APPALACHIA",
            min_x=min_x,
            min_y=min_y,
            max_x=max_x,
            max_y=max_y,
        )
        return (
            len(terrain_ids.get("cells") or []),
            [str(warning) for warning in terrain_ids.get("warnings") or []],
        )
    finally:
        plugin.close()


@contextlib.contextmanager
def _expose_plugin_mod_dir(
    output_root_name: str,
    plugin_name: str,
    *,
    project_root: Path,
):
    plugin_stem = Path(plugin_name).stem
    mod_dir = project_root / "mods" / output_root_name

    if (
        not output_root_name
        or output_root_name in {".", ".."}
        or "/" in output_root_name
        or "\\" in output_root_name
    ):
        raise ValueError(f"invalid output mod name: {output_root_name!r}")
    registered_plugin_names = {
        pair.merge.output_name if pair.merge is not None else pair.source_plugins[0]
        for pair in SOURCE_PAIRS.values()
    }
    if plugin_name not in registered_plugin_names:
        raise ValueError(f"unsupported conversion output plugin: {plugin_name}")
    if not mod_dir.is_dir():
        raise FileNotFoundError(f"missing plugin output dir: {mod_dir}")

    if output_root_name == plugin_stem:
        yield output_root_name, project_root
        return

    plugin_path = mod_dir / plugin_name
    if not plugin_path.is_file():
        raise FileNotFoundError(f"missing plugin output: {plugin_path}")
    exposed_plugin = mod_dir / f"{output_root_name}{plugin_path.suffix}"
    if exposed_plugin.exists():
        raise FileExistsError(
            f"temporary deploy alias already exists: {exposed_plugin}"
        )
    try:
        try:
            os.link(plugin_path, exposed_plugin)
        except OSError:
            shutil.copy2(plugin_path, exposed_plugin)
        yield output_root_name, project_root
    finally:
        exposed_plugin.unlink(missing_ok=True)


def _effective_conversion_workers(conversion_workers: int | None) -> int:
    if conversion_workers is not None:
        return conversion_workers

    from bacup_lib.models import auto_conversion_worker_count

    return auto_conversion_worker_count()


def _effective_resource_dir(paths: "RegenPaths") -> Path:
    """Resource (tool-binary) dir for deploy. Explicit when set (frozen builds
    pass the bundled get_resource_dir()); otherwise <project_root>/resource."""
    if paths.resource_dir is not None:
        return paths.resource_dir
    return paths.output_root.parent.parent / "resource"


def _deploy_output_mods(
    output_root_name: str,
    *,
    plugin_names: list[str],
    project_root: Path,
    game_data_dir: Path,
    resource_dir: Path,
    deploy_archives: bool = True,
) -> None:
    from creation_lib.build.deployer import deploy_mod

    for plugin_name in plugin_names:
        plugin_stem = Path(plugin_name).stem
        with _expose_plugin_mod_dir(
            output_root_name,
            plugin_name,
            project_root=project_root,
        ) as (mod_name, exposed_project_root):
            deploy_mod(
                mod_name,
                game="fo4",
                game_data_dir=game_data_dir,
                skip_build=True,
                skip_pack=True,
                skip_papyrus_compile=True,
                esp_only=False,
                skip_validation=True,
                project_root=exposed_project_root,
                resource_dir=resource_dir,
                deploy_archives=deploy_archives,
            )
        if output_root_name != plugin_stem:
            suffix = Path(plugin_name).suffix
            deployed_alias = game_data_dir / f"{output_root_name}{suffix}"
            if not deployed_alias.is_file():
                raise FileNotFoundError(
                    f"deployer did not produce expected plugin alias: {deployed_alias}"
                )
            os.replace(deployed_alias, game_data_dir / plugin_name)


def _deployed_archive_names(
    game_data_dir: Path,
    plugin_names: list[str],
) -> list[str]:
    from creation_lib.build.archive_plan import discover_mod_archives

    archive_names: list[str] = []
    for plugin_name in plugin_names:
        plugin_stem = Path(plugin_name).stem
        archive_names.extend(
            archive.name
            for archive in discover_mod_archives(
                game_data_dir,
                plugin_stem,
                extensions=(".ba2",),
            )
        )
    return _unique_archive_names(archive_names)


def _output_archive_names(
    output_root: Path,
    plugin_names: list[str],
) -> list[str]:
    from creation_lib.build.archive_plan import discover_mod_archives

    archive_names: list[str] = []
    for plugin_name in plugin_names:
        plugin_stem = Path(plugin_name).stem
        mod_dir = output_root
        archive_names.extend(
            archive.name
            for archive in discover_mod_archives(
                mod_dir,
                plugin_stem,
                extensions=(".ba2",),
            )
        )
    return _unique_archive_names(archive_names)


def _archives_for_labels(
    archive_names: list[str], labels: tuple[str, ...]
) -> list[str]:
    """Filter archive file names to those whose BA2 label matches one of
    `labels` by prefix: label == P or label == f"{P}{n}" for an integer n.
    This is a prefix match, not equality, so "Meshes" catches its numeric
    shards (Meshes1, Meshes2) without also catching "MeshesExtra" — a
    distinct family the caller must pass explicitly when it should swap too.
    """
    from creation_lib.build.archive_plan import _is_generated_archive_name

    prefix = f"{OUTPUT_MOD_NAME} - "
    matched: list[str] = []
    for name in archive_names:
        path = Path(name)
        if not path.name.startswith(prefix) or not _is_generated_archive_name(
            path, prefix
        ):
            continue
        label = path.stem[len(prefix) :]
        if label.endswith("_xbox"):
            label = label[: -len("_xbox")]
        for want in labels:
            if label == want or (
                label.startswith(want) and label[len(want) :].isdigit()
            ):
                matched.append(name)
                break
    return matched


def _swap_deploy_archives(
    output_root: Path,
    deploy_data_dir: Path,
    plugin_names: list[str],
    swap_labels: tuple[str, ...],
) -> None:
    """Delete the deployed BA2 shards for `swap_labels` and replace them with
    the freshly packed archives carrying the same labels. Every other
    deployed archive is left untouched. The caller (the upgrade resolver)
    passes the full expanded label set for a family (e.g. Meshes +
    MeshesExtra) so a swap sweeps all of that family's shards.

    `.btd4` needs no handling here: it deploys loose under `Terrain/` and the
    unconditional loose-dir deploy in `deploy_mod` (run right after this by
    `_deploy_output_mods`) already mirrors `output_root/Terrain/` -> deployed
    `Terrain/` whenever it's populated, and is a no-op otherwise — matching
    "regen copies fresh, reuse leaves the deployed file alone" for free.
    """
    deployed = _deployed_archive_names(deploy_data_dir, plugin_names)
    for stale_name in _archives_for_labels(deployed, swap_labels):
        stale_path = deploy_data_dir / stale_name
        if stale_path.is_file():
            stale_path.unlink()

    packed = _output_archive_names(output_root, plugin_names)
    for fresh_name in _archives_for_labels(packed, swap_labels):
        shutil.copy2(output_root / fresh_name, deploy_data_dir / fresh_name)


def _undeploy_output_mods(
    output_root_name: str,
    *,
    plugin_names: list[str],
    project_root: Path,
    game_data_dir: Path,
) -> list[str]:
    from creation_lib.build.deployer import undeploy_mod

    removed: list[str] = []
    for plugin_name in plugin_names:
        plugin_stem = Path(plugin_name).stem
        try:
            with _expose_plugin_mod_dir(
                output_root_name,
                plugin_name,
                project_root=project_root,
            ) as (mod_name, exposed_project_root):
                removed.extend(
                    undeploy_mod(
                        mod_name,
                        game="fo4",
                        game_data_dir=game_data_dir,
                        project_root=exposed_project_root,
                    )
                )
            if output_root_name != plugin_stem:
                removed.extend(_undeploy_output_mod_files(plugin_stem, game_data_dir))
        except FileNotFoundError:
            if output_root_name != plugin_stem:
                removed.extend(
                    _undeploy_output_mod_files(output_root_name, game_data_dir)
                )
            removed.extend(_undeploy_output_mod_files(plugin_stem, game_data_dir))
    return removed


def _undeploy_output_mod_files(plugin_stem: str, game_data_dir: Path) -> list[str]:
    from creation_lib.build.archive_plan import discover_mod_archives

    removed: list[str] = []
    for extension in ("esp", "esl", "esm"):
        plugin_path = game_data_dir / f"{plugin_stem}.{extension}"
        if plugin_path.is_file():
            plugin_path.unlink()
            removed.append(plugin_path.name)

    for archive in discover_mod_archives(game_data_dir, plugin_stem):
        archive.unlink()
        removed.append(archive.name)

    strings_dir = game_data_dir / "Strings"
    if strings_dir.is_dir():
        for strings_file in strings_dir.glob(f"{plugin_stem}_*"):
            if strings_file.is_file():
                strings_file.unlink()
                removed.append(f"Strings/{strings_file.name}")

    return removed


def _sanitize_fo4_ck_payloads_worker(plugin_path: Path) -> tuple[int, Path | None]:
    from creation_lib.esp.native_runtime import (
        plugin_handle_call,
        plugin_handle_close,
        plugin_handle_load,
    )

    changed_subrecords = 0
    strings_dir = plugin_path.parent / "Strings"
    handle = plugin_handle_load(
        str(plugin_path),
        game="fo4",
        strings_dir=str(strings_dir),
        language="en",
    )
    saved_path: Path | None = None
    try:
        max_lengths = [
            (record_sig, subrecord_sig, int(max_len))
            for (
                record_sig,
                subrecord_sig,
            ), max_len in _FO4_CK_MAX_SUBRECORD_LENGTHS.items()
        ]
        row_projections = [
            (record_sig, subrecord_sig, int(source_len), int(target_len))
            for (record_sig, subrecord_sig), (
                source_len,
                target_len,
            ) in _FO4_CK_ROW_PROJECTIONS.items()
        ]
        changed_subrecords = int(
            plugin_handle_call(
                handle,
                "sanitize_subrecord_payloads",
                max_lengths,
                row_projections,
            )
        )
        if changed_subrecords:
            saved_path = plugin_path.with_name(f".{plugin_path.stem}.ckfix.tmp.esm")
            saved_path.unlink(missing_ok=True)
            plugin_handle_call(handle, "save", str(saved_path))
    finally:
        plugin_handle_close(handle)
    return changed_subrecords, saved_path


def _run_fo4_ck_sanitizer_worker(plugin_path: Path) -> tuple[int, Path | None]:
    worker = """
import importlib.util
import json
import sys
from pathlib import Path

script_path = Path(sys.argv[1])
plugin_path = Path(sys.argv[2])
sys.path.insert(0, str(script_path.parent))
spec = importlib.util.spec_from_file_location("regen_ck_sanitizer_worker", script_path)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
sys.modules[spec.name] = module
spec.loader.exec_module(module)
changed, saved_path = module._sanitize_fo4_ck_payloads_worker(plugin_path)
print(json.dumps({"changed": changed, "saved_path": str(saved_path) if saved_path else None}))
"""
    result = subprocess.run(
        [
            sys.executable,
            "-c",
            worker,
            str(Path(__file__).resolve()),
            str(plugin_path.resolve()),
        ],
        cwd=str(_REPO_ROOT_FALLBACK),
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(
            "FO4 CK sanitizer worker failed"
            f"\nstdout:\n{result.stdout}"
            f"\nstderr:\n{result.stderr}"
        )
    lines = [line for line in result.stdout.splitlines() if line.strip()]
    if not lines:
        raise RuntimeError("FO4 CK sanitizer worker did not report a result")
    payload = json.loads(lines[-1])
    saved_path = payload.get("saved_path")
    return int(payload["changed"]), Path(saved_path) if saved_path else None


def _sanitize_fo4_ck_payloads(plugin_path: Path) -> int:
    from creation_lib.esp.plugin import replace_plugin_with_localized_sidecars

    if getattr(sys, "frozen", False):
        # Frozen builds have no standalone Python at sys.executable, so the
        # subprocess sanitizer worker can't run -- do it in-process instead.
        changed_subrecords, saved_path = _sanitize_fo4_ck_payloads_worker(plugin_path)
    else:
        changed_subrecords, saved_path = _run_fo4_ck_sanitizer_worker(plugin_path)
    if saved_path is not None:
        replace_plugin_with_localized_sidecars(saved_path, plugin_path)
        saved_path.unlink(missing_ok=True)
    return changed_subrecords


def _sanitize_fo4_ck_payloads_for_outputs(
    output_root: Path,
    plugin_names: list[str],
) -> None:
    for plugin_name in plugin_names:
        plugin_stem = Path(plugin_name).stem
        plugin_path = output_root / f"{plugin_stem}.esm"
        if not plugin_path.is_file():
            continue
        changed = _sanitize_fo4_ck_payloads(plugin_path)
        if changed:
            LOG.info(
                "FO4 CK sanitizer updated %s subrecords in %s", changed, plugin_path
            )


def _sanitize_fo4_ck_materials_for_outputs(
    output_root: Path,
    plugin_names: list[str],
) -> int:
    from bacup_lib.workflows.asset_phases import apply_material_overrides
    from creation_lib.material_tools.bgsm_bin import read_bgsm

    bgsm_paths: list[Path] = []
    for plugin_name in plugin_names:
        plugin_stem = Path(plugin_name).stem
        materials_root = output_root / "data" / "Materials"
        if not materials_root.is_dir():
            continue
        for path in materials_root.rglob("*.bgsm"):
            with path.open("rb") as handle:
                try:
                    cast_shadows = read_bgsm(handle).CastShadows
                except Exception:
                    continue
            if not cast_shadows:
                bgsm_paths.append(path)

    if bgsm_paths:
        apply_material_overrides(bgsm_paths, {"bCastShadows": True})
    return len(bgsm_paths)


def _resolve_source_plugins(
    plugin_names: list[str],
    *,
    data_dir: Path | None,
    extracted_dir: Path,
) -> list[Path]:
    source_plugins: list[Path] = []

    for plugin_name in plugin_names:
        if data_dir is not None:
            data_path = data_dir / plugin_name
            if data_path.is_file():
                source_plugins.append(data_path)
                continue

        extracted_path = extracted_dir / plugin_name
        if extracted_path.is_file():
            source_plugins.append(extracted_path)
            continue

        checked_paths = []
        if data_dir is not None:
            checked_paths.append(str(data_dir / plugin_name))
        checked_paths.append(str(extracted_path))
        raise FileNotFoundError(
            f"missing source plugin {plugin_name}; checked {', '.join(checked_paths)}"
        )

    return source_plugins


def _land_cache_paths(output_root: Path) -> tuple[Path, Path]:
    return output_root / _LAND_CACHE_PLUGIN, output_root / _LAND_CACHE_MARKER


def _land_cache_asset_root(output_root: Path) -> Path:
    return output_root / _LAND_CACHE_ASSET_DIR


def _casefold_child_dirs(parent: Path, name: str) -> list[Path]:
    if not parent.is_dir():
        return []
    folded = name.casefold()
    return [
        child
        for child in parent.iterdir()
        if child.is_dir() and child.name.casefold() == folded
    ]


def _iter_land_cache_asset_files(output_root: Path):
    for data_root in _casefold_child_dirs(output_root, "data"):
        for family in ("textures", "materials"):
            for family_root in _casefold_child_dirs(data_root, family):
                for terrain_root in _casefold_child_dirs(family_root, "terrain"):
                    for path in sorted(terrain_root.rglob("*")):
                        if path.is_file():
                            yield data_root, path


def _snapshot_land_cache_assets(output_root: Path) -> int:
    """Snapshot generated terrain assets that cached LTEX/TXST records reference."""
    cache_root = _land_cache_asset_root(output_root)
    tmp = cache_root.with_name(cache_root.name + ".tmp")
    if tmp.exists():
        shutil.rmtree(tmp)

    copied = 0
    for data_root, path in _iter_land_cache_asset_files(output_root):
        rel = path.relative_to(data_root)
        dst = tmp / "data" / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, dst)
        copied += 1

    if cache_root.exists():
        shutil.rmtree(cache_root)
    if copied:
        os.replace(tmp, cache_root)
    elif tmp.exists():
        shutil.rmtree(tmp)
    return copied


def _restore_land_cache_assets(output_root: Path) -> int:
    cache_root = _land_cache_asset_root(output_root)
    if not cache_root.is_dir():
        return 0

    copied = 0
    for path in sorted(cache_root.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(cache_root)
        dst = output_root / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, dst)
        copied += 1
    return copied


def _restore_land_cache_terrain_assets(output_root: Path) -> int:
    cache_root = _land_cache_asset_root(output_root)
    if not cache_root.is_dir():
        return 0

    copied = 0
    for path in sorted(cache_root.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(cache_root)
        if any(part.casefold() == "objects" for part in rel.parts):
            continue
        dst = output_root / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, dst)
        copied += 1
    return copied


def _source_plugin_signature(
    plugin_names: list[str],
    *,
    data_dir: Path | None,
    extracted_dir: Path,
) -> dict:
    """Cheap fingerprint of the FO76 source plugin(s): name -> size + mtime.

    The --re-use-land staleness signal. Content-hashing the multi-GB source on
    every run is not worth it; size+mtime catches a re-extract/replace, which is
    the only *input* change that invalidates reused terrain/navmesh. Terrain
    *code* changes are the user's responsibility (see the standing v1 notice).
    """
    sig: dict = {}
    try:
        for path in _resolve_source_plugins(
            plugin_names, data_dir=data_dir, extracted_dir=extracted_dir
        ):
            stat = path.stat()
            sig[path.name] = {"size": stat.st_size, "mtime": int(stat.st_mtime)}
    except FileNotFoundError:
        pass
    return sig


def _snapshot_land_cache(
    output_root: Path,
    plugin_names: list[str],
    *,
    data_dir: Path | None,
    extracted_dir: Path,
) -> bool:
    """Copy the just-built output ESM as the --re-use-land terrain/navmesh cache."""
    plugin_stem = Path(plugin_names[0]).stem
    esm_path = output_root / f"{plugin_stem}.esm"
    if not esm_path.is_file():
        return False
    cache_plugin, cache_marker = _land_cache_paths(output_root)
    tmp = cache_plugin.with_suffix(cache_plugin.suffix + ".tmp")
    tmp.unlink(missing_ok=True)
    shutil.copyfile(esm_path, tmp)
    os.replace(tmp, cache_plugin)
    asset_file_count = _snapshot_land_cache_assets(output_root)
    _write_land_cache_marker(
        cache_marker,
        cache_plugin,
        plugin_names,
        data_dir=data_dir,
        extracted_dir=extracted_dir,
        asset_file_count=asset_file_count,
    )
    LOG.info(
        "regen: refreshed --re-use-land cache (%s, %.1f GB, terrain assets=%d)",
        cache_plugin.name,
        cache_plugin.stat().st_size / 1024**3,
        asset_file_count,
    )
    return True


def _snapshot_land_cache_from_run(
    output_root: Path,
    plugin_names: list[str],
    run,
    *,
    data_dir: Path | None,
    extracted_dir: Path,
) -> bool:
    cache_plugin, cache_marker = _land_cache_paths(output_root)
    cache_plugin.parent.mkdir(parents=True, exist_ok=True)
    tmp = cache_plugin.with_suffix(cache_plugin.suffix + ".tmp")
    tmp.unlink(missing_ok=True)
    try:
        run.save_target(str(tmp), emit_authoring_yaml=False, run_nvnm_validator=False)
        os.replace(tmp, cache_plugin)
    except Exception:
        tmp.unlink(missing_ok=True)
        raise
    asset_file_count = _snapshot_land_cache_assets(output_root)
    _write_land_cache_marker(
        cache_marker,
        cache_plugin,
        plugin_names,
        data_dir=data_dir,
        extracted_dir=extracted_dir,
        asset_file_count=asset_file_count,
    )
    LOG.info(
        "regen: refreshed --re-use-land cache (%s, %.1f GB, terrain assets=%d)",
        cache_plugin.name,
        cache_plugin.stat().st_size / 1024**3,
        asset_file_count,
    )
    return True


def _write_land_cache_marker(
    cache_marker: Path,
    cache_plugin: Path,
    plugin_names: list[str],
    *,
    data_dir: Path | None,
    extracted_dir: Path,
    asset_file_count: int = 0,
) -> None:
    marker = {
        "version": 1,
        "written_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "source": _source_plugin_signature(
            plugin_names, data_dir=data_dir, extracted_dir=extracted_dir
        ),
        "cache_plugin": cache_plugin.name,
        "cache_assets": {
            "path": _LAND_CACHE_ASSET_DIR,
            "files": int(asset_file_count),
        },
    }
    marker_tmp = cache_marker.with_suffix(".json.tmp")
    marker_tmp.write_text(json.dumps(marker, indent=2), encoding="utf-8")
    os.replace(marker_tmp, cache_marker)


def _check_land_cache(
    output_root: Path,
    plugin_names: list[str],
    *,
    data_dir: Path | None,
    extracted_dir: Path,
) -> list[str]:
    """Validate the --re-use-land cache before a reuse run. Returns non-fatal
    staleness warnings; raises FileNotFoundError if the cache is missing (the
    caller turns that into a clear hard error)."""
    cache_plugin, cache_marker = _land_cache_paths(output_root)
    if not cache_plugin.is_file():
        raise FileNotFoundError(
            f"--re-use-land: no terrain/navmesh cache at {cache_plugin}. "
            "Run a full regen first to populate it "
            "(e.g. `uv run python scripts/regen.py --deploy`)."
        )
    warnings: list[str] = []
    if cache_marker.is_file():
        try:
            marker = json.loads(cache_marker.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            marker = {}
        recorded = marker.get("source")
        if recorded and recorded != _source_plugin_signature(
            plugin_names, data_dir=data_dir, extracted_dir=extracted_dir
        ):
            warnings.append(
                "source plugin changed since the cache was written; reused "
                "terrain/navmesh may not match the new input"
            )
    return warnings


def _deploy_post_steps(
    paths: "RegenPaths",
    plugin_names: list[str],
    timing_report,
    *,
    archives_already_deployed: bool = False,
    update_runtime_ini: bool = True,
    swap_labels: tuple[str, ...] | None = None,
) -> None:
    """Deploy generated output and register runtime archive INI entries.

    `swap_labels=None` is the current full deploy (unchanged behavior): every
    freshly packed archive replaces the deployed set. Non-None restricts the
    BA2 deploy to a selective per-family swap: only the deployed shards whose
    label matches one of `swap_labels` (by prefix — see `_archives_for_labels`)
    are deleted and replaced from the freshly packed output; every other
    deployed archive is left in place. The ESM is always replaced either way.
    """
    deploy_started = time.perf_counter()
    project_root = paths.output_root.parent.parent
    deploy_data_dir = paths.deploy_data_dir or paths.target_data_dir
    deploys_to_virtual_data = paths.deploy_data_dir is not None
    ck_ini_path = paths.target_ck_ini_path
    if paths.runtime_ini_path is not None:
        runtime_ini_target = paths.runtime_ini_path
    elif not deploys_to_virtual_data:
        runtime_ini_target = paths.target_custom_ini_path
    else:
        runtime_ini_target = None
    archive_plugin_names = [f"{paths.mod_name}.esm"]
    if swap_labels is not None:
        _swap_deploy_archives(
            paths.output_root,
            deploy_data_dir,
            archive_plugin_names,
            swap_labels,
        )
    _deploy_output_mods(
        paths.mod_name,
        plugin_names=plugin_names,
        project_root=project_root,
        game_data_dir=deploy_data_dir,
        resource_dir=_effective_resource_dir(paths),
        deploy_archives=not archives_already_deployed and swap_labels is None,
    )
    archive_entries = _deployed_archive_names(
        deploy_data_dir,
        archive_plugin_names,
    )
    if runtime_ini_target is None:
        timing_report.record(
            "deploy",
            time.perf_counter() - deploy_started,
            plugins=plugin_names,
            deploy_data_dir=str(deploy_data_dir),
            removed_archive_ini_entries=0,
            removed_runtime_ini_entries=0,
            cleaned_archive_ini_overrides=0,
            registered_runtime_ini_entries=0,
            runtime_ini_state_saved=False,
            ini_updates_skipped=True,
        )
        LOG.info(
            "deployed to virtual Data folder %s; skipped Fallout INI updates",
            deploy_data_dir,
        )
        return

    if not update_runtime_ini:
        timing_report.record(
            "deploy",
            time.perf_counter() - deploy_started,
            plugins=plugin_names,
            deploy_data_dir=str(deploy_data_dir),
            removed_archive_ini_entries=0,
            removed_runtime_ini_entries=0,
            cleaned_archive_ini_overrides=0,
            registered_runtime_ini_entries=0,
            runtime_ini_state_saved=False,
            ini_updates_skipped=True,
        )
        LOG.info(
            "deployed to %s; skipped Fallout INI updates",
            deploy_data_dir,
        )
        return

    runtime_ini_state_path = paths.output_root / _RUNTIME_ARCHIVE_INI_STATE
    runtime_ini_state_saved = _write_runtime_archive_ini_state(
        runtime_ini_state_path, ini_path=runtime_ini_target
    )
    if not deploys_to_virtual_data:
        ck_archive_entries = _unique_archive_names(
            [
                *_fo4_ini_archive_names_for_plugins(
                    archive_plugin_names, ini_path=ck_ini_path
                ),
                *archive_entries,
            ]
        )
        removed_ini_entries = _remove_fo4_archive_ini_entries(
            ck_archive_entries, ini_path=ck_ini_path
        )
        cleaned_ini_overrides = _cleanup_fo4_archive_ini_overrides(ini_path=ck_ini_path)
    else:
        removed_ini_entries = []
        cleaned_ini_overrides = 0
    runtime_archive_entries = _unique_archive_names(
        [
            *_fo4_ini_archive_names_for_plugins(
                archive_plugin_names,
                ini_path=runtime_ini_target,
            ),
            *archive_entries,
        ]
    )
    removed_runtime_ini_entries = _remove_fo4_archive_ini_entries(
        runtime_archive_entries,
        ini_path=runtime_ini_target,
    )
    registered_ini_entries = _register_runtime_archive_ini_entries(
        archive_entries,
        ini_path=runtime_ini_target,
        base_ini_path=paths.target_game_ini_path,
    )
    timing_report.record(
        "deploy",
        time.perf_counter() - deploy_started,
        plugins=plugin_names,
        deploy_data_dir=str(deploy_data_dir),
        removed_archive_ini_entries=len(removed_ini_entries),
        removed_runtime_archive_ini_entries=len(removed_runtime_ini_entries),
        cleaned_archive_ini_overrides=cleaned_ini_overrides,
        registered_runtime_ini_entries=len(registered_ini_entries),
        runtime_ini_state_saved=runtime_ini_state_saved,
        ini_updates_skipped=False,
    )
    if removed_ini_entries:
        LOG.info(
            "CK archive INI entries removed: %s",
            ", ".join(removed_ini_entries),
        )
    if cleaned_ini_overrides:
        LOG.info("CK archive INI shader/startup overrides removed")
    if removed_runtime_ini_entries:
        LOG.info(
            "runtime Fallout4Custom.ini stale archive entries removed: %s",
            ", ".join(removed_runtime_ini_entries),
        )
    if registered_ini_entries:
        LOG.info(
            "runtime Fallout4Custom.ini [Archive] registered %s of %s archive(s): %s",
            len(registered_ini_entries),
            len(archive_entries),
            ", ".join(registered_ini_entries),
        )
    else:
        LOG.info(
            "runtime Fallout4Custom.ini [Archive] already current (%s archive(s))",
            len(archive_entries),
        )


def _undeploy_main(
    paths: "RegenPaths", plugin_names: list[str], timing_report, started: float
) -> int:
    """Remove deployed output and restore runtime archive INI state."""
    output_root = paths.output_root
    diagnostics_root = paths.diagnostics_root or output_root
    game_data_dir = paths.deploy_data_dir or paths.target_data_dir
    undeploys_virtual_data = paths.deploy_data_dir is not None
    ck_ini_path = paths.target_ck_ini_path
    if paths.runtime_ini_path is not None:
        runtime_ini_target = paths.runtime_ini_path
    elif not undeploys_virtual_data:
        runtime_ini_target = paths.target_custom_ini_path
    else:
        runtime_ini_target = None
    archive_plugin_names = [f"{paths.mod_name}.esm"]
    archive_name_inputs = [
        *_deployed_archive_names(game_data_dir, archive_plugin_names),
        *_output_archive_names(output_root, archive_plugin_names),
    ]
    if not undeploys_virtual_data:
        archive_name_inputs.extend(
            _fo4_ini_archive_names_for_plugins(
                archive_plugin_names, ini_path=ck_ini_path
            )
        )
    archive_names = _unique_archive_names(archive_name_inputs)
    undeploy_started = time.perf_counter()
    removed_files = _undeploy_output_mods(
        paths.mod_name,
        plugin_names=plugin_names,
        project_root=output_root.parent.parent,
        game_data_dir=game_data_dir,
    )
    if not undeploys_virtual_data:
        removed_ini_entries = _remove_fo4_archive_ini_entries(
            archive_names, ini_path=ck_ini_path
        )
    else:
        removed_ini_entries = []
    if runtime_ini_target is None:
        removed_runtime_ini_entries = []
        runtime_ini_state_restored = False
    else:
        removed_runtime_ini_entries = _remove_fo4_archive_ini_entries(
            archive_names,
            ini_path=runtime_ini_target,
        )
        runtime_ini_state_restored = _restore_runtime_archive_ini_state(
            output_root / _RUNTIME_ARCHIVE_INI_STATE,
            ini_path=runtime_ini_target,
        )
    timing_report.record(
        "undeploy",
        time.perf_counter() - undeploy_started,
        plugins=plugin_names,
        deploy_data_dir=str(game_data_dir),
        removed_files=len(removed_files),
        removed_ini_entries=len(removed_ini_entries),
        removed_runtime_ini_entries=len(removed_runtime_ini_entries),
        runtime_ini_state_restored=runtime_ini_state_restored,
        ini_updates_skipped=runtime_ini_target is None,
    )
    timing_report.write_json(
        diagnostics_root / "conversion_timing.json",
        total_elapsed_seconds=time.perf_counter() - started,
    )
    LOG.info(
        "undeploy removed files=%s CK archive INI entries=%s runtime archive INI entries=%s",
        len(removed_files),
        len(removed_ini_entries),
        len(removed_runtime_ini_entries),
    )
    return 0


# ---------------------------------------------------------------------------
# Public path-explicit entry points (used by scripts/regen.py and the UI).
# ---------------------------------------------------------------------------


class _TerrainGraftPlan(NamedTuple):
    graft_esm: Path | None  # graft source ESM, or None (no graft)
    reuse_terrain_navmesh: bool  # -> build_kwargs / PluginPortOptions
    run_land_cache_block: bool  # run the legacy .regen_land_cache check/restore
    force_convert_terrain: bool  # fallback: regenerate terrain fully


def _plugin_is_readable(path: Path) -> bool:
    try:
        return path.is_file() and path.stat().st_size > 0
    except OSError:
        return False


def _resolve_terrain_graft(
    options: RegenOptions,
    terrain_graft_esm: Path | None,
    runner,
) -> _TerrainGraftPlan:
    """Decide the terrain-graft source and land-cache handling for a run.

    Non-upgrade (``terrain_graft_esm is None``): unchanged legacy behavior --
    the graft, if any, reads ``.regen_land_cache.esm`` and the cache block runs
    iff ``re_use_land``. Upgrade (``terrain_graft_esm`` set): graft from the live
    deployed ESM and never touch the cache; if that ESM is missing/unreadable,
    fall back to a full terrain regen with a WARN rather than hard-failing.
    """
    if terrain_graft_esm is None:
        return _TerrainGraftPlan(
            graft_esm=None,
            reuse_terrain_navmesh=options.re_use_land,
            run_land_cache_block=options.re_use_land,
            force_convert_terrain=False,
        )
    if _plugin_is_readable(terrain_graft_esm):
        return _TerrainGraftPlan(
            graft_esm=terrain_graft_esm,
            reuse_terrain_navmesh=True,
            run_land_cache_block=False,
            force_convert_terrain=False,
        )
    runner.emit_log(
        "WARN",
        f"regen upgrade: terrain graft source unreadable ({terrain_graft_esm}); "
        "regenerating terrain from scratch",
    )
    return _TerrainGraftPlan(
        graft_esm=None,
        reuse_terrain_navmesh=False,
        run_land_cache_block=False,
        force_convert_terrain=True,
    )


@dataclass(frozen=True)
class _UpgradeNoOp:
    """Sentinel: the deployed version already equals the target -- nothing to
    regenerate. ``run_full_regen`` logs "already at <target>" and exits 0 without
    deploying."""

    target: str
    already_current: bool


def _resolve_upgrade_plan(
    paths: "RegenPaths",
    options: "RegenOptions",
    pair: SourcePair | None = None,
):
    """Resolve an upgrade run into an execution plan.

    Returns one of:
      * ``None`` -- ``options.upgrade`` is False; the full-build path is unchanged.
      * :class:`_UpgradeNoOp` -- deployed version already equals the target.
      * :class:`~bacup_lib.family_map.UpgradePlan` -- the family
        closure to regenerate (``full_build`` when the range unions to ALL).

    ``from_version`` is ``options.upgrade_from`` when set, else the TES4 SNAM of
    the deployed pair output plugin. A missing/unrecognized deployed version
    unions to ``{"ALL"}`` -> a full-build plan. A downgrade range raises.
    """
    if not options.upgrade:
        return None
    pair = pair or get_pair(DEFAULT_PAIR_ID)
    from bacup_lib.family_map import resolve_upgrade_plan
    from bacup_lib.upgrade_manifest import (
        load_upgrade_manifest,
        requires_forced_regen,
        resolve_family_union,
    )
    from bacup_lib.version_stamp import read_plugin_snam

    manifest_path = options.upgrade_manifest_path
    if manifest_path is None:
        raise ValueError("upgrade mode requires options.upgrade_manifest_path")
    manifest_path = Path(manifest_path)
    if not manifest_path.is_file():
        raise FileNotFoundError(f"upgrade manifest not found: {manifest_path}")
    manifest = load_upgrade_manifest(manifest_path)

    if options.upgrade_from is not None:
        from_version = options.upgrade_from
    else:
        deployed_esm = (
            paths.deploy_data_dir or paths.target_data_dir
        ) / pair.output_plugin_name
        from_version = read_plugin_snam(deployed_esm)

    target = options.mod_version or manifest.current
    family_union = resolve_family_union(
        manifest,
        from_version,
        target,
        conversion_id=pair.pair_id,
    )
    force_regen = requires_forced_regen(
        manifest,
        from_version,
        target,
        conversion_id=pair.pair_id,
    )
    if not family_union and not force_regen:
        return _UpgradeNoOp(target=target, already_current=from_version == target)
    plan = resolve_upgrade_plan(frozenset({"ALL"}) if force_regen else family_union)
    return replace(
        plan,
        force_regen=force_regen,
    )


def _clean_forced_regen_output(paths: "RegenPaths", runner) -> None:
    extracted_dir = Path(paths.source_extracted_dir)
    if not extracted_dir.is_dir():
        raise FileNotFoundError(
            "forced regen requires an existing source extracted directory: "
            f"{extracted_dir}. Re-run Setup / Re-extract Game Data before converting."
        )

    output_root = Path(paths.output_root)
    protected_paths = {
        Path(paths.source_data_dir).resolve(),
        extracted_dir.resolve(),
        Path(paths.target_data_dir).resolve(),
    }
    if paths.deploy_data_dir is not None:
        protected_paths.add(Path(paths.deploy_data_dir).resolve())
    resolved_output = output_root.resolve()
    if any(
        resolved_output == protected
        or resolved_output in protected.parents
        or protected in resolved_output.parents
        for protected in protected_paths
    ):
        raise ValueError(f"refusing to clean protected conversion path: {output_root}")
    if output_root.exists() and not output_root.is_dir():
        raise NotADirectoryError(
            f"conversion output root is not a directory: {output_root}"
        )
    if output_root.is_dir():
        runner.emit_log("INFO", f"forced regen: clearing local output {output_root}")
        shutil.rmtree(output_root)


def _resolve_stamp_version(options: "RegenOptions") -> str | None:
    """Version to stamp into TES4 SNAM: an explicit ``mod_version``, else the
    manifest's ``current`` when a manifest is configured. ``None`` otherwise --
    the unchanged non-upgrade behavior (SNAM left untouched)."""
    if options.mod_version:
        return options.mod_version
    if options.upgrade_manifest_path is not None:
        from bacup_lib.upgrade_manifest import load_upgrade_manifest

        manifest_path = Path(options.upgrade_manifest_path)
        if manifest_path.is_file():
            return load_upgrade_manifest(manifest_path).current
    return None


def _run_merge_stage(
    pair: SourcePair,
    *,
    primary_paths: tuple[Path, ...],
    grafted_paths: tuple[Path, ...],
    work_dir: Path,
) -> Path:
    from bacup_lib.native_runtime import load_native_module

    if pair.merge is None:
        raise ValueError(f"source pair {pair.pair_id!r} has no merge stage")
    work_dir.mkdir(parents=True, exist_ok=True)
    output_path = work_dir / pair.merge.output_name
    load_native_module().conversion_merge_sources(
        {
            "primary_paths": [str(path) for path in primary_paths],
            "grafted_paths": [str(path) for path in grafted_paths],
            "output_path": str(output_path),
            "report_path": str(work_dir / "merge_report.json"),
            "game": pair.source_game,
        }
    )
    return output_path


def run_scripts_only(
    paths: RegenPaths,
    options: RegenOptions,
    *,
    runner,
    pair: SourcePair | None = None,
    papyrus_compiler: str = "native",
) -> RegenResult:
    """Decompile, patch, and compile scripts without loading or writing a plugin."""
    from bacup_lib.models import (
        ConversionContext,
        ConversionSummary,
        PluginPortOptions,
        PluginPortRequest,
    )
    from bacup_lib.target_assets import (
        build_target_asset_store,
        ensure_target_asset_catalog,
    )
    from bacup_lib.workflows.unified import _UnifiedRecordRuntime

    started = time.perf_counter()
    pair = pair or get_pair(DEFAULT_PAIR_ID)
    if pair.pair_id != DEFAULT_PAIR_ID:
        raise ValueError("--scripts-only currently supports only fo76:fo4")

    source_root = Path(paths.source_extracted_dir)
    if not source_root.is_dir():
        raise FileNotFoundError(f"source extracted directory not found: {source_root}")
    for asset_root in paths.additional_source_asset_roots:
        if not Path(asset_root).is_dir():
            raise FileNotFoundError(
                f"additional source asset root not found: {asset_root}"
            )

    output_root = Path(paths.output_root)
    diagnostics_root = Path(paths.diagnostics_root or output_root)
    output_root.mkdir(parents=True, exist_ok=True)
    diagnostics_root.mkdir(parents=True, exist_ok=True)
    resolved_workers = options.workers or _effective_conversion_workers(None)
    target_catalog_path = ensure_target_asset_catalog(
        paths.target_data_dir,
        paths.target_asset_catalog_path,
        log=lambda message: runner.emit_log("INFO", message),
    )
    target_asset_store = build_target_asset_store(
        target_data_dir=paths.target_data_dir,
        catalog_path=target_catalog_path,
        cache_dir=paths.target_asset_cache_dir,
        overlay_dir=paths.target_extracted_dir,
    )

    port_options = PluginPortOptions(
        translate_records=False,
        convert_placed_records=False,
        convert_npc_faces=False,
        convert_terrain=False,
        convert_nifs=False,
        convert_btos=False,
        synthesize_object_lod=False,
        convert_textures=False,
        convert_materials=False,
        convert_havok=False,
        synthesize_drivers=False,
        convert_animations=False,
        generate_anim_text_data=False,
        convert_scripts=True,
        copy_sounds=False,
        build_esp=False,
        conversion_workers=resolved_workers,
        papyrus_compiler=papyrus_compiler,
    )
    request = PluginPortRequest(
        source_game=pair.source_game,
        target_game=pair.target_game,
        source_plugins=[],
        output_root=output_root.parent,
        output_mod_name=paths.mod_name,
        target_extracted_dir=paths.target_extracted_dir,
        target_data_dir=paths.target_data_dir,
        source_data_dir=source_root,
        additional_source_asset_roots=paths.additional_source_asset_roots,
        target_asset_catalog_path=target_catalog_path,
        target_asset_cache_dir=paths.target_asset_cache_dir,
        target_master_paths=[],
        options=port_options,
        emit_authoring_yaml=False,
        diagnostics_root=diagnostics_root,
    )
    summary = ConversionSummary(mod_path=str(output_root))
    ctx = ConversionContext(
        source_game=pair.source_game,
        target_game=pair.target_game,
        mod_path=output_root,
        output_plugin_name=pair.output_plugin_name,
        target_extracted_dir=paths.target_extracted_dir,
        target_data_dir=paths.target_data_dir,
        formkey_mapper=None,
        fixups=None,
        summary=summary,
        target_asset_store=target_asset_store,
        target_asset_catalog_path=target_catalog_path,
        target_asset_cache_dir=paths.target_asset_cache_dir,
        source_data_dir=source_root,
        additional_source_asset_roots=paths.additional_source_asset_roots,
        conversion_workers=resolved_workers,
        emit_authoring_yaml=False,
        diagnostics_root=diagnostics_root,
    )
    setattr(ctx, "scripts_only", True)

    runner.emit_log(
        "INFO",
        "scripts-only: decompile, apply script_patches, and compile; "
        "plugin and asset phases are disabled",
    )
    _UnifiedRecordRuntime(request)._run_convert_scripts_phase(ctx, runner)

    failures = []
    if summary.scripts_flagged:
        failures.append(
            f"script conversion reported {summary.scripts_flagged} failure(s); "
            f"see {diagnostics_root / 'script_port_report.json'}"
        )
    return RegenResult(
        exit_code=2 if failures else 0,
        output_root=output_root,
        elapsed_seconds=time.perf_counter() - started,
        deployed=False,
        failures=failures,
    )


def run_full_regen(
    paths: RegenPaths,
    options: RegenOptions,
    *,
    pair: SourcePair | None = None,
    phases,
    runner,
    lod_settings: dict | None = None,
    lod_worldspaces: list[str] | None = None,
    timing_report=None,
    build_option_overrides: dict | None = None,
    terrain_graft_esm: Path | None = None,
) -> RegenResult:
    """Run the whole FO76->FO4 regen against explicit paths.

    Env-free counterpart of the old scripts/regen.py::run_full_mode body. The
    caller supplies a ``runner`` (StreamingConversionRunner for the CLI, the
    UI's ConversionRunner thread otherwise) and a ``phases`` PhaseSelection.
    """
    from bacup_lib.family_map import UpgradePlan
    from bacup_lib.models import PluginPortRequest, write_coverage_report
    from bacup_lib.source_pairs import DEFAULT_PAIR_ID, get_pair
    from bacup_lib.timing_report import TimingReport
    from bacup_lib.workflows.unified import run_unified

    started = time.perf_counter()
    pair = pair or get_pair(DEFAULT_PAIR_ID)
    timing_report = timing_report or TimingReport()
    output_root = paths.output_root
    diagnostics_root = paths.diagnostics_root or output_root
    plugin_names = list(pair.source_plugins)
    for asset_root in paths.additional_source_asset_roots:
        if not Path(asset_root).is_dir():
            raise FileNotFoundError(
                f"additional source asset root not found: {asset_root}"
            )
    resolved_workers = options.workers or _effective_conversion_workers(None)
    os.environ.setdefault("RAYON_NUM_THREADS", str(resolved_workers))

    upgrade_plan = _resolve_upgrade_plan(paths, options, pair)
    if isinstance(upgrade_plan, _UpgradeNoOp):
        message = (
            f"regen upgrade: deployed version already at {upgrade_plan.target}; "
            "nothing to regenerate"
            if upgrade_plan.already_current
            else f"regen upgrade: no changes for {pair.pair_id} through "
            f"{upgrade_plan.target}; nothing to regenerate"
        )
        runner.emit_log("INFO", message)
        return RegenResult(
            exit_code=0,
            output_root=output_root,
            elapsed_seconds=time.perf_counter() - started,
            deployed=False,
        )
    if isinstance(upgrade_plan, UpgradePlan) and upgrade_plan.force_regen:
        _clean_forced_regen_output(paths, runner)

    output_root.mkdir(parents=True, exist_ok=True)
    diagnostics_root.mkdir(parents=True, exist_ok=True)

    # The FO4 target-asset catalog is a derived index of the official BA2s
    # (always installed), not a shipped artifact. Build it on demand so the
    # multi-GB extracted FO4 dir is never required for a run.
    from bacup_lib.target_assets import ensure_target_asset_catalog

    ensure_target_asset_catalog(
        paths.target_data_dir,
        paths.target_asset_catalog_path,
        log=lambda message: runner.emit_log("INFO", message),
    )

    stamp_version = _resolve_stamp_version(options)

    # ``swap_labels`` stays None on the full-build and non-upgrade paths so the
    # deploy call and the direct-pack branch below behave byte-identically to
    # before. A selective upgrade sets it to the changed families' BA2 labels.
    swap_labels: tuple[str, ...] | None = None
    if isinstance(upgrade_plan, UpgradePlan):
        phases = upgrade_plan.phases
        reuse_terrain = not upgrade_plan.regen_terrain and not upgrade_plan.full_build
        deployed_esm = (
            paths.deploy_data_dir or paths.target_data_dir
        ) / pair.output_plugin_name
        terrain_graft_esm = deployed_esm if reuse_terrain else None
        # LOD reuse: unless the LOD family is being regenerated, skip lodgen
        # entirely and reuse the deployed LOD BA2. lod_mode gates BOTH
        # _prepare_lod_generation_settings (via options.lod_mode -> the lod_hook)
        # AND synthesize_object_lod/convert_btos (via phases.lod_mode ->
        # _build_options), so rebind options too, not just phases.
        keep_lod = upgrade_plan.full_build or phases.convert_lod
        effective_lod_mode = options.lod_mode if keep_lod else "none"
        options = replace(
            options, re_use_land=reuse_terrain, lod_mode=effective_lod_mode
        )
        swap_labels = None if upgrade_plan.full_build else upgrade_plan.swap_labels

    graft_plan = _resolve_terrain_graft(options, terrain_graft_esm, runner)

    if graft_plan.run_land_cache_block:
        # Raises FileNotFoundError if the cache is absent; staleness -> warnings.
        _check_land_cache(
            output_root,
            plugin_names,
            data_dir=paths.source_data_dir,
            extracted_dir=paths.source_extracted_dir,
        )
        restored_assets = _restore_land_cache_assets(output_root)
        if restored_assets:
            LOG.info(
                "regen: restored --re-use-land terrain assets (%d files)",
                restored_assets,
            )
        else:
            LOG.warning(
                "regen: --re-use-land cache has no terrain asset snapshot; "
                "run a full regen once to refresh the cache"
            )

    memory_report = None
    if options.memory_report:
        from bacup_lib.memory_report import MemoryReport

        memory_report = MemoryReport(
            sample_interval_seconds=_MEMORY_SAMPLE_INTERVAL_SECONDS
        )
        setattr(timing_report, "memory_report", memory_report)
        memory_report.start()

    phases.lod_mode = options.lod_mode
    if graft_plan.force_convert_terrain:
        phases.convert_terrain = True
    build_kwargs = dict(
        pair_id=pair.pair_id,
        validate_output=options.validate_output,
        validation_fail_on_error=not options.validation_warn_only,
        fo76_data_dir=paths.source_data_dir,
        phases=phases,
        reuse_terrain_navmesh=graft_plan.reuse_terrain_navmesh,
        terrain_graft_esm=graft_plan.graft_esm,
        overwrite_existing=True,
        force_cpu_textures=options.cpu_textures,
        pbr_carry=options.pbr_carry,
        texture_landscape_mip_flooding=options.texture_landscape_mip_flooding,
        papyrus_compiler="native",
        emit_btd4=options.emit_btd4,
        generate_anim_text_data=options.generate_anim_text_data,
        anim_text_data_native=options.anim_text_data_native,
        validate_collision=options.validate_collision,
    )
    if build_option_overrides:
        build_kwargs.update(build_option_overrides)
    port_options = _build_options(
        False, resolved_workers, options.records_limit, **build_kwargs
    )
    port_options.include_interior = options.include_interior
    port_options.carry_interior_previs = options.carry_interior_previs
    object_lod_overlay_path = output_root / _OBJECT_LOD_OVERLAY_RELATIVE_PATH

    if pair.merge is None:
        source_plugins = _resolve_source_plugins(
            plugin_names,
            data_dir=paths.source_data_dir,
            extracted_dir=paths.source_extracted_dir,
        )
    else:
        merged_plugin = _run_merge_stage(
            pair,
            primary_paths=paths.merge_primary_plugin_paths,
            grafted_paths=paths.merge_grafted_plugin_paths,
            work_dir=diagnostics_root / "merge",
        )
        source_plugins = [merged_plugin]
        plugin_names = [merged_plugin.name]
    request = PluginPortRequest(
        source_game=pair.source_game,
        target_game=pair.target_game,
        source_plugins=source_plugins,
        output_root=output_root.parent,
        output_mod_name=paths.mod_name,
        target_extracted_dir=paths.target_extracted_dir,
        target_data_dir=paths.target_data_dir,
        source_data_dir=paths.source_extracted_dir,
        additional_source_asset_roots=paths.additional_source_asset_roots,
        target_asset_catalog_path=paths.target_asset_catalog_path,
        target_asset_cache_dir=paths.target_asset_cache_dir,
        target_master_paths=[],
        options=port_options,
        emit_authoring_yaml=bool(options.export_yaml),
        diagnostics_root=diagnostics_root,
    )
    setattr(request, "timing_report", timing_report)
    if stamp_version is not None:
        # Read in unified.py's build_esp phase and stamped into the target
        # plugin's TES4 SNAM in-memory before serialization (avoids a
        # load-set-save on the written ~1 GB ESM).
        setattr(request, "mod_version", stamp_version)

    lod_hook = None
    land_cache_snapshotted = False

    def land_cache_hook(ctx) -> bool:
        nonlocal land_cache_snapshotted
        if options.re_use_land or not options.write_land_cache:
            return False
        run = getattr(ctx, "_rust_conversion_run", None)
        if run is None:
            return False
        land_cache_snapshotted = _snapshot_land_cache_from_run(
            output_root,
            plugin_names,
            run,
            data_dir=paths.source_data_dir,
            extracted_dir=paths.source_extracted_dir,
        )
        return land_cache_snapshotted

    lod_prepared = _prepare_lod_generation_settings(
        options,
        lod_settings,
        lod_worldspaces,
        resolved_workers,
        pair_id=pair.pair_id,
    )
    if lod_prepared is not None:
        wss, lod_settings, lod_uses_fo76_bto, discover_worldspaces = lod_prepared

        _lod_plugin_stem = Path(plugin_names[0]).stem

        def lod_hook(
            mod_root,
            _wss=wss,
            _s=lod_settings,
            _r=runner,
            _stem=_lod_plugin_stem,
            _source_data=paths.source_data_dir,
            _source_extracted=paths.source_extracted_dir,
            _plugins=plugin_names,
            _reuse_land=options.re_use_land,
            _write_land_cache=options.write_land_cache,
            _lod_mode=options.lod_mode,
            _lod_uses_fo76_bto=lod_uses_fo76_bto,
            _discover_worldspaces=discover_worldspaces,
            _fo76_profile=pair.pair_id == DEFAULT_PAIR_ID,
            _object_lod_overlay=object_lod_overlay_path,
            _use_object_lod_overlay=bool(port_options.synthesize_object_lod),
            _preserve_terrain_assets=not bool(port_options.convert_terrain),
        ):
            nonlocal land_cache_snapshotted
            if _write_land_cache and not _reuse_land and not land_cache_snapshotted:
                land_cache_snapshotted = _snapshot_land_cache(
                    Path(mod_root),
                    _plugins,
                    data_dir=_source_data,
                    extracted_dir=_source_extracted,
                )
            working_esm = Path(mod_root) / f"{_stem}.esm"
            if not working_esm.is_file():
                raise FileNotFoundError(
                    f"LOD generation requires the built plugin before lodgen: {working_esm}"
                )
            resolved_worldspaces = _resolve_lod_worldspaces(
                _wss,
                discover_from_plugin=_discover_worldspaces,
                working_esm=working_esm,
                runner_log=_r.emit_log,
            )
            overlay_for_lod = None
            if _use_object_lod_overlay and _object_lod_overlay.is_file():
                _bind_object_lod_overlay_to_plugin(_object_lod_overlay, working_esm)
                overlay_for_lod = _object_lod_overlay
            try:
                _run_generate_lod(
                    mod_root=mod_root,
                    worldspaces=resolved_worldspaces,
                    # Plugin/records: ONLY our working output ESM (never the game install).
                    working_esm=working_esm,
                    # Assets: converted output, then the sparse target cache.
                    asset_dirs=_target_lod_asset_dirs(
                        paths, _lod_mode, working_esm=working_esm
                    ),
                    settings=_s,
                    runner_log=_r.emit_log,
                    source_data_dir=(_source_extracted if _lod_uses_fo76_bto else None),
                    fo76_profile=_fo76_profile,
                    object_lod_overlay=overlay_for_lod,
                )
            finally:
                _remove_object_lod_overlay(_object_lod_overlay)
            if _preserve_terrain_assets and land_cache_snapshotted:
                restored = _restore_land_cache_terrain_assets(Path(mod_root))
                if restored:
                    _r.emit_log(
                        "INFO",
                        f"lodgen: restored {restored} pre-existing terrain asset(s)",
                    )

    failures: list[str] = []
    warnings: list[str] = []
    archive_output_dir = None
    # A selective swap (swap_labels set) must pack into output_root so
    # _deploy_post_steps can delete-then-copy the changed families from there;
    # direct-pack-to-deploy would leave the swap source empty. Full-build and
    # non-upgrade runs keep the direct-pack behavior.
    if options.deploy and options.direct_deploy_archives and swap_labels is None:
        archive_output_dir = paths.deploy_data_dir or paths.target_data_dir
    try:
        memory_stage = (
            memory_report.scoped_stage("pipeline:unified", plugins=plugin_names)
            if memory_report is not None
            else contextlib.nullcontext()
        )
        with _object_lod_overlay_scope(object_lod_overlay_path):
            with memory_stage:
                unified_result = run_unified(
                    request,
                    runner,
                    enable_ba2=options.deploy,
                    archive_max_bytes=options.archive_max_bytes,
                    expanded_archives=(options.ba2_mode == "expanded"),
                    archive_output_dir=archive_output_dir,
                    fo4_ba2_target=options.fo4_ba2_target,
                    max_asset_failures=options.max_asset_failures,
                    serialize_tracks=True,
                    asset_conversion_workers=options.asset_workers or resolved_workers,
                    lod_hook=lod_hook,
                    land_cache_hook=land_cache_hook,
                )
        result = unified_result.run_result
    except Exception:
        _write_conversion_reports(
            timing_report,
            memory_report,
            diagnostics_root,
            time.perf_counter() - started,
        )
        raise

    sanitizer_started = time.perf_counter()
    runner.emit_log("INFO", "postflight: sanitizing generated plugin outputs")
    _sanitize_existing_outputs(output_root, plugin_names)
    runner.emit_log(
        "INFO",
        "postflight: plugin sanitization complete "
        f"elapsed={time.perf_counter() - sanitizer_started:.1f}s",
    )
    write_coverage_report(
        diagnostics_root / "conversion_report.md",
        decisions=result.decisions,
        translated_counts=result.translated_counts,
        skipped_counts=result.skipped_counts,
        failed_nifs=result.failed_nifs,
        failed_textures=result.failed_textures,
        failed_bgsms=result.failed_bgsms,
    )
    if _bto_failure_count(result, port_options, False):
        failures.append(
            f"BTO conversion failed: {getattr(result, 'btos_failed', 0)}/"
            f"{getattr(result, 'btos_total', 0)} terrain BTO(s)"
        )

    if not failures:
        if options.deep_invariants:
            runner.emit_log("INFO", "postflight: running deep output invariants")
            f, w = _check_run_invariants(
                output_root,
                False,
                plugin_names,
                invariant_workers=_invariant_worker_count(resolved_workers),
                deep_invariants=True,
                skip_pack=bool(archive_output_dir) or not options.deploy,
            )
            failures.extend(f)
            warnings.extend(w)
        else:
            runner.emit_log("INFO", "postflight: deep output invariants disabled")

    deployed = False
    if not failures:
        if (
            options.write_land_cache
            and not options.re_use_land
            and not land_cache_snapshotted
        ):
            land_cache_snapshotted = _snapshot_land_cache(
                output_root,
                plugin_names,
                data_dir=paths.source_data_dir,
                extracted_dir=paths.source_extracted_dir,
            )
        if options.deploy:
            deploy_stage = (
                memory_report.scoped_stage("pipeline:deploy", plugins=plugin_names)
                if memory_report is not None
                else contextlib.nullcontext()
            )
            deploy_kwargs = dict(
                archives_already_deployed=bool(archive_output_dir),
                update_runtime_ini=options.update_runtime_ini,
            )
            if swap_labels is not None:
                deploy_kwargs["swap_labels"] = swap_labels
            with deploy_stage:
                _deploy_post_steps(
                    paths,
                    plugin_names,
                    timing_report,
                    **deploy_kwargs,
                )
            deployed = True

    elapsed = time.perf_counter() - started
    _write_conversion_reports(timing_report, memory_report, diagnostics_root, elapsed)

    exit_code = 0 if not failures else 2
    if (
        exit_code == 0
        and options.max_seconds is not None
        and elapsed > options.max_seconds
    ):
        exit_code = 3
    return RegenResult(
        exit_code=exit_code,
        output_root=output_root,
        elapsed_seconds=elapsed,
        deployed=deployed,
        failures=failures,
        warnings=warnings,
    )


def run_resume_from_phase(
    paths: RegenPaths,
    options: RegenOptions,
    *,
    start_phase: str,
    phases,
    runner,
    lod_settings: dict | None = None,
    lod_worldspaces: list[str] | None = None,
    timing_report=None,
) -> RegenResult:
    """Resume a generated Appalachia output from a chosen phase.

    The selected phase and every downstream phase overwrite existing outputs.
    Earlier conversion phases are used only for the minimal setup the pipeline
    needs, not for rebuilding records or terrain.
    """
    from bacup_lib.timing_report import TimingReport

    phase = _normalize_resume_phase(start_phase)
    plugin_names = list(FO76_PLUGINS)
    _require_existing_plugins(paths, plugin_names, start_phase=phase)

    if phase in {"nifs", "textures", "havok"}:
        selected = _resume_phase_selection(phase, phases)
        selected.lod_mode = options.lod_mode
        runner.emit_log(
            "INFO",
            "Resuming Appalachia conversion from "
            f"{_RESUME_PHASE_LABELS[phase]}; outputs from this phase onward will be overwritten",
        )
        return run_full_regen(
            paths,
            options,
            phases=selected,
            runner=runner,
            lod_settings=lod_settings,
            lod_worldspaces=lod_worldspaces,
            timing_report=timing_report,
        )

    if phase == "deploy":
        runner.emit_log("INFO", "Resuming Appalachia conversion from Deploy Mod")
        return deploy_existing(
            paths,
            plugin_names=plugin_names,
            update_runtime_ini=options.update_runtime_ini,
            timing_report=timing_report,
        )

    started = time.perf_counter()
    tr = timing_report or TimingReport()
    output_root = paths.output_root
    diagnostics_root = paths.diagnostics_root or output_root
    output_root.mkdir(parents=True, exist_ok=True)
    diagnostics_root.mkdir(parents=True, exist_ok=True)
    resolved_workers = options.workers or _effective_conversion_workers(None)
    failures: list[str] = []
    warnings: list[str] = []
    archives_already_deployed = False

    try:
        runner.emit_log(
            "INFO",
            "Resuming Appalachia conversion from "
            f"{_RESUME_PHASE_LABELS[phase]}; outputs from this phase onward will be overwritten",
        )
        if phase == "lodgen":
            _run_runner_phase(
                runner,
                90,
                "Generate LOD",
                lambda: _run_existing_lodgen(
                    paths,
                    options,
                    plugin_names,
                    runner=runner,
                    lod_settings=lod_settings,
                    lod_worldspaces=lod_worldspaces,
                    resolved_workers=resolved_workers,
                ),
            )

        _sanitize_existing_outputs(output_root, plugin_names)

        if phase in {"lodgen", "pack"}:
            pack_result = {"archives_already_deployed": False}

            def pack_body() -> None:
                pack_result["archives_already_deployed"] = _pack_existing_output(
                    paths,
                    options,
                    resolved_workers=resolved_workers,
                )

            _run_runner_phase(
                runner,
                91,
                "Pack BA2",
                pack_body,
            )
            archives_already_deployed = bool(pack_result["archives_already_deployed"])

        if options.deep_invariants:
            f, w = _check_run_invariants(
                output_root,
                False,
                plugin_names,
                invariant_workers=_invariant_worker_count(resolved_workers),
                deep_invariants=True,
                skip_pack=archives_already_deployed,
            )
            failures.extend(f)
            warnings.extend(w)

        deployed = False
        if not failures and options.deploy:
            _run_runner_phase(
                runner,
                92,
                "Deploy Mod",
                lambda: _deploy_post_steps(
                    paths,
                    plugin_names,
                    tr,
                    archives_already_deployed=archives_already_deployed,
                    update_runtime_ini=options.update_runtime_ini,
                ),
            )
            deployed = True
    except Exception:
        _write_conversion_reports(
            tr,
            None,
            diagnostics_root,
            time.perf_counter() - started,
        )
        raise

    elapsed = time.perf_counter() - started
    _write_conversion_reports(tr, None, diagnostics_root, elapsed)
    exit_code = 0 if not failures else 2
    if (
        exit_code == 0
        and options.max_seconds is not None
        and elapsed > options.max_seconds
    ):
        exit_code = 3
    return RegenResult(
        exit_code=exit_code,
        output_root=output_root,
        elapsed_seconds=elapsed,
        deployed=deployed,
        failures=failures,
        warnings=warnings,
    )


def deploy_existing(
    paths: RegenPaths,
    *,
    plugin_names: list[str] | None = None,
    update_runtime_ini: bool = True,
    timing_report=None,
) -> RegenResult:
    """Deploy an already-generated conversion output without running conversion."""
    from bacup_lib.timing_report import TimingReport

    started = time.perf_counter()
    tr = timing_report or TimingReport()
    names = plugin_names or list(FO76_PLUGINS)
    missing = [name for name in names if not (paths.output_root / name).is_file()]
    if missing:
        return RegenResult(
            exit_code=2,
            output_root=paths.output_root,
            elapsed_seconds=time.perf_counter() - started,
            deployed=False,
            failures=[
                f"missing generated plugin: {paths.output_root / name}"
                for name in missing
            ],
        )

    output_archives = _output_archive_names(paths.output_root, names)
    _deploy_post_steps(
        paths,
        names,
        tr,
        archives_already_deployed=not bool(output_archives),
        update_runtime_ini=update_runtime_ini,
    )
    return RegenResult(
        exit_code=0,
        output_root=paths.output_root,
        elapsed_seconds=time.perf_counter() - started,
        deployed=True,
    )


def undeploy(
    paths: RegenPaths, *, plugin_names: list[str] | None = None, timing_report=None
) -> RegenResult:
    from bacup_lib.timing_report import TimingReport

    tr = timing_report or TimingReport()
    names = plugin_names or list(FO76_PLUGINS)
    code = _undeploy_main(paths, names, tr, time.perf_counter())
    return RegenResult(
        exit_code=code,
        output_root=paths.output_root,
        elapsed_seconds=0.0,
        deployed=False,
    )
