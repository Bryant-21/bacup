"""Modal popup for browsing and searching reference behaviors from the indexed database."""

from __future__ import annotations

from imgui_bundle import imgui

from .behavior_db import BehaviorDB


class BehaviorBrowserDialog:
    """ImGui modal popup for browsing behaviors stored in the SQLite index.

    Usage::

        browser = BehaviorBrowserDialog()
        # To open (pass a dict of game_id → BehaviorDB):
        browser.open({"fo4": db_fo4, "fo76": db_fo76})
        # Each frame:
        browser.render()
        if browser.open_path:
            load_behavior(browser.open_path)
    """

    def __init__(self) -> None:
        self._open: bool = False
        self._pending_open: bool = False
        self._game_dbs: dict[str, BehaviorDB] = {}
        self._game: str = ""
        self._db: BehaviorDB | None = None
        self._search_text: str = ""
        self._category: str = ""  # empty = all
        self._source: str = ""  # empty = all
        self._results: list[dict] = []
        self._selected_id: str | None = None
        self._selected_meta: dict | None = None
        self._categories: list[str] = []
        self._sources: list[str] = []
        self.open_path: str | None = None  # set when user clicks "Open in Editor"

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def open(self, game_dbs: dict[str, BehaviorDB] | BehaviorDB) -> None:
        """Open the browser dialog.

        Pass a dict of {game_id: BehaviorDB} for multi-game support, or a
        single BehaviorDB for backwards compatibility.
        """
        self._open = True
        self._pending_open = True

        if isinstance(game_dbs, BehaviorDB):
            # Backwards-compat: wrap single db
            self._game_dbs = {"fo4": game_dbs}
        else:
            self._game_dbs = game_dbs

        # Pick first available game
        self._game = next(iter(self._game_dbs), "")
        self._db = self._game_dbs.get(self._game)

        self._search_text = ""
        self._category = ""
        self._source = ""
        self._selected_id = None
        self._selected_meta = None
        self._refresh_filters()
        self._do_search()

    def _refresh_filters(self) -> None:
        if self._db is not None:
            self._categories = self._db.get_categories()
            self._sources = self._db.get_sources()
        else:
            self._categories = []
            self._sources = []

    def render(self) -> None:
        """Call each frame. Draws the modal popup when open."""
        if not self._open:
            return

        self.open_path = None  # reset each frame

        if self._pending_open:
            imgui.open_popup("Browse Behaviors##browser")
            self._pending_open = False

        center = imgui.get_main_viewport().get_center()
        imgui.set_next_window_pos(center, imgui.Cond_.appearing, imgui.ImVec2(0.5, 0.5))
        imgui.set_next_window_size(imgui.ImVec2(900, 600), imgui.Cond_.appearing)

        opened, _ = imgui.begin_popup_modal("Browse Behaviors##browser")
        if not opened:
            self._open = False
            return

        self._render_filters()
        imgui.separator()
        self._render_results_table()
        imgui.separator()
        self._render_details()
        imgui.separator()
        self._render_buttons()

        imgui.end_popup()

    # ------------------------------------------------------------------
    # Filter row
    # ------------------------------------------------------------------

    def _render_filters(self) -> None:
        avail_width = imgui.get_content_region_avail().x
        combo_width = 140.0
        spacing = imgui.get_style().item_spacing.x
        num_combos = 3  # game + category + source
        search_width = avail_width - num_combos * (combo_width + spacing)
        if search_width < 100.0:
            search_width = 100.0

        imgui.set_next_item_width(search_width)
        changed, self._search_text = imgui.input_text_with_hint(
            "##browser_search", "Search behaviors...", self._search_text
        )
        if changed:
            self._do_search()

        imgui.same_line()
        imgui.set_next_item_width(combo_width)
        if imgui.begin_combo("##browser_game", self._game or "Game"):
            for game_id in self._game_dbs:
                selected, _ = imgui.selectable(game_id, self._game == game_id)
                if selected and game_id != self._game:
                    self._game = game_id
                    self._db = self._game_dbs[game_id]
                    self._category = ""
                    self._source = ""
                    self._refresh_filters()
                    self._do_search()
            imgui.end_combo()

        imgui.same_line()
        imgui.set_next_item_width(combo_width)
        if imgui.begin_combo("##browser_category", self._category or "All Categories"):
            if imgui.selectable("All Categories", self._category == "")[0]:
                self._category = ""
                self._do_search()
            for cat in self._categories:
                selected, _ = imgui.selectable(cat, self._category == cat)
                if selected:
                    self._category = cat
                    self._do_search()
            imgui.end_combo()

        imgui.same_line()
        imgui.set_next_item_width(combo_width)
        if imgui.begin_combo("##browser_source", self._source or "All Sources"):
            if imgui.selectable("All Sources", self._source == "")[0]:
                self._source = ""
                self._do_search()
            for src in self._sources:
                selected, _ = imgui.selectable(src, self._source == src)
                if selected:
                    self._source = src
                    self._do_search()
            imgui.end_combo()

    # ------------------------------------------------------------------
    # Results table
    # ------------------------------------------------------------------

    def _render_results_table(self) -> None:
        # Calculate available height: leave room for details + buttons
        detail_height = 140.0 if self._selected_meta else 0.0
        button_row_height = imgui.get_frame_height_with_spacing() * 2
        table_height = (
            imgui.get_content_region_avail().y
            - detail_height
            - button_row_height
            - 2 * (imgui.get_style().item_spacing.y + 1)  # separators
        )
        if table_height < 80.0:
            table_height = 80.0

        flags = (
            imgui.TableFlags_.borders_inner_h
            | imgui.TableFlags_.row_bg
            | imgui.TableFlags_.scroll_y
            | imgui.TableFlags_.resizable
            | imgui.TableFlags_.sortable
            | imgui.TableFlags_.sizing_stretch_prop
        )

        if not imgui.begin_table("##browser_results", 5, flags, imgui.ImVec2(0, table_height)):
            return

        imgui.table_setup_column("Name", imgui.TableColumnFlags_.default_sort, 0.40)
        imgui.table_setup_column("Category", imgui.TableColumnFlags_.none, 0.20)
        imgui.table_setup_column("Source", imgui.TableColumnFlags_.none, 0.15)
        imgui.table_setup_column("Nodes", imgui.TableColumnFlags_.none, 0.10)
        imgui.table_setup_column("Usable", imgui.TableColumnFlags_.none, 0.10)
        imgui.table_setup_scroll_freeze(0, 1)
        imgui.table_headers_row()

        for row in self._results:
            row_id = row.get("id", "")
            is_selected = self._selected_id == row_id

            imgui.table_next_row()

            imgui.table_next_column()
            display_name = self._clean_name(row.get("name", row_id))
            clicked, _ = imgui.selectable(
                f"{display_name}##row_{row_id}",
                is_selected,
                imgui.SelectableFlags_.span_all_columns
                | imgui.SelectableFlags_.allow_double_click,
            )
            if clicked:
                self._select_behavior(row_id)
                if imgui.is_mouse_double_clicked(imgui.MouseButton_.left):
                    self._open_selected()

            imgui.table_next_column()
            imgui.text_unformatted(row.get("category", ""))

            imgui.table_next_column()
            imgui.text_unformatted(row.get("source", ""))

            imgui.table_next_column()
            node_count = row.get("node_count")
            imgui.text_unformatted(str(node_count) if node_count is not None else "")

            imgui.table_next_column()
            usable = row.get("usable")
            if usable is not None:
                imgui.text_unformatted("Yes" if usable else "No")

        imgui.end_table()

    # ------------------------------------------------------------------
    # Detail panel
    # ------------------------------------------------------------------

    def _render_details(self) -> None:
        if self._selected_meta is None:
            imgui.text_disabled("Select a behavior to view details.")
            return

        meta = self._selected_meta
        child_height = 130.0
        imgui.begin_child("##browser_details", imgui.ImVec2(0, child_height), imgui.ChildFlags_.borders)

        imgui.text_colored(imgui.ImVec4(0.5, 0.8, 1.0, 1.0), self._clean_name(meta.get("name", "")))
        imgui.same_line()
        imgui.text_disabled(f"  [{meta.get('category', '')} / {meta.get('source', '')}]")

        if imgui.begin_table("##detail_cols", 2, imgui.TableFlags_.none, imgui.ImVec2(0, 0)):
            imgui.table_setup_column("left", imgui.TableColumnFlags_.width_stretch, 0.5)
            imgui.table_setup_column("right", imgui.TableColumnFlags_.width_stretch, 0.5)

            imgui.table_next_row()
            imgui.table_next_column()
            self._render_detail_list(
                "Events", meta.get("events", []), max_show=6
            )

            imgui.table_next_column()
            variables = meta.get("variables", [])
            var_strs = [f"{v['name']} ({v['type']})" for v in variables]
            self._render_detail_list("Variables", var_strs, max_show=6)

            imgui.table_next_row()
            imgui.table_next_column()
            self._render_detail_list(
                "Sequences", meta.get("sequences", []), max_show=6
            )

            imgui.table_next_column()
            transitions = meta.get("transitions", [])
            tr_strs = [
                f"{t['name']} ({float(t['duration']):.2f}s)" if t.get("duration") else t["name"]
                for t in transitions
            ]
            self._render_detail_list("Transitions", tr_strs, max_show=6)

            imgui.end_table()

        imgui.end_child()

    @staticmethod
    def _render_detail_list(label: str, items: list[str], max_show: int = 6) -> None:
        count = len(items)
        imgui.text(f"{label} ({count}):")
        if count == 0:
            imgui.text_disabled("  (none)")
        else:
            for item in items[:max_show]:
                imgui.bullet_text(item)
            if count > max_show:
                imgui.text_disabled(f"  ... and {count - max_show} more")

    # ------------------------------------------------------------------
    # Buttons
    # ------------------------------------------------------------------

    def _render_buttons(self) -> None:
        can_open = self._selected_id is not None
        if not can_open:
            imgui.begin_disabled()
        if imgui.button("Open in Editor"):
            self._open_selected()
        if not can_open:
            imgui.end_disabled()

        imgui.same_line()
        if imgui.button("Close"):
            self._open = False
            imgui.close_current_popup()

        # Show result count on the right
        imgui.same_line(imgui.get_content_region_avail().x - 120)
        imgui.text_disabled(f"{len(self._results)} result(s)")

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _do_search(self) -> None:
        if self._db is None:
            return
        self._results = self._db.search_behaviors(
            query=self._search_text.strip(),
            category=self._category or None,
            source=self._source or None,
            limit=100,
        )
        self._selected_id = None
        self._selected_meta = None

    def _select_behavior(self, behavior_id: str) -> None:
        if self._selected_id == behavior_id:
            return
        self._selected_id = behavior_id
        if self._db is not None:
            self._selected_meta = self._db.get_behavior_metadata(behavior_id)

    def _open_selected(self) -> None:
        if self._selected_id is None or self._db is None:
            return
        xml_path = self._db.get_behavior_xml_path(self._selected_id)
        if xml_path:
            self.open_path = xml_path
            self._open = False
            imgui.close_current_popup()

    @staticmethod
    def _clean_name(name: str) -> str:
        """Strip the common 'Behaviors - ' prefix for cleaner display."""
        if name.startswith("Behaviors - "):
            return name[len("Behaviors - "):]
        return name
