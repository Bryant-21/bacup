"""End-to-end integration tests for multi-game NIF support.

Parametrized tests covering game detection, NifFile.new(), profile roundtrip,
NifSession game_profile, cross-game validation, and archive format selection.
"""
import pytest
from unittest.mock import MagicMock

from creation_lib.nif.nif_file import NifFile
from creation_lib.core.game_profiles import (
    detect_game, get_profile, GAME_PROFILES,
    FO4_PROFILE, SKYRIMSE_PROFILE, FO76_PROFILE, STARFIELD_PROFILE,
)
from ui.editor.nif_session import NifSession, NifRegistry
from creation_lib.renderer.scene_renderer import SceneNode
from ui.editor.panels.validation import ValidationPanel


# All supported games with expected BS versions
_ALL_GAMES = [
    pytest.param("fo4", 130, "Fallout 4", id="fo4"),
    pytest.param("skyrimse", 100, "Skyrim Special Edition", id="skyrimse"),
    pytest.param("fo76", 155, "Fallout 76", id="fo76"),
    pytest.param("starfield", 170, "Starfield", id="starfield"),
]


def _make_session(nif_id="main", game_id=None, parent_nif_id=None):
    """Create a NifSession with optional game profile."""
    nif = MagicMock()
    nif.blocks = []
    scene_root = SceneNode(name="root", block_id=-1, nif_id=nif_id)
    anim_mgr = MagicMock()
    s = NifSession(
        nif_id=nif_id, nif=nif, file_path="test.nif",
        scene_root=scene_root, anim_manager=anim_mgr,
        parent_nif_id=parent_nif_id,
    )
    if game_id:
        s.game_profile = get_profile(game_id)
    return s


# ---------------------------------------------------------------------------
# 1. Game profile registry
# ---------------------------------------------------------------------------


class TestGameProfileRegistry:
    """Built-in game profiles are registered and have valid configuration."""

    def test_oblivion_profile_registered(self):
        profile = get_profile("oblivion")
        assert profile.display_name == "Oblivion"
        assert profile.bs_version_range == (10, 10)
        assert profile.collision_layer_enum == "OblivionLayer"
        assert profile.physics_material_enum == "OblivionHavokMaterial"

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_profile_registered(self, game_id, bs_version, display_name):
        profile = get_profile(game_id)
        assert profile.id == game_id
        assert profile.display_name == display_name

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_bs_version_in_range(self, game_id, bs_version, display_name):
        profile = get_profile(game_id)
        lo, hi = profile.bs_version_range
        assert lo <= bs_version <= hi

    def test_no_overlapping_bs_ranges(self):
        """BS version ranges must not overlap between games."""
        ranges = [(p.id, p.bs_version_range) for p in GAME_PROFILES.values()]
        for i, (id_a, (lo_a, hi_a)) in enumerate(ranges):
            for id_b, (lo_b, hi_b) in ranges[i + 1:]:
                assert hi_a < lo_b or hi_b < lo_a, (
                    f"Overlapping BS ranges: {id_a} [{lo_a}-{hi_a}] "
                    f"and {id_b} [{lo_b}-{hi_b}]"
                )

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_profile_has_env_var(self, game_id, bs_version, display_name):
        profile = get_profile(game_id)
        assert profile.env_var_name
        assert profile.env_var_name.endswith("_DIR")

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_profile_has_archive_config(self, game_id, bs_version, display_name):
        profile = get_profile(game_id)
        assert profile.archive_format in ("ba2", "bsa")
        assert len(profile.archive_extensions) >= 1

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_profile_has_shader_modules(self, game_id, bs_version, display_name):
        profile = get_profile(game_id)
        assert "common" in profile.shader_modules
        assert "lighting" in profile.shader_modules
        assert len(profile.effect_shader_modules) >= 1


# ---------------------------------------------------------------------------
# 2. Game detection from BS version
# ---------------------------------------------------------------------------


class TestDetectGame:
    """detect_game() maps BS version to correct profile."""

    def test_detect_oblivion_version(self):
        profile = detect_game(10)
        assert profile is not None
        assert profile.id == "oblivion"

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_detect_exact_version(self, game_id, bs_version, display_name):
        profile = detect_game(bs_version)
        assert profile is not None
        assert profile.id == game_id

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_detect_range_high(self, game_id, bs_version, display_name):
        """Detection works at the high end of each game's BS range."""
        profile = get_profile(game_id)
        result = detect_game(profile.bs_version_range[1])
        assert result is not None
        assert result.id == game_id

    def test_unknown_version_returns_none(self):
        assert detect_game(999) is None
        assert detect_game(0) is None
        assert detect_game(50) is None


# ---------------------------------------------------------------------------
# 3. NifFile.new() multi-game support
# ---------------------------------------------------------------------------


class TestNifFileNew:
    """NifFile.new() creates correct headers for each game."""

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_new_sets_bs_version(self, game_id, bs_version, display_name):
        nif = NifFile.new(game_id)
        assert nif.header.bs_version == bs_version

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_new_sets_nif_version(self, game_id, bs_version, display_name):
        nif = NifFile.new(game_id)
        assert nif.header.version == (20, 2, 0, 7)
        assert nif.header.user_version == 12

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_new_has_bsfadenode_root(self, game_id, bs_version, display_name):
        nif = NifFile.new(game_id)
        assert len(nif.blocks) == 1
        assert nif.blocks[0].type_name == "BSFadeNode"

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_new_sets_detected_game(self, game_id, bs_version, display_name):
        nif = NifFile.new(game_id)
        assert nif.detected_game is not None
        assert nif.detected_game.id == game_id

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_new_roundtrip_detection(self, game_id, bs_version, display_name):
        """NifFile.new() -> detect_game(bs_version) returns same profile."""
        nif = NifFile.new(game_id)
        detected = detect_game(nif.header.bs_version)
        assert detected is not None
        assert detected.id == game_id

    def test_fo4_alias(self):
        """FO4 (uppercase) alias works."""
        nif = NifFile.new("FO4")
        assert nif.header.bs_version == 130
        assert nif.detected_game.id == "fo4"

    def test_detected_game_none_on_fresh_init(self):
        nif = NifFile()
        assert nif.detected_game is None


# ---------------------------------------------------------------------------
# 4. NifSession game profile integration
# ---------------------------------------------------------------------------


class TestSessionGameProfile:
    """NifSession stores and uses game_profile correctly."""

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_session_stores_profile(self, game_id, bs_version, display_name):
        s = _make_session(game_id=game_id)
        assert s.game_profile is not None
        assert s.game_profile.id == game_id

    def test_session_profile_defaults_none(self):
        s = _make_session()
        assert s.game_profile is None


class TestCrossGameAttachment:
    """Cross-game NIF attachment sets mismatch flag instead of blocking."""

    def test_same_game_allowed(self):
        reg = NifRegistry()
        reg.add_session(_make_session("main", game_id="fo4"))
        reg.add_session(_make_session("child_0", game_id="fo4",
                                       parent_nif_id="main"))
        assert "child_0" in reg.sessions
        assert reg.sessions["child_0"].cross_game_mismatch is False

    @pytest.mark.parametrize("parent_game,child_game", [
        ("fo4", "skyrimse"),
        ("fo4", "starfield"),
        ("skyrimse", "fo76"),
        ("starfield", "fo4"),
    ])
    def test_cross_game_allowed_with_mismatch_flag(self, parent_game, child_game):
        """Cross-game attachment should succeed and set mismatch flag."""
        reg = NifRegistry()
        reg.add_session(_make_session("main", game_id=parent_game))
        child = _make_session("child_0", game_id=child_game,
                              parent_nif_id="main")
        # Should NOT raise — cross-game attachment is now allowed
        reg.add_session(child)
        assert "child_0" in reg.sessions
        assert child.cross_game_mismatch is True

    def test_no_profile_skips_check(self):
        reg = NifRegistry()
        reg.add_session(_make_session("main"))  # no game profile
        child = _make_session("child_0", game_id="skyrimse",
                              parent_nif_id="main")
        reg.add_session(child)  # should not raise
        assert "child_0" in reg.sessions
        assert child.cross_game_mismatch is False


# ---------------------------------------------------------------------------
# 5. Validation panel game-specific checks
# ---------------------------------------------------------------------------


def _make_validation_nif(bs_version, blocks=None):
    """Create a mock NIF with given BS version for validation testing."""
    nif = MagicMock()
    nif.blocks = blocks or []

    schema = MagicMock()
    schema.is_subtype_of = lambda t, base: t == base
    schema.get_all_fields = lambda t: []
    nif.schema = schema

    header = MagicMock()
    header.bs_version = bs_version
    nif.header = header
    return nif


class TestValidationBSVersion:
    """Validation checks BS version matches game profile."""

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_matching_version_no_warning(self, game_id, bs_version, display_name):
        profile = get_profile(game_id)
        nif = _make_validation_nif(bs_version)
        app = MagicMock()
        panel = ValidationPanel(app)
        panel._check_bs_version(nif, profile)
        assert len(panel._issues) == 0

    def test_mismatched_version_warns(self):
        """FO4 profile with Skyrim BS version triggers warning."""
        profile = get_profile("fo4")
        nif = _make_validation_nif(100)  # Skyrim version
        app = MagicMock()
        panel = ValidationPanel(app)
        panel._check_bs_version(nif, profile)
        assert len(panel._issues) == 1
        assert panel._issues[0][0] == "WARNING"
        assert "BS version" in panel._issues[0][2]


class TestValidationMaterialFormat:
    """Validation checks material format matches game profile."""

    def test_bgsm_on_starfield_warns(self):
        """BGSM material on Starfield NIF (expects .mat) triggers warning."""
        profile = get_profile("starfield")

        # Create mock BSTriShape with BGSM shader property
        shape = MagicMock()
        shape.type_name = "BSTriShape"
        shape.block_id = 0
        shape.get_field = lambda name: 1 if name == "Shader Property" else None

        shader = MagicMock()
        shader.type_name = "BSLightingShaderProperty"
        shader.block_id = 1
        shader.get_field = lambda name: "materials\\test.bgsm" if name == "Name" else None

        nif = MagicMock()
        nif.blocks = [shape, shader]
        nif.get_block = lambda i: [shape, shader][i]
        schema = MagicMock()
        schema.is_subtype_of = lambda t, base: (
            t == base or (t == "BSTriShape" and base == "BSTriShape")
        )
        nif.schema = schema

        app = MagicMock()
        panel = ValidationPanel(app)
        panel._check_material_format(nif, profile)
        assert any("BGSM" in issue[2] or "bgsm" in issue[2].lower()
                    for issue in panel._issues)


# ---------------------------------------------------------------------------
# 6. Material model per game
# ---------------------------------------------------------------------------


class TestMaterialModel:
    """Each game has the correct material model assigned."""

    def test_fo4_spec_gloss(self):
        assert FO4_PROFILE.material_model == "spec-gloss"

    def test_skyrimse_spec_gloss(self):
        assert SKYRIMSE_PROFILE.material_model == "spec-gloss"

    def test_fo76_metallic_roughness(self):
        assert FO76_PROFILE.material_model == "metallic-roughness"

    def test_starfield_metallic_roughness(self):
        assert STARFIELD_PROFILE.material_model == "metallic-roughness"

    def test_fo4_bgsm_format(self):
        assert FO4_PROFILE.material_format == "bgsm"

    def test_starfield_mat_format(self):
        assert STARFIELD_PROFILE.material_format == "mat"


# ---------------------------------------------------------------------------
# 7. Archive format per game
# ---------------------------------------------------------------------------


class TestArchiveFormat:
    """Each game uses the correct archive format."""

    def test_fo4_ba2(self):
        assert FO4_PROFILE.archive_format == "ba2"
        assert ".ba2" in FO4_PROFILE.archive_extensions

    def test_skyrimse_bsa(self):
        assert SKYRIMSE_PROFILE.archive_format == "bsa"
        assert ".bsa" in SKYRIMSE_PROFILE.archive_extensions

    def test_fo76_ba2(self):
        assert FO76_PROFILE.archive_format == "ba2"
        assert ".ba2" in FO76_PROFILE.archive_extensions

    def test_starfield_ba2(self):
        assert STARFIELD_PROFILE.archive_format == "ba2"
        assert ".ba2" in STARFIELD_PROFILE.archive_extensions


# ---------------------------------------------------------------------------
# 8. Shader module configuration per game
# ---------------------------------------------------------------------------


class TestShaderModules:
    """Each game has correct shader module lists."""

    def test_fo4_specgloss_shader(self):
        assert "specgloss" in FO4_PROFILE.shader_modules

    def test_fo76_metalrough_shader(self):
        assert "metalrough" in FO76_PROFILE.shader_modules

    def test_starfield_layered_shader(self):
        assert "starfield_layered" in STARFIELD_PROFILE.shader_modules

    def test_skyrimse_specgloss_shader(self):
        assert "specgloss" in SKYRIMSE_PROFILE.shader_modules

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_all_have_common_and_lighting(self, game_id, bs_version, display_name):
        profile = get_profile(game_id)
        assert "common" in profile.shader_modules
        assert "lighting" in profile.shader_modules


# ---------------------------------------------------------------------------
# 9. Lighting tuning defaults
# ---------------------------------------------------------------------------


class TestLightingDefaults:
    """Each game has a non-empty lighting tuning default."""

    @pytest.mark.parametrize("game_id,bs_version,display_name", _ALL_GAMES)
    def test_has_lighting_tuning(self, game_id, bs_version, display_name):
        profile = get_profile(game_id)
        assert profile.default_lighting_tuning  # non-empty string
        assert profile.default_lighting_preset  # non-empty string

    @pytest.mark.parametrize("game_id,expected", [
        ("fo4", "Fallout 4"),
        ("skyrimse", "Skyrim SE"),
        ("fo76", "Fallout 76"),
        ("starfield", "Starfield"),
    ])
    def test_lighting_tuning_values(self, game_id, expected):
        profile = get_profile(game_id)
        assert profile.default_lighting_tuning == expected
