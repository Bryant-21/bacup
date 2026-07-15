"""Cloth Maker workspace — wraps ui.cloth_maker for the toolkit."""
from __future__ import annotations

import logging
from pathlib import Path

from imgui_bundle import hello_imgui, imgui, icons_fontawesome_6 as fa

from creation_lib.ui.shell import BaseWorkspace, make_window
from creation_lib.ui.widgets.user_guide import UserGuide

_log = logging.getLogger("toolkit.cloth_maker")
_NS = "##cloth_maker"


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


_RIGHT_PANELS = [
    ("viewer", f"Viewer{_NS}", fa.ICON_FA_EYE, "Viewer"),
    ("parameters", f"Parameters{_NS}", fa.ICON_FA_SLIDERS, "Parameters"),
    ("preview", f"Preview{_NS}", fa.ICON_FA_PLAY, "Preview"),
    ("authoring", f"Authoring{_NS}", fa.ICON_FA_PEN_RULER, "Authoring"),
]


class ClothMakerWorkspace(BaseWorkspace):
    """Cloth viewing and authoring workspace."""

    name = "Cloth"
    icon = "CLT"
    id = "cloth_maker"

    def get_user_guide(self):
        from ui.cloth_maker.panels.help_panel import USER_GUIDE_MARKDOWN

        return UserGuide(
            "Cloth Maker User Guide",
            USER_GUIDE_MARKDOWN,
            "cloth_maker_user_guide",
        )

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self._active_right_panel: str = "viewer"

    def get_dockable_windows(self):
        return [
            make_window(f"Viewport{_NS}", "MainDockSpace"),
            make_window(f"Cloth Tree{_NS}", "LeftDock"),
            make_window(f"Viewer{_NS}", "RightDock"),
            make_window(f"Parameters{_NS}", "RightDock", is_visible=False),
            make_window(f"Preview{_NS}", "RightDock", is_visible=False),
            make_window(f"Templates{_NS}", "LeftDock"),
            make_window(f"Cloth Area{_NS}", "LeftDock"),
            make_window(f"Region{_NS}", "LeftDockBottom"),
            make_window(f"Authoring{_NS}", "RightDock", is_visible=False),
            make_window(f"Help{_NS}", "RightDock", is_visible=False),
        ]

    def initialize(self) -> None:
        from ui.cloth_maker.cloth_maker_app import ClothMakerApp
        from ui.cloth_maker.panels.help_panel import HelpPanel

        self._help_panel = HelpPanel()
        self._app = ClothMakerApp(toolkit_settings=self._toolkit_settings)
        self._app.setup()
        self._app._init_panels()
        self._app._first_frame = False
        self._initialized = True

        if self._pending_settings:
            self._apply_saved_settings(self._pending_settings)
            self._pending_settings = None

        self._bind_panels({
            f"Viewport{_NS}": self._draw_viewport,
            f"Cloth Tree{_NS}": self._app.cloth_tree_panel.draw,
            f"Viewer{_NS}": self._app.viewer_panel.draw,
            f"Parameters{_NS}": self._app.param_panel.draw,
            f"Preview{_NS}": self._app.preview_panel.draw,
            f"Templates{_NS}": self._app.template_panel.draw,
            f"Cloth Area{_NS}": self._app.cloth_area_panel.draw,
            f"Region{_NS}": self._app.region_panel.draw,
            f"Authoring{_NS}": self._app.authoring_panel.draw,
            f"Help{_NS}": self._help_panel.draw,
        })
        _log.info("Cloth Maker workspace initialized")

    def _draw_viewport(self) -> None:
        """Draw the 3D viewport panel."""
        if self._app is not None:
            self._app._draw_viewport()

    def draw_menu(self) -> None:
        app = self._app
        if app is None:
            return

        has_cloth = app.scene.loaded

        if imgui.begin_menu("File"):
            if imgui.menu_item("Import NIF...", "Ctrl+I", False)[0]:
                path = _open_file_dialog(
                    "Import NIF",
                    [("NIF files", "*.nif"), ("All files", "*.*")],
                )
                if path:
                    app.import_nif(path)

            # Open Recent submenu
            self._open_recent_submenu()

            imgui.separator()
            if imgui.menu_item("Export NIF...", "Ctrl+E", False, has_cloth)[0]:
                path = _save_file_dialog(
                    "Export Cloth NIF",
                    [("NIF files", "*.nif"), ("All files", "*.*")],
                    ".nif",
                )
                if path:
                    app.export_nif(path)
            imgui.end_menu()

        if imgui.begin_menu("Edit"):
            can_undo = app.undo_stack.can_undo
            undo_label = f"Undo: {app.undo_stack.undo_label}" if can_undo else "Undo"
            if imgui.menu_item(undo_label, "Ctrl+Z", False, can_undo)[0]:
                app.undo()

            can_redo = app.undo_stack.can_redo
            redo_label = f"Redo: {app.undo_stack.redo_label}" if can_redo else "Redo"
            if imgui.menu_item(redo_label, "Ctrl+Y", False, can_redo)[0]:
                app.redo()
            imgui.end_menu()

        if imgui.begin_menu("View"):
            # Rendering options
            _, app.wireframe = imgui.menu_item(
                "Wireframe", "", app.wireframe,
            )
            _, app.backface_culling = imgui.menu_item(
                "Backface Culling", "", app.backface_culling,
            )
            grid_vis = app.scene_settings.grid_visible
            changed, grid_vis = imgui.menu_item(
                "Grid", "G", grid_vis,
            )
            if changed:
                app.scene_settings.grid_visible = grid_vis
                renderer = getattr(app, 'renderer', None)
                if renderer is not None:
                    renderer.grid_visible = grid_vis
            imgui.separator()

            # Cloth overlay toggles
            if has_cloth:
                _, app.scene.show_particles = imgui.menu_item(
                    "Particles", "", app.scene.show_particles,
                )
                _, app.scene.show_constraints = imgui.menu_item(
                    "Constraints", "", app.scene.show_constraints,
                )
                _, app.scene.show_capsules = imgui.menu_item(
                    "Capsules", "", app.scene.show_capsules,
                )
                _, app.scene.show_pins = imgui.menu_item(
                    "Pin Markers", "", app.scene.show_pins,
                )
            else:
                imgui.text_disabled("No cloth loaded")
            imgui.separator()
            overlay = getattr(app, 'controls_overlay', None)
            if overlay is not None:
                _, overlay.visible = imgui.menu_item(
                    "Controls Overlay", "H", overlay.visible,
                )
            imgui.end_menu()

    def _open_recent_submenu(self) -> None:
        """Draw the Open Recent submenu in the File menu."""
        from ui.cloth_maker.cloth_maker_app import get_recent_list, clear_recent

        recents = get_recent_list()
        enabled = bool(recents)
        if imgui.begin_menu("Open Recent", enabled):
            for i, path in enumerate(recents):
                label = f"{Path(path).name}##{i}"
                if imgui.menu_item(label, "", False)[0]:
                    if self._app:
                        self._app.import_nif(path)
                if imgui.is_item_hovered():
                    imgui.set_tooltip(path)

            if recents:
                imgui.separator()
                if imgui.menu_item("Clear Recent", "", False)[0]:
                    clear_recent()

            imgui.end_menu()

        if self._view_helper:
            self._view_helper.draw([
                f"Viewport{_NS}", f"Cloth Tree{_NS}",
                f"Viewer{_NS}", f"Parameters{_NS}",
                f"Preview{_NS}",
                f"Templates{_NS}",
                f"Cloth Area{_NS}",
                f"Region{_NS}",
                f"Authoring{_NS}",
                f"Help{_NS}",
            ])

    def has_toolbar(self) -> bool:
        return True

    def _set_right_panel(self, panel_id: str) -> None:
        """Show one right-side panel exclusively, hiding the others."""
        self._active_right_panel = panel_id
        dp = hello_imgui.get_runner_params().docking_params
        for pid, label, _, _ in _RIGHT_PANELS:
            for w in dp.dockable_windows:
                if w.label == label:
                    w.is_visible = (pid == panel_id)
                    break

    def draw_toolbar(self, icon_font=None) -> None:
        def _btn(icon: str) -> bool:
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(icon)
            if icon_font:
                imgui.pop_font()
            return clicked

        if _btn(fa.ICON_FA_FOLDER_OPEN):
            path = _open_file_dialog(
                "Import NIF",
                [("NIF files", "*.nif"), ("All files", "*.*")],
            )
            if path and self._app:
                self._app.import_nif(path)
        imgui.set_item_tooltip("Import NIF")

        imgui.same_line()

        # Undo
        can_undo = bool(self._app and self._app.undo_stack.can_undo)
        if not can_undo:
            imgui.begin_disabled()
        if _btn(f"{fa.ICON_FA_ROTATE_LEFT}##cm_undo"):
            if self._app:
                self._app.undo()
        undo_tip = f"Undo: {self._app.undo_stack.undo_label}" if can_undo else "Undo"
        imgui.set_item_tooltip(undo_tip)
        if not can_undo:
            imgui.end_disabled()

        imgui.same_line()

        # Redo
        can_redo = bool(self._app and self._app.undo_stack.can_redo)
        if not can_redo:
            imgui.begin_disabled()
        if _btn(f"{fa.ICON_FA_ROTATE_RIGHT}##cm_redo"):
            if self._app:
                self._app.redo()
        redo_tip = f"Redo: {self._app.undo_stack.redo_label}" if can_redo else "Redo"
        imgui.set_item_tooltip(redo_tip)
        if not can_redo:
            imgui.end_disabled()

        imgui.same_line()
        if _btn(f"{fa.ICON_FA_EXPAND}##cm_frame"):
            if self._app:
                sd = self._app.skin_data
                if sd is not None:
                    self._app.frame_camera(sd.vertices)
                elif (self._app.scene.particle_data is not None
                      and self._app.scene.particle_data.positions is not None):
                    self._app.frame_camera(
                        self._app.scene.particle_data.positions)
        imgui.set_item_tooltip("Frame camera on mesh (F)")

        imgui.same_line()
        imgui.text("|")
        imgui.same_line()

        for panel_id, label, icon, tooltip in _RIGHT_PANELS:
            is_active = self._active_right_panel == panel_id
            if is_active:
                imgui.push_style_color(
                    imgui.Col_.button, imgui.ImVec4(0.2, 0.4, 0.7, 1.0))
            if _btn(f"{icon}##{panel_id}"):
                self._set_right_panel(panel_id)
            if is_active:
                imgui.pop_style_color()
            imgui.set_item_tooltip(tooltip)
            imgui.same_line()

        imgui.text("|")
        imgui.same_line()

        overlay = getattr(self._app, 'controls_overlay', None) if self._app else None
        if overlay is not None:
            overlay_on = overlay.visible
            if overlay_on:
                imgui.push_style_color(
                    imgui.Col_.button, imgui.ImVec4(0.2, 0.4, 0.7, 1.0))
            if _btn(f"{fa.ICON_FA_KEYBOARD}##cm_controls_overlay"):
                overlay.visible = not overlay.visible
            if overlay_on:
                imgui.pop_style_color()
            imgui.set_item_tooltip("Controls Overlay (H)")
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

        # Draw floating controls overlay
        if self._app and self._app.controls_overlay:
            self._app.controls_overlay.draw()

        io = imgui.get_io()
        if not io.want_text_input:
            if imgui.is_key_pressed(imgui.Key.f1):
                self._toggle_help_panel()

            # Undo / Redo keybindings
            ctrl = io.key_ctrl
            if ctrl and imgui.is_key_pressed(imgui.Key.z) and self._app:
                self._app.undo()
            if ctrl and imgui.is_key_pressed(imgui.Key.y) and self._app:
                self._app.redo()

            # Grid toggle
            if imgui.is_key_pressed(imgui.Key.g) and self._app:
                self._app.scene_settings.grid_visible = not self._app.scene_settings.grid_visible
                renderer = getattr(self._app, 'renderer', None)
                if renderer is not None:
                    renderer.grid_visible = self._app.scene_settings.grid_visible

    def on_activate(self) -> None:
        super().on_activate()
        from imgui_bundle import hello_imgui
        hello_imgui.get_runner_params().fps_idling.enable_idling = False
        _log.info("Cloth Maker workspace activated")

    def on_deactivate(self) -> None:
        super().on_deactivate()
        from imgui_bundle import hello_imgui
        hello_imgui.get_runner_params().fps_idling.enable_idling = True
        _log.info("Cloth Maker workspace deactivated")

    def get_settings_defaults(self) -> dict:
        from ui.mesh_workspace.scene_settings import SceneSettings
        return {
            "show_particles": True,
            "show_constraints": True,
            "show_capsules": True,
            "show_pins": True,
            "show_controls_overlay": False,
            "wireframe": False,
            "backface_culling": True,
            "active_right_panel": "viewer",
            "last_import_path": "",
            "scene": SceneSettings(grid_visible=False).to_dict(),
        }

    def apply_settings(self, settings: dict) -> None:
        if self._initialized and self._app:
            self._apply_saved_settings(settings)
        else:
            self._pending_settings = settings

    def _apply_saved_settings(self, settings: dict):
        if self._app:
            self._app.scene.show_particles = settings.get("show_particles", True)
            self._app.scene.show_constraints = settings.get("show_constraints", True)
            self._app.scene.show_capsules = settings.get("show_capsules", True)
            self._app.scene.show_pins = settings.get("show_pins", True)
            self._app.wireframe = settings.get("wireframe", False)
            self._app.backface_culling = settings.get("backface_culling", True)
            if self._app.controls_overlay is not None:
                self._app.controls_overlay.visible = settings.get("show_controls_overlay", False)
            self._set_right_panel(settings.get("active_right_panel", "viewer"))

            # Restore shared scene settings
            scene_dict = settings.get("scene")
            if scene_dict and isinstance(scene_dict, dict):
                from ui.mesh_workspace.scene_settings import SceneSettings
                self._app.scene_settings = SceneSettings.from_dict(scene_dict)
                self._app.scene_settings.apply_to(self._app)

    def collect_settings(self) -> dict:
        if self._app:
            overlay = self._app.controls_overlay
            return {
                "show_particles": self._app.scene.show_particles,
                "show_constraints": self._app.scene.show_constraints,
                "show_capsules": self._app.scene.show_capsules,
                "show_pins": self._app.scene.show_pins,
                "show_controls_overlay": overlay.visible if overlay else False,
                "wireframe": self._app.wireframe,
                "backface_culling": self._app.backface_culling,
                "active_right_panel": self._active_right_panel,
                "last_import_path": self._app.scene.nif_path,
                "scene": self._app.scene_settings.to_dict(),
            }
        return self.get_settings_defaults()
