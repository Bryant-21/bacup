"""Shape library browser — gallery grid with FBO-rendered thumbnails."""
from __future__ import annotations

import io
import queue
import sqlite3
from pathlib import Path
from typing import TYPE_CHECKING

import numpy as np
from imgui_bundle import imgui
from PIL import Image

if TYPE_CHECKING:
    from ui.swf_editor.swf_editor_app import SwfEditorApp

_GAMES = [
    ("fo4",      "Fallout 4"),
    ("skyrimse", "Skyrim SE"),
    ("starfield","Starfield"),
    ("fo76",     "Fallout 76"),
    ("fo3",      "Fallout 3"),
    ("fnv",      "Fallout: NV"),
]
_GAME_IDS    = [g for g, _ in _GAMES]
_GAME_LABELS = [l for _, l in _GAMES]

_THUMB_SIZES = (64, 96, 128)
_MAX_RESULTS = 200
_MAX_QUEUE_DRAIN_PER_FRAME = 10  # GL uploads per frame (avoids stutter)


def _db_dir() -> Path:
    try:
        from app.paths import get_db_dir
        return get_db_dir()
    except Exception:
        return Path("data")


class LibraryPanel:
    def __init__(self, app: SwfEditorApp) -> None:
        self.app = app
        self._search_query: str = ""
        self._results: list[dict] = []
        self._game_idx: int = 0
        self._source_filter: int = 0       # 0=All, 1=Base Game, 2=User
        self._category_idx: int = 0        # 0=All, 1+=specific category
        self._categories: list[str] = ["All"]  # populated from DB
        self._dirty: bool = True
        self._thumb_size_idx: int = 1      # index into _THUMB_SIZES -> 96px default
        self._selected_id: int | None = None

        # Thumbnail pipeline
        self._result_queue: queue.Queue = queue.Queue()
        self._loader = None                # ThumbnailLoader, lazy init
        self._cache = None                 # ThumbnailCache, lazy init
        # pending: shape_id -> True while loader is working on it
        self._pending: set[int] = set()
        # texture IDs for current result set
        self._textures: dict[int, int] = {}  # shape_id -> GL texture glo
        self._tex_objects: list = []  # prevent GC of moderngl.Texture objects

    # ------------------------------------------------------------------ draw --

    def draw(self) -> None:
        visible, _ = imgui.begin("Library##swf")
        if not visible:
            imgui.end()
            return

        self._ensure_pipeline()
        self._drain_result_queue()
        self._draw_toolbar()

        if self._dirty:
            self._search()
            self._dirty = False

        imgui.separator()

        db_path = self._db_path()
        if not db_path.exists():
            imgui.text_disabled("Build this library in Settings → Indexes.")
            imgui.end()
            return

        if not self._results:
            imgui.text_disabled("No shapes found.")
            imgui.end()
            return

        self._draw_gallery()
        imgui.end()

    # --------------------------------------------------------------- toolbar --

    def _draw_toolbar(self) -> None:
        # Game selector
        imgui.set_next_item_width(120)
        changed, self._game_idx = imgui.combo(
            "##swf_lib_game", self._game_idx, _GAME_LABELS
        )
        if changed:
            self._dirty = True
            self._category_idx = 0
            self._categories = ["All"]
            self._flush_cache()

        imgui.same_line()
        db_path = self._db_path()
        if db_path.exists():
            size_mb = db_path.stat().st_size // (1024 * 1024)
            imgui.text_colored(imgui.ImVec4(0.4, 0.8, 0.4, 1), f"{size_mb} MB")
        else:
            imgui.text_colored(imgui.ImVec4(0.6, 0.6, 0.6, 1), "Not built")

        imgui.spacing()

        # Search bar
        imgui.set_next_item_width(-1)
        changed, self._search_query = imgui.input_text(
            "##swf_lib_search", self._search_query
        )
        if changed:
            self._dirty = True

        # Category filter
        imgui.set_next_item_width(160)
        cat_changed, self._category_idx = imgui.combo(
            "##swf_lib_cat", self._category_idx, self._categories
        )
        if cat_changed:
            self._dirty = True

        # Source filter
        imgui.same_line()
        imgui.set_next_item_width(90)
        filter_changed, self._source_filter = imgui.combo(
            "##swf_lib_src", self._source_filter, ["All", "Base", "User"]
        )
        if filter_changed:
            self._dirty = True

        # Size toggle
        imgui.text("Size:")
        for i, sz in enumerate(_THUMB_SIZES):
            imgui.same_line()
            if imgui.radio_button(str(sz), self._thumb_size_idx == i):
                self._thumb_size_idx = i
                if self._cache:
                    self._cache.invalidate()
                self._textures.clear()
                for tex in self._tex_objects:
                    tex.release()
                self._tex_objects.clear()
                self._dirty = True

    # --------------------------------------------------------------- gallery --

    def _draw_gallery(self) -> None:
        size = _THUMB_SIZES[self._thumb_size_idx]
        panel_w = imgui.get_content_region_avail().x
        n_cols = max(1, int(panel_w // (size + 8)))

        dl = imgui.get_window_draw_list()

        # Group results by source SWF file
        groups: list[tuple[str, list[dict]]] = []
        current_source = None
        current_group: list[dict] = []
        for row in self._results[:_MAX_RESULTS]:
            src = row.get("source_swf", "")
            if src != current_source:
                if current_group:
                    groups.append((current_source or "", current_group))
                current_source = src
                current_group = [row]
            else:
                current_group.append(row)
        if current_group:
            groups.append((current_source or "", current_group))

        for source_name, group_rows in groups:
            # Collapsible header per source SWF
            label = source_name.replace(".swf", "") if source_name else "Unknown"
            if not imgui.collapsing_header(
                f"{label} ({len(group_rows)})##swf_src_{source_name}",
                imgui.TreeNodeFlags_.default_open,
            ):
                continue

            col = 0
            for row in group_rows:
                shape_id = row["id"]
                tex_id = self._textures.get(shape_id)

                cursor = imgui.get_cursor_screen_pos()

                if tex_id is not None:
                    imgui.image(
                        imgui.ImTextureRef(tex_id),
                        imgui.ImVec2(size, size),
                        uv0=imgui.ImVec2(0, 1),
                        uv1=imgui.ImVec2(1, 0),
                    )
                else:
                    imgui.dummy(imgui.ImVec2(size, size))
                    dl.add_rect_filled(
                        cursor,
                        imgui.ImVec2(cursor.x + size, cursor.y + size),
                        imgui.get_color_u32(imgui.ImVec4(0.2, 0.2, 0.2, 1.0)),
                    )
                    t = imgui.get_time()
                    dots = "." * (1 + int(t * 2) % 3)
                    mid_x = cursor.x + size / 2 - 8
                    mid_y = cursor.y + size / 2 - 7
                    dl.add_text(imgui.ImVec2(mid_x, mid_y), 0xFFAAAAAA, dots)

                # Selection border
                if self._selected_id == shape_id:
                    dl.add_rect(
                        cursor,
                        imgui.ImVec2(cursor.x + size, cursor.y + size),
                        imgui.get_color_u32(imgui.ImVec4(0.2, 0.7, 1.0, 1.0)),
                        0.0, 0, 2.0,
                    )

                if imgui.is_item_clicked(imgui.MouseButton_.left):
                    self._selected_id = shape_id

                if imgui.is_item_hovered():
                    name = row.get("name", "")
                    tags = row.get("tags", "")
                    imgui.set_tooltip(f"{name}\n{source_name}\nTags: {tags}")

                    if imgui.is_mouse_double_clicked(imgui.MouseButton_.left):
                        self.app.place_shape(row)

                # Name label below tile
                name_short = (row.get("name") or "")[:12]
                lx = cursor.x
                ly = cursor.y + size + 1
                dl.add_text(imgui.ImVec2(lx, ly), 0xFFCCCCCC, name_short)

                col += 1
                if col < n_cols:
                    imgui.same_line(spacing=4)
                else:
                    col = 0
                    imgui.dummy(imgui.ImVec2(0, 14))

            # End row if mid-row
            if col > 0:
                imgui.dummy(imgui.ImVec2(0, 14))

    # ---------------------------------------------------------- search/load --

    def _search(self) -> None:
        db_path = self._db_path()
        if not db_path.exists():
            self._results = []
            return

        try:
            conn = sqlite3.connect(str(db_path))
            conn.row_factory = sqlite3.Row

            # Refresh category list from DB
            cat_rows = conn.execute(
                "SELECT DISTINCT tags FROM shapes ORDER BY tags"
            ).fetchall()
            self._categories = ["All"] + [r[0] for r in cat_rows if r[0]]
            if self._category_idx >= len(self._categories):
                self._category_idx = 0

            q = self._search_query.strip()

            # Build category filter clause
            cat_filter = ""
            cat_params: list = []
            if self._category_idx > 0:
                selected_cat = self._categories[self._category_idx]
                cat_filter = " AND tags = ?"
                cat_params = [selected_cat]

            if q:
                rows = conn.execute(
                    "SELECT s.* FROM shapes s "
                    "JOIN shapes_fts f ON s.id = f.rowid "
                    "WHERE shapes_fts MATCH ?" + cat_filter +
                    " ORDER BY s.source_swf, s.source_tag LIMIT 200",
                    [q] + cat_params,
                ).fetchall()
            else:
                if cat_filter:
                    rows = conn.execute(
                        "SELECT * FROM shapes WHERE 1=1" + cat_filter +
                        " ORDER BY source_swf, source_tag LIMIT 200",
                        cat_params,
                    ).fetchall()
                else:
                    rows = conn.execute(
                        "SELECT * FROM shapes ORDER BY source_swf, source_tag LIMIT 200"
                    ).fetchall()

            self._results = []
            for row in rows:
                d = dict(row)
                if self._source_filter == 1 and d.get("user_added", 0):
                    continue
                if self._source_filter == 2 and not d.get("user_added", 0):
                    continue
                self._results.append(d)
            conn.close()
        except Exception:
            self._results = []
            return

        # Cancel any stale loader work
        if self._loader:
            self._loader.clear_pending()
        self._pending.clear()
        self._textures.clear()
        for tex in self._tex_objects:
            tex.release()
        self._tex_objects.clear()

        size = _THUMB_SIZES[self._thumb_size_idx]
        for row in self._results[:_MAX_RESULTS]:
            shape_id = row["id"]
            # Check memory cache first
            if self._cache:
                tex = self._cache.get(shape_id, size)
                if tex is not None:
                    self._textures[shape_id] = tex
                    continue
                # Check disk cache
                png_bytes = self._cache.load_png(shape_id, size)
                if png_bytes:
                    tex_id = self._upload_png(png_bytes, size)
                    if tex_id is not None:
                        self._cache.put(shape_id, size, tex_id,
                                        self._png_to_raw_rgba(png_bytes, size))
                        self._textures[shape_id] = tex_id
                    continue

            # Queue for background tessellation
            blob = row.get("shape_data") or b""
            bounds_raw = row.get("bounds") or "[0,0,100,100]"
            try:
                import json
                bounds = json.loads(bounds_raw)
            except Exception:
                bounds = [0.0, 0.0, 100.0, 100.0]

            if self._loader and blob:
                self._loader.request(shape_id, blob, bounds, size)
                self._pending.add(shape_id)

    def _drain_result_queue(self) -> None:
        """Upload up to N thumbnail results per frame (GL work on main thread)."""
        drained = 0
        size = _THUMB_SIZES[self._thumb_size_idx]
        while drained < _MAX_QUEUE_DRAIN_PER_FRAME:
            try:
                shape_id, result_size, verts, bounds = self._result_queue.get_nowait()
            except queue.Empty:
                break

            self._pending.discard(shape_id)

            # Skip if size changed (stale result)
            if result_size != size:
                drained += 1
                continue

            if verts is None or not self.app.renderer:
                drained += 1
                continue

            try:
                _fbo_glo, png_bytes = self.app.renderer.render_thumbnail_vertices(verts, size)
                # Upload as a NEW dedicated texture (FBO texture is pooled/reused)
                tex_id = self._upload_png(png_bytes, size)
                if tex_id is not None:
                    if self._cache:
                        raw = self._png_to_raw_rgba(png_bytes, size)
                        self._cache.put(shape_id, size, tex_id, raw)
                    self._textures[shape_id] = tex_id
            except Exception:
                pass

            drained += 1

    # ---------------------------------------------------------------- helpers --

    def _ensure_pipeline(self) -> None:
        """Lazy-init loader and cache after GL context is ready."""
        if self._loader is None:
            from ui.swf_editor.thumbnail_loader import ThumbnailLoader
            self._loader = ThumbnailLoader(self._result_queue)

        if self._cache is None:
            from ui.swf_editor.thumbnail_cache import ThumbnailCache
            game = _GAME_IDS[self._game_idx]
            self._cache = ThumbnailCache(game, _db_dir() / "thumbnails")

    def _flush_cache(self, wipe_disk: bool = False) -> None:
        """Rebuild cache for new game selection."""
        if self._cache:
            self._cache.invalidate(disk=wipe_disk)
        self._textures.clear()
        for tex in self._tex_objects:
            tex.release()
        self._tex_objects.clear()
        self._pending.clear()
        # Recreate cache for new game
        from ui.swf_editor.thumbnail_cache import ThumbnailCache
        game = _GAME_IDS[self._game_idx]
        self._cache = ThumbnailCache(game, _db_dir() / "thumbnails")

    def _db_path(self) -> Path:
        return _db_dir() / f"{_GAME_IDS[self._game_idx]}_swf_shapes.db"

    def _upload_png(self, png_bytes: bytes, size: int) -> int | None:
        """Decode PNG bytes -> upload GL texture. Returns texture glo or None."""
        if not self.app.renderer:
            return None
        try:
            img = Image.open(io.BytesIO(png_bytes)).convert("RGBA").resize((size, size))
            raw = img.tobytes()
            tex = self.app.renderer.ctx.texture((size, size), 4, data=raw)
            self._tex_objects.append(tex)  # prevent GC
            return tex.glo
        except Exception:
            return None

    @staticmethod
    def _png_to_raw_rgba(png_bytes: bytes, size: int) -> bytes:
        """Decode PNG -> raw RGBA bytes for ThumbnailCache.put()."""
        img = Image.open(io.BytesIO(png_bytes)).convert("RGBA").resize((size, size))
        return img.tobytes()
