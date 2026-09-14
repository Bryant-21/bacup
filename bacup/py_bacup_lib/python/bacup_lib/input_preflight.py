"""Pre-conversion scan of FO76 extracted inputs and the FO4 target corpus.

Catches incomplete extraction (e.g. missing APPALACHIA terrain-LOD BTOs) BEFORE
a run starts, instead of failing deep in LOD generation. Pure and path-explicit:
no os.environ, no ToolkitSettings.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

from bacup_lib.target_assets import default_target_asset_catalog
from creation_lib.db.native_runtime import Database


@dataclass(frozen=True)
class MissingInput:
    label: str
    checked_path: str
    fix_hint: str


@dataclass
class InputPreflightReport:
    required_missing: list[MissingInput] = field(default_factory=list)
    optional_missing: list[MissingInput] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return not self.required_missing


def _find_child_ci(base: Path, name: str) -> Path | None:
    if not base.is_dir():
        return None
    lowered = name.casefold()
    try:
        for entry in base.iterdir():
            if entry.name.casefold() == lowered:
                return entry
    except OSError:
        return None
    return None


def _resolve_ci_path(base: Path, *parts: str) -> Path:
    """Case-insensitive descent; returns the literal join for the first part
    that does not exist so the reported path is human-readable."""
    current = base
    for idx, part in enumerate(parts):
        child = _find_child_ci(current, part)
        if child is None:
            return current.joinpath(*parts[idx:])
        current = child
    return current


def _bto_objects_dir(fo76_extracted: Path, world: str) -> Path:
    return _resolve_ci_path(fo76_extracted, "Meshes", "Terrain", world, "Objects")


def _has_bto(objects_dir: Path) -> bool:
    if not objects_dir.is_dir():
        return False
    try:
        return any(
            entry.is_file() and entry.name.casefold().endswith(".bto")
            for entry in objects_dir.iterdir()
        )
    except OSError:
        return False


# Compiling any converted script needs the type of everything it touches, and
# those resolve transitively through this chain. If they are absent the compiler
# cannot type a single `Extends`, so every script fails at once.
_PAPYRUS_ANCHOR_TYPES = ("ScriptObject", "Form", "ObjectReference")


def _missing_papyrus_anchors(
    *roots: Path | None, fo4_data: Path | None = None
) -> list[str]:
    """Anchor types absent from every candidate FO4 script corpus.

    The type universe is built from the game's compiled `.pex`, so this checks
    for those rather than the `.psc` that only a Creation Kit install provides.
    Anchor presence rather than directory existence is the test: an empty or
    partial `Scripts` directory satisfies `is_dir()` and still compiles nothing.
    """
    found: set[str] = set()
    for root in roots:
        if root is None:
            continue
        scripts_dir = _resolve_ci_path(Path(root), "Scripts")
        if not scripts_dir.is_dir():
            continue
        try:
            names = {entry.stem.casefold() for entry in scripts_dir.glob("*.pex")}
        except OSError:
            continue
        found.update(
            anchor for anchor in _PAPYRUS_ANCHOR_TYPES if anchor.casefold() in names
        )
    missing = [anchor for anchor in _PAPYRUS_ANCHOR_TYPES if anchor not in found]
    if not missing:
        return []
    if _anchors_in_archives(fo4_data, missing):
        return []
    # With a shipped corpus the conversion compiles without reading the game, so an
    # install with no minable types doesn't block the run.
    from creation_lib.pex.corpus import bundled_corpus_archive

    if bundled_corpus_archive("fo4") is not None:
        return []
    return missing


def _anchors_in_archives(fo4_data: Path | None, anchors: list[str]) -> bool:
    """Whether the archives hold the anchors the loose directories lack.

    Fallout 4 keeps its compiled scripts inside `Fallout4 - Misc.ba2`. A player
    who never extracted the game has none loose, which says nothing about
    whether the conversion can read them — it opens the archives either way.
    """
    if fo4_data is None or not Path(fo4_data).is_dir():
        return False
    try:
        from bacup_lib.target_assets import TargetAssetStore

        store = TargetAssetStore(target_data_dir=fo4_data)
        return all(
            store.has_asset(f"scripts/{anchor.lower()}.pex") for anchor in anchors
        )
    except Exception:
        return False


def scan_conversion_inputs(
    paths: object,
    worldspaces: Iterable[str] = ("Appalachia",),
) -> InputPreflightReport:
    report = InputPreflightReport()
    fo76_data = Path(getattr(paths, "source_data_dir", "") or "")
    fo76_ext = Path(getattr(paths, "source_extracted_dir", "") or "")
    fo4_data = Path(getattr(paths, "target_data_dir", "") or "")
    catalog = Path(
        getattr(paths, "target_asset_catalog_path", None)
        or default_target_asset_catalog()
    )

    source_plugin = _resolve_ci_path(fo76_data, "SeventySix.esm")
    if not source_plugin.is_file():
        report.required_missing.append(
            MissingInput(
                "FO76 source plugin",
                str(source_plugin),
                "Point the Fallout 76 install path at a valid Fallout 76 Data folder "
                "containing SeventySix.esm.",
            )
        )

    if not fo76_ext.is_dir():
        report.required_missing.append(
            MissingInput(
                "FO76 extracted directory",
                str(fo76_ext),
                "Re-run Setup / Re-extract Game Data and select the extracted "
                "Fallout 76 directory before converting.",
            )
        )
    else:
        for world in worldspaces:
            objects_dir = _bto_objects_dir(fo76_ext, world)
            if not _has_bto(objects_dir):
                report.required_missing.append(
                    MissingInput(
                        f"FO76 terrain-LOD BTOs ({world})",
                        str(objects_dir),
                        "Re-extract the Fallout 76 terrain-LOD archives into your FO76 "
                        f"extracted dir (expected *.bto under Meshes/Terrain/{world}/Objects).",
                    )
                )

    if not fo4_data.is_dir():
        report.required_missing.append(
            MissingInput(
                "FO4 Data directory",
                str(fo4_data),
                "Point the Fallout 4 install path at a valid Data directory containing "
                "Fallout4.esm and the official BA2 archives.",
            )
        )
    else:
        target_master = _resolve_ci_path(fo4_data, "Fallout4.esm")
        if not target_master.is_file():
            report.required_missing.append(
                MissingInput(
                    "FO4 base master",
                    str(target_master),
                    "Verify the Fallout 4 Data directory contains Fallout4.esm.",
                )
            )

        fo4_ext = Path(getattr(paths, "target_extracted_dir", "") or "")
        missing_anchors = _missing_papyrus_anchors(
            fo4_ext if str(fo4_ext) else None, fo4_data, fo4_data=fo4_data
        )
        if missing_anchors:
            report.required_missing.append(
                MissingInput(
                    "FO4 Papyrus script corpus "
                    f"(missing {', '.join(missing_anchors)})",
                    str(_resolve_ci_path(fo4_ext or fo4_data, "Scripts")),
                    "Verify the Fallout 4 install: neither its loose Scripts "
                    "directory nor its archives hold the base script types. The "
                    "converter builds the Papyrus type universe from those .pex; "
                    "without them no converted script compiles and its records "
                    "lose their script bindings.",
                )
            )
    if catalog.is_file() and fo4_data.is_dir():
        try:
            with Database.open(str(catalog), mode="ro") as db:
                rows = db.query_all(
                    "SELECT name, content_pack, required FROM archives ORDER BY priority"
                )
        except (RuntimeError, OSError):
            pass
        else:
            installed = {
                entry.name.casefold(): entry
                for entry in fo4_data.iterdir()
                if entry.is_file() and entry.suffix.casefold() == ".ba2"
            }
            for row in rows:
                archive_name = row["name"]
                content_pack = row["content_pack"]
                required = row["required"]
                if str(archive_name).casefold() in installed:
                    continue
                missing = MissingInput(
                    f"FO4 archive ({content_pack})",
                    str(fo4_data / str(archive_name)),
                    "Verify the corresponding Fallout 4 base game or DLC files.",
                )
                if required:
                    report.required_missing.append(missing)
                else:
                    report.optional_missing.append(missing)

    return report
