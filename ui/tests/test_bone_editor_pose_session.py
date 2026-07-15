from pathlib import Path

import numpy as np
import pytest


SKELETON_HKX = Path("resource/skeleton.hkx")


def _make_session():
    from creation_lib.bone_edit.skeleton import SkeletonManager
    from ui.bone_editor.pose_session import PoseSession

    skel = SkeletonManager.from_hkx(SKELETON_HKX)
    return PoseSession(skeleton=skel)


def test_pose_session_starts_empty():
    s = _make_session()
    assert s.pose.is_empty()
    assert not s.can_undo()
    assert not s.can_redo()


def test_pose_session_set_local_rotation_pushes_undo():
    s = _make_session()
    s.set_local_rotation("RArm_Hand", np.array([0.0, 0.0, 0.1, 0.995]))
    assert "RArm_Hand" in s.pose.rotations
    assert s.can_undo()


def test_pose_session_undo_restores_previous():
    s = _make_session()
    s.set_local_rotation("RArm_Hand", np.array([0.0, 0.0, 0.1, 0.995]))
    s.undo()
    assert s.pose.is_empty()
    assert s.can_redo()


def test_pose_session_redo_re_applies():
    s = _make_session()
    q = np.array([0.0, 0.0, 0.1, 0.995])
    s.set_local_rotation("RArm_Hand", q)
    s.undo()
    s.redo()
    np.testing.assert_allclose(s.pose.rotations["RArm_Hand"], q)


def test_pose_session_reset_bone():
    s = _make_session()
    s.set_local_rotation("RArm_Hand", np.array([0.0, 0.0, 0.1, 0.995]))
    s.set_local_translation("RArm_Hand", np.array([1.0, 0.0, 0.0]))
    s.reset_bone("RArm_Hand")
    assert s.pose.is_empty()


def test_pose_session_reset_all():
    s = _make_session()
    s.set_local_rotation("RArm_Hand", np.array([0.0, 0.0, 0.1, 0.995]))
    s.set_local_translation("HEAD", np.array([0.0, 0.0, 0.5]))
    s.reset_all()
    assert s.pose.is_empty()


def test_pose_session_drag_ik_tip_writes_root_and_mid_rotations():
    s = _make_session()
    if "RArm_Hand" not in s.chains:
        pytest.skip("Test skeleton has no RArm_Hand IK chain")
    chain = s.chains["RArm_Hand"]
    cur_world = s.get_world_pose()[chain.tip][1]
    target = cur_world + np.array([0.0, 0.0, 0.5])
    s.drag_ik_tip("RArm_Hand", target)
    assert chain.root in s.pose.rotations
    assert chain.mid in s.pose.rotations


def test_pose_session_get_world_pose_includes_all_bones():
    s = _make_session()
    world = s.get_world_pose()
    assert "RArm_Hand" in world
    rot, pos = world["RArm_Hand"]
    assert rot.shape == (4,)
    assert pos.shape == (3,)


def test_pose_session_get_world_pose_reflects_translation_delta():
    s = _make_session()
    base_pos = s.get_world_pose()["RArm_Hand"][1]
    s.set_local_translation("RArm_Hand", np.array([5.0, 0.0, 0.0]))
    new_pos = s.get_world_pose()["RArm_Hand"][1]
    delta = new_pos - base_pos
    assert float(np.linalg.norm(delta)) > 1e-3
