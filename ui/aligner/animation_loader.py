"""Parse HKX animation files to extract frame 0 bone poses.

Uses creation_lib.hkxpack to unpack HKX → XML (pure Python, no Java),
then parses the animation format to get bone transforms.

Strategy:
- Translations: use skeleton reference pose (bone lengths don't change)
- COM translation: extracted from dynamicTranslations (the only dynamic bone)
- Rotations: decoded from animation staticRotations/dynamicRotations
  using the clean type+offset encoding (value & 3 = type, value >> 2 = index)
- World positions computed using skeleton parent indices
"""
from __future__ import annotations

import logging
import math
import os
import re
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

import numpy as np

from app.paths import get_code_root as _get_code_root

_log = logging.getLogger("aligner.animation")

_SKELETON_XML = _get_code_root() / "resource" / "skeleton.xml"


def _quat_to_matrix(q: list[float]) -> np.ndarray:
    """Convert quaternion (x, y, z, w) to 3x3 rotation matrix."""
    x, y, z, w = q
    return np.array([
        [1 - 2*(y*y + z*z), 2*(x*y - z*w),     2*(x*z + y*w)],
        [2*(x*y + z*w),     1 - 2*(x*x + z*z), 2*(y*z - x*w)],
        [2*(x*z - y*w),     2*(y*z + x*w),     1 - 2*(x*x + y*y)],
    ], dtype=np.float64)


def _parse_skeleton_xml(path: Path) -> dict:
    """Parse skeleton.xml for bone names, parent indices, and reference pose."""
    tree = ET.parse(str(path))
    for obj in tree.iter("hkobject"):
        if obj.get("class") != "hkaSkeleton":
            continue

        pi_el = obj.find('.//hkparam[@name="parentIndices"]')
        parent_indices = [int(x) for x in pi_el.text.split()]

        bone_names = []
        bones_param = obj.find('.//hkparam[@name="bones"]')
        for bone_obj in bones_param.findall("hkobject"):
            name_el = bone_obj.find('hkparam[@name="name"]')
            bone_names.append(name_el.text if name_el.text else "")

        # Reference pose: (tx ty tz tw)(qx qy qz qw)(sx sy sz sw) per bone
        rp_el = obj.find('.//hkparam[@name="referencePose"]')
        tuples = re.findall(r"\(([^)]+)\)", rp_el.text)
        ref_poses = []
        for i in range(0, len(tuples), 3):
            t = [float(x) for x in tuples[i].split()]
            q = [float(x) for x in tuples[i + 1].split()]
            s = [float(x) for x in tuples[i + 2].split()]
            ref_poses.append({"t": t[:3], "q": q[:4], "s": s[:3]})

        return {
            "bone_names": bone_names,
            "parent_indices": parent_indices,
            "ref_poses": ref_poses,
        }
    raise ValueError("No hkaSkeleton found in skeleton XML")


def _parse_animation_xml(path: Path) -> dict:
    """Parse animation XML for frame 0 rotation data."""
    tree = ET.parse(str(path))
    for obj in tree.iter("hkobject"):
        if obj.get("class") != "hkaLosslessCompressedAnimation":
            continue

        num_frames = int(obj.find('.//hkparam[@name="numFrames"]').text)

        # Static rotations (quaternions in parentheses)
        sr_el = obj.find('.//hkparam[@name="staticRotations"]')
        sr_tuples = re.findall(r"\(([^)]+)\)", sr_el.text)
        static_rots = [[float(x) for x in t.split()] for t in sr_tuples]

        dr_el = obj.find('.//hkparam[@name="dynamicRotations"]')
        dr_tuples = re.findall(r"\(([^)]+)\)", dr_el.text)
        dyn_rots = [[float(x) for x in t.split()] for t in dr_tuples]

        # Rotation type+offset array
        rto_el = obj.find('.//hkparam[@name="rotationTypeAndOffsets"]')
        rot_tao = [int(x) for x in rto_el.text.split()]

        # Dynamic translations (for COM bone)
        dt_el = obj.find('.//hkparam[@name="dynamicTranslations"]')
        dyn_trans = [float(x) for x in dt_el.text.split()]

        # Translation type+offset array
        tto_el = obj.find('.//hkparam[@name="translationTypeAndOffsets"]')
        trans_tao = [int(x) for x in tto_el.text.split()]

        st_el = obj.find('.//hkparam[@name="staticTranslations"]')
        static_trans = [float(x) for x in st_el.text.split()]

        return {
            "num_frames": num_frames,
            "static_rots": static_rots,
            "dyn_rots": dyn_rots,
            "rot_tao": rot_tao,
            "dyn_trans": dyn_trans,
            "trans_tao": trans_tao,
            "static_trans": static_trans,
        }
    raise ValueError("No hkaLosslessCompressedAnimation found in animation XML")


def _parse_spline_animation_xml(path: Path) -> dict:
    """Parse hkaSplineCompressedAnimation XML for frame 0 data."""
    tree = ET.parse(str(path))
    for obj in tree.iter("hkobject"):
        if obj.get("class") != "hkaSplineCompressedAnimation":
            continue

        def _int_param(name: str, default: int = 0) -> int:
            param = obj.find(f'.//hkparam[@name="{name}"]')
            if param is not None and param.text:
                return int(param.text.strip())
            return default

        def _float_param(name: str, default: float = 0.0) -> float:
            param = obj.find(f'.//hkparam[@name="{name}"]')
            if param is not None and param.text:
                return float(param.text.strip())
            return default

        def _int_array(name: str) -> list[int]:
            param = obj.find(f'.//hkparam[@name="{name}"]')
            if param is not None and param.text:
                return [int(x) for x in param.text.split() if x]
            return []

        num_frames = _int_param("numFrames")
        num_tracks = _int_param("numberOfTransformTracks")
        num_float_tracks = _int_param("numberOfFloatTracks")
        num_blocks = _int_param("numBlocks", 1)
        max_frames_per_block = _int_param("maxFramesPerBlock", 256)
        mask_and_quant_size = _int_param(
            "maskAndQuantizationSize",
            ((num_tracks * 4) + num_float_tracks + 3) & ~3,
        )
        block_duration = _float_param("blockDuration")
        block_inverse_duration = _float_param("blockInverseDuration")
        frame_duration = _float_param("frameDuration")

        # Block offsets
        block_offsets = _int_array("blockOffsets") or [0]
        float_block_offsets = _int_array("floatBlockOffsets")

        # Raw data bytes
        data_el = obj.find('.//hkparam[@name="data"]')
        raw_bytes = bytes([int(x) for x in data_el.text.split()])

        return {
            "format": "spline",
            "num_frames": num_frames,
            "num_tracks": num_tracks,
            "num_float_tracks": num_float_tracks,
            "num_blocks": num_blocks,
            "max_frames_per_block": max_frames_per_block,
            "mask_and_quant_size": mask_and_quant_size,
            "block_duration": block_duration,
            "block_inverse_duration": block_inverse_duration,
            "frame_duration": frame_duration,
            "block_offsets": block_offsets,
            "float_block_offsets": float_block_offsets,
            "data": raw_bytes,
        }
    raise ValueError("No hkaSplineCompressedAnimation found in animation XML")


def _parse_any_animation_xml(path: Path) -> dict:
    """Try to parse either lossless or spline compressed animation."""
    tree = ET.parse(str(path))
    for obj in tree.iter("hkobject"):
        cls = obj.get("class", "")
        if cls == "hkaLosslessCompressedAnimation":
            return _parse_animation_xml(path)
        if cls == "hkaSplineCompressedAnimation":
            return _parse_spline_animation_xml(path)
    raise ValueError("No supported animation class found in XML "
                     "(expected hkaLosslessCompressedAnimation or hkaSplineCompressedAnimation)")


def unpack_hkx(hkx_path: Path) -> Path:
    """Unpack HKX to temporary XML file.

    Caller is responsible for deleting the returned temp file.
    """
    from creation_lib._native.havok_native import hkx_to_xml
    xml = hkx_to_xml(hkx_path.read_bytes())
    fd, tmp_path = tempfile.mkstemp(suffix=".xml", prefix="hkx_unpack_")
    import io
    with io.open(fd, "w", encoding="utf-8") as f:
        f.write(xml)
    return Path(tmp_path)


def _get_frame0_rotations(anim: dict, num_bones: int) -> list[list[float] | None]:
    """Extract frame 0 rotation quaternion for each bone from animation data.

    Returns list of quaternions [x,y,z,w] or None (use reference pose).
    Encoding: value & 3 = type (0=identity, 1=static, 2=dynamic), value >> 2 = index.
    """
    rotations: list[list[float] | None] = [None] * num_bones
    num_frames = anim["num_frames"]

    for bone_idx, packed in enumerate(anim["rot_tao"]):
        if bone_idx >= num_bones:
            break
        typ = packed & 3
        offset = packed >> 2

        if typ == 0:
            # Identity — use reference pose
            continue
        elif typ == 1:
            # Static rotation
            if offset < len(anim["static_rots"]):
                rotations[bone_idx] = anim["static_rots"][offset]
        elif typ == 2:
            # Dynamic — frame 0 is at offset * num_frames
            frame0_idx = offset * num_frames
            if frame0_idx < len(anim["dyn_rots"]):
                rotations[bone_idx] = anim["dyn_rots"][frame0_idx]

    return rotations


def _get_frame0_translations(anim: dict, num_bones: int) -> list[list[float] | None]:
    """Extract frame 0 translations where reliably decodable.

    Only handles:
    - Type 0 (identity) → use reference pose (return None)
    - Type 2 (dynamic) → first entry in dynamicTranslations for that bone
    - Type 1 (static) with small offsets → staticTranslations vec3

    Returns list of [x,y,z] or None (use reference pose).
    """
    translations: list[list[float] | None] = [None] * num_bones
    num_frames = anim["num_frames"]
    static = anim["static_trans"]
    num_static_vec3s = len(static) // 3

    for bone_idx, packed in enumerate(anim["trans_tao"]):
        if bone_idx >= num_bones:
            break
        typ = packed & 3
        offset = packed >> 2

        if typ == 0:
            # Identity — use reference pose
            continue
        elif typ == 1:
            # Static — only use if offset is within bounds as vec3 index
            if offset < num_static_vec3s:
                base = offset * 3
                translations[bone_idx] = [static[base], static[base + 1], static[base + 2]]
            # else: offset too large (complex packed encoding), fall back to ref pose
        elif typ == 2:
            # Dynamic — frame 0
            frame0_float = offset * num_frames * 3
            if frame0_float + 2 < len(anim["dyn_trans"]):
                translations[bone_idx] = [
                    anim["dyn_trans"][frame0_float],
                    anim["dyn_trans"][frame0_float + 1],
                    anim["dyn_trans"][frame0_float + 2],
                ]

    return translations


def _compute_world_transforms(
    skeleton: dict,
    anim_rotations: list[list[float] | None],
    anim_translations: list[list[float] | None],
) -> tuple[dict[str, tuple[float, float, float]], dict[str, np.ndarray]]:
    """Compute world-space positions and rotations for all bones using parent chain walk.

    Uses animation rotations/translations where available, otherwise reference pose.

    Returns:
        (world_positions, world_rotations) where:
        - world_positions maps bone_name → (x, y, z) world position
        - world_rotations maps bone_name → 3x3 rotation matrix (numpy)
    """
    bone_names = skeleton["bone_names"]
    parent_indices = skeleton["parent_indices"]
    ref_poses = skeleton["ref_poses"]
    num_bones = len(bone_names)

    # Build local transforms for each bone
    local_translations = []
    local_rotations = []
    for i in range(num_bones):
        # Translation: animation override or reference pose
        if anim_translations[i] is not None:
            t = anim_translations[i]
        else:
            t = ref_poses[i]["t"]
        local_translations.append(np.array(t, dtype=np.float64))

        # Rotation: animation override or reference pose
        if anim_rotations[i] is not None:
            q = anim_rotations[i]
        else:
            q = ref_poses[i]["q"]
        local_rotations.append(_quat_to_matrix(q))

    # Compute world transforms via parent chain
    world_positions = {}
    world_rotations = {}
    world_trans = [None] * num_bones
    world_rots = [None] * num_bones

    # Process in order (parents before children)
    for i in range(num_bones):
        parent = parent_indices[i]
        if parent < 0 or parent >= num_bones or parent == 65535:
            # Root bone
            world_trans[i] = local_translations[i].copy()
            world_rots[i] = local_rotations[i].copy()
        else:
            # world_pos = parent_rot @ (local_trans * parent_scale) + parent_trans
            # Ignoring scale for simplicity (scales are ~1.0 in practice)
            world_trans[i] = world_rots[parent] @ local_translations[i] + world_trans[parent]
            world_rots[i] = world_rots[parent] @ local_rotations[i]

        world_positions[bone_names[i]] = tuple(world_trans[i])
        world_rotations[bone_names[i]] = world_rots[i]

    return world_positions, world_rotations


def load_sighted_pose(
    hkx_path: str,
    skeleton_xml_path: str | None = None,
) -> tuple[dict[str, tuple[float, float, float]], dict[str, "np.ndarray"]]:
    """Load a sighted animation HKX and compute frame 0 world-space bone transforms.

    Returns:
        (world_positions, world_rotations) where:
        - world_positions: dict mapping bone name → (x, y, z) world position at frame 0
        - world_rotations: dict mapping bone name → 3x3 rotation matrix (numpy)
    """
    skel_path = Path(skeleton_xml_path) if skeleton_xml_path else _SKELETON_XML
    skeleton = _parse_skeleton_xml(skel_path)
    num_bones = len(skeleton["bone_names"])

    _log.info("Unpacking %s ...", hkx_path)
    tmp_xml = unpack_hkx(Path(hkx_path))
    try:
        anim = _parse_any_animation_xml(tmp_xml)
    finally:
        tmp_xml.unlink(missing_ok=True)

    if anim.get("format") == "spline":
        # Spline compressed — use the shared Havok decompressor so the
        # bone editor and the rest of the pipeline interpret the block
        # layout the same way.
        _log.info("Spline compressed animation: %d frames, %d tracks",
                  anim["num_frames"], anim["num_tracks"])
        import json as _json
        import types as _types
        from creation_lib._native import havok_native as _hn

        _raw_json = _hn.havok_decompress_spline(
            anim["data"],
            anim["num_tracks"],
            anim["num_float_tracks"],
            1,
            anim["max_frames_per_block"],
            anim["num_blocks"],
            anim["block_offsets"],
            anim["float_block_offsets"],
            anim["mask_and_quant_size"],
            anim["block_duration"],
            anim["block_inverse_duration"],
            anim["frame_duration"],
        )
        _raw_frames = _json.loads(_raw_json)
        frame0 = [
            _types.SimpleNamespace(
                translation=tuple(t["translation"]),
                rotation=tuple(t["rotation"]),
                scale=tuple(t["scale"]),
            )
            for t in (_raw_frames[0] if _raw_frames else [])
        ]
        rotations = [list(f.rotation) for f in frame0] + [None] * max(0, num_bones - len(frame0))
        translations = [list(f.translation) for f in frame0] + [None] * max(0, num_bones - len(frame0))
    else:
        # Lossless compressed
        _log.info("Lossless animation: %d frames, %d rotation tracks",
                  anim["num_frames"], len(anim["rot_tao"]))
        rotations = _get_frame0_rotations(anim, num_bones)
        translations = _get_frame0_translations(anim, num_bones)

    world_positions, world_rotations = _compute_world_transforms(skeleton, rotations, translations)

    for name in ["Camera", "Weapon", "RArm_Hand", "COM"]:
        if name in world_positions:
            pos = world_positions[name]
            _log.info("  %s: (%.2f, %.2f, %.2f)", name, *pos)

    return world_positions, world_rotations
