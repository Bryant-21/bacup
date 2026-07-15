"""Behavior Graph Editor workspace — wraps ui.behaivor for the toolkit."""
from __future__ import annotations

import logging
from pathlib import Path

from imgui_bundle import hello_imgui, imgui, icons_fontawesome_6 as fa

from creation_lib.ui.shell import BaseWorkspace, make_window
from creation_lib.ui.widgets.user_guide import UserGuide

_log = logging.getLogger("toolkit.behavior")
_NS = "##behavior"


class BehaviorWorkspace(BaseWorkspace):
    """Workspace wrapper for the behavior graph editor."""

    name = "Behavior Graph"
    icon = "BHV"
    id = "behavior"

    def get_user_guide(self):
        from ui.behaivor.panels.help_panel import USER_GUIDE_MARKDOWN

        return UserGuide(
            "Behavior Graph Editor User Guide",
            USER_GUIDE_MARKDOWN,
            "behavior_user_guide",
        )

    def get_dockable_windows(self):
        return [
            make_window(f"Node Palette{_NS}", "LeftDock"),
            make_window(f"Graph Canvas{_NS}", "MainDockSpace"),
            make_window(f"Properties{_NS}", "LeftDockBottom"),
            make_window(f"Help{_NS}", "RightDock", is_visible=False),
        ]

    def get_required_addons(self) -> dict:
        return {"with_node_editor": True}

    def initialize(self) -> None:
        from ui.behaivor.main_window import BehaviorEditorApp
        from ui.behaivor.panels.help_panel import HelpPanel

        self._app = BehaviorEditorApp()
        self._help_panel = HelpPanel()

        self._app._palette_label = f"Node Palette{_NS}"
        self._app._canvas_label = f"Graph Canvas{_NS}"
        self._app._props_label = f"Properties{_NS}"
        self._app.palette.window_name = f"Node Palette{_NS}"
        self._app.prop_editor.window_name = f"Properties{_NS}"

        self._bind_panels({
            f"Node Palette{_NS}": self._app.palette.render,
            f"Graph Canvas{_NS}": self._app._canvas_window,
            f"Properties{_NS}": lambda: self._app.prop_editor.render(self._app.model),
            f"Help{_NS}": self._help_panel.draw,
        })

        if self._pending_settings:
            self.apply_settings(self._pending_settings)
            self._pending_settings = None

        self._initialized = True
        _log.info("Behavior workspace initialized")

    def draw_menu(self) -> None:
        self._app.draw_menu_items(include_help=False)
        if self._view_helper:
            self._view_helper.draw([
                "Node Palette##behavior",
                "Graph Canvas##behavior",
                "Properties##behavior",
                f"Help{_NS}",
            ])

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        from ui.behaivor.main_window import _show_toolbar
        _show_toolbar(self._app, icon_font)

        def _btn(icon: str) -> bool:
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(icon)
            if icon_font:
                imgui.pop_font()
            return clicked

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
        if imgui.is_key_pressed(imgui.Key.f1) and not io.want_text_input:
            self._toggle_help_panel()

        if self._app.palette.selected_type_id is not None:
            self._app.canvas.request_create_node(self._app.palette.selected_type_id)

        self._app.prop_editor.selected_node_id = self._app.canvas.selected_node_id

        self._app.global_dialogs.render(self._app.model.global_state)
        if self._app._behaviors_index_enabled:
            self._app.browser.render()
            if self._app.browser.open_path:
                self._app._import_xml_path(self._app.browser.open_path)

        if self._app._show_about:
            imgui.open_popup("About##about")
            self._app._show_about = False
        if imgui.begin_popup_modal("About##about")[0]:
            imgui.text("Behavior Graph Editor")
            imgui.text("Fallout 4 MCP Project")
            imgui.separator()
            if imgui.button("OK"):
                imgui.close_current_popup()
            imgui.end_popup()

    def on_activate(self) -> None:
        super().on_activate()
        _log.info("Behavior workspace activated")

    def on_deactivate(self) -> None:
        super().on_deactivate()
        _log.info("Behavior workspace deactivated")

    def cleanup(self) -> None:
        if self._app:
            self._app.on_exit()

    def get_settings_defaults(self) -> dict:
        return {"recent_files": []}

    def apply_settings(self, settings: dict) -> None:
        if self._app:
            from ui.toolkit.app_paths import get_db_dir
            flag = settings.get("indexes", {}).get("behaviors", True)
            db_exists = (get_db_dir() / "fo4_havok.db").is_file()
            self._app.set_behaviors_index_enabled(flag or db_exists)
        else:
            self._pending_settings = settings

    def open_file(self, path: str) -> None:
        """Open a behavior HKX directly in the graph editor."""
        self._app.model.import_hkx(path)
        self._app._current_file = None
        n = len(self._app.model.nodes)
        c = len(self._app.model.connections)
        self._app._status_msg = f"Imported HKX: {n} nodes, {c} connections"
        self._app.prop_editor.selected_node_id = None
        if getattr(self._app, "_auto_layout_on_import", False):
            self._app.canvas.request_layout()

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
            ext = Path(path).suffix.lower()
            if ext == ".hkx":
                self.open_file(path)
                return True
            if ext == ".xml":
                self._app._import_xml_path(path)
                return True
        return False
