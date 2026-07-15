"""Recent file list persistence for the NIF editor.

Stores the list of recently opened NIF files in ~/.nif_editor_recent.json.
"""

import json
from pathlib import Path

_RECENT_FILE = Path.home() / ".nif_editor_recent.json"
_MAX_ENTRIES = 10


def _load() -> list[str]:
    """Load the recent files list from disk."""
    try:
        if _RECENT_FILE.exists():
            data = json.loads(_RECENT_FILE.read_text(encoding="utf-8"))
            if isinstance(data, list):
                return [str(p) for p in data if isinstance(p, str)]
    except Exception:
        pass
    return []


def _save(entries: list[str]) -> None:
    """Save the recent files list to disk."""
    try:
        _RECENT_FILE.write_text(
            json.dumps(entries, indent=2), encoding="utf-8"
        )
    except Exception:
        pass


def add(filepath: str) -> None:
    """Add a file path to the front of the recent files list.

    Deduplicates and keeps at most _MAX_ENTRIES entries.
    """
    filepath = str(Path(filepath).resolve())
    entries = _load()

    # Remove duplicates of this path
    entries = [p for p in entries if p != filepath]

    # Insert at front
    entries.insert(0, filepath)

    # Trim to max
    entries = entries[:_MAX_ENTRIES]

    _save(entries)


def get_list() -> list[str]:
    """Return the list of recently opened file paths (most recent first)."""
    return _load()


def remove(filepath: str) -> None:
    """Remove a specific file path from the recent files list."""
    filepath = str(Path(filepath).resolve())
    entries = _load()
    entries = [p for p in entries if p != filepath]
    _save(entries)


def clear() -> None:
    """Clear the recent files list."""
    _save([])
