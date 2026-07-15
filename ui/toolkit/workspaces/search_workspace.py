"""Search workspace — wraps ui.search.SearchApp for the toolkit."""
from __future__ import annotations

import logging

from imgui_bundle import imgui

from creation_lib.ui.shell import BaseWorkspace, make_window

_log = logging.getLogger("toolkit.search")
_NS = "##search"


class SearchWorkspace(BaseWorkspace):
    """Workspace wrapper for the FO4 data search UI."""

    name = "Omni Search"
    icon = "SRC"
    id = "search"
    user_guide_body = """
Search game data by record, script, wiki, or external mod content.
Use the search panel to narrow results, then inspect matching entries in the content panel.
"""

    def get_dockable_windows(self):
        return [
            make_window(f"Search{_NS}", "LeftDock"),
            make_window(f"Content{_NS}", "MainDockSpace"),
        ]

    def initialize(self) -> None:
        from ui.search.search_app import SearchApp

        self._app = SearchApp()
        self._bind_panels({
            f"Search{_NS}": self._draw_search,
            f"Content{_NS}": self._draw_content,
        })

        if self._pending_settings:
            self.apply_settings(self._pending_settings)
            self._pending_settings = None

        self._initialized = True
        _log.info("Search workspace initialized")

    def _draw_search(self):
        if imgui.begin(f"Search{_NS}"):
            self._app.draw_search_panel()
        imgui.end()

    def _draw_content(self):
        if imgui.begin(f"Content{_NS}"):
            self._app.draw_content_panel()
        imgui.end()

    def draw_menu(self) -> None:
        if self._view_helper:
            self._view_helper.draw(["Search##search", "Content##search"])

    def apply_settings(self, settings: dict) -> None:
        if self._app:
            from ui.toolkit.app_paths import get_db_dir
            idx = settings.get("indexes", {})
            db_dir = get_db_dir()
            fo4_data = idx.get("fo4_data", True) or (db_dir / "fo4_records.db").is_file()
            nifs = idx.get("nifs", True) or (db_dir / "fo4_nifs.db").is_file()
            behaviors = idx.get("behaviors", True) or (db_dir / "fo4_havok.db").is_file()
            self._app.set_index_flags(fo4_data=fo4_data, nifs=nifs, behaviors=behaviors)
        else:
            self._pending_settings = settings
