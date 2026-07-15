"""ESP Editor workspace shim — registers the EspEditorApp with the toolkit."""

from __future__ import annotations

import logging
from pathlib import Path

from imgui_bundle import imgui, icons_fontawesome_6 as fa

from creation_lib.ui.shell import BaseWorkspace, make_window

_log = logging.getLogger("toolkit.esp_editor")
_NS = "##esp_editor"


class EspEditorWorkspace(BaseWorkspace):
    name = "ESP Editor"
    icon = "ESP"
    id = "esp_editor"
    user_guide_body = """
Open a plugin, inspect records, and edit overrides in the record view.
Use the tools and conflict checks to validate changes before saving.
"""

    def get_dockable_windows(self):
        return [
            make_window(f"Plugins{_NS}", "LeftDock"),
            make_window(f"Record{_NS}", "MainDockSpace"),
            make_window(f"Info{_NS}", "RightDock"),
        ]

    def initialize(self) -> None:
        from ui.esp_editor.app import EspEditorApp

        self._app = EspEditorApp(toolkit_settings=self._toolkit_settings)
        self._bind_panels({
            f"Plugins{_NS}": self._draw_plugins,
            f"Record{_NS}": self._draw_record,
            f"Info{_NS}": self._draw_info,
        })

        if self._pending_settings:
            self.apply_settings(self._pending_settings)
            self._pending_settings = None

        self._initialized = True
        _log.info("ESP Editor workspace initialized")

    def _draw_plugins(self):
        if imgui.begin(f"Plugins{_NS}"):
            self._app.draw_nav_tree()
        imgui.end()

    def _draw_record(self):
        if imgui.begin(f"Record{_NS}"):
            self._app.draw_record_view()
        imgui.end()

    def _draw_info(self):
        if imgui.begin(f"Info{_NS}"):
            self._app.draw_info_tabs()
        imgui.end()

    def draw(self) -> None:
        if not self.active or not self._app:
            return
        # Per-frame: drain background futures, then draw the overlay if busy.
        self._app.poll()
        self._app.draw_busy_overlay()
        # Keyboard shortcuts: Ctrl+S save, Ctrl+Z undo, Ctrl+Y redo, Ctrl+O open.
        io = imgui.get_io()
        if io.key_ctrl:
            if imgui.is_key_pressed(imgui.Key.s):
                if io.key_shift:
                    self._app.save_active(save_as=True)
                else:
                    self._app.open_save_popup()
            elif imgui.is_key_pressed(imgui.Key.o):
                self._app.open_plugin()
            elif imgui.is_key_pressed(imgui.Key.z):
                self._app.undo()
            elif imgui.is_key_pressed(imgui.Key.y):
                self._app.redo()

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        """Standard toolbar — file ops, undo/redo, validate, conflict tools."""
        if self._app is None:
            return

        def _btn(label: str) -> bool:
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(label)
            if icon_font:
                imgui.pop_font()
            return clicked

        def _icon(name: str, fallback: str) -> str:
            return getattr(fa, name, fallback)

        def _sep() -> None:
            imgui.same_line()
            imgui.text("|")
            imgui.same_line()

        has_active = self._app.session.active is not None
        has_dirty = bool(self._app._dirty_handles)
        has_plugins = bool(self._app.session.plugins)
        has_patch = self._app.session._patch_handle is not None
        has_selection = self._app.selection.record is not None
        has_undo = bool(self._app._undo)
        has_redo = bool(self._app._redo)

        # Open
        if _btn(_icon("ICON_FA_FOLDER_OPEN", "Open")):
            self._app.open_plugin()
        imgui.set_item_tooltip("Open plugin (Ctrl+O)")
        imgui.same_line()

        # Open Folder
        if _btn(_icon("ICON_FA_FOLDER_TREE", "Folder")):
            self._app.open_folder()
        imgui.set_item_tooltip("Open mod folder")
        imgui.same_line()

        # Import Load Order
        if _btn(_icon("ICON_FA_LIST_OL", "Order")):
            self._app.import_load_order()
        imgui.set_item_tooltip("Import load order...")
        imgui.same_line()

        # Save
        if not has_dirty:
            imgui.begin_disabled()
        if _btn(_icon("ICON_FA_FLOPPY_DISK", "Save")):
            self._app.open_save_popup()
        if not has_dirty:
            imgui.end_disabled()
        imgui.set_item_tooltip("Save dirty plugins (Ctrl+S)")
        imgui.same_line()

        # Save As
        if not has_active:
            imgui.begin_disabled()
        if _btn(_icon("ICON_FA_FILE_EXPORT", "SaveAs")):
            self._app.save_active(save_as=True)
        if not has_active:
            imgui.end_disabled()
        imgui.set_item_tooltip("Save active plugin as... (Ctrl+Shift+S)")
        imgui.same_line()

        # Close active
        if not has_active:
            imgui.begin_disabled()
        if _btn(_icon("ICON_FA_XMARK", "Close")):
            self._app.close_active()
        if not has_active:
            imgui.end_disabled()
        imgui.set_item_tooltip("Close active plugin")

        _sep()

        # Undo / Redo
        if not has_undo:
            imgui.begin_disabled()
        if _btn(_icon("ICON_FA_ROTATE_LEFT", "Undo")):
            self._app.undo()
        if not has_undo:
            imgui.end_disabled()
        imgui.set_item_tooltip("Undo (Ctrl+Z)")
        imgui.same_line()

        if not has_redo:
            imgui.begin_disabled()
        if _btn(_icon("ICON_FA_ROTATE_RIGHT", "Redo")):
            self._app.redo()
        if not has_redo:
            imgui.end_disabled()
        imgui.set_item_tooltip("Redo (Ctrl+Y)")

        _sep()

        # Conflict scan
        if not has_active:
            imgui.begin_disabled()
        if _btn(_icon("ICON_FA_CIRCLE_EXCLAMATION", "Check")):
            self._app.run_validation()
        if not has_active:
            imgui.end_disabled()
        imgui.set_item_tooltip("Check active plugin for errors")
        imgui.same_line()

        if not has_plugins:
            imgui.begin_disabled()
        if _btn(_icon("ICON_FA_CODE_COMPARE", "Scan")):
            self._app.run_conflict_scan()
        if not has_plugins:
            imgui.end_disabled()
        imgui.set_item_tooltip("Scan conflicts")
        imgui.same_line()

        # Copy as override
        if not has_selection:
            imgui.begin_disabled()
        if _btn(_icon("ICON_FA_CLONE", "Override")):
            self._copy_as_override()
        if not has_selection:
            imgui.end_disabled()
        imgui.set_item_tooltip("Copy selection as override into active plugin")

        _sep()

        # Patch
        if _btn(_icon("ICON_FA_FILE_CIRCLE_PLUS", "NewPatch")):
            self._app.open_new_patch_popup()
        imgui.set_item_tooltip("New patch plugin...")

        if has_patch:
            imgui.same_line()
            n_sel = len(self._app._conflict_selected_fids)
            if n_sel == 0:
                imgui.begin_disabled()
            if _btn(_icon("ICON_FA_PLUS", "AddWinner")):
                self._app.add_selected_to_patch(automerge=False)
            if n_sel == 0:
                imgui.end_disabled()
            imgui.set_item_tooltip(f"Add {n_sel} winner(s) to patch")
            imgui.same_line()
            if n_sel == 0:
                imgui.begin_disabled()
            if _btn(_icon("ICON_FA_OBJECT_GROUP", "AutoMerge")):
                self._app.add_selected_to_patch(automerge=True)
            if n_sel == 0:
                imgui.end_disabled()
            imgui.set_item_tooltip(f"Auto-merge {n_sel} selection(s) into patch")

    def draw_menu(self) -> None:
        if not self._app:
            return
        if self._view_helper:
            self._view_helper.draw([f"Plugins{_NS}", f"Record{_NS}", f"Info{_NS}"])

        if imgui.begin_menu("Plugin"):
            if imgui.menu_item("Open...", "Ctrl+O", False)[0]:
                self._app.open_plugin()
            if imgui.menu_item("Open Folder...", "", False)[0]:
                self._app.open_folder()
            if imgui.menu_item("Import Load Order...", "", False)[0]:
                self._app.import_load_order()
            if imgui.menu_item("Save", "Ctrl+S", False)[0]:
                self._app.open_save_popup()
            if imgui.menu_item("Save As...", "Ctrl+Shift+S", False)[0]:
                self._app.save_active(save_as=True)
            imgui.separator()
            if imgui.menu_item("Close Active", "", False)[0]:
                self._app.close_active()
            imgui.end_menu()
        if imgui.begin_menu("Edit"):
            if imgui.menu_item("Undo", "Ctrl+Z", False)[0]:
                self._app.undo()
            if imgui.menu_item("Redo", "Ctrl+Y", False)[0]:
                self._app.redo()
            imgui.end_menu()
        if imgui.begin_menu("Tools"):
            has_active = self._app.session.active is not None
            if imgui.menu_item("Copy as Override", "", False)[0]:
                self._copy_as_override()
            if imgui.menu_item("Check Active Plugin for Errors", "", False, has_active)[0]:
                self._app.run_validation()
            if imgui.menu_item("Scan Conflicts", "", False)[0]:
                self._app.run_conflict_scan()
            imgui.separator()
            if imgui.menu_item("Build Reference Info", "", False)[0]:
                self._app.run_build_ref_info()
            if imgui.menu_item("Build Reachable Info", "", False)[0]:
                self._app.run_build_reachable()
            imgui.end_menu()
        if imgui.begin_menu("Patch"):
            if imgui.menu_item("New Patch Plugin...", "", False)[0]:
                self._app.open_new_patch_popup()
            has_target = self._app.session._patch_handle is not None
            if imgui.menu_item("Add Selected Winners to Patch", "", False, has_target)[0]:
                self._app.add_selected_to_patch(automerge=False)
            if imgui.menu_item("Auto-Merge Selected", "", False, has_target)[0]:
                self._app.add_selected_to_patch(automerge=True)
            imgui.end_menu()

    def _copy_as_override(self) -> None:
        from creation_lib.esp.editor import copy_as_override

        sel = self._app.selection
        if sel.record is None:
            return
        try:
            copy_as_override(self._app.session, sel.record.form_id, deep=False)
            self._app._cached_root.pop(self._app.session.active.handle, None)
        except Exception:
            _log.exception("Copy as override failed")

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
            if ext in (".esp", ".esm", ".esl"):
                self._app.open_plugin(path)
                return True
        return False

    def cleanup(self) -> None:
        if self._app:
            try:
                self._app.cleanup()
            except Exception:
                _log.exception("ESP editor cleanup failed")
