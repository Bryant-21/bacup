"""3D viewport panel for scope aligner."""
from __future__ import annotations

import logging

import glm
import moderngl
import numpy as np
from imgui_bundle import imgui

from ui.aligner.scope_camera import CameraMode
from ui.aligner.crosshair_overlay import draw_crosshair

_log = logging.getLogger("aligner.viewport")


def _open_file_dialog(title: str, filetypes=None) -> str | None:
    """Open a native file dialog."""
    if filetypes is None:
        filetypes = [("All files", "*.*")]
    try:
        from creation_lib.ui.widgets.pick_folder import pick_file
        return pick_file(title, filetypes)
    except Exception:
        return None


class ViewportPanel:
    """Renders the 3D scene and handles camera input."""

    def __init__(self, app):
        self._app = app
        self.window_name = "Viewport##aligner"
        self._show_preset_popup = False
        self._logged_skin = False

        # Preset body panel
        self._ref_body_panel = None
        if app._toolkit_settings:
            from ui.shared.reference_body_panel import ReferenceBodyPanel
            self._ref_body_panel = ReferenceBodyPanel(
                toolkit_settings=app._toolkit_settings,
                on_load=self._on_preset_load,
                games=["fo4"],
            )

    def draw(self):
        flags = (imgui.WindowFlags_.no_scrollbar.value
                 | imgui.WindowFlags_.no_scroll_with_mouse.value)
        imgui.begin(self.window_name, flags=flags)

        viewport_pos = imgui.get_cursor_screen_pos()
        size = imgui.get_content_region_avail()

        renderer = self._app.renderer
        camera = self._app.camera
        vp_matrix = None

        if size.x > 0 and size.y > 0 and renderer:
            renderer.ensure_fbo(size.x, size.y)
            renderer.render(camera, self._app.lighting)

            # Render skinned body mesh into FBO (behind weapon)
            self._render_skinned_meshes(renderer, camera, size)

            # Render skeleton overlay into FBO (before reading texture)
            skel = self._app.skeleton_display
            if skel.visible and skel._num_vertices > 0 and renderer.fbo:
                renderer.fbo.use()
                cp_prog = renderer.programs.get("connect_point")
                if cp_prog:
                    aspect = size.x / max(size.y, 1)
                    view = camera.get_view_matrix()
                    proj = camera.get_projection_matrix(aspect)
                    vp_matrix = proj * view
                    vp_tuple = tuple(c for col in vp_matrix for c in col)
                    self._app.ctx.disable(moderngl.DEPTH_TEST)
                    skel.render(cp_prog, vp_tuple)
                    self._app.ctx.enable(moderngl.DEPTH_TEST)
                self._app.ctx.screen.use()  # restore screen FBO for imgui

            tex_id = renderer.get_fbo_texture_id()
            if tex_id:
                imgui.image(
                    imgui.ImTextureRef(tex_id), size,
                    uv0=imgui.ImVec2(0, 1), uv1=imgui.ImVec2(1, 0),
                )

            # Skeleton labels (imgui overlay)
            if skel.visible and vp_matrix is not None:
                skel.draw_labels(vp_matrix, viewport_pos, size)

            if camera.mode == CameraMode.SCOPE_VIEW:
                draw_crosshair(viewport_pos, size)

            # Camera input (orbit mode only)
            if imgui.is_window_hovered():
                camera.handle_input(imgui.get_io())

        self._draw_preset_popup()

        imgui.set_cursor_pos_y(
            imgui.get_window_height()
            - imgui.get_text_line_height_with_spacing() - 4
        )
        imgui.text_colored(
            imgui.ImVec4(0.8, 0.8, 0.8, 1.0),
            self._app.status_text,
        )

        imgui.end()

    def _render_skinned_meshes(self, renderer, camera, size):
        """Render skinned body meshes with current animation pose."""
        skinned_r = self._app.skinned_renderer
        if skinned_r is None or not self._app.skinned_meshes:
            return
        if self._app.skeleton is None or renderer.fbo is None:
            return

        renderer.fbo.use()

        ctx = self._app.ctx
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

        for mesh in self._app.skinned_meshes:
            try:
                bone_matrices = skinned_r.compute_bone_matrices(
                    self._app.skeleton, mesh, deltas=None,
                    anim_positions=self._app._world_positions,
                    anim_rotations=self._app._world_rotations,
                )
                if not self._logged_skin:
                    _log.info("Rendering skinned body: %d bones, %d indices",
                              len(mesh.bone_names), mesh.index_count)
                skinned_r.render(
                    mesh, mvp_tuple, model_tuple, normal_tuple,
                    bone_matrices=bone_matrices,
                )
            except Exception:
                _log.exception("Failed to render skinned mesh")

        self._logged_skin = True
        ctx.enable(moderngl.CULL_FACE)
        ctx.screen.use()

    def _draw_preset_popup(self):
        """Show reference body preset popup (triggered by toolbar button)."""
        if self._show_preset_popup:
            self._show_preset_popup = False
            if self._ref_body_panel is not None:
                imgui.open_popup("Preset Body##aligner_popup")

        if self._ref_body_panel is not None:
            if imgui.begin_popup("Preset Body##aligner_popup"):
                self._ref_body_panel.draw()
                imgui.spacing()
                imgui.separator()
                imgui.spacing()
                if imgui.button("Browse Custom NIF...", imgui.ImVec2(-1, 0)):
                    path = _open_file_dialog(
                        "Select Body Mesh NIF",
                        filetypes=[("NIF files", "*.nif"), ("All files", "*.*")],
                    )
                    if path:
                        imgui.close_current_popup()
                imgui.end_popup()

    def _on_preset_load(self, skeleton_hkx: str, skeleton_nif: str | None,
                        body_nif_paths: list[str], game: str):
        """Handle preset body load from ReferenceBodyPanel."""
        try:
            self._app.load_composite_body(skeleton_hkx, skeleton_nif,
                                          body_nif_paths, game)
            self._logged_skin = False  # Reset logging for new mesh
        except Exception as e:
            _log.exception("Failed to load preset body")
            self._app.status_text = f"Error: {e}"
