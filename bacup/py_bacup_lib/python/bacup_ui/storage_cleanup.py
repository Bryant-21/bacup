from __future__ import annotations

import ctypes
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Iterable


_TEMP_DIR_PREFIXES = (
    "bacup-",
    "bacup_",
    "behavior_render_",
    "hkxunpack_",
    "weapon_combo_",
)
@dataclass(frozen=True)
class CleanupTarget:
    key: str
    label: str
    detail: str
    paths: tuple[Path, ...]
    size_bytes: int = 0


@dataclass(frozen=True)
class CleanupResult:
    deleted_keys: tuple[str, ...]
    removed_paths: tuple[Path, ...]
    freed_bytes: int
    failures: tuple[str, ...]


def windows_temp_dir() -> Path:
    return Path(tempfile.gettempdir())


def legacy_local_data_dir() -> Path:
    local_app_data = os.environ.get("LOCALAPPDATA")
    root = Path(local_app_data) if local_app_data else Path.home() / "AppData" / "Local"
    return root / "modkit21" / "conversion"


def _resolved(path: Path) -> Path:
    try:
        return path.resolve()
    except OSError:
        return Path(os.path.abspath(path))


def _safe_standalone_directory(
    path: Path,
    forbidden_roots: Iterable[Path],
) -> bool:
    if not path.is_dir():
        return False
    candidate = _resolved(path)
    if candidate == Path(candidate.anchor) or candidate == _resolved(Path.home()):
        return False
    for forbidden in forbidden_roots:
        forbidden = _resolved(forbidden)
        if candidate == forbidden or candidate in forbidden.parents:
            return False
    return True


def _case_insensitive_child(root: Path, name: str) -> Path | None:
    try:
        return next(
            (
                child
                for child in root.iterdir()
                if child.name.casefold() == name.casefold() and child.is_dir()
            ),
            None,
        )
    except OSError:
        return None


def discover_cleanup_targets(
    *,
    fo4_extracted_dir: Path | None,
    fo76_extracted_dir: Path | None,
    forbidden_roots: Iterable[Path] = (),
    temp_root: Path | None = None,
    legacy_local_data_root: Path | None = None,
) -> tuple[CleanupTarget, ...]:
    temp_root = temp_root or windows_temp_dir()
    legacy_local_data_root = legacy_local_data_root or legacy_local_data_dir()
    protected = (*forbidden_roots, temp_root)
    targets: list[CleanupTarget] = []

    if fo4_extracted_dir and _safe_standalone_directory(
        fo4_extracted_dir, protected
    ):
        targets.append(
            CleanupTarget(
                key="fo4_extracted",
                label="Fallout 4 extracted directory",
                detail=(
                    "Optional loose-file development cache. BACUP reads official "
                    "Fallout 4 BA2s directly."
                ),
                paths=(fo4_extracted_dir,),
            )
        )

    if fo76_extracted_dir and fo76_extracted_dir.is_dir():
        for name, key, label, detail in (
            (
                "geoexporter",
                "fo76_geoexporter",
                "Fallout 76 GeoExporter data",
                "GeoExporter files are not consumed by the converter.",
            ),
            (
                "vis",
                "fo76_vis",
                "Fallout 76 VIS data",
                "VIS files are not consumed by the converter.",
            ),
        ):
            child = _case_insensitive_child(fo76_extracted_dir, name)
            if child is None:
                continue
            targets.append(
                CleanupTarget(
                    key=key,
                    label=label,
                    detail=detail,
                    paths=(child,),
                )
            )

    temp_paths: list[Path] = []
    try:
        for child in temp_root.iterdir():
            if not child.is_dir():
                continue
            lowered = child.name.casefold()
            if any(lowered.startswith(prefix) for prefix in _TEMP_DIR_PREFIXES):
                temp_paths.append(child)
    except OSError:
        pass
    if temp_paths:
        targets.append(
            CleanupTarget(
                key="bacup_temp",
                label="Previous BACUP temporary folders",
                detail="Abandoned conversion work folders under Windows Temp.",
                paths=tuple(sorted(temp_paths, key=lambda path: path.name.casefold())),
            )
        )
    if _safe_standalone_directory(legacy_local_data_root, protected):
        targets.append(
            CleanupTarget(
                key="legacy_local_data",
                label="Previous BACUP LocalAppData cache",
                detail=(
                    "Legacy FO4 catalog and extracted target assets. Current builds "
                    "store BACUP data beside the application instead."
                ),
                paths=(legacy_local_data_root,),
            )
        )
    return tuple(targets)


def _is_link_like(path: Path) -> bool:
    is_junction = getattr(path, "is_junction", None)
    return path.is_symlink() or bool(callable(is_junction) and is_junction())


def _path_size_bytes(path: Path) -> int:
    try:
        if _is_link_like(path) or path.is_file():
            return path.lstat().st_size
        if not path.is_dir():
            return 0
    except OSError:
        return 0

    total = 0
    pending = [path]
    while pending:
        current = pending.pop()
        try:
            with os.scandir(current) as entries:
                for entry in entries:
                    try:
                        if entry.is_dir(follow_symlinks=False):
                            pending.append(Path(entry.path))
                        elif entry.is_file(follow_symlinks=False):
                            total += entry.stat(follow_symlinks=False).st_size
                    except OSError:
                        continue
        except OSError:
            continue
    return total


def measure_cleanup_targets(
    targets: Iterable[CleanupTarget],
) -> tuple[CleanupTarget, ...]:
    return tuple(
        replace(target, size_bytes=sum(_path_size_bytes(path) for path in target.paths))
        for target in targets
    )


def _remove_path(path: Path) -> None:
    from bacup_lib.native_runtime import load_native_module

    try:
        load_native_module().conversion_remove_path(str(path.absolute()))
    except (AttributeError, RuntimeError) as error:
        raise OSError(f"Native cleanup unavailable: {error}") from error


def delete_cleanup_targets(targets: Iterable[CleanupTarget]) -> CleanupResult:
    deleted_keys: list[str] = []
    removed_paths: list[Path] = []
    failures: list[str] = []
    freed_bytes = 0
    for target in targets:
        target_failed = False
        for path in target.paths:
            try:
                _remove_path(path)
                removed_paths.append(path)
            except OSError as exc:
                target_failed = True
                failures.append(f"{path}: {exc}")
        if not target_failed:
            deleted_keys.append(target.key)
            freed_bytes += target.size_bytes
    return CleanupResult(
        deleted_keys=tuple(deleted_keys),
        removed_paths=tuple(removed_paths),
        freed_bytes=freed_bytes,
        failures=tuple(failures),
    )


def is_running_as_admin() -> bool:
    try:
        return bool(ctypes.windll.shell32.IsUserAnAdmin())
    except (AttributeError, OSError):
        return False


def elevated_launch_command() -> tuple[str, str, str]:
    frozen = bool(getattr(sys, "frozen", False))
    arguments = list(sys.argv[1:]) if frozen else ["-m", "bacup_ui", *sys.argv[1:]]
    working_dir = Path(sys.executable).resolve().parent if frozen else Path.cwd()
    return sys.executable, subprocess.list2cmdline(arguments), str(working_dir)


def restart_as_admin() -> None:
    try:
        shell_execute = ctypes.windll.shell32.ShellExecuteW
    except AttributeError as exc:
        raise OSError("Administrator restart is only available on Windows") from exc
    executable, parameters, working_dir = elevated_launch_command()
    result = shell_execute(
        None,
        "runas",
        executable,
        parameters,
        working_dir,
        1,
    )
    if result <= 32:
        raise OSError(f"Windows elevation request failed ({result})")
