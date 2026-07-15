"""Weight Painter workspace — wraps ui.weight_painter for the toolkit."""
from __future__ import annotations

import logging

from imgui_bundle import hello_imgui, imgui, icons_fontawesome_6 as fa

from creation_lib.ui.shell import BaseWorkspace, make_window
from creation_lib.ui.widgets.user_guide import UserGuide

_log = logging.getLogger("toolkit.weight_painter")
_NS = "##weight_painter"


class WeightPainterWorkspace(BaseWorkspace):
    """Weight painting and auto-skinning workspace."""

    name = "Weights"
    icon = "WGT"
    id = "weight_painter"

    def get_user_guide(self):
        from ui.weight_painter.panels.help_panel import USER_GUIDE_MARKDOWN

        return UserGuide(
            "Weight Painter User Guide",
            USER_GUIDE_MARKDOWN,
            "weight_painter_user_guide",
        )

    def get_dockable_windows(self):
        return [
            make_window(f"Viewport{_NS}", "MainDockSpace"),
            make_window(f"Bones{_NS}", "LeftDock"),
            make_window(f"Brush{_NS}", "RightDock"),
            make_window(f"Segments{_NS}", "LeftDockBottom"),
            make_window(f"Help{_NS}", "RightDock", is_visible=False),
        ]

    def initialize(self) -> None:
        from ui.weight_painter.weight_painter_app import WeightPainterApp
        from ui.weight_painter.panels.help_panel import HelpPanel

        self._app = WeightPainterApp(toolkit_settings=self._toolkit_settings)
        self._app.setup()
        self._app._init_panels()
        self._app._first_frame = False
        self._help_panel = HelpPanel()
        self._initialized = True

        if self._pending_settings:
            self._apply_saved_settings(self._pending_settings)
            self._pending_settings = None

        self._bind_panels({
            f"Viewport{_NS}": self._app.viewport_panel.draw,
            f"Bones{_NS}": self._app.bone_tree_panel.draw,
            f"Brush{_NS}": self._app.brush_panel.draw,
            f"Segments{_NS}": self._app.segment_panel.draw,
            f"Help{_NS}": self._help_panel.draw,
        })
        _log.info("Weight Painter workspace initialized")

    def draw_menu(self) -> None:
        app = self._app
        if app is None:
            return

        has_mesh = app.skin_data is not None

        if imgui.begin_menu("File"):
            if imgui.menu_item("Import Mesh...", "Ctrl+I", False)[0]:
                app._show_import_dialog = True
            if imgui.menu_item("Load Reference Body...", "", False)[0]:
                app._show_reference_dialog = True
            imgui.separator()
            if imgui.menu_item("Export NIF...", "Ctrl+E", False, has_mesh)[0]:
                app._show_export_dialog = True
            imgui.separator()
            if imgui.menu_item("Transfer Weights...", "", False, has_mesh)[0]:
                app._show_transfer_dialog = True
            imgui.separator()
            if imgui.menu_item("Close", "", False, has_mesh)[0]:
                app.close_mesh()
            imgui.end_menu()

        if imgui.begin_menu("Edit"):
            has_undo = len(app.undo_stack) > 0
            has_redo = len(app.redo_stack) > 0
            if imgui.menu_item("Undo", "Ctrl+Z", False, has_undo)[0]:
                app.undo()
            if imgui.menu_item("Redo", "Ctrl+Y", False, has_redo)[0]:
                app.redo()
            imgui.end_menu()

        if imgui.begin_menu("View"):
            dm = app.display_mode
            if imgui.menu_item("Weights Mode", "W", dm == "weights")[0]:
                app.set_display_mode("weights" if dm != "weights" else "shaded")
            if imgui.menu_item("All Bones Mode", "A", dm == "all_weights")[0]:
                app.set_display_mode(
                    "all_weights" if dm != "all_weights" else "weights"
                )
            if imgui.menu_item("Segment Mode", "P", dm == "segments")[0]:
                app.set_display_mode(
                    "segments" if dm != "segments" else "weights"
                )
            has_vc = (has_mesh and app.skin_data.vertex_colors is not None)
            if imgui.menu_item("Vertex Colors", "V", dm == "vertex_colors", has_vc)[0]:
                app.set_display_mode(
                    "vertex_colors" if dm != "vertex_colors" else "weights"
                )
            imgui.separator()
            _, app.show_wireframe = imgui.menu_item(
                "Wireframe", "F", app.show_wireframe,
            )
            _, app.show_segment_edges = imgui.menu_item(
                "Segment Edges", "B", app.show_segment_edges,
            )
            _, app.show_mask = imgui.menu_item(
                "Show Mask", "M", app.show_mask,
            )
            imgui.separator()
            _, app.mirror_x = imgui.menu_item(
                "Mirror X", "X", app.mirror_x,
            )
            _, app.auto_normalize = imgui.menu_item(
                "Auto-normalize", "N", app.auto_normalize,
            )
            imgui.end_menu()

        if self._view_helper:
            self._view_helper.draw([
                f"Viewport{_NS}", f"Bones{_NS}",
                f"Brush{_NS}", f"Segments{_NS}",
                f"Help{_NS}",
            ])

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        def _btn(icon: str) -> bool:
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(icon)
            if icon_font:
                imgui.pop_font()
            return clicked

        if _btn(fa.ICON_FA_FOLDER_OPEN):
            self._app._open_import_dialog()
        imgui.set_item_tooltip("Import Mesh (.nif, .obj)")
        imgui.same_line()

        has_ref = self._app.reference_skin is not None
        if has_ref:
            imgui.push_style_color(
                imgui.Col_.button, imgui.ImVec4(0.2, 0.5, 0.2, 1.0))
        if _btn(fa.ICON_FA_PERSON):
            self._app._show_reference_dialog = True
        if has_ref:
            imgui.pop_style_color()
        ref_label = (f"Reference: {self._app.reference_skin.num_vertices}v"
                     if has_ref else "Load Reference Body (.nif)")
        imgui.set_item_tooltip(ref_label)
        imgui.same_line()

        has_mesh = self._app.skin_data is not None
        has_ref = self._app.reference_skin is not None
        if not (has_mesh and has_ref):
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_WAND_MAGIC_SPARKLES):
            self._app.auto_skin()
        if not (has_mesh and has_ref):
            imgui.end_disabled()
        imgui.set_item_tooltip("Auto-Skin (requires reference body)")
        imgui.same_line()

        if not has_mesh:
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_FLOPPY_DISK):
            self._app._open_export_dialog()
        if not has_mesh:
            imgui.end_disabled()
        imgui.set_item_tooltip("Export NIF")

        imgui.same_line()
        imgui.text("|")
        imgui.same_line()

        modes = [
            ("weights", fa.ICON_FA_WEIGHT_HANGING, "Weight Heatmap"),
            ("segments", fa.ICON_FA_PUZZLE_PIECE, "Segment Colors"),
            ("shaded", fa.ICON_FA_CUBE, "Shaded"),
        ]
        for mode_id, icon, tooltip in modes:
            is_active = self._app.display_mode == mode_id
            if is_active:
                imgui.push_style_color(
                    imgui.Col_.button, imgui.ImVec4(0.2, 0.4, 0.7, 1.0))
            if _btn(f"{icon}##{mode_id}"):
                self._app.display_mode = mode_id
            if is_active:
                imgui.pop_style_color()
            imgui.set_item_tooltip(tooltip)
            imgui.same_line()

    def _toggle_help_panel(self):
        dp = hello_imgui.get_runner_params().docking_params
        for w in dp.dockable_windows:
            if w.label == f"Help{_NS}":
                w.is_visible = not w.is_visible
                break

    def toggle_user_guide(self) -> None:
        self._toggle_help_panel()

    def draw(self) -> None:
        if not self.active or not self._initialized:
            return
        io = imgui.get_io()
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.z):
            self._app.undo()
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.y):
            self._app.redo()
        if imgui.is_key_pressed(imgui.Key.f1) and not io.want_text_input:
            self._toggle_help_panel()

    def on_activate(self) -> None:
        super().on_activate()
        _log.info("Weight Painter workspace activated")

    def on_deactivate(self) -> None:
        super().on_deactivate()
        _log.info("Weight Painter workspace deactivated")

    def get_settings_defaults(self) -> dict:
        return {
            "brush_radius": 5.0,
            "brush_strength": 0.5,
            "brush_falloff": 0.5,
            "auto_normalize": True,
            "last_import_path": "",
            "last_export_path": "",
        }

    def apply_settings(self, settings: dict) -> None:
        if self._initialized and self._app:
            self._apply_saved_settings(settings)
        else:
            self._pending_settings = settings

    def _apply_saved_settings(self, settings: dict):
        if self._app:
            self._app.brush_radius = settings.get("brush_radius", 5.0)
            self._app.brush_strength = settings.get("brush_strength", 0.5)
            self._app.brush_falloff = settings.get("brush_falloff", 0.5)
            self._app.auto_normalize = settings.get("auto_normalize", True)

    def collect_settings(self) -> dict:
        if self._app:
            return {
                "brush_radius": self._app.brush_radius,
                "brush_strength": self._app.brush_strength,
                "brush_falloff": self._app.brush_falloff,
                "auto_normalize": self._app.auto_normalize,
                "last_import_path": self._app.file_path,
                "last_export_path": "",
            }
        return self.get_settings_defaults()
