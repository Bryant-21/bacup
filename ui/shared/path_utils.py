"""Shared path utilities for game-relative path conversion."""
from __future__ import annotations


def to_game_relative_path(abs_path: str, file_type: str) -> str:
    """Convert an absolute filesystem path to a game-relative path.

    FO4 texture paths: stored as ``Textures/path/to/file.dds``
    FO4 material paths: stored as ``path/to/file.bgsm`` (relative to Data/Materials/)

    Args:
        abs_path: Absolute path from a file dialog. May use backslashes.
        file_type: ``"texture"`` or ``"material"``

    Returns:
        Game-relative path string using forward slashes.
    """
    path = abs_path.replace("\\", "/")
    parts = path.split("/")
    lower_parts = [p.lower() for p in parts]

    if file_type == "texture":
        # Look for Textures/ segment — return from that segment onward
        for i, lp in enumerate(lower_parts):
            if lp == "textures":
                return "/".join(parts[i:])
        # Fallback: strip to after Data/
        for i, lp in enumerate(lower_parts):
            if lp == "data":
                return "/".join(parts[i + 1:])

    elif file_type == "material":
        # Look for Materials/ segment — return everything AFTER it
        # (FO4 material paths do not include the Materials/ prefix)
        for i, lp in enumerate(lower_parts):
            if lp == "materials":
                return "/".join(parts[i + 1:])
        # Fallback: strip to after Data/
        for i, lp in enumerate(lower_parts):
            if lp == "data":
                return "/".join(parts[i + 1:])

    # Final fallback: filename only
    return parts[-1] if parts else abs_path
