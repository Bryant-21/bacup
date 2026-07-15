"""Tests for NifSession, NifRegistry, and AttachmentNode."""
import pytest
from unittest.mock import MagicMock
from ui.editor.nif_session import NifSession, NifRegistry, AttachmentNode
from creation_lib.renderer.scene_renderer import SceneNode
from creation_lib.core.game_profiles import get_profile


def _mock_session(nif_id="main", file_path="test.nif", parent_nif_id=None,
                  attachment_point=None):
    """Create a NifSession with mock NIF and scene root."""
    nif = MagicMock()
    nif.blocks = []
    scene_root = SceneNode(name="root", block_id=-1, nif_id=nif_id)
    anim_mgr = MagicMock()
    return NifSession(
        nif_id=nif_id, nif=nif, file_path=file_path,
        scene_root=scene_root, anim_manager=anim_mgr,
        parent_nif_id=parent_nif_id, attachment_point=attachment_point,
    )


def _mock_session_with_game(nif_id="main", game_id="fo4", parent_nif_id=None,
                             **kw):
    s = _mock_session(nif_id=nif_id, parent_nif_id=parent_nif_id, **kw)
    s.game_profile = get_profile(game_id)
    return s


class TestNifSession:
    def test_defaults(self):
        s = _mock_session()
        assert s.nif_id == "main"
        assert s.dirty is False
        assert s.parent_nif_id is None
        assert s.attachment_point is None
        assert s.attachment_node is None
        assert isinstance(s.hidden_block_ids, set)

    def test_particle_runtime_default_is_none(self):
        s = _mock_session()
        assert s.particle_models == []
        assert s.particle_runtime is None

    def test_dirty_flag(self):
        s = _mock_session()
        s.dirty = True
        assert s.dirty is True

    def test_read_only_default(self):
        s = _mock_session()
        assert s.read_only is False

    def test_read_only_flag(self):
        s = _mock_session()
        s.read_only = True
        assert s.read_only is True


class TestNifRegistry:
    def test_add_and_get_session(self):
        reg = NifRegistry()
        s = _mock_session("main", "weapon.nif")
        reg.add_session(s)
        assert reg.get_session("main") is s
        assert reg.main_id == "main"

    def test_active_session(self):
        reg = NifRegistry()
        reg.add_session(_mock_session("main", "weapon.nif"))
        reg.add_session(_mock_session("child_0", "scope.nif",
                                       parent_nif_id="main",
                                       attachment_point="p-barrel"))
        assert reg.active_id == "main"
        reg.active_id = "child_0"
        assert reg.active_session.nif_id == "child_0"

    def test_get_children(self):
        reg = NifRegistry()
        reg.add_session(_mock_session("main", "weapon.nif"))
        reg.add_session(_mock_session("child_0", "scope.nif",
                                       parent_nif_id="main",
                                       attachment_point="p-barrel"))
        reg.add_session(_mock_session("child_1", "muzzle.nif",
                                       parent_nif_id="main",
                                       attachment_point="p-muzzle"))
        children = reg.get_children("main")
        assert len(children) == 2
        assert {c.nif_id for c in children} == {"child_0", "child_1"}

    def test_remove_session(self):
        reg = NifRegistry()
        reg.add_session(_mock_session("main", "weapon.nif"))
        reg.add_session(_mock_session("child_0", "scope.nif",
                                       parent_nif_id="main"))
        reg.remove_session("child_0")
        assert "child_0" not in reg.sessions
        assert reg.get_children("main") == []

    def test_remove_resets_active_to_main(self):
        reg = NifRegistry()
        reg.add_session(_mock_session("main", "weapon.nif"))
        reg.add_session(_mock_session("child_0", "scope.nif",
                                       parent_nif_id="main"))
        reg.active_id = "child_0"
        reg.remove_session("child_0")
        assert reg.active_id == "main"

    def test_all_sessions_order(self):
        reg = NifRegistry()
        reg.add_session(_mock_session("main", "weapon.nif"))
        reg.add_session(_mock_session("child_0", "scope.nif",
                                       parent_nif_id="main"))
        result = reg.all_sessions()
        assert result[0].nif_id == "main"
        assert result[1].nif_id == "child_0"

    def test_get_nonexistent_raises(self):
        reg = NifRegistry()
        with pytest.raises(KeyError):
            reg.get_session("nope")

    def test_next_child_id(self):
        reg = NifRegistry()
        reg.add_session(_mock_session("main", "weapon.nif"))
        assert reg.next_child_id() == "child_0"
        reg.add_session(_mock_session("child_0", "scope.nif",
                                       parent_nif_id="main"))
        assert reg.next_child_id() == "child_1"

    def test_has_multiple_nifs(self):
        reg = NifRegistry()
        reg.add_session(_mock_session("main", "weapon.nif"))
        assert reg.has_multiple_nifs is False
        reg.add_session(_mock_session("child_0", "scope.nif",
                                       parent_nif_id="main"))
        assert reg.has_multiple_nifs is True


class TestAttachmentNode:
    def test_fields(self):
        node = AttachmentNode(
            name="attach_p-barrel",
            block_id=-1,
            parent_nif_id="main",
            child_nif_id="child_0",
            connect_point_name="p-barrel",
        )
        assert node.is_attachment is True
        assert node.block_id == -1
        assert node.nif_id == ""  # belongs to neither NIF
        assert node.parent_nif_id == "main"
        assert node.child_nif_id == "child_0"
        assert node.connect_point_name == "p-barrel"
        # Inherited fields initialized correctly by dataclass
        assert isinstance(node.children, list)
        assert node.mesh is None
        assert node.visible is True

    def test_children_grafting(self):
        parent_root = SceneNode(name="weapon_root", block_id=0, nif_id="main")
        child_root = SceneNode(name="scope_root", block_id=0, nif_id="child_0")
        attach = AttachmentNode(
            name="attach_p-barrel",
            block_id=-1,
            parent_nif_id="main",
            child_nif_id="child_0",
            connect_point_name="p-barrel",
        )
        attach.children.append(child_root)
        parent_root.children.append(attach)
        # Walk tree: parent_root -> attach -> child_root
        assert parent_root.children[0] is attach
        assert attach.children[0] is child_root


class TestCrossGameAttachment:
    def test_same_game_attachment_allowed(self):
        reg = NifRegistry()
        parent = _mock_session_with_game("main", "fo4")
        reg.add_session(parent)
        child = _mock_session_with_game("child_0", "fo4", parent_nif_id="main")
        reg.add_session(child)  # should not raise
        assert "child_0" in reg.sessions

    def test_cross_game_attachment_allowed_with_mismatch(self):
        reg = NifRegistry()
        parent = _mock_session_with_game("main", "fo4")
        reg.add_session(parent)
        child = _mock_session_with_game("child_0", "skyrimse", parent_nif_id="main")
        reg.add_session(child)  # should NOT raise
        assert "child_0" in reg.sessions
        assert child.cross_game_mismatch is True

    def test_no_profile_skips_check(self):
        """When game_profile is None, cross-game check is skipped."""
        reg = NifRegistry()
        parent = _mock_session("main")
        reg.add_session(parent)
        child = _mock_session_with_game("child_0", "skyrimse", parent_nif_id="main")
        reg.add_session(child)  # should not raise
        assert "child_0" in reg.sessions

    def test_game_profile_field_defaults_to_none(self):
        s = _mock_session()
        assert s.game_profile is None
