"""SWF Pipboy icon editor -- main application class.

2D orthographic canvas workspace. Does not inherit MeshWorkspaceBase
(that's for 3D mesh editing). Uses its own OrthoCamera and SwfRenderer.
"""

from __future__ import annotations

import logging
from pathlib import Path

import glm
from imgui_bundle import imgui
import moderngl

from ui.mesh_workspace.undo import UndoStack
from ui.swf_editor.ortho_camera import OrthoCamera
from ui.swf_editor.swf_renderer import SwfRenderer
from ui.swf_editor.tools import BaseTool
from ui.swf_editor.commands import DocumentActions

_log = logging.getLogger(__name__)


class SwfEditorApp(DocumentActions):
    """Main SWF editor application."""

    def __init__(self, toolkit_settings=None):
        self._toolkit_settings = toolkit_settings
        self.active = False

        # GL resources (initialized in setup())
        self.ctx: moderngl.Context | None = None
        self.renderer: SwfRenderer | None = None
        self.camera = OrthoCamera()

        # Document
        self._init_document()
        self.undo_stack: UndoStack = UndoStack(max_entries=50)

        # Tool state
        self.active_tool: str = "select"
        self._tools: dict[str, BaseTool] = {}
        self._tools_initialized = False

        # Panels (initialized in _init_panels())
        self.canvas_panel = None
        self.timeline_panel = None
        self.layers_panel = None
        self.tools_panel = None
        self.properties_panel = None
        self.library_panel = None
        self.document_panel = None
        self.stage_panel = None

        self._first_frame = True
        self._panels_initialized = False

        # Status
        self.status_text = "Ready"

    @property
    def current_tool(self) -> BaseTool | None:
        """Return the active tool instance."""
        return self._tools.get(self.active_tool)

    def _init_tools(self) -> None:
        """Create tool instances. Called once after init."""
        from ui.swf_editor.tools.select_tool import SelectTool
        from ui.swf_editor.tools.direct_select import DirectSelectTool
        from ui.swf_editor.tools.pen_tool import PenTool
        from ui.swf_editor.tools.shape_tool import ShapeTool, ShapeMode
        from ui.swf_editor.tools.fill_tool import FillTool

        self._tools["select"] = SelectTool(self)
        self._tools["direct_select"] = DirectSelectTool(self)
        self._tools["pen"] = PenTool(self)

        rect_tool = ShapeTool(self)
        rect_tool.mode = ShapeMode.RECTANGLE
        self._tools["rect"] = rect_tool

        ellipse_tool = ShapeTool(self)
        ellipse_tool.mode = ShapeMode.ELLIPSE
        self._tools["ellipse"] = ellipse_tool

        line_tool = ShapeTool(self)
        line_tool.mode = ShapeMode.LINE
        self._tools["line"] = line_tool

        self._tools["fill"] = FillTool(self)

        # Eyedropper is fill tool in eyedropper mode
        eyedropper = FillTool(self)
        eyedropper.mode = "eyedropper"
        self._tools["eyedropper"] = eyedropper

        self._tools_initialized = True

    def setup(self) -> None:
        """Initialize GL resources. Called on first frame when context available."""
        self.ctx = moderngl.get_context()
        self.renderer = SwfRenderer(self.ctx)
        self.camera.fit_canvas()
        if not self._tools_initialized:
            self._init_tools()
        _log.info("SWF editor GL context initialized")

    def _init_panels(self) -> None:
        """Create panel instances. Called after setup()."""
        from ui.swf_editor.panels.canvas_panel import CanvasPanel
        from ui.swf_editor.panels.timeline_panel import TimelinePanel
        from ui.swf_editor.panels.layers_panel import LayersPanel
        from ui.swf_editor.panels.tools_panel import ToolsPanel
        from ui.swf_editor.panels.properties_panel import PropertiesPanel
        from ui.swf_editor.panels.library_panel import LibraryPanel
        from ui.swf_editor.panels.document_panel import DocumentPanel
        from ui.swf_editor.panels.stage_panel import StagePanel

        self.canvas_panel = CanvasPanel(self)
        self.timeline_panel = TimelinePanel(self)
        self.layers_panel = LayersPanel(self)
        self.tools_panel = ToolsPanel(self)
        self.properties_panel = PropertiesPanel(self)
        self.library_panel = LibraryPanel(self)
        self.document_panel = DocumentPanel(self)
        self.stage_panel = StagePanel(self)
        self._panels_initialized = True

    def draw_workspace(self) -> None:
        """Main draw loop -- called each frame."""
        if self._first_frame:
            self.setup()
            self._first_frame = False
        if not self._panels_initialized:
            self._init_panels()

        self._handle_shortcuts()
        self.poll()
        self.draw_pending_dialog()
        self._draw_viewport()

    def _draw_viewport(self) -> None:
        """Render the 2D canvas to FBO and blit to ImGui."""
        if not self.renderer:
            return

        imgui.push_style_var(imgui.StyleVar_.window_padding, imgui.ImVec2(0, 0))
        visible, _ = imgui.begin("Viewport##swf", flags=imgui.WindowFlags_.no_scrollbar)
        imgui.pop_style_var()
        if not visible:
            imgui.end()
            return

        if self.document.mode == "binary":
            self.stage_panel.draw_viewport()
            imgui.end()
            return
        if self.document.mode != "visual":
            imgui.text_wrapped("Use Document for AS3 source, compiled structure and Ruffle preview. The vector canvas is available for artwork projects.")
            imgui.text_wrapped(self.status_text)
            imgui.end()
            return

        size = imgui.get_content_region_avail()
        w, h = max(int(size.x), 1), max(int(size.y), 1)

        self.camera.viewport_width = w
        self.camera.viewport_height = h
        if self._fit_on_next_viewport and w >= 40 and h >= 40:
            self.camera.fit_canvas()
            self._fit_on_next_viewport = False

        self.renderer.ensure_fbo(w, h)

        # Collect visible shapes
        entries = self.scene.get_render_entries()
        shape_list = []
        for shape, transform, alpha in entries:
            shape_list.append(
                (shape, self._gl_transform(transform), alpha)
            )  # NOTE: transform is discarded here (identity used) -- per-shape placement not yet applied

        if self.timeline_panel and self.timeline_panel.onion_skin:
            original_frame = self.scene.current_frame
            ghosts = []
            for frame in range(max(0, original_frame - self.timeline_panel.onion_range),
                               min(self.scene.total_frames, original_frame + self.timeline_panel.onion_range + 1)):
                if frame != original_frame:
                    self.scene.current_frame = frame
                    ghosts.extend((s, self._gl_transform(t), a * 0.18) for s, t, a in self.scene.get_render_entries())
            self.scene.current_frame = original_frame
            shape_list = ghosts + shape_list
        projection = self.camera.get_projection()
        self.renderer.render(
            projection,
            shape_list,
            bg_color=(0.12, 0.12, 0.14),
            canvas_width=self.scene.canvas_width,
            canvas_height=self.scene.canvas_height,
            canvas_color=(self.scene.background.r / 255, self.scene.background.g / 255,
                          self.scene.background.b / 255),
        )

        tex_id = self.renderer.get_fbo_texture_id()
        if tex_id:
            # Record image position for mouse coordinate mapping
            img_pos = imgui.get_cursor_screen_pos()
            imgui.image(
                imgui.ImTextureRef(tex_id),
                imgui.ImVec2(w, h),
                uv0=imgui.ImVec2(0, 1),
                uv1=imgui.ImVec2(1, 0),
            )

        # Input handling
        if tex_id and (imgui.is_item_hovered() or self._captured_tool is not None):
            io = imgui.get_io()
            # Mouse position relative to viewport image widget
            mx = io.mouse_pos.x - img_pos.x
            my = io.mouse_pos.y - img_pos.y

            # Camera: scroll zoom (toward cursor), middle-drag pan
            if io.mouse_wheel != 0:
                self.camera.do_zoom(io.mouse_wheel, mx, my)
            if io.mouse_down[2]:  # middle button pan
                self.camera.do_pan(io.mouse_delta.x, io.mouse_delta.y)

            # Hand/zoom tools: left-drag pans/zooms directly
            if self.active_tool == "hand":
                if io.mouse_down[0]:
                    self.camera.do_pan(io.mouse_delta.x, io.mouse_delta.y)
            elif self.active_tool == "zoom":
                if imgui.is_mouse_clicked(0):
                    direction = -1.0 if io.key_alt else 1.0
                    self.camera.do_zoom(direction * 2, mx, my)
            else:
                # Tool input: left-click dispatched to active tool
                tool = self._captured_tool or self.current_tool
                if tool:
                    for btn in range(2):  # left=0, right=1
                        if imgui.is_mouse_clicked(btn):
                            self._gesture_snapshot = self._take_snapshot()
                            self._gesture_undo_count = len(self.undo_stack._undo)
                            if tool.on_mouse_down(mx, my, btn):
                                self._captured_tool = tool

                    tool.on_mouse_move(mx, my)

                    for btn in range(2):
                        if imgui.is_mouse_released(btn):
                            tool.on_mouse_up(mx, my, btn)
                            self._captured_tool = None
                            self._gesture_snapshot = None

        # Tool overlay (draw handles, preview shapes, etc.)
        tool = self.current_tool
        if tool and tex_id:
            dl = imgui.get_window_draw_list()
            from ui.swf_editor.ortho_camera import OverlayCamera
            dl.push_clip_rect(img_pos, imgui.ImVec2(img_pos.x + w, img_pos.y + h), True)
            tool.draw_overlay(dl, OverlayCamera(self.camera, img_pos.x, img_pos.y))
            dl.pop_clip_rect()

        imgui.end()

    def _handle_shortcuts(self) -> None:
        """Process keyboard shortcuts for tool switching."""
        if self.library_panel:
            self.library_panel.background_tick()
        if self.stage_panel:
            try:
                self.stage_panel.tick()
            except Exception as exc:
                self.report_error(exc)
        io = imgui.get_io()

        if io.key_ctrl:
            if imgui.is_key_pressed(imgui.Key.o):
                self.choose_open()
            elif imgui.is_key_pressed(imgui.Key.n):
                self.new_document()
            elif imgui.is_key_pressed(imgui.Key.s):
                self.save_document(save_as=io.key_shift)
            elif imgui.is_key_pressed(imgui.Key.e):
                self.choose_export()
        if imgui.is_key_pressed(imgui.Key.f5):
            self.start_build(preview=io.key_ctrl)
        if io.want_text_input:
            return

        tool_keys = {
            imgui.Key.v: "select",
            imgui.Key.a: "direct_select",
            imgui.Key.p: "pen",
            imgui.Key.r: "rect",
            imgui.Key.e: "ellipse",
            imgui.Key.l: "line",
            imgui.Key.g: "fill",
            imgui.Key.i: "eyedropper",
            imgui.Key.h: "hand",
            imgui.Key.z: "zoom",
        }
        for key, tool in tool_keys.items():
            if not (io.key_ctrl or io.key_alt or io.key_shift) and imgui.is_key_pressed(key):
                self.select_tool(tool)
                break

        # Forward key events to active tool (e.g. Escape cancels pen path)
        active = self.current_tool
        if self.document.mode != "visual":
            active = None
        if active:
            for key in (
                imgui.Key.escape,
                imgui.Key.delete,
                imgui.Key.backspace,
                imgui.Key.enter,
            ):
                if imgui.is_key_pressed(key):
                    active.on_key(key, True)
                    if key == imgui.Key.escape:
                        self.cancel_gesture()

        # Undo/Redo
        ctrl = io.key_ctrl
        shift = io.key_shift
        if ctrl and imgui.is_key_pressed(imgui.Key.z):
            if shift:
                self.redo()
            else:
                self.undo()

        # Path boolean operations (Ctrl+Shift+key)
        if ctrl and shift and self.document.mode == "visual":
            if imgui.is_key_pressed(imgui.Key.u):
                self.boolean_op("union")
            elif imgui.is_key_pressed(imgui.Key.i):
                self.boolean_op("intersect")
            elif imgui.is_key_pressed(imgui.Key.x):
                self.boolean_op("exclude")

    def open_swf(self, path: str) -> None:
        """Open a SWF file for editing."""
        self.open_document(path)

    def place_shape(self, shape_row: dict) -> bool:
        """Compatibility entry: preview and placement use the same geometry decoder."""
        from creation_lib.swf.library_assets import decode_geometry
        try:
            shape, warning = decode_geometry(shape_row.get("shape_data") or b"", shape_row.get("svg_preview") or "")
            placed = self.place_library_asset(shape, shape_row.get("name", "shape"))
            if placed and warning:
                self.status_text += " • " + warning
            return placed
        except Exception as exc:
            self.report_error(exc)
            return False

    def place_library_asset(self, shape, name="shape") -> bool:
        """Copy a validated vector onto the active frame as one undoable action."""
        import copy
        from creation_lib.swf.library_assets import validate_geometry
        from ui.swf_editor.swf_scene import EditorDisplayEntry, Keyframe
        if self.document.mode != "visual":
            self.report_error("Library placement is available in vector artwork projects")
            return False
        if not self.scene.layers:
            self.report_error("Add a layer before placing library artwork")
            return False
        layer = self.scene.layers[self.scene.active_layer_index]
        if layer.locked:
            self.report_error("The active layer is locked")
            return False
        validate_geometry(shape)
        shape_id = max(self.scene.library_symbols.keys(), default=0) + 1
        if shape_id > 65535:
            self.report_error("SWF character ID limit reached")
            return False
        new_shape = copy.deepcopy(shape)
        new_shape.shape_id = shape_id
        self.push_undo("Place Library Shape")
        self.scene.library_symbols[shape_id] = new_shape
        keyframe = layer.keyframe_at(self.scene.current_frame)
        if not keyframe:
            keyframe = Keyframe(frame=self.scene.current_frame)
            layer.keyframes.append(keyframe)
            layer.keyframes.sort(key=lambda item: item.frame)
        keyframe.display_list.append(EditorDisplayEntry(shape_id=shape_id))
        self.scene.selection = [(self.scene.active_layer_index, len(keyframe.display_list) - 1)]
        self.scene.dirty = True
        self.status_text = f"Placed {name}"
        return True

    def export_swf(self, path: str) -> None:
        """Export current document to SWF."""
        try:
            data = self.document.export(path)
            self.status_text = f"Exported {Path(path).name} ({len(data):,} bytes)"
        except Exception as exc:
            self.report_error(exc)

    def undo(self) -> None:
        current = self._take_snapshot()
        result = self.undo_stack.undo()
        if result:
            _label, snap = result
            self.undo_stack._redo[-1] = (_label, current)
            self._restore_snapshot(snap)

    def redo(self) -> None:
        current = self._take_snapshot()
        result = self.undo_stack.redo()
        if result:
            _label, snap = result
            self.undo_stack._undo[-1] = (_label, current)
            self._restore_snapshot(snap)

    def push_undo(self, label: str) -> None:
        """Save current state for undo."""
        snap = self._take_snapshot()
        self.undo_stack.push(label, snap)
        self.scene.dirty = True

    def _take_snapshot(self) -> dict:
        """Capture current scene state for undo."""
        import copy

        return copy.deepcopy(self.document)

    def _restore_snapshot(self, snap: dict) -> None:
        """Restore scene state from undo snapshot."""
        import copy

        saved_state, path = self.document.saved_state, self.document.path
        self.document = copy.deepcopy(snap)
        self.document.saved_state, self.document.path = saved_state, path
        self.compiled_bytes = self.compiled_key = None
        if self.document_panel:
            self.document_panel.reset()
        if self.renderer:
            # Invalidate all cached tessellations
            for sid in self.scene.library_symbols:
                self.renderer.invalidate_shape(sid)

    def boolean_op(self, operation: str) -> None:
        """Perform path boolean operation on selected shapes.

        operation: "union" | "subtract" | "intersect" | "exclude"
        Requires 2+ shapes selected.
        """
        import pyclipper

        from creation_lib.swf.shapes import (
            ShapeDef,
            StraightEdge,
            StyleChange,
            EndShape,
        )
        from creation_lib.swf.types import FillStyle, RGBA

        scene = self.scene
        if len(scene.selection) < 2:
            return

        op_map = {
            "union": pyclipper.CT_UNION,
            "subtract": pyclipper.CT_DIFFERENCE,
            "intersect": pyclipper.CT_INTERSECTION,
            "exclude": pyclipper.CT_XOR,
        }
        clip_type = op_map.get(operation, pyclipper.CT_UNION)

        # Collect polygon paths from selected shapes
        all_polys: list[list[tuple[int, int]]] = []
        selected_entries = []
        for layer_idx, entry_idx in scene.selection:
            layer = scene.layers[layer_idx]
            kf = layer.keyframe_at(scene.current_frame)
            if not kf or entry_idx >= len(kf.display_list):
                continue
            de = kf.display_list[entry_idx]
            shape = scene.library_symbols.get(de.shape_id)
            if not shape:
                continue
            selected_entries.append((layer_idx, entry_idx, de, shape))
            all_polys.append(self._shape_to_polygons(shape, de.transform))

        if len(all_polys) < 2:
            return

        # Run clipper operation
        pc = pyclipper.Pyclipper()
        pc.AddPaths(
            pyclipper.scale_to_clipper(all_polys[0]),
            pyclipper.PT_SUBJECT,
            True,
        )
        for poly in all_polys[1:]:
            pc.AddPaths(
                pyclipper.scale_to_clipper(poly),
                pyclipper.PT_SUBJECT if operation == "union" else pyclipper.PT_CLIP,
                True,
            )

        try:
            result_polys = pc.Execute(
                clip_type,
                pyclipper.PFT_NONZERO,
                pyclipper.PFT_NONZERO,
            )
        except pyclipper.ClipperException:
            _log.warning("Clipper boolean op failed for %s", operation)
            return

        if not result_polys:
            self.push_undo(f"Path {operation.title()}")
            for li, ei, _, _ in sorted(selected_entries, key=lambda e: e[:2], reverse=True):
                scene.layers[li].keyframe_at(scene.current_frame).display_list.pop(ei)
            scene.selection.clear()
            return

        result_polys = pyclipper.scale_from_clipper(result_polys)

        # Convert result polygons back to shape records
        records = []
        for poly in result_polys:
            if len(poly) < 3:
                continue
            records.append(
                StyleChange(
                    move_x=int(poly[0][0] * 20),
                    move_y=int(poly[0][1] * 20),
                    fill1=1,
                )
            )
            for j in range(1, len(poly)):
                dx = int((poly[j][0] - poly[j - 1][0]) * 20)
                dy = int((poly[j][1] - poly[j - 1][1]) * 20)
                records.append(StraightEdge(dx=dx, dy=dy))
            # Close back to start
            dx = int((poly[0][0] - poly[-1][0]) * 20)
            dy = int((poly[0][1] - poly[-1][1]) * 20)
            records.append(StraightEdge(dx=dx, dy=dy))
        records.append(EndShape())

        if not records:
            return

        # Compute bounds from all result polygons
        all_pts = [pt for poly in result_polys for pt in poly]
        min_x = min(p[0] for p in all_pts) * 20
        min_y = min(p[1] for p in all_pts) * 20
        max_x = max(p[0] for p in all_pts) * 20
        max_y = max(p[1] for p in all_pts) * 20

        # Create new shape from result
        shape_id = max(scene.library_symbols.keys(), default=0) + 1
        result_shape = ShapeDef(
            shape_id=shape_id,
            bounds=tuple(round(v) for v in (min_x, min_y, max_x, max_y)),
            fill_styles=[FillStyle(fill_type=0, color=RGBA(255, 255, 255, 255))],
            line_styles=[],
            records=records,
        )
        self.push_undo(f"Path {operation.title()}")
        scene.library_symbols[shape_id] = result_shape

        # Remove old selected entries (in reverse order to preserve indices)
        from ui.swf_editor.swf_scene import EditorDisplayEntry

        removals = sorted(
            [(li, ei) for li, ei, _, _ in selected_entries],
            reverse=True,
        )
        for li, ei in removals:
            layer = scene.layers[li]
            kf = layer.keyframe_at(scene.current_frame)
            if kf and ei < len(kf.display_list):
                kf.display_list.pop(ei)

        # Add result shape to first selected layer
        first_layer_idx = selected_entries[0][0]
        layer = scene.layers[first_layer_idx]
        kf = layer.keyframe_at(scene.current_frame)
        if kf:
            kf.display_list.append(EditorDisplayEntry(shape_id=shape_id))
            new_idx = len(kf.display_list) - 1
            scene.selection = [(first_layer_idx, new_idx)]
        else:
            scene.selection = []

        scene.dirty = True

    @staticmethod
    def _shape_to_polygons(shape, transform):
        from creation_lib.swf.svg_io import _bucket_edges, _stitch
        from ui.swf_editor.swf_renderer import _subdivide_quadratic
        fills, _ = _bucket_edges(shape)
        polygons = []
        for _, segments in fills.values():
            for contour in _stitch(segments):
                points = [(contour[0][0] / 20, contour[0][1] / 20)]
                for sx, sy, ex, ey, cx, cy in contour:
                    if cx is None:
                        points.append((ex / 20, ey / 20))
                    else:
                        points.extend(_subdivide_quadratic(sx / 20, sy / 20, cx / 20, cy / 20, ex / 20, ey / 20)[1:])
                if len(points) >= 3:
                    polygons.append([transform.apply(x, y) for x, y in points])
        return polygons

    @staticmethod
    def _shape_to_polygon(
        shape,
        offset_x: float = 0.0,
        offset_y: float = 0.0,
    ) -> list[tuple[float, float]]:
        """Convert shape records to a flat polygon (adaptive bezier subdivision)."""
        from creation_lib.swf.shapes import StraightEdge, CurvedEdge, StyleChange, EndShape

        points: list[tuple[float, float]] = []
        cur_x, cur_y = 0.0, 0.0

        for rec in shape.records:
            if isinstance(rec, StyleChange) and rec.has_move:
                cur_x = rec.move_x / 20.0 + offset_x
                cur_y = rec.move_y / 20.0 + offset_y
                points.append((cur_x, cur_y))
            elif isinstance(rec, StraightEdge):
                cur_x += rec.dx / 20.0
                cur_y += rec.dy / 20.0
                points.append((cur_x, cur_y))
            elif isinstance(rec, CurvedEdge):
                # Subdivide quadratic bezier into line segments
                cx = cur_x + rec.cx / 20.0
                cy = cur_y + rec.cy / 20.0
                ex = cx + rec.ax / 20.0
                ey = cy + rec.ay / 20.0
                # 8-step subdivision
                for t_i in range(1, 9):
                    t = t_i / 8.0
                    inv = 1.0 - t
                    px = inv * inv * cur_x + 2 * inv * t * cx + t * t * ex
                    py = inv * inv * cur_y + 2 * inv * t * cy + t * t * ey
                    points.append((px, py))
                cur_x, cur_y = ex, ey
            elif isinstance(rec, EndShape):
                break

        return points

    def cleanup(self) -> None:
        """Release GL resources."""
        self.jobs.close()
        self.close_preview()
        if self.library_panel:
            self.library_panel.close()
        if self.stage_panel:
            self.stage_panel.close()
        if self.renderer:
            self.renderer.cleanup()

    @staticmethod
    def _gl_transform(transform):
        matrix = transform.to_matrix()
        return glm.mat4(matrix.scale_x, matrix.rotate_skew_0, 0, 0,
                        matrix.rotate_skew_1, matrix.scale_y, 0, 0,
                        0, 0, 1, 0, matrix.translate_x / 20, matrix.translate_y / 20, 0, 1)
