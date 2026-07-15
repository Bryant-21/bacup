from __future__ import annotations

import re

_DATA_ROOTS = ("meshes", "textures", "materials", "sound")
_DRIVE_PATH_RE = re.compile(r"^[a-zA-Z]:/")


def normalize_asset_source_path(source_path: str) -> str:
    rel_path = source_path.replace("\\", "/").replace("\0", "").strip().strip('"')
    lower = rel_path.lower()
    absolute_like = _DRIVE_PATH_RE.match(rel_path) is not None or lower.startswith("/")

    data_idx = lower.rfind("/data/")
    if data_idx != -1:
        rel_path = rel_path[data_idx + 6 :]
    elif lower.startswith("data/"):
        rel_path = rel_path[5:]

    rel_path = rel_path.lstrip("/")
    lower = rel_path.lower()
    parts = rel_path.split("/")

    if parts and parts[0].lower() in _DATA_ROOTS:
        return rel_path

    if absolute_like or "/build/pc/" in lower:
        for idx, part in enumerate(parts):
            if part.lower() in _DATA_ROOTS:
                return "/".join(parts[idx:])

    return rel_path
