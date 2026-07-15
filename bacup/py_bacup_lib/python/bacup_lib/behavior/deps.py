"""Behavior dependency extraction helpers for the dependency walker."""
from __future__ import annotations

import logging
import os

from bacup_lib.models import AssetRef

_log = logging.getLogger("conversion.walker")


def extract_behavior_refs(nif_path: str) -> list[str]:
    """Open a NIF and return behavior graph HKX paths from BSBehaviorGraphExtraData.

    Complements ``NifIndexLookup.get_behaviors`` for cases where the NIF
    index is incomplete or hasn't been built for this NIF.  Reading the
    NIF is slow relative to an index lookup, so this should only be called
    on the small set of NIFs that end up in a walk graph.

    Returns an empty list on any read error so callers can treat this as
    a best-effort enrichment pass that never blocks the main walk.
    """
    try:
        from creation_lib.nif.nif_file import NifFile
    except Exception:
        return []
    try:
        nif = NifFile.load(nif_path)
    except Exception as e:
        _log.debug("Behavior ref walk failed for %s: %s", nif_path, e)
        return []

    refs: list[str] = []
    for block in nif.blocks:
        if block.type_name != "BSBehaviorGraphExtraData":
            continue
        bpath = block.get_field("Behaviour Graph File") or ""
        if isinstance(bpath, str) and bpath.strip():
            refs.append(bpath.strip().replace("\\", "/"))

    # Dedupe while preserving order.
    seen: set[str] = set()
    out: list[str] = []
    for r in refs:
        key = r.lower()
        if key not in seen:
            seen.add(key)
            out.append(r)
    return out


def expand_behavior_bundle(
    behavior_asset: AssetRef, extracted_dir: str | None
) -> list[AssetRef]:
    """Expand a behavior project HKX into its companion files.

    A Havok behavior project (e.g. ``Effects/EffectBehaviors/Foo/Foo.hkx``)
    lives in a directory alongside ``Behaviors/Behavior.hkx`` and
    ``Characters/Character.hkx``.  The NIF only references the project file,
    so we need to discover the companions from the filesystem.
    """
    if not extracted_dir:
        return []
    src = behavior_asset.source_path
    if not src.lower().endswith(".hkx"):
        return []

    # The project file is at <dir>/<name>.hkx — companions are in sibling dirs
    project_dir = os.path.dirname(src)
    if not project_dir:
        return []

    companions: list[AssetRef] = []
    mesh_root = os.path.join(extracted_dir, "meshes")
    base = os.path.join(mesh_root, project_dir)
    if not os.path.isdir(base):
        alt_mesh_root = os.path.join(extracted_dir, "Meshes")
        alt_base = os.path.join(alt_mesh_root, project_dir)
        if os.path.isdir(alt_base):
            mesh_root = alt_mesh_root
            base = alt_base
    if not os.path.isdir(base):
        return []

    # Walk the behavior directory for all .hkx files that aren't the project
    project_basename = os.path.basename(src).lower()
    for root, _dirs, files in os.walk(base):
        for fname in files:
            if not fname.lower().endswith(".hkx"):
                continue
            full = os.path.join(root, fname)
            rel = os.path.relpath(full, mesh_root).replace("\\", "/")
            if rel.lower() == src.lower():
                continue  # Skip the project file itself
            companions.append(AssetRef("behavior", rel))

    return companions
