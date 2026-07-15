"""Bone tree panel — bone list with selection and vertex count."""
from __future__ import annotations

from typing import TYPE_CHECKING

import numpy as np
from imgui_bundle import imgui

if TYPE_CHECKING:
    from ..weight_painter_app import WeightPainterApp


class BoneTreePanel:
    """Bone hierarchy tree with weight influence counts and selection."""

    def __init__(self, app: WeightPainterApp):
        self.app = app
        self._filter_text = ""
        self._vert_counts: list[int] | None = None  # Cached per-bone vertex counts
        self._vert_counts_dirty = True
        self._children_map: dict[int, list[int]] = {}  # parent_idx -> [child_indices]
        self._root_bones: list[int] = []
        self._hierarchy_built = False

    def _build_hierarchy(self):
        """Build parent-child maps from bone_parents."""
        sd = self.app.skin_data
        if sd is None:
            return

        self._children_map.clear()
        self._root_bones.clear()

        if sd.bone_parents and len(sd.bone_parents) == len(sd.bone_names):
            for i, parent in enumerate(sd.bone_parents):
                if parent < 0:
                    self._root_bones.append(i)
                else:
                    self._children_map.setdefault(parent, []).append(i)
        else:
            # No hierarchy — all bones are roots (flat list)
            self._root_bones = list(range(len(sd.bone_names)))

        self._hierarchy_built = True

    def _compute_vert_counts(self):
        """Compute per-bone vertex influence counts."""
        sd = self.app.skin_data
        if sd is None:
            self._vert_counts = None
            return
        counts = []
        for i in range(len(sd.bone_names)):
            mask = (sd.bone_indices == i) & (sd.weights > 0)
            counts.append(int(mask.any(axis=1).sum()))
        self._vert_counts = counts
        self._vert_counts_dirty = False

    def invalidate(self):
        """Call when bone data changes (e.g. after weight transfer)."""
        self._vert_counts_dirty = True
        self._hierarchy_built = False

    def draw(self):
        visible, _ = imgui.begin("Bones##weight_painter")
        if not visible:
            imgui.end()
            return

        if self.app.skin_data is None:
            imgui.text_disabled("No mesh loaded")
            imgui.end()
            return

        sd = self.app.skin_data

        if not self._hierarchy_built:
            self._build_hierarchy()
        if self._vert_counts_dirty or self._vert_counts is None:
            self._compute_vert_counts()

        # Display mode toggle: single bone vs all bones
        is_all = self.app.display_mode == "all_weights"
        if imgui.button("All Bones" if not is_all else "Single Bone",
                        imgui.ImVec2(-1, 0)):
            if is_all:
                self.app.set_display_mode("weights")
            else:
                self.app.set_display_mode("all_weights")
        if is_all:
            imgui.text_disabled("Showing all bone influences")

        imgui.spacing()

        # Search filter
        _, self._filter_text = imgui.input_text(
            "Filter##bone_filter_wp", self._filter_text, 256,
        )

        imgui.separator()

        filter_lower = self._filter_text.strip().lower()

        # Scrollable bone list
        imgui.begin_child("##bone_list_scroll_wp", imgui.ImVec2(0, -30))

        if filter_lower:
            # When filtering, show flat list of matches
            self._draw_flat_list(sd, filter_lower)
        else:
            # Show hierarchical tree
            for root_idx in self._root_bones:
                self._draw_bone_tree_node(sd, root_idx)

        imgui.end_child()

        # --- Copy/Paste/Swap buttons ---
        has_sel = self.app.selected_bone_idx >= 0
        has_clip = self.app._copied_weights is not None

        if not has_sel:
            imgui.begin_disabled()
        if imgui.button("Copy##bone_copy", imgui.ImVec2(0, 0)):
            self.app.copy_bone_weights()
        imgui.set_item_tooltip("Copy weights from selected bone (Ctrl+C)")
        if not has_sel:
            imgui.end_disabled()

        imgui.same_line()

        if not (has_sel and has_clip):
            imgui.begin_disabled()
        if imgui.button("Paste##bone_paste", imgui.ImVec2(0, 0)):
            self.app.paste_bone_weights()
        if has_clip:
            imgui.set_item_tooltip(
                f"Paste weights from {self.app._copied_bone_name} (Ctrl+V)"
            )
        else:
            imgui.set_item_tooltip("Copy a bone's weights first")
        if not (has_sel and has_clip):
            imgui.end_disabled()

        imgui.same_line()

        can_swap = (
            has_sel and has_clip
            and self.app._copied_bone_idx != self.app.selected_bone_idx
        )
        if not can_swap:
            imgui.begin_disabled()
        if imgui.button("Swap##bone_swap", imgui.ImVec2(0, 0)):
            self.app.swap_bone_weights()
        if has_clip:
            imgui.set_item_tooltip(
                f"Swap weights: {self.app._copied_bone_name} <-> selected"
            )
        else:
            imgui.set_item_tooltip("Copy a bone's weights first")
        if not can_swap:
            imgui.end_disabled()

        # Status bar
        imgui.separator()
        imgui.text(f"{len(sd.bone_names)} bones")
        if self.app.selected_bone_name:
            imgui.same_line()
            imgui.text(f"| Selected: {self.app.selected_bone_name}")

        imgui.end()

    # ------------------------------------------------------------------
    # Tree drawing helpers
    # ------------------------------------------------------------------

    def _draw_bone_tree_node(self, sd, bone_idx: int):
        """Recursively draw a bone as a tree node with its children."""
        name = sd.bone_names[bone_idx]
        vert_count = self._vert_counts[bone_idx] if self._vert_counts else 0
        selected = (bone_idx == self.app.selected_bone_idx)
        children = self._children_map.get(bone_idx, [])
        is_leaf = len(children) == 0

        # Tree node flags
        flags = (imgui.TreeNodeFlags_.open_on_arrow
                 | imgui.TreeNodeFlags_.span_avail_width)
        if is_leaf:
            flags |= (imgui.TreeNodeFlags_.leaf
                       | imgui.TreeNodeFlags_.no_tree_push_on_open)
        if selected:
            flags |= imgui.TreeNodeFlags_.selected
        # Auto-open root bones
        if (sd.bone_parents
                and bone_idx < len(sd.bone_parents)
                and sd.bone_parents[bone_idx] < 0):
            imgui.set_next_item_open(True, imgui.Cond_.once)

        if selected:
            imgui.push_style_color(
                imgui.Col_.text, imgui.ImVec4(0.2, 1.0, 0.4, 1.0),
            )

        label = f"{name} ({vert_count})##bone_{bone_idx}"
        opened = imgui.tree_node_ex(label, flags)

        if selected:
            imgui.pop_style_color()

        # Handle click
        if imgui.is_item_clicked(0):
            self.app.select_bone(bone_idx)
            if self.app.display_mode == "all_weights":
                self.app.set_display_mode("weights")

        if opened and not is_leaf:
            for child_idx in children:
                self._draw_bone_tree_node(sd, child_idx)
            imgui.tree_pop()

    def _draw_flat_list(self, sd, filter_lower: str):
        """Draw a flat filtered bone list (used when search filter is active)."""
        for i, name in enumerate(sd.bone_names):
            if filter_lower not in name.lower():
                continue

            vert_count = self._vert_counts[i] if self._vert_counts else 0
            selected = (i == self.app.selected_bone_idx)

            if selected:
                imgui.push_style_color(
                    imgui.Col_.text, imgui.ImVec4(0.2, 1.0, 0.4, 1.0),
                )

            label = f"{name} ({vert_count})"
            if imgui.selectable(f"{label}##bone_{i}", selected)[0]:
                self.app.select_bone(i)
                if self.app.display_mode == "all_weights":
                    self.app.set_display_mode("weights")

            if selected:
                imgui.pop_style_color()
