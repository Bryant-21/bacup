"""Helpers for locating native embedded conversion data files."""
from __future__ import annotations

from functools import lru_cache
import os
from pathlib import Path
import sys


_SENTINEL_MAP = "fo76_to_fo4.yaml"
_SENTINEL_FACE_RESOURCE = "fnv_to_fo4_correspondence_male.npz"


def _dedupe(paths: list[Path]) -> list[Path]:
    seen: set[str] = set()
    out: list[Path] = []
    for path in paths:
        key = os.path.normcase(os.path.abspath(path))
        if key in seen:
            continue
        seen.add(key)
        out.append(path)
    return out


def _candidate_translation_map_dirs(
    module_file: str | Path | None = None,
    meipass: str | Path | None = None,
) -> list[Path]:
    module_path = Path(module_file or __file__).resolve()
    candidates: list[Path] = []

    env_override = os.environ.get("CREATION_LIB_TRANSLATION_MAPS_DIR", "").strip()
    if env_override:
        candidates.append(Path(env_override))

    bacup_lib_root = module_path.parent
    candidates.append(bacup_lib_root / "resources" / "conversion" / "translation_maps")

    bundle_root_value = meipass or getattr(sys, "_MEIPASS", "")
    bundle_root = Path(bundle_root_value) if bundle_root_value else None
    if bundle_root is not None:
        candidates.extend(
            [
                bundle_root
                / "bacup_lib"
                / "resources"
                / "conversion"
                / "translation_maps",
                bundle_root
                / "native"
                / "conversion"
                / "src"
                / "embedded"
                / "translation_maps",
                bundle_root
                / "bacup"
                / "py_bacup_lib"
                / "native"
                / "conversion"
                / "src"
                / "embedded"
                / "translation_maps",
            ]
        )

    for parent in module_path.parents:
        candidates.append(
            parent
            / "native"
            / "conversion"
            / "src"
            / "embedded"
            / "translation_maps"
        )
        candidates.append(
            parent
            / "bacup"
            / "py_bacup_lib"
            / "native"
            / "conversion"
            / "src"
            / "embedded"
            / "translation_maps"
        )

    return _dedupe(candidates)


@lru_cache(maxsize=1)
def native_translation_maps_dir() -> Path:
    candidates = _candidate_translation_map_dirs()
    for maps_dir in candidates:
        if maps_dir.is_dir() and (maps_dir / _SENTINEL_MAP).is_file():
            return maps_dir
    checked = "\n  - ".join(str(path) for path in candidates)
    raise FileNotFoundError(
        f"Native translation maps directory not found. Checked:\n  - {checked}"
    )


def _candidate_face_resource_dirs(
    module_file: str | Path | None = None,
    meipass: str | Path | None = None,
) -> list[Path]:
    module_path = Path(module_file or __file__).resolve()
    relative = Path("native/conversion/src/phase/resources/face")
    candidates: list[Path] = []

    bundle_root_value = meipass or getattr(sys, "_MEIPASS", "")
    if bundle_root_value:
        candidates.append(Path(bundle_root_value) / relative)

    for parent in module_path.parents:
        candidates.append(parent / relative)
        candidates.append(parent / "bacup" / "py_bacup_lib" / relative)

    return _dedupe(candidates)


@lru_cache(maxsize=1)
def native_face_resources_dir() -> Path:
    candidates = _candidate_face_resource_dirs()
    for resource_dir in candidates:
        if (resource_dir / _SENTINEL_FACE_RESOURCE).is_file():
            return resource_dir
    checked = "\n  - ".join(str(path) for path in candidates)
    raise FileNotFoundError(
        f"Native face resources directory not found. Checked:\n  - {checked}"
    )
