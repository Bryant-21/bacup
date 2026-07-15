"""Preset selector panel — two-section layout for stacking presets.

Top section: Active presets shown in application order with reorder controls.
Bottom section: Available (inactive) presets that can be activated.
"""
from __future__ import annotations

import logging

from imgui_bundle import imgui

_log = logging.getLogger("toolkit.voice_changer.preset_panel")
_NS = "##voice_changer"

# Green tint for active presets
_ACTIVE_COLOR = imgui.ImVec4(0.15, 0.65, 0.15, 1.0)
_ACTIVE_HOVERED = imgui.ImVec4(0.20, 0.75, 0.20, 1.0)
_ACTIVE_BG = imgui.ImVec4(0.10, 0.40, 0.10, 0.45)

# Blue tint for previewed (selected but not active) preset
_PREVIEW_COLOR = imgui.ImVec4(0.20, 0.45, 0.80, 1.0)
_PREVIEW_HOVERED = imgui.ImVec4(0.25, 0.55, 0.90, 1.0)
_PREVIEW_BG = imgui.ImVec4(0.10, 0.25, 0.55, 0.45)

# Dimmed text for order numbers
_DIM_TEXT = imgui.ImVec4(0.6, 0.6, 0.6, 1.0)


class PresetPanel:
    """Left dock panel with active (ordered) and available preset sections."""

    def __init__(self, app):
        self._app = app
        self.window_name = f"Presets{_NS}"
        self._preset_list: list[dict] = []
        self._save_name: str = ""
        self._save_desc: str = ""
        self._show_save_popup: bool = False
        self._ctx_slug: str | None = None  # slug for right-click context menu
        self._refresh_list()

    def _refresh_list(self):
        """Reload the preset list from disk."""
        self._preset_list = self._app.preset_manager.list_presets()

    def _preset_by_slug(self, slug: str) -> dict | None:
        for p in self._preset_list:
            if p["slug"] == slug:
                return p
        return None

    def draw(self):
        imgui.begin(self.window_name)

        active_slugs = self._app.active_preset_slugs
        active_count = len(active_slugs)
        avail_region = imgui.get_content_region_avail()

        # --- Active section (top) ---
        imgui.text_colored(_ACTIVE_COLOR, f"Active ({active_count})")
        imgui.same_line()
        imgui.text_disabled("  order = apply order")
        imgui.separator()

        # Active list takes ~40% of space (min 80px), or just enough for content
        active_height = max(80.0, min(avail_region.y * 0.40, 40.0 * max(active_count, 2)))
        if imgui.begin_child("##active_presets", imgui.ImVec2(0, active_height),
                             child_flags=imgui.ChildFlags_.borders.value):
            if not active_count:
                imgui.text_disabled("No active presets — double-click below to add")
            else:
                for i, slug in enumerate(active_slugs):
                    preset = self._preset_by_slug(slug)
                    name = preset["name"] if preset else slug

                    # Order number
                    imgui.push_style_color(imgui.Col_.text.value, _DIM_TEXT)
                    imgui.text(f"{i + 1}.")
                    imgui.pop_style_color()
                    imgui.same_line()

                    # Move up button
                    if i == 0:
                        imgui.begin_disabled()
                    if imgui.small_button(f"^##up_{i}"):
                        self._app.move_preset(slug, -1)
                    if i == 0:
                        imgui.end_disabled()
                    imgui.same_line()

                    # Move down button
                    if i == active_count - 1:
                        imgui.begin_disabled()
                    if imgui.small_button(f"v##dn_{i}"):
                        self._app.move_preset(slug, 1)
                    if i == active_count - 1:
                        imgui.end_disabled()
                    imgui.same_line()

                    # Preset name (green, selectable)
                    imgui.push_style_color(imgui.Col_.header.value, _ACTIVE_BG)
                    imgui.push_style_color(imgui.Col_.header_hovered.value, _ACTIVE_HOVERED)
                    imgui.push_style_color(imgui.Col_.header_active.value, _ACTIVE_COLOR)
                    imgui.push_style_color(imgui.Col_.text.value, imgui.ImVec4(0.4, 1.0, 0.4, 1.0))
                    imgui.selectable(f"{name}##active_{i}", True)
                    imgui.pop_style_color(4)

                    # Double-click to deactivate
                    if imgui.is_item_hovered() and imgui.is_mouse_double_clicked(imgui.MouseButton_.left.value):
                        self._app.remove_preset(slug)

                    # Single-click: clear preview so filter builder shows the editable active chain
                    elif imgui.is_item_hovered() and imgui.is_mouse_clicked(imgui.MouseButton_.left.value):
                        self._app.clear_preview()

                    # Right-click context menu
                    if imgui.begin_popup_context_item(f"##ctx_active_{i}"):
                        if i > 0 and imgui.menu_item("Move up", "", False)[0]:
                            self._app.move_preset(slug, -1)
                        if i < active_count - 1 and imgui.menu_item("Move down", "", False)[0]:
                            self._app.move_preset(slug, 1)
                        if imgui.menu_item("Move to top", "", False)[0]:
                            self._app.move_preset_to(slug, 0)
                        if imgui.menu_item("Move to bottom", "", False)[0]:
                            self._app.move_preset_to(slug, active_count - 1)
                        imgui.separator()
                        if imgui.menu_item("Deactivate", "", False)[0]:
                            self._app.remove_preset(slug)
                        imgui.end_popup()

                    if imgui.is_item_hovered() and preset and preset.get("description"):
                        imgui.set_item_tooltip(preset["description"])

        imgui.end_child()

        # --- Available section (bottom) ---
        imgui.spacing()
        inactive = [p for p in self._preset_list if p["slug"] not in active_slugs]
        imgui.text_disabled(f"Available ({len(inactive)})")
        imgui.separator()

        remaining = imgui.get_content_region_avail()
        list_height = remaining.y - 80  # reserve space for buttons
        if imgui.begin_child("##available_presets", imgui.ImVec2(0, list_height),
                             child_flags=imgui.ChildFlags_.borders.value):
            if not inactive:
                imgui.text_disabled("All presets are active")
            for i, preset in enumerate(inactive):
                slug = preset["slug"]
                is_previewing = slug == self._app.preview_preset_slug

                label = preset["name"]
                if preset["builtin"]:
                    label += "  [built-in]"

                style_count = 0
                if is_previewing:
                    imgui.push_style_color(imgui.Col_.header.value, _PREVIEW_BG)
                    imgui.push_style_color(imgui.Col_.header_hovered.value, _PREVIEW_HOVERED)
                    imgui.push_style_color(imgui.Col_.header_active.value, _PREVIEW_COLOR)
                    imgui.push_style_color(imgui.Col_.text.value, imgui.ImVec4(0.5, 0.7, 1.0, 1.0))
                    style_count = 4

                imgui.selectable(f"{label}##avail_{i}", is_previewing)

                if style_count:
                    imgui.pop_style_color(style_count)

                # Double-click to activate
                if imgui.is_item_hovered() and imgui.is_mouse_double_clicked(imgui.MouseButton_.left.value):
                    self._app.add_preset(slug)
                # Single-click to preview
                elif imgui.is_item_hovered() and imgui.is_mouse_clicked(imgui.MouseButton_.left.value):
                    self._app.preview_preset(slug)

                # Right-click context menu
                if imgui.begin_popup_context_item(f"##ctx_avail_{i}"):
                    if imgui.menu_item("Activate", "", False)[0]:
                        self._app.add_preset(slug)
                    imgui.separator()
                    if not preset["builtin"]:
                        if imgui.menu_item("Delete preset", "", False)[0]:
                            self._app.preset_manager.delete_preset(slug)
                            self._refresh_list()
                    imgui.end_popup()

                if imgui.is_item_hovered() and preset.get("description"):
                    imgui.set_item_tooltip(preset["description"])

        imgui.end_child()

        # Action buttons
        imgui.separator()

        if imgui.button("New Preset"):
            self._app.clear_all_presets()
            self._app.focus_filter_builder = True

        imgui.same_line()
        if imgui.button("Save As..."):
            self._save_name = "Custom Mix"
            self._save_desc = ""
            self._show_save_popup = True
            imgui.open_popup("Save Preset##vc")

        imgui.same_line()
        if not active_count:
            imgui.begin_disabled()
        if imgui.button("Clear All"):
            self._app.clear_all_presets()
        if not active_count:
            imgui.end_disabled()

        imgui.same_line()
        # Delete — enabled when a non-builtin preset is active
        can_delete = False
        delete_target = None
        for slug in active_slugs:
            p = self._preset_by_slug(slug)
            if p and not p["builtin"]:
                can_delete = True
                delete_target = p
                break
        if not can_delete:
            imgui.begin_disabled()
        if imgui.button("Delete"):
            if delete_target:
                self._app.remove_preset(delete_target["slug"])
                self._app.preset_manager.delete_preset(delete_target["slug"])
                self._refresh_list()
        if not can_delete:
            imgui.end_disabled()

        # Save popup
        self._draw_save_popup()

        imgui.end()

    def _draw_save_popup(self):
        if imgui.begin_popup_modal("Save Preset##vc", flags=imgui.WindowFlags_.always_auto_resize.value)[0]:
            _, self._save_name = imgui.input_text("Name", self._save_name, 256)
            _, self._save_desc = imgui.input_text("Description", self._save_desc, 512)

            slug = self._save_name.lower().replace(" ", "_").replace("-", "_")
            slug = "".join(c for c in slug if c.isalnum() or c == "_")

            imgui.separator()

            if imgui.button("Save") and slug:
                self._app.preset_manager.save_preset(
                    slug, self._save_name, self._save_desc, self._app.active_chain
                )
                self._app.active_preset_slug = slug
                self._app.active_preset_name = self._save_name
                self._refresh_list()
                imgui.close_current_popup()

            imgui.same_line()
            if imgui.button("Cancel"):
                imgui.close_current_popup()

            imgui.end_popup()

    def collect_settings(self) -> dict:
        return {"active_presets": list(self._app.active_preset_slugs)}

    def restore_settings(self, active_preset: str = "", active_presets: list | None = None):
        if active_presets:
            for slug in active_presets:
                self._app.add_preset(slug)
        elif active_preset:
            self._app.add_preset(active_preset)
        self._refresh_list()
