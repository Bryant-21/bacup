"""Batch texture conversion for porting entire mod texture directories."""
from __future__ import annotations

import logging
from dataclasses import dataclass, field
from pathlib import Path

from creation_lib.core.game_profiles import GameProfile
from creation_lib.textures.naming import detect_texture_role

_log = logging.getLogger("conversion.texture.batch")

TEXTURE_EXTENSIONS = {".dds", ".png", ".tga", ".bmp"}


@dataclass
class ConversionDetail:
    source_file: Path
    output_file: Path | None
    role: str | None
    status: str  # "converted", "skipped", "error"
    error_message: str | None = None


@dataclass
class BatchReport:
    total_files: int = 0
    converted_files: int = 0
    skipped_files: int = 0
    errors: int = 0
    details: list[ConversionDetail] = field(default_factory=list)


def group_textures_by_base(
    files: list[Path],
    profile: GameProfile,
) -> dict[str, list[tuple[Path, str | None]]]:
    """Group texture files by their base name (before the game-specific suffix).

    Returns dict of base_name -> [(filepath, role), ...]
    """
    groups: dict[str, list[tuple[Path, str | None]]] = {}
    for filepath in files:
        role = detect_texture_role(filepath.name, profile)
        if role is None:
            base = filepath.stem
        else:
            suffix = profile.texture_suffixes[role]
            stem = filepath.stem
            idx = stem.lower().rfind(suffix.lower())
            base = stem[:idx] if idx >= 0 else stem

        groups.setdefault(base, []).append((filepath, role))
    return groups


def batch_convert(
    source_dir: str | Path,
    output_dir: str | Path,
    source_profile: GameProfile,
    target_profile: GameProfile,
) -> BatchReport:
    """Convert all textures in source_dir from source game to target game.

    Groups textures by base name first, then converts as complete texture sets,
    which enables multi-file operations like FO76->FO4 _r+_l -> _s merging.
    """
    # Import here to avoid circular import (__init__ imports batch)
    from .native import convert_texture_paths

    source_dir = Path(source_dir)
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    report = BatchReport()

    # Collect texture files, skip non-textures
    tex_files = []
    for filepath in sorted(source_dir.rglob("*")):
        if not filepath.is_file():
            continue
        report.total_files += 1
        if filepath.suffix.lower() not in TEXTURE_EXTENSIONS:
            report.skipped_files += 1
            report.details.append(ConversionDetail(
                source_file=filepath, output_file=None,
                role=None, status="skipped",
            ))
            continue
        tex_files.append(filepath)

    # Group by base name
    groups = group_textures_by_base(tex_files, source_profile)

    for base_name, files_and_roles in groups.items():
        try:
            result = convert_texture_paths(
                files_and_roles,
                output_dir,
                source_profile,
                target_profile,
            )
            for item in result.get("converted", []):
                out_path = Path(item["path"])
                report.converted_files += 1
                report.details.append(ConversionDetail(
                    source_file=files_and_roles[0][0],
                    output_file=out_path,
                    role=item.get("role"),
                    status="converted",
                ))
            for item in result.get("skipped", []):
                report.skipped_files += 1
                report.details.append(ConversionDetail(
                    source_file=files_and_roles[0][0],
                    output_file=None,
                    role=item.get("role"),
                    status="skipped",
                    error_message=item.get("reason"),
                ))
        except Exception as e:
            _log.error("Failed to convert group %s: %s", base_name, e)
            for filepath, role in files_and_roles:
                report.errors += 1
                report.details.append(ConversionDetail(
                    source_file=filepath, output_file=None,
                    role=role, status="error", error_message=str(e),
                ))

    return report
