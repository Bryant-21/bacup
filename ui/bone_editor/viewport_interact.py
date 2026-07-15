"""Interaction half of the bone editor viewport.

Owns: click-to-select, gizmo dragging (translate / rotate / IK / pole),
hotkeys (W/E/G/Esc/Ctrl+Z/Ctrl+Y). Calls pose_session for all mutations
and viewport_render for all drawing.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Optional

import glm
import numpy as np
from imgui_bundle import imgui

from creation_lib.bone_edit.bone_classifier import BoneCategory
from creation_lib.renderer.gizmo import GizmoManager, ROTATE, TRANSLATE

from .viewport_render import POLE_HANDLE_PREFIX, _GizmoTarget

if TYPE_CHECKING:
    from .pose_session import PoseSession

_log = logging.getLogger("bone_editor.viewport_interact")

_PICK_RADIUS = 20.0  # screen pixels


class ViewportInteract:
    def __init__(self, pose_session: "PoseSession"):
        self.pose_session = pose_session
        self.gizmo = GizmoManager()
        self.gizmo.manipulate_active = False
        self.selected_bone: Optional[str] = None
        self._gizmo_was_using = False
        self._projected_labels: list = []

    def update_projected_labels(self, projected) -> None:
        self._projected_labels = projected

    def handle_input(
        self,
        camera,
        viewport_pos: imgui.ImVec2,
        viewport_size: imgui.ImVec2,
    ) -> None:
        """Process gizmo, click-pick, and hotkeys for this frame."""
        # Animation playback gate: while the PlaybackController is active,
        # the viewport is strictly read-only — gizmo is hidden, click-pick
        # is suppressed, and all hotkeys (W/E/G/Ctrl+Z/Ctrl+Y/Esc) are
        # inert. Selection state is preserved so it's back exactly as the
        # user left it when playback stops.
        if getattr(self.pose_session, "_playback_active", False):
            return
        # Click-to-select
        if (imgui.is_window_hovered()
                and imgui.is_mouse_clicked(imgui.MouseButton_.left)
                and not self.gizmo.is_using()):
            self._pick_bone(imgui.get_mouse_pos())

        # Gizmo
        if self.gizmo.manipulate_active and self.selected_bone is not None:
            bone_mat = self._get_selected_bone_matrix()
            if bone_mat is None:
                if not getattr(self, "_logged_no_mat", False):
                    _log.warning(
                        "gizmo: no matrix for selected bone %r "
                        "(world_pose has %d entries, in_pose=%s)",
                        self.selected_bone,
                        len(self.pose_session.get_world_pose()),
                        self.selected_bone in self.pose_session.get_world_pose(),
                    )
                    self._logged_no_mat = True
            else:
                self._logged_no_mat = False
                if not getattr(self, "_logged_mat", False):
                    _log.info(
                        "gizmo draw: bone=%r pos=(%.2f,%.2f,%.2f) op=%s active=%s",
                        self.selected_bone,
                        float(bone_mat[3][0]), float(bone_mat[3][1]), float(bone_mat[3][2]),
                        self.gizmo.operation, self.gizmo.manipulate_active,
                    )
                    self._logged_mat = True
                target = _GizmoTarget(bone_mat)
                new_mat = self.gizmo.draw(camera, viewport_pos, viewport_size, target)
                if new_mat is not None:
                    self._apply_gizmo_result(bone_mat, new_mat)

        # Drag edge detection — MUST run every frame regardless of whether
        # the gizmo produced a new_mat this frame. ImGuizmo can report
        # is_using()=True for one or more frames before the user has
        # moved the handle enough to produce a new matrix. If the rising
        # edge is gated on `new_mat is not None`, `_gizmo_was_using` gets
        # flipped True by the unconditional assignment below before
        # `begin_drag()` ever fires — so `_in_drag` never engages and
        # every subsequent `set_local_rotation` pushes its own undo
        # entry (one per frame instead of one per drag).
        self._update_drag_state(self.gizmo.is_using())

        # Hotkeys
        if imgui.is_window_hovered() or imgui.is_window_focused():
            io = imgui.get_io()
            ctrl = io.key_ctrl
            if ctrl and imgui.is_key_pressed(imgui.Key.z, repeat=False):
                self.pose_session.undo()
            elif ctrl and imgui.is_key_pressed(imgui.Key.y, repeat=False):
                self.pose_session.redo()
            elif imgui.is_key_pressed(imgui.Key.w, repeat=False):
                self._set_translate_if_allowed()
            elif imgui.is_key_pressed(imgui.Key.e, repeat=False):
                self.gizmo.set_operation(ROTATE)
            elif imgui.is_key_pressed(imgui.Key.g, repeat=False):
                if self.gizmo.manipulate_active:
                    self.gizmo.deactivate_manipulate()
                else:
                    self.gizmo.set_operation(self.gizmo.operation)
            elif imgui.is_key_pressed(imgui.Key.escape, repeat=False):
                self.gizmo.deactivate_manipulate()

    def _update_drag_state(self, now_using: bool) -> None:
        """Rising/falling edge on gizmo.is_using() → begin_drag / end_drag.

        Extracted from handle_input() so the edge detection is (a) testable
        without an imgui context and (b) guaranteed to run every frame,
        independent of whether the gizmo produced a new matrix.
        """
        if now_using and not self._gizmo_was_using:
            # Drag just started — take one undo snapshot for the whole drag.
            self.pose_session.begin_drag()
            _log.info(
                "drag start: bone=%r op=%s",
                self.selected_bone, self.gizmo.operation,
            )
        elif self._gizmo_was_using and not now_using:
            # Drag just ended — close the session; no-op drags are popped
            # inside PoseSession.end_drag().
            self.pose_session.end_drag()
        self._gizmo_was_using = now_using

    def select_bone(self, bone: Optional[str]) -> None:
        # Reset one-shot debug flags on every new selection so we get
        # per-selection diagnostics rather than only the first-ever one.
        self._logged_mat = False
        self._logged_no_mat = False
        self._logged_rotate = False
        self.selected_bone = bone
        if bone is None:
            self.gizmo.deactivate_manipulate()
            return
        if bone.startswith(POLE_HANDLE_PREFIX):
            # Pole handles only make sense with the translate gizmo
            self.gizmo.set_operation(TRANSLATE)
            _log.info("selected pole handle %r (translate)", bone)
            return
        # Auto-activate the most useful gizmo for this bone category:
        #   LIMB_SEGMENT (head, neck, spine, upper arm, thigh, …) → rotate
        #   IK_TIP / IK_POLE / MOUNT → translate
        cat = self.pose_session.categories.get(bone, BoneCategory.LIMB_SEGMENT)
        if cat == BoneCategory.LIMB_SEGMENT:
            self.gizmo.set_operation(ROTATE)
        else:
            self.gizmo.set_operation(TRANSLATE)
        _log.info(
            "selected bone %r cat=%s op=%s active=%s",
            bone, cat.value, self.gizmo.operation, self.gizmo.manipulate_active,
        )

    @staticmethod
    def _pole_mid_name(selected: str) -> Optional[str]:
        if selected and selected.startswith(POLE_HANDLE_PREFIX):
            return selected[len(POLE_HANDLE_PREFIX):]
        return None

    def _pick_bone(self, mouse_pos: imgui.ImVec2) -> None:
        nearest_name = None
        nearest_dist = _PICK_RADIUS
        for name, sx, sy, _color in self._projected_labels:
            dx = sx - mouse_pos.x
            dy = sy - mouse_pos.y
            d = (dx * dx + dy * dy) ** 0.5
            if d < nearest_dist:
                nearest_dist = d
                nearest_name = name
        if nearest_name is not None:
            self.select_bone(nearest_name)

    def _get_selected_bone_matrix(self) -> Optional[glm.mat4]:
        if self.selected_bone is None:
            return None
        mid = self._pole_mid_name(self.selected_bone)
        if mid is not None:
            pos = self.pose_session.pole_targets.get(mid)
            if pos is None:
                return None
            m = glm.mat4(1.0)
            m[3][0] = float(pos[0])
            m[3][1] = float(pos[1])
            m[3][2] = float(pos[2])
            return m
        # IK_POLE bones (e.g. *_ForeArm1, *_Calf): the gizmo edits the
        # pole *target*, not the bone itself. Snap to the pole position so
        # dragging translates the pole — without this, the pole snaps to
        # the bone on the first frame of any drag.
        cat = self.pose_session.categories.get(self.selected_bone)
        if cat == BoneCategory.IK_POLE:
            poles = self.pose_session.get_pole_targets()
            pos = poles.get(self.selected_bone)
            if pos is not None:
                m = glm.mat4(1.0)
                m[3][0] = float(pos[0])
                m[3][1] = float(pos[1])
                m[3][2] = float(pos[2])
                return m
        world = self.pose_session.get_world_pose()
        entry = world.get(self.selected_bone)
        if entry is None:
            return None
        rot_q, pos = entry
        from creation_lib.bone_edit.quat_util import quat_to_matrix
        rot_mat = quat_to_matrix(rot_q)
        m = glm.mat4(1.0)
        for r in range(3):
            for c in range(3):
                m[c][r] = float(rot_mat[r, c])
        m[3][0] = float(pos[0])
        m[3][1] = float(pos[1])
        m[3][2] = float(pos[2])
        return m

    def _apply_gizmo_result(self, old_mat: glm.mat4, new_mat: glm.mat4) -> None:
        bone = self.selected_bone
        if bone is None:
            return

        # Pole handles: always treated as translate-only IK pole drag.
        mid = self._pole_mid_name(bone)
        if mid is not None:
            new_pos = np.array([new_mat[3][0], new_mat[3][1], new_mat[3][2]])
            self.pose_session.drag_ik_pole(mid, new_pos)
            return

        cat = self.pose_session.categories.get(bone, BoneCategory.LIMB_SEGMENT)
        op = self.gizmo.operation

        if op == TRANSLATE:
            new_pos = np.array([new_mat[3][0], new_mat[3][1], new_mat[3][2]])
            if cat == BoneCategory.IK_TIP:
                self.pose_session.drag_ik_tip(bone, new_pos)
            elif cat == BoneCategory.IK_POLE:
                self.pose_session.drag_ik_pole(bone, new_pos)
            elif cat == BoneCategory.MOUNT:
                # Direct local translation: convert world delta to parent-local
                old_pos = np.array([old_mat[3][0], old_mat[3][1], old_mat[3][2]])
                world_delta = new_pos - old_pos
                local_delta = self._world_to_parent_local(bone, world_delta)
                # Compose with existing translation
                existing = self.pose_session.pose.translations.get(
                    bone, np.zeros(3),
                )
                self.pose_session.set_local_translation(bone, existing + local_delta)
            # LIMB_SEGMENT: translate gizmo should not be active; ignore
        elif op == ROTATE:
            # Extract rotation from new_mat, convert to parent-local delta
            from creation_lib.bone_edit.ik_solver import world_rot_delta_to_local
            from creation_lib.bone_edit.quat_util import (
                quat_multiply,
                quat_normalize,
            )

            old_rot_q = self._extract_quat(old_mat)
            new_rot_q = self._extract_quat(new_mat)
            world = self.pose_session.get_world_pose()
            parent_rot_q = self.pose_session._get_parent_world_rot(bone, world)
            local_delta = world_rot_delta_to_local(new_rot_q, old_rot_q, parent_rot_q)

            # Same fragile pre-multiply cancellation as PoseSession._solve_and_store
            # (mid bone) — see PoseSession._solve_and_store for the full algebra. If
            # set_local_rotation ever stops storing the delta as the rotation
            # used by _compose_rotation's pre-multiply, this branch breaks
            # silently. Keep these two paths in sync.
            existing = self.pose_session.pose.rotations.get(bone)
            if existing is None:
                self.pose_session.set_local_rotation(bone, local_delta)
            else:
                composed = quat_normalize(quat_multiply(local_delta, existing))
                self.pose_session.set_local_rotation(bone, composed)
            if not getattr(self, "_logged_rotate", False):
                _log.info(
                    "rotate apply: bone=%r delta=%s old=%s new=%s",
                    bone, local_delta, old_rot_q, new_rot_q,
                )
                self._logged_rotate = True

    def _world_to_parent_local(
        self, bone: str, world_vec: np.ndarray,
    ) -> np.ndarray:
        from creation_lib.bone_edit.quat_util import quat_to_matrix

        world = self.pose_session.get_world_pose()
        parent_rot = self.pose_session._get_parent_world_rot(bone, world)
        return quat_to_matrix(parent_rot).T @ world_vec

    @staticmethod
    def _extract_quat(m: glm.mat4) -> np.ndarray:
        from creation_lib.bone_edit.quat_util import mat_to_quat

        mat = np.array([
            [float(m[0][0]), float(m[1][0]), float(m[2][0])],
            [float(m[0][1]), float(m[1][1]), float(m[2][1])],
            [float(m[0][2]), float(m[1][2]), float(m[2][2])],
        ])
        return mat_to_quat(mat)

    def _set_translate_if_allowed(self) -> None:
        bone = self.selected_bone
        if bone is None:
            return
        if bone.startswith(POLE_HANDLE_PREFIX):
            self.gizmo.set_operation(TRANSLATE)
            return
        cat = self.pose_session.categories.get(bone, BoneCategory.LIMB_SEGMENT)
        if cat == BoneCategory.LIMB_SEGMENT:
            return  # No translate gizmo for limb segments
        self.gizmo.set_operation(TRANSLATE)
