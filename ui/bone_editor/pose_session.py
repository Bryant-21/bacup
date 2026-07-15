"""PoseSession — the bone editor's mutation API and single source of truth.

Wraps a PoseDelta with:
- Direct setters (set_local_rotation, set_local_translation)
- IK operations (drag_ik_tip, drag_ik_pole)
- Undo/redo with snapshot copies
- get_world_pose() — composes baseline (anim frame 0 OR bind pose) with deltas
  for both viewport rendering and skeleton overlay

The viewport and panels read from this single object. The apply pipeline
reads pose_session.pose directly. There is no preview/apply divergence.
"""

from __future__ import annotations

import logging
from typing import Dict, Optional

import numpy as np

from creation_lib.bone_edit.bone_classifier import BoneCategory, BoneClassifier, IkChain
from creation_lib.bone_edit.ik_solver import solve_two_bone_ik, world_rot_delta_to_local
from creation_lib.bone_edit.pose import PoseDelta
from creation_lib.bone_edit.quat_util import (
    mat_to_quat,
    quat_conjugate,
    quat_multiply,
    quat_normalize,
    quat_to_matrix,
)
from creation_lib.bone_edit.skeleton import SkeletonManager

_log = logging.getLogger("bone_editor.pose_session")

_UNDO_MAX = 64


class PoseSession:
    def __init__(
        self,
        skeleton: SkeletonManager,
        classifier: Optional[BoneClassifier] = None,
    ):
        self.skeleton = skeleton
        self.classifier = classifier or BoneClassifier()
        self.chains: Dict[str, IkChain] = self.classifier.detect_chains(
            skeleton.bone_names, skeleton.parent_indices,
        )
        self.categories: Dict[str, BoneCategory] = self.classifier.classify_all(
            skeleton.bone_names, chains=self.chains,
        )
        self.pose = PoseDelta()

        # Optional baseline pose from a reference animation (frame 0 world transforms)
        # If not set, the bind pose is the baseline.
        self._baseline_world: Optional[Dict[str, tuple[np.ndarray, np.ndarray]]] = None

        # Animation playback override — while set, get_world_pose() uses this
        # per-frame pose as the baseline instead of _baseline_world. User pose
        # deltas still compose on top so the viewport shows the *edited*
        # animation. PlaybackController owns the lifetime of this field.
        #
        # Format: bone_name -> (rot_quat_xyzw np.ndarray(4), world_pos np.ndarray(3))
        self.playback_pose: Optional[Dict[str, tuple[np.ndarray, np.ndarray]]] = None

        # Set by PlaybackController. When True:
        #   - _push_undo() is a no-op (prevents playback-time mutations from
        #     flooding the undo stack).
        #   - ViewportInteract.handle_input() early-returns (gizmo / click /
        #     hotkeys all inert).
        self._playback_active: bool = False

        # Persistent pole targets, keyed by mid-bone name. World-space position.
        # Populated lazily on first get_pole_targets() / drag_ik_tip() call.
        self.pole_targets: Dict[str, np.ndarray] = {}
        self._pole_targets_initialized = False

        self._undo_stack: list[PoseDelta] = []
        self._redo_stack: list[PoseDelta] = []
        # When a gizmo drag is in progress, every intermediate mutation
        # skips the undo push — the snapshot taken at begin_drag() is the
        # single entry for the entire drag.
        self._in_drag = False

    # --------------------------------------------------------------
    # Baseline
    # --------------------------------------------------------------

    def set_baseline(
        self,
        world_positions: Dict[str, np.ndarray],
        world_rotations: Dict[str, np.ndarray],
    ) -> None:
        """Set a per-bone world-space baseline (e.g., from animation frame 0).

        world_rotations are 3x3 matrices.
        """
        self._baseline_world = {}
        for name in self.skeleton.bone_names:
            if name in world_positions and name in world_rotations:
                self._baseline_world[name] = (
                    world_rotations[name].copy(),
                    np.asarray(world_positions[name], dtype=np.float64),
                )

    def clear_baseline(self) -> None:
        self._baseline_world = None

    # --------------------------------------------------------------
    # Undo / redo
    # --------------------------------------------------------------

    def can_undo(self) -> bool:
        return bool(self._undo_stack)

    def can_redo(self) -> bool:
        return bool(self._redo_stack)

    def _push_undo(self) -> None:
        if self._in_drag:
            # Drag session started by begin_drag() already took the one
            # snapshot that represents the drag; skip per-frame churn.
            return
        if self._playback_active:
            # Playback is running — editing is gated at the input layer,
            # but defence in depth: never grow the undo stack from
            # anything triggered during playback.
            return
        self._undo_stack.append(self.pose.copy())
        if len(self._undo_stack) > _UNDO_MAX:
            self._undo_stack.pop(0)
        self._redo_stack.clear()

    def begin_drag(self) -> None:
        """Open a drag session — takes one undo snapshot for the whole drag."""
        if self._in_drag:
            return
        self._undo_stack.append(self.pose.copy())
        if len(self._undo_stack) > _UNDO_MAX:
            self._undo_stack.pop(0)
        self._redo_stack.clear()
        self._in_drag = True

    def end_drag(self) -> None:
        """Close a drag session. If the pose is identical to the snapshot
        (a click that didn't actually move anything), discard the entry so
        the undo stack doesn't fill with no-ops.
        """
        if not self._in_drag:
            return
        self._in_drag = False
        if self._undo_stack and self._undo_stack[-1].equals(self.pose):
            self._undo_stack.pop()

    def undo(self) -> None:
        if self._playback_active:
            return
        if not self._undo_stack:
            return
        self._redo_stack.append(self.pose.copy())
        self.pose = self._undo_stack.pop()

    def redo(self) -> None:
        if self._playback_active:
            return
        if not self._redo_stack:
            return
        self._undo_stack.append(self.pose.copy())
        self.pose = self._redo_stack.pop()

    # --------------------------------------------------------------
    # Direct setters
    # --------------------------------------------------------------
    #
    # All mutation entry points guard on `_playback_active`: editing during
    # playback corrupts poses. The guard lives in the model (here) instead of
    # at every UI call site so new UI buttons (bone_panel, viewport_panel
    # overlay, any future "Mirror Pose" / hotkey / menu item) can't bypass it.

    def set_local_rotation(self, bone: str, local_quat: np.ndarray) -> None:
        if self._playback_active:
            return
        self._push_undo()
        self.pose.set_rotation(bone, local_quat)

    def set_local_translation(self, bone: str, local_vec: np.ndarray) -> None:
        if self._playback_active:
            return
        self._push_undo()
        self.pose.set_translation(bone, local_vec)

    def reset_bone(self, bone: str) -> None:
        if self._playback_active:
            return
        if self.pose.get_local_transform(bone) is None:
            return
        self._push_undo()
        self.pose.clear_bone(bone)

    def reset_all(self) -> None:
        if self._playback_active:
            return
        if self.pose.is_empty():
            return
        self._push_undo()
        self.pose = PoseDelta()

    # --------------------------------------------------------------
    # IK operations
    # --------------------------------------------------------------

    def drag_ik_tip(self, tip: str, target_world_pos: np.ndarray) -> None:
        chain = self.chains.get(tip)
        if chain is None:
            _log.debug("No IK chain for %s - falling back to direct translation", tip)
            return
        pole = self.pole_targets.get(chain.mid)  # may be None → solver uses default
        self._solve_and_store(chain, target_world_pos, pole_world_pos=pole)

    def drag_ik_pole(self, mid: str, pole_world_pos: np.ndarray) -> None:
        chain: Optional[IkChain] = None
        for c in self.chains.values():
            if c.mid == mid:
                chain = c
                break
        if chain is None:
            return
        # Persist the new pole target (undo-tracked via _solve_and_store's push).
        self.pole_targets[mid] = np.asarray(pole_world_pos, dtype=np.float64).copy()
        world = self.get_world_pose()
        cur_tip_pos = world[chain.tip][1]
        self._solve_and_store(chain, cur_tip_pos, pole_world_pos=pole_world_pos)

    # --------------------------------------------------------------
    # Pole targets (visible IK swivel handles)
    # --------------------------------------------------------------

    def get_pole_targets(self) -> Dict[str, np.ndarray]:
        """Return mid_bone_name -> world pole position for every IK chain.

        Lazy-initializes defaults on the first call so that a visible
        handle exists for the user to drag without any upfront solve.
        """
        if not self._pole_targets_initialized:
            self._initialize_pole_targets()
        return dict(self.pole_targets)

    def reset_pole_targets(self) -> None:
        """Recompute default pole positions from the current pose."""
        self._pole_targets_initialized = False
        self.pole_targets.clear()
        self._initialize_pole_targets()

    def _initialize_pole_targets(self) -> None:
        world = self.get_world_pose()
        for chain in self.chains.values():
            if chain.mid in self.pole_targets:
                continue
            root_pos = world.get(chain.root, (None, None))[1]
            mid_pos = world.get(chain.mid, (None, None))[1]
            tip_pos = world.get(chain.tip, (None, None))[1]
            if root_pos is None or mid_pos is None or tip_pos is None:
                continue
            self.pole_targets[chain.mid] = self._default_pole_position(
                root_pos, mid_pos, tip_pos,
            )
        self._pole_targets_initialized = True

    @staticmethod
    def _default_pole_position(
        root_pos: np.ndarray,
        mid_pos: np.ndarray,
        tip_pos: np.ndarray,
    ) -> np.ndarray:
        """Place the pole on the far side of the current bend, at a
        distance comparable to the total limb length so it's visible.
        """
        d_vec = tip_pos - root_pos
        d_len = float(np.linalg.norm(d_vec))
        limb_len = float(
            np.linalg.norm(mid_pos - root_pos) + np.linalg.norm(tip_pos - mid_pos)
        )
        offset_dist = max(limb_len * 0.75, 5.0)
        if d_len < 1e-6:
            return mid_pos + np.array([0.0, 0.0, offset_dist])
        forward = d_vec / d_len
        # Component of (mid - root) perpendicular to the root->tip axis
        mr = mid_pos - root_pos
        perp = mr - forward * float(np.dot(mr, forward))
        perp_len = float(np.linalg.norm(perp))
        if perp_len < 1e-4:
            # Straight limb — fall back to world "up" then axis-project off forward
            up = np.array([0.0, 0.0, 1.0])
            if abs(float(np.dot(up, forward))) > 0.99:
                up = np.array([0.0, 1.0, 0.0])
            perp = up - forward * float(np.dot(up, forward))
            perp_len = float(np.linalg.norm(perp)) or 1.0
        bend_dir = perp / perp_len
        return mid_pos + bend_dir * offset_dist

    def _solve_and_store(
        self,
        chain: IkChain,
        target_world_pos: np.ndarray,
        pole_world_pos: Optional[np.ndarray],
    ) -> None:
        if self._playback_active:
            return
        world = self.get_world_pose()
        root_rot_q, root_pos = world[chain.root]
        mid_rot_q, mid_pos = world[chain.mid]
        tip_rot_q, tip_pos = world[chain.tip]

        if pole_world_pos is None:
            pole_world_pos = mid_pos.copy()

        l1 = float(np.linalg.norm(mid_pos - root_pos))
        l2 = float(np.linalg.norm(tip_pos - mid_pos))
        if l1 < 1e-6 or l2 < 1e-6:
            return

        # Per-bone local child direction. The solver used to assume
        # every bone's local +X axis pointed at its IK child. That
        # assumption holds for FO4 human arms/legs and PA legs, but PA
        # UpperArm has a ~1.2° offset between its rest +X and the
        # direction to ForeArm1 — enough to make the solve
        # non-idempotent and cause the arm to slowly swing while the
        # user holds the pole handle stationary. Computing the axis
        # from the current world pose (inverse of the current world
        # rotation applied to the world-space child offset) gives the
        # invariant local direction regardless of rest conventions.
        root_rot_mat = quat_to_matrix(root_rot_q)
        mid_rot_mat = quat_to_matrix(mid_rot_q)
        root_local_child_dir = root_rot_mat.T @ (mid_pos - root_pos) / l1
        mid_local_child_dir = mid_rot_mat.T @ (tip_pos - mid_pos) / l2

        new_root_world, new_mid_local = solve_two_bone_ik(
            root_world_pos=root_pos,
            mid_world_pos=mid_pos,
            tip_world_pos=tip_pos,
            target_world_pos=target_world_pos,
            pole_world_pos=pole_world_pos,
            root_to_mid_length=l1,
            mid_to_tip_length=l2,
            root_world_rot=root_rot_q,
            mid_world_rot=mid_rot_q,
            root_local_child_dir=root_local_child_dir,
            mid_local_child_dir=mid_local_child_dir,
        )

        # Convert root's new world rotation to a parent-local delta.
        # The solver returns new_mid as local-to-new-root (parent-local), so we
        # can compose it directly with any existing delta on the mid bone
        # without another world->local conversion.
        root_parent_world = self._get_parent_world_rot(chain.root, world)

        local_root_delta = world_rot_delta_to_local(
            new_root_world, root_rot_q, root_parent_world,
        )

        # For the mid bone, new_mid_local is the *absolute* new parent-local
        # rotation (relative to the new root). We need a delta that, when
        # composed by _compose_rotation, yields new_mid_local as the bone's
        # local rotation.
        #
        # FRAGILE CANCELLATION — DO NOT REORDER:
        #   _compose_rotation pre-multiplies: composed = delta * existing.
        #   _get_local then pre-multiplies: local = composed * ref.
        #   We pass delta = new_mid_local * old_mid_local^-1, where
        #   old_mid_local = existing * ref (the current local from _get_local).
        #   So: local = (new_mid_local * old_mid_local^-1) * existing * ref
        #             = new_mid_local * (old_mid_local^-1 * old_mid_local)
        #             = new_mid_local. ✓
        # If _compose_rotation ever switches to post-multiply, or if push_undo
        # is reordered such that old_mid_local is read after the compose, this
        # cancellation breaks silently and the IK drag will misbehave.
        old_mid_local_rot, _ = self._get_local(chain.mid, self.skeleton.get_bone_index(chain.mid))
        local_mid_delta = quat_normalize(
            quat_multiply(new_mid_local, quat_conjugate(old_mid_local_rot))
        )

        self._push_undo()
        self._compose_rotation(chain.root, local_root_delta)
        self._compose_rotation(chain.mid, local_mid_delta)

    def _compose_rotation(self, bone: str, delta_q: np.ndarray) -> None:
        existing = self.pose.rotations.get(bone)
        if existing is None:
            self.pose.set_rotation(bone, delta_q)
        else:
            composed = quat_normalize(quat_multiply(delta_q, existing))
            self.pose.set_rotation(bone, composed)

    def _get_parent_world_rot(
        self, bone: str, world: Dict[str, tuple[np.ndarray, np.ndarray]],
    ) -> np.ndarray:
        idx = self.skeleton.get_bone_index(bone)
        if idx is None:
            return np.array([0.0, 0.0, 0.0, 1.0])
        parent_idx = self.skeleton.parent_indices[idx]
        if parent_idx < 0 or parent_idx >= self.skeleton.bone_count:
            return np.array([0.0, 0.0, 0.0, 1.0])
        parent_name = self.skeleton.bone_names[parent_idx]
        if parent_name in world:
            return world[parent_name][0]
        return np.array([0.0, 0.0, 0.0, 1.0])

    # --------------------------------------------------------------
    # World-pose composition (read API for viewport AND apply pipeline)
    # --------------------------------------------------------------

    def get_world_pose(self) -> Dict[str, tuple[np.ndarray, np.ndarray]]:
        """Return bone_name -> (world_rot_quat, world_pos) for every bone."""
        # Animation playback path: the controller has pushed a per-frame
        # world pose. Use it as the baseline so pose deltas still compose
        # on top, then restore. This re-uses the same baseline_world ->
        # local -> *delta pipeline the editor uses when no playback is
        # active, so the composition semantics are identical.
        if self.playback_pose is not None:
            saved = self._baseline_world
            try:
                self._baseline_world = self._playback_pose_as_baseline(
                    self.playback_pose,
                )
                return self._compose_world_pose()
            finally:
                self._baseline_world = saved
        return self._compose_world_pose()

    @staticmethod
    def _playback_pose_as_baseline(
        playback: Dict[str, tuple[np.ndarray, np.ndarray]],
    ) -> Dict[str, tuple[np.ndarray, np.ndarray]]:
        """Convert a playback_pose (quat + pos) into the baseline_world
        format (3x3 rotation matrix + pos) so _baseline_local_for can
        consume it unchanged.
        """
        out: Dict[str, tuple[np.ndarray, np.ndarray]] = {}
        for name, (rot_q, pos) in playback.items():
            out[name] = (quat_to_matrix(rot_q), np.asarray(pos, dtype=np.float64))
        return out

    def _compose_world_pose(self) -> Dict[str, tuple[np.ndarray, np.ndarray]]:
        n = self.skeleton.bone_count
        world_rot_q: list[Optional[np.ndarray]] = [None] * n
        world_pos: list[Optional[np.ndarray]] = [None] * n

        for i in range(n):
            name = self.skeleton.bone_names[i]
            local_rot_q, local_pos = self._get_local(name, i)

            parent_idx = self.skeleton.parent_indices[i]
            if parent_idx < 0 or parent_idx >= n:
                world_rot_q[i] = local_rot_q
                world_pos[i] = local_pos
            else:
                p_rot_q = world_rot_q[parent_idx]
                p_pos = world_pos[parent_idx]
                p_mat = quat_to_matrix(p_rot_q)
                world_rot_q[i] = quat_normalize(quat_multiply(p_rot_q, local_rot_q))
                world_pos[i] = p_mat @ local_pos + p_pos

        return {
            self.skeleton.bone_names[i]: (world_rot_q[i], world_pos[i])
            for i in range(n)
        }

    def _get_local(
        self, name: str, idx: int,
    ) -> tuple[np.ndarray, np.ndarray]:
        """Compute the bone's local-space (rot_quat, translation) by composing
        baseline_local * pose_delta_local.
        """
        local_rot = self.skeleton.ref_rotations[idx].copy()
        local_pos = self.skeleton.ref_translations[idx].copy()

        if self._baseline_world is not None and name in self._baseline_world:
            local_rot, local_pos = self._baseline_local_for(name, idx)

        delta_rot = self.pose.rotations.get(name)
        if delta_rot is not None:
            local_rot = quat_normalize(quat_multiply(delta_rot, local_rot))
        delta_trans = self.pose.translations.get(name)
        if delta_trans is not None:
            local_pos = local_pos + delta_trans

        return local_rot, local_pos

    def _baseline_local_for(
        self, name: str, idx: int,
    ) -> tuple[np.ndarray, np.ndarray]:
        """Convert baseline world transform to local space relative to parent."""
        bone_rot_world_mat, bone_pos_world = self._baseline_world[name]
        bone_rot_world_q = mat_to_quat(bone_rot_world_mat)

        parent_idx = self.skeleton.parent_indices[idx]
        if parent_idx < 0 or parent_idx >= self.skeleton.bone_count:
            return bone_rot_world_q, bone_pos_world.copy()
        parent_name = self.skeleton.bone_names[parent_idx]
        if parent_name not in self._baseline_world:
            return bone_rot_world_q, bone_pos_world.copy()

        parent_rot_world_mat, parent_pos_world = self._baseline_world[parent_name]
        parent_rot_world_q = mat_to_quat(parent_rot_world_mat)

        local_rot_q = quat_normalize(
            quat_multiply(quat_conjugate(parent_rot_world_q), bone_rot_world_q)
        )
        rel = bone_pos_world - parent_pos_world
        local_pos = parent_rot_world_mat.T @ rel
        return local_rot_q, local_pos
