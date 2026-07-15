"""Menu bar items for the material editor (File, Edit menus)."""

from __future__ import annotations

from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ..app import MaterialEditorApp


# Game presets: (label, default_bgsm_version, default_bgem_version)
_TYPE_ITEMS = ["BGSM", "BGEM"]

_GAME_PRESETS = [
    ("Fallout 4", 2),
    ("Fallout 76", 21),
]


def draw_menu(app: MaterialEditorApp) -> None:
    """Draw File and Edit menus. Called inside a main menu bar context."""
    if imgui.begin_menu("File"):
        if imgui.menu_item("New", "Ctrl+N", False)[0]:
            app.new_material(app.file_type, app.version)
        if imgui.menu_item("Open...", "Ctrl+O", False)[0]:
            app.open_file_dialog()
        if imgui.menu_item("Save", "Ctrl+S", False, app.file_path is not None)[0]:
            app.save_file()
        if imgui.menu_item("Save As...", "", False)[0]:
            app.save_file_as()
        imgui.separator()
        if imgui.menu_item("Import JSON...", "", False)[0]:
            app.import_json()
        if imgui.menu_item("Export JSON...", "", False, bool(app.fields_dict))[0]:
            app.export_json()
        imgui.separator()
        if app.recent_files and imgui.begin_menu("Recent Files"):
            for path in app.recent_files:
                if imgui.menu_item(path, "", False)[0]:
                    app.open_file(path)
            imgui.end_menu()
        imgui.end_menu()

    if imgui.begin_menu("Edit"):
        if imgui.menu_item("Undo", "Ctrl+Z", False, bool(app.undo_stack))[0]:
            app.undo()
        if imgui.menu_item("Redo", "Ctrl+Y", False, bool(app.redo_stack))[0]:
            app.redo()
        imgui.end_menu()


def draw_version_row(app: MaterialEditorApp) -> None:
    """Draw the version selector row above the tab bar."""
    imgui.text("Version:")
    imgui.same_line()
    imgui.set_next_item_width(130)
    changed, new_ver = imgui.input_int("##version", app.version)
    if changed and new_ver >= 0:
        app.set_field("version", new_ver, track_undo=False)
        app.version = new_ver

    imgui.same_line()
    imgui.text("Game:")
    imgui.same_line()
    imgui.set_next_item_width(140)
    current_label = next(
        (label for label, ver in _GAME_PRESETS if ver == app.version), "Custom"
    )
    if imgui.begin_combo("##game_preset", current_label):
        for label, default_ver in _GAME_PRESETS:
            selected = app.version == default_ver
            if imgui.selectable(f"{label} (v{default_ver})", selected)[0]:
                app.version = default_ver
                app.fields_dict["version"] = default_ver
        imgui.end_combo()

    imgui.same_line()
    imgui.text("Type:")
    imgui.same_line()
    imgui.set_next_item_width(100)
    type_idx = 0 if app.file_type == "bgsm" else 1
    changed, new_idx = imgui.combo("##file_type", type_idx, _TYPE_ITEMS)
    if changed:
        app.new_material(_TYPE_ITEMS[new_idx].lower(), app.version)
    if app.dirty:
        imgui.same_line()
        imgui.text_colored(imgui.ImVec4(1.0, 0.8, 0.2, 1.0), " [Modified]")
    if app.file_path:
        # File path on its own line to avoid overlap when window is narrow
        imgui.text_disabled(app.file_path)
