"""Extract Camera and Weapon bone world positions from the 1st-person skeleton NIF.

Uses the same parent-chain walk pattern as connect_point_display.py.
"""
from __future__ import annotations

import logging
from pathlib import Path

import numpy as np

from app.paths import get_code_root as _get_code_root

_log = logging.getLogger("aligner.skeleton")

# Default skeleton path (bundled in repo)
_SKELETON_NIF = _get_code_root() / "resource" / "skeleton.nif"


def _extract_ref(ref) -> int:
    """Extract block index from a reference value."""
    if isinstance(ref, (int, float)):
        return int(ref)
    if isinstance(ref, dict):
        return int(ref.get("value", ref.get("Value", -1)))
    return -1


def _build_parent_map(nif) -> dict[int, int]:
    """Build child→parent block-id mapping from NiNode Children refs."""
    parent_map = {}
    for block in nif.blocks:
        if not nif.schema.is_subtype_of(block.type_name, "NiNode"):
            continue
        children = block.get_field("Children") or []
        for ref in children:
            ref_id = _extract_ref(ref)
            if ref_id >= 0:
                parent_map[ref_id] = block.block_id
    return parent_map


def _compute_world_position(nif, block_id: int, parent_map: dict) -> tuple[float, float, float]:
    """Compute world-space position by accumulating transforms up the parent chain."""
    chain = []
    bid = block_id
    while bid is not None:
        chain.append(bid)
        bid = parent_map.get(bid)
    chain.reverse()

    wx, wy, wz = 0.0, 0.0, 0.0
    cum_rot = np.eye(3, dtype=np.float64)
    cum_scale = 1.0

    for bid in chain:
        block = nif.get_block(bid)
        if not block:
            continue

        trans = block.get_field("Translation") or {}
        tx = float(trans.get("x", 0))
        ty = float(trans.get("y", 0))
        tz = float(trans.get("z", 0))

        scale = float(block.get_field("Scale") or 1.0)

        local = np.array([tx, ty, tz])
        world_t = cum_rot @ (local * cum_scale) + np.array([wx, wy, wz])
        wx, wy, wz = world_t

        rot = block.get_field("Rotation") or {}
        local_rot = np.array([
            [float(rot.get("m11", 1)), float(rot.get("m21", 0)), float(rot.get("m31", 0))],
            [float(rot.get("m12", 0)), float(rot.get("m22", 1)), float(rot.get("m32", 0))],
            [float(rot.get("m13", 0)), float(rot.get("m23", 0)), float(rot.get("m33", 1))],
        ])
        cum_rot = cum_rot @ local_rot
        cum_scale *= scale

    return (wx, wy, wz)


def _find_bone_by_name(nif, name: str) -> int | None:
    """Find an NiNode block by name, return block_id or None."""
    for block in nif.blocks:
        if nif.schema.is_subtype_of(block.type_name, "NiNode"):
            block_name = block.get_field("Name") or ""
            if block_name == name:
                return block.block_id
    return None


def load_skeleton_bones(skeleton_nif_path: str | None = None) -> dict:
    """Load skeleton NIF and extract Camera/Weapon bone world positions.

    Returns dict with:
        camera_pos: (x, y, z) — Camera bone world position
        weapon_pos: (x, y, z) — Weapon bone world position
        aim_direction: (x, y, z) — normalized aim vector (forward from weapon)
    """
    from creation_lib.nif.nif_file import NifFile

    path = Path(skeleton_nif_path) if skeleton_nif_path else _SKELETON_NIF
    if not path.exists():
        raise FileNotFoundError(f"Skeleton NIF not found: {path}")

    nif = NifFile.load(str(path))
    parent_map = _build_parent_map(nif)

    camera_id = _find_bone_by_name(nif, "Camera")
    if camera_id is None:
        _log.warning("Camera bone not found in skeleton, using default position")
        camera_pos = (0.0, 0.0, 120.48)
    else:
        camera_pos = _compute_world_position(nif, camera_id, parent_map)
        _log.info("Camera bone world pos: (%.2f, %.2f, %.2f)", *camera_pos)

    # Find Weapon bone — try "Weapon" first (HKX skeleton), then "RArm_Hand" (NIF skeleton)
    weapon_id = _find_bone_by_name(nif, "Weapon")
    if weapon_id is None:
        weapon_id = _find_bone_by_name(nif, "RArm_Hand")
    if weapon_id is None:
        _log.warning("Weapon bone not found in skeleton, using default position")
        weapon_pos = (0.0, 0.0, 100.0)
    else:
        weapon_pos = _compute_world_position(nif, weapon_id, parent_map)
        _log.info("Weapon bone world pos: (%.2f, %.2f, %.2f)", *weapon_pos)

    # Aim direction: forward from weapon bone (positive Y in Fallout 4's coordinate system)
    aim_dir = np.array([0.0, 1.0, 0.0])
    length = np.linalg.norm(aim_dir)
    if length > 0:
        aim_dir = aim_dir / length

    return {
        "camera_pos": camera_pos,
        "weapon_pos": weapon_pos,
        "aim_direction": tuple(aim_dir),
    }


def load_sighted_pose_bones(hkx_path: str) -> dict:
    """Load a sighted animation HKX and return Camera/Weapon bone data.

    Unpacks the HKX, parses frame 0 bone transforms using the HKX skeleton's
    bone hierarchy and the animation's rotation data, then returns the same
    dict format as load_skeleton_bones() plus world rotations.

    Returns dict with:
        camera_pos: (x, y, z) — Camera bone world position at sighted frame 0
        weapon_pos: (x, y, z) — Weapon bone world position at sighted frame 0
        weapon_rot: 3x3 numpy array — Weapon bone world rotation matrix
        aim_direction: (x, y, z) — normalized forward vector (positive Y)
    """
    from .animation_loader import load_sighted_pose

    positions, rotations = load_sighted_pose(hkx_path)

    camera_pos = positions.get("Camera", (0.0, 0.0, 120.48))
    weapon_pos = positions.get("Weapon", positions.get("RArm_Hand", (0.0, 0.0, 100.0)))

    # Get Weapon bone world rotation (includes full parent chain)
    weapon_rot = rotations.get("Weapon", rotations.get("RArm_Hand"))

    _log.info("Sighted pose — Camera: (%.2f, %.2f, %.2f), Weapon: (%.2f, %.2f, %.2f)",
              *camera_pos, *weapon_pos)
    if weapon_rot is not None:
        _log.info("Weapon bone world rotation:\n%s", weapon_rot)

    return {
        "camera_pos": camera_pos,
        "weapon_pos": weapon_pos,
        "weapon_rot": weapon_rot,
        "aim_direction": (0.0, 1.0, 0.0),
    }
