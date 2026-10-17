"""Path and logging adapters for native Papyrus header preparation."""
from __future__ import annotations

from pathlib import Path
from typing import Callable

from bacup_lib.native_runtime import load_native_module


def find_pex_corpus(*roots: Path | None) -> Path | None:
    path = load_native_module().script_headers_find_corpus(
        [str(root) for root in roots if root is not None]
    )
    return Path(path) if path else None


def materialize_pex_corpus(store: object) -> Path | None:
    if store is None:
        return None
    try:
        assets = store.list_assets(prefix="scripts/", suffix=".pex")
    except Exception:
        return None
    if not load_native_module().script_headers_has_anchors(assets):
        return None
    sample = store.materialize(assets[0])
    if sample is None:
        return None
    store.materialize_many(assets)
    depth = len(Path(assets[0]).parts) - 1
    root = Path(sample)
    return root.parents[depth - 1] if depth else root.parent


def flags_file(out_root: Path) -> Path | None:
    from creation_lib.pex.headers import FLAGS_FILE_NAME

    candidate = Path(out_root) / FLAGS_FILE_NAME
    return candidate if candidate.is_file() else None


def ensure_headers(
    pex_root: Path,
    out_root: Path,
    *,
    emit_log: Callable[[str, str], None] | None = None,
) -> Path:
    for level, message in load_native_module().script_headers_ensure(str(pex_root), str(out_root)):
        if emit_log is not None:
            emit_log(level, message)
    return Path(out_root)
