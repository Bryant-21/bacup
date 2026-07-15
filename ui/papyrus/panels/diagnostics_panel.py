"""DiagnosticsPanel — error/warning list for the active Papyrus script."""
from __future__ import annotations

import logging

from imgui_bundle import imgui

_log = logging.getLogger("toolkit.papyrus.diagnostics")


class DiagnosticsPanel:
    """Bottom-dock panel listing errors and warnings for the active file."""

    def __init__(self, app):
        """
        Args:
            app: PapyrusEditorApp instance.
        """
        self._app = app
        self.window_name = "Diagnostics##papyrus"
        self._visible = True

    def draw(self):
        if not self._visible:
            return
        expanded, opened = imgui.begin(self.window_name, True)
        if not opened:
            self._visible = False
            imgui.end()
            return
        if expanded:
            self._draw_content()
        imgui.end()

    def _draw_content(self):
        path = self._app.active_path
        if path is None or path not in self._app.open_files:
            imgui.text_disabled("No file open.")
            return

        buf = self._app.open_files[path]
        diags = buf.diagnostics

        if not diags:
            imgui.text_disabled("No errors or warnings.")
            return

        if imgui.begin_table("##diags", 3,
                              imgui.TableFlags_.borders_inner_v
                              | imgui.TableFlags_.row_bg
                              | imgui.TableFlags_.scroll_y,
                              imgui.ImVec2(0, 0)):
            imgui.table_setup_column("Level", imgui.TableColumnFlags_.width_fixed, 70)
            imgui.table_setup_column("Line", imgui.TableColumnFlags_.width_fixed, 50)
            imgui.table_setup_column("Message", imgui.TableColumnFlags_.width_stretch)
            imgui.table_headers_row()

            for diag in diags:
                imgui.table_next_row()
                imgui.table_set_column_index(0)

                # Full-row selectable in column 0
                row_id = f"##diag_{diag.line}_{id(diag)}"
                clicked = imgui.selectable(
                    row_id, False,
                    imgui.SelectableFlags_.span_all_columns
                    | imgui.SelectableFlags_.allow_overlap,
                )[0]
                if clicked and path in self._app.open_files:
                    self._app.open_files[path].editor.set_cursor_position(diag.line, 0)

                # Severity label drawn over the selectable
                imgui.same_line()
                if diag.severity == "error":
                    imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.3, 0.3, 1.0))
                    imgui.text_unformatted("ERROR")
                    imgui.pop_style_color()
                else:
                    imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.85, 0.3, 1.0))
                    imgui.text_unformatted("WARN")
                    imgui.pop_style_color()

                imgui.table_set_column_index(1)
                imgui.text_unformatted(str(diag.line + 1))

                imgui.table_set_column_index(2)
                imgui.text_unformatted(diag.message)

            imgui.end_table()
