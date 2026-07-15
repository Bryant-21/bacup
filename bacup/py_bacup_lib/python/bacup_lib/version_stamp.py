from __future__ import annotations
from pathlib import Path


def read_plugin_snam(esm_path: Path) -> str | None:
    """Read the TES4 SNAM (description) from a plugin on disk.

    Returns `None` if the file doesn't exist or the plugin has no SNAM.
    """
    esm_path = Path(esm_path)
    if not esm_path.is_file():
        return None

    from creation_lib.esp import Plugin

    plugin = Plugin.load(esm_path, game="fo4")
    try:
        description = plugin.header.description
    finally:
        plugin.close()
    return description or None


def read_plugin_snam_header(esm_path) -> str | None:
    """Read TES4 SNAM (description) by parsing only the header record.

    Fast, GIL-friendly alternative to read_plugin_snam for hot UI paths:
    reads just the first record's bytes instead of loading the whole plugin.
    Returns None if the file is missing, not a TES4 plugin, the header record
    is zlib-compressed, or there is no SNAM subrecord.
    """
    esm_path = Path(esm_path)
    try:
        with open(esm_path, "rb") as f:
            header = f.read(24)  # FO4 record header is 24 bytes
            if len(header) < 24 or header[:4] != b"TES4":
                return None
            data_size = int.from_bytes(header[4:8], "little")
            flags = int.from_bytes(header[8:12], "little")
            if flags & 0x00040000:  # compressed record — bail (never happens for TES4)
                return None
            data = f.read(data_size)
    except OSError:
        return None
    if len(data) < data_size:
        return None
    i, n = 0, len(data)
    while i + 6 <= n:
        sig = data[i:i + 4]
        size = int.from_bytes(data[i + 4:i + 6], "little")
        i += 6
        if i + size > n:
            break
        if sig == b"SNAM":
            return data[i:i + size].split(b"\x00", 1)[0].decode("utf-8", "replace") or None
        i += size
    return None


def stamp_plugin_version(run, version: str) -> None:
    """Stamp `version` into TES4 SNAM on a conversion run's target plugin."""
    run.set_target_description(version)
