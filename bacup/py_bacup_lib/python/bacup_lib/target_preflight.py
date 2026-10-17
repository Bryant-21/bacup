"""Native preflight adapters for target records and assets."""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

from bacup_lib.native_runtime import load_native_module
from bacup_lib.target_masters import resolve_official_target_master_paths


class TargetAssetIndex:
    def __init__(self, native, *, workers: int, store=None) -> None:
        self._native = native
        self.store = store
        self.workers = workers
        self.asset_count = int(native.asset_count)
        self.files_scanned = (
            self.asset_count if store is not None else int(native.files_scanned)
        )
        self.warnings = list(native.warnings)

    @property
    def keys(self) -> set[str]:
        return set() if self.store is not None else self._native.keys

    @property
    def owners(self) -> dict[str, str]:
        return {} if self.store is not None else self._native.owners

    def has_asset(self, asset: object) -> bool:
        return bool(self._native.has_asset_for_type(
            str(getattr(asset, "source_path", "") or ""),
            str(getattr(asset, "asset_type", "") or ""),
        ))


@dataclass
class TargetRecordPreflight:
    rows: list[tuple[str, str, str]] = field(default_factory=list)
    master_names: list[str] = field(default_factory=list)
    missing_masters: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    def rows_for_native(self) -> list[tuple[str, str, str]]:
        return self.rows


def build_target_asset_index(
    target_asset_roots: str | Path | Iterable[str | Path | None] | None,
    *,
    workers: int | None = None,
    store: object | None = None,
) -> TargetAssetIndex:
    try:
        workers = max(1, int(workers)) if workers is not None else 1
    except (TypeError, ValueError):
        workers = 1
    if store is not None:
        return TargetAssetIndex(store, workers=workers, store=store)
    if isinstance(target_asset_roots, (str, Path)):
        target_asset_roots = (target_asset_roots,)
    native = load_native_module().conversion_target_asset_index(
        [str(root) for root in (target_asset_roots or ()) if root], workers
    )
    return TargetAssetIndex(native, workers=workers)


def build_target_record_preflight(
    target_game: str,
    *,
    target_master_paths: Iterable[str | Path] = (),
    target_data_dir: str | Path | None = None,
    target_extracted_dir: str | Path | None = None,
) -> TargetRecordPreflight:
    paths, missing = resolve_official_target_master_paths(
        target_game,
        target_master_paths=target_master_paths,
        target_data_dir=target_data_dir,
        target_extracted_dir=target_extracted_dir,
    )
    rows, warnings = load_native_module().conversion_target_record_preflight(
        target_game, [str(path) for path in paths]
    )
    return TargetRecordPreflight(
        rows=rows,
        master_names=[path.name for path in paths],
        missing_masters=list(missing),
        warnings=warnings,
    )
