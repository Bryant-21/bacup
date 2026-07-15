"""SWF editor workspace adapter for the toolkit."""
from __future__ import annotations

import logging
from pathlib import Path

from imgui_bundle import hello_imgui, imgui

from creation_lib.ui.shell import BaseWorkspace, make_window

_log = logging.getLogger(__name__)

_NS = "##swf"


class SwfEditorWorkspace(BaseWorkspace):
    name = "SWF Editor"
    icon = "SWF"
    id = "swf"
    user_guide_body = """
Create or open a SWF, edit the scene with the viewport, and manage layers, properties, and timeline controls.
Use the library panel for reusable shapes and export when the file is ready.
"""

    def get_dockable_windows(self) -> list[hello_imgui.DockableWindow]:
        return [
            make_window(f"Viewport{_NS}", "MainDockSpace"),
            make_window(f"Layers{_NS}", "LeftDock"),
            make_window(f"Properties{_NS}", "RightDock"),
            make_window(f"Library{_NS}", "RightDock"),
            make_window(f"Timeline{_NS}", "BottomDock"),
        ]

    def get_required_addons(self) -> dict:
        return {}

    def initialize(self) -> None:
        from ui.swf_editor.swf_editor_app import SwfEditorApp

        self._app = SwfEditorApp(toolkit_settings=self._toolkit_settings)
        self._app.setup()
        self._app._init_panels()
        self._app._first_frame = False

        self._bind_panels({
            f"Viewport{_NS}": self._app._draw_viewport,
            f"Layers{_NS}": self._app.layers_panel.draw,
            f"Properties{_NS}": self._app.properties_panel.draw,
            f"Library{_NS}": self._app.library_panel.draw,
            f"Timeline{_NS}": self._app.timeline_panel.draw,
        })

        if self._pending_settings:
            self._apply_saved_settings(self._pending_settings)
            self._pending_settings = None

        self._initialized = True
        _log.info("SWF editor workspace initialized")

    def draw(self) -> None:
        if not self.active or not self._initialized:
            return
        self._app._handle_shortcuts()

    def draw_menu(self) -> None:
        if self._app is None:
            return

        if imgui.begin_menu("File"):
            if imgui.menu_item("New SWF", "Ctrl+N", False)[0]:
                from ui.swf_editor.swf_scene import SwfScene
                self._app.scene = SwfScene()
                self._app.camera.fit_canvas()
            if imgui.menu_item("Open SWF...", "Ctrl+O", False)[0]:
                try:
                    from creation_lib.ui.widgets.pick_folder import pick_file
                    path = pick_file(
                        "Open SWF",
                        [("SWF files", "*.swf"), ("All files", "*.*")],
                    )
                    if path:
                        self._app.open_swf(path)
                except Exception:
                    pass
            imgui.separator()
            if imgui.menu_item("Export SWF...", "Ctrl+E", False)[0]:
                try:
                    from creation_lib.ui.widgets.pick_folder import pick_save_file
                    path = pick_save_file(
                        "Export SWF",
                        [("SWF files", "*.swf")],
                        default_ext=".swf",
                    )
                    if path:
                        self._app.export_swf(path)
                except Exception:
                    pass
            imgui.end_menu()

        if imgui.begin_menu("Edit"):
            can_undo = self._app.undo_stack.can_undo
            can_redo = self._app.undo_stack.can_redo
            if imgui.menu_item(
                f"Undo {self._app.undo_stack.undo_label}" if can_undo else "Undo",
                "Ctrl+Z", False, can_undo,
            )[0]:
                self._app.undo()
            if imgui.menu_item(
                f"Redo {self._app.undo_stack.redo_label}" if can_redo else "Redo",
                "Ctrl+Shift+Z", False, can_redo,
            )[0]:
                self._app.redo()
            imgui.end_menu()

        if imgui.begin_menu("View"):
            if imgui.menu_item("Fit Canvas", "Ctrl+0", False)[0]:
                self._app.camera.fit_canvas()
            changed, snap = imgui.menu_item(
                "Grid Snap", "", self._app.camera.snap_to_grid
            )
            if changed:
                self._app.camera.snap_to_grid = snap
            imgui.end_menu()

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        if not self._app:
            return
        from imgui_bundle import icons_fontawesome_6 as fa

        def _btn(icon: str, tooltip: str) -> bool:
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(icon)
            if icon_font:
                imgui.pop_font()
            if imgui.is_item_hovered():
                imgui.set_tooltip(tooltip)
            return clicked

        def _tool_btn(icon: str, tool_id: str, tooltip: str) -> None:
            active = self._app.active_tool == tool_id
            if active:
                imgui.push_style_color(imgui.Col_.button, imgui.ImVec4(0.2, 0.4, 0.7, 1.0))
            if _btn(icon, tooltip):
                self._app.active_tool = tool_id
            if active:
                imgui.pop_style_color()
            imgui.same_line()

        def _sep() -> None:
            imgui.text("|")
            imgui.same_line()

        # Undo / Redo
        can_undo = self._app.undo_stack.can_undo
        can_redo = self._app.undo_stack.can_redo
        if not can_undo:
            imgui.begin_disabled()
        if _btn(getattr(fa, "ICON_FA_ROTATE_LEFT", "U"), "Undo (Ctrl+Z)"):
            self._app.undo()
        if not can_undo:
            imgui.end_disabled()
        imgui.same_line()
        if not can_redo:
            imgui.begin_disabled()
        if _btn(getattr(fa, "ICON_FA_ROTATE_RIGHT", "R"), "Redo (Ctrl+Shift+Z)"):
            self._app.redo()
        if not can_redo:
            imgui.end_disabled()
        imgui.same_line()
        _sep()

        # Tools
        _tool_btn(getattr(fa, "ICON_FA_ARROW_POINTER", "V"),   "select",        "Select (V)")
        _tool_btn(getattr(fa, "ICON_FA_BEZIER_CURVE",   "A"),  "direct_select", "Direct Select (A)")
        _tool_btn(getattr(fa, "ICON_FA_PEN_NIB",        "P"),  "pen",           "Pen (P)")
        _tool_btn(getattr(fa, "ICON_FA_SQUARE",         "R"),  "rect",          "Rectangle (R)")
        _tool_btn(getattr(fa, "ICON_FA_CIRCLE",         "E"),  "ellipse",       "Ellipse (E)")
        _tool_btn(getattr(fa, "ICON_FA_MINUS",          "L"),  "line",          "Line (L)")
        _tool_btn(getattr(fa, "ICON_FA_FILL_DRIP",      "G"),  "fill",          "Fill Bucket (G)")
        _tool_btn(getattr(fa, "ICON_FA_EYEDROPPER",     "I"),  "eyedropper",    "Eyedropper (I)")
        _tool_btn(getattr(fa, "ICON_FA_HAND",           "H"),  "hand",          "Hand (H)")
        _tool_btn(getattr(fa, "ICON_FA_MAGNIFYING_GLASS_PLUS", "Z"), "zoom",    "Zoom (Z)")
        _sep()

        # Zoom level
        imgui.text(f"{int(self._app.camera.zoom * 100)}%")

    def handle_file_drop(
        self,
        paths: list[str],
        *,
        x: float | None = None,
        y: float | None = None,
    ) -> bool:
        if self._app is None:
            return False
        for path in paths:
            if Path(path).suffix.lower() == ".swf":
                self._app.open_swf(path)
                return True
        return False

    def on_activate(self) -> None:
        super().on_activate()
        if self._app:
            self._app.active = True

    def on_deactivate(self) -> None:
        super().on_deactivate()
        if self._app:
            self._app.active = False

    def cleanup(self) -> None:
        if self._app:
            self._app.cleanup()

    def get_settings_defaults(self) -> dict:
        return {
            "grid_snap": False,
            "onion_skin": False,
        }

    def apply_settings(self, settings: dict) -> None:
        if self._initialized and self._app:
            self._apply_saved_settings(settings)
        else:
            self._pending_settings = settings

    def _apply_saved_settings(self, settings: dict) -> None:
        if self._app:
            self._app.camera.snap_to_grid = settings.get("grid_snap", False)

    def collect_settings(self) -> dict:
        if self._app:
            return {
                "grid_snap": self._app.camera.snap_to_grid,
            }
        return self.get_settings_defaults()
