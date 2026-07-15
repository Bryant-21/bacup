from __future__ import annotations

import io
import logging
import time
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

from creation_lib.material_tools.bgsm_bin import read_bgsm


LOG = logging.getLogger(__name__)
_PLUGIN_EXTENSIONS = {".esm", ".esp", ".esl"}


@dataclass(frozen=True)
class RunInvariantResult:
    ok: bool
    failures: list[str]


def check_run_invariants(
    out_dir: str | Path,
    *,
    expected_plugins: Iterable[str],
    source_prefix: str,
    allowed_asset_prefixes: Iterable[str] = (),
    max_workers: int = 1,
) -> RunInvariantResult:
    root = Path(out_dir)
    asset_root = root / "data" if (root / "data").is_dir() else root
    failures: list[str] = []

    actual_plugins = sorted(
        path.name
        for path in root.glob("*")
        if path.is_file() and path.suffix.lower() in _PLUGIN_EXTENSIONS
        and not path.name.startswith(".")
    )
    expected_plugin_list = sorted(str(plugin) for plugin in expected_plugins)
    if actual_plugins != expected_plugin_list:
        failures.append(
            f"plugin set mismatch: {actual_plugins} vs {expected_plugin_list}"
        )

    max_workers = max(1, int(max_workers))
    asset_started = time.perf_counter()
    kind_roots = [
        asset_root / asset_kind
        for asset_kind in ("Meshes", "Materials", "Textures", "Sound")
        if (asset_root / asset_kind).is_dir()
    ]
    if max_workers <= 1 or len(kind_roots) <= 1:
        asset_results = [
            _scan_asset_prefix(kind_root, source_prefix, allowed_asset_prefixes)
            for kind_root in kind_roots
        ]
    else:
        with ThreadPoolExecutor(
            max_workers=min(max_workers, len(kind_roots)),
            thread_name_prefix="invariant-assets",
        ) as executor:
            asset_results = list(
                executor.map(
                    lambda kind_root: _scan_asset_prefix(
                        kind_root,
                        source_prefix,
                        allowed_asset_prefixes,
                    ),
                    kind_roots,
                )
            )
    asset_files = sum(file_count for file_count, _ in asset_results)
    for _, asset_failures in asset_results:
        failures.extend(asset_failures)
    LOG.info(
        "invariant loose_asset_scan: root=%s files=%d workers=%d elapsed=%.3fs",
        asset_root,
        asset_files,
        max_workers,
        time.perf_counter() - asset_started,
    )

    materials_root = asset_root / "Materials"
    if materials_root.is_dir():
        bgsm_started = time.perf_counter()
        bgsm_paths = [path for path in materials_root.rglob("*.bgsm") if path.is_file()]
        if max_workers <= 1 or len(bgsm_paths) <= 1:
            bgsm_failures = [_check_bgsm(path) for path in bgsm_paths]
        else:
            with ThreadPoolExecutor(max_workers=max_workers, thread_name_prefix="invariant-bgsm") as executor:
                bgsm_failures = list(executor.map(_check_bgsm, bgsm_paths))
        failures.extend(failure for failure in bgsm_failures if failure is not None)
        LOG.info(
            "invariant bgsm_scan: root=%s files=%d workers=%d elapsed=%.3fs",
            materials_root,
            len(bgsm_paths),
            max_workers,
            time.perf_counter() - bgsm_started,
        )

    return RunInvariantResult(ok=not failures, failures=failures)


def _scan_asset_prefix(
    kind_root: Path,
    source_prefix: str,
    allowed_asset_prefixes: Iterable[str] = (),
) -> tuple[int, list[str]]:
    failures: list[str] = []
    file_count = 0
    allowed_prefixes = set(allowed_asset_prefixes)
    for path in kind_root.rglob("*"):
        if not path.is_file():
            continue
        file_count += 1
        rel_parts = path.relative_to(kind_root).parts
        if (
            rel_parts
            and rel_parts[0] not in allowed_prefixes
            and rel_parts[0].lower() == source_prefix.lower()
        ):
            failures.append(f"asset inside source prefix: {path}")
    return file_count, failures


def _check_bgsm(path: Path) -> str | None:
    with path.open("rb") as handle:
        try:
            bgsm = read_bgsm(io.BufferedReader(handle))
        except Exception as exc:
            return f"invalid BGSM: {path}: {exc}"
    if not bgsm.CastShadows:
        return f"BGSM with bCastShadows=False: {path}"
    return None
