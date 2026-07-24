"""Target master plugin discovery for conversion workflows."""
from __future__ import annotations

import logging
from collections.abc import Iterable
from pathlib import Path

_log = logging.getLogger("conversion.target_masters")

_OFFICIAL_TARGET_MASTER_NAME_OVERRIDES = {
    "fo4": [
        "Fallout4.esm",
        "DLCRobot.esm",
        "DLCworkshop01.esm",
        "DLCCoast.esm",
        "DLCworkshop02.esm",
        "DLCworkshop03.esm",
        "DLCNukaWorld.esm",
    ],
}


def resolve_target_master_paths(
    target_game: str,
    *,
    target_master_paths: Iterable[str | Path] = (),
    target_data_dir: str | Path | None = None,
    target_extracted_dir: str | Path | None = None,
) -> list[Path]:
    """Resolve explicit target masters plus the target game's base ESM."""
    explicit_paths: list[Path] = []
    search_dirs: list[Path] = []
    for value in target_master_paths or ():
        path = Path(value)
        if path.is_dir():
            search_dirs.append(path)
        elif path.is_file():
            explicit_paths.append(path)

    for value in (target_data_dir, target_extracted_dir):
        if value:
            path = Path(value)
            if path.is_dir():
                search_dirs.append(path)

    paths: list[Path] = []
    seen: set[str] = set()

    def add(path: Path) -> None:
        key = str(path.resolve()).casefold()
        if key in seen:
            return
        seen.add(key)
        paths.append(path)

    for path in explicit_paths:
        add(path)

    base_master = _target_base_master_name(target_game)
    if base_master:
        found = _find_named_file(base_master, search_dirs)
        if found is not None:
            add(found)

    return paths


def official_target_master_names(target_game: str) -> list[str]:
    """Return official target-game masters in stable load-order preference."""
    canonical_game = str(target_game).lower()
    override = _OFFICIAL_TARGET_MASTER_NAME_OVERRIDES.get(canonical_game)
    if override:
        return list(override)

    try:
        from creation_lib.esp.schema.corpus import OFFICIAL_PLUGIN_ALLOWLISTS
    except Exception:
        OFFICIAL_PLUGIN_ALLOWLISTS = {}

    names = list(OFFICIAL_PLUGIN_ALLOWLISTS.get(canonical_game, []))
    if names:
        return names

    base_master = _target_base_master_name(target_game)
    return [base_master] if base_master else []


def resolve_official_target_master_paths(
    target_game: str,
    *,
    target_master_paths: Iterable[str | Path] = (),
    target_data_dir: str | Path | None = None,
    target_extracted_dir: str | Path | None = None,
) -> tuple[list[Path], list[str]]:
    """Resolve official target masters and report unavailable names.

    Explicit file paths are considered first. Directory inputs and target roots
    are searched by filename. Missing official masters are returned to the caller
    for diagnostics rather than raising.
    """
    explicit_files: dict[str, Path] = {}
    search_dirs: list[Path] = []
    for value in target_master_paths or ():
        path = Path(value)
        if path.is_file():
            explicit_files[path.name.casefold()] = path
        elif path.is_dir():
            search_dirs.append(path)

    for value in (target_data_dir, target_extracted_dir):
        if value:
            path = Path(value)
            if path.is_dir():
                search_dirs.append(path)

    resolved: list[Path] = []
    missing: list[str] = []
    seen_paths: set[str] = set()

    def add_path(path: Path) -> None:
        key = str(path.resolve()).casefold()
        if key in seen_paths:
            return
        seen_paths.add(key)
        resolved.append(path)

    for name in official_target_master_names(target_game):
        explicit = explicit_files.get(name.casefold())
        if explicit is not None:
            add_path(explicit)
            continue
        found = _find_named_file(name, search_dirs)
        if found is None:
            missing.append(name)
            continue
        add_path(found)

    return resolved, missing


def resolve_target_master_plugin_paths(
    target_game: str,
    *,
    target_master_paths: Iterable[str | Path] = (),
    target_data_dir: str | Path | None = None,
    target_extracted_dir: str | Path | None = None,
) -> tuple[list[Path], list[str]]:
    """Resolve the deduplicated official + explicit/fallback master path set.

    This is the master set the native ConversionRun must load: passing only
    the request's explicit paths (usually empty) gives the run zero masters
    and clobbers config.target_master_names at run creation.
    """
    official_paths, missing = resolve_official_target_master_paths(
        target_game,
        target_master_paths=target_master_paths,
        target_data_dir=target_data_dir,
        target_extracted_dir=target_extracted_dir,
    )
    if missing:
        # Missing masters silently degrade master-resident lookups in the fixup
        # phase (e.g. strip_invalid_npc_face_morphs can't read HumanRace's FMRI
        # table, so it refuses to strip and the NPC's invalid morphs survive).
        # Surface it so a misconfigured target Data dir is visible, not silent.
        _log.warning(
            "target masters not found for %s: %s "
            "(searched target_master_paths/target_data_dir/target_extracted_dir) "
            "— master-resident fixup lookups will be degraded",
            target_game,
            ", ".join(missing),
        )
    fallback_paths = resolve_target_master_paths(
        target_game,
        target_master_paths=target_master_paths,
        target_data_dir=target_data_dir,
        target_extracted_dir=target_extracted_dir,
    )
    paths: list[Path] = []
    seen: set[str] = set()

    def add(path: Path) -> None:
        key = str(path.resolve()).casefold()
        if key in seen:
            return
        seen.add(key)
        paths.append(path)

    for path in official_paths:
        add(path)
    for path in fallback_paths:
        add(path)

    return paths, missing


def resolve_required_target_master_path(
    name: str,
    *,
    target_master_paths: Iterable[str | Path] = (),
    target_data_dir: str | Path | None = None,
    target_extracted_dir: str | Path | None = None,
) -> Path:
    """Resolve one required non-official master or raise with its search scope."""
    search_dirs: list[Path] = []
    nested_search_dirs: list[Path] = []
    for value in target_master_paths or ():
        path = Path(value)
        if path.is_file() and path.name.casefold() == name.casefold():
            return path
        if path.is_dir():
            search_dirs.append(path)
            nested_search_dirs.append(path)
    for value in (target_data_dir, target_extracted_dir):
        if value:
            path = Path(value)
            if path.is_dir():
                search_dirs.append(path)
    found = _find_named_file(name, search_dirs)
    if found is None:
        found = _find_named_file_in_child_dirs(name, nested_search_dirs)
    if found is not None:
        return found
    searched = (
        ", ".join(str(path) for path in search_dirs)
        or "no configured target directories"
    )
    raise FileNotFoundError(
        f"required target master {name} was not found (searched {searched})"
    )


def open_target_master_handles(
    target_game: str,
    *,
    target_master_paths: Iterable[str | Path] = (),
    target_data_dir: str | Path | None = None,
    target_extracted_dir: str | Path | None = None,
) -> list:
    """Open target master handles and close partial results on failure."""
    from creation_lib.esp.plugin import Plugin

    paths, _missing = resolve_target_master_plugin_paths(
        target_game,
        target_master_paths=target_master_paths,
        target_data_dir=target_data_dir,
        target_extracted_dir=target_extracted_dir,
    )

    handles = []
    try:
        for path in paths:
            # Masters are read-only here (formid->sig / eid lookups + a few
            # on-demand record reads), so load them index-only to avoid the
            # multi-GB resident parsed tree per master.
            handles.append(Plugin.load(path, game=target_game, lazy_index=True))
    except Exception:
        close_plugin_handles(handles)
        raise
    if not handles and official_target_master_names(target_game):
        _log.warning(
            "no target master handles opened for %s — fixups that read "
            "master-resident records (RACE/LCTN/ECZN/...) will be skipped",
            target_game,
        )
    return handles


def close_plugin_handles(handles: Iterable) -> None:
    for handle in handles or ():
        close = getattr(handle, "close", None)
        if callable(close):
            close()


def _target_base_master_name(target_game: str) -> str:
    try:
        from creation_lib.core.game_profiles import get_profile

        profile = get_profile(target_game)
    except Exception:
        return ""
    return str(getattr(profile, "master_esm", "") or "")


def _find_named_file(name: str, search_dirs: Iterable[Path]) -> Path | None:
    lowered = name.casefold()
    for base_dir in search_dirs:
        for candidate_dir in _target_master_search_dirs(base_dir):
            candidate = candidate_dir / name
            if candidate.is_file():
                return candidate
            try:
                children = list(candidate_dir.iterdir())
            except OSError:
                continue
            for child in children:
                if child.is_file() and child.name.casefold() == lowered:
                    return child
    return None


def _find_named_file_in_child_dirs(
    name: str, search_dirs: Iterable[Path]
) -> Path | None:
    for search_dir in search_dirs:
        try:
            child_dirs = sorted(
                (child for child in search_dir.iterdir() if child.is_dir()),
                key=lambda path: path.name.casefold(),
            )
        except OSError:
            continue
        found = _find_named_file(name, child_dirs)
        if found is not None:
            return found
    return None


def _target_master_search_dirs(base_dir: Path) -> list[Path]:
    search_dirs = [base_dir]
    data_dir = base_dir / "Data"
    if data_dir.is_dir():
        search_dirs.append(data_dir)
    return search_dirs
