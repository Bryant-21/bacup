"""Modal popup for browsing and searching NIF files from the indexed database."""

from __future__ import annotations

import logging
import os
import re

from imgui_bundle import imgui

from creation_lib.db.store import GameDataStore
from creation_lib.db.db import open_db
from ui.editor.nif_file_types import is_nif_like_path

_log = logging.getLogger("nif_editor.nif_browser")

# Regex to extract integer from AddOnNodeX names
_ADDON_NODE_RE = re.compile(r"AddOnNode(\d+)", re.IGNORECASE)

_GAME_OPTIONS: list[tuple[str, str]] = [
    ("fo4", "Fallout 4"),
    ("fo3", "Fallout 3"),
    ("fnv", "Fallout: New Vegas"),
    ("skyrimse", "Skyrim SE"),
    ("fo76", "Fallout 76"),
    ("starfield", "Starfield"),
]


class NifBrowserDialog:
    """ImGui modal popup for browsing NIFs stored in the SQLite index.

    Usage::

        browser = NifBrowserDialog()
        # To open:
        browser.open(my_store)
        # Each frame:
        browser.render()
        if browser.open_path:
            load_nif(browser.open_path)
    """

    def __init__(self) -> None:
        self._open: bool = False
        self._pending_open: bool = False
        self._store: GameDataStore | None = None
        self._game: str = "fo4"
        self._search_text: str = ""
        self._category: str = ""  # empty = all
        self._source: str = ""  # empty = all
        self._results: list[dict] = []
        self._selected_id: str | None = None
        self._selected_detail: dict | None = None
        self._categories: list[str] = []
        self._sources: list[str] = []
        self._addn_cache: dict[str, list[dict]] = {}  # nif_id -> ADDN info list
        self._connect_points: list[str] = []

        # Pagination
        self._page: int = 0
        self._page_size: int = 100
        self._total_count: int = 0

        # Output signals — checked by app.py each frame
        self.open_path: str | None = None
        self.attach_info: tuple[str, str] | None = None  # (nif_path, cp_name)
        self.bash_path: str | None = None  # path to bash into current NIF
        self.addn_open_path: str | None = None

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def open(self, game_id: str = "fo4") -> None:
        """Open the browser dialog for the given game, populate filters, run initial search."""
        self._open = True
        self._pending_open = True
        self._game = game_id
        self._search_text = ""
        self._category = ""
        self._source = ""
        self._selected_id = None
        self._selected_detail = None
        self._page = 0
        self._addn_cache.clear()
        self._reload_store()

    def _reload_store(self) -> None:
        """Recreate the data store for the current game and refresh filters."""
        from app.paths import get_db_dir
        self._store = GameDataStore(db_dir=str(get_db_dir()), game=self._game)
        try:
            cat_counts = self._store.list_items("nifs", "category")
            self._categories = sorted(cat_counts.keys())
        except Exception:
            self._categories = []
        try:
            src_counts = self._store.list_items("nifs", "source")
            self._sources = sorted(src_counts.keys())
        except Exception:
            self._sources = []
        self._category = ""
        self._source = ""
        self._do_search()

    def render(self) -> None:
        """Call each frame. Draws the modal popup when open."""
        if not self._open:
            return

        # Reset output signals each frame
        self.open_path = None
        self.attach_info = None
        self.bash_path = None
        self.addn_open_path = None

        if self._pending_open:
            imgui.open_popup("Browse NIFs##nif_browser")
            self._pending_open = False

        center = imgui.get_main_viewport().get_center()
        imgui.set_next_window_pos(center, imgui.Cond_.appearing, imgui.ImVec2(0.5, 0.5))
        imgui.set_next_window_size(imgui.ImVec2(1220, 760), imgui.Cond_.appearing)

        opened, _ = imgui.begin_popup_modal("Browse NIFs##nif_browser")
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
        cat_width = 180.0
        game_width = 220.0
        spacing = imgui.get_style().item_spacing.x
        search_width = avail_width - (cat_width + spacing) - (game_width + spacing)
        if search_width < 100.0:
            search_width = 100.0

        imgui.set_next_item_width(search_width)
        changed, self._search_text = imgui.input_text_with_hint(
            "##nif_search", "Search NIFs...", self._search_text
        )
        if changed:
            self._do_search()

        imgui.same_line()
        imgui.set_next_item_width(cat_width)
        if imgui.begin_combo("##nif_category", self._category or "All Categories"):
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
        imgui.set_next_item_width(game_width)
        current_game_label = next((n for g, n in _GAME_OPTIONS if g == self._game), self._game)
        if imgui.begin_combo("##nif_game", current_game_label):
            for g_id, g_name in _GAME_OPTIONS:
                selected, _ = imgui.selectable(g_name, self._game == g_id)
                if selected and g_id != self._game:
                    self._game = g_id
                    self._addn_cache.clear()
                    self._reload_store()
            imgui.end_combo()

    # ------------------------------------------------------------------
    # Results table
    # ------------------------------------------------------------------

    def _render_results_table(self) -> None:
        detail_height = 190.0 if self._selected_detail else 0.0
        button_row_height = imgui.get_frame_height_with_spacing() * 2
        table_height = (
            imgui.get_content_region_avail().y
            - detail_height
            - button_row_height
            - 2 * (imgui.get_style().item_spacing.y + 1)
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

        if not imgui.begin_table("##nif_results", 5, flags, imgui.ImVec2(0, table_height)):
            return

        imgui.table_setup_column("Name", imgui.TableColumnFlags_.default_sort, 0.35)
        imgui.table_setup_column("Category", imgui.TableColumnFlags_.none, 0.15)
        imgui.table_setup_column("Source", imgui.TableColumnFlags_.none, 0.15)
        imgui.table_setup_column("Blocks", imgui.TableColumnFlags_.none, 0.10)
        imgui.table_setup_column("Features", imgui.TableColumnFlags_.none, 0.15)
        imgui.table_setup_scroll_freeze(0, 1)
        imgui.table_headers_row()

        for row in self._results:
            row_id = row.get("id", "")
            is_selected = self._selected_id == row_id

            imgui.table_next_row()

            imgui.table_next_column()
            display_name = row.get("name", row_id)
            clicked, _ = imgui.selectable(
                f"{display_name}##row_{row_id}",
                is_selected,
                imgui.SelectableFlags_.span_all_columns
                | imgui.SelectableFlags_.allow_double_click,
            )
            if clicked:
                self._select_nif(row_id)
                if imgui.is_mouse_double_clicked(imgui.MouseButton_.left):
                    self._open_selected()

            imgui.table_next_column()
            imgui.text_unformatted(row.get("category", ""))

            imgui.table_next_column()
            imgui.text_unformatted(row.get("source", ""))

            imgui.table_next_column()
            block_count = row.get("block_count")
            imgui.text_unformatted(str(block_count) if block_count is not None else "")

            imgui.table_next_column()
            features = []
            if row.get("has_particles"):
                features.append("particles")
            if row.get("has_behavior"):
                features.append("behavior")
            if row.get("has_controllers"):
                features.append("controllers")
            imgui.text_unformatted(", ".join(features) if features else "")

        imgui.end_table()

    # ------------------------------------------------------------------
    # Detail panel
    # ------------------------------------------------------------------

    def _render_details(self) -> None:
        if self._selected_detail is None:
            imgui.text_disabled("Select a NIF to view details.")
            return

        detail = self._selected_detail
        child_height = 180.0
        imgui.begin_child("##nif_details", imgui.ImVec2(0, child_height), imgui.ChildFlags_.borders)

        # Header
        name = detail.get("name", "")
        imgui.text_colored(imgui.ImVec4(0.5, 0.8, 1.0, 1.0), name)
        imgui.same_line()
        imgui.text_disabled(f"  [{detail.get('category', '')} / {detail.get('source', '')}]")
        imgui.same_line()
        path = detail.get("path", "")
        if path:
            imgui.text_disabled(f"  {path}")

        if imgui.begin_table("##detail_cols", 2, imgui.TableFlags_.none, imgui.ImVec2(0, 0)):
            imgui.table_setup_column("left", imgui.TableColumnFlags_.width_stretch, 0.5)
            imgui.table_setup_column("right", imgui.TableColumnFlags_.width_stretch, 0.5)

            imgui.table_next_row()
            imgui.table_next_column()
            self._render_detail_list("Block Types", detail.get("block_types", []), max_show=6)

            imgui.table_next_column()
            self._render_detail_list("Textures", detail.get("textures", []), max_show=4)

            imgui.table_next_row()
            imgui.table_next_column()
            self._render_detail_list("Materials", detail.get("materials", []), max_show=4)

            imgui.table_next_column()
            # AddOnNodes
            addn_list = detail.get("addon_nodes", [])
            addn_strs = []
            for a in addn_list:
                label = f"#{a['node_index']} -> {a['editor_id']}"
                if a.get("model"):
                    label += " (Model)"
                if a.get("light"):
                    label += " (Light)"
                addn_strs.append(label)
            if addn_strs:
                self._render_detail_list("AddOnNodes", addn_strs, max_show=4)
            else:
                sequences = detail.get("sequences", [])
                self._render_detail_list("Sequences", sequences, max_show=4)

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
        has_addn = bool(
            self._selected_detail
            and self._selected_detail.get("addon_nodes")
            and any(a.get("model") for a in self._selected_detail["addon_nodes"])
        )

        if not can_open:
            imgui.begin_disabled()
        if imgui.button("Open in Editor"):
            self._open_selected()
        if not can_open:
            imgui.end_disabled()

        imgui.same_line()
        # Attach to CP — uses connect points from the app's currently loaded NIF
        self._render_attach_button()

        imgui.same_line()
        if not can_open:
            imgui.begin_disabled()
        if imgui.button("Bash"):
            self._bash_selected()
        if not can_open:
            imgui.end_disabled()

        imgui.same_line()
        if not has_addn:
            imgui.begin_disabled()
        if imgui.button("Load ADDN NIF"):
            self._load_addn_nif()
        if not has_addn:
            imgui.end_disabled()

        imgui.same_line()
        if imgui.button("Close"):
            self._open = False
            imgui.close_current_popup()

        # Pagination controls — right-aligned
        self._render_pagination()

    def _render_pagination(self) -> None:
        """Render prev/next page controls and result count, right-aligned."""
        total = self._total_count
        page = self._page
        page_size = self._page_size
        first = page * page_size + 1 if self._results else 0
        last = page * page_size + len(self._results)

        has_prev = page > 0
        # For FTS we use a sentinel: total = last+1 means "at least one more page"
        has_next = total > last

        # Build info string
        query = self._search_text.strip()
        if not query and total > 0:
            info = f"{first}-{last} of {total}"
        elif self._results:
            info = f"{first}-{last}" + (" (more)" if has_next else "")
        else:
            info = "0 results"

        # Right-align the pagination block
        btn_w = imgui.calc_text_size("< Prev").x + imgui.get_style().frame_padding.x * 2
        next_w = imgui.calc_text_size("Next >").x + imgui.get_style().frame_padding.x * 2
        info_w = imgui.calc_text_size(info).x
        spacing = imgui.get_style().item_spacing.x
        block_w = btn_w + spacing + next_w + spacing + info_w
        cursor_x = imgui.get_cursor_pos_x()
        avail_x = imgui.get_content_region_avail().x + cursor_x
        start_x = avail_x - block_w
        if start_x > cursor_x + spacing:
            imgui.same_line(start_x)
        else:
            imgui.same_line()

        if not has_prev:
            imgui.begin_disabled()
        if imgui.button("< Prev"):
            self._page -= 1
            self._do_search(reset_page=False)
            self._selected_id = None
            self._selected_detail = None
        if not has_prev:
            imgui.end_disabled()

        imgui.same_line()
        if not has_next:
            imgui.begin_disabled()
        if imgui.button("Next >"):
            self._page += 1
            self._do_search(reset_page=False)
            self._selected_id = None
            self._selected_detail = None
        if not has_next:
            imgui.end_disabled()

        imgui.same_line()
        imgui.text_disabled(info)

    def _render_attach_button(self) -> None:
        """Render 'Attach to CP' dropdown button."""
        can_attach = self._selected_id is not None and bool(self._connect_points)
        if not can_attach:
            imgui.begin_disabled()

        imgui.set_next_item_width(130.0)
        if imgui.begin_combo("##attach_cp", "Attach to CP"):
            for cp_name in self._connect_points:
                if imgui.selectable(cp_name, False)[0]:
                    self._attach_selected(cp_name)
            imgui.end_combo()

        if not can_attach:
            imgui.end_disabled()

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def set_connect_points(self, cp_names: list[str]) -> None:
        """Set available connect point names from the editor's current NIF."""
        self._connect_points = cp_names

    def _do_search(self, reset_page: bool = True) -> None:
        if self._store is None:
            return

        if reset_page:
            self._page = 0

        query = self._search_text.strip()
        filters = {}
        if self._category:
            filters["category"] = self._category
        offset = self._page * self._page_size
        if query:
            # Fetch one extra to detect whether more pages exist
            hits = self._store.search(
                "nifs", query, limit=self._page_size + 1, offset=offset, **filters
            )
            has_more = len(hits) > self._page_size
            self._results = hits[: self._page_size]
            # Total: known pages so far + 1 if more exist
            if has_more:
                self._total_count = offset + self._page_size + 1  # sentinel: at least one more
            else:
                self._total_count = offset + len(self._results)
        else:
            # Empty query — direct SQL for browsing all NIFs
            self._results, self._total_count = self._browse_all(filters)

        if reset_page:
            self._selected_id = None
            self._selected_detail = None

    def _browse_all(self, filters: dict) -> tuple[list[dict], int]:
        """Browse all NIFs when search text is empty. Returns (results, total_count)."""
        if self._store is None:
            return [], 0
        try:
            db_path = self._store._db_path("nifs")
            conn = open_db(db_path)
            wheres = []
            params: list = []
            if filters.get("category"):
                wheres.append("category = ?")
                params.append(filters["category"])
            if filters.get("source"):
                wheres.append("source = ?")
                params.append(filters["source"])
            where_clause = (" WHERE " + " AND ".join(wheres)) if wheres else ""

            total = conn.execute(
                f"SELECT COUNT(*) FROM nifs{where_clause}", params
            ).fetchone()[0]

            offset = self._page * self._page_size
            page_params = params + [self._page_size, offset]
            rows = conn.execute(
                f"SELECT * FROM nifs{where_clause} ORDER BY name LIMIT ? OFFSET ?",
                page_params,
            ).fetchall()
            return [dict(r) for r in rows], total
        except Exception as e:
            _log.warning("NIF browse failed: %s", e)
            return [], 0

    def _select_nif(self, nif_id: str) -> None:
        if self._selected_id == nif_id:
            return
        self._selected_id = nif_id
        self._selected_detail = self._fetch_detail(nif_id)

    def _fetch_detail(self, nif_id: str) -> dict | None:
        """Fetch rich detail data for a NIF by joining related tables."""
        if self._store is None:
            return None
        try:
            # Get base record
            base = self._store.get_content("nifs", nif_id)
            if not base:
                return None

            db_path = self._store._db_path("nifs")
            conn = open_db(db_path)

            # Block types
            rows = conn.execute(
                "SELECT type_name, count FROM nif_block_types WHERE nif_id = ? ORDER BY count DESC",
                (nif_id,),
            ).fetchall()
            base["block_types"] = [f"{r['type_name']}({r['count']})" for r in rows]
            block_type_names = {r["type_name"] for r in rows}

            # Textures
            rows = conn.execute(
                "SELECT texture_path FROM nif_textures WHERE nif_id = ?", (nif_id,)
            ).fetchall()
            base["textures"] = [os.path.basename(r["texture_path"]) for r in rows]

            # Materials
            rows = conn.execute(
                "SELECT material_path FROM nif_materials WHERE nif_id = ?", (nif_id,)
            ).fetchall()
            base["materials"] = [os.path.basename(r["material_path"]) for r in rows]

            # Sequences
            rows = conn.execute(
                "SELECT sequence_name FROM nif_sequences WHERE nif_id = ?", (nif_id,)
            ).fetchall()
            base["sequences"] = [r["sequence_name"] for r in rows]

            # AddOnNode cross-reference
            base["addon_nodes"] = []
            if "BSValueNode" in block_type_names:
                base["addon_nodes"] = self._fetch_addon_nodes(nif_id, base)

            return base
        except Exception as e:
            _log.warning("NIF detail fetch failed: %s", e)
            return None

    def _fetch_addon_nodes(self, nif_id: str, base: dict) -> list[dict]:
        """Lazy-load BSValueNode data from the NIF and cross-reference ADDN records."""
        # Check cache
        if nif_id in self._addn_cache:
            return self._addn_cache[nif_id]

        results = []
        abs_path = base.get("abs_path") or base.get("source_path")
        if not abs_path:
            return results

        # If abs_path is just the source dir, construct full path
        nif_path = base.get("path", "")
        if abs_path and nif_path and not is_nif_like_path(abs_path):
            full_path = os.path.join(abs_path, nif_path)
        else:
            full_path = abs_path

        if not os.path.isfile(full_path):
            return results

        try:
            from creation_lib.nif.nif_file import NifFile
            nif = NifFile.load(full_path)

            for block in nif.blocks:
                if block.type_name != "BSValueNode":
                    continue
                name = block.get_field("Name") or ""
                value = block.get_field("Value")
                if value is None:
                    # Try parsing from name
                    m = _ADDON_NODE_RE.match(name)
                    if m:
                        value = int(m.group(1))
                if value is None:
                    continue

                # Look up ADDN record by node_index
                addn_info = self._lookup_addn(int(value))
                if addn_info:
                    results.append(addn_info)
                else:
                    results.append({
                        "node_index": int(value),
                        "editor_id": f"(unknown ADDN #{value})",
                        "model": None,
                        "light": None,
                        "sound": None,
                    })
        except Exception as e:
            _log.warning("Failed to load BSValueNode data from %s: %s", full_path, e)

        self._addn_cache[nif_id] = results
        return results

    def _lookup_addn(self, node_index: int) -> dict | None:
        """Look up an ADDN record by node_index using direct SQL."""
        if self._store is None:
            return None
        try:
            db_path = self._store._db_path("records")
            conn = open_db(db_path)
            row = conn.execute(
                "SELECT * FROM records WHERE record_type = 'AddonNodes' AND node_index = ?",
                (node_index,),
            ).fetchone()
            if not row:
                return None

            result = {
                "node_index": node_index,
                "editor_id": row["editor_id"],
                "form_key": row["form_key"],
                "model": None,
                "light": None,
                "sound": None,
            }

            # Parse YAML content for Model, Light, Sound
            content = row["content"] if "content" in row.keys() else ""
            if content:
                import yaml
                try:
                    data = yaml.safe_load(content)
                    if isinstance(data, dict):
                        model = data.get("Model")
                        if isinstance(model, dict):
                            result["model"] = model.get("File")
                        elif isinstance(model, str):
                            result["model"] = model
                        result["light"] = data.get("Light")
                        result["sound"] = data.get("Sound")
                except Exception:
                    pass

            return result
        except Exception as e:
            _log.warning("ADDN lookup failed for index %d: %s", node_index, e)
            return None

    def _open_selected(self) -> None:
        if self._selected_id is None or self._selected_detail is None:
            return
        abs_path = self._get_selected_abs_path()
        if abs_path:
            self.open_path = abs_path
            self._open = False
            imgui.close_current_popup()

    def _attach_selected(self, cp_name: str) -> None:
        if self._selected_id is None or self._selected_detail is None:
            return
        abs_path = self._get_selected_abs_path()
        if abs_path:
            self.attach_info = (abs_path, cp_name)
            self._open = False
            imgui.close_current_popup()

    def _bash_selected(self) -> None:
        if self._selected_id is None or self._selected_detail is None:
            return
        abs_path = self._get_selected_abs_path()
        if abs_path:
            self.bash_path = abs_path
            self._open = False
            imgui.close_current_popup()

    def _load_addn_nif(self) -> None:
        """Open the first ADDN's Model NIF as read-only."""
        if not self._selected_detail:
            return
        for addn in self._selected_detail.get("addon_nodes", []):
            model = addn.get("model")
            if model:
                # Resolve against FO4 meshes directory
                source_path = self._selected_detail.get("source_path", "")
                if source_path:
                    full = os.path.join(source_path, model)
                    if os.path.isfile(full):
                        self.addn_open_path = full
                        self._open = False
                        imgui.close_current_popup()
                        return
                _log.warning("ADDN model NIF not found: %s", model)
                return

    def _get_selected_abs_path(self) -> str | None:
        """Resolve the absolute path of the selected NIF."""
        if not self._selected_detail:
            return None
        abs_path = self._selected_detail.get("abs_path")
        if abs_path and os.path.isfile(abs_path):
            return abs_path
        # Construct from source_path + path
        source_path = self._selected_detail.get("source_path", "")
        path = self._selected_detail.get("path", "")
        if source_path and path:
            full = os.path.join(source_path, path)
            if os.path.isfile(full):
                return full
        return None
