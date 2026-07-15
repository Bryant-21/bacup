from __future__ import annotations

import os


def _mod_prefix(orchestrator) -> str:
    configured_prefix = str(getattr(orchestrator, "mod_prefix", "") or "").strip()
    if configured_prefix:
        return configured_prefix
    mod_name = os.path.basename(getattr(orchestrator, "mod_path", "")).strip()
    if "_" in mod_name:
        return mod_name.split("_", 1)[0] or mod_name
    return mod_name or "B21"


def _weapon_overlay_output_rel(orchestrator, weap_eid: str) -> str:
    return f"Meshes/AnimsTextData/{_mod_prefix(orchestrator)}_{weap_eid}_FireAdditive.hkx"

