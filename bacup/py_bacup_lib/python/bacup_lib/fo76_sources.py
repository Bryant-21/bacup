"""Pure FO76 source path resolution helpers."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


@dataclass(frozen=True)
class PathResolution:
    path: Path | None
    candidates: tuple[Path, ...]


@dataclass(frozen=True)
class PathListResolution:
    paths: tuple[Path, ...]
    candidates: tuple[Path, ...]


_BTD_WORLDSPACE_EDITOR_IDS = {
    "appalachia": "APPALACHIA",
    "exm1pittworldspace": "EXM1PittWorldspace",
}
_SOURCE_WORLDSPACE_DIR_NAMES = {
    "appalachia": "APPALACHIA - 25DA15_SeventySix.esm",
    "exm1pittworldspace": "EXM1PittWorldspace - 635F96_SeventySix.esm",
}


def _as_path(value: str | Path | None) -> Path | None:
    if value is None:
        return None
    path = Path(value)
    return path if str(path) else None


def _unique_existing_order(paths: Iterable[Path]) -> tuple[Path, ...]:
    seen: set[str] = set()
    result: list[Path] = []
    for path in paths:
        key = str(path)
        if key in seen:
            continue
        seen.add(key)
        result.append(path)
    return tuple(result)


def resolve_fo76_plugin(
    plugin_name: str,
    *,
    data_dir: str | Path | None = None,
    extracted_dir: str | Path | None = None,
) -> PathResolution:
    candidates: list[Path] = []
    data_root = _as_path(data_dir)
    extracted_root = _as_path(extracted_dir)
    if data_root is not None:
        candidates.append(data_root / plugin_name)
    if extracted_root is not None:
        candidates.append(extracted_root / plugin_name)
    checked = _unique_existing_order(candidates)
    return PathResolution(
        path=next((path for path in checked if path.is_file()), None),
        candidates=checked,
    )


def resolve_appalachia_btd(
    *,
    data_dir: str | Path | None = None,
    extracted_dir: str | Path | None = None,
    source_data_dir: str | Path | None = None,
) -> PathResolution:
    roots = _unique_existing_order(
        path
        for path in (
            _as_path(source_data_dir),
            _as_path(data_dir),
            _as_path(extracted_dir),
        )
        if path is not None
    )
    candidates: list[Path] = []
    for root in roots:
        candidates.extend(
            [
                root / "Terrain" / "Appalachia.btd",
                root / "terrain" / "appalachia.btd",
                root / "Terrain" / "APPALACHIA.btd",
            ]
        )
    checked = _unique_existing_order(candidates)
    return PathResolution(
        path=next((path for path in checked if path.is_file()), None),
        candidates=checked,
    )


def fo76_worldspace_editor_id_from_btd(path: str | Path) -> str:
    btd_path = Path(path)
    return _BTD_WORLDSPACE_EDITOR_IDS.get(btd_path.stem.lower(), btd_path.stem)


def resolve_fo76_btd_paths(
    *,
    data_dir: str | Path | None = None,
    extracted_dir: str | Path | None = None,
    source_data_dir: str | Path | None = None,
) -> PathListResolution:
    roots = _unique_existing_order(
        path
        for path in (
            _as_path(source_data_dir),
            _as_path(extracted_dir),
            _as_path(data_dir),
        )
        if path is not None
    )
    candidates: list[Path] = []
    paths: list[Path] = []
    seen_worldspaces: set[str] = set()
    for root in roots:
        for terrain_dir in (root / "Terrain", root / "terrain"):
            candidates.append(terrain_dir / "*.btd")
            if not terrain_dir.is_dir():
                continue
            for path in sorted(
                terrain_dir.glob("*.btd"),
                key=lambda item: item.stem.lower(),
            ):
                if not path.is_file():
                    continue
                key = fo76_worldspace_editor_id_from_btd(path).lower()
                if key in seen_worldspaces:
                    continue
                seen_worldspaces.add(key)
                paths.append(path)
    return PathListResolution(paths=tuple(paths), candidates=tuple(candidates))


def resolve_fo76_source_worldspace_authoring_dir(
    worldspace_editor_id: str,
    *,
    repo_root: str | Path,
) -> Path | None:
    dirname = _SOURCE_WORLDSPACE_DIR_NAMES.get(worldspace_editor_id.lower())
    if dirname is None:
        return None
    return (
        Path(repo_root)
        / "data"
        / "fo76_esm_yaml"
        / "SeventySix"
        / "records"
        / "WRLD"
        / dirname
    )


def require_resolved(resolution: PathResolution, *, label: str) -> Path:
    if resolution.path is not None:
        return resolution.path
    checked = "\n".join(f"  - {path}" for path in resolution.candidates)
    suffix = f"; checked:\n{checked}" if checked else "; no candidate paths were configured"
    raise FileNotFoundError(f"{label} not found{suffix}")


def require_any_resolved(
    resolution: PathListResolution,
    *,
    label: str,
) -> tuple[Path, ...]:
    if resolution.paths:
        return resolution.paths
    checked = "\n".join(f"  - {path}" for path in resolution.candidates)
    suffix = f"; checked:\n{checked}" if checked else "; no candidate paths were configured"
    raise FileNotFoundError(f"{label} not found{suffix}")
