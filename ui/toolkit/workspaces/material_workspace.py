"""Material Editor workspace — BGSM/BGEM property editor."""
from __future__ import annotations

import logging

from imgui_bundle import imgui

from creation_lib.ui.shell import BaseWorkspace, make_window
from ui.materials.user_guide import USER_GUIDE_MARKDOWN

_log = logging.getLogger("toolkit.materials")
_NS = "##materials"


class MaterialWorkspace(BaseWorkspace):
    """Workspace shell for the material editor inside the toolkit."""

    name = "Materials"
    icon = "MAT"
    id = "materials"
    user_guide_body = USER_GUIDE_MARKDOWN

    def get_dockable_windows(self):
        return [make_window(f"Material Editor{_NS}", "MainDockSpace")]

    def initialize(self) -> None:
        from ui.materials.app import MaterialEditorApp

        self._app = MaterialEditorApp(toolkit_settings=self._toolkit_settings)
        if self._toolkit_settings:
            ws = self._toolkit_settings.get_workspace_settings(self.id)
            self._app.apply_settings(ws)

        if self._pending_settings:
            self._app.apply_settings(self._pending_settings)
            self._pending_settings = None

        self._bind_panels({f"Material Editor{_NS}": self._draw_editor})
        self._initialized = True
        _log.info("Material workspace initialized")

    def _draw_editor(self):
        if imgui.begin(f"Material Editor{_NS}"):
            self._app.draw()
        imgui.end()

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        from imgui_bundle import imgui, icons_fontawesome_6 as fa

        def _btn(icon):
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(icon)
            if icon_font:
                imgui.pop_font()
            return clicked

        if _btn(fa.ICON_FA_FILE):
            self._app.new_material(self._app.file_type)
        imgui.set_item_tooltip(f"New {self._app.file_type.upper()} Material")

        imgui.same_line()

        if _btn(fa.ICON_FA_FOLDER_OPEN):
            self._app.open_file_dialog()
        imgui.set_item_tooltip("Open Material")

        imgui.same_line()

        no_file = not self._app.file_path
        if no_file:
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_FLOPPY_DISK):
            self._app.save_file()
        imgui.set_item_tooltip("Save (Ctrl+S)")
        if no_file:
            imgui.end_disabled()

        imgui.same_line()

        can_undo = bool(self._app.undo_stack)
        if not can_undo:
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_ROTATE_LEFT):
            self._app.undo()
        imgui.set_item_tooltip("Undo")
        if not can_undo:
            imgui.end_disabled()

        imgui.same_line()

        can_redo = bool(self._app.redo_stack)
        if not can_redo:
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_ROTATE_RIGHT):
            self._app.redo()
        imgui.set_item_tooltip("Redo")
        if not can_redo:
            imgui.end_disabled()

    def draw_menu(self) -> None:
        self._app.draw_menu()
        if self._view_helper:
            self._view_helper.draw(["Material Editor##materials"])

    def draw(self) -> None:
        if not self.active or not self._initialized:
            return
        self._app.process_shortcuts()

    def on_activate(self) -> None:
        super().on_activate()
        _log.info("Material workspace activated")

    def on_deactivate(self) -> None:
        self.active = False
        _log.info("Material workspace deactivated")

    def get_settings_defaults(self) -> dict:
        if self._app:
            return self._app.get_settings_defaults()
        return {}

    def apply_settings(self, settings: dict) -> None:
        if self._app:
            self._app.apply_settings(settings)
        else:
            self._pending_settings = settings

    def collect_settings(self) -> dict:
        if self._app:
            return self._app.collect_settings()
        return self._pending_settings or {}

    def draw_settings(self) -> None:
        imgui.text("Material Editor Settings")
        imgui.separator()
        imgui.text_disabled("No additional settings at this time.")

    def open_file(self, path: str) -> None:
        self._app.open_file(path)
