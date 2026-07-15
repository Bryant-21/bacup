"""Viewport panel — ImGui window that hosts render + interact + camera controls.

Thin glue: per-frame rebuild SkeletonDisplay from PoseSession's world pose,
draw the FBO, draw the skeleton overlay, hand input to ViewportInteract.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

import glm
import moderngl
from imgui_bundle import imgui

from creation_lib.renderer.gizmo import ROTATE, TRANSLATE
from ui.aligner.crosshair_overlay import draw_crosshair
from ui.aligner.scope_camera import CameraMode

from .viewport_render import SkeletonDisplay

if TYPE_CHECKING:
    from .bone_editor_app import BoneEditorApp

_log = logging.getLogger("bone_editor.viewport_panel")


class ViewportPanel:
    def __init__(self, app: "BoneEditorApp"):
        self.app = app
        self.skeleton_display = SkeletonDisplay()
        self._last_pose_revision = -1  # rebuild trigger

    def draw(self) -> None:
        flags = (imgui.WindowFlags_.no_scrollbar.value
                 | imgui.WindowFlags_.no_scroll_with_mouse.value)
        visible, _ = imgui.begin("Viewport##bone_editor", flags=flags)
        if not visible:
            imgui.end()
            return

        viewport_pos = imgui.get_cursor_screen_pos()
        size = imgui.get_content_region_avail()

        renderer = self.app.renderer
        camera = self.app.camera
        sess = self.app.pose_session

        # Keep first-person camera anchored to the live Camera-bone position
        # so spine/neck/head edits update the scope view in real time.
        if camera.mode == CameraMode.SCOPE_VIEW and sess is not None:
            self.app._update_camera_bone_pos()

        # Weapon NIF (if loaded) follows the Weapon bone every frame so it
        # tracks pose edits.
        weapon_session = self.app.registry.sessions.get("weapon")
        if weapon_session is not None and sess is not None:
            self.app._apply_weapon_bone_transform(weapon_session.scene_root)

        if size.x > 0 and size.y > 0 and renderer is not None:
            renderer.ensure_fbo(size.x, size.y)
            renderer.render(camera, self.app.lighting)

            # Skinned mesh
            self._render_skinned(renderer, camera, size, sess)

            # Skeleton overlay
            vp_matrix = None
            if sess is not None and self.skeleton_display.visible:
                cp_prog = renderer.programs.get("connect_point")
                if cp_prog is not None and self.app.ctx is not None:
                    world_pose = sess.get_world_pose()
                    sel = (self.app.viewport_interact.selected_bone
                           if self.app.viewport_interact else None)
                    bone_panel = self.app.bone_panel
                    extra_hidden = (
                        bone_panel.compute_extra_hidden() if bone_panel else set()
                    )
                    show_poles = bone_panel.show_pole_handles if bone_panel else True
                    self.skeleton_display.rebuild(
                        skeleton=self.app.skeleton,
                        world_pose=world_pose,
                        categories=sess.categories,
                        edited_bones=sess.pose.edited_bones(),
                        selected_bone=sel,
                        hidden_bones=self.app.classifier_hidden | extra_hidden,
                        ctx=self.app.ctx,
                        program=cp_prog,
                        pole_targets=sess.get_pole_targets() if show_poles else None,
                    )
                    if renderer.fbo is not None:
                        renderer.fbo.use()
                        aspect = size.x / max(size.y, 1)
                        view = camera.get_view_matrix()
                        proj = camera.get_projection_matrix(aspect)
                        vp_matrix = proj * view
                        vp_tuple = tuple(c for col in vp_matrix for c in col)
                        self.app.ctx.disable(moderngl.DEPTH_TEST)
                        self.skeleton_display.render(cp_prog, vp_tuple)
                        self.app.ctx.enable(moderngl.DEPTH_TEST)
                        self.app.ctx.screen.use()

            # FBO image
            tex_id = renderer.get_fbo_texture_id()
            if tex_id:
                imgui.image(
                    imgui.ImTextureRef(tex_id), size,
                    uv0=imgui.ImVec2(0, 1), uv1=imgui.ImVec2(1, 0),
                )

            # Crosshair overlay in 1st-person/scope view
            if camera.mode == CameraMode.SCOPE_VIEW:
                draw_crosshair(viewport_pos, size)

            # Labels + interaction
            if vp_matrix is not None and self.app.viewport_interact is not None:
                projected = self.skeleton_display.project_labels(
                    vp_matrix, viewport_pos, size,
                )
                self.app.viewport_interact.update_projected_labels(projected)
                self.skeleton_display.draw_labels(projected, visible_names=None)
                self.app.viewport_interact.handle_input(camera, viewport_pos, size)

            # Camera input (skip while gizmo is being dragged)
            if (imgui.is_window_hovered()
                    and self.app.viewport_interact is not None
                    and not self.app.viewport_interact.gizmo.is_using()):
                camera.handle_input(imgui.get_io())

            # Toolbar
            self._draw_toolbar(camera, viewport_pos)

        # Status bar at bottom
        imgui.set_cursor_pos_y(
            imgui.get_window_height()
            - imgui.get_text_line_height_with_spacing() - 4
        )
        imgui.text_colored(imgui.ImVec4(0.8, 0.8, 0.8, 1.0), self.app.status_text)
        imgui.end()

    def _render_skinned(self, renderer, camera, size, sess) -> None:
        if not self.app.skinned_meshes or self.app.skinned_renderer is None:
            return
        if sess is None or self.app.skeleton is None or renderer.fbo is None:
            return
        skinned_r = self.app.skinned_renderer

        # Convert PoseSession's world pose to the format compute_bone_matrices
        # expects: anim_positions = {name: (x,y,z)}, anim_rotations = {name: 3x3 matrix}
        from creation_lib.bone_edit.quat_util import quat_to_matrix
        world_pose = sess.get_world_pose()
        anim_positions: dict = {}
        anim_rotations: dict = {}
        for name, (rot_q, pos) in world_pose.items():
            anim_positions[name] = (float(pos[0]), float(pos[1]), float(pos[2]))
            anim_rotations[name] = quat_to_matrix(rot_q)

        # One-shot diagnostic: confirm the edited bones that we expect the
        # skinned mesh to rotate are actually present in the mesh's bone list
        # (case-sensitive lookup in compute_bone_matrices). Case/name mismatch
        # causes a silent fallback to parent rotation.
        if not getattr(self, "_logged_bone_match", False) and sess.pose.rotations:
            edited = list(sess.pose.rotations.keys())
            for mesh in self.app.skinned_meshes:
                present = [b for b in edited if b in mesh.bone_names]
                missing = [b for b in edited if b not in mesh.bone_names]
                _log.info(
                    "skin bone match: edited=%s present_in_mesh=%s missing_in_mesh=%s",
                    edited, present, missing,
                )
                if missing:
                    # Show mesh bones that could plausibly be the same (case-insensitive)
                    low = {b.lower(): b for b in mesh.bone_names}
                    for m in missing:
                        hit = low.get(m.lower())
                        if hit and hit != m:
                            _log.info(
                                "skin bone casing mismatch: pose=%r mesh=%r",
                                m, hit,
                            )
            self._logged_bone_match = True

        # Per-selection diagnostic: log the world rotation of the currently
        # selected bone so we can confirm it updates frame-to-frame during drag.
        sel = (self.app.viewport_interact.selected_bone
               if self.app.viewport_interact else None)
        if sel and sel in world_pose:
            rot_q, _ = world_pose[sel]
            cur_log = (float(rot_q[0]), float(rot_q[1]),
                       float(rot_q[2]), float(rot_q[3]))
            if cur_log != getattr(self, "_last_sel_rot", None):
                _log.info("render head rot: %s=%s", sel, cur_log)
                self._last_sel_rot = cur_log

        renderer.fbo.use()
        ctx = self.app.ctx
        ctx.enable(moderngl.DEPTH_TEST)
        ctx.disable(moderngl.CULL_FACE)

        aspect = size.x / max(size.y, 1)
        view = camera.get_view_matrix()
        proj = camera.get_projection_matrix(aspect)
        model = glm.mat4(1.0)
        mvp = proj * view * model
        mvp_tuple = tuple(c for col in mvp for c in col)
        model_tuple = tuple(c for col in model for c in col)
        normal_mat = glm.mat3(glm.transpose(glm.inverse(model)))
        normal_tuple = tuple(c for col in normal_mat for c in col)

        for mesh in self.app.skinned_meshes:
            try:
                bone_matrices = skinned_r.compute_bone_matrices(
                    self.app.skeleton, mesh,
                    deltas=None,  # PoseSession's world_pose already includes deltas
                    anim_positions=anim_positions,
                    anim_rotations=anim_rotations,
                )
                skinned_r.render(
                    mesh, mvp_tuple, model_tuple, normal_tuple,
                    bone_matrices=bone_matrices,
                )
            except Exception:
                _log.exception("Failed to render skinned mesh")

        ctx.enable(moderngl.CULL_FACE)
        ctx.screen.use()

    def _draw_toolbar(self, camera, viewport_pos) -> None:
        btn_y = viewport_pos.y + 4
        btn_x = viewport_pos.x + 4
        imgui.set_cursor_screen_pos(imgui.ImVec2(btn_x, btn_y))

        orbit_active = camera.mode == CameraMode.ORBIT
        fp_active = camera.mode == CameraMode.SCOPE_VIEW
        if orbit_active:
            imgui.push_style_color(imgui.Col_.button, imgui.ImVec4(0.3, 0.5, 0.8, 1.0))
        if imgui.small_button("Orbit"):
            camera.mode = CameraMode.ORBIT
        if orbit_active:
            imgui.pop_style_color()

        imgui.same_line()
        # 1st Person needs the Camera bone position; without it scope view
        # collapses to the origin. Disable until the bone is available.
        has_camera_bone = False
        sess = self.app.pose_session
        if sess is not None:
            has_camera_bone = "Camera" in sess.get_world_pose()
        if not has_camera_bone:
            imgui.begin_disabled()
        if fp_active:
            imgui.push_style_color(imgui.Col_.button, imgui.ImVec4(0.3, 0.5, 0.8, 1.0))
        if imgui.small_button("1st Person"):
            camera.mode = CameraMode.SCOPE_VIEW
            self.app._update_camera_bone_pos()
        if fp_active:
            imgui.pop_style_color()
        if not has_camera_bone:
            imgui.end_disabled()

        imgui.same_line()
        imgui.text(" | ")

        sess = self.app.pose_session
        vi = self.app.viewport_interact
        if vi is not None:
            imgui.same_line()
            if not sess.can_undo():
                imgui.begin_disabled()
            if imgui.small_button("Undo"):
                sess.undo()
            if not sess.can_undo():
                imgui.end_disabled()

            imgui.same_line()
            if not sess.can_redo():
                imgui.begin_disabled()
            if imgui.small_button("Redo"):
                sess.redo()
            if not sess.can_redo():
                imgui.end_disabled()

            imgui.same_line()
            imgui.text(" | ")
            imgui.same_line()
            if imgui.small_button("Reset All"):
                sess.reset_all()

            imgui.same_line()
            imgui.text(" | ")
            imgui.same_line()
            is_trans = vi.gizmo.manipulate_active and vi.gizmo.operation == TRANSLATE
            if is_trans:
                imgui.push_style_color(
                    imgui.Col_.button, imgui.ImVec4(0.3, 0.5, 0.8, 1.0))
            if imgui.small_button("W Move"):
                vi._set_translate_if_allowed()
            if is_trans:
                imgui.pop_style_color()

            imgui.same_line()
            is_rot = vi.gizmo.manipulate_active and vi.gizmo.operation == ROTATE
            if is_rot:
                imgui.push_style_color(
                    imgui.Col_.button, imgui.ImVec4(0.3, 0.5, 0.8, 1.0))
            if imgui.small_button("E Rotate"):
                vi.gizmo.set_operation(ROTATE)
            if is_rot:
                imgui.pop_style_color()

        setup = self.app.setup_panel
        if setup is not None and setup._ref_body_panel is not None:
            imgui.same_line()
            imgui.text(" | ")
            imgui.same_line()
            if imgui.small_button("Load Body..."):
                setup._show_preset_popup = True
