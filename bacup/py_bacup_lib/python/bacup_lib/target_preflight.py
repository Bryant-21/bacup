"""Preflight indexes for target-record and target-asset reuse."""
from __future__ import annotations

import os
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Iterable

from bacup_lib.target_masters import resolve_official_target_master_paths
from creation_lib.esp.plugin import Plugin


RecordRowsCollector = Callable[[object, str], list[dict]]


@dataclass
class TargetAssetIndex:
    keys: set[str] = field(default_factory=set)
    owners: dict[str, str] = field(default_factory=dict)
    warnings: list[str] = field(default_factory=list)
    files_scanned: int = 0
    workers: int = 1
    store: object | None = None
    asset_count: int = 0

    def has_asset(self, asset: object) -> bool:
        source_path = str(getattr(asset, "source_path", "") or "")
        if not source_path:
            return False
        key = normalize_asset_key(source_path)
        if self.store is not None and self.store.has_asset(key):
            return True
        if key in self.keys:
            return True
        prefix = _asset_type_prefix(str(getattr(asset, "asset_type", "") or ""))
        if prefix is None or _has_data_dir_prefix(key):
            return False
        if self.store is not None and self.store.has_asset(f"{prefix}{key}"):
            return True
        return f"{prefix}{key}" in self.keys


@dataclass(frozen=True)
class TargetRecordReuse:
    editor_id: str
    signature: str
    form_key: str
    plugin_name: str


@dataclass
class TargetRecordPreflight:
    records: dict[tuple[str, str], TargetRecordReuse] = field(default_factory=dict)
    master_names: list[str] = field(default_factory=list)
    missing_masters: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    def rows_for_native(self) -> list[tuple[str, str, str]]:
        return [
            (entry.editor_id, entry.signature, entry.form_key)
            for entry in self.records.values()
        ]


def _record_key(editor_id: str, signature: str) -> tuple[str, str]:
    return (str(editor_id).casefold(), str(signature).upper())


def _plugin_name_from_form_key(form_key: str) -> str:
    if ":" not in form_key:
        return ""
    return form_key.split(":", 1)[1]


def _legacy_form_key(form_key: str) -> str:
    if ":" not in form_key:
        return form_key
    plugin_name, object_id = form_key.rsplit(":", 1)
    try:
        return f"{int(object_id, 16) & 0x00FFFFFF:06X}:{plugin_name}"
    except ValueError:
        return form_key


def _resolved_path_key(path: str | Path | None) -> str | None:
    if path is None:
        return None
    try:
        return str(Path(path).resolve()).casefold()
    except OSError:
        return str(Path(path)).casefold()


def _handle_path(handle: object) -> Path | None:
    value = getattr(handle, "file_path", None) or getattr(handle, "path", None)
    if value is None:
        return None
    return Path(value)


def _handles_by_resolved_path(handles: Iterable[object]) -> dict[str, object]:
    by_path: dict[str, object] = {}
    for handle in handles or ():
        key = _resolved_path_key(_handle_path(handle))
        if key is None or key in by_path:
            continue
        by_path[key] = handle
    return by_path


_TARGET_ASSET_DIRS = ("Meshes", "Textures", "Materials", "Sound")
_TARGET_ASSET_PREFIXES = ("meshes/", "textures/", "materials/", "sound/")
_ASSET_TYPE_PREFIXES = {
    "nif": "meshes/",
    "animation": "meshes/",
    "animation_dir": "meshes/",
    "behavior": "meshes/",
    "texture": "textures/",
    "material": "materials/",
    "sound": "sound/",
    "audio": "sound/",
}
# FO4's Creation Kit exposes these as built-in actor values even though they are
# not emitted as physical AVIF rows in Fallout4.esm.
_FO4_HARDCODED_AVIF_ROWS = (
    ("AnimationMult", "0002D2"),
    ("WeapReloadSpeedMult", "0002D3"),
    ("ActionPointsRate", "0002D8"),
    ("RadsRate", "0002DB"),
    ("MeleeDamage", "0002DE"),
    ("weaponSpeedMult", "000312"),
    ("PowerGenerated", "00032E"),
    ("Food", "000331"),
    ("Water", "000332"),
    ("Fatigue", "00034F"),
    ("ActionPointsRateMult", "000359"),
    ("ConditionRateMult", "00035A"),
    ("PowerArmorBattery", "00035C"),
    ("ReflectDamage", "00035F"),
    ("Sneak", "00037F"),
)
_FO4_HARDCODED_GLOB_ROWS = (
    ("PlayCredits", "000063"),
)


def _asset_type_prefix(asset_type: str) -> str | None:
    return _ASSET_TYPE_PREFIXES.get(asset_type.casefold())


def _has_data_dir_prefix(key: str) -> bool:
    return key.startswith(_TARGET_ASSET_PREFIXES)


def _add_fo4_hardcoded_actor_values(result: TargetRecordPreflight) -> None:
    if not any(name.casefold() == "fallout4.esm" for name in result.master_names):
        return

    for editor_id, object_id in _FO4_HARDCODED_AVIF_ROWS:
        key = _record_key(editor_id, "AVIF")
        if key in result.records:
            continue
        result.records[key] = TargetRecordReuse(
            editor_id=editor_id,
            signature="AVIF",
            form_key=f"{object_id}:Fallout4.esm",
            plugin_name="Fallout4.esm",
        )
    for editor_id, object_id in _FO4_HARDCODED_GLOB_ROWS:
        key = _record_key(editor_id, "GLOB")
        if key in result.records:
            continue
        result.records[key] = TargetRecordReuse(
            editor_id=editor_id,
            signature="GLOB",
            form_key=f"{object_id}:Fallout4.esm",
            plugin_name="Fallout4.esm",
        )


def normalize_asset_key(path: str | Path, *, root: str | Path | None = None) -> str:
    value = str(path).replace("\\", "/").strip()
    if root is not None:
        root_value = str(root).replace("\\", "/").rstrip("/")
        if value.casefold().startswith(root_value.casefold() + "/"):
            value = value[len(root_value) + 1 :]
    lowered = value.casefold()
    marker = "/data/"
    marker_index = lowered.rfind(marker)
    if marker_index != -1:
        value = value[marker_index + len(marker) :]
    elif lowered.startswith("data/"):
        value = value[5:]
    else:
        for prefix in _TARGET_ASSET_PREFIXES:
            marker = f"/{prefix}"
            marker_index = lowered.rfind(marker)
            if marker_index != -1:
                value = value[marker_index + 1 :]
                break
    return value.lstrip("/").casefold()


def _coerce_target_asset_roots(
    roots: str | Path | Iterable[str | Path | None] | None,
) -> list[Path]:
    if roots is None:
        return []
    if isinstance(roots, (str, Path)):
        candidates: Iterable[str | Path | None] = (roots,)
    else:
        candidates = roots

    paths: list[Path] = []
    seen: set[str] = set()
    for candidate in candidates:
        if not candidate:
            continue
        path = Path(candidate)
        try:
            key = str(path.resolve()).casefold()
        except OSError:
            key = str(path).casefold()
        if key in seen:
            continue
        seen.add(key)
        paths.append(path)
    return paths


def _worker_count(workers: int | None) -> int:
    if workers is None:
        return 1
    try:
        return max(1, int(workers))
    except (TypeError, ValueError):
        return 1


def _iter_file_paths(root: str) -> Iterable[str]:
    stack = [root]
    while stack:
        current = stack.pop()
        try:
            with os.scandir(current) as entries:
                for entry in entries:
                    try:
                        if entry.is_dir(follow_symlinks=False):
                            stack.append(entry.path)
                        elif entry.is_file(follow_symlinks=False):
                            yield entry.path
                    except OSError:
                        continue
        except OSError:
            continue


def _scan_asset_paths(root: Path, scan_root: str) -> list[tuple[str, str]]:
    return [
        (normalize_asset_key(path, root=root), path)
        for path in _iter_file_paths(scan_root)
    ]


def _asset_scan_tasks(base: Path) -> tuple[list[str], list[str]]:
    direct_files: list[str] = []
    subdirs: list[str] = []
    try:
        with os.scandir(base) as entries:
            for entry in entries:
                try:
                    if entry.is_dir(follow_symlinks=False):
                        subdirs.append(entry.path)
                    elif entry.is_file(follow_symlinks=False):
                        direct_files.append(entry.path)
                except OSError:
                    continue
    except OSError:
        return [str(base)], []
    return subdirs, direct_files


def _scan_asset_base(
    root: Path,
    base: Path,
    *,
    workers: int,
) -> list[tuple[str, str]]:
    scan_roots, direct_files = _asset_scan_tasks(base)
    rows = [(normalize_asset_key(path, root=root), path) for path in direct_files]
    if not scan_roots:
        return rows

    if workers <= 1 or len(scan_roots) == 1:
        for scan_root in scan_roots:
            rows.extend(_scan_asset_paths(root, scan_root))
        return rows

    max_workers = min(workers, len(scan_roots))
    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        futures = [
            executor.submit(_scan_asset_paths, root, scan_root)
            for scan_root in scan_roots
        ]
        for future in futures:
            rows.extend(future.result())
    return rows


def build_target_asset_index(
    target_asset_roots: str | Path | Iterable[str | Path | None] | None,
    *,
    workers: int | None = None,
    store: object | None = None,
) -> TargetAssetIndex:
    index = TargetAssetIndex(store=store)
    index.workers = _worker_count(workers)
    if store is not None:
        index.asset_count = int(store.asset_count)
        index.files_scanned = index.asset_count
        index.warnings.extend(getattr(store, "warnings", ()) or ())
        return index
    roots = _coerce_target_asset_roots(target_asset_roots)
    if not roots:
        index.warnings.append("target asset preflight: target asset roots are not configured")
        return index

    for root in roots:
        if not root.is_dir():
            index.warnings.append(f"target asset preflight: missing directory {root}")
            continue

        try:
            top_level_dirs = [path for path in root.iterdir() if path.is_dir()]
        except OSError as exc:
            index.warnings.append(f"target asset preflight: cannot read directory {root}: {exc}")
            continue

        for dirname in _TARGET_ASSET_DIRS:
            matches = [
                path for path in top_level_dirs if path.name.casefold() == dirname.casefold()
            ]
            if not matches:
                continue
            base = matches[0]
            for duplicate in matches[1:]:
                index.warnings.append(
                    f"target asset preflight: duplicate top-level asset directory "
                    f"{base.name}: kept {base}, skipped {duplicate}"
                )
            for key, path in _scan_asset_base(root, base, workers=index.workers):
                index.files_scanned += 1
                previous = index.owners.get(key)
                if previous is not None:
                    index.warnings.append(
                        f"target asset preflight: duplicate normalized asset {key}: "
                        f"kept {previous}, skipped {path}"
                    )
                    continue
                index.keys.add(key)
                index.owners[key] = str(path)

    index.asset_count = len(index.keys)

    return index


def _collect_eid_rows_native(plugin: object, game: str) -> list[dict]:
    del game
    return [
        {
            "editor_id": editor_id,
            "signature": signature,
            "form_key": _legacy_form_key(form_key),
        }
        for form_key, editor_id, signature, _object_id, _raw_form_id in plugin.record_index_rows()
    ]


def build_target_record_preflight(
    target_game: str,
    *,
    target_master_paths: Iterable[str | Path] = (),
    target_data_dir: str | Path | None = None,
    target_extracted_dir: str | Path | None = None,
    target_master_handles: Iterable[object] = (),
    collect_eid_rows: RecordRowsCollector | None = None,
) -> TargetRecordPreflight:
    paths, missing = resolve_official_target_master_paths(
        target_game,
        target_master_paths=target_master_paths,
        target_data_dir=target_data_dir,
        target_extracted_dir=target_extracted_dir,
    )
    collector = collect_eid_rows or _collect_eid_rows_native
    result = TargetRecordPreflight(missing_masters=list(missing))
    open_handles_by_path = _handles_by_resolved_path(target_master_handles)

    for path in paths:
        result.master_names.append(path.name)
        plugin = open_handles_by_path.get(_resolved_path_key(path))
        close_plugin = plugin is None
        if plugin is None:
            # Index-only load: the collector reads EID rows from the CoreSection
            # (formid/eid index), never the record tree, so the multi-GB eager
            # master tree never has to be built here.
            plugin = Plugin.load(path, game=target_game, lazy_index=True)
        try:
            rows = collector(plugin, target_game)
        finally:
            if close_plugin:
                plugin.close()

        for row in rows:
            editor_id = str(row.get("editor_id") or "")
            signature = str(row.get("signature") or "").upper()
            form_key = str(row.get("form_key") or "")
            if not editor_id or len(signature) != 4 or not form_key:
                continue
            key = _record_key(editor_id, signature)
            if key in result.records:
                kept = result.records[key]
                if kept.form_key == form_key:
                    continue
                result.warnings.append(
                    "duplicate target EditorID/signature "
                    f"{editor_id}/{signature}: kept {kept.form_key}, skipped {form_key}"
                )
                continue
            result.records[key] = TargetRecordReuse(
                editor_id=editor_id,
                signature=signature,
                form_key=form_key,
                plugin_name=_plugin_name_from_form_key(form_key),
            )

    if str(target_game).casefold() == "fo4":
        _add_fo4_hardcoded_actor_values(result)

    return result
