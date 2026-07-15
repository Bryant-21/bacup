"""Version helpers for ModBox21."""

from .app_paths import get_code_root


def get_version() -> str:
    """Read version from the bundled VERSION file. Returns 'unknown' on failure."""
    try:
        return (get_code_root() / "VERSION").read_text(encoding="utf-8").strip()
    except Exception:
        return "unknown"
