"""Brush settings panel — brush type, radius, strength, falloff, mode, transfer."""
from __future__ import annotations

from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ..weight_painter_app import WeightPainterApp

_TRANSFER_METHODS = ["Barycentric", "Proximity", "Hybrid"]
_TRANSFER_METHOD_IDS = ["barycentric", "proximity", "hybrid"]


class BrushPanel:
    """Brush configuration panel for weight painting."""

    def __init__(self, app: WeightPainterApp):
        self.app = app

    def draw(self):
        visible, _ = imgui.begin("Brush##weight_painter")
        if not visible:
            imgui.end()
            return

        # Brush type selector
        brush_types = [
            "Paint", "Smooth", "Blur", "Gradient", "Mirror", "Flood",
            "Segment", "Mask", "Unmask",
        ]
        brush_ids = [
            "paint", "smooth", "blur", "gradient", "mirror", "flood",
            "segment", "mask", "unmask",
        ]

        imgui.text("Brush Type")
        for i, (label, bid) in enumerate(zip(brush_types, brush_ids)):
            if i > 0:
                imgui.same_line()
            selected = self.app.brush_type == bid
            if selected:
                imgui.push_style_color(
                    imgui.Col_.button, imgui.ImVec4(0.9, 0.3, 0.2, 1.0),
                )
            if imgui.button(f"{label}##brush_{bid}"):
                self.app.brush_type = bid
                # Reset gradient state when switching away
                if bid != "gradient":
                    self.app.gradient_pending = False
                    self.app.gradient_start = None
                # Auto-switch display mode for segment brush
                if bid == "segment":
                    self.app.set_display_mode("segments")
            imgui.set_item_tooltip(f"Shortcut: {i + 1}")
            if selected:
                imgui.pop_style_color()

        imgui.separator()

        # Brush settings — SLIDERS, not drags
        _, self.app.brush_radius = imgui.slider_float(
            "Radius", self.app.brush_radius, 0.1, 50.0,
        )
        _, self.app.brush_strength = imgui.slider_float(
            "Strength", self.app.brush_strength, 0.0, 1.0,
        )
        _, self.app.brush_falloff = imgui.slider_float(
            "Falloff", self.app.brush_falloff, 0.01, 1.0,
        )

        imgui.separator()

        # Paint mode (only relevant for paint brush)
        if self.app.brush_type == "paint":
            imgui.text("Mode")
            modes = ["Add", "Subtract", "Set"]
            mode_ids = ["add", "subtract", "set"]
            for i, (label, mid) in enumerate(zip(modes, mode_ids)):
                if i > 0:
                    imgui.same_line()
                selected = self.app.paint_mode == mid
                if selected:
                    imgui.push_style_color(
                        imgui.Col_.button, imgui.ImVec4(0.9, 0.3, 0.2, 1.0),
                    )
                if imgui.button(f"{label}##mode_{mid}"):
                    self.app.paint_mode = mid
                if selected:
                    imgui.pop_style_color()

            imgui.separator()

        # Segment brush target info
        if self.app.brush_type == "segment":
            from .segment_panel import _part_name, get_segment_color
            part_id = self.app.selected_segment_id
            if part_id >= 0:
                color = get_segment_color(part_id)
                imgui.text_colored(
                    imgui.ImVec4(*color, 1.0),
                    f"Target: {_part_name(part_id)} [{part_id}]",
                )
            else:
                imgui.text_colored(
                    imgui.ImVec4(1.0, 0.4, 0.4, 1.0),
                    "Select a segment in the Segments panel first",
                )
            imgui.separator()

        # Options
        _, self.app.auto_normalize = imgui.checkbox(
            "Auto-normalize", self.app.auto_normalize,
        )
        _, self.app.mirror_x = imgui.checkbox("Mirror X", self.app.mirror_x)
        _, self.app.show_wireframe = imgui.checkbox(
            "Wireframe", self.app.show_wireframe,
        )
        _, self.app.show_segment_edges = imgui.checkbox(
            "Segment Edges", self.app.show_segment_edges,
        )
        imgui.set_item_tooltip("Show segment boundaries over weight view (B)")
        _, self.app.show_mask = imgui.checkbox(
            "Show Mask", self.app.show_mask,
        )
        imgui.set_item_tooltip("Shortcut: M")

        # Mask action buttons
        has_mask = self.app.mask is not None
        if not has_mask:
            imgui.begin_disabled()
        if imgui.button("Clear Mask"):
            self.app.clear_mask()
        imgui.same_line()
        if imgui.button("Invert Mask"):
            self.app.invert_mask()
        if not has_mask:
            imgui.end_disabled()

        imgui.separator()

        # Undo/Redo buttons
        has_undo = len(self.app.undo_stack) > 0
        has_redo = len(self.app.redo_stack) > 0

        if not has_undo:
            imgui.begin_disabled()
        if imgui.button("Undo (Ctrl+Z)"):
            self.app.undo()
        if not has_undo:
            imgui.end_disabled()

        imgui.same_line()

        if not has_redo:
            imgui.begin_disabled()
        if imgui.button("Redo (Ctrl+Y)"):
            self.app.redo()
        if not has_redo:
            imgui.end_disabled()

        # Undo stack info
        imgui.text_disabled(
            f"Undo: {len(self.app.undo_stack)} | Redo: {len(self.app.redo_stack)}"
        )

        imgui.separator()

        # -----------------------------------------------------------
        # Transfer Weights section
        # -----------------------------------------------------------
        self._draw_transfer_section()

        imgui.end()

    def _draw_transfer_section(self):
        """Draw the Transfer Weights UI section."""
        imgui.spacing()
        imgui.text("Transfer Weights")

        has_mesh = self.app.skin_data is not None
        if not has_mesh:
            imgui.begin_disabled()

        if imgui.button("Transfer Weights...", imgui.ImVec2(-1, 0)):
            self.app._show_transfer_dialog = True
            self.app._transfer_stats = ""
            imgui.open_popup("Transfer Weights##popup")

        if not has_mesh:
            imgui.end_disabled()
            imgui.set_item_tooltip("Import a mesh first")

        # Transfer popup dialog
        self._draw_transfer_popup()

    def _draw_transfer_popup(self):
        """Draw the transfer weights modal popup."""
        center = imgui.get_main_viewport().get_center()
        imgui.set_next_window_pos(center, imgui.Cond_.appearing, imgui.ImVec2(0.5, 0.5))
        imgui.set_next_window_size(imgui.ImVec2(420, 0), imgui.Cond_.appearing)

        if not imgui.begin_popup_modal(
            "Transfer Weights##popup",
            flags=imgui.WindowFlags_.always_auto_resize,
        )[0]:
            return

        app = self.app

        # Source selection
        imgui.text("Source Mesh")
        imgui.separator()

        has_ref = app.reference_skin is not None
        ref_label = (
            f"Reference body ({app.reference_skin.num_vertices} verts, "
            f"{len(app.reference_skin.bone_names)} bones)"
            if has_ref
            else "No reference body loaded"
        )

        if imgui.radio_button("Use Reference Body", app._transfer_source == "reference"):
            app._transfer_source = "reference"
        if not has_ref:
            imgui.same_line()
            imgui.text_disabled("(not loaded)")

        if imgui.radio_button("Browse NIF File", app._transfer_source == "file"):
            app._transfer_source = "file"

        if app._transfer_source == "file":
            imgui.indent()
            imgui.set_next_item_width(-80)
            _, app._transfer_file_path = imgui.input_text(
                "##transfer_path", app._transfer_file_path,
            )
            imgui.same_line()
            if imgui.button("Browse##transfer"):
                # Use the file dialog infrastructure if available, else just show the input
                pass
            imgui.unindent()
        elif has_ref:
            imgui.indent()
            imgui.text_disabled(ref_label)
            imgui.unindent()

        imgui.spacing()

        # Transfer method
        imgui.text("Method")
        imgui.separator()
        for i, label in enumerate(_TRANSFER_METHODS):
            if imgui.radio_button(f"{label}##xfer_method", app._transfer_method == i):
                app._transfer_method = i
            if i < len(_TRANSFER_METHODS) - 1:
                imgui.same_line()

        # Method description
        descriptions = [
            "Projects target vertices onto source triangles for precise weight interpolation.",
            "Uses nearest source vertices weighted by inverse distance.",
            "Barycentric with proximity fallback for distant vertices (recommended).",
        ]
        imgui.text_wrapped(descriptions[app._transfer_method])

        imgui.spacing()

        # Options
        imgui.text("Options")
        imgui.separator()

        _, app._transfer_bone_filter = imgui.input_text(
            "Bone Filter", app._transfer_bone_filter,
        )
        imgui.set_item_tooltip(
            "Only transfer bones whose names contain this text (leave empty for all)"
        )

        _, app._transfer_segments = imgui.checkbox(
            "Transfer Segments", app._transfer_segments,
        )
        imgui.set_item_tooltip(
            "Re-assign segment IDs based on transferred bone weights"
        )

        # Vertex count preview
        if app.skin_data is not None:
            imgui.spacing()
            imgui.text_disabled(f"Target: {app.skin_data.num_vertices} vertices")
            if app._transfer_source == "reference" and has_ref:
                imgui.text_disabled(
                    f"Source: {app.reference_skin.num_vertices} vertices, "
                    f"{len(app.reference_skin.bone_names)} bones"
                )

        # Show last transfer stats if available
        if app._transfer_stats:
            imgui.spacing()
            imgui.text_colored(
                imgui.ImVec4(0.4, 0.9, 0.4, 1.0),
                f"Result: {app._transfer_stats}",
            )

        imgui.spacing()
        imgui.separator()

        # Action buttons
        can_transfer = app.skin_data is not None and (
            (app._transfer_source == "reference" and has_ref)
            or (app._transfer_source == "file" and app._transfer_file_path.strip())
        )

        if not can_transfer:
            imgui.begin_disabled()
        if imgui.button("Transfer", imgui.ImVec2(120, 0)):
            method = _TRANSFER_METHOD_IDS[app._transfer_method]
            source_path = (
                app._transfer_file_path.strip()
                if app._transfer_source == "file"
                else None
            )
            app.transfer_weights_from_mesh(
                source_path=source_path,
                method=method,
                bone_filter=app._transfer_bone_filter,
                transfer_segments=app._transfer_segments,
            )
        if not can_transfer:
            imgui.end_disabled()

        imgui.same_line()
        if imgui.button("Close", imgui.ImVec2(120, 0)):
            app._show_transfer_dialog = False
            imgui.close_current_popup()

        imgui.end_popup()
