"""Restore FO76 UI sources missing from the extracted tree by re-reading the game's BA2s.

Users delete or lose extracted files; every UI converter reads its SWFs (and the
``interface/`` tables beside them) from the extracted tree, so one missing file used
to fail that whole converter.
"""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass, field
from pathlib import Path

from creation_lib.ba2 import native_runtime
from creation_lib.preprocessor.extraction import find_archives


@dataclass
class ReextractResult:
    restored: list[str] = field(default_factory=list)
    failed: dict[str, str] = field(default_factory=dict)


def _is_ui_source(member: str) -> bool:
    lowered = member.casefold()
    return lowered.startswith("interface/") or lowered.endswith(".swf")


def _ui_members_by_archive(archive_dirs: Iterable[Path]) -> tuple[dict[str, tuple[Path, str]], dict[str, str]]:
    newest: dict[str, tuple[Path, str]] = {}
    unreadable: dict[str, str] = {}
    for data_dir in archive_dirs:
        if not Path(data_dir).is_dir():
            continue
        # find_archives returns setup's extraction order, so a later archive's copy wins,
        # matching what the original extraction left on disk.
        for archive in find_archives(Path(data_dir), "ba2"):
            if "textures" in archive.stem.casefold():
                continue
            try:
                members = native_runtime.list_archive(str(archive)) or []
            except Exception as exc:  # noqa: BLE001 - reported per archive
                unreadable[archive.name] = str(exc)
                continue
            for member in members:
                rel = member.replace("\\", "/")
                if _is_ui_source(rel):
                    newest[rel.casefold()] = (archive, member)
    return newest, unreadable


def restore_missing_ui_sources(extracted_root: Path, archive_dirs: Iterable[Path]) -> ReextractResult:
    extracted_root = Path(extracted_root)
    result = ReextractResult()
    members, unreadable = _ui_members_by_archive(archive_dirs)
    for archive_name, reason in unreadable.items():
        result.failed[archive_name] = f"could not read archive: {reason}"
    for archive, member in members.values():
        rel = member.replace("\\", "/")
        destination = extracted_root.joinpath(*rel.split("/"))
        if destination.is_file():
            continue
        try:
            payload = native_runtime.extract_one(str(archive), member)
            if payload is None:
                raise OSError(f"not found in {archive.name}")
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(payload)
        except Exception as exc:  # noqa: BLE001 - reported per file
            result.failed[rel] = f"{archive.name}: {exc}"
            continue
        result.restored.append(rel)
    return result
