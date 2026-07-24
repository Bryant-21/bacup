"""Tests for Starfield .mesh binary parser."""
import os
import struct
from pathlib import Path

import numpy as np
import pytest


def _make_minimal_mesh(
    num_verts=3, num_tris=1, scale=1.0, version=2,
    has_uvs=True, has_normals=True, has_tangents=False,
    has_colors=False, has_weights=False, weights_per_vert=0,
):
    """Build a minimal valid .mesh binary blob for testing."""
    buf = bytearray()
    # Header
    buf += struct.pack("<I", version)  # magic/version
    indices_size = num_tris * 3
    buf += struct.pack("<I", indices_size)
    # Triangles (uint16 x indices_size)
    for i in range(num_tris):
        buf += struct.pack("<3H", i * 3, i * 3 + 1, i * 3 + 2)
    # Scale, weights_per_vertex, num_positions
    buf += struct.pack("<fII", scale, weights_per_vert, num_verts)
    # Positions: 3 x int16 per vertex (6 bytes each)
    for i in range(num_verts):
        x = int(i * 1000)
        y = int(i * 2000)
        z = int(i * 3000)
        buf += struct.pack("<3h", x, y, z)
    # UVs (coord1): count + uint32 per UV (two float16s packed)
    uv_count = num_verts if has_uvs else 0
    buf += struct.pack("<I", uv_count)
    for i in range(uv_count):
        u_half = np.float16(i * 0.25)
        v_half = np.float16(i * 0.5)
        packed = np.array([u_half, v_half], dtype=np.float16).view(np.uint32)[0]
        buf += struct.pack("<I", packed)
    # UVs (coord2): always 0 for our tests
    buf += struct.pack("<I", 0)
    # Colors
    color_count = num_verts if has_colors else 0
    buf += struct.pack("<I", color_count)
    for i in range(color_count):
        buf += struct.pack("<I", 0xFF808080)  # BGRA grey
    # Normals: UDecVector4 (10.10.10.2 packed uint32)
    normal_count = num_verts if has_normals else 0
    buf += struct.pack("<I", normal_count)
    for i in range(normal_count):
        # Encode (0, 0, 1) as 10.10.10.2: x=511, y=511, z=1023
        packed = (511) | (511 << 10) | (1023 << 20)
        buf += struct.pack("<I", packed)
    # Tangents
    tangent_count = num_verts if has_tangents else 0
    buf += struct.pack("<I", tangent_count)
    for i in range(tangent_count):
        packed = (1023) | (511 << 10) | (511 << 20)
        buf += struct.pack("<I", packed)
    # Weights
    weight_count = num_verts * weights_per_vert if has_weights else 0
    buf += struct.pack("<I", weight_count)
    for i in range(weight_count):
        buf += struct.pack("<HH", 0, 32767)  # bone 0, full weight
    # LODs (version >= 1)
    if version >= 1:
        buf += struct.pack("<I", 0)  # 0 LODs
    return bytes(buf)


def test_parse_basic_triangle():
    from creation_lib.renderer.sf_mesh_loader import parse_sf_mesh
    data = _make_minimal_mesh(num_verts=3, num_tris=1, scale=2.0)
    result = parse_sf_mesh(data)
    assert result is not None
    assert result.positions.shape == (3, 3)
    assert result.triangles.shape == (1, 3)
    assert result.uvs.shape == (3, 2)
    assert result.normals.shape == (3, 3)
    # Check scale applied to positions
    assert result.positions[0, 0] == pytest.approx(0.0, abs=0.01)
    assert result.scale == 2.0


def test_parse_no_uvs():
    from creation_lib.renderer.sf_mesh_loader import parse_sf_mesh
    data = _make_minimal_mesh(num_verts=3, has_uvs=False)
    result = parse_sf_mesh(data)
    assert result.uvs.shape == (3, 2)  # should get zeros
    assert np.allclose(result.uvs, 0.0)


def test_parse_no_normals():
    from creation_lib.renderer.sf_mesh_loader import parse_sf_mesh
    data = _make_minimal_mesh(num_verts=3, has_normals=False)
    result = parse_sf_mesh(data)
    # Should auto-compute normals
    assert result.normals.shape == (3, 3)


def test_parse_with_colors():
    from creation_lib.renderer.sf_mesh_loader import parse_sf_mesh
    data = _make_minimal_mesh(num_verts=3, has_colors=True)
    result = parse_sf_mesh(data)
    assert result.colors is not None
    assert result.colors.shape == (3, 4)


def test_invalid_version_returns_none():
    from creation_lib.renderer.sf_mesh_loader import parse_sf_mesh
    data = struct.pack("<I", 99)  # invalid version
    result = parse_sf_mesh(data)
    assert result is None


def test_zero_scale_returns_none():
    from creation_lib.renderer.sf_mesh_loader import parse_sf_mesh
    data = _make_minimal_mesh(scale=0.0)
    result = parse_sf_mesh(data)
    assert result is None


def test_resolve_sf_mesh_path():
    """Test mesh path resolution from hash to filesystem path."""
    from creation_lib.renderer.nif_loader import _resolve_sf_mesh_file
    from pathlib import Path
    import tempfile

    with tempfile.TemporaryDirectory() as tmpdir:
        geo_dir = Path(tmpdir) / "geometries" / "abc123"
        geo_dir.mkdir(parents=True)
        mesh_file = geo_dir / "def456.mesh"
        mesh_file.write_bytes(b"test")

        result = _resolve_sf_mesh_file("abc123\\def456", [Path(tmpdir)])
        assert result is not None
        assert result == b"test"


def test_resolve_sf_mesh_path_not_found():
    from creation_lib.renderer.nif_loader import _resolve_sf_mesh_file
    from pathlib import Path
    result = _resolve_sf_mesh_file("nonexistent\\path", [Path(".")])
    assert result is None


def test_real_novablast_mesh():
    """Integration test with an actual Starfield mesh file."""
    starfield_extracted_dir = Path(
        os.environ.get("STARFIELD_EXTRACTED_DIR")
        or Path(__file__).resolve().parents[2] / "extracted" / "starfield"
    )
    mesh_path = starfield_extracted_dir / "meshes/geometries/5cf8552e4c1fee905eb8/ee008e8a733347785ea6.mesh"
    if not mesh_path.exists():
        pytest.skip("Starfield mesh file not available (set STARFIELD_EXTRACTED_DIR)")
    from creation_lib.renderer.sf_mesh_loader import parse_sf_mesh
    data = mesh_path.read_bytes()
    result = parse_sf_mesh(data)
    assert result is not None
    assert result.positions.shape == (1418, 3)
    assert result.triangles.shape == (1334, 3)
    assert result.normals.shape == (1418, 3)
    assert result.uvs.shape == (1418, 2)
    # Sanity: positions should be small weapon-scale values
    assert np.abs(result.positions).max() < 5.0
