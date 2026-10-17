"""Virtualized vector library with isolated previews and copied personal assets."""
from __future__ import annotations

from collections import OrderedDict
import json
from pathlib import Path
import time

from imgui_bundle import imgui

from ui.swf_editor.library.catalog import Filters, source_stamp
from ui.swf_editor.library.controller import LibraryController

_GAMES = [('fo4', 'Fallout 4'), ('skyrimse', 'Skyrim SE'), ('starfield', 'Starfield'),
          ('fo76', 'Fallout 76'), ('fo3', 'Fallout 3'), ('fnv', 'Fallout: NV')]
_GAME_IDS = [g for g, _ in _GAMES]
_THUMB_SIZES = (64, 96, 128)
_MAX_TEXTURES = 128


def _fit_label(text, width):
    if imgui.calc_text_size(text).x <= width:
        return text
    while text and imgui.calc_text_size(text + '…').x > width:
        text = text[:-1]
    return text + '…'


def _db_dir():
    from app.paths import get_db_dir
    return get_db_dir()


class LibraryPanel:
    def __init__(self, app, *, db_paths=None, cache_dir=None, user_db=None):
        self.app = app
        root = _db_dir()
        self._db_paths = db_paths if db_paths is not None else {game: root / f'{game}_swf_shapes.db' for game in _GAME_IDS}
        self._game_idx = self._source_filter = self._category_idx = self._source_idx = 0
        self._thumb_size_idx = 1
        self._search_query = ''
        self._trace_threshold = 128
        self._categories, self._sources = ['All categories'], ['All source movies']
        self._search_at = None
        self._scroll_reset = False
        self._pending_place = None
        self._textures = OrderedDict()
        self._closed = False
        self.controller = LibraryController(self._db_paths.get('fo4'), user_db or root / 'swf_user_shapes.db',
                                            cache_dir or root / 'swf_library_cache', 'fo4')
        self._last_visible = []
        self._tile_rects = {}
        self._control_rects = {}
        self._last_draw_frame = -1

    def _remember_control(self, name):
        a, b = imgui.get_item_rect_min(), imgui.get_item_rect_max()
        self._control_rects[name] = (a.x, a.y, b.x, b.y)

    def search_now(self):
        self._search_at = None
        self._pending_place = None
        self._scroll_reset = True
        self.controller.search(Filters(self._search_query, self._categories[self._category_idx] if self._category_idx else '',
                                       ('all', 'base', 'user')[self._source_filter], self._sources[self._source_idx] if self._source_idx else ''))

    def set_game(self, index):
        self._game_idx = index
        self._category_idx = self._source_idx = self._source_filter = 0
        self._search_query = ''
        self._categories, self._sources = ['All categories'], ['All source movies']
        self._pending_place = None
        self._scroll_reset = True
        game = _GAME_IDS[index]
        self.controller.set_source(self._db_paths.get(game), game)

    def poll(self):
        self.controller.poll()
        if self._search_at is not None and time.monotonic() >= self._search_at:
            self.search_now()
        self._categories = ['All categories', *self.controller.categories]
        self._sources = ['All source movies', *self.controller.sources]
        self._category_idx = min(self._category_idx, len(self._categories) - 1)
        self._source_idx = min(self._source_idx, len(self._sources) - 1)
        if self._pending_place:
            document, layer, frame, key = self._pending_place
            selected = self.controller.selection
            if (self.app.document is not document or self.app.scene.active_layer_index != layer or self.app.scene.current_frame != frame
                    or selected is None or selected.key != key or self.app._pending_action is not None):
                self._pending_place = None
            elif self.controller.selection_error:
                self.app.report_error(self.controller.selection_error)
                self._pending_place = None
            elif self.controller.shape is not None:
                if source_stamp(Path(selected.source_path)) != selected.source_stamp:
                    self.controller.search(reset_selection=True)
                    self.app.report_error('Library source changed; select the refreshed asset before placing')
                else:
                    self.app.place_library_asset(self.controller.shape, selected.name)
                self._pending_place = None

    def request_place(self, asset=None):
        if self.app.document.mode != 'visual':
            self.app.report_error('Library placement is available in vector artwork projects')
            return
        asset = asset or self.controller.selection
        if asset is None:
            return
        self.controller.select(asset)
        self._pending_place = (self.app.document, self.app.scene.active_layer_index, self.app.scene.current_frame, asset.key)
        self.poll()

    def draw(self):
        self._last_draw_frame = imgui.get_frame_count()
        self.poll()
        visible, _ = imgui.begin('Library##swf')
        if not visible:
            self.controller.pause()
            imgui.end()
            return
        try:
            self._toolbar()
            imgui.separator()
            c = self.controller
            imgui.text_disabled(c.status)
            if c.error:
                imgui.text_wrapped(c.error)
                if imgui.button('Retry library'):
                    c.retry()
            elif not c.total and not c.loading:
                imgui.text_wrapped('No matching assets. Import your artwork below or build game libraries in Settings > Indexes.')
            self._gallery()
            imgui.separator()
            imgui.begin_child('##swf_lib_details', imgui.ImVec2(0, 0))
            try:
                self._details()
            finally:
                imgui.end_child()
        finally:
            imgui.end()

    def _toolbar(self):
        imgui.set_next_item_width(max(110, imgui.get_content_region_avail().x - 100))
        changed, index = imgui.combo('##swf_lib_game', self._game_idx, [label for _, label in _GAMES])
        self._remember_control('game')
        if changed:
            self.set_game(index)
        imgui.same_line()
        imgui.set_next_item_width(85)
        changed, self._thumb_size_idx = imgui.combo('##swf_lib_size', self._thumb_size_idx, [f'{s} px' for s in _THUMB_SIZES])
        self._remember_control('size')
        if changed:
            self.controller.pause()
        imgui.set_next_item_width(-1)
        changed, self._search_query = imgui.input_text_with_hint('##swf_lib_search', 'Search names, tags, source movies', self._search_query)
        self._remember_control('search')
        if changed:
            self._search_at = time.monotonic() + .15
        width = imgui.get_content_region_avail().x
        imgui.set_next_item_width(max(100, width - 93))
        changed, self._category_idx = imgui.combo('##swf_lib_cat', self._category_idx, self._categories)
        self._remember_control('category')
        imgui.same_line()
        imgui.set_next_item_width(85)
        origin_changed, self._source_filter = imgui.combo('##swf_lib_origin', self._source_filter, ['All', 'Base', 'User'])
        self._remember_control('origin')
        imgui.set_next_item_width(-1)
        source_changed, self._source_idx = imgui.combo('##swf_lib_source', self._source_idx, self._sources)
        self._remember_control('source')
        if changed or origin_changed or source_changed:
            self.search_now()

    def _texture(self, key, size):
        if key in self._textures:
            self._textures.move_to_end(key)
            return self._textures[key]
        pixels = self.controller.pixels.get(key)
        if pixels is None or key in self.controller.errors or self.app.ctx is None or self._uploads >= 2:
            return None
        self._uploads += 1
        try:
            texture = self.app.ctx.texture((size, size), 4, pixels)
            self._textures[key] = texture
            while len(self._textures) > _MAX_TEXTURES:
                _, old = self._textures.popitem(last=False)
                old.release()
            return texture
        except Exception as exc:
            self.controller.errors[key] = f'Cannot upload preview: {exc}'
            return None

    def _gallery(self):
        c = self.controller
        size = _THUMB_SIZES[self._thumb_size_idx]
        self._uploads = 0
        self._tile_rects.clear()
        wanted = []
        height = max(100, imgui.get_content_region_avail().y - (250 if c.selection else 210))
        imgui.begin_child('##swf_lib_grid', imgui.ImVec2(0, height))
        try:
            position = imgui.get_window_pos()
            dimensions = imgui.get_window_size()
            self._grid_bounds = (position.x, position.y, position.x + dimensions.x, position.y + dimensions.y)
            if self._scroll_reset:
                imgui.set_scroll_y(0)
                self._scroll_reset = False
            width = imgui.get_content_region_avail().x
            gap = imgui.get_style().item_spacing.x
            columns = max(1, int((width + gap) // (size + gap)))
            row_height = size + imgui.get_text_line_height() * 2 + imgui.get_style().item_spacing.y
            clipper = imgui.ListClipper()
            clipper.begin((c.total + columns - 1) // columns, row_height)
            while clipper.step():
                for row in range(clipper.display_start, clipper.display_end):
                    for column in range(columns):
                        index = row * columns + column
                        if index >= c.total:
                            break
                        asset = c.asset_at(index)
                        cursor = imgui.get_cursor_screen_pos()
                        imgui.invisible_button(f'asset##{index}', imgui.ImVec2(size, row_height - imgui.get_style().item_spacing.y))
                        dl = imgui.get_window_draw_list()
                        end = imgui.ImVec2(cursor.x + size, cursor.y + size)
                        dl.add_rect_filled(cursor, end, 0xff292c32, 5)
                        key = (asset.key, size) if asset else None
                        texture = self._texture(key, size) if key else None
                        if texture:
                            dl.add_image(imgui.ImTextureRef(texture.glo), cursor, end)
                        else:
                            label = 'Unavailable' if key in c.errors else 'Loading…'
                            dl.add_text(imgui.ImVec2(cursor.x + 5, cursor.y + size / 2 - 7), 0xff999999, label)
                        if asset:
                            wanted.append(asset)
                            self._tile_rects[asset.key] = (cursor.x, cursor.y, cursor.x + size, cursor.y + size)
                            if c.selection and c.selection.key == asset.key:
                                dl.add_rect(cursor, end, 0xffffb34d, rounding=5, thickness=2)
                            if imgui.is_item_clicked():
                                c.select(asset)
                            if imgui.is_item_hovered():
                                imgui.set_tooltip(f'{asset.name}\n{asset.source_swf} • character {asset.source_tag}\n{asset.tags}\nDouble-click to place' +
                                                  ('\n' + c.errors[key] if key in c.errors else '') +
                                                  ('\n' + c.warnings[asset.key] if asset.key in c.warnings else ''))
                                if imgui.is_mouse_double_clicked(imgui.MouseButton_.left):
                                    self.request_place(asset)
                            label = _fit_label(asset.name, size)
                            source = Path(asset.source_swf).stem
                            dl.add_text(imgui.ImVec2(cursor.x, end.y + 1), 0xffdddddd, label)
                            dl.add_text(imgui.ImVec2(cursor.x, end.y + 1 + imgui.get_text_line_height()), 0xff999999, _fit_label(source, size))
                        if column < columns - 1 and index + 1 < c.total:
                            imgui.same_line()
        finally:
            imgui.end_child()
        self._last_visible = wanted
        c.want_previews(wanted, size)

    def _details(self):
        c = self.controller
        self._control_rects.pop('cancel', None)
        if c.importing:
            if imgui.button('Cancel library import'):
                c.cancel_import()
            self._remember_control('cancel')
        asset = c.selection
        if asset:
            imgui.text_wrapped(asset.name)
            imgui.text_disabled(f'{asset.source_swf} • #{asset.source_tag} • {asset.geometry_bytes // 1024:,} KB')
            if c.selection_error:
                imgui.text_wrapped(c.selection_error)
            elif asset.key in c.warnings:
                imgui.text_wrapped(c.warnings[asset.key])
        else:
            imgui.text_wrapped('Select an asset to inspect. Double-click to place.')
        imgui.begin_disabled(asset is None or self.app.document.mode != 'visual')
        if imgui.button('Place selected'):
            self.request_place()
        self._remember_control('place')
        imgui.end_disabled()
        imgui.same_line()
        imgui.begin_disabled(c.importing)
        if imgui.button('Import artwork…'):
            self.choose_import()
        self._remember_control('import')
        imgui.end_disabled()
        selected = self.app.scene.selection
        imgui.begin_disabled(c.importing or self.app.document.mode != 'visual' or not selected)
        if imgui.button('Save selected shapes to User library'):
            self.save_selection()
        self._remember_control('save_selection')
        if imgui.is_item_hovered():
            imgui.set_tooltip('Copy each selected shape’s original geometry and paint. Use Save project to preserve its canvas position, rotation, scale and instance opacity.')
        imgui.end_disabled()
        imgui.set_next_item_width(-1)
        _, self._trace_threshold = imgui.slider_int('##swf_trace_threshold', self._trace_threshold, 0, 255, 'Raster threshold: %d')
        if imgui.is_item_hovered():
            imgui.set_tooltip('Raster import traces bright artwork on a dark background. SVG and SWF keep their vector geometry.')
        if c.import_status:
            imgui.text_wrapped(c.import_status)
        if self._pending_place and c.geometry is None:
            imgui.text_disabled('Loading selected geometry…')
        if c.selection_error and imgui.button('Retry selected asset'):
            c.retry()

    def choose_import(self):
        try:
            from creation_lib.ui.widgets.pick_folder import pick_file
            filename = pick_file('Copy vector artwork into User library', [('Artwork', '*.svg;*.png;*.jpg;*.jpeg;*.bmp;*.dds;*.swf')])
            if filename:
                self.controller.request_import(filename, self._trace_threshold)
        except Exception as exc:
            self.controller.import_status = f'Import failed: {exc}'

    def save_selection(self):
        try:
            from creation_lib.swf.library_assets import encoded_geometry
            entries = []
            for li, ei in self.app.scene.selection:
                layer = self.app.scene.layers[li]
                keyframe = layer.keyframe_at(self.app.scene.current_frame)
                shape = self.app.scene.library_symbols[keyframe.display_list[ei].shape_id]
                entries.append((f'{layer.name} #{shape.shape_id}', json.dumps(encoded_geometry(shape)).encode(),
                                self.app.document.path.name if self.app.document.path else 'Canvas artwork', 'Saved'))
            self.controller.save_assets(entries)
        except Exception as exc:
            self.controller.import_status = f'Cannot save selection: {exc}'

    def pause(self):
        self._pending_place = None
        self.controller.pause()

    def background_tick(self):
        """Drain completed jobs even when another dock tab hides the gallery."""
        if imgui.get_frame_count() - self._last_draw_frame > 1:
            self.pause()
        self.poll()

    def close(self):
        if self._closed:
            return
        self._closed = True
        self.controller.close()
        for texture in self._textures.values():
            texture.release()
        self._textures.clear()
