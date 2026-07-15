"""LOD settings profile resolution shared by CLI and UI callers."""
from __future__ import annotations

import json
from pathlib import Path


PROFILE_AUTO = "auto"
PROFILE_NATIVE = "native"
PROFILE_HIGH_QUALITY = "high-quality"
PROFILE_PERFORMANCE = "performance"

PROFILE_CHOICES = (
    PROFILE_AUTO,
    PROFILE_NATIVE,
    PROFILE_HIGH_QUALITY,
    PROFILE_PERFORMANCE,
)

PROFILE_LABELS = {
    PROFILE_HIGH_QUALITY: "High quality",
    PROFILE_PERFORMANCE: "Performance",
}

PROFILE_FILENAMES = {
    PROFILE_NATIVE: "native.fo76fo4.json",
    PROFILE_HIGH_QUALITY: "hybrid-high.fo76fo4.json",
    PROFILE_PERFORMANCE: "hybrid-performance.fo76fo4.json",
}


def normalize_profile(profile: str | None, lod_mode: str) -> str:
    value = (profile or PROFILE_AUTO).strip().lower().replace("_", "-")
    if value == PROFILE_AUTO:
        return PROFILE_NATIVE if lod_mode == "generate" else PROFILE_HIGH_QUALITY
    if value not in PROFILE_FILENAMES:
        raise ValueError(f"unknown LOD settings profile: {profile!r}")
    return value


def profile_path(code_root: Path, profile: str) -> Path:
    filename = PROFILE_FILENAMES[profile]
    return code_root / "bacup" / "scripts" / "lod_settings" / filename


def load_profile_settings(
    code_roots: list[Path],
    *,
    profile: str | None,
    lod_mode: str,
) -> dict:
    resolved = normalize_profile(profile, lod_mode)
    checked: list[Path] = []
    for root in code_roots:
        path = profile_path(root, resolved)
        checked.append(path)
        if path.is_file():
            with path.open(encoding="utf-8") as f:
                return json.load(f)

    if resolved == PROFILE_HIGH_QUALITY:
        for root in code_roots:
            legacy_path = root / "bacup" / "scripts" / "lod_settings.fo76fo4.json"
            checked.append(legacy_path)
            if legacy_path.is_file():
                with legacy_path.open(encoding="utf-8") as f:
                    return json.load(f)

    searched = ", ".join(str(path) for path in checked)
    raise FileNotFoundError(
        f"LOD settings profile '{resolved}' was not found; checked: {searched}"
    )
