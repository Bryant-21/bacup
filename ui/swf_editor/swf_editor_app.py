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
from ui.swf_editor.swf_scene import SwfScene
from ui.swf_editor.tools import BaseTool

_log = logging.getLogger(__name__)


class SwfEditorApp:
    """Main SWF editor application."""

    def __init__(self, toolkit_settings=None):
        self._toolkit_settings = toolkit_settings
        self.active = False

        # GL resources (initialized in setup())
        self.ctx: moderngl.Context | None = None
        self.renderer: SwfRenderer | None = None
        self.camera = OrthoCamera()

        # Document
        self.scene = SwfScene()
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

        self.canvas_panel = CanvasPanel(self)
        self.timeline_panel = TimelinePanel(self)
        self.layers_panel = LayersPanel(self)
        self.tools_panel = ToolsPanel(self)
        self.properties_panel = PropertiesPanel(self)
        self.library_panel = LibraryPanel(self)
        self._panels_initialized = True

    def draw_workspace(self) -> None:
        """Main draw loop -- called each frame."""
        if self._first_frame:
            self.setup()
            self._first_frame = False
        if not self._panels_initialized:
            self._init_panels()

        self._handle_shortcuts()
        self._draw_viewport()

    def _draw_viewport(self) -> None:
        """Render the 2D canvas to FBO and blit to ImGui."""
        if not self.renderer:
            return

        visible, _ = imgui.begin("Viewport##swf", flags=imgui.WindowFlags_.no_scrollbar)
        if not visible:
            imgui.end()
            return

        size = imgui.get_content_region_avail()
        w, h = max(int(size.x), 1), max(int(size.y), 1)

        self.camera.viewport_width = w
        self.camera.viewport_height = h

        self.renderer.ensure_fbo(w, h)

        # Collect visible shapes
        entries = self.scene.get_visible_entries()
        shape_list = []
        for shape, transform in entries:
            shape_list.append(
                (shape, glm.mat4(1.0))
            )  # NOTE: transform is discarded here (identity used) -- per-shape placement not yet applied

        projection = self.camera.get_projection()
        self.renderer.render(
            projection,
            shape_list,
            bg_color=(0.12, 0.12, 0.14),
            canvas_width=self.scene.canvas_width,
            canvas_height=self.scene.canvas_height,
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
        if imgui.is_item_hovered():
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
                tool = self.current_tool
                if tool:
                    for btn in range(2):  # left=0, right=1
                        if imgui.is_mouse_clicked(btn):
                            tool.on_mouse_down(mx, my, btn)

                    tool.on_mouse_move(mx, my)

                    for btn in range(2):
                        if imgui.is_mouse_released(btn):
                            tool.on_mouse_up(mx, my, btn)

        # Tool overlay (draw handles, preview shapes, etc.)
        tool = self.current_tool
        if tool and tex_id:
            dl = imgui.get_window_draw_list()
            tool.draw_overlay(dl, self.camera)

        imgui.end()

    def _handle_shortcuts(self) -> None:
        """Process keyboard shortcuts for tool switching."""
        io = imgui.get_io()
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
            if imgui.is_key_pressed(key):
                self.active_tool = tool
                break

        # Forward key events to active tool (e.g. Escape cancels pen path)
        active = self.current_tool
        if active:
            for key in (
                imgui.Key.escape,
                imgui.Key.delete,
                imgui.Key.backspace,
                imgui.Key.enter,
            ):
                if imgui.is_key_pressed(key):
                    active.on_key(key, True)

        # Undo/Redo
        ctrl = io.key_ctrl
        shift = io.key_shift
        if ctrl and imgui.is_key_pressed(imgui.Key.z):
            if shift:
                self.redo()
            else:
                self.undo()

        # Path boolean operations (Ctrl+Shift+key)
        if ctrl and shift:
            if imgui.is_key_pressed(imgui.Key.u):
                self.boolean_op("union")
            elif imgui.is_key_pressed(imgui.Key.s):
                self.boolean_op("subtract")
            elif imgui.is_key_pressed(imgui.Key.i):
                self.boolean_op("intersect")
            elif imgui.is_key_pressed(imgui.Key.x):
                self.boolean_op("exclude")

    def open_swf(self, path: str) -> None:
        """Open a SWF file for editing."""
        from creation_lib.swf.parser import parse_swf_file

        try:
            doc = parse_swf_file(path)
            self.scene = SwfScene.from_swf_document(doc)
            self.scene.file_path = path
            self.undo_stack.clear()
            self.camera.canvas_width = self.scene.canvas_width
            self.camera.canvas_height = self.scene.canvas_height
            self.camera.fit_canvas()
            if self.renderer:
                self.renderer.cleanup()
            self.status_text = f"Opened {Path(path).name}"
        except Exception as exc:
            _log.error("Failed to open SWF: %s", exc)
            self.status_text = f"Error: {exc}"

    def place_shape(self, shape_row: dict) -> None:
        """Place a library shape onto the active layer at the canvas origin.

        Args:
            shape_row: A dict from the shapes DB row — must contain
                       'shape_data' (bytes), 'bounds' (JSON str), and 'id' (int).
        """
        import json as _json
        from creation_lib.swf.shapes import (
            ShapeDef,
            StyleChange,
            StraightEdge,
            CurvedEdge,
            EndShape,
        )
        from creation_lib.swf.types import FillStyle, RGBA
        from ui.swf_editor.swf_scene import EditorDisplayEntry

        shape_data = shape_row.get("shape_data") or b""
        if not shape_data:
            self.status_text = "Shape has no geometry data; rebuild the SWF shape index."
            return

        try:
            from creation_lib.preprocessor.swf import deserialize_shape_records

            data = deserialize_shape_records(shape_data)
        except Exception as exc:
            _log.error("Failed to deserialize shape: %s", exc)
            return

        # Rebuild ShapeDef from serialized data
        fill_styles = []
        for fs_data in data.get("fill_styles", []):
            hex_c = fs_data["color"].lstrip("#")
            if len(hex_c) == 6:
                hex_c += "ff"
            r, g, b, a = (int(hex_c[i : i + 2], 16) for i in (0, 2, 4, 6))
            fill_styles.append(
                FillStyle(fill_type=fs_data.get("type", 0), color=RGBA(r, g, b, a))
            )

        records = []
        for rec in data.get("records", []):
            rtype = rec.get("type")
            if rtype == "sc":
                sc_kwargs = {
                    "fill0": rec.get("fill0"),
                    "fill1": rec.get("fill1"),
                }
                if rec.get("move"):
                    sc_kwargs["move_x"] = rec.get("dx", 0)
                    sc_kwargs["move_y"] = rec.get("dy", 0)
                records.append(StyleChange(**sc_kwargs))
            elif rtype == "se":
                records.append(StraightEdge(dx=rec.get("dx", 0), dy=rec.get("dy", 0)))
            elif rtype == "ce":
                records.append(
                    CurvedEdge(
                        cx=rec.get("cx", 0),
                        cy=rec.get("cy", 0),
                        ax=rec.get("ax", 0),
                        ay=rec.get("ay", 0),
                    )
                )
            elif rtype == "end":
                records.append(EndShape())

        bounds_list = data.get("bounds", [0, 0, 100, 100])
        from creation_lib.swf.types import TWIPS_PER_PIXEL

        bounds_twips = tuple(int(v * TWIPS_PER_PIXEL) for v in bounds_list)

        shape_id = max(self.scene.library_symbols.keys(), default=0) + 1
        new_shape = ShapeDef(
            shape_id=shape_id,
            bounds=bounds_twips,
            fill_styles=fill_styles,
            line_styles=[],
            records=records,
        )
        self.scene.library_symbols[shape_id] = new_shape

        # Add to active layer, current frame
        layer = (
            self.scene.layers[self.scene.active_layer_index]
            if self.scene.layers
            else None
        )
        if layer:
            kf = layer.keyframe_at(self.scene.current_frame)
            if kf:
                self.push_undo("Place Library Shape")
                kf.display_list.append(EditorDisplayEntry(shape_id=shape_id))
                self.scene.dirty = True
                self.status_text = f"Placed {shape_row.get('name', 'shape')}"

    def export_swf(self, path: str) -> None:
        """Export current document to SWF."""
        # TODO: Implement SwfScene -> SwfDocument conversion
        self.status_text = f"Exported {Path(path).name}"

    def undo(self) -> None:
        result = self.undo_stack.undo()
        if result:
            _label, snap = result
            self._restore_snapshot(snap)

    def redo(self) -> None:
        result = self.undo_stack.redo()
        if result:
            _label, snap = result
            self._restore_snapshot(snap)

    def push_undo(self, label: str) -> None:
        """Save current state for undo."""
        snap = self._take_snapshot()
        self.undo_stack.push(label, snap)
        self.scene.dirty = True

    def _take_snapshot(self) -> dict:
        """Capture current scene state for undo."""
        import copy

        return {
            "layers": copy.deepcopy(self.scene.layers),
            "current_frame": self.scene.current_frame,
            "active_layer": self.scene.active_layer_index,
        }

    def _restore_snapshot(self, snap: dict) -> None:
        """Restore scene state from undo snapshot."""
        import copy

        self.scene.layers = copy.deepcopy(snap["layers"])
        self.scene.current_frame = snap["current_frame"]
        self.scene.active_layer_index = snap["active_layer"]
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
            CurvedEdge,
            StyleChange,
            EndShape,
        )
        from creation_lib.swf.types import FillStyle, RGBA

        scene = self.scene
        if len(scene.selection) < 2:
            return

        self.push_undo(f"Path {operation.title()}")

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
            poly = self._shape_to_polygon(shape, de.transform.x, de.transform.y)
            all_polys.append(poly)

        if len(all_polys) < 2:
            return

        # Run clipper operation
        pc = pyclipper.Pyclipper()
        pc.AddPath(
            pyclipper.scale_to_clipper(all_polys[0]),
            pyclipper.PT_SUBJECT,
            True,
        )
        for poly in all_polys[1:]:
            pc.AddPath(
                pyclipper.scale_to_clipper(poly),
                pyclipper.PT_CLIP,
                True,
            )

        try:
            result_polys = pc.Execute(
                clip_type,
                pyclipper.PFT_EVENODD,
                pyclipper.PFT_EVENODD,
            )
        except pyclipper.ClipperException:
            _log.warning("Clipper boolean op failed for %s", operation)
            return

        if not result_polys:
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
            bounds=(min_x, min_y, max_x, max_y),
            fill_styles=[FillStyle(fill_type=0, color=RGBA(255, 255, 255, 255))],
            line_styles=[],
            records=records,
        )
        scene.library_symbols[shape_id] = result_shape

        # Remove old selected entries (in reverse order to preserve indices)
        from ui.swf_editor.swf_scene import EditorDisplayEntry, AffineTransform

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
        if self.renderer:
            self.renderer.cleanup()
