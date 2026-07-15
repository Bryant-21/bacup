"""Bone panel — flat searchable list with category labels and active deltas."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from imgui_bundle import imgui

from creation_lib.bone_edit.bone_classifier import BoneCategory

if TYPE_CHECKING:
    from ..bone_editor_app import BoneEditorApp

_log = logging.getLogger("bone_editor.bone_panel")


_CAT_LABELS = {
    BoneCategory.LIMB_SEGMENT: "LIMB",
    BoneCategory.IK_TIP: "IK_TIP",
    BoneCategory.IK_POLE: "IK_POLE",
    BoneCategory.MOUNT: "MOUNT",
}

_CAT_COLORS = {
    BoneCategory.LIMB_SEGMENT: imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
    BoneCategory.IK_TIP: imgui.ImVec4(0.2, 0.8, 1.0, 1.0),
    BoneCategory.IK_POLE: imgui.ImVec4(1.0, 0.3, 0.95, 1.0),
    BoneCategory.MOUNT: imgui.ImVec4(0.95, 0.85, 0.2, 1.0),
}


class BonePanel:
    def __init__(self, app: "BoneEditorApp"):
        self.app = app
        self._search = ""
        self._edited_only = False
        # Per-category visibility filters. Drives both the panel list and
        # the viewport overlay (viewport_panel reads compute_extra_hidden /
        # show_pole_handles when calling SkeletonDisplay.rebuild).
        self._show_categories: dict[BoneCategory, bool] = {
            BoneCategory.LIMB_SEGMENT: True,
            BoneCategory.IK_TIP: True,
            BoneCategory.IK_POLE: True,
            BoneCategory.MOUNT: True,
        }
        self._show_pole_handles = True

    # ── Public API consumed by viewport_panel ──────────────────────────
    @property
    def show_pole_handles(self) -> bool:
        return self._show_pole_handles

    def compute_extra_hidden(self) -> set[str]:
        """Bones to hide on top of `app.classifier_hidden` due to category filters."""
        sess = self.app.pose_session
        if sess is None or self.app.skeleton is None:
            return set()
        hidden_cats = {
            cat for cat, shown in self._show_categories.items() if not shown
        }
        if not hidden_cats:
            return set()
        return {
            name for name in self.app.skeleton.bone_names
            if sess.categories.get(name, BoneCategory.LIMB_SEGMENT) in hidden_cats
        }

    def draw(self) -> None:
        visible, _ = imgui.begin("Bones##bone_editor")
        if not visible:
            imgui.end()
            return

        if self.app.pose_session is None or self.app.skeleton is None:
            imgui.text_colored(imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                               "Load a skeleton first")
            imgui.end()
            return

        _, self._search = imgui.input_text("Search", self._search, 64)
        imgui.same_line()
        _, self._edited_only = imgui.checkbox("Edited only", self._edited_only)

        # Category filters — affect both this panel and the viewport overlay
        self._draw_category_filters()

        imgui.separator()

        sess = self.app.pose_session
        edited = sess.pose.edited_bones()
        hidden = self.app.classifier_hidden if hasattr(self.app, "classifier_hidden") else set()
        extra_hidden = self.compute_extra_hidden()

        # List
        search_lc = self._search.strip().lower()
        if imgui.begin_child("##bone_list", imgui.ImVec2(0, -110)):
            for name in self.app.skeleton.bone_names:
                if name in hidden or name in extra_hidden:
                    continue
                if search_lc and search_lc not in name.lower():
                    continue
                if self._edited_only and name not in edited:
                    continue
                self._draw_row(name, sess, edited)
            imgui.end_child()

        imgui.separator()
        self._draw_selected_details(sess)

        imgui.end()

    def _draw_category_filters(self) -> None:
        """5 checkboxes — LIMB / IK_TIP / IK_POLE / MOUNT / POLE.

        POLE is the orange pole-handle visualisation (a non-bone marker
        keyed by mid bone name); the other four map directly to BoneCategory.
        """
        rows = [
            (BoneCategory.LIMB_SEGMENT, "LIMB"),
            (BoneCategory.IK_TIP, "IK_TIP"),
            (BoneCategory.IK_POLE, "IK_POLE"),
            (BoneCategory.MOUNT, "MOUNT"),
        ]
        for i, (cat, label) in enumerate(rows):
            if i > 0:
                imgui.same_line()
            imgui.push_style_color(imgui.Col_.text, _CAT_COLORS[cat])
            _, self._show_categories[cat] = imgui.checkbox(
                label, self._show_categories[cat],
            )
            imgui.pop_style_color()
        imgui.same_line()
        imgui.push_style_color(
            imgui.Col_.text, imgui.ImVec4(1.0, 0.55, 0.10, 1.0),
        )
        _, self._show_pole_handles = imgui.checkbox(
            "POLE", self._show_pole_handles,
        )
        imgui.pop_style_color()

    def _draw_row(self, name, sess, edited) -> None:
        cat = sess.categories.get(name, BoneCategory.LIMB_SEGMENT)
        is_selected = (self.app.viewport_interact is not None
                       and self.app.viewport_interact.selected_bone == name)
        marker = " *" if name in edited else ""
        label = f"{name}  [{_CAT_LABELS[cat]}]{marker}"

        imgui.push_style_color(imgui.Col_.text, _CAT_COLORS[cat])
        clicked, _ = imgui.selectable(label, is_selected)
        imgui.pop_style_color()

        if clicked and self.app.viewport_interact is not None:
            self.app.viewport_interact.select_bone(name)

        # Right-click context menu for category override
        if imgui.begin_popup_context_item(f"##ctx_{name}"):
            imgui.text("Set type:")
            imgui.separator()
            for new_cat in (BoneCategory.LIMB_SEGMENT, BoneCategory.IK_TIP,
                            BoneCategory.IK_POLE, BoneCategory.MOUNT):
                if imgui.menu_item(_CAT_LABELS[new_cat], "", False)[0]:
                    sess.classifier.set_override(name, new_cat)
                    sess.chains = sess.classifier.detect_chains(
                        sess.skeleton.bone_names, sess.skeleton.parent_indices,
                    )
                    sess.categories = sess.classifier.classify_all(
                        sess.skeleton.bone_names, chains=sess.chains,
                    )
            imgui.separator()
            has_transforms = (
                name in sess.pose.rotations or name in sess.pose.translations
            )
            if imgui.menu_item("Clear transforms", "", False, has_transforms)[0]:
                sess.reset_bone(name)
            imgui.end_popup()

    def _draw_selected_details(self, sess) -> None:
        sel = (self.app.viewport_interact.selected_bone
               if self.app.viewport_interact is not None else None)
        if sel is None:
            imgui.text_colored(imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                               "(no bone selected)")
            return

        imgui.text(f"Selected: {sel}")
        delta = sess.pose.get_local_transform(sel)
        if delta is None:
            imgui.text_disabled("  no edits")
        else:
            rot, trans = delta
            imgui.text(f"  rot: ({rot[0]:+.3f}, {rot[1]:+.3f}, {rot[2]:+.3f}, {rot[3]:+.3f})")
            imgui.text(f"  trans: ({trans[0]:+.3f}, {trans[1]:+.3f}, {trans[2]:+.3f})")
            if imgui.button("Reset Bone"):
                sess.reset_bone(sel)
