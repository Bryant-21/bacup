"""Viewport panel — 3D viewport with weight heatmap overlay and brush interaction."""
from __future__ import annotations

import logging
from typing import TYPE_CHECKING

import glm
import moderngl
import numpy as np
from imgui_bundle import imgui

if TYPE_CHECKING:
    from ..weight_painter_app import WeightPainterApp

_log = logging.getLogger("weight_painter.viewport")


def _open_file_dialog(title: str, filetypes=None) -> str | None:
    """Open a native file dialog."""
    if filetypes is None:
        filetypes = [("All files", "*.*")]
    try:
        from creation_lib.ui.widgets.pick_folder import pick_file
        return pick_file(title, filetypes)
    except Exception:
        _log.warning("File dialog not available")
        return None


def _save_file_dialog(title: str, filetypes=None, default_ext: str = "") -> str | None:
    """Open a native save-file dialog."""
    if filetypes is None:
        filetypes = [("All files", "*.*")]
    try:
        from creation_lib.ui.widgets.pick_folder import pick_save_file
        return pick_save_file(title, filetypes, default_ext=default_ext)
    except Exception:
        _log.warning("File dialog not available")
        return None


class ViewportPanel:
    """3D viewport for weight painting with mesh preview and brush input."""

    def __init__(self, app: WeightPainterApp):
        self.app = app
        self._hover_vertex: int = -1
        self._painting = False  # True while mouse is held down painting
        self._last_brush_center: np.ndarray | None = None
        self._cursor_hit_point: np.ndarray | None = None
        self._cursor_normal: np.ndarray | None = None
        # Shift+click smooth: store previous brush type while shift is held
        self._shift_held = False
        self._pre_shift_brush: str | None = None

        # Preset reference body panel
        self._ref_body_panel = None
        if app.toolkit_settings:
            from ui.shared.reference_body_panel import ReferenceBodyPanel
            self._ref_body_panel = ReferenceBodyPanel(
                toolkit_settings=app.toolkit_settings,
                on_load=self._on_preset_load,
            )

    def draw(self):
        flags = (imgui.WindowFlags_.no_scrollbar.value
                 | imgui.WindowFlags_.no_scroll_with_mouse.value)
        visible, _ = imgui.begin("Viewport##weight_painter", flags=flags)
        if not visible:
            imgui.end()
            return

        viewport_pos = imgui.get_cursor_screen_pos()
        size = imgui.get_content_region_avail()

        renderer = self.app.renderer
        camera = self.app.camera

        if size.x <= 0 or size.y <= 0 or not renderer:
            imgui.end()
            return

        renderer.ensure_fbo(int(size.x), int(size.y))

        # Render scene (grid, background)
        renderer.render(camera, self.app.lighting)

        # Render skinned mesh with weight overlay into FBO
        has_mesh = self.app.skinned_mesh or self.app.reference_skinned_mesh
        if has_mesh and renderer.fbo:
            self._render_skinned_mesh(renderer, camera, size)

        # Render wireframe overlay on top of mesh
        if self.app.show_wireframe and self.app._wireframe_vao and renderer.fbo:
            self._render_wireframe(renderer, camera, size)

        # Render segment boundary edges on top (visible in all display modes)
        if (self.app.show_segment_edges
                and self.app._segment_edge_vao
                and renderer.fbo):
            self._render_segment_edges(renderer, camera, size)

        # Render gradient preview line (start → cursor)
        if (self.app.brush_type == "gradient"
                and self.app.gradient_pending
                and self.app.gradient_start is not None
                and self._cursor_hit_point is not None
                and renderer.fbo):
            self._render_gradient_line(renderer, camera, size)

        # Render brush cursor ring on mesh surface
        if self._cursor_hit_point is not None and renderer.fbo:
            self._render_brush_cursor(renderer, camera, size)

        # Display FBO texture
        tex_id = renderer.get_fbo_texture_id()
        if tex_id:
            imgui.image(
                imgui.ImTextureRef(tex_id), size,
                uv0=imgui.ImVec2(0, 1), uv1=imgui.ImVec2(1, 0),
            )

        # Handle input (camera + brush)
        if imgui.is_item_hovered():
            io = imgui.get_io()

            # Alt or middle-mouse or right-mouse = camera orbit/pan
            if io.key_alt or io.mouse_down[2] or io.mouse_down[1]:
                camera.handle_input(io)
                # Hide brush cursor during camera manipulation
                self._cursor_hit_point = None
                self._cursor_normal = None
            elif io.mouse_down[0] and self.app.skin_data and (
                self.app.selected_bone_idx >= 0
                or self.app.brush_type in ("segment", "mask", "unmask")
            ):
                # Left click = paint brush (segment/mask brushes don't need bone selection)
                self._handle_brush(io, viewport_pos, size)
            else:
                # Track hover vertex
                self._update_hover(io, viewport_pos, size)
                if not io.mouse_down[0] and self._painting:
                    self._painting = False

            # Mouse wheel = zoom
            if io.mouse_wheel != 0:
                camera.handle_input(io)

        # Keyboard shortcuts (when viewport window is focused or hovered)
        if imgui.is_window_focused() or imgui.is_item_hovered():
            self._handle_keyboard_shortcuts()

        # Draw weight info overlay
        if self._hover_vertex >= 0 and self.app.skin_data:
            self._draw_vertex_info(viewport_pos, size)

        # Import / export / reference dialogs
        self._draw_import_dialog()
        self._draw_export_dialog()
        self._draw_reference_dialog()

        # Status text at bottom
        imgui.set_cursor_pos_y(
            imgui.get_window_height()
            - imgui.get_text_line_height_with_spacing() - 4
        )
        imgui.text_colored(
            imgui.ImVec4(0.8, 0.8, 0.8, 1.0),
            self.app.status_text,
        )

        imgui.end()

    def _render_skinned_mesh(self, renderer, camera, size):
        """Render the skinned mesh into the FBO with weight/segment overlay."""
        skinned_r = self.app.skinned_renderer
        mesh = self.app.skinned_mesh
        if skinned_r is None or renderer.fbo is None:
            return

        renderer.fbo.use()
        ctx = self.app.ctx

        aspect = size.x / max(size.y, 1)
        view = camera.get_view_matrix()
        proj = camera.get_projection_matrix(aspect)
        model = glm.mat4(1.0)

        mvp = proj * view * model
        mvp_tuple = tuple(c for col in mvp for c in col)
        model_tuple = tuple(c for col in model for c in col)

        normal_mat = glm.mat3(glm.transpose(glm.inverse(model)))
        normal_tuple = tuple(c for col in normal_mat for c in col)

        # GL state for skinned mesh rendering
        ctx.enable(moderngl.DEPTH_TEST)
        ctx.disable(moderngl.CULL_FACE)  # NIF winding may differ

        # --- Render reference body as transparent overlay (behind target) ---
        ref_mesh = self.app.reference_skinned_mesh
        if ref_mesh is not None:
            ctx.enable(moderngl.BLEND)
            ctx.blend_func = (
                moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA,
            )
            skinned_r.render(
                ref_mesh, mvp_tuple, model_tuple, normal_tuple,
                bone_matrices=None,
                alpha=0.25,
            )
            ctx.disable(moderngl.BLEND)

        # --- Bind diffuse texture if available ---
        if (mesh is not None
                and self.app._diffuse_texture is not None
                and mesh.diffuse_texture_id is not None):
            self.app._diffuse_texture.use(location=0)
            prog = skinned_r.program
            if prog and "u_diffuse_tex" in prog:
                prog["u_diffuse_tex"].value = 0

        # --- Render target mesh with active display mode ---
        if mesh is not None:
            mode = self.app.display_mode

            if mode == "segments" and mesh.segment_submeshes:
                # Submesh-based segment rendering (hard boundaries)
                skinned_r.render_submeshes(
                    mesh, mvp_tuple, model_tuple, normal_tuple,
                    bone_matrices=None,
                    selected_segment_id=self.app.selected_segment_id,
                    show_mask=self.app.show_mask,
                )
            else:
                weight_mode = mode == "weights"
                # "all_weights" reuses segment_color VBO with bone-blended colors
                segment_mode = mode in ("segments", "all_weights")
                vertex_color_mode = mode == "vertex_colors"

                skinned_r.render(
                    mesh, mvp_tuple, model_tuple, normal_tuple,
                    bone_matrices=None,  # Rest pose
                    weight_mode=weight_mode,
                    selected_bone_index=self.app.selected_bone_idx,
                    segment_mode=segment_mode,
                    vertex_color_mode=vertex_color_mode,
                    show_mask=self.app.show_mask,
                )

        ctx.screen.use()

    def _render_wireframe(self, renderer, camera, size):
        """Render wireframe edges over the mesh with depth test."""
        app = self.app
        brush_cursor = app.brush_cursor
        if brush_cursor is None or brush_cursor.program is None:
            return
        if app._wireframe_vao is None:
            return

        renderer.fbo.use()

        aspect = size.x / max(size.y, 1)
        view = camera.get_view_matrix()
        proj = camera.get_projection_matrix(aspect)
        mvp = proj * view  # model is identity

        mvp_tuple = tuple(c for col in mvp for c in col)

        prog = brush_cursor.program
        prog["u_mvp"].value = mvp_tuple
        prog["u_color"].value = (0.0, 0.0, 0.0, 0.3)

        ctx = app.ctx
        ctx.enable(moderngl.DEPTH_TEST)
        ctx.enable(moderngl.BLEND)
        ctx.blend_func = (moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA)

        app._wireframe_vao.render(moderngl.LINES)

        ctx.disable(moderngl.BLEND)

        ctx.screen.use()

    def _render_segment_edges(self, renderer, camera, size):
        """Render segment boundary edges as colored lines on top of any view.

        This allows viewing segment boundaries simultaneously with weight
        heatmaps, rather than requiring an exclusive segment display mode.
        """
        app = self.app
        brush_cursor = app.brush_cursor
        if brush_cursor is None or brush_cursor.program is None:
            return
        if app._segment_edge_vao is None:
            return

        renderer.fbo.use()

        aspect = size.x / max(size.y, 1)
        view = camera.get_view_matrix()
        proj = camera.get_projection_matrix(aspect)
        mvp = proj * view

        mvp_tuple = tuple(c for col in mvp for c in col)

        prog = brush_cursor.program
        prog["u_mvp"].value = mvp_tuple
        # Bright yellow-orange lines for segment boundaries
        prog["u_color"].value = (1.0, 0.7, 0.0, 0.85)

        ctx = app.ctx
        # Disable depth test so boundary lines always show on top of mesh
        ctx.disable(moderngl.DEPTH_TEST)
        ctx.enable(moderngl.BLEND)
        ctx.blend_func = (moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA)

        app._segment_edge_vao.render(moderngl.LINES)

        ctx.enable(moderngl.DEPTH_TEST)
        ctx.disable(moderngl.BLEND)

        ctx.screen.use()

    def _render_gradient_line(self, renderer, camera, size):
        """Render a preview line from gradient start to current cursor."""
        brush_cursor = self.app.brush_cursor
        if brush_cursor is None or brush_cursor.program is None:
            return

        start = self.app.gradient_start
        end = self._cursor_hit_point
        if start is None or end is None:
            return

        renderer.fbo.use()

        aspect = size.x / max(size.y, 1)
        view = self.app.camera.get_view_matrix()
        proj = self.app.camera.get_projection_matrix(aspect)
        vp = proj * view
        vp_tuple = tuple(c for col in vp for c in col)

        # Build a 2-vertex line buffer
        line_data = np.array([start, end], dtype=np.float32)
        line_vbo = self.app.ctx.buffer(line_data.tobytes())
        line_vao = self.app.ctx.vertex_array(
            brush_cursor.program,
            [(line_vbo, "3f", "in_position")],
        )

        brush_cursor.program["u_mvp"].value = vp_tuple
        brush_cursor.program["u_color"].value = (1.0, 0.9, 0.2, 0.9)  # Yellow

        self.app.ctx.disable(moderngl.DEPTH_TEST)
        line_vao.render(moderngl.LINES)
        self.app.ctx.enable(moderngl.DEPTH_TEST)

        line_vao.release()
        line_vbo.release()

        self.app.ctx.screen.use()

    def _render_brush_cursor(self, renderer, camera, size):
        """Render the 3D brush ring at the current cursor position."""
        brush_cursor = self.app.brush_cursor
        if brush_cursor is None:
            return
        if self._cursor_hit_point is None or self._cursor_normal is None:
            return

        renderer.fbo.use()

        aspect = size.x / max(size.y, 1)
        view = camera.get_view_matrix()
        proj = camera.get_projection_matrix(aspect)

        vp = proj * view
        vp_tuple = tuple(c for col in vp for c in col)

        # Choose cursor color based on brush mode
        if self.app.brush_type == "segment" and self.app.selected_segment_id >= 0:
            from .segment_panel import get_segment_color
            pc = get_segment_color(self.app.selected_segment_id)
            alpha = 0.95 if self._painting else 0.8
            color = (pc[0], pc[1], pc[2], alpha)
        elif self.app.brush_type == "mask":
            alpha = 0.95 if self._painting else 0.8
            color = (0.3, 0.3, 0.3, alpha)  # Dark gray for masking
        elif self.app.brush_type == "unmask":
            alpha = 0.95 if self._painting else 0.8
            color = (0.9, 0.9, 0.3, alpha)  # Yellow for unmasking
        elif self._painting:
            color = (1.0, 0.4, 0.2, 0.95)  # Orange while painting
        else:
            color = (1.0, 1.0, 1.0, 0.8)  # White while hovering

        brush_cursor.render(
            vp_tuple,
            self._cursor_hit_point,
            self._cursor_normal,
            self.app.brush_radius,
            color=color,
        )

        # Render mirrored cursor when mirror_x is enabled
        if self.app.mirror_x and self._cursor_hit_point is not None:
            mirror_pos = self._cursor_hit_point.copy()
            mirror_pos[0] = -mirror_pos[0]
            mirror_normal = self._cursor_normal.copy()
            mirror_normal[0] = -mirror_normal[0]
            mirror_color = (0.4, 0.7, 1.0, 0.25)  # Light blue, 25% alpha
            brush_cursor.render(
                vp_tuple,
                mirror_pos,
                mirror_normal,
                self.app.brush_radius,
                color=mirror_color,
            )

        self.app.ctx.screen.use()

    def _handle_brush(self, io, viewport_pos, size):
        """Handle brush painting on the mesh."""
        if self.app.mesh_picker is None or self.app.skin_data is None:
            return

        # Calculate mouse position relative to viewport
        mouse_x = io.mouse_pos.x - viewport_pos.x
        mouse_y = io.mouse_pos.y - viewport_pos.y

        if mouse_x < 0 or mouse_y < 0 or mouse_x > size.x or mouse_y > size.y:
            return

        camera = self.app.camera
        aspect = size.x / max(size.y, 1)
        view = camera.get_view_matrix()
        proj = camera.get_projection_matrix(aspect)

        ray_origin, ray_dir = self.app.mesh_picker.unproject_ray(
            mouse_x, mouse_y, size.x, size.y, view, proj,
        )

        result = self.app.mesh_picker.pick_surface_point(ray_origin, ray_dir)
        if result is None:
            return

        hit_point, tri_idx = result

        # Update brush cursor position during painting too
        self._cursor_hit_point = hit_point.astype(np.float32)
        sd = self.app.skin_data
        tri = sd.triangles[tri_idx]
        v0, v1, v2 = sd.vertices[tri[0]], sd.vertices[tri[1]], sd.vertices[tri[2]]
        normal = np.cross(v1 - v0, v2 - v0)
        n_len = np.linalg.norm(normal)
        self._cursor_normal = (
            (normal / n_len).astype(np.float32) if n_len > 1e-6
            else np.array([0, 0, 1], dtype=np.float32)
        )

        # Gradient brush: two-click workflow (start → end)
        if self.app.brush_type == "gradient":
            if not self._painting:
                self._painting = True
                if not self.app.gradient_pending:
                    # First click — set start point
                    self.app.gradient_start = hit_point.astype(np.float32)
                    self.app.gradient_pending = True
                    self.app.status_text = "Gradient: click endpoint"
                else:
                    # Second click — apply gradient
                    self.app.push_undo("Brush: gradient")
                    self.app.apply_gradient(
                        self.app.gradient_start,
                        hit_point.astype(np.float32),
                    )
                    self.app.gradient_pending = False
                    self.app.gradient_start = None
                    self.app.status_text = "Gradient applied"
            return

        # Push undo only on first click (not during drag)
        if not self._painting:
            self.app.push_undo(f"Brush: {self.app.brush_type}")
            self._painting = True

        if self.app.brush_type == "segment":
            self.app.apply_segment_brush(
                hit_point.astype(np.float32), tri_idx,
            )
        elif self.app.brush_type in ("mask", "unmask"):
            self.app.apply_mask_brush(
                hit_point.astype(np.float32),
                unmask=(self.app.brush_type == "unmask"),
                hit_tri_idx=tri_idx,
            )
        else:
            self.app.apply_brush(hit_point.astype(np.float32), tri_idx)

        # Find hover vertex (nearest to hit point)
        dists = np.linalg.norm(
            self.app.skin_data.vertices - hit_point.astype(np.float32), axis=1,
        )
        self._hover_vertex = int(np.argmin(dists))

    def _update_hover(self, io, viewport_pos, size):
        """Track vertex under cursor for info display and brush cursor."""
        if self.app.mesh_picker is None or self.app.skin_data is None:
            self._hover_vertex = -1
            self._cursor_hit_point = None
            self._cursor_normal = None
            return

        mouse_x = io.mouse_pos.x - viewport_pos.x
        mouse_y = io.mouse_pos.y - viewport_pos.y

        if mouse_x < 0 or mouse_y < 0 or mouse_x > size.x or mouse_y > size.y:
            self._hover_vertex = -1
            self._cursor_hit_point = None
            self._cursor_normal = None
            return

        camera = self.app.camera
        aspect = size.x / max(size.y, 1)
        view = camera.get_view_matrix()
        proj = camera.get_projection_matrix(aspect)

        ray_origin, ray_dir = self.app.mesh_picker.unproject_ray(
            mouse_x, mouse_y, size.x, size.y, view, proj,
        )

        vi = self.app.mesh_picker.pick_vertex(ray_origin, ray_dir, radius=2.0)
        self._hover_vertex = vi if vi is not None else -1

        # Track surface hit point + normal for brush cursor
        result = self.app.mesh_picker.pick_surface_point(ray_origin, ray_dir)
        if result is not None:
            hit_point, tri_idx = result
            self._cursor_hit_point = hit_point.astype(np.float32)
            # Compute face normal from triangle vertices
            sd = self.app.skin_data
            tri = sd.triangles[tri_idx]
            v0 = sd.vertices[tri[0]]
            v1 = sd.vertices[tri[1]]
            v2 = sd.vertices[tri[2]]
            edge1 = v1 - v0
            edge2 = v2 - v0
            normal = np.cross(edge1, edge2)
            n_len = np.linalg.norm(normal)
            if n_len > 1e-6:
                self._cursor_normal = (normal / n_len).astype(np.float32)
            else:
                self._cursor_normal = np.array([0, 0, 1], dtype=np.float32)
        else:
            self._cursor_hit_point = None
            self._cursor_normal = None

    def _handle_keyboard_shortcuts(self):
        """Process keyboard shortcuts for the weight painter."""
        io = imgui.get_io()
        app = self.app

        # --- Shift+click: temporarily switch to smooth brush ---
        shift_down = io.key_shift
        if shift_down and not self._shift_held:
            # Shift just pressed — save current brush and switch to smooth
            self._shift_held = True
            if app.brush_type != "smooth":
                self._pre_shift_brush = app.brush_type
                app.brush_type = "smooth"
        elif not shift_down and self._shift_held:
            # Shift released — restore previous brush
            self._shift_held = False
            if self._pre_shift_brush is not None:
                app.brush_type = self._pre_shift_brush
                self._pre_shift_brush = None

        # Don't process single-key shortcuts if a text input is active
        if io.want_text_input:
            return

        ctrl = io.key_ctrl

        # Ctrl+Z → undo
        if ctrl and imgui.is_key_pressed(imgui.Key.z) and not io.key_shift:
            app.undo()
        # Ctrl+Y or Ctrl+Shift+Z → redo
        if ctrl and imgui.is_key_pressed(imgui.Key.y):
            app.redo()
        if ctrl and io.key_shift and imgui.is_key_pressed(imgui.Key.z):
            app.redo()

        # [ / ] → decrease/increase brush radius
        if imgui.is_key_pressed(imgui.Key.left_bracket):
            app.brush_radius = max(0.1, app.brush_radius * 0.9)
        if imgui.is_key_pressed(imgui.Key.right_bracket):
            app.brush_radius = min(50.0, app.brush_radius * 1.1)

        # 1-7 → select brush type
        _BRUSH_KEYS = [
            (imgui.Key._1, "paint"),
            (imgui.Key._2, "smooth"),
            (imgui.Key._3, "blur"),
            (imgui.Key._4, "gradient"),
            (imgui.Key._5, "mirror"),
            (imgui.Key._6, "flood"),
            (imgui.Key._7, "segment"),
        ]
        if not ctrl:
            for key, brush_id in _BRUSH_KEYS:
                if imgui.is_key_pressed(key):
                    app.brush_type = brush_id
                    if brush_id == "segment":
                        app.set_display_mode("segments")

        # P → toggle segment view mode
        if not ctrl and imgui.is_key_pressed(imgui.Key.p):
            if app.display_mode == "segments":
                app.set_display_mode("weights")
            else:
                app.set_display_mode("segments")

        # W → toggle weight view mode
        if not ctrl and imgui.is_key_pressed(imgui.Key.w):
            if app.display_mode == "weights":
                app.set_display_mode("shaded")
            else:
                app.set_display_mode("weights")

        # A → toggle all-bones view
        if not ctrl and imgui.is_key_pressed(imgui.Key.a):
            if app.display_mode == "all_weights":
                app.set_display_mode("weights")
            else:
                app.set_display_mode("all_weights")

        # V → toggle vertex color display (only if mesh has vertex colors)
        if not ctrl and imgui.is_key_pressed(imgui.Key.v):
            if (app.skin_data is not None
                    and app.skin_data.vertex_colors is not None):
                if app.display_mode == "vertex_colors":
                    app.set_display_mode("weights")
                else:
                    app.set_display_mode("vertex_colors")

        # X → toggle mirror X
        if not ctrl and imgui.is_key_pressed(imgui.Key.x):
            app.mirror_x = not app.mirror_x

        # N → toggle auto-normalize
        if not ctrl and imgui.is_key_pressed(imgui.Key.n):
            app.auto_normalize = not app.auto_normalize

        # F → toggle wireframe overlay
        if not ctrl and imgui.is_key_pressed(imgui.Key.f):
            app.show_wireframe = not app.show_wireframe

        # B → toggle segment boundary edges
        if not ctrl and imgui.is_key_pressed(imgui.Key.b):
            app.show_segment_edges = not app.show_segment_edges

        # M → toggle mask visibility
        if not ctrl and imgui.is_key_pressed(imgui.Key.m):
            app.show_mask = not app.show_mask

    def _draw_vertex_info(self, viewport_pos, size):
        """Draw vertex weight info as an overlay tooltip."""
        sd = self.app.skin_data
        if sd is None or self._hover_vertex < 0:
            return

        vi = self._hover_vertex
        if vi >= sd.num_vertices:
            return

        bone_weights = sd.get_vertex_weights(vi)
        if not bone_weights:
            return

        # Draw as a floating tooltip near cursor
        io = imgui.get_io()
        imgui.set_next_window_pos(
            imgui.ImVec2(io.mouse_pos.x + 16, io.mouse_pos.y + 16),
        )
        imgui.set_next_window_bg_alpha(0.75)
        tooltip_flags = (
            imgui.WindowFlags_.no_decoration.value
            | imgui.WindowFlags_.always_auto_resize.value
            | imgui.WindowFlags_.no_saved_settings.value
            | imgui.WindowFlags_.no_focus_on_appearing.value
            | imgui.WindowFlags_.no_nav.value
            | imgui.WindowFlags_.no_move.value
            | imgui.WindowFlags_.no_inputs.value
        )
        imgui.begin(f"##vertex_info_{vi}", flags=tooltip_flags)
        imgui.text(f"Vertex {vi}")
        imgui.separator()
        for bone_name, weight in bone_weights:
            # Color: blue (0) -> green (0.5) -> red (1.0)
            r = min(weight * 2, 1.0)
            g = min((1.0 - weight) * 2, 1.0) if weight > 0.5 else min(weight * 2, 1.0)
            b = max(0.0, 1.0 - weight * 2)
            imgui.text_colored(
                imgui.ImVec4(r, g, b, 1.0),
                f"  {bone_name}: {weight:.3f}",
            )
        imgui.end()

    def _draw_import_dialog(self):
        """Open a native file picker for mesh import."""
        if self.app._show_import_dialog:
            self.app._show_import_dialog = False
            path = _open_file_dialog(
                "Import Mesh",
                filetypes=[
                    ("Mesh files", "*.nif *.obj"),
                    ("NIF files", "*.nif"),
                    ("OBJ files", "*.obj"),
                    ("All files", "*.*"),
                ],
            )
            if path:
                self.app.import_mesh(path)

    def _draw_export_dialog(self):
        """Open a native file picker for NIF export."""
        if self.app._show_export_dialog:
            self.app._show_export_dialog = False
            path = _save_file_dialog(
                "Export NIF",
                filetypes=[("NIF files", "*.nif"), ("All files", "*.*")],
                default_ext=".nif",
            )
            if path:
                self.app.export_nif(path)

    def _draw_reference_dialog(self):
        """Show reference body preset popup or fall back to native file picker."""
        if self.app._show_reference_dialog:
            self.app._show_reference_dialog = False
            if self._ref_body_panel is not None:
                imgui.open_popup("Reference Body##ref_body_popup")
            else:
                # No toolkit_settings — fall back to file dialog
                path = _open_file_dialog(
                    "Load Reference Body",
                    filetypes=[("NIF files", "*.nif"), ("All files", "*.*")],
                )
                if path:
                    self.app.load_reference(path)

        # Preset popup
        if self._ref_body_panel is not None:
            if imgui.begin_popup("Reference Body##ref_body_popup"):
                self._ref_body_panel.draw()
                imgui.spacing()
                imgui.separator()
                imgui.spacing()
                if imgui.button("Browse Custom NIF...", imgui.ImVec2(-1, 0)):
                    path = _open_file_dialog(
                        "Load Reference Body",
                        filetypes=[("NIF files", "*.nif"), ("All files", "*.*")],
                    )
                    if path:
                        self.app.load_reference(path)
                        imgui.close_current_popup()
                imgui.end_popup()

    def _on_preset_load(self, skeleton_hkx: str, skeleton_nif: str | None,
                        body_nif_paths: list[str], game: str):
        """Handle preset body load — merge body parts into reference_skin."""
        from creation_lib.skinning.reference_body import extract_skin_data_from_nif, _merge_skin_data

        parts = []
        for nif_path in body_nif_paths:
            try:
                skin = extract_skin_data_from_nif(nif_path)
                parts.append(skin)
            except Exception as e:
                _log.warning("Failed to load %s: %s", nif_path, e)

        if not parts:
            self.app.status_text = "No body parts could be loaded"
            return

        merged = _merge_skin_data(parts) if len(parts) > 1 else parts[0]
        self.app.reference_skin = merged
        self.app._build_reference_gpu_mesh()
        # Frame camera on reference if no target mesh loaded yet
        if self.app.skin_data is None:
            self.app._frame_camera_on_mesh(merged)
        self.app.status_text = (
            f"Reference: {merged.num_vertices}v, {len(merged.bone_names)} bones"
        )
        _log.info("Loaded reference preset: %d parts, %d verts",
                  len(parts), merged.num_vertices)
