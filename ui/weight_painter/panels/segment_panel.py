"""Segment panel — segment tree (FO4) / flat segment list (Skyrim) and assignment tools."""
from __future__ import annotations

from typing import TYPE_CHECKING

import numpy as np
from imgui_bundle import imgui

from creation_lib.skinning.partitions import BODY_PART_IDS, generate_ssf_path
from creation_lib.skinning.skin_data import SegmentInfo, SubSegmentInfo

if TYPE_CHECKING:
    from ..weight_painter_app import WeightPainterApp

# Reverse map: part ID -> name
_PART_ID_TO_NAME: dict[int, str] = {}
for _name, _pid in BODY_PART_IDS.items():
    if _pid not in _PART_ID_TO_NAME:
        _PART_ID_TO_NAME[_pid] = _name

# Sorted list for body part combo box
_BODY_PART_ITEMS: list[tuple[str, int]] = sorted(BODY_PART_IDS.items(), key=lambda x: x[1])

SEGMENT_COLORS: dict[int, tuple[float, float, float]] = {
    30: (0.91, 0.30, 0.24),   # Body - red
    32: (0.20, 0.60, 0.86),   # Head - blue
    33: (0.18, 0.80, 0.44),   # Hair - green
    34: (0.95, 0.61, 0.07),   # L Arm - orange
    35: (0.61, 0.35, 0.71),   # R Arm - purple
    36: (0.84, 0.50, 0.40),   # Hands - brown
    37: (0.10, 0.74, 0.61),   # L Leg - teal
    38: (0.94, 0.76, 0.06),   # R Leg - gold
    39: (0.55, 0.55, 0.55),   # Feet - gray
    41: (1.00, 0.20, 0.20),   # Weapon - bright red
}


def get_segment_color(seg_id: int) -> tuple[float, float, float]:
    """Get a color for any segment ID. Uses golden-ratio HSV for distinct colors."""
    if seg_id < 0:
        return (0.4, 0.1, 0.1)  # Unassigned: dark red
    # Golden ratio hue stepping for distinct colors
    import colorsys
    hue = (seg_id * 0.618033988749895) % 1.0
    r, g, b = colorsys.hsv_to_rgb(hue, 0.7, 0.85)
    return (r, g, b)


def _color_swatch(seg_id: int):
    """Draw a small color swatch for a segment."""
    color = get_segment_color(seg_id)
    dl = imgui.get_window_draw_list()
    pos = imgui.get_cursor_screen_pos()
    dl.add_rect_filled(
        pos,
        imgui.ImVec2(pos.x + 10, pos.y + 10),
        imgui.get_color_u32(imgui.ImVec4(*color, 1.0)),
    )
    imgui.dummy(imgui.ImVec2(14, 10))
    imgui.same_line()


def _part_name(part_id: int) -> str:
    """Resolve a body part ID to its human-readable name."""
    return _PART_ID_TO_NAME.get(part_id, f"Part {part_id}")


class SegmentPanel:
    """Segment tree panel with FO4 hierarchy support."""

    def __init__(self, app: WeightPainterApp):
        self.app = app
        # Selection state: (segment_index, sub_segment_index) or (-1, -1)
        self._selected_seg: int = -1
        self._selected_subseg: int = -1
        # Context menu target
        self._ctx_seg: int = -1
        self._ctx_subseg: int = -1

    def draw(self):
        visible, _ = imgui.begin("Segments##weight_painter")
        if not visible:
            imgui.end()
            return

        if self.app.skin_data is None:
            imgui.text_disabled("No mesh loaded")
            imgui.end()
            return

        sd = self.app.skin_data
        has_segments = len(sd.segments) > 0

        # Header indicating format
        if has_segments:
            imgui.text_disabled(f"FO4 Segments ({len(sd.segments)})")
            # SSF path: editable field
            self._draw_ssf_field(sd)
        else:
            imgui.text_disabled("Flat Segments (Skyrim)")

        imgui.separator()

        # Scrollable tree area
        imgui.begin_child("##segment_tree_scroll", imgui.ImVec2(0, -120))

        if has_segments:
            self._draw_segment_tree(sd)
        else:
            self._draw_flat_segments(sd)

        imgui.end_child()

        imgui.separator()

        # Summary
        self._draw_summary(sd, has_segments)

        imgui.separator()

        # Action buttons
        self._draw_actions(sd, has_segments)

        imgui.end()

    # ------------------------------------------------------------------
    # SSF path field
    # ------------------------------------------------------------------

    def _draw_ssf_field(self, sd):
        """Draw editable SSF file path with auto-generate button."""
        imgui.set_next_item_width(-80)
        changed, new_val = imgui.input_text(
            "##ssf_path", sd.ssf_file, imgui.InputTextFlags_.enter_returns_true,
        )
        if changed:
            sd.ssf_file = new_val
            self.app.modified = True

        imgui.same_line()
        if imgui.button("Auto##ssf_auto"):
            if self.app.file_path:
                sd.ssf_file = generate_ssf_path(self.app.file_path)
                self.app.modified = True
        if imgui.is_item_hovered():
            imgui.set_tooltip("Generate SSF path from NIF filename")

    # ------------------------------------------------------------------
    # FO4 hierarchical segment tree
    # ------------------------------------------------------------------

    def _draw_segment_tree(self, sd):
        """Draw FO4 two-level segment tree: Segment -> Sub-Segments."""
        for seg_idx, seg in enumerate(sd.segments):
            is_seg_selected = (
                self._selected_seg == seg_idx and self._selected_subseg == -1
            )

            # Tree node flags
            flags = (
                imgui.TreeNodeFlags_.open_on_arrow
                | imgui.TreeNodeFlags_.span_avail_width
            )
            if is_seg_selected:
                flags |= imgui.TreeNodeFlags_.selected
            if not seg.sub_segments:
                flags |= imgui.TreeNodeFlags_.leaf

            # Color swatch for segment
            _color_swatch(seg_idx)

            # Segment label
            sub_count = len(seg.sub_segments)
            label = f"Segment {seg_idx} ({seg.num_primitives} tris, {sub_count} sub)"
            node_open = imgui.tree_node_ex(
                f"{label}##seg_{seg_idx}", flags,
            )

            # Click to select segment
            if imgui.is_item_clicked(imgui.MouseButton_.left):
                self._select_segment(seg_idx, -1, sd)

            # Right-click context menu
            if imgui.begin_popup_context_item(f"##seg_ctx_{seg_idx}"):
                self._ctx_seg = seg_idx
                self._ctx_subseg = -1
                self._draw_segment_context_menu(sd)
                imgui.end_popup()

            if node_open:
                # Draw sub-segments
                for sseg_idx, sseg in enumerate(seg.sub_segments):
                    is_sseg_selected = (
                        self._selected_seg == seg_idx
                        and self._selected_subseg == sseg_idx
                    )

                    sseg_flags = (
                        imgui.TreeNodeFlags_.leaf
                        | imgui.TreeNodeFlags_.no_tree_push_on_open
                        | imgui.TreeNodeFlags_.span_avail_width
                    )
                    if is_sseg_selected:
                        sseg_flags |= imgui.TreeNodeFlags_.selected

                    _color_swatch(seg_idx)

                    part_name = _part_name(sseg.user_index)
                    bone_str = (
                        f"bone=0x{sseg.bone_id:08X}"
                        if sseg.bone_id != 0xFFFFFFFF
                        else ""
                    )
                    sseg_label = (
                        f"{part_name} [{sseg.user_index}] "
                        f"({sseg.num_primitives} tris)"
                    )
                    if bone_str:
                        sseg_label += f" {bone_str}"

                    imgui.tree_node_ex(
                        f"{sseg_label}##sseg_{seg_idx}_{sseg_idx}", sseg_flags,
                    )

                    if imgui.is_item_clicked(imgui.MouseButton_.left):
                        self._select_segment(seg_idx, sseg_idx, sd)

                    # Sub-segment context menu
                    if imgui.begin_popup_context_item(
                        f"##sseg_ctx_{seg_idx}_{sseg_idx}"
                    ):
                        self._ctx_seg = seg_idx
                        self._ctx_subseg = sseg_idx
                        self._draw_subsegment_context_menu(sd)
                        imgui.end_popup()

                imgui.tree_pop()

    def _select_segment(self, seg_idx: int, subseg_idx: int, sd):
        """Select a segment or sub-segment and highlight in viewport."""
        # Toggle off on re-click
        if self._selected_seg == seg_idx and self._selected_subseg == subseg_idx:
            self._selected_seg = -1
            self._selected_subseg = -1
            self.app.select_segment(-1)
            return

        self._selected_seg = seg_idx
        self._selected_subseg = subseg_idx

        # Highlight the segment index in the viewport
        if 0 <= seg_idx < len(sd.segments):
            self.app.select_segment(seg_idx)

    def _draw_segment_context_menu(self, sd):
        """Context menu for a segment node."""
        if imgui.menu_item("Add Sub-Segment", "", False)[0]:
            seg_idx = self._ctx_seg
            if 0 <= seg_idx < len(sd.segments):
                self.app.push_undo("Add Sub-Segment")
                new_sseg = SubSegmentInfo(
                    start_index=0,
                    num_primitives=0,
                    user_index=30,  # Default to "Body"
                )
                sd.segments[seg_idx].sub_segments.append(new_sseg)
                self.app.modified = True
                self.app._rebuild_segment_colors()

        if imgui.menu_item("Delete Segment", "", False)[0]:
            seg_idx = self._ctx_seg
            if 0 <= seg_idx < len(sd.segments):
                self.app.push_undo("Delete Segment")
                sd.segments.pop(seg_idx)
                if sd.segment_ids is not None:
                    sd.segment_ids[sd.segment_ids == seg_idx] = -1
                    sd.segment_ids[sd.segment_ids > seg_idx] -= 1
                if self._selected_seg == seg_idx:
                    self._selected_seg = -1
                    self._selected_subseg = -1
                    self.app.select_segment(-1)
                elif self._selected_seg > seg_idx:
                    self._selected_seg -= 1
                self.app.modified = True
                self.app._rebuild_segment_submeshes()

        imgui.separator()

        # Set body part type submenu
        if imgui.begin_menu("Set Body Part"):
            for name, pid in _BODY_PART_ITEMS:
                if imgui.menu_item(f"{name} ({pid})", "", False)[0]:
                    seg_idx = self._ctx_seg
                    if 0 <= seg_idx < len(sd.segments):
                        self.app.push_undo("Set Body Part")
                        sd.segments[seg_idx].user_index = pid
                        for sseg in sd.segments[seg_idx].sub_segments:
                            sseg.user_index = pid
                        self.app.modified = True
                        self.app._rebuild_segment_colors()
            imgui.end_menu()

    def _draw_subsegment_context_menu(self, sd):
        """Context menu for a sub-segment node."""
        if imgui.menu_item("Delete Sub-Segment", "", False)[0]:
            seg_idx = self._ctx_seg
            sseg_idx = self._ctx_subseg
            if 0 <= seg_idx < len(sd.segments):
                subs = sd.segments[seg_idx].sub_segments
                if 0 <= sseg_idx < len(subs):
                    self.app.push_undo("Delete Sub-Segment")
                    subs.pop(sseg_idx)
                    if (
                        self._selected_seg == seg_idx
                        and self._selected_subseg == sseg_idx
                    ):
                        self._selected_subseg = -1
                        self.app.select_segment(-1)
                    elif (
                        self._selected_seg == seg_idx
                        and self._selected_subseg > sseg_idx
                    ):
                        self._selected_subseg -= 1
                    self.app.modified = True
                    self.app._rebuild_segment_colors()

        imgui.separator()

        if imgui.begin_menu("Set Body Part"):
            for name, pid in _BODY_PART_ITEMS:
                if imgui.menu_item(f"{name} ({pid})", "", False)[0]:
                    seg_idx = self._ctx_seg
                    sseg_idx = self._ctx_subseg
                    if 0 <= seg_idx < len(sd.segments):
                        subs = sd.segments[seg_idx].sub_segments
                        if 0 <= sseg_idx < len(subs):
                            self.app.push_undo("Set Sub-Segment Body Part")
                            subs[sseg_idx].user_index = pid
                            self.app.modified = True
                            self.app._rebuild_segment_colors()
            imgui.end_menu()

    # ------------------------------------------------------------------
    # Skyrim flat segment list (backward compat)
    # ------------------------------------------------------------------

    def _draw_flat_segments(self, sd):
        """Draw flat segment list for Skyrim-style meshes."""
        seg_ids = sd.segment_ids
        unique_segs = sorted(set(seg_ids.tolist()) - {-1})
        sel_id = self.app.selected_segment_id

        for seg_id in unique_segs:
            tri_count = int((seg_ids == seg_id).sum())
            is_selected = (seg_id == sel_id)

            _color_swatch(seg_id)

            label = f"Segment {seg_id} ({tri_count} tris)"
            if imgui.selectable(f"{label}##seg_{seg_id}", is_selected)[0]:
                if is_selected:
                    self.app.select_segment(-1)
                else:
                    self.app.select_segment(seg_id)

        unassigned = int((seg_ids == -1).sum())
        if unassigned > 0:
            imgui.text_colored(
                imgui.ImVec4(1, 0.3, 0.3, 1),
                f"Unassigned: {unassigned} tris",
            )

        if not unique_segs and unassigned == 0:
            imgui.text_disabled("No segment data")

    # ------------------------------------------------------------------
    # Summary and action buttons
    # ------------------------------------------------------------------

    def _draw_summary(self, sd, has_segments: bool):
        """Draw summary line."""
        total_tris = sd.num_triangles
        if has_segments:
            seg_tris = sum(s.num_primitives for s in sd.segments)
            imgui.text(
                f"Total: {total_tris} tris | "
                f"Segments: {len(sd.segments)} ({seg_tris} assigned)"
            )
        else:
            seg_ids = sd.segment_ids
            unassigned = int((seg_ids == -1).sum())
            assigned = total_tris - unassigned
            imgui.text(f"Total: {total_tris} tris, {assigned} assigned")

    def _draw_actions(self, sd, has_segments: bool):
        """Draw action buttons at the bottom of the panel."""
        has_ref = self.app.reference_skin is not None
        has_bones = len(sd.bone_names) > 0

        # Add segment button (FO4 mode)
        if has_segments:
            if imgui.button("Add Segment", imgui.ImVec2(-1, 0)):
                self.app.push_undo("Add Segment")
                new_seg = SegmentInfo(
                    start_index=0,
                    num_primitives=0,
                    sub_segments=[],
                )
                sd.segments.append(new_seg)
                self.app.modified = True
                self.app._rebuild_segment_colors()
            imgui.spacing()

        # Auto-assign buttons
        if not has_bones:
            imgui.begin_disabled()
        if imgui.button("Auto-assign from Bones", imgui.ImVec2(-1, 0)):
            self.app.auto_assign_segments("bones")
        if not has_bones:
            imgui.end_disabled()
            imgui.set_item_tooltip("Requires bone names in the mesh")

        if not has_ref:
            imgui.begin_disabled()
        if imgui.button("Auto-assign from Reference", imgui.ImVec2(-1, 0)):
            self.app.auto_assign_segments("reference")
        if not has_ref:
            imgui.end_disabled()
            imgui.set_item_tooltip("Requires a loaded reference body")

        imgui.spacing()

        # Rebuild segments from painted segment IDs
        has_assigned = sd.num_triangles > 0 and np.any(sd.segment_ids >= 0)
        if not has_assigned:
            imgui.begin_disabled()
        if imgui.button("Rebuild Segments from IDs", imgui.ImVec2(-1, 0)):
            self.app.push_undo("Rebuild Segments")
            from creation_lib.skinning.partitions import (
                rebuild_fo4_segments_from_body_parts,
                sync_fo4_segments_from_ids,
            )
            if sd.segments:
                sd.segments = sync_fo4_segments_from_ids(sd)
            else:
                sd.segments, sd.segment_ids = rebuild_fo4_segments_from_body_parts(
                    sd, sd.segment_ids,
                )
            self.app.modified = True
            self.app._rebuild_segment_submeshes()
        if not has_assigned:
            imgui.end_disabled()
        if imgui.is_item_hovered():
            imgui.set_tooltip(
                "Rebuild FO4 segment hierarchy from painted segment IDs.\n"
                "Use after auto-assign or segment painting."
            )

        imgui.spacing()
        imgui.separator()

        # Quick toggle to segment view mode
        is_segment_view = self.app.display_mode == "segments"
        if imgui.button(
            "Hide Segment View" if is_segment_view else "Show Segment View",
            imgui.ImVec2(-1, 0),
        ):
            if is_segment_view:
                self.app.selected_segment_id = -1
                self.app.set_display_mode("weights")
            else:
                self.app.set_display_mode("segments")
