"""Collision block summary helpers for the scene tree.

Inspects Havok/collision blocks (bhk*) and produces a short label suffix
plus human-readable detail lines for tooltip display.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass

_log = logging.getLogger("nif_editor.collision_info")

try:
    from creation_lib.nif.operations.collision_materials import (
        collision_material_type_name,
        format_collision_material,
    )
except Exception:  # pragma: no cover - editor can run before creation_lib is installed
    collision_material_type_name = None
    format_collision_material = None

# Cache for parsed packfile summaries: (block_id, blob_len, blob_hash) -> dict | None
_packfile_cache: dict[tuple[int, int, int], dict | None] = {}
_preview_cache: dict[tuple[int, int, int, int], list[dict]] = {}


@dataclass(frozen=True)
class HavokShapeInfo:
    """One selectable shape decoded from a bhkPhysicsSystem packfile."""

    body_id: int
    shape_index: int | None
    class_name: str
    display_type: str
    layer: str | None
    materials: tuple[str, ...]
    material_types: tuple[str, ...]
    vertex_count: int | None = None
    triangle_count: int | None = None
    sub_shapes: tuple["HavokShapeInfo", ...] = ()


def _parse_packfile_summary(blob: bytes, block_id: int = -1) -> dict | None:
    """Parse a Havok 2014 packfile blob and return a summary dict.

    Returns None on parse failure so callers can fall back to raw size display.
    Dict shape:
        shape_kind: "convex_polytope" | "compound_polytope" | "compound_mesh" | "compressed_mesh" | "unknown"
        objects: [{"class_name": str, "n_vertices": int|None, "n_faces": int|None,
                   "n_planes": int|None, "n_instances": int|None}]
        n_subshapes: int | None
        blob_size: int
    """
    cache_key = (block_id, len(blob), hash(blob))
    if cache_key in _packfile_cache:
        return _packfile_cache[cache_key]

    result = None
    try:
        import json as _json
        from creation_lib._native import havok_native
        result = _json.loads(havok_native.havok_collision_summary(blob))
    except Exception:
        _log.debug("Failed to parse bhkPhysicsSystem packfile blob", exc_info=True)
        result = None

    _packfile_cache[cache_key] = result
    return result


# Prefixes/names that identify a collision/Havok block.
_COLLISION_PREFIXES = ("bhk",)
_EXTRA_COLLISION_TYPES = frozenset({"bhkPhysicsSystem"})

# FO4/Skyrim Havok collision layer names (low byte of Havok Filter).
_LAYER_NAMES: dict[int, str] = {
    0: "UNIDENTIFIED",
    1: "STATIC",
    2: "ANIMSTATIC",
    3: "TRANSPARENT",
    4: "CLUTTER",
    5: "WEAPON",
    6: "PROJECTILE",
    7: "SPELL",
    8: "BIPED",
    9: "TREES",
    10: "PROPS",
    11: "WATER",
    12: "TRIGGER",
    13: "TERRAIN",
    14: "TRAP",
    15: "NONCOLLIDABLE",
    16: "CLOUD_TRAP",
    17: "GROUND",
    18: "PORTAL",
    19: "DEBRIS_SMALL",
    20: "DEBRIS_LARGE",
    21: "ACOUSTIC_SPACE",
    22: "ACTORZONE",
    23: "PROJECTILEZONE",
    24: "GASBAG",
    25: "SHELLCASING",
    26: "TRANSPARENT_SMALL",
    27: "INVISIBLE_WALL",
    28: "TRANSPARENT_SMALL_ANIM",
    29: "WARD",
    30: "CHARCONTROLLER",
    31: "STAIRHELPER",
    32: "DEADBIP",
    33: "BIPED_NO_CC",
    34: "AVOIDBOX",
    35: "COLLISIONBOX",
    36: "CAMERASPHERE",
    37: "DOORDETECTION",
    38: "CONEPROJECTILE",
    39: "CAMERAPICK",
    40: "ITEMPICK",
    41: "LINEOFSIGHT",
    42: "PATHPICK",
    43: "CUSTOMPICK1",
    44: "CUSTOMPICK2",
    45: "SPELLEXPLOSION",
    46: "DROPPINGPICK",
}

# Havok motion type IDs.
_MOTION_TYPE_NAMES: dict[int, str] = {
    0: "INVALID",
    1: "DYNAMIC",
    2: "SPHERE_INERTIA",
    3: "SPHERE_INERTIA_2",
    4: "BOX_INERTIA",
    5: "BOX_INERTIA_2",
    6: "KEYFRAMED",
    7: "FIXED",
    8: "THIN_BOX_INERTIA",
    9: "CHARACTER",
}

# Havok quality type IDs.
_QUALITY_TYPE_NAMES: dict[int, str] = {
    0: "INVALID",
    1: "FIXED",
    2: "KEYFRAMED",
    3: "DEBRIS",
    4: "MOVING",
    5: "CRITICAL",
    6: "BULLET",
    7: "USER",
    8: "CHARACTER",
    9: "KEYFRAMED_REPORT",
}

# bhkCollisionObject.Flags bit meanings (from nif.xml bhkCOFlags enum).
_COLL_OBJ_FLAGS: dict[int, str] = {
    0x001: "ACTIVE",
    0x002: "NOTIFY",
    0x004: "SET_LOCAL",
    0x008: "DBD_LINEAR",
    0x010: "RESET_POS",
    0x020: "SYNC_ON_UPDATE",
    0x040: "ANIM_TARGETED",
    0x080: "USE_ABV",
}


def is_collision_block(type_name: str) -> bool:
    """True if the block type is a Havok/collision block."""
    if not type_name:
        return False
    if type_name in _EXTRA_COLLISION_TYPES:
        return True
    return any(type_name.startswith(p) for p in _COLLISION_PREFIXES)


def _as_int(value, default: int | None = None) -> int | None:
    """Coerce ints, floats, or refs to int; return default on failure."""
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, (int, float)):
        return int(value)
    if isinstance(value, dict):
        for key in ("value", "Value", "block_id"):
            if key in value:
                try:
                    return int(value[key])
                except (TypeError, ValueError):
                    return default
    return default


def _get(block, field_name: str, default=None):
    """Safe field accessor — tolerant of missing blocks/fields."""
    try:
        val = block.get_field(field_name)
    except Exception:
        return default
    if val is None:
        return default
    return val


def _material_int(value) -> int | None:
    """Material field can come back as a wrapping dict or plain int."""
    if isinstance(value, dict):
        for key in ("Material", "value", "Value"):
            if key in value:
                return _as_int(value[key])
        return None
    return _as_int(value)


def _material_label(value) -> str | None:
    mat = _material_int(value)
    if mat is None:
        return None
    if format_collision_material is not None:
        try:
            return format_collision_material(mat)
        except Exception:
            pass
    return f"0x{mat:08X} ({mat})"


def _material_type_label(value) -> str | None:
    mat = _material_int(value)
    if mat is None or collision_material_type_name is None:
        return None
    try:
        return collision_material_type_name(mat)
    except Exception:
        return None


def _append_material_lines(lines: list[str], value) -> None:
    material = _material_label(value)
    if material:
        lines.append(f"Material: {material}")
    material_type = _material_type_label(value)
    if material_type:
        lines.append(f"Material Type: {material_type}")


def _fmt_v3(vec, precision: int = 3) -> str:
    """Format a {x,y,z} vector as '(x.xxx, y.yyy, z.zzz)'."""
    if not isinstance(vec, dict):
        return str(vec)
    x = float(vec.get("x", 0.0))
    y = float(vec.get("y", 0.0))
    z = float(vec.get("z", 0.0))
    return f"({x:.{precision}f}, {y:.{precision}f}, {z:.{precision}f})"


def _layer_from_filter(havok_filter) -> int | None:
    """Extract Layer (low byte) from a Havok Filter field (struct or int)."""
    if isinstance(havok_filter, dict):
        if "Layer" in havok_filter:
            return _as_int(havok_filter.get("Layer"))
        if "Layer & Flags" in havok_filter:
            combined = _as_int(havok_filter.get("Layer & Flags"))
            if combined is not None:
                return combined & 0xFF
    v = _as_int(havok_filter)
    if v is None:
        return None
    return v & 0xFF


def _layer_label(layer) -> str | None:
    layer_id = _as_int(layer)
    if layer_id is None:
        return None
    return f"{_LAYER_NAMES.get(layer_id, f'LAYER_{layer_id}')} ({layer_id})"


def _ref_label(value) -> str | None:
    """Render a Ref/Ptr value as a block-id string, or None if unset."""
    rid = _as_int(value, default=-1)
    if rid is None or rid < 0:
        return None
    return f"#{rid}"


def _shape_class_label(shape_class: str | None) -> str | None:
    if shape_class == "hknpCompressedMeshShape":
        return "compressed mesh"
    if shape_class == "hknpConvexPolytopeShape":
        return "convex polytope"
    if shape_class == "hknpDynamicCompoundShape":
        return "compound"
    return shape_class


def _body_shape_from_summary(summary: dict | None, body_id: int | None) -> str | None:
    if summary is None or body_id is None:
        return None
    for body in summary.get("bodies") or []:
        if _as_int(body.get("body_id")) == body_id:
            return _shape_class_label(body.get("shape_class"))
    return None


def _body_layer_from_summary(summary: dict | None, body_id: int | None) -> str | None:
    if summary is None or body_id is None:
        return None
    for body in summary.get("bodies") or []:
        if _as_int(body.get("body_id")) == body_id:
            return _layer_label(body.get("layer"))
    return None


def _body_material_from_summary(summary: dict | None, body_id: int | None) -> str | None:
    if summary is None or body_id is None:
        return None
    for body in summary.get("bodies") or []:
        if _as_int(body.get("body_id")) == body_id:
            return _material_label(body.get("material_crc"))
    return None


def _body_material_type_from_summary(summary: dict | None, body_id: int | None) -> str | None:
    if summary is None or body_id is None:
        return None
    for body in summary.get("bodies") or []:
        if _as_int(body.get("body_id")) == body_id:
            return _material_type_label(body.get("material_crc"))
    return None


def _body_label_from_summary(summary: dict | None, body_id: int | None) -> str | None:
    body_shape = _body_shape_from_summary(summary, body_id)
    body_layer = _body_layer_from_summary(summary, body_id)
    body_material = _body_material_from_summary(summary, body_id)
    parts = [part for part in (body_shape, body_layer, body_material) if part]
    return ", ".join(parts) if parts else None


def _physics_system_blob(block) -> bytes | None:
    binary = _get(block, "Binary Data")
    raw = binary.get("Data") if isinstance(binary, dict) else None
    if isinstance(raw, (bytes, bytearray)):
        return bytes(raw)
    if isinstance(raw, list) and raw:
        try:
            return bytes(raw)
        except (TypeError, ValueError):
            return None
    return None


def _parse_packfile_previews(
    blob: bytes,
    block_id: int,
    body_id: int,
) -> list[dict]:
    cache_key = (block_id, len(blob), hash(blob), body_id)
    cached = _preview_cache.get(cache_key)
    if cached is not None:
        return cached
    previews: list[dict] = []
    try:
        from creation_lib.havok.collision_preview import (
            extract_preview_meshes_from_blob,
        )

        previews = extract_preview_meshes_from_blob(
            blob, havok_scale=1.0, body_id=body_id
        )
    except Exception:
        _log.debug("Failed to decode bhkPhysicsSystem preview shapes", exc_info=True)
    _preview_cache[cache_key] = previews
    return previews


_PREVIEW_CLASS_NAMES = {
    "box": "hknpBoxShape",
    "capsule": "hknpCapsuleShape",
    "compressed_mesh": "hknpCompressedMeshShape",
    "convex_hull": "hknpConvexPolytopeShape",
    "sphere": "hknpSphereShape",
}


def _display_shape_type(class_name: str | None) -> str:
    labels = {
        "hknpBoxShape": "Box",
        "hknpCapsuleShape": "Capsule",
        "hknpCompressedMeshShape": "Compressed Mesh",
        "hknpCompoundShape": "Compound",
        "hknpConvexPolytopeShape": "Convex Polytope",
        "hknpDynamicCompoundShape": "Dynamic Compound",
        "hknpSphereShape": "Sphere",
        "hknpStaticCompoundShape": "Static Compound",
    }
    if not class_name:
        return "Unknown Shape"
    return labels.get(class_name, class_name)


def _body_material_labels(body: dict) -> tuple[tuple[str, ...], tuple[str, ...]]:
    values = []
    primary = _as_int(body.get("material_crc"))
    if primary is not None:
        values.append(primary)
    for material in body.get("bs_materials") or []:
        if not isinstance(material, dict):
            continue
        material_crc = _as_int(material.get("material_crc"))
        if material_crc is not None:
            values.append(material_crc)

    labels: list[str] = []
    type_labels: list[str] = []
    for value in dict.fromkeys(values):
        label = _material_label(value)
        if label:
            labels.append(label)
        type_label = _material_type_label(value)
        if type_label:
            type_labels.append(type_label)
    return tuple(labels), tuple(dict.fromkeys(type_labels))


def _shape_object_candidates(summary: dict, class_name: str) -> list[dict]:
    return [
        obj
        for obj in summary.get("objects") or []
        if obj.get("class_name") == class_name
    ]


def inspect_physics_system_shapes(block) -> tuple[HavokShapeInfo, ...]:
    """Decode virtual body/shape rows stored inside a bhkPhysicsSystem blob."""
    if block is None or getattr(block, "type_name", "") != "bhkPhysicsSystem":
        return ()
    blob = _physics_system_blob(block)
    if blob is None:
        return ()
    block_id = getattr(block, "block_id", -1)
    summary = _parse_packfile_summary(blob, block_id)
    if not summary:
        return ()

    bodies = list(summary.get("bodies") or [])
    if not bodies:
        kind = summary.get("shape_kind")
        class_name = {
            "compressed_mesh": "hknpCompressedMeshShape",
            "compound_mesh": "hknpDynamicCompoundShape",
            "compound_polytope": "hknpDynamicCompoundShape",
            "convex_polytope": "hknpConvexPolytopeShape",
        }.get(kind)
        if class_name:
            bodies = [{"body_id": 0, "shape_class": class_name}]

    result: list[HavokShapeInfo] = []
    for body in bodies:
        body_id = _as_int(body.get("body_id"), default=len(result))
        if body_id is None:
            continue
        outer_class = body.get("shape_class") or ""
        previews = _parse_packfile_previews(blob, block_id, body_id)
        materials, material_types = _body_material_labels(body)
        layer = _layer_label(body.get("layer"))
        is_compound = "CompoundShape" in outer_class or len(previews) > 1

        children: list[HavokShapeInfo] = []
        class_offsets: dict[str, int] = {}
        for shape_index, preview in enumerate(previews):
            preview_type = str(preview.get("shape_type") or "")
            class_name = _PREVIEW_CLASS_NAMES.get(
                preview_type, preview_type or "Unknown"
            )
            candidates = _shape_object_candidates(summary, class_name)
            candidate_index = class_offsets.get(class_name, 0)
            source = (
                candidates[candidate_index] if candidate_index < len(candidates) else {}
            )
            class_offsets[class_name] = candidate_index + 1
            mesh = preview.get("mesh") if isinstance(preview, dict) else None
            mesh = mesh if isinstance(mesh, dict) else {}
            vertex_count = _as_int(source.get("n_vertices"))
            if vertex_count is None:
                vertex_count = len(mesh.get("vertices") or []) or None
            triangle_count = _as_int(source.get("n_faces"))
            if triangle_count is None:
                triangle_count = len(mesh.get("triangles") or []) or None
            children.append(
                HavokShapeInfo(
                    body_id=body_id,
                    shape_index=shape_index,
                    class_name=class_name,
                    display_type=_display_shape_type(class_name),
                    layer=layer,
                    materials=materials,
                    material_types=material_types,
                    vertex_count=vertex_count,
                    triangle_count=triangle_count,
                )
            )

        if is_compound:
            result.append(
                HavokShapeInfo(
                    body_id=body_id,
                    shape_index=None,
                    class_name=outer_class or "hknpCompoundShape",
                    display_type=_display_shape_type(
                        outer_class or "hknpCompoundShape"
                    ),
                    layer=layer,
                    materials=materials,
                    material_types=material_types,
                    sub_shapes=tuple(children),
                )
            )
            continue

        child = children[0] if children else None
        class_name = outer_class or (child.class_name if child else "Unknown")
        candidates = _shape_object_candidates(summary, class_name)
        source = candidates[0] if candidates else {}
        result.append(
            HavokShapeInfo(
                body_id=body_id,
                shape_index=None,
                class_name=class_name,
                display_type=_display_shape_type(class_name),
                layer=layer,
                materials=materials,
                material_types=material_types,
                vertex_count=(
                    child.vertex_count if child else _as_int(source.get("n_vertices"))
                ),
                triangle_count=(
                    child.triangle_count if child else _as_int(source.get("n_faces"))
                ),
            )
        )
    return tuple(result)


def find_physics_system_shape(
    block,
    body_id: int,
    shape_index: int | None,
) -> HavokShapeInfo | None:
    for body in inspect_physics_system_shapes(block):
        if body.body_id != body_id:
            continue
        if shape_index is None:
            return body
        for shape in body.sub_shapes:
            if shape.shape_index == shape_index:
                return shape
    return None


def havok_shape_detail_lines(shape: HavokShapeInfo) -> list[str]:
    lines = [f"Type: {shape.display_type}", f"Havok Class: {shape.class_name}"]
    lines.append(f"Body ID: {shape.body_id}")
    if shape.shape_index is not None:
        lines.append(f"Sub-shape Index: {shape.shape_index}")
    if shape.layer:
        lines.append(f"Layer: {shape.layer}")
    for material in shape.materials:
        lines.append(f"Material: {material}")
    for material_type in shape.material_types:
        lines.append(f"Material Type: {material_type}")
    if shape.vertex_count is not None:
        lines.append(f"Vertices: {shape.vertex_count}")
    if shape.triangle_count is not None:
        lines.append(f"Triangles: {shape.triangle_count}")
    if shape.sub_shapes:
        lines.append(f"Sub-shapes: {len(shape.sub_shapes)}")
    return lines


def summarize_np_body_shape(nif, coll_obj) -> str | None:
    """Return the hknp body shape/layer label for a bhkNPCollisionObject."""
    if nif is None or coll_obj is None:
        return None
    if getattr(coll_obj, "type_name", "") != "bhkNPCollisionObject":
        return None
    data_id = _as_int(_get(coll_obj, "Data"), default=-1)
    body_id = _as_int(_get(coll_obj, "Body ID"))
    if data_id is None or data_id < 0:
        return None
    phys = nif.get_block(data_id)
    if phys is None or phys.type_name != "bhkPhysicsSystem":
        return None
    binary = _get(phys, "Binary Data")
    raw = binary.get("Data") if isinstance(binary, dict) else None
    if isinstance(raw, (bytes, bytearray)):
        blob = bytes(raw)
    elif isinstance(raw, list) and raw:
        blob = bytes(raw)
    else:
        return None
    return _body_label_from_summary(_parse_packfile_summary(blob, data_id), body_id)


def _decode_flags(flags_int: int, flag_map: dict[int, str]) -> str:
    """Decode an integer flags field into a comma-separated name list."""
    named = [name for bit, name in flag_map.items() if flags_int & bit]
    return ", ".join(named) if named else f"0x{flags_int:02X}"


def _resolve_shape_type(nif, ref_value) -> str | None:
    """Follow a Shape Ref and return the block type name, or None."""
    if nif is None:
        return None
    sid = _as_int(ref_value, default=-1)
    if sid is None or sid < 0:
        return None
    try:
        shape_block = nif.get_block(sid)
        if shape_block:
            return shape_block.type_name
    except Exception:
        pass
    return None


def _summarize_collision_object(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []

    flags_int = _as_int(_get(block, "Flags"))
    if flags_int is not None:
        lines.append(f"Flags: {_decode_flags(flags_int, _COLL_OBJ_FLAGS)}")

    if nif is None:
        return "", lines

    body_id = _as_int(_get(block, "Body ID"))
    if body_id is not None:
        lines.append(f"Body ID: {body_id}")

    # Use schema to find Ref/Ptr fields — avoids hardcoding "Body"/"Target" which
    # may differ between bhkCollisionObject, bhkNPCollisionObject, bhkBlendCollisionObject.
    try:
        from creation_lib.nif.schema import build_field_def_map
        schema = nif.schema
        fdef_map = build_field_def_map(schema, block.type_name)

        for field_name, field_value in block.fields:
            if field_name == "Flags":
                continue
            fdef = fdef_map.get(field_name)
            if fdef is None:
                continue
            if fdef.type not in ("Ref", "Ptr") and fdef.template not in ("Ref", "Ptr"):
                continue
            if not isinstance(field_value, int) or field_value < 0:
                continue
            ref_block = nif.get_block(field_value)
            if not ref_block:
                continue
            lines.append(f"{field_name}: #{field_value} ({ref_block.type_name})")

            # For rigid bodies: pull shape type and layer inline.
            if ref_block.type_name in ("bhkRigidBody", "bhkRigidBodyT"):
                shape_type = _resolve_shape_type(nif, _get(ref_block, "Shape"))
                if shape_type:
                    lines.append(f"  Shape: {shape_type}")
                rbi = _get(ref_block, "Rigid Body Info")
                if isinstance(rbi, dict):
                    layer = _as_int(rbi.get("Layer"))
                    if layer is not None:
                        lines.append(f"  Layer: {_layer_label(layer)}")
                    motion = _as_int(rbi.get("Motion System"))
                    if motion is not None:
                        lines.append(f"  Motion: {_MOTION_TYPE_NAMES.get(motion, str(motion))}")

            # For NP physics systems: show binary blob size.
            elif ref_block.type_name in ("bhkPhysicsSystem", "bhkRagdollSystem"):
                binary = _get(ref_block, "Binary Data")
                size = None
                blob = None
                if isinstance(binary, dict):
                    size = _as_int(binary.get("Size"))
                    if size is None:
                        data = binary.get("Data")
                        if isinstance(data, (list, bytes, bytearray)):
                            size = len(data)
                    data = binary.get("Data")
                    if isinstance(data, (bytes, bytearray)):
                        blob = bytes(data)
                    elif isinstance(data, list) and data:
                        blob = bytes(data)
                if size is not None:
                    lines.append(f"  NP binary: {size} bytes")
                body_label = _body_label_from_summary(
                    _parse_packfile_summary(blob, field_value) if blob else None,
                    body_id,
                )
                if body_label:
                    lines.append(f"  Body {body_id}: {body_label}")
    except Exception:
        pass

    return "", lines


def _summarize_rigid_body(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []

    # Rigid Body Info is a sub-struct on FO4; fall back to flat fields otherwise.
    info = _get(block, "Rigid Body Info")
    if not isinstance(info, dict):
        info = {}

    def _pick(name):
        if name in info:
            return info[name]
        return _get(block, name)

    mass = _pick("Mass")
    if isinstance(mass, (int, float)):
        lines.append(f"Mass: {float(mass):.3f}")
    friction = _pick("Friction")
    if isinstance(friction, (int, float)):
        lines.append(f"Friction: {float(friction):.3f}")
    restitution = _pick("Restitution")
    if isinstance(restitution, (int, float)):
        lines.append(f"Restitution: {float(restitution):.3f}")
    ldamp = _pick("Linear Damping")
    if isinstance(ldamp, (int, float)):
        lines.append(f"Linear Damping: {float(ldamp):.3f}")
    adamp = _pick("Angular Damping")
    if isinstance(adamp, (int, float)):
        lines.append(f"Angular Damping: {float(adamp):.3f}")

    # Havok Filter sub-struct (Skyrim/legacy format) — always resolved so it's
    # available for both layer and group fallbacks below.
    hf = info.get("Havok Filter") if isinstance(info, dict) else None
    if hf is None:
        hf = _get(block, "Havok Filter")

    # Layer: FO4 stores it flat in Rigid Body Info; Skyrim wraps in Havok Filter.
    layer = _as_int(info.get("Layer")) if isinstance(info, dict) else None
    if layer is None:
        layer = _as_int(_get(block, "Layer"))
    if layer is None:
        layer = _layer_from_filter(hf)
    if layer is None:
        hf2 = info.get("Havok Filter Copy") if isinstance(info, dict) else None
        if hf2 is None:
            hf2 = _get(block, "Havok Filter Copy")
        layer = _layer_from_filter(hf2)
    if layer is not None:
        lines.append(f"Layer: {_layer_label(layer)}")

    # Group — FO4: flat in Rigid Body Info; Skyrim: inside Havok Filter dict.
    group = _as_int(info.get("Group")) if isinstance(info, dict) else None
    if group is None and isinstance(hf, dict):
        group = _as_int(hf.get("Group"))
    if group is not None:
        lines.append(f"Group: {group}")

    # Motion type — FO4 uses "Motion System" inside Rigid Body Info.
    motion = _pick("Motion System")
    if motion is None:
        motion = _pick("Motion Type")
    motion_int = _as_int(motion)
    if motion_int is not None:
        lines.append(f"Motion Type: {_MOTION_TYPE_NAMES.get(motion_int, str(motion_int))}")

    # Quality type (may live inside Rigid Body Info sub-struct).
    quality = _pick("Quality Type")
    quality_int = _as_int(quality)
    if quality_int is not None:
        lines.append(f"Quality Type: {_QUALITY_TYPE_NAMES.get(quality_int, str(quality_int))}")

    shape_ref = _get(block, "Shape")
    shape_label = _ref_label(shape_ref)
    if shape_label:
        shape_type = _resolve_shape_type(nif, shape_ref)
        if shape_type:
            lines.append(f"Shape: {shape_label} ({shape_type})")
        else:
            lines.append(f"Shape: {shape_label}")

    suffix_parts = []
    if isinstance(mass, (int, float)):
        suffix_parts.append(f"mass {float(mass):.1f}")
    if layer is not None:
        suffix_parts.append(_LAYER_NAMES.get(layer, f"LAYER_{layer}"))
    suffix = f"({', '.join(suffix_parts)})" if suffix_parts else ""
    return suffix, lines


def _summarize_box_shape(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []
    dims = _get(block, "Dimensions")
    if isinstance(dims, dict):
        lines.append(f"Dimensions: {_fmt_v3(dims)}")
    radius = _get(block, "Radius")
    if isinstance(radius, (int, float)):
        lines.append(f"Radius: {float(radius):.3f}")
    _append_material_lines(lines, _get(block, "Material"))
    suffix = ""
    if isinstance(dims, dict):
        suffix = (
            f"(box {float(dims.get('x', 0.0)):.2f} x "
            f"{float(dims.get('y', 0.0)):.2f} x "
            f"{float(dims.get('z', 0.0)):.2f})"
        )
    return suffix, lines


def _summarize_sphere_shape(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []
    radius = _get(block, "Radius")
    suffix = ""
    if isinstance(radius, (int, float)):
        lines.append(f"Radius: {float(radius):.3f}")
        suffix = f"(sphere r={float(radius):.2f})"
    _append_material_lines(lines, _get(block, "Material"))
    return suffix, lines


def _summarize_capsule_shape(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []
    radius = _get(block, "Radius")
    if isinstance(radius, (int, float)):
        lines.append(f"Radius: {float(radius):.3f}")
    r1 = _get(block, "Radius 1")
    if isinstance(r1, (int, float)):
        lines.append(f"Radius 1: {float(r1):.3f}")
    r2 = _get(block, "Radius 2")
    if isinstance(r2, (int, float)):
        lines.append(f"Radius 2: {float(r2):.3f}")
    p1 = _get(block, "First Point")
    if isinstance(p1, dict):
        lines.append(f"First Point: {_fmt_v3(p1)}")
    p2 = _get(block, "Second Point")
    if isinstance(p2, dict):
        lines.append(f"Second Point: {_fmt_v3(p2)}")
    length = None
    if isinstance(p1, dict) and isinstance(p2, dict):
        dx = float(p2.get("x", 0.0)) - float(p1.get("x", 0.0))
        dy = float(p2.get("y", 0.0)) - float(p1.get("y", 0.0))
        dz = float(p2.get("z", 0.0)) - float(p1.get("z", 0.0))
        length = (dx * dx + dy * dy + dz * dz) ** 0.5
        lines.append(f"Length: {length:.3f}")
    _append_material_lines(lines, _get(block, "Material"))
    suffix = ""
    if isinstance(radius, (int, float)) and length is not None:
        suffix = f"(capsule r={float(radius):.2f} len={length:.2f})"
    return suffix, lines


def _summarize_convex_vertices(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []
    n_verts = _as_int(_get(block, "Num Vertices"))
    if n_verts is None:
        verts = _get(block, "Vertices")
        if isinstance(verts, list):
            n_verts = len(verts)
    n_normals = _as_int(_get(block, "Num Normals"))
    if n_normals is None:
        normals = _get(block, "Normals")
        if isinstance(normals, list):
            n_normals = len(normals)
    if n_verts is not None:
        lines.append(f"Vertices: {n_verts}")
    if n_normals is not None:
        lines.append(f"Normals: {n_normals}")
    radius = _get(block, "Radius")
    if isinstance(radius, (int, float)):
        lines.append(f"Radius: {float(radius):.3f}")
    _append_material_lines(lines, _get(block, "Material"))
    suffix = f"(convex hull, {n_verts} verts)" if n_verts is not None else "(convex hull)"
    return suffix, lines


def _summarize_transform_shape(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []
    _append_material_lines(lines, _get(block, "Material"))
    radius = _get(block, "Radius")
    if isinstance(radius, (int, float)):
        lines.append(f"Radius: {float(radius):.3f}")
    inner_ref = _get(block, "Shape")
    inner = _ref_label(inner_ref)
    if inner:
        inner_type = _resolve_shape_type(nif, inner_ref)
        if inner_type:
            lines.append(f"Shape: {inner} ({inner_type})")
        else:
            lines.append(f"Shape: {inner}")
    xform = _get(block, "Transform")
    if isinstance(xform, dict):
        if "Translation" in xform and isinstance(xform["Translation"], dict):
            lines.append(f"Translation: {_fmt_v3(xform['Translation'])}")
        else:
            tx = xform.get("m14", xform.get("m41", xform.get("m03")))
            ty = xform.get("m24", xform.get("m42", xform.get("m13")))
            tz = xform.get("m34", xform.get("m43", xform.get("m23")))
            if tx is not None and ty is not None and tz is not None:
                lines.append(
                    f"Translation: ({float(tx):.3f}, {float(ty):.3f}, {float(tz):.3f})"
                )
    return "(transform)", lines


def _summarize_list_shape(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []
    n_sub = _as_int(_get(block, "Num Sub Shapes"))
    if n_sub is None:
        subs = _get(block, "Sub Shapes")
        if isinstance(subs, list):
            n_sub = len(subs)
    if n_sub is not None:
        lines.append(f"Sub Shapes: {n_sub}")
    _append_material_lines(lines, _get(block, "Material"))
    suffix = f"(list, {n_sub} shapes)" if n_sub is not None else "(list)"
    return suffix, lines


def _summarize_mopp(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []
    inner_ref = _get(block, "Shape")
    inner = _ref_label(inner_ref)
    if inner:
        inner_type = _resolve_shape_type(nif, inner_ref)
        if inner_type:
            lines.append(f"Shape: {inner} ({inner_type})")
        else:
            lines.append(f"Shape: {inner}")
    mopp_size = _as_int(_get(block, "MOPP Data Size"))
    if mopp_size is None:
        mopp_data = _get(block, "MOPP Data")
        if isinstance(mopp_data, list):
            mopp_size = len(mopp_data)
    if mopp_size is not None:
        lines.append(f"MOPP Data Size: {mopp_size}")
    _append_material_lines(lines, _get(block, "Material"))
    scale = _get(block, "Scale")
    if isinstance(scale, (int, float)):
        lines.append(f"Scale: {float(scale):.3f}")
    return "(MOPP)", lines


def _summarize_compressed_mesh_shape(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []
    radius = _get(block, "Radius")
    if isinstance(radius, (int, float)):
        lines.append(f"Radius: {float(radius):.3f}")
    rcopy = _get(block, "Radius Copy")
    if isinstance(rcopy, (int, float)):
        lines.append(f"Radius Copy: {float(rcopy):.3f}")
    scale = _get(block, "Scale")
    if isinstance(scale, dict):
        lines.append(f"Scale: {_fmt_v3(scale)}")
    elif isinstance(scale, (int, float)):
        lines.append(f"Scale: {float(scale):.3f}")
    return "(compressed mesh)", lines


def _summarize_compressed_mesh_data(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []
    n_chunks = _as_int(_get(block, "Num Chunks"))
    if n_chunks is None:
        chunks = _get(block, "Chunks")
        if isinstance(chunks, list):
            n_chunks = len(chunks)
    n_big = _as_int(_get(block, "Num Big Verts"))
    if n_big is None:
        bv = _get(block, "Big Verts")
        if isinstance(bv, list):
            n_big = len(bv)
    n_tris = _as_int(_get(block, "Num Triangles"))
    if n_tris is None:
        tris = _get(block, "Big Tris")
        if isinstance(tris, list):
            n_tris = len(tris)
    if n_chunks is not None:
        lines.append(f"Chunks: {n_chunks}")
    if n_big is not None:
        lines.append(f"Big Verts: {n_big}")
    if n_tris is not None:
        lines.append(f"Triangles: {n_tris}")
    parts = []
    if n_chunks is not None:
        parts.append(f"{n_chunks} chunks")
    if n_tris is not None:
        parts.append(f"{n_tris} tris")
    suffix = f"({', '.join(parts)})" if parts else ""
    return suffix, lines


def _summarize_tri_strips_shape(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []
    n_data = _as_int(_get(block, "Num Data"))
    if n_data is None:
        data = _get(block, "Strips Data")
        if isinstance(data, list):
            n_data = len(data)
    n_sub = _as_int(_get(block, "Num Sub Shapes"))
    if n_sub is None:
        subs = _get(block, "Sub Shapes")
        if isinstance(subs, list):
            n_sub = len(subs)
    if n_data is not None:
        lines.append(f"Data: {n_data}")
    if n_sub is not None:
        lines.append(f"Sub Shapes: {n_sub}")
    radius = _get(block, "Radius")
    if isinstance(radius, (int, float)):
        lines.append(f"Radius: {float(radius):.3f}")
    scale = _get(block, "Scale")
    if isinstance(scale, dict):
        lines.append(f"Scale: {_fmt_v3(scale)}")
    elif isinstance(scale, (int, float)):
        lines.append(f"Scale: {float(scale):.3f}")
    return "(tri strips)", lines


def _summarize_physics_system(nif, block) -> tuple[str, list[str]]:
    lines: list[str] = []
    binary = _get(block, "Binary Data")
    blob: bytes | None = None
    if isinstance(binary, dict):
        raw = binary.get("Data")
        if isinstance(raw, (bytes, bytearray)):
            blob = bytes(raw)
        elif isinstance(raw, list) and raw:
            blob = bytes(raw)

    if blob is None:
        size = _as_int(binary.get("Size")) if isinstance(binary, dict) else None
        size_str = f"{size}" if size is not None else "?"
        return f"({size_str} bytes)", [f"Binary Data Size: {size_str}"]

    block_id = getattr(block, "block_id", -1)
    summary = _parse_packfile_summary(blob, block_id)

    if summary is None:
        lines.append(f"Binary Data Size: {len(blob)} bytes")
        return f"({len(blob)} bytes)", lines

    lines.append(f"Binary Data Size: {len(blob)} bytes")
    kind = summary["shape_kind"]
    objects = summary["objects"]
    bodies = summary.get("bodies") or []

    if len(bodies) > 1:
        body_labels: list[str] = []
        for body in bodies:
            body_id = _as_int(body.get("body_id"))
            shape_label = _shape_class_label(body.get("shape_class")) or "unknown"
            layer_label = _layer_label(body.get("layer"))
            material_label = _material_label(body.get("material_crc"))
            material_type = _material_type_label(body.get("material_crc"))
            body_labels.append(shape_label)
            id_label = "?" if body_id is None else str(body_id)
            detail = ", ".join(
                part for part in (shape_label, layer_label, material_label) if part
            )
            lines.append(f"Body {id_label}: {detail}")
            if material_type:
                lines.append(f"Body {id_label} Material Type: {material_type}")
        unique_labels = list(dict.fromkeys(body_labels))
        suffix_detail = " + ".join(unique_labels) if unique_labels else "mixed"
        return f"({len(bodies)} bodies: {suffix_detail})", lines

    if kind == "convex_polytope":
        material_label = _body_material_from_summary(summary, 0)
        material_type = _body_material_type_from_summary(summary, 0)
        shape_obj = next((o for o in objects if o["class_name"] == "hknpConvexPolytopeShape"), None)
        nv = shape_obj["n_vertices"] if shape_obj else None
        nf = shape_obj["n_faces"] if shape_obj else None
        np_ = shape_obj["n_planes"] if shape_obj else None
        empty_polytope = (
            summary.get("geometry_status") == "empty_polytope"
            or (nv == 0 and nf == 0 and np_ == 0)
        )
        parts = []
        if nv is not None:
            parts.append(f"{nv} vertices")
        if nf is not None:
            parts.append(f"{nf} faces")
        detail = ", ".join(parts)
        if empty_polytope:
            suffix = "(Unsupported/empty polytope)"
        else:
            suffix = f"(Convex Polytope, {detail})" if detail else "(Convex Polytope)"
        if shape_obj:
            plane_str = f", {np_} planes" if np_ is not None else ""
            if empty_polytope:
                lines.append(
                    "Shape: hknpConvexPolytopeShape (no decoded geometry)"
                )
            else:
                lines.append(f"Shape: hknpConvexPolytopeShape ({nv} vertices, {nf} faces{plane_str})")
        if material_label:
            lines.append(f"Material: {material_label}")
        if material_type:
            lines.append(f"Material Type: {material_type}")
        for o in objects:
            cn = o["class_name"]
            if cn in ("hkRefCountedProperties", "hknpShapeMassProperties", "hknpBSMaterialProperties"):
                lines.append(f"Properties: {cn}")

    elif kind in ("compound_polytope", "compound_mesh"):
        material_label = _body_material_from_summary(summary, 0)
        material_type = _body_material_type_from_summary(summary, 0)
        n_sub = summary["n_subshapes"] or 0
        sub_label = "polytope" if kind == "compound_polytope" else "mesh"
        suffix = f"(Compound, {n_sub} {sub_label} sub-shapes)"
        lines.append("Outer: hknpDynamicCompoundShape")
        sub_shapes = [o for o in objects if o["class_name"] in ("hknpConvexPolytopeShape", "hknpCompressedMeshShape")]
        shown = sub_shapes[:3]
        for idx, o in enumerate(shown, 1):
            nv = o["n_vertices"]
            nf = o["n_faces"]
            parts = []
            if nv is not None:
                parts.append(f"{nv} verts")
            if nf is not None:
                parts.append(f"{nf} faces")
            detail = ", ".join(parts)
            lines.append(f"Sub-shape {idx}/{n_sub}: {o['class_name']} ({detail})")
        if len(sub_shapes) > 3:
            lines.append(f"  … and {len(sub_shapes) - 3} more")
        for o in objects:
            if o["class_name"] == "hknpDynamicCompoundShapeData":
                lines.append("Tree: hknpDynamicCompoundShapeData")
                break
        if material_label:
            lines.append(f"Material: {material_label}")
        if material_type:
            lines.append(f"Material Type: {material_type}")

    elif kind == "compressed_mesh":
        material_label = _body_material_from_summary(summary, 0)
        material_type = _body_material_type_from_summary(summary, 0)
        suffix = "(Compressed Mesh)"
        lines.append("Shape: hknpCompressedMeshShape")
        if material_label:
            lines.append(f"Material: {material_label}")
        if material_type:
            lines.append(f"Material Type: {material_type}")

    else:
        suffix = f"({len(blob)} bytes)"

    return suffix, lines


# Dispatch: exact type name -> handler(nif, block).
_HANDLERS = {
    "bhkCollisionObject": _summarize_collision_object,
    "bhkNPCollisionObject": _summarize_collision_object,
    "bhkBlendCollisionObject": _summarize_collision_object,
    "bhkSPCollisionObject": _summarize_collision_object,
    "bhkRigidBody": _summarize_rigid_body,
    "bhkRigidBodyT": _summarize_rigid_body,
    "bhkBoxShape": _summarize_box_shape,
    "bhkSphereShape": _summarize_sphere_shape,
    "bhkCapsuleShape": _summarize_capsule_shape,
    "bhkConvexVerticesShape": _summarize_convex_vertices,
    "bhkConvexTransformShape": _summarize_transform_shape,
    "bhkTransformShape": _summarize_transform_shape,
    "bhkListShape": _summarize_list_shape,
    "bhkMoppBvTreeShape": _summarize_mopp,
    "bhkCompressedMeshShape": _summarize_compressed_mesh_shape,
    "bhkCompressedMeshShapeData": _summarize_compressed_mesh_data,
    "bhkNiTriStripsShape": _summarize_tri_strips_shape,
    "bhkPackedNiTriStripsShape": _summarize_tri_strips_shape,
    "bhkPhysicsSystem": _summarize_physics_system,
}


def summarize_collision_block(nif, block) -> tuple[str, list[str]]:
    """Return (short_label_suffix, detail_lines) for a Havok/collision block.

    The suffix is appended to the scene-tree label in parentheses.
    The detail_lines are displayed one per line in the hover tooltip.
    """
    if block is None:
        return "", []
    type_name = getattr(block, "type_name", "") or ""
    handler = _HANDLERS.get(type_name)
    if handler is None:
        return "", []
    try:
        return handler(nif, block)
    except Exception:
        return "", []
