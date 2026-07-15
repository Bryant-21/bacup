"""Window title for the B.A.C.U.P. app variant."""
from __future__ import annotations

from bacup_lib.upgrade_manifest import bundled_upgrade_manifest_path, load_upgrade_manifest

_FALLBACK = "B.A.C.U.P."


def appalachia_window_title() -> str:
    try:
        manifest = load_upgrade_manifest(bundled_upgrade_manifest_path())
        return f"{_FALLBACK} - {manifest.current}"
    except Exception:
        return _FALLBACK
