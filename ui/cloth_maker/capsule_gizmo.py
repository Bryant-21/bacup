"""Drag/rotate gizmo for cloth capsule colliders.

Wraps the shared ``GizmoManager`` so the user can translate and rotate
already-placed capsules directly in the viewport instead of tweaking
sliders in the authoring panel. Selection tracks the authoring panel's
``_capsule_bone_idx`` so the combo and gizmo stay in sync.
"""
from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Optional

import glm
import numpy as np
from imgui_bundle import imgui

from creation_lib.renderer.gizmo import GizmoManager, ROTATE, TRANSLATE

if TYPE_CHECKING:
    from .cloth_maker_app import ClothMakerApp
    from .cloth_scene import CapsuleData

_log = logging.getLogger("cloth_maker.capsule_gizmo")

_PICK_RADIUS_PX = 12.0


class _GizmoTarget:
    """Minimal object with ``world_transform`` for ``GizmoManager.draw``."""

    def __init__(self, world_transform: glm.mat4):
        self.world_transform = world_transform


class CapsuleGizmo:
    """Viewport gizmo for translating/rotating capsule colliders."""

    def __init__(self, app: "ClothMakerApp"):
        self.app = app
        self.gizmo = GizmoManager()
        self.gizmo.manipulate_active = False
        self.gizmo.operation = TRANSLATE
        self._was_using = False

    # ------------------------------------------------------------------
    # Selection mirrors the authoring panel's combo index.
    # ------------------------------------------------------------------
    def _selected_idx(self) -> int:
        ap = self.app.authoring_panel
        if ap is None:
            return -1
        idx = ap._capsule_bone_idx
        caps = self.app.scene.capsules
        if 0 <= idx < len(caps):
            return idx
        return -1

    def _set_selected_idx(self, idx: int) -> None:
        ap = self.app.authoring_panel
        if ap is None:
            return
        ap._capsule_bone_idx = idx
        # Force authoring panel to reload slider values from the new selection.
        ap._capsule_tool_active = False

    # ------------------------------------------------------------------
    # World matrix construction / decomposition
    # ------------------------------------------------------------------
    @staticmethod
    def _build_target_matrix(cap: "CapsuleData") -> glm.mat4:
        """Build a world matrix with local X aligned to the capsule axis."""
        start = np.asarray(cap.start, dtype=np.float64)
        end = np.asarray(cap.end, dtype=np.float64)
        mid = 0.5 * (start + end)
        axis = end - start
        length = float(np.linalg.norm(axis))
        if length < 1e-6:
            x = np.array([1.0, 0.0, 0.0])
        else:
            x = axis / length
        up_ref = (
            np.array([0.0, 0.0, 1.0])
            if abs(float(x[2])) < 0.9
            else np.array([0.0, 1.0, 0.0])
        )
        y = np.cross(up_ref, x)
        ny = float(np.linalg.norm(y))
        if ny < 1e-6:
            y = np.array([0.0, 1.0, 0.0])
        else:
            y /= ny
        z = np.cross(x, y)

        m = glm.mat4(1.0)
        m[0][0], m[0][1], m[0][2] = float(x[0]), float(x[1]), float(x[2])
        m[1][0], m[1][1], m[1][2] = float(y[0]), float(y[1]), float(y[2])
        m[2][0], m[2][1], m[2][2] = float(z[0]), float(z[1]), float(z[2])
        m[3][0], m[3][1], m[3][2] = float(mid[0]), float(mid[1]), float(mid[2])
        return m

    def _apply_new_matrix(self, cap_idx: int, new_mat: glm.mat4) -> None:
        """Write a dragged/rotated matrix back to the cloth graph."""
        caps = self.app.scene.capsules
        if not (0 <= cap_idx < len(caps)):
            return
        cap = caps[cap_idx]

        start = np.asarray(cap.start, dtype=np.float64)
        end = np.asarray(cap.end, dtype=np.float64)
        half_len = 0.5 * float(np.linalg.norm(end - start))
        if half_len < 1e-6:
            return

        mid = np.array(
            [float(new_mat[3][0]), float(new_mat[3][1]), float(new_mat[3][2])]
        )
        axis = np.array(
            [float(new_mat[0][0]), float(new_mat[0][1]), float(new_mat[0][2])]
        )
        n = float(np.linalg.norm(axis))
        if n < 1e-6:
            return
        axis /= n

        new_start = mid - axis * half_len
        new_end = mid + axis * half_len

        try:
            local_s, local_e = self.app.scene.world_segment_to_bone_local(
                cap.bone_name, new_start, new_end,
            )
        except Exception as e:
            _log.warning("Gizmo: world→bone-local failed for %r: %s",
                         cap.bone_name, e)
            return

        try:
            from creation_lib._native import havok_native
            scene = self.app.scene
            new_blob = havok_native.cloth_set_capsule_endpoints(
                scene.blob,
                cap_idx,
                local_s.tolist() + [0.0],
                local_e.tolist() + [0.0],
            )
            scene.refresh_from_blob(new_blob)
        except Exception as e:
            _log.error("Gizmo: failed to write capsule endpoints: %s",
                       e, exc_info=True)

    # ------------------------------------------------------------------
    # Per-frame draw + hotkeys
    # ------------------------------------------------------------------
    def draw(
        self,
        camera,
        viewport_pos: imgui.ImVec2,
        viewport_size: imgui.ImVec2,
    ) -> None:
        """Draw the gizmo for the selected capsule (if any)."""
        idx = self._selected_idx()
        if idx < 0 or not self.app.scene.show_capsules:
            self._was_using = False
            return
        if not self.gizmo.manipulate_active:
            return

        cap = self.app.scene.capsules[idx]
        target = _GizmoTarget(self._build_target_matrix(cap))
        new_mat = self.gizmo.draw(camera, viewport_pos, viewport_size, target)

        now_using = self.gizmo.is_using()
        if now_using and not self._was_using:
            self.app.push_undo("Transform capsule")
        self._was_using = now_using

        if new_mat is not None:
            self._apply_new_matrix(idx, new_mat)

    def handle_hotkeys(self) -> bool:
        """Handle W/E/Esc hotkeys while the viewport is hovered/focused.

        Returns True if the gizmo is currently being manipulated (caller
        should suppress click-pick / brush input).
        """
        io = imgui.get_io()
        if not io.want_text_input:
            if imgui.is_key_pressed(imgui.Key.w, repeat=False):
                if self._selected_idx() >= 0:
                    self.gizmo.operation = TRANSLATE
                    self.gizmo.manipulate_active = True
            elif imgui.is_key_pressed(imgui.Key.e, repeat=False):
                if self._selected_idx() >= 0:
                    self.gizmo.operation = ROTATE
                    self.gizmo.manipulate_active = True
            elif imgui.is_key_pressed(imgui.Key.escape, repeat=False):
                self.gizmo.deactivate_manipulate()
        return self.gizmo.is_using()

    def is_using(self) -> bool:
        return self.gizmo.is_using()

    # ------------------------------------------------------------------
    # Click-pick
    # ------------------------------------------------------------------
    def pick(
        self,
        camera,
        viewport_pos: imgui.ImVec2,
        viewport_size: imgui.ImVec2,
        mouse_pos: imgui.ImVec2,
    ) -> bool:
        """Select the capsule whose screen-space segment is closest to the mouse."""
        caps = self.app.scene.capsules
        if not caps or not self.app.scene.show_capsules:
            return False
        aspect = viewport_size.x / max(viewport_size.y, 1)
        vp_mat = camera.get_projection_matrix(aspect) * camera.get_view_matrix()
        mx = mouse_pos.x - viewport_pos.x
        my = mouse_pos.y - viewport_pos.y

        best_idx = -1
        best_dist = _PICK_RADIUS_PX
        for i, cap in enumerate(caps):
            s_ok, sx, sy = _project(cap.start, vp_mat, viewport_size)
            e_ok, ex, ey = _project(cap.end, vp_mat, viewport_size)
            if not (s_ok and e_ok):
                continue
            d = _point_segment_distance_2d(mx, my, sx, sy, ex, ey)
            if d < best_dist:
                best_dist = d
                best_idx = i
        if best_idx < 0:
            return False
        self._set_selected_idx(best_idx)
        # Auto-show gizmo on viewport pick; preserve the current operation
        # (TRANSLATE by default, or whatever the user last chose via W/E).
        self.gizmo.manipulate_active = True
        return True


# ----------------------------------------------------------------------
# Projection helpers
# ----------------------------------------------------------------------

def _project(
    world_pt,
    vp_mat: glm.mat4,
    size: imgui.ImVec2,
) -> tuple[bool, float, float]:
    v = glm.vec4(float(world_pt[0]), float(world_pt[1]), float(world_pt[2]), 1.0)
    clip = vp_mat * v
    if clip.w <= 0.0:
        return False, 0.0, 0.0
    ndc_x = clip.x / clip.w
    ndc_y = clip.y / clip.w
    sx = (ndc_x * 0.5 + 0.5) * size.x
    sy = (1.0 - (ndc_y * 0.5 + 0.5)) * size.y
    return True, sx, sy


def _point_segment_distance_2d(
    px: float, py: float,
    ax: float, ay: float,
    bx: float, by: float,
) -> float:
    abx = bx - ax
    aby = by - ay
    ab_len2 = abx * abx + aby * aby
    if ab_len2 < 1e-6:
        dx = px - ax
        dy = py - ay
        return (dx * dx + dy * dy) ** 0.5
    t = ((px - ax) * abx + (py - ay) * aby) / ab_len2
    t = max(0.0, min(1.0, t))
    cx = ax + t * abx
    cy = ay + t * aby
    dx = px - cx
    dy = py - cy
    return (dx * dx + dy * dy) ** 0.5
