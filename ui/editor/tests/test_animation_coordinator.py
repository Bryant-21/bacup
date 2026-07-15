"""Tests for cross-NIF animation coordination."""
import pytest
from unittest.mock import MagicMock, call
from ui.editor.animation_coordinator import AnimationCoordinator
from ui.editor.nif_session import NifSession, NifRegistry
from creation_lib.renderer.scene_renderer import SceneNode


def _make_registry_with_anims(main_seqs, child_seqs):
    """Create registry with mock animation managers."""
    reg = NifRegistry()
    for nid, seqs in [("main", main_seqs), ("child_0", child_seqs)]:
        mgr = MagicMock()
        mgr.has_sequence = lambda name, s=seqs: name in s
        mgr.get_sequences.return_value = list(s for s in seqs)
        root = SceneNode(name="root", block_id=-1, nif_id=nid)
        reg.add_session(NifSession(
            nif_id=nid, nif=MagicMock(), file_path=f"{nid}.nif",
            scene_root=root, anim_manager=mgr,
            parent_nif_id=None if nid == "main" else "main",
        ))
    return reg


class TestAnimationCoordinator:
    def test_play_shared_sequence(self):
        reg = _make_registry_with_anims(["idle", "fire"], ["idle"])
        coord = AnimationCoordinator(reg)
        coord.play("idle")
        reg.get_session("main").anim_manager.play.assert_called_with("idle")
        reg.get_session("child_0").anim_manager.play.assert_called_with("idle")

    def test_play_unique_sequence(self):
        reg = _make_registry_with_anims(["idle", "fire"], ["idle"])
        coord = AnimationCoordinator(reg)
        coord.play("fire")
        reg.get_session("main").anim_manager.play.assert_called_with("fire")
        reg.get_session("child_0").anim_manager.play.assert_not_called()

    def test_select_shared_sequence_without_playing(self):
        reg = _make_registry_with_anims(["idle", "fire"], ["idle"])
        coord = AnimationCoordinator(reg)
        coord.select("idle")
        reg.get_session("main").anim_manager.select_sequence.assert_called_with("idle")
        reg.get_session("child_0").anim_manager.select_sequence.assert_called_with("idle")
        reg.get_session("main").anim_manager.play.assert_not_called()
        reg.get_session("child_0").anim_manager.play.assert_not_called()

    def test_get_all_sequences(self):
        reg = _make_registry_with_anims(["idle", "fire"], ["idle", "zoom"])
        coord = AnimationCoordinator(reg)
        seqs = coord.get_all_sequences()
        assert set(seqs.keys()) == {"idle", "fire", "zoom"}
        assert set(seqs["idle"]) == {"main", "child_0"}
        assert seqs["fire"] == ["main"]
        assert seqs["zoom"] == ["child_0"]

    def test_update_all(self):
        reg = _make_registry_with_anims(["idle"], ["idle"])
        coord = AnimationCoordinator(reg)
        coord.update(0.016)
        for s in reg.all_sessions():
            s.anim_manager.update.assert_called_once()

    def test_stop_all(self):
        reg = _make_registry_with_anims(["idle"], ["idle"])
        coord = AnimationCoordinator(reg)
        coord.stop()
        for s in reg.all_sessions():
            s.anim_manager.stop.assert_called_once()
