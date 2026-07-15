"""NIF bash engine — merges source NIF data into the target root.

Unlike import (which copies blocks as-is), bashing creates a new NiNode under
the target root for each bashed source, copies geometry and collision under it,
and merges connect points, BSXFlags, and animation controllers at root level.
"""
from __future__ import annotations

import logging
import os
from dataclasses import dataclass, field

from creation_lib.nif.nif_file import NifFile

_log = logging.getLogger(__name__)

# Block types that live in Extra Data List
_EXTRA_DATA_TYPES = frozenset({
    "BSConnectPoint::Parents", "BSConnectPoint::Children",
    "BSXFlags", "BSBehaviorGraphExtraData", "NiDefaultAVObjectPalette",
    "NiStringExtraData", "NiIntegerExtraData", "NiFloatExtraData",
    "NiBooleanExtraData", "NiBinaryExtraData", "BSInvMarker",
    "BSBoneLODExtraData", "BSFurnitureMarkerNode",
})

_ANIMATION_TYPES = frozenset({
    "NiControllerManager",
    "NiControllerSequence",
    "NiMultiTargetTransformController",
})


@dataclass
class BashResult:
    """Result of a bash operation."""
    blocks_added: int = 0
    merged: list[str] = field(default_factory=list)
    skipped: list[str] = field(default_factory=list)
    error: str = ""


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _ref_to_id(ref) -> int:
    """Normalize a Ref value (int or dict) to a block ID, or -1."""
    if isinstance(ref, int):
        return ref
    if isinstance(ref, dict):
        return ref.get("block_id", -1)
    return -1


def _walk_controller_chain(nif: NifFile, root_bid: int) -> list[int]:
    """Walk the Controller linked list on a node, return all block IDs in the chain."""
    result = []
    node = nif.get_block(root_bid)
    if node is None:
        return result
    bid = node.get_field("Controller")
    if not isinstance(bid, int) or bid < 0:
        return result
    visited = set()
    while 0 <= bid < len(nif.blocks) and bid not in visited:
        visited.add(bid)
        result.append(bid)
        block = nif.get_block(bid)
        if block is None:
            break
        bid = block.get_field("Next Controller")
        if not isinstance(bid, int):
            break
    return result


def _find_block_on_root(nif: NifFile, type_name: str) -> int | None:
    """Find a block of *type_name* on root (Children, Extra Data List, or Controller chain)."""
    root = nif.get_block(0)
    if root is None:
        return None

    for ref in (root.get_field("Children") or []):
        bid = _ref_to_id(ref)
        if 0 <= bid < len(nif.blocks):
            block = nif.get_block(bid)
            if block and block.type_name == type_name:
                return bid

    for ref in (root.get_field("Extra Data List") or []):
        bid = _ref_to_id(ref)
        if 0 <= bid < len(nif.blocks):
            block = nif.get_block(bid)
            if block and block.type_name == type_name:
                return bid

    # Controller chain (NiControllerManager lives here)
    for bid in _walk_controller_chain(nif, 0):
        block = nif.get_block(bid)
        if block and block.type_name == type_name:
            return bid

    return None


def _gather_source_parts(source: NifFile) -> dict:
    """Categorize source root's children AND extra data into parts.

    Returns dict with keys: geometry, animations, connect_points,
    extra_data, collision_id.
    """
    parts: dict = {
        "geometry": [],       # block IDs from Children
        "animations": [],     # block IDs from Children
        "connect_points": [], # block IDs from Children or Extra Data List
        "extra_data": [],     # block IDs from Extra Data List (BSXFlags, etc.)
        "collision_id": None, # block ID of Collision Object on root
    }

    root = source.get_block(0)
    if root is None:
        return parts

    # --- Children ---
    for ref in (root.get_field("Children") or []):
        bid = _ref_to_id(ref)
        if bid < 0 or bid >= len(source.blocks):
            continue
        block = source.get_block(bid)
        if block is None:
            continue
        tname = block.type_name
        if tname in _ANIMATION_TYPES:
            parts["animations"].append(bid)
        elif tname in ("BSConnectPoint::Parents", "BSConnectPoint::Children"):
            parts["connect_points"].append(bid)
        elif tname in _EXTRA_DATA_TYPES:
            parts["extra_data"].append(bid)
        else:
            parts["geometry"].append(bid)

    # --- Extra Data List ---
    for ref in (root.get_field("Extra Data List") or []):
        bid = _ref_to_id(ref)
        if bid < 0 or bid >= len(source.blocks):
            continue
        block = source.get_block(bid)
        if block is None:
            continue
        tname = block.type_name
        if tname in ("BSConnectPoint::Parents", "BSConnectPoint::Children"):
            parts["connect_points"].append(bid)
        else:
            parts["extra_data"].append(bid)

    # --- Controller chain (NiControllerManager typically lives here) ---
    for bid in _walk_controller_chain(source, 0):
        block = source.get_block(bid)
        if block is None:
            continue
        if block.type_name in _ANIMATION_TYPES:
            if bid not in parts["animations"]:
                parts["animations"].append(bid)

    # --- Collision Object (Ref on root, not in Children) ---
    col_ref = root.get_field("Collision Object")
    if isinstance(col_ref, int) and col_ref >= 0:
        parts["collision_id"] = col_ref

    return parts


def _add_to_extra_data_list(nif: NifFile, node_bid: int, new_block_bid: int) -> None:
    """Append a block ref to a node's Extra Data List."""
    node = nif.get_block(node_bid)
    if node is None:
        return
    edl = list(node.get_field("Extra Data List") or [])
    edl.append(new_block_bid)
    node.set_field("Extra Data List", edl)
    node.set_field("Num Extra Data List", len(edl))


def _add_to_children(nif: NifFile, node_bid: int, new_child_bid: int) -> None:
    """Append a block ref to a node's Children list."""
    node = nif.get_block(node_bid)
    if node is None:
        return
    children = list(node.get_field("Children") or [])
    children.append(new_child_bid)
    node.set_field("Children", children)
    node.set_field("Num Children", len(children))


# ---------------------------------------------------------------------------
# Per-type merge functions
# ---------------------------------------------------------------------------

def _merge_connect_point_parents(target: NifFile, t_bid: int,
                                  source: NifFile, s_bid: int) -> str | None:
    """Merge BSConnectPoint::Parents entries (deduplicate by Name)."""
    t_block = target.get_block(t_bid)
    s_block = source.get_block(s_bid)
    if not t_block or not s_block:
        return None

    t_cps = list(t_block.get_field("Connect Points") or [])
    s_cps = s_block.get_field("Connect Points") or []
    existing_names = {cp.get("Name", "") for cp in t_cps if isinstance(cp, dict)}

    added = 0
    for cp in s_cps:
        if not isinstance(cp, dict):
            continue
        name = cp.get("Name", "")
        if name not in existing_names:
            t_cps.append(cp)
            existing_names.add(name)
            added += 1

    if added:
        t_block.set_field("Connect Points", t_cps)
        t_block.set_field("Num Connect Points", len(t_cps))
        return f"Merged {added} parent connect point(s)"
    return None


def _merge_connect_point_children(target: NifFile, t_bid: int,
                                   source: NifFile, s_bid: int) -> str | None:
    """Merge BSConnectPoint::Children point names (deduplicate)."""
    t_block = target.get_block(t_bid)
    s_block = source.get_block(s_bid)
    if not t_block or not s_block:
        return None

    t_points = list(t_block.get_field("Points") or [])
    s_points = s_block.get_field("Points") or []
    existing = set(t_points)

    added = 0
    for pt in s_points:
        if pt not in existing:
            t_points.append(pt)
            existing.add(pt)
            added += 1

    # OR the Skinned flag
    s_skinned = s_block.get_field("Skinned")
    if s_skinned:
        t_block.set_field("Skinned", True)

    if added:
        t_block.set_field("Points", t_points)
        t_block.set_field("Num Points", len(t_points))
        return f"Merged {added} child connect point(s)"
    return None


def _merge_bsx_flags(target: NifFile, t_bid: int,
                     source: NifFile, s_bid: int) -> str | None:
    """OR the BSXFlags integer data."""
    t_block = target.get_block(t_bid)
    s_block = source.get_block(s_bid)
    if not t_block or not s_block:
        return None

    t_val = t_block.get_field("Integer Data") or 0
    s_val = s_block.get_field("Integer Data") or 0
    merged = t_val | s_val
    if merged != t_val:
        t_block.set_field("Integer Data", merged)
        return f"BSXFlags: {t_val:#x} | {s_val:#x} = {merged:#x}"
    return None


def _merge_object_palette(target: NifFile, t_bid: int,
                          id_map: dict[int, int]) -> str | None:
    """Append new geometry entries to NiDefaultAVObjectPalette Objs."""
    t_block = target.get_block(t_bid)
    if not t_block:
        return None

    objs = list(t_block.get_field("Objs") or [])
    added = 0
    for _src_id, new_id in id_map.items():
        new_block = target.get_block(new_id)
        if new_block is None:
            continue
        name = new_block.get_field("Name") or ""
        if not name:
            continue
        objs.append({"Name": name, "AV Object": new_id})
        added += 1

    if added:
        t_block.set_field("Objs", objs)
        t_block.set_field("Num Objs", len(objs))
        return f"Added {added} palette entries"
    return None


def _merge_animations(target: NifFile, source: NifFile,
                      source_anim_ids: list[int],
                      geometry_id_map: dict[int, int]) -> list[str]:
    """Merge animation blocks from source into target.

    If target has a NiControllerManager, appends source sequences to it.
    Otherwise falls back to plain copy_blocks.
    """
    from creation_lib.nif.operations.copy import copy_blocks

    if not source_anim_ids:
        return []

    messages: list[str] = []
    t_mgr_bid = _find_block_on_root(target, "NiControllerManager")

    if t_mgr_bid is None:
        # No existing manager — copy the animation blocks and wire to root Controller
        id_map = copy_blocks(source, source_anim_ids, target)
        # Find the copied NiControllerManager and set root's Controller ref
        t_root = target.get_block(0)
        for s_id in source_anim_ids:
            s_block = source.get_block(s_id)
            if s_block and s_block.type_name == "NiControllerManager":
                new_mgr_id = id_map.get(s_id)
                if new_mgr_id is not None and t_root:
                    t_root.set_field("Controller", new_mgr_id)
                    # Set the manager's Target back-ref to root
                    new_mgr = target.get_block(new_mgr_id)
                    if new_mgr:
                        new_mgr.set_field("Target", 0)
                break
        messages.append(f"Copied {len(id_map)} animation blocks (no existing manager)")
        return messages

    t_mgr = target.get_block(t_mgr_bid)
    if t_mgr is None:
        return messages

    # Find source NiControllerManager
    s_mgr_bid = None
    for sid in source_anim_ids:
        s_block = source.get_block(sid)
        if s_block and s_block.type_name == "NiControllerManager":
            s_mgr_bid = sid
            break

    if s_mgr_bid is None:
        # No manager in source anim blocks — copy as-is (loose controllers)
        id_map = copy_blocks(source, source_anim_ids, target)
        messages.append(f"Copied {len(id_map)} animation blocks (source has no manager)")
        return messages

    s_mgr = source.get_block(s_mgr_bid)
    if s_mgr is None:
        return messages

    # Collect source controller sequence block IDs
    s_seq_refs = s_mgr.get_field("Controller Sequences") or []
    s_seq_ids = [r for r in s_seq_refs if isinstance(r, int) and r >= 0]

    if not s_seq_ids:
        messages.append("Source manager has no controller sequences")
        return messages

    # Copy source sequences (with deps) into target
    seq_id_map = copy_blocks(source, s_seq_ids, target)

    # Append new sequence refs to target manager
    t_seqs = list(t_mgr.get_field("Controller Sequences") or [])
    added = 0
    for s_id in s_seq_ids:
        new_id = seq_id_map.get(s_id)
        if new_id is not None:
            t_seqs.append(new_id)
            added += 1
    t_mgr.set_field("Controller Sequences", t_seqs)
    t_mgr.set_field("Num Controller Sequences", len(t_seqs))
    messages.append(f"Merged {added} controller sequence(s)")

    # Merge NiDefaultAVObjectPalette entries if both have one
    t_pal_ref = t_mgr.get_field("Object Palette")
    if isinstance(t_pal_ref, int) and t_pal_ref >= 0:
        msg = _merge_object_palette(target, t_pal_ref, geometry_id_map)
        if msg:
            messages.append(msg)

    # Remap Extra Targets in copied NiMultiTargetTransformController blocks
    for _src_id, new_id in seq_id_map.items():
        block = target.get_block(new_id)
        if block is None or block.type_name != "NiMultiTargetTransformController":
            continue
        targets = block.get_field("Extra Targets") or []
        remapped = []
        for t in targets:
            if isinstance(t, int) and t in geometry_id_map:
                remapped.append(geometry_id_map[t])
            else:
                remapped.append(t)
        block.set_field("Extra Targets", remapped)

    return messages


# ---------------------------------------------------------------------------
# Main entry point
# ---------------------------------------------------------------------------

def bash_nif(app, source: NifFile, source_path: str = "") -> BashResult:
    """Bash (merge) a source NIF into the active target NIF.

    Creates a new NiNode under the target root named after the source file.
    Geometry and collision go under that node. Connect points, BSXFlags,
    and animations are merged at root level.

    Args:
        app: NifEditorApp instance.
        source: Already-loaded source NifFile.
        source_path: Original file path (used for naming the wrapper node).

    Returns:
        BashResult with counts and details.
    """
    from creation_lib.nif.actions import SnapshotAction
    from creation_lib.nif.operations.sanitize import sanitize_links, reorder_blocks
    from creation_lib.nif.operations.copy import copy_blocks

    target = app.nif
    if target is None:
        return BashResult(error="No NIF file loaded")

    if not source.blocks:
        return BashResult(error="Source NIF has no blocks")

    source_root = source.get_block(0)
    if source_root is None or not source.schema.is_subtype_of(source_root.type_name, "NiNode"):
        return BashResult(error="Source NIF root is not an NiNode subtype")

    # Snapshot for undo
    cmd = SnapshotAction(_description="Bash NIF")
    cmd.capture_before(target)

    merged: list[str] = []
    skipped: list[str] = []
    blocks_before = len(target.blocks)

    # Gather all parts from source root
    parts = _gather_source_parts(source)

    # --- Create wrapper NiNode under target root ---
    node_name = os.path.splitext(os.path.basename(source_path))[0] if source_path else ""
    if not node_name:
        # Fall back to source root's Name
        root_name = source_root.get_field("Name") or ""
        if isinstance(root_name, list):
            root_name = "".join(str(c) for c in root_name)
        node_name = root_name or "BashedNode"

    wrapper = target.add_block("NiNode", {"Name": node_name})
    wrapper_bid = wrapper.block_id
    _add_to_children(target, 0, wrapper_bid)
    merged.append(f'Created NiNode "{node_name}"')

    # --- Geometry (under wrapper node) ---
    geometry_id_map: dict[int, int] = {}
    if parts["geometry"]:
        geometry_id_map = copy_blocks(source, parts["geometry"], target,
                                      attach_to=wrapper_bid)
        merged.append(f"Added {len(parts['geometry'])} geometry branch(es)")

    # --- Collision (copy onto wrapper node) ---
    if parts["collision_id"] is not None:
        col_map = copy_blocks(source, [parts["collision_id"]], target)
        new_col_id = col_map.get(parts["collision_id"])
        if new_col_id is not None:
            wrapper.set_field("Collision Object", new_col_id)
            merged.append("Copied collision data")

    # --- Connect Points (merge at root level, in Extra Data List) ---
    for s_bid in parts["connect_points"]:
        s_block = source.get_block(s_bid)
        if s_block is None:
            continue
        cp_type = s_block.type_name
        t_bid = _find_block_on_root(target, cp_type)

        if t_bid is not None:
            if cp_type == "BSConnectPoint::Parents":
                msg = _merge_connect_point_parents(target, t_bid, source, s_bid)
            else:
                msg = _merge_connect_point_children(target, t_bid, source, s_bid)
            if msg:
                merged.append(msg)
        else:
            # No existing block — copy and add to root's Extra Data List
            cp_map = copy_blocks(source, [s_bid], target)
            new_cp_bid = cp_map.get(s_bid)
            if new_cp_bid is not None:
                _add_to_extra_data_list(target, 0, new_cp_bid)
            merged.append(f"Copied {cp_type}")

    # --- Root Extra Data (merge at root level) ---
    for s_bid in parts["extra_data"]:
        s_block = source.get_block(s_bid)
        if s_block is None:
            continue
        tname = s_block.type_name

        if tname == "BSXFlags":
            t_bid = _find_block_on_root(target, "BSXFlags")
            if t_bid is not None:
                msg = _merge_bsx_flags(target, t_bid, source, s_bid)
                if msg:
                    merged.append(msg)
            else:
                ed_map = copy_blocks(source, [s_bid], target)
                new_bid = ed_map.get(s_bid)
                if new_bid is not None:
                    _add_to_extra_data_list(target, 0, new_bid)
                merged.append("Copied BSXFlags")

        elif tname == "BSBehaviorGraphExtraData":
            t_bid = _find_block_on_root(target, "BSBehaviorGraphExtraData")
            if t_bid is not None:
                skipped.append("Skipped BSBehaviorGraphExtraData (already exists)")
            else:
                ed_map = copy_blocks(source, [s_bid], target)
                new_bid = ed_map.get(s_bid)
                if new_bid is not None:
                    _add_to_extra_data_list(target, 0, new_bid)
                merged.append("Copied BSBehaviorGraphExtraData")

        elif tname == "NiDefaultAVObjectPalette":
            pass  # Handled in animation merge

        else:
            t_bid = _find_block_on_root(target, tname)
            if t_bid is not None:
                skipped.append(f"Skipped {tname} (already exists)")
            else:
                ed_map = copy_blocks(source, [s_bid], target)
                new_bid = ed_map.get(s_bid)
                if new_bid is not None:
                    _add_to_extra_data_list(target, 0, new_bid)
                merged.append(f"Copied {tname}")

    # --- Animations (merge at root level) ---
    anim_msgs = _merge_animations(target, source, parts["animations"],
                                  geometry_id_map)
    merged.extend(anim_msgs)

    # --- Post-processing ---
    sanitize_links(target)
    reorder_blocks(target)

    blocks_added = len(target.blocks) - blocks_before

    # Push undo
    cmd.capture_after(target)
    app.undo_manager.push(app.registry.active_id, cmd)
    app._nif_dirty = True

    for msg in merged:
        _log.info("Bash: %s", msg)
    for msg in skipped:
        _log.info("Bash: %s", msg)

    return BashResult(
        blocks_added=blocks_added,
        merged=merged,
        skipped=skipped,
    )
