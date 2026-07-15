"""Main window for the Behavior Graph Editor (imgui-bundle version).

Orchestrates GraphModel, GraphCanvas, NodePalette, PropertyEditor,
GlobalDataDialogs, and BehaviorBrowser into a docked layout.
"""

import logging
import os

from imgui_bundle import hello_imgui, imgui, immapp
from creation_lib.ui.widgets.pick_folder import pick_file, pick_save_file
from creation_lib.ui.widgets.user_guide import (
    UserGuide,
    draw_generic_user_guide_window,
    draw_user_guide_menu_item,
    draw_toolbar_help_button,
)

from .graph_model import GraphModel
from .graph_canvas import GraphCanvas
from .node_palette import NodePalettePanel
from .property_editor import PropertyEditorPanel
from .global_data_dialogs import GlobalDataDialogs
from .behavior_browser import BehaviorBrowserDialog
from .behavior_db import BehaviorDB, get_available_game_dbs
from .presets import WEAPON_TOGGLE_VARIABLES, WEAPON_TOGGLE_EVENTS, STANDARD_TRANSITIONS

log = logging.getLogger(__name__)


def _open_file_dialog(title, filetypes):
    return pick_file(title, filetypes)


def _save_file_dialog(title, filetypes, default_ext=""):
    return pick_save_file(title, filetypes, default_ext=default_ext)


def _show_toolbar(app: "BehaviorEditorApp", icon_font=None, include_help: bool = False) -> None:
    """Draw icon toolbar buttons for the behavior editor.

    Shared between toolkit BehaviorWorkspace.draw_toolbar() and
    the standalone main() edge toolbar callback.
    icon_font: if provided, push around each button only (not tooltips).
    """
    from imgui_bundle import imgui, icons_fontawesome_6 as fa

    def _btn(icon):
        if icon_font:
            imgui.push_font(icon_font, icon_font.legacy_size)
        clicked = imgui.button(icon)
        if icon_font:
            imgui.pop_font()
        return clicked

    # New
    if _btn(fa.ICON_FA_FILE):
        app._new_graph()
    imgui.set_item_tooltip("New Graph")

    imgui.same_line()

    # Open
    if _btn(fa.ICON_FA_FOLDER_OPEN):
        app._open_graph()
    imgui.set_item_tooltip("Open Graph")

    imgui.same_line()

    # Save
    if _btn(fa.ICON_FA_FLOPPY_DISK):
        app._save_graph()
    imgui.set_item_tooltip("Save Graph")

    if include_help:
        draw_toolbar_help_button(app, icon_font)


# ---------------------------------------------------------------------------
# Docking layout
# ---------------------------------------------------------------------------


def _create_layout() -> hello_imgui.DockingParams:
    params = hello_imgui.DockingParams()
    params.layout_condition = hello_imgui.DockingLayoutCondition.application_start

    params.docking_splits = [
        hello_imgui.DockingSplit(
            initial_dock_="MainDockSpace",
            new_dock_="LeftDock",
            direction_=imgui.Dir.left,
            ratio_=0.18,
        ),
        hello_imgui.DockingSplit(
            initial_dock_="LeftDock",
            new_dock_="LeftDockBottom",
            direction_=imgui.Dir.down,
            ratio_=0.50,
        ),
    ]

    _noop = lambda: None  # noqa: E731

    def _win(label, dock):
        w = hello_imgui.DockableWindow(label_=label, dock_space_name_=dock)
        w.call_begin_end = False
        w.gui_function = _noop
        return w

    params.dockable_windows = [
        _win("Node Palette", "LeftDock"),
        _win("Graph Canvas", "MainDockSpace"),
        _win("Properties", "LeftDockBottom"),
    ]
    return params


# ---------------------------------------------------------------------------
# Main app class
# ---------------------------------------------------------------------------


class BehaviorEditorApp:
    """Top-level application state and GUI loop."""

    def __init__(self):
        self.model = GraphModel()
        self.canvas = GraphCanvas()
        self.palette = NodePalettePanel()
        self.prop_editor = PropertyEditorPanel()
        self.global_dialogs = GlobalDataDialogs()
        self.browser = BehaviorBrowserDialog()
        self._game_dbs = get_available_game_dbs()
        self.behavior_db = self._game_dbs.get("fo4") or BehaviorDB()

        self._current_file: str | None = None
        self._status_msg = ""
        self._show_about = False
        self._show_settings = False
        self._auto_layout_on_import = True
        self._show_user_guide = False
        self.active = True  # toolkit sets this to False when workspace is inactive
        self._behaviors_index_enabled: bool = True

        # Window labels (toolkit overrides with ##behavior suffix)
        self._palette_label = "Node Palette"
        self._canvas_label = "Graph Canvas"
        self._props_label = "Properties"

        # Start with default nodes
        self.model.create_initial_nodes()

    def get_user_guide(self) -> UserGuide:
        from .panels.help_panel import USER_GUIDE_MARKDOWN

        return UserGuide(
            "Behavior Graph Editor User Guide",
            USER_GUIDE_MARKDOWN,
            "behavior_user_guide",
        )

    def toggle_user_guide(self) -> None:
        self._show_user_guide = not self._show_user_guide

    def draw_user_guide_window(self) -> None:
        self._show_user_guide = draw_generic_user_guide_window(
            self._show_user_guide,
            self.get_user_guide(),
        )

    def set_behaviors_index_enabled(self, enabled: bool) -> None:
        """Hide browse features when the behaviors index has not been built."""
        self._behaviors_index_enabled = enabled

    def gui(self):
        """Called every frame by hello_imgui."""
        if not self.active:
            return

        self._menu_bar()
        self.palette.render()
        self._canvas_window()
        self.prop_editor.render(self.model)

        # Relay palette selection to canvas
        if self.palette.selected_type_id is not None:
            self.canvas.request_create_node(self.palette.selected_type_id)

        # Relay canvas selection to property editor
        self.prop_editor.selected_node_id = self.canvas.selected_node_id

        # Render modal dialogs
        self.global_dialogs.render(self.model.global_state)
        self.browser.render()

        # Handle browser open request — canvas already rendered above this
        # frame so the node editor context is closed; safe to import now.
        if self.browser.open_path:
            self._import_xml_path(self.browser.open_path)

        # Settings popup
        self._render_settings()

        # About popup
        if self._show_about:
            imgui.open_popup("About##about")
            self._show_about = False
        if imgui.begin_popup_modal("About##about")[0]:
            imgui.text("Behavior Graph Editor")
            imgui.text("Fallout 4 MCP Project")
            imgui.separator()
            if imgui.button("OK"):
                imgui.close_current_popup()
            imgui.end_popup()

        self.draw_user_guide_window()

    def _canvas_window(self):
        """Render the graph canvas in its dock."""
        flags = (
            imgui.WindowFlags_.no_scrollbar | imgui.WindowFlags_.no_scroll_with_mouse
        )
        visible, _ = imgui.begin(self._canvas_label, flags=flags)
        if visible:
            self.canvas.render(self.model)
        imgui.end()

    # --- Menu bar ---

    def draw_menu_items(self, include_help: bool = True):
        """Draw menu items only — for toolkit mode where host owns the menu bar."""
        self._file_menu()
        self._data_menu()
        if self._behaviors_index_enabled:
            self._browse_menu()
        self._edit_menu()
        self._view_menu()
        if include_help:
            self._help_menu()

        # Status message on right side
        if self._status_msg:
            avail = imgui.get_content_region_avail().x
            text_width = imgui.calc_text_size(self._status_msg).x
            imgui.same_line(avail - text_width)
            imgui.text_disabled(self._status_msg)

    def _menu_bar(self):
        """Draw the full menu bar (standalone mode)."""
        if imgui.begin_main_menu_bar():
            self.draw_menu_items()
            imgui.end_main_menu_bar()

    def _file_menu(self):
        if imgui.begin_menu("File"):
            if imgui.menu_item("New", "Ctrl+N", False)[0]:
                self._new_graph()
            imgui.separator()
            if imgui.menu_item("Open...", "Ctrl+O", False)[0]:
                self._open_graph()
            if imgui.menu_item("Save", "Ctrl+S", False)[0]:
                self._save_graph()
            if imgui.menu_item("Save As...", "Ctrl+Shift+S", False)[0]:
                self._save_graph_as()
            imgui.separator()
            if imgui.menu_item("Import XML...", "", False)[0]:
                self._import_xml()
            if imgui.menu_item("Export XML...", "", False)[0]:
                self._export_xml()
            imgui.separator()
            if imgui.menu_item("Import HKX...", "", False)[0]:
                self._import_hkx()
            if imgui.menu_item("Export HKX...", "", False)[0]:
                self._export_hkx()
            imgui.end_menu()

    def _data_menu(self):
        if imgui.begin_menu("Data"):
            if imgui.menu_item("Variables...", "", False)[0]:
                self.global_dialogs.open("variables", self.behavior_db)
            if imgui.menu_item("Events...", "", False)[0]:
                self.global_dialogs.open("events", self.behavior_db)
            if imgui.menu_item("Transitions...", "", False)[0]:
                self.global_dialogs.open("transitions", self.behavior_db)
            if imgui.menu_item("Payloads...", "", False)[0]:
                self.global_dialogs.open("payloads", self.behavior_db)
            if imgui.menu_item("Properties...", "", False)[0]:
                self.global_dialogs.open("properties", self.behavior_db)
            imgui.separator()
            if imgui.begin_menu("Presets"):
                if imgui.menu_item("Add Weapon Toggle Variables", "", False)[0]:
                    self._add_weapon_toggle_vars()
                if imgui.menu_item("Add Common Events", "", False)[0]:
                    self._add_common_events()
                if imgui.menu_item("Add Standard Transitions", "", False)[0]:
                    self._add_standard_transitions()
                imgui.end_menu()
            imgui.end_menu()

    def _browse_menu(self):
        if imgui.begin_menu("Browse"):
            db_ok = bool(self._game_dbs)
            if not db_ok:
                imgui.begin_disabled()
            if imgui.menu_item("Browse Behaviors...", "Ctrl+B", False)[0]:
                self.browser.open(self._game_dbs)
            if not db_ok:
                imgui.end_disabled()
                if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled):
                    imgui.set_tooltip(
                        "havok.db not found.\nRun: uv run python py_creation_lib/python/creation_lib/preprocessor/havok.py"
                    )
            imgui.end_menu()

    def _edit_menu(self):
        if imgui.begin_menu("Edit"):
            if imgui.menu_item("Delete Selected", "Del", False)[0]:
                self._delete_selected()
            imgui.separator()
            if imgui.menu_item("Validate Graph", "Ctrl+Shift+V", False)[0]:
                self._validate_graph()
            imgui.end_menu()

    def _view_menu(self):
        if imgui.begin_menu("View"):
            if imgui.menu_item("Auto Layout", "Ctrl+L", False)[0]:
                self.canvas.request_layout()
            if imgui.menu_item("Fit All Nodes", "", False)[0]:
                self.canvas.navigate_to_content()

            imgui.separator()

            if imgui.begin_menu("Panels"):
                changed, val = imgui.checkbox("Node Palette", self.palette._visible)
                if changed:
                    self.palette._visible = val
                changed, val = imgui.checkbox("Properties", self.prop_editor._visible)
                if changed:
                    self.prop_editor._visible = val
                imgui.end_menu()

            if imgui.menu_item("Settings", "", False)[0]:
                self._show_settings = True

            imgui.separator()

            rp = hello_imgui.get_runner_params()
            if rp:
                changed, val = imgui.checkbox(
                    "Status Bar", rp.imgui_window_params.show_status_bar
                )
                if changed:
                    rp.imgui_window_params.show_status_bar = val

            imgui.end_menu()

    def _help_menu(self):
        if imgui.begin_menu("Help"):
            draw_user_guide_menu_item(self)
            imgui.separator()
            if imgui.menu_item("About", "", False)[0]:
                self._show_about = True
            imgui.end_menu()

    # --- File operations ---

    def _new_graph(self):
        self.model.create_initial_nodes()
        self._current_file = None
        self._status_msg = "New graph"
        self.prop_editor.selected_node_id = None

    def _open_graph(self):
        path = _open_file_dialog("Open Graph", [("JSON", "*.json"), ("All", "*.*")])
        if not path:
            return
        try:
            self.model.load_json(path)
            self._current_file = path
            self._status_msg = os.path.basename(path)
            self.prop_editor.selected_node_id = None
        except Exception as e:
            log.error("Load failed: %s", e, exc_info=True)
            self._status_msg = f"Load error: {e}"

    def _save_graph(self):
        if self._current_file:
            self._save_to(self._current_file)
        else:
            self._save_graph_as()

    def _save_graph_as(self):
        path = _save_file_dialog("Save Graph", [("JSON", "*.json")], ".json")
        if not path:
            return
        self._save_to(path)

    def _save_to(self, path):
        try:
            self.model.save_json(path)
            self._current_file = path
            self._status_msg = f"Saved: {os.path.basename(path)}"
        except Exception as e:
            log.error("Save failed: %s", e, exc_info=True)
            self._status_msg = f"Save error: {e}"

    def _import_xml(self):
        path = _open_file_dialog("Import XML", [("XML", "*.xml"), ("All", "*.*")])
        if not path:
            return
        self._import_xml_path(path)

    def _import_xml_path(self, path):
        try:
            result = self.model.import_xml(path)
            self._current_file = None
            n = len(self.model.nodes)
            c = len(self.model.connections)
            self._status_msg = f"Imported: {n} nodes, {c} connections"
            self.prop_editor.selected_node_id = None
            if self._auto_layout_on_import:
                self.canvas.request_layout()
            unhandled = result.get("unhandled", [])
            if unhandled:
                log.warning("Unhandled nodes: %s", unhandled)
        except Exception as e:
            log.error("XML import failed: %s", e, exc_info=True)
            self._status_msg = f"Import error: {e}"

    def _export_xml(self):
        path = _save_file_dialog("Export XML", [("XML", "*.xml")], ".xml")
        if not path:
            return
        try:
            self.model.export_xml(path)
            self._status_msg = f"Exported XML: {os.path.basename(path)}"
        except Exception as e:
            log.error("XML export failed: %s", e, exc_info=True)
            self._status_msg = f"Export error: {e}"

    def _import_hkx(self):
        path = _open_file_dialog("Import HKX", [("HKX", "*.hkx"), ("All", "*.*")])
        if not path:
            return
        try:
            result = self.model.import_hkx(path)
            self._current_file = None
            n = len(self.model.nodes)
            c = len(self.model.connections)
            self._status_msg = f"Imported HKX: {n} nodes, {c} connections"
            self.prop_editor.selected_node_id = None
            if self._auto_layout_on_import:
                self.canvas.request_layout()
        except Exception as e:
            log.error("HKX import failed: %s", e, exc_info=True)
            self._status_msg = f"HKX import error: {e}"

    def _export_hkx(self):
        path = _save_file_dialog("Export HKX", [("HKX", "*.hkx")], ".hkx")
        if not path:
            return
        try:
            self.model.export_hkx(path)
            self._status_msg = f"Exported HKX: {os.path.basename(path)}"
        except Exception as e:
            log.error("HKX export failed: %s", e, exc_info=True)
            self._status_msg = f"HKX export error: {e}"

    # --- Edit operations ---

    def _delete_selected(self):
        nid = self.canvas.selected_node_id
        if nid is not None:
            self.model.delete_node(nid)
            self.prop_editor.selected_node_id = None

    def _validate_graph(self):
        warnings = self.model.validate()

        # Also check against behavior_db if available
        if self.behavior_db.available:
            defined_events = set()
            for e in self.model.global_state.events:
                name = (
                    e.get("eventName", e.get("name", ""))
                    if isinstance(e, dict)
                    else e.name
                )
                defined_events.add(name)
            ref_lower = {e.lower(): e for e in self.behavior_db.get_all_event_names()}
            for name in defined_events:
                ref_name = ref_lower.get(name.lower())
                if ref_name and ref_name != name:
                    warnings.append(
                        f"Event '{name}' differs from reference casing '{ref_name}'"
                    )

        if warnings:
            self._status_msg = f"Validation: {len(warnings)} warning(s)"
            for w in warnings[:10]:
                log.warning("Validation: %s", w)
        else:
            self._status_msg = "Validation: OK"

    # --- Presets ---

    def _add_weapon_toggle_vars(self):
        existing = set()
        for v in self.model.global_state.variables:
            name = (
                v.get("variableName", v.get("name", ""))
                if isinstance(v, dict)
                else v.name
            )
            existing.add(name)
        added = 0
        for preset in WEAPON_TOGGLE_VARIABLES:
            if preset["variableName"] not in existing:
                entry = dict(
                    preset,
                    variableID=len(self.model.global_state.variables),
                    variableQuadValues="(0.0 0.0 0.0 0.0)",
                )
                self.model.global_state.variables.append(entry)
                added += 1
        self._status_msg = (
            f"Added {added} weapon toggle variables" if added else "All already exist"
        )

    def _add_common_events(self):
        existing = set()
        for e in self.model.global_state.events:
            name = (
                e.get("eventName", e.get("name", "")) if isinstance(e, dict) else e.name
            )
            existing.add(name)
        added = 0
        for evt_name in WEAPON_TOGGLE_EVENTS:
            if evt_name not in existing:
                self.model.global_state.events.append(
                    {
                        "eventID": len(self.model.global_state.events),
                        "eventName": evt_name,
                        "eventFlags": 0,
                    }
                )
                added += 1
        self._status_msg = (
            f"Added {added} common events" if added else "All already exist"
        )

    def _add_standard_transitions(self):
        existing = set()
        for t in self.model.global_state.transitions:
            name = t.get("transitionName", "") if isinstance(t, dict) else ""
            existing.add(name)
        added = 0
        for preset in STANDARD_TRANSITIONS:
            if preset["transitionName"] not in existing:
                entry = dict(
                    preset, transitionID=len(self.model.global_state.transitions) + 1
                )
                self.model.global_state.transitions.append(entry)
                added += 1
        self._status_msg = (
            f"Added {added} standard transitions" if added else "All already exist"
        )

    def _render_settings(self):
        if self._show_settings:
            imgui.open_popup("Settings##behavior_settings")
            self._show_settings = False

        center = imgui.get_main_viewport().get_center()
        imgui.set_next_window_pos(center, imgui.Cond_.appearing, imgui.ImVec2(0.5, 0.5))
        opened, visible = imgui.begin_popup_modal(
            "Settings##behavior_settings",
            True,
            imgui.WindowFlags_.always_auto_resize,
        )
        if opened:
            imgui.text("Behavior Graph Editor Settings")
            imgui.separator()

            changed, val = imgui.checkbox(
                "Auto-layout on import", self._auto_layout_on_import
            )
            if changed:
                self._auto_layout_on_import = val

            imgui.separator()
            if imgui.button("Close", imgui.ImVec2(120, 0)):
                imgui.close_current_popup()
            imgui.end_popup()

    def on_exit(self):
        self.canvas.destroy()
        for db in self._game_dbs.values():
            db.close()


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def main():
    import sys
    from pathlib import Path

    # Ensure project root is on sys.path
    project_root = str(Path(__file__).resolve().parents[2])
    if project_root not in sys.path:
        sys.path.insert(0, project_root)

    from imgui_app import create_runner_params, run_app

    logging.basicConfig(level=logging.INFO)

    app = BehaviorEditorApp()

    params = create_runner_params(
        title="Behavior Graph Editor",
        width=1400,
        height=900,
        gui_fn=app.gui,
        layout_fn=_create_layout,
        on_exit_fn=app.on_exit,
    )

    # Top icon toolbar for standalone mode
    _toolbar_opts = hello_imgui.EdgeToolbarOptions()
    _toolbar_opts.size_em = 2.5
    params.callbacks.add_edge_toolbar(
        hello_imgui.EdgeToolbarType.top,
        lambda: _show_toolbar(app, include_help=True),
        _toolbar_opts,
    )

    addons = immapp.AddOnsParams()
    addons.with_node_editor = True
    addons.with_markdown = True

    run_app(params, addons)
