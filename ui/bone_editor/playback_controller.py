"""Animation playback controller for the bone editor.

Owns the transport state (playing/looping/current frame) and a pre-computed
list of per-frame world poses. Does NOT load HKX itself — the app layer
pre-samples a clip into world frames and hands them off via `set_frames()`.
This keeps the controller trivially unit-testable with fake data.

Frame format:
    frames: list[dict[bone_name, (rot_quat_xyzw: np.ndarray(4),
                                  world_pos_xyz: np.ndarray(3))]]

The controller enforces read-only editing semantics via PoseSession:
 - `is_active()` flips PoseSession._playback_active, which gates
   ViewportInteract.handle_input (gizmo/click/hotkeys) and
   PoseSession._push_undo (no new undo entries).
 - On one-shot stop the session's `playback_pose` is cleared, snapping
   the viewport back to baseline * user-delta.
"""
from __future__ import annotations

import bisect
import math
from typing import Optional

import numpy as np

from creation_lib.bone_edit.quat_util import mat_to_quat


WorldFrame = dict  # bone_name -> (rot_quat_xyzw np.ndarray(4), world_pos np.ndarray(3))


# --------------------------------------------------------------------------
# Clip sampling — turns an AnimationClip (per-bone keyframe channels)
# into a list of pre-computed world poses, one per frame, so the viewport can
# consume the result without re-walking the FK chain every frame.
# --------------------------------------------------------------------------


def _slerp(q1, q2, t: float):
    dot = sum(a * b for a, b in zip(q1, q2))
    if dot < 0.0:
        q2 = tuple(-c for c in q2)
        dot = -dot
    dot = min(dot, 1.0)
    if dot > 0.9995:
        result = tuple(a + t * (b - a) for a, b in zip(q1, q2))
        norm = math.sqrt(sum(c * c for c in result))
        return tuple(c / norm for c in result) if norm > 0 else tuple(q1)
    theta = math.acos(dot)
    sin_theta = math.sin(theta)
    s1 = math.sin((1.0 - t) * theta) / sin_theta
    s2 = math.sin(t * theta) / sin_theta
    result = tuple(s1 * a + s2 * b for a, b in zip(q1, q2))
    norm = math.sqrt(sum(c * c for c in result))
    return tuple(c / norm for c in result) if norm > 0 else tuple(q1)


def _lerp_vec(v1, v2, t: float):
    return tuple(a + t * (b - a) for a, b in zip(v1, v2))


def _sample_channel_at(keyframes, t: float, is_rotation: bool):
    if not keyframes:
        return None
    if len(keyframes) == 1:
        return keyframes[0].value
    times = [kf.time for kf in keyframes]
    idx = bisect.bisect_right(times, t)
    if idx == 0:
        return keyframes[0].value
    if idx >= len(keyframes):
        return keyframes[-1].value
    kf0 = keyframes[idx - 1]
    kf1 = keyframes[idx]
    span = kf1.time - kf0.time
    if span <= 0:
        return kf0.value
    frac = (t - kf0.time) / span
    if is_rotation:
        return _slerp(kf0.value, kf1.value, frac)
    return _lerp_vec(kf0.value, kf1.value, frac)


def sample_clip_to_world_frames(clip, skeleton_dict: dict, fps: float = 30.0
                                ) -> list[WorldFrame]:
    """Pre-sample an AnimationClip into one world pose per frame.

    Uses the same `_compute_world_transforms` helper as animation_loader so
    the world poses match what `load_sighted_pose` produces for frame 0.

    Returns:
        list of frame dicts: `bone_name -> (rot_quat_xyzw, world_pos)`.
        `rot_quat_xyzw` is a length-4 np.ndarray, `world_pos` is length-3.
    """
    from ui.aligner.animation_loader import _compute_world_transforms

    duration = float(getattr(clip, "duration", 0.0) or 0.0)
    if duration <= 0.0:
        return []

    bone_names = list(skeleton_dict["bone_names"])
    num_bones = len(bone_names)
    channels = {ch.bone_name: ch for ch in clip.channels}

    num_frames = max(2, int(round(duration * fps)) + 1)
    out: list[WorldFrame] = []

    for frame_idx in range(num_frames):
        t = min(duration, frame_idx / fps)

        anim_rotations: list[list[float] | None] = [None] * num_bones
        anim_translations: list[list[float] | None] = [None] * num_bones
        for i, name in enumerate(bone_names):
            ch = channels.get(name)
            if ch is None:
                continue
            rot = _sample_channel_at(ch.rotations, t, is_rotation=True)
            if rot is not None:
                anim_rotations[i] = list(rot)
            trans = _sample_channel_at(ch.translations, t, is_rotation=False)
            if trans is not None:
                anim_translations[i] = list(trans)

        world_positions, world_rotations = _compute_world_transforms(
            skeleton_dict, anim_rotations, anim_translations,
        )

        frame: WorldFrame = {}
        for name in bone_names:
            if name not in world_positions or name not in world_rotations:
                continue
            rot_mat = world_rotations[name]
            rot_q = mat_to_quat(rot_mat)
            pos = np.asarray(world_positions[name], dtype=np.float64)
            frame[name] = (rot_q, pos)
        out.append(frame)

    return out


class PlaybackController:
    def __init__(self) -> None:
        self.frames: list[WorldFrame] = []
        self.frame_rate: float = 30.0
        self.current_frame: int = 0
        self._time_accumulator: float = 0.0
        self._playing: bool = False
        self._looping: bool = False

        # Set by BoneEditorApp after construction so the controller can
        # push per-frame poses into the single render source of truth.
        # Keep the coupling narrow: only writes `playback_pose` and
        # `_playback_active`.
        self.pose_session = None  # type: ignore[assignment]

    # --------------------------------------------------------------
    # Data loading
    # --------------------------------------------------------------

    def set_frames(self, frames: list[WorldFrame], fps: float = 30.0) -> None:
        """Replace the loaded animation with a new pre-sampled frame list."""
        self.stop()
        self.frames = list(frames)
        self.frame_rate = max(1.0, float(fps))
        self.current_frame = 0
        self._time_accumulator = 0.0

    def clear(self) -> None:
        self.stop()
        self.frames = []

    def has_frames(self) -> bool:
        return bool(self.frames)

    # --------------------------------------------------------------
    # Transport
    # --------------------------------------------------------------

    def play(self, loop: bool = False) -> None:
        if not self.frames:
            return
        self._playing = True
        self._looping = bool(loop)
        self.current_frame = 0
        self._time_accumulator = 0.0
        self._push_current_pose()
        self._set_session_active(True)

    def stop(self) -> None:
        self._playing = False
        self._looping = False
        self.current_frame = 0
        self._time_accumulator = 0.0
        self._clear_session_pose()
        self._set_session_active(False)

    # --------------------------------------------------------------
    # Status
    # --------------------------------------------------------------

    def is_active(self) -> bool:
        return self._playing

    def is_looping(self) -> bool:
        return self._playing and self._looping

    def current_pose(self) -> Optional[WorldFrame]:
        if not self._playing or not self.frames:
            return None
        idx = max(0, min(self.current_frame, len(self.frames) - 1))
        return self.frames[idx]

    def progress(self) -> float:
        """0.0 - 1.0 playback progress for progress-bar style UI."""
        if not self.frames:
            return 0.0
        return float(self.current_frame) / float(max(1, len(self.frames) - 1))

    # --------------------------------------------------------------
    # Per-frame tick
    # --------------------------------------------------------------

    def update(self, dt: float) -> None:
        """Advance playback time by `dt` seconds.

        One-shot playback stops at the last frame (does NOT wrap). Loop
        playback wraps modulo frame count. Either way, the current frame's
        pose is pushed into `pose_session.playback_pose`.
        """
        if not self._playing or not self.frames:
            return

        self._time_accumulator += max(0.0, float(dt))
        frame_dur = 1.0 / self.frame_rate
        advanced = False
        while self._time_accumulator >= frame_dur:
            self._time_accumulator -= frame_dur
            self.current_frame += 1
            advanced = True

            if self.current_frame >= len(self.frames):
                if self._looping:
                    self.current_frame %= len(self.frames)
                else:
                    # One-shot finished: snap back, clear playback state.
                    self.stop()
                    return

        if advanced:
            self._push_current_pose()

    # --------------------------------------------------------------
    # PoseSession plumbing
    # --------------------------------------------------------------

    def _push_current_pose(self) -> None:
        if self.pose_session is None or not self.frames:
            return
        idx = max(0, min(self.current_frame, len(self.frames) - 1))
        # PoseSession reads this dict on every get_world_pose() call while
        # playback_pose is not None (see PoseSession._get_local).
        self.pose_session.playback_pose = self.frames[idx]

    def _clear_session_pose(self) -> None:
        if self.pose_session is None:
            return
        self.pose_session.playback_pose = None

    def _set_session_active(self, active: bool) -> None:
        if self.pose_session is None:
            return
        self.pose_session._playback_active = bool(active)
