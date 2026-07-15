"""Gizmo-drag undo batching — every drag must be ONE undo entry, not N.

Regression from the bone-editor rewrite: every frame of a drag pushed its
own undo entry, so Undo after an arm rotation only reverted the last frame.
`PoseSession.begin_drag()` / `end_drag()` are the batch boundary; while
`_in_drag` is True, `_push_undo()` is a no-op.
"""
from pathlib import Path

import numpy as np


SKELETON_HKX = Path("resource/skeleton.hkx")


def _make_session():
    from creation_lib.bone_edit.skeleton import SkeletonManager
    from ui.bone_editor.pose_session import PoseSession

    skel = SkeletonManager.from_hkx(SKELETON_HKX)
    return PoseSession(skeleton=skel)


def test_drag_batches_many_set_local_rotation_calls_into_one_undo_entry():
    s = _make_session()
    bone = "RArm_Hand"
    assert s._undo_stack == []

    s.begin_drag()
    # Simulate a gizmo drag across many frames: each frame the viewport
    # converts the gizmo delta into a different parent-local quat and calls
    # set_local_rotation. Previously, each call pushed its own undo entry.
    quats = [
        np.array([0.0, 0.0, 0.05, 0.9987]),
        np.array([0.0, 0.0, 0.10, 0.9950]),
        np.array([0.0, 0.0, 0.15, 0.9887]),
        np.array([0.0, 0.0, 0.20, 0.9798]),
        np.array([0.0, 0.0, 0.25, 0.9682]),
    ]
    for q in quats:
        s.set_local_rotation(bone, q)
    s.end_drag()

    assert len(s._undo_stack) == 1, (
        f"drag should push exactly one undo entry, got {len(s._undo_stack)}"
    )
    np.testing.assert_allclose(s.pose.rotations[bone], quats[-1])

    # One undo should revert the ENTIRE drag, not just the last frame.
    s.undo()
    assert s.pose.is_empty()


def test_noop_drag_does_not_grow_undo_stack():
    s = _make_session()
    assert s._undo_stack == []

    # User clicks on the gizmo but never actually moves it — begin_drag
    # fires on rising edge, end_drag on falling edge, but set_local_rotation
    # is never called (or is called with identity, producing no pose change).
    s.begin_drag()
    s.end_drag()

    assert s._undo_stack == [], (
        "no-op drag must not grow undo stack "
        f"(got {len(s._undo_stack)} entries)"
    )
    assert s.pose.is_empty()


def test_drag_ik_tip_inside_drag_session_batches_into_one_entry():
    s = _make_session()
    if "RArm_Hand" not in s.chains:
        import pytest
        pytest.skip("Test skeleton has no RArm_Hand IK chain")

    chain = s.chains["RArm_Hand"]
    start_tip = s.get_world_pose()[chain.tip][1].copy()

    s.begin_drag()
    # Five successive IK solves (what a real drag produces, one per frame).
    for delta in (0.1, 0.2, 0.3, 0.4, 0.5):
        s.drag_ik_tip("RArm_Hand", start_tip + np.array([0.0, 0.0, delta]))
    s.end_drag()

    assert len(s._undo_stack) == 1, (
        f"IK drag should push exactly one undo entry, got {len(s._undo_stack)}"
    )

    # Undo should clear both root and mid rotations set by the solver.
    s.undo()
    assert s.pose.is_empty()


def test_viewport_interact_edge_detection_fires_begin_drag_before_new_mat():
    """Regression for the real bug: ImGuizmo reports is_using()=True for
    one or more frames BEFORE it produces a non-None new_mat. Before the
    fix, `begin_drag()` was gated inside `if new_mat is not None:`, so
    `_gizmo_was_using` was flipped True on the gated frames (by the
    unconditional assignment at the end of handle_input), `begin_drag`
    never fired, and every frame of the drag pushed its own undo entry.

    This test drives the edge detection directly via _update_drag_state,
    simulating: is_using=True for 2 gated frames (no new_mat), then
    is_using=True for 5 frames WITH new_mat (set_local_rotation calls),
    then is_using=False (drag released).
    """
    from ui.bone_editor.viewport_interact import ViewportInteract

    s = _make_session()
    interact = ViewportInteract(s)
    bone = "RArm_Hand"

    # Frame 1-2: gizmo is_using=True, no new_mat yet. Edge detection
    # MUST run these frames and fire begin_drag on the first one.
    interact._update_drag_state(True)
    interact._update_drag_state(True)
    assert s._in_drag, "begin_drag should fire on first is_using=True frame"

    # Frames 3-7: new_mat arrives, set_local_rotation called each frame.
    for z in (0.05, 0.10, 0.15, 0.20, 0.25):
        s.set_local_rotation(bone, np.array([0.0, 0.0, z, 0.99]))
        interact._update_drag_state(True)

    # Frame 8: user releases the gizmo.
    interact._update_drag_state(False)

    assert not s._in_drag
    assert len(s._undo_stack) == 1, (
        f"entire drag must collapse to ONE undo entry, got "
        f"{len(s._undo_stack)}"
    )

    s.undo()
    assert s.pose.is_empty()


def test_separate_drags_each_push_their_own_entry():
    s = _make_session()

    s.begin_drag()
    s.set_local_rotation("RArm_Hand", np.array([0.0, 0.0, 0.1, 0.995]))
    s.end_drag()

    s.begin_drag()
    s.set_local_rotation("LArm_Hand", np.array([0.0, 0.0, 0.1, 0.995]))
    s.end_drag()

    assert len(s._undo_stack) == 2

    s.undo()
    assert "LArm_Hand" not in s.pose.rotations
    assert "RArm_Hand" in s.pose.rotations

    s.undo()
    assert s.pose.is_empty()
