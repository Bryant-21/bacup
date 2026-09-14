"""Materialize the Papyrus type universe from the target game's own `.pex`.

Fallout 4 ships compiled `.pex` and no `.psc`; the sources are a Creation Kit
deliverable. Rather than requiring users to install the CK, synthesize
header-only `.psc` from the `.pex` their extraction already produced and hand
that directory to the compiler as an ordinary import root.

Staleness is content-addressed: the stamp covers the emitter, its manifest, and
every source `.pex`'s size and mtime, so editing the emitter invalidates every
header without anyone remembering to bump a version.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Callable

_MANIFEST_NAME = ".header_cache.json"
_ANCHOR_TYPES = ("scriptobject", "form", "objectreference")


def _emitter_stamp() -> str:
    """Fingerprint the emitter and its manifest so changes invalidate output."""
    from creation_lib.pex import headers as headers_module

    digest = hashlib.sha256()
    for path in (
        Path(headers_module.__file__),
        headers_module.DATA_DIR / "fo4_papyrus_defaults.json",
    ):
        try:
            digest.update(path.read_bytes())
        except OSError:
            digest.update(b"<missing>")
    return digest.hexdigest()[:16]


def _source_fingerprint(pex_paths: list[Path]) -> dict[str, list[int]]:
    fingerprint: dict[str, list[int]] = {}
    for path in pex_paths:
        try:
            stat = path.stat()
        except OSError:
            continue
        fingerprint[path.name.lower()] = [stat.st_size, int(stat.st_mtime)]
    return fingerprint


def find_pex_corpus(*roots: Path | None) -> Path | None:
    """First root holding the anchor types the whole type graph resolves through.

    Anchor presence rather than directory existence: an empty or partial
    `Scripts` directory satisfies `is_dir()` and still compiles nothing.
    """
    for root in roots:
        if root is None:
            continue
        candidate = Path(root)
        if not candidate.is_dir():
            continue
        names = {
            path.stem.lower()
            for path in candidate.glob("*.pex")
        }
        if all(anchor in names for anchor in _ANCHOR_TYPES):
            return candidate
    return None


def materialize_pex_corpus(store: object) -> Path | None:
    """Unpack the target game's `.pex` out of its archives, and return the root.

    Fallout 4 keeps its 7875 compiled scripts inside `Fallout4 - Misc.ba2`, so a
    player who never extracted the game has none of them loose on disk even
    though the game reads them fine. Requiring extraction just to build the type
    universe turns a working install into a blocked conversion.
    """
    if store is None:
        return None
    try:
        assets = store.list_assets(prefix="scripts/", suffix=".pex")
    except Exception:
        return None
    stems = {Path(asset).stem.lower() for asset in assets}
    if not all(anchor in stems for anchor in _ANCHOR_TYPES):
        return None

    # Pair one asset with its materialized path to locate the corpus root: the
    # cache mirrors the asset layout, but not necessarily its casing.
    sample = store.materialize(assets[0])
    if sample is None:
        return None
    store.materialize_many(assets)
    depth = len(Path(assets[0]).parts) - 1
    root = Path(sample)
    return root.parents[depth - 1] if depth else root.parent


def _write_flags_file(out_root: Path) -> None:
    """Place the user-flag definitions beside the headers.

    `Institute_Papyrus_Flags.flg` maps flag names to bits and also ships only
    with the Creation Kit, so the import root has to carry its own copy.
    """
    from creation_lib.pex import headers as headers_module

    source = headers_module.DATA_DIR / headers_module.FLAGS_FILE_NAME
    if not source.is_file():
        return
    (out_root / source.name).write_bytes(source.read_bytes())


def flags_file(out_root: Path) -> Path | None:
    from creation_lib.pex import headers as headers_module

    candidate = Path(out_root) / headers_module.FLAGS_FILE_NAME
    return candidate if candidate.is_file() else None


def ensure_headers(
    pex_root: Path,
    out_root: Path,
    *,
    emit_log: Callable[[str, str], None] | None = None,
) -> Path:
    """Generate header-only `.psc` for every `.pex`, skipping unchanged input."""
    from creation_lib.pex.headers import emit_headers_for_file, load_manifest

    pex_root = Path(pex_root)
    out_root = Path(out_root)
    out_root.mkdir(parents=True, exist_ok=True)

    pex_paths = sorted(pex_root.rglob("*.pex"))
    stamp = _emitter_stamp()
    fingerprint = _source_fingerprint(pex_paths)
    manifest_path = out_root / _MANIFEST_NAME

    try:
        cached = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        cached = {}

    if cached.get("emitter") == stamp and cached.get("sources") == fingerprint:
        if emit_log:
            emit_log("INFO", f"[Scripts] type universe up to date ({len(pex_paths)} pex)")
        return out_root

    _write_flags_file(out_root)
    api_manifest = load_manifest()
    written = 0
    failed = 0
    for pex_path in pex_paths:
        try:
            for script_name, text in emit_headers_for_file(pex_path, api_manifest):
                dest = out_root.joinpath(*script_name.split(":")).with_suffix(".psc")
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_text(text, encoding="utf-8")
                written += 1
        except Exception as exc:  # one unreadable pex must not sink the run
            failed += 1
            if emit_log and failed <= 10:
                emit_log("WARN", f"[Scripts] could not read {pex_path.name}: {exc}")

    manifest_path.write_text(
        json.dumps({"emitter": stamp, "sources": fingerprint}),
        encoding="utf-8",
    )
    if emit_log:
        emit_log(
            "INFO",
            f"[Scripts] built type universe from {len(pex_paths)} pex: "
            f"{written} header(s), {failed} unreadable",
        )
    return out_root
