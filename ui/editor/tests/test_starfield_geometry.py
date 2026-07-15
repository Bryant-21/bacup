"""Tests for Starfield external geometry detection and placeholder rendering."""
import pytest
from unittest.mock import MagicMock, patch
import numpy as np

from creation_lib.renderer.nif_loader import (
    _extract_external_geometry_info,
    _make_bbox_placeholder,
    PreparedShape,
)


def _make_mock_block(type_name, block_id, fields):
    """Create a mock NIF block with get_field support."""
    block = MagicMock()
    block.type_name = type_name
    block.block_id = block_id
    block.get_field = lambda name: fields.get(name)
    return block


def _make_mock_schema():
    """Schema where BSGeometry is recognized but is NOT a subtype of BSTriShape."""
    schema = MagicMock()
    hierarchy = {
        "BSGeometry": ["BSGeometry", "NiAVObject", "NiObjectNET", "NiObject"],
        "BSTriShape": ["BSTriShape", "NiAVObject", "NiObjectNET", "NiObject"],
        "NiNode": ["NiNode", "NiAVObject", "NiObjectNET", "NiObject"],
        "BSFadeNode": ["BSFadeNode", "NiNode", "NiAVObject", "NiObjectNET", "NiObject"],
    }
    schema.is_subtype_of = lambda t, base: base in hierarchy.get(t, [t])
    return schema


class TestExtractExternalGeometryInfo:
    """Test _extract_external_geometry_info detects mesh paths from BSGeometry."""

    def test_detects_external_mesh_path(self):
        """BSGeometry with Mesh Path in Meshes array is detected."""
        meshes = [
            {"Has Mesh": 1, "Mesh": {"Mesh Path": "geometries/weapon/gun01.mesh", "Indices Size": 0, "Num Verts": 0, "Flags": 0}},
            {"Has Mesh": 0},
            {"Has Mesh": 0},
            {"Has Mesh": 0},
        ]
        block = _make_mock_block("BSGeometry", 5, {
            "Meshes": meshes,
            "Bounding Sphere": {"Center": {"x": 0, "y": 0, "z": 0}, "Radius": 10.0},
            "Bounding Box": {"Center": {"x": 0, "y": 0, "z": 0}, "Dimensions": {"x": 5, "y": 5, "z": 5}},
            "Name": "Gun01",
        })
        paths = _extract_external_geometry_info(block)
        assert paths == ["geometries/weapon/gun01.mesh"]

    def test_multiple_mesh_paths(self):
        """Multiple LOD meshes are all captured."""
        meshes = [
            {"Has Mesh": 1, "Mesh": {"Mesh Path": "geometries/a.mesh", "Indices Size": 0, "Num Verts": 0, "Flags": 0}},
            {"Has Mesh": 1, "Mesh": {"Mesh Path": "geometries/b.mesh", "Indices Size": 0, "Num Verts": 0, "Flags": 0}},
            {"Has Mesh": 0},
            {"Has Mesh": 0},
        ]
        block = _make_mock_block("BSGeometry", 6, {"Meshes": meshes, "Name": "Multi"})
        paths = _extract_external_geometry_info(block)
        assert len(paths) == 2
        assert "geometries/a.mesh" in paths
        assert "geometries/b.mesh" in paths

    def test_no_meshes_returns_empty(self):
        """BSGeometry with no Meshes field returns empty list."""
        block = _make_mock_block("BSGeometry", 7, {"Meshes": None, "Name": "Empty"})
        paths = _extract_external_geometry_info(block)
        assert paths == []

    def test_inline_mesh_data_not_detected(self):
        """BSGeometry with inline Mesh Data (no Mesh Path) is not external."""
        meshes = [
            {"Has Mesh": 1, "Mesh": {"Mesh Data": {"Num Verts": 100}, "Indices Size": 300, "Num Verts": 100, "Flags": 512}},
            {"Has Mesh": 0},
            {"Has Mesh": 0},
            {"Has Mesh": 0},
        ]
        block = _make_mock_block("BSGeometry", 8, {"Meshes": meshes, "Name": "Inline"})
        paths = _extract_external_geometry_info(block)
        assert paths == []

    def test_empty_mesh_path_ignored(self):
        """Empty string mesh path is ignored."""
        meshes = [
            {"Has Mesh": 1, "Mesh": {"Mesh Path": "", "Indices Size": 0, "Num Verts": 0, "Flags": 0}},
            {"Has Mesh": 0},
            {"Has Mesh": 0},
            {"Has Mesh": 0},
        ]
        block = _make_mock_block("BSGeometry", 9, {"Meshes": meshes, "Name": "EmptyPath"})
        paths = _extract_external_geometry_info(block)
        assert paths == []


class TestMakeBboxPlaceholder:
    """Test _make_bbox_placeholder creates a valid PreparedShape from bounding box."""

    def test_creates_box_from_bounding_box(self):
        """Placeholder uses BSBoundingBox center + dimensions."""
        block = _make_mock_block("BSGeometry", 10, {
            "Bounding Box": {"Center": {"x": 1, "y": 2, "z": 3}, "Dimensions": {"x": 5, "y": 6, "z": 7}},
            "Bounding Sphere": {"Center": {"x": 1, "y": 2, "z": 3}, "Radius": 10.0},
            "Name": "TestShape",
            "Translation": {"x": 0, "y": 0, "z": 0},
            "Rotation": {},
            "Scale": 1.0,
        })
        ps = _make_bbox_placeholder(block, ["geometries/test.mesh"])
        assert ps is not None
        assert ps.block_id == 10
        assert ps.name == "TestShape"
        assert ps.verts.shape == (8, 3)  # 8 box corners
        assert ps.tris.shape[0] == 12   # 12 triangles for a box
        assert ps.external_mesh_paths == ["geometries/test.mesh"]

    def test_falls_back_to_bounding_sphere(self):
        """When no Bounding Box, use Bounding Sphere radius as half-extent."""
        block = _make_mock_block("BSGeometry", 11, {
            "Bounding Box": None,
            "Bounding Sphere": {"Center": {"x": 0, "y": 0, "z": 0}, "Radius": 5.0},
            "Name": "SphereOnly",
            "Translation": {"x": 0, "y": 0, "z": 0},
            "Rotation": {},
            "Scale": 1.0,
        })
        ps = _make_bbox_placeholder(block, ["geometries/sphere.mesh"])
        assert ps is not None
        assert ps.verts.shape == (8, 3)
        # Verts should span from -5 to +5 on each axis
        assert ps.verts.min() == pytest.approx(-5.0)
        assert ps.verts.max() == pytest.approx(5.0)

    def test_default_unit_box_when_no_bounds(self):
        """Without any bounds data, creates a unit box."""
        block = _make_mock_block("BSGeometry", 12, {
            "Bounding Box": None,
            "Bounding Sphere": None,
            "Name": "NoBounds",
            "Translation": {"x": 0, "y": 0, "z": 0},
            "Rotation": {},
            "Scale": 1.0,
        })
        ps = _make_bbox_placeholder(block, ["geometries/nobounds.mesh"])
        assert ps is not None
        assert ps.verts.shape == (8, 3)


class TestPrepareWalkHandlesBSGeometry:
    """Test that _prepare_walk_blocks handles BSGeometry blocks."""

    def test_bsgeometry_added_to_shapes(self):
        """BSGeometry with external mesh path produces a PreparedShape in shapes dict."""
        from creation_lib.renderer.nif_loader import _prepare_walk_blocks

        meshes = [
            {"Has Mesh": 1, "Mesh": {"Mesh Path": "geometries/test.mesh", "Indices Size": 0, "Num Verts": 0, "Flags": 0}},
            {"Has Mesh": 0}, {"Has Mesh": 0}, {"Has Mesh": 0},
        ]
        block = _make_mock_block("BSGeometry", 5, {
            "Meshes": meshes,
            "Bounding Box": {"Center": {"x": 0, "y": 0, "z": 0}, "Dimensions": {"x": 5, "y": 5, "z": 5}},
            "Bounding Sphere": {"Center": {"x": 0, "y": 0, "z": 0}, "Radius": 10.0},
            "Name": "StarfieldMesh",
            "Translation": {"x": 0, "y": 0, "z": 0},
            "Rotation": {},
            "Scale": 1.0,
        })

        nif = MagicMock()
        nif.schema = _make_mock_schema()

        shapes = {}
        _prepare_walk_blocks(nif, block, shapes, "main")
        assert 5 in shapes
        assert shapes[5].external_mesh_paths == ["geometries/test.mesh"]

    def test_bsgeometry_without_external_paths_skipped(self):
        """BSGeometry with inline data (no external paths) does not create placeholder."""
        from creation_lib.renderer.nif_loader import _prepare_walk_blocks

        meshes = [
            {"Has Mesh": 1, "Mesh": {"Mesh Data": {"Num Verts": 100}, "Indices Size": 300, "Num Verts": 100, "Flags": 512}},
            {"Has Mesh": 0}, {"Has Mesh": 0}, {"Has Mesh": 0},
        ]
        block = _make_mock_block("BSGeometry", 6, {
            "Meshes": meshes,
            "Name": "InlineGeom",
        })

        nif = MagicMock()
        nif.schema = _make_mock_schema()

        shapes = {}
        _prepare_walk_blocks(nif, block, shapes, "main")
        # Inline geometry without vertex data list won't produce a shape
        # (it's handled differently from BSTriShape; the placeholder is only for external)
        assert 6 not in shapes


class TestValidationExternalGeometry:
    """Test that validation panel detects external geometry warnings."""

    def test_check_external_geometry_warns(self):
        from ui.editor.panels.validation import ValidationPanel

        app = MagicMock()
        nif = MagicMock()
        schema = _make_mock_schema()
        nif.schema = schema

        meshes = [
            {"Has Mesh": 1, "Mesh": {"Mesh Path": "geometries/gun.mesh", "Indices Size": 0, "Num Verts": 0, "Flags": 0}},
            {"Has Mesh": 0}, {"Has Mesh": 0}, {"Has Mesh": 0},
        ]
        block = _make_mock_block("BSGeometry", 5, {
            "Meshes": meshes,
            "Name": "StarfieldGun",
        })
        nif.blocks = [block]

        panel = ValidationPanel(app)
        panel._check_external_geometry(nif)
        assert len(panel._issues) == 1
        severity, block_id, msg = panel._issues[0]
        assert severity == "WARNING"
        assert block_id == 5
        assert "external geometry" in msg.lower()
        assert "geometries/gun.mesh" in msg

    def test_no_warning_for_inline_geometry(self):
        from ui.editor.panels.validation import ValidationPanel

        app = MagicMock()
        nif = MagicMock()
        schema = _make_mock_schema()
        nif.schema = schema

        meshes = [
            {"Has Mesh": 1, "Mesh": {"Mesh Data": {"Num Verts": 100}, "Indices Size": 300, "Num Verts": 100, "Flags": 512}},
            {"Has Mesh": 0}, {"Has Mesh": 0}, {"Has Mesh": 0},
        ]
        block = _make_mock_block("BSGeometry", 6, {
            "Meshes": meshes,
            "Name": "InlineGeom",
        })
        nif.blocks = [block]

        panel = ValidationPanel(app)
        panel._check_external_geometry(nif)
        assert len(panel._issues) == 0
