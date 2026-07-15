"""Starfield .mesh file parser.

Decodes the compressed binary vertex format used by BSGeometry blocks.
Reference: NifSkope src/io/MeshFile.cpp
"""
from __future__ import annotations

import struct
import logging
from dataclasses import dataclass

import numpy as np

_log = logging.getLogger("nif_editor.sf_mesh_loader")


@dataclass
class SFMeshData:
    """Parsed Starfield mesh geometry."""
    positions: np.ndarray   # (N, 3) float32
    normals: np.ndarray     # (N, 3) float32
    uvs: np.ndarray         # (N, 2) float32
    uv2: np.ndarray         # (N, 2) float32 — second UV channel (zero array if absent)
    triangles: np.ndarray   # (T, 3) uint32
    tangents: np.ndarray | None   # (N, 3) float32 or None
    bitangents: np.ndarray | None # (N, 3) float32 or None
    colors: np.ndarray | None     # (N, 4) float32 or None
    scale: float
    version: int


def _decode_udec_normals(packed: np.ndarray) -> np.ndarray:
    """Decode UDecVector4 (unsigned 10.10.10.2) packed normals to float32 (N,3)."""
    x = (packed & 0x3FF).astype(np.float32) / 511.0 - 1.0
    y = ((packed >> 10) & 0x3FF).astype(np.float32) / 511.0 - 1.0
    z = ((packed >> 20) & 0x3FF).astype(np.float32) / 511.0 - 1.0
    return np.column_stack((x, y, z))


def _compute_normals(positions: np.ndarray, triangles: np.ndarray) -> np.ndarray:
    """Compute smooth area-weighted normals from positions and triangles."""
    normals = np.zeros_like(positions)
    v0 = positions[triangles[:, 0]]
    v1 = positions[triangles[:, 1]]
    v2 = positions[triangles[:, 2]]
    face_normals = np.cross(v1 - v0, v2 - v0)
    for i in range(3):
        np.add.at(normals, triangles[:, i], face_normals)
    lengths = np.linalg.norm(normals, axis=1, keepdims=True)
    lengths[lengths < 1e-8] = 1.0
    return (normals / lengths).astype(np.float32)


def parse_sf_mesh(data: bytes) -> SFMeshData | None:
    """Parse a Starfield .mesh binary blob into numpy arrays.

    Returns None if the data is invalid or too short.
    """
    if len(data) < 20:
        return None

    pos = 0

    # Version (magic)
    version = struct.unpack_from("<I", data, pos)[0]; pos += 4
    if version > 2:
        _log.warning("Unknown .mesh version %d", version)
        return None

    # Triangles
    indices_size = struct.unpack_from("<I", data, pos)[0]; pos += 4
    num_tris = indices_size // 3
    if num_tris == 0:
        return None
    triangles = np.frombuffer(data, dtype=np.uint16, count=indices_size, offset=pos)
    pos += indices_size * 2
    triangles = triangles.reshape(num_tris, 3).astype(np.uint32)

    # Scale, weights per vertex, num positions
    scale, weights_per_vert, num_positions = struct.unpack_from("<fII", data, pos); pos += 12
    if scale <= 0.0 or num_positions == 0:
        return None

    # Positions: 3 x int16 per vertex
    raw_pos = np.frombuffer(data, dtype=np.int16, count=num_positions * 3, offset=pos)
    pos += num_positions * 6
    positions = raw_pos.reshape(num_positions, 3).astype(np.float32) / 32767.0 * scale

    # UVs (coord1): count + packed float16 pairs
    num_uv1 = struct.unpack_from("<I", data, pos)[0]; pos += 4
    if num_uv1 > 0:
        raw_uv = np.frombuffer(data, dtype=np.float16, count=num_uv1 * 2, offset=pos)
        pos += num_uv1 * 4
        uvs = raw_uv.reshape(num_uv1, 2).astype(np.float32)
    else:
        uvs = np.zeros((num_positions, 2), dtype=np.float32)

    # UVs (coord2): count + packed float16 pairs (some layers use this)
    num_uv2 = struct.unpack_from("<I", data, pos)[0]; pos += 4
    if num_uv2 > 0:
        raw_uv2 = np.frombuffer(data, dtype=np.float16, count=num_uv2 * 2, offset=pos)
        pos += num_uv2 * 4
        uv2 = raw_uv2.reshape(num_uv2, 2).astype(np.float32)
    else:
        uv2 = np.zeros((num_positions, 2), dtype=np.float32)

    # Colors: BGRA uint32. Default to (1,1,1,1) when absent so the SF shader's
    # `baseColor *= vVertexColor.rgb` doesn't multiply everything by zero on
    # decal/sub meshes that don't carry per-vertex colors. Matches the
    # standalone tools/sf_render_test.py:219.
    num_colors = struct.unpack_from("<I", data, pos)[0]; pos += 4
    colors = np.ones((num_positions, 4), dtype=np.float32)
    if num_colors > 0:
        raw_colors = np.frombuffer(data, dtype=np.uint8, count=num_colors * 4, offset=pos)
        pos += num_colors * 4
        bgra = raw_colors.reshape(num_colors, 4).astype(np.float32) / 255.0
        # BGRA -> RGBA
        colors = bgra[:, [2, 1, 0, 3]].copy()

    # Normals: UDecVector4
    num_normals = struct.unpack_from("<I", data, pos)[0]; pos += 4
    if num_normals > 0:
        raw_normals = np.frombuffer(data, dtype=np.uint32, count=num_normals, offset=pos)
        pos += num_normals * 4
        normals = _decode_udec_normals(raw_normals)
    else:
        normals = None  # will compute below

    # Tangents: UDecVector4 with w=bitangent basis
    num_tangents = struct.unpack_from("<I", data, pos)[0]; pos += 4
    tangents = None
    bitangents = None
    if num_tangents > 0:
        raw_tangents = np.frombuffer(data, dtype=np.uint32, count=num_tangents, offset=pos)
        pos += num_tangents * 4
        tangents = _decode_udec_normals(raw_tangents)
        # w component = bitangent basis sign: w field is bits 30-31 (0-3),
        # mapped as: w >= 2 → +1, else -1 (matches NifSkope MeshFile.cpp)
        w_raw = ((raw_tangents >> 30) & 0x3).astype(np.float32)
        w = np.where(w_raw >= 2.0, 1.0, -1.0).astype(np.float32)
        if normals is not None:
            bitangents = np.cross(normals, tangents) * w[:, np.newaxis]

    # Compute normals if not present in file
    if normals is None:
        normals = _compute_normals(positions, triangles)

    # Pad UVs to match vertex count if needed
    if uvs.shape[0] < num_positions:
        padded = np.zeros((num_positions, 2), dtype=np.float32)
        padded[:uvs.shape[0]] = uvs
        uvs = padded

    if uv2.shape[0] < num_positions:
        padded2 = np.zeros((num_positions, 2), dtype=np.float32)
        padded2[:uv2.shape[0]] = uv2
        uv2 = padded2

    return SFMeshData(
        positions=positions,
        normals=normals,
        uvs=uvs,
        uv2=uv2,
        triangles=triangles,
        tangents=tangents,
        bitangents=bitangents,
        colors=colors,
        scale=scale,
        version=version,
    )
