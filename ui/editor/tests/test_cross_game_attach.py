"""Tests for cross-game NIF attachment with automatic conversion."""
from unittest.mock import MagicMock

from creation_lib.core.game_profiles import FO4_PROFILE, FO76_PROFILE


class TestCrossGameMismatchFlag:
    def test_mismatch_flag_set_on_cross_game_session(self):
        """NifSession should have cross_game_mismatch=True after cross-game add."""
        from ui.editor.nif_session import NifRegistry, NifSession
        from creation_lib.renderer.scene_renderer import SceneNode

        registry = NifRegistry()
        parent = NifSession(
            nif_id="main", nif=MagicMock(), file_path="parent.nif",
            scene_root=SceneNode(name="root", block_id=-1, nif_id="main"),
            anim_manager=MagicMock(), game_profile=FO4_PROFILE,
        )
        registry.add_session(parent)

        child = NifSession(
            nif_id="child_0", nif=MagicMock(), file_path="child.nif",
            scene_root=SceneNode(name="root", block_id=-1, nif_id="child_0"),
            anim_manager=MagicMock(),
            parent_nif_id="main", game_profile=FO76_PROFILE,
        )
        registry.add_session(child)
        assert child.cross_game_mismatch is True

    def test_same_game_no_mismatch(self):
        """Same-game attachment should not set mismatch flag."""
        from ui.editor.nif_session import NifRegistry, NifSession
        from creation_lib.renderer.scene_renderer import SceneNode

        registry = NifRegistry()
        parent = NifSession(
            nif_id="main", nif=MagicMock(), file_path="parent.nif",
            scene_root=SceneNode(name="root", block_id=-1, nif_id="main"),
            anim_manager=MagicMock(), game_profile=FO4_PROFILE,
        )
        registry.add_session(parent)

        child = NifSession(
            nif_id="child_0", nif=MagicMock(), file_path="child.nif",
            scene_root=SceneNode(name="root", block_id=-1, nif_id="child_0"),
            anim_manager=MagicMock(),
            parent_nif_id="main", game_profile=FO4_PROFILE,
        )
        registry.add_session(child)
        assert child.cross_game_mismatch is False
