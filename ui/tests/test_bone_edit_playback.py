"""Tests for bone editor animation playback controller + PoseSession integration.

These tests do not touch HKX — the controller takes a pre-computed frame list,
so we can construct fake frames and exercise all transport/update logic in
isolation. HKX loading is a thin wrapper exercised only via the UI layer.
"""
from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest


SKELETON_HKX = Path("resource/skeleton.hkx")


def _make_session():
    from creation_lib.bone_edit.skeleton import SkeletonManager
    from ui.bone_editor.pose_session import PoseSession

    skel = SkeletonManager.from_hkx(SKELETON_HKX)
    return PoseSession(skeleton=skel)


def _fake_frames(n: int, bone_names: list[str]) -> list[dict]:
    """Build n frames where each bone's world pos increases linearly with frame.

    Each entry is bone_name -> (rot_quat_xyzw, world_pos). Rotations are identity.
    """
    frames: list[dict] = []
    ident_q = np.array([0.0, 0.0, 0.0, 1.0])
    for i in range(n):
        frame = {}
        for b_idx, name in enumerate(bone_names):
            frame[name] = (
                ident_q.copy(),
                np.array([float(i), float(b_idx), 0.0]),
            )
        frames.append(frame)
    return frames


# --------------------------------------------------------------------------
# PlaybackController
# --------------------------------------------------------------------------

def test_playback_controller_starts_inactive():
    from ui.bone_editor.playback_controller import PlaybackController

    pb = PlaybackController()
    assert not pb.is_active()
    assert pb.current_pose() is None
    # Update on empty controller is a no-op
    pb.update(0.1)
    assert not pb.is_active()


def test_playback_controller_one_shot_stops_at_end():
    from ui.bone_editor.playback_controller import PlaybackController

    frames = _fake_frames(30, ["Bip01", "HEAD"])
    pb = PlaybackController()
    pb.set_frames(frames, fps=30.0)
    pb.play(loop=False)
    assert pb.is_active()

    # Drive 30 frames' worth of time (plus a tiny nudge so we definitely pass the end)
    for _ in range(31):
        pb.update(1.0 / 30.0)

    assert not pb.is_active(), "one-shot must stop itself at end"
    assert pb.current_pose() is None, "after one-shot end, current_pose should clear"


def test_playback_controller_loop_wraps():
    from ui.bone_editor.playback_controller import PlaybackController

    frames = _fake_frames(10, ["Bip01"])
    pb = PlaybackController()
    pb.set_frames(frames, fps=30.0)
    pb.play(loop=True)

    # Drive 15 frames' worth of time — index should wrap from 9 back into the
    # first half of the sequence. Loop must still be active.
    for _ in range(15):
        pb.update(1.0 / 30.0)

    assert pb.is_active(), "loop must stay active indefinitely"
    pose = pb.current_pose()
    assert pose is not None
    # Position x equals frame index in our fake sequence, so it should be < 10.
    x = float(pose["Bip01"][1][0])
    assert 0 <= x < 10


def test_playback_controller_stop_clears_state():
    from ui.bone_editor.playback_controller import PlaybackController

    frames = _fake_frames(20, ["Bip01"])
    pb = PlaybackController()
    pb.set_frames(frames, fps=30.0)
    pb.play(loop=True)
    pb.update(5 / 30.0)
    assert pb.is_active()

    pb.stop()
    assert not pb.is_active()
    assert pb.current_pose() is None


# --------------------------------------------------------------------------
# PoseSession integration — undo skip + playback_pose read
# --------------------------------------------------------------------------

def test_mutations_suppressed_during_playback():
    """All PoseSession mutation entry points are no-ops while _playback_active.

    Option A defence-in-depth: the read-only invariant lives in the model
    so UI call sites (bone panel right-click menu, viewport-panel overlay
    buttons, any future Mirror Pose hotkey, etc.) can't bypass it by
    forgetting to gate.
    """
    s = _make_session()
    s._playback_active = True
    undo_before = len(s._undo_stack)
    pose_before = s.pose.copy()

    # Direct setters
    s.set_local_rotation("RArm_Hand", np.array([0.0, 0.0, 0.1, 0.995]))
    s.set_local_translation("RArm_Hand", np.array([1.0, 0.0, 0.0]))

    # Undo / redo
    s.undo()
    s.redo()

    assert len(s._undo_stack) == undo_before, \
        "undo stack must not grow while playback is active"
    assert s.pose.equals(pose_before), \
        "pose must not be mutated while playback is active"


def test_reset_methods_suppressed_during_playback():
    """`reset_bone` / `reset_all` are reachable from the bone panel's
    right-click menu and the viewport-panel overlay toolbar. Both must
    become no-ops while playback is running so an accidentally-clicked
    reset during playback can't silently clobber the user's edits.
    """
    s = _make_session()
    # Establish a non-empty pose FIRST (before playback starts), so the
    # reset calls below have something to clobber if the gate doesn't work.
    s.set_local_rotation("RArm_Hand", np.array([0.0, 0.0, 0.1, 0.995]))
    s.set_local_translation("HEAD", np.array([0.0, 0.0, 0.5]))
    snapshot = s.pose.copy()

    s._playback_active = True

    s.reset_bone("RArm_Hand")
    assert s.pose.equals(snapshot), \
        "reset_bone must not mutate during playback"

    s.reset_all()
    assert s.pose.equals(snapshot), \
        "reset_all must not mutate during playback"


def test_get_world_pose_uses_playback_pose():
    """When playback_pose is set, world pose for each bone is composed from
    the playback frame instead of the bind pose."""
    s = _make_session()

    # Build a playback pose whose Bip01 (root) sits 100 units away in Z.
    # Every other bone stays at its bind-pose world position so that we
    # only need to verify the root bone to catch whether playback_pose is
    # being consulted at all.
    bind_world = s.get_world_pose()
    playback = {}
    for name, (rot_q, pos) in bind_world.items():
        playback[name] = (rot_q.copy(), pos.copy())

    # Translate only the root by +100 in Z.
    root_name = s.skeleton.bone_names[0]
    playback[root_name] = (
        bind_world[root_name][0].copy(),
        bind_world[root_name][1] + np.array([0.0, 0.0, 100.0]),
    )

    s.playback_pose = playback
    world = s.get_world_pose()
    delta_z = float(world[root_name][1][2] - bind_world[root_name][1][2])
    assert delta_z == pytest.approx(100.0, abs=1e-4)
