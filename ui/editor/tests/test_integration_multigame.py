"""End-to-end integration tests for multi-game NIF editor support.

Tests the full pipeline: game detection, session management, archive resolution,
validation checks, and Starfield external geometry — without requiring OpenGL.
"""
import pytest
from pathlib import Path
from unittest.mock import MagicMock, patch
import tempfile
import os

from creation_lib.nif.nif_file import NifFile
from creation_lib.core.game_profiles import detect_game, get_profile, GAME_PROFILES
from ui.editor.nif_session import NifSession, NifRegistry
from creation_lib.renderer.scene_renderer import SceneNode
from creation_lib.renderer.nif_loader import (
    PreparedShape,
    PreparedNifData,
    _extract_external_geometry_info,
    _make_bbox_placeholder,
    _prepare_walk_blocks,
)

FIXTURES = Path(__file__).parent / "fixtures"


# ---------------------------------------------------------------------------
# 1. Game detection from real NIF fixtures (skip if not available)
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("filename,expected_game", [
    ("fo4_sample.nif", "fo4"),
    ("skyrimse_sample.nif", "skyrimse"),
    ("fo76_sample.nif", "fo76"),
    ("starfield_sample.nif", "starfield"),
])
def test_detect_game_from_fixture_nif(filename, expected_game):
    """Load a real NIF file and verify game detection from BS version."""
    path = FIXTURES / filename
    if not path.exists():
        pytest.skip(f"Fixture {filename} not available")
    nif = NifFile.load(str(path))
    profile = detect_game(nif.header.bs_version)
    assert profile is not None
    assert profile.id == expected_game


# ---------------------------------------------------------------------------
# 2. NifFile.new() roundtrip for all games
# ---------------------------------------------------------------------------

class TestNifNewRoundtrip:
    """Verify NifFile.new() creates valid headers for every registered game."""

    @pytest.mark.parametrize("game_id", list(GAME_PROFILES.keys()))
    def test_new_nif_has_correct_bs_version(self, game_id):
        profile = get_profile(game_id)
        nif = NifFile.new(game_id)
        lo, hi = profile.bs_version_range
        assert lo <= nif.header.bs_version <= hi

    @pytest.mark.parametrize("game_id", list(GAME_PROFILES.keys()))
    def test_new_nif_detects_back_to_same_game(self, game_id):
        nif = NifFile.new(game_id)
        detected = detect_game(nif.header.bs_version)
        assert detected is not None
        assert detected.id == game_id

    @pytest.mark.parametrize("game_id", list(GAME_PROFILES.keys()))
    def test_new_nif_has_root_block(self, game_id):
        nif = NifFile.new(game_id)
        assert len(nif.blocks) >= 1
        assert nif.blocks[0].type_name in ("BSFadeNode", "NiNode")

    @pytest.mark.parametrize("game_id", list(GAME_PROFILES.keys()))
    def test_new_nif_sets_detected_game(self, game_id):
        nif = NifFile.new(game_id)
        assert nif.detected_game is not None
        assert nif.detected_game.id == game_id


# ---------------------------------------------------------------------------
# 3. Cross-game session management
# ---------------------------------------------------------------------------

class TestCrossGameSessionIntegration:
    """Test NifRegistry cross-game attachment validation end-to-end."""

    def _make_session(self, nif_id, game_id, parent_nif_id=None):
        profile = get_profile(game_id)
        return NifSession(
            nif_id=nif_id,
            nif=MagicMock(),
            file_path=f"/fake/{nif_id}.nif",
            scene_root=SceneNode(name="root", block_id=-1),
            anim_manager=None,
            parent_nif_id=parent_nif_id,
            game_profile=profile,
        )

    def test_same_game_sessions_allowed(self):
        reg = NifRegistry()
        main = self._make_session("main", "fo4")
        reg.add_session(main)
        child = self._make_session("child", "fo4", parent_nif_id="main")
        reg.add_session(child)  # Should not raise
        assert "child" in reg.sessions

    def test_cross_game_sessions_allowed_with_mismatch(self):
        reg = NifRegistry()
        main = self._make_session("main", "fo4")
        reg.add_session(main)
        child = self._make_session("child", "skyrimse", parent_nif_id="main")
        reg.add_session(child)  # should NOT raise
        assert "child" in reg.sessions
        assert child.cross_game_mismatch is True

    def test_all_cross_game_pairs_set_mismatch(self):
        """Every cross-game pair should set mismatch flag."""
        games = list(GAME_PROFILES.keys())
        for parent_game in games:
            for child_game in games:
                if parent_game == child_game:
                    continue
                reg = NifRegistry()
                main = self._make_session("main", parent_game)
                reg.add_session(main)
                child = self._make_session("child", child_game, parent_nif_id="main")
                reg.add_session(child)
                assert child.cross_game_mismatch is True

    def test_no_profile_skips_validation(self):
        """Sessions without game_profile don't trigger cross-game check."""
        reg = NifRegistry()
        main = NifSession(
            nif_id="main", nif=MagicMock(), file_path="/fake/main.nif",
            scene_root=SceneNode(name="root", block_id=-1),
            anim_manager=None, game_profile=None,
        )
        reg.add_session(main)
        child = NifSession(
            nif_id="child", nif=MagicMock(), file_path="/fake/child.nif",
            scene_root=SceneNode(name="root", block_id=-1),
            anim_manager=None, parent_nif_id="main", game_profile=None,
        )
        reg.add_session(child)  # Should not raise
        assert "child" in reg.sessions


# ---------------------------------------------------------------------------
# 4. Archive manager integration
# ---------------------------------------------------------------------------

class TestArchiveManagerIntegration:
    """Test BA2Manager handles mixed BA2 + BSA formats."""

    def test_scan_mixed_ba2_bsa(self):
        from creation_lib.ba2 import BA2Manager
        mgr = BA2Manager()
        with tempfile.TemporaryDirectory() as tmp:
            for name in ["Skyrim.bsa", "Fallout.ba2", "other.zip"]:
                Path(tmp, name).touch()
            with patch("creation_lib.ba2.ba2_manager.BSAReader") as mock_bsa_cls, \
                 patch("creation_lib.ba2.ba2_manager.BA2File") as mock_ba2_cls:
                mock_bsa_cls.return_value.file_count = 0
                mock_ba2_cls.return_value.file_count = 0
                mgr._scan_dir(Path(tmp))
        # Should find 2 archives, skip .zip
        assert len(mgr._archives) == 2

    def test_find_returns_first_hit(self):
        from creation_lib.ba2 import BA2Manager
        mgr = BA2Manager()
        mock_bsa = MagicMock()
        mock_bsa.extract.return_value = b"bsa_data"
        mock_ba2 = MagicMock()
        mock_ba2.extract.return_value = None
        mgr._archives = [mock_ba2, mock_bsa]
        result = mgr.find("textures/test.dds")
        assert result == b"bsa_data"


# ---------------------------------------------------------------------------
# 5. Game profile consistency
# ---------------------------------------------------------------------------

class TestGameProfileConsistency:
    """Verify all game profiles have required fields and non-overlapping BS version ranges."""

    def test_all_profiles_have_display_name(self):
        for game_id, profile in GAME_PROFILES.items():
            assert profile.display_name, f"{game_id} missing display_name"

    def test_all_profiles_have_bs_version_range(self):
        for game_id, profile in GAME_PROFILES.items():
            lo, hi = profile.bs_version_range
            assert lo <= hi, f"{game_id} has invalid bs_version_range"

    def test_bs_version_ranges_dont_overlap(self):
        profiles = list(GAME_PROFILES.values())
        for i, a in enumerate(profiles):
            for b in profiles[i + 1:]:
                a_lo, a_hi = a.bs_version_range
                b_lo, b_hi = b.bs_version_range
                overlaps = a_lo <= b_hi and b_lo <= a_hi
                assert not overlaps, (
                    f"{a.id} ({a_lo}-{a_hi}) overlaps with {b.id} ({b_lo}-{b_hi})"
                )

    def test_all_profiles_have_archive_extensions(self):
        for game_id, profile in GAME_PROFILES.items():
            assert profile.archive_extensions, f"{game_id} missing archive_extensions"

    def test_all_profiles_have_material_format(self):
        for game_id, profile in GAME_PROFILES.items():
            assert profile.material_format, f"{game_id} missing material_format"

    def test_detect_game_returns_correct_profile_for_all_ranges(self):
        """Detect game at the low end of each profile's BS version range."""
        for game_id, profile in GAME_PROFILES.items():
            lo, _ = profile.bs_version_range
            detected = detect_game(lo)
            assert detected is not None, f"detect_game({lo}) returned None for {game_id}"
            assert detected.id == game_id


# ---------------------------------------------------------------------------
# 6. Validation panel integration
# ---------------------------------------------------------------------------

class TestValidationIntegration:
    """Test validation panel detects game-specific issues."""

    def _make_panel(self):
        from ui.editor.panels.validation import ValidationPanel
        app = MagicMock()
        return ValidationPanel(app)

    def _make_mock_block(self, type_name, block_id, fields):
        block = MagicMock()
        block.type_name = type_name
        block.block_id = block_id
        block.get_field = lambda name: fields.get(name)
        return block

    def _make_mock_nif(self, blocks, schema_hierarchy=None):
        nif = MagicMock()
        nif.blocks = blocks
        if schema_hierarchy is None:
            schema_hierarchy = {
                "BSTriShape": ["BSTriShape", "NiAVObject"],
                "BSGeometry": ["BSGeometry", "NiAVObject"],
                "NiNode": ["NiNode", "NiAVObject"],
            }
        nif.schema.is_subtype_of = lambda t, base: base in schema_hierarchy.get(t, [t])
        return nif

    def test_external_geometry_warning_in_validation(self):
        """Validation detects BSGeometry with external mesh path."""
        meshes = [
            {"Has Mesh": 1, "Mesh": {"Mesh Path": "geometries/gun.mesh", "Flags": 0}},
            {"Has Mesh": 0}, {"Has Mesh": 0}, {"Has Mesh": 0},
        ]
        block = self._make_mock_block("BSGeometry", 5, {
            "Meshes": meshes, "Name": "Gun",
        })
        nif = self._make_mock_nif([block])
        panel = self._make_panel()
        panel._check_external_geometry(nif)
        assert any("external geometry" in msg.lower() for _, _, msg in panel._issues)

    def test_no_external_geometry_warning_for_non_bsgeometry(self):
        """Non-BSGeometry blocks don't trigger external geometry warning."""
        block = self._make_mock_block("BSTriShape", 5, {
            "Vertex Data": [{"Vertex": {"x": 0, "y": 0, "z": 0}}],
            "Triangles": [{"v1": 0, "v2": 0, "v3": 0}],
        })
        nif = self._make_mock_nif([block])
        panel = self._make_panel()
        panel._check_external_geometry(nif)
        assert len(panel._issues) == 0


# ---------------------------------------------------------------------------
# 7. Starfield BSGeometry in loader pipeline
# ---------------------------------------------------------------------------

class TestStarfieldLoaderPipeline:
    """Integration test: BSGeometry detection through the prepare phase."""

    def _make_mock_block(self, type_name, block_id, fields):
        block = MagicMock()
        block.type_name = type_name
        block.block_id = block_id
        block.get_field = lambda name: fields.get(name)
        return block

    def _make_nif_with_bsgeometry(self):
        """Create a mock NIF with NiNode root containing a BSGeometry child."""
        schema = MagicMock()
        hierarchy = {
            "BSGeometry": ["BSGeometry", "NiAVObject"],
            "BSTriShape": ["BSTriShape", "NiAVObject"],
            "NiNode": ["NiNode", "NiAVObject"],
            "BSFadeNode": ["BSFadeNode", "NiNode", "NiAVObject"],
        }
        schema.is_subtype_of = lambda t, base: base in hierarchy.get(t, [t])

        meshes = [
            {"Has Mesh": 1, "Mesh": {"Mesh Path": "geometries/weapon/gun.mesh", "Flags": 0}},
            {"Has Mesh": 0}, {"Has Mesh": 0}, {"Has Mesh": 0},
        ]
        geom_block = self._make_mock_block("BSGeometry", 1, {
            "Meshes": meshes,
            "Bounding Box": {"Center": {"x": 0, "y": 0, "z": 0}, "Dimensions": {"x": 10, "y": 10, "z": 10}},
            "Bounding Sphere": {"Center": {"x": 0, "y": 0, "z": 0}, "Radius": 15.0},
            "Name": "GunMesh",
            "Translation": {"x": 0, "y": 0, "z": 0},
            "Rotation": {},
            "Scale": 1.0,
        })

        root_block = self._make_mock_block("BSFadeNode", 0, {
            "Children": [1],
            "Name": "Root",
            "Translation": {"x": 0, "y": 0, "z": 0},
            "Rotation": {},
            "Scale": 1.0,
        })

        nif = MagicMock()
        nif.schema = schema
        nif.blocks = [root_block, geom_block]
        nif.get_block = lambda i: nif.blocks[i] if 0 <= i < len(nif.blocks) else None
        return nif

    def test_prepare_walk_finds_bsgeometry_under_ninode(self):
        """_prepare_walk_blocks finds BSGeometry children of NiNode roots."""
        nif = self._make_nif_with_bsgeometry()
        shapes = {}
        _prepare_walk_blocks(nif, nif.blocks[0], shapes, "main")
        assert 1 in shapes
        ps = shapes[1]
        assert ps.external_mesh_paths == ["geometries/weapon/gun.mesh"]
        assert ps.verts.shape == (8, 3)
        assert ps.tris.shape[0] == 12

    def test_placeholder_uses_bounding_box_dimensions(self):
        """Placeholder verts span the BSBoundingBox dimensions."""
        nif = self._make_nif_with_bsgeometry()
        shapes = {}
        _prepare_walk_blocks(nif, nif.blocks[0], shapes, "main")
        ps = shapes[1]
        # BBox center=0, dims=10 => verts from -10 to +10
        assert ps.verts.min() == pytest.approx(-10.0)
        assert ps.verts.max() == pytest.approx(10.0)

    def test_normal_bstrishape_unaffected(self):
        """BSTriShape blocks still go through normal extraction (not placeholder)."""
        schema = MagicMock()
        hierarchy = {
            "BSTriShape": ["BSTriShape", "NiAVObject"],
            "NiNode": ["NiNode", "NiAVObject"],
        }
        schema.is_subtype_of = lambda t, base: base in hierarchy.get(t, [t])

        block = self._make_mock_block("BSTriShape", 0, {
            "Vertex Data": [
                {"Vertex": {"x": 0, "y": 0, "z": 0}},
                {"Vertex": {"x": 1, "y": 0, "z": 0}},
                {"Vertex": {"x": 0, "y": 1, "z": 0}},
            ],
            "Triangles": [{"v1": 0, "v2": 1, "v3": 2}],
            "Name": "TestTri",
            "Translation": {"x": 0, "y": 0, "z": 0},
            "Rotation": {},
            "Scale": 1.0,
        })
        nif = MagicMock()
        nif.schema = schema
        shapes = {}
        _prepare_walk_blocks(nif, block, shapes, "main")
        assert 0 in shapes
        assert shapes[0].external_mesh_paths is None  # Not external geometry


# ---------------------------------------------------------------------------
# 8. Full test suite regression check
# ---------------------------------------------------------------------------

class TestAllTestsRunnable:
    """Meta-test: verify all game profiles can be instantiated."""

    def test_all_game_profiles_instantiable(self):
        for game_id in GAME_PROFILES:
            profile = get_profile(game_id)
            assert profile.id == game_id
            NifFile.new(game_id)  # Should not raise
