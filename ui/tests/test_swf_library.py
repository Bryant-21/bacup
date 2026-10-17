"""Real catalog, process, renderer and transaction checks on disposable artwork."""
from __future__ import annotations

from contextlib import closing
import hashlib
import json
import sqlite3
from threading import Event
import time

from PIL import Image
import pytest

from creation_lib.preprocessor.swf import CREATE_TABLES_DDL, serialize_shape_records
from creation_lib.swf.library_assets import decode_geometry, encoded_geometry
from creation_lib.swf.shapes import StyleChange
from creation_lib.swf.svg_io import shape_to_svg, svg_to_shapes
from creation_lib.swf.types import FillStyle, Gradient, GradientRecord, LineStyle2, MATRIX, RGBA
from ui.swf_editor.library.catalog import Catalog, Cancelled, Filters, add_personal, payload
from ui.swf_editor.library.controller import LibraryController, MAX_PAGES, MAX_PIXELS_BYTES
from ui.swf_editor.library.worker import import_file, preview
from ui.swf_editor.swf_editor_app import SwfEditorApp
from ui.tests.benchmark_swf_library import make_fixture
from ui.tests.test_swf_editor_workflow import SVG


def shape():
    return svg_to_shapes(SVG)[0]


def put(path, item, name='B21_Alpha'):
    with closing(sqlite3.connect(path)) as conn, conn:
        conn.executescript(CREATE_TABLES_DDL)
        conn.execute('INSERT INTO shapes(name,source_swf,source_tag,shape_data,tags,user_added) VALUES (?,?,?,?,?,?)',
                     (name, 'B21_Test.swf', 1, json.dumps(encoded_geometry(item)).encode(), 'Perk/Alpha', 0))
        conn.execute("INSERT INTO shapes_fts(shapes_fts) VALUES ('rebuild')")


def catalog(tmp_path, base):
    return Catalog(base, tmp_path / 'personal.db', tmp_path / 'metadata', 'fo4')


def settle(controller, condition, seconds=15):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        controller.poll()
        if condition():
            return
        if controller.error:
            raise AssertionError(controller.error)
        time.sleep(.005)
    raise AssertionError(f'Timed out: {controller.status}, {controller.selection_error}, {controller.import_status}, {dict(controller.errors)}')


def test_metadata_pages_all_assets_filter_before_paging_and_literal_queries(tmp_path):
    base = tmp_path / 'large.db'
    make_fixture(base, 25000)
    before = (base.stat().st_size, base.stat().st_mtime_ns)
    c = catalog(tmp_path, base)
    page = c.query(Filters())
    assert page.total == 25000 and len(page.assets) == 128
    assert c.query(Filters(origin='user')).total == 500
    assert all(a.user_added for a in c.query(Filters(origin='user')).assets)
    last = c.query(Filters(), 24960)
    assert len(last.assets) == 40 and last.assets[-1].name == 'B21_Asset024999'
    assert c.query(Filters(query='B21_Asset02499')).total == 10
    assert c.query(Filters(query='" B21_Asset024999 OR:*')).total == 0
    assert c.query(Filters(query='%%%')).total == 0
    assert c.query(Filters(category='Perk')).total == 25000
    assert c.query(Filters(source='B21_Source0249.swf')).total == 100
    assert (base.stat().st_size, base.stat().st_mtime_ns) == before
    with closing(sqlite3.connect(c.snapshot()[0])) as connection:
        assert 'shape_data' not in {r[1] for r in connection.execute('PRAGMA table_info(assets)')}


def test_full_geometry_survives_index_preview_placement_save_and_undo(tmp_path):
    item = shape()
    item.shape_version = 4
    item.shape_flags = 7
    item.edge_bounds = item.bounds
    item.fill_styles = [FillStyle(0x13, gradient_matrix=MATRIX(scale_x=.0625, scale_y=.0625),
        gradient=Gradient(records=[GradientRecord(0, RGBA(255, 0, 0, 50)), GradientRecord(255, RGBA(0, 0, 255, 220))]), focal_point=.5)]
    item.line_styles = [LineStyle2(20, start_cap=1, end_cap=2, join_style=2, miter_limit=2, color=RGBA(0, 255, 0, 100))]
    item.records[0].line = 1
    item.records.insert(-1, StyleChange(new_fill_styles=[FillStyle(0, RGBA(200, 200, 200, 75))], new_line_styles=[]))
    blob = serialize_shape_records(item)
    restored, warning = decode_geometry(blob)
    assert not warning
    assert restored == item
    base = tmp_path / 'base.db'
    put(base, item)
    asset = catalog(tmp_path, base).query(Filters()).assets[0]
    pixels, warning, hit = preview(asset, 96, tmp_path / 'previews')
    assert len(pixels) == 96 * 96 * 4 and max(pixels[3::4]) > 0
    assert not warning and not hit
    assert preview(asset, 96, tmp_path / 'previews')[2]
    editor = SwfEditorApp()
    try:
        assert editor.place_shape({'shape_data': blob, 'name': asset.name})
        new = editor.scene.library_symbols[1]
        assert new.fill_styles == item.fill_styles and new.line_styles == item.line_styles
        path = tmp_path / 'copy.swfproj'
        editor.document.save(path)
        from ui.swf_editor.document import SwfEditingDocument
        reopened = SwfEditingDocument.open(path)
        assert reopened.scene.library_symbols[1] == new
        editor.undo()
        assert not editor.scene.library_symbols
        editor.redo()
        assert editor.scene.library_symbols[1] == new
        editor.scene.layers[0].locked = True
        assert not editor.place_library_asset(item)
    finally:
        editor.cleanup()


def test_legacy_preview_background_and_errors_are_explicit():
    item = shape()
    legacy = json.dumps({'records': [], 'fill_styles': []}).encode()
    recovered, warning = decode_geometry(legacy, shape_to_svg(item))
    assert recovered.bounds == item.bounds
    assert len(recovered.fill_styles) == 1 and recovered.fill_styles[0].color.r == 255
    assert 'Legacy' in warning
    with pytest.raises(ValueError, match='stroke'):
        decode_geometry(legacy, '<svg><path d="M0 0 L10 10" stroke="white"/></svg>')
    with pytest.raises(ValueError, match='object'):
        decode_geometry(b'[]')
    with pytest.raises(ValueError):
        decode_geometry(b'{broken')
    item.fill_styles[0] = FillStyle(0x40, bitmap_id=1, bitmap_matrix=MATRIX())
    with pytest.raises(ValueError, match='Bitmap'):
        decode_geometry(json.dumps(encoded_geometry(item)).encode())


def test_changed_sources_same_row_ids_and_corrupt_preview_cache(tmp_path):
    first, second = tmp_path / 'first.db', tmp_path / 'second.db'
    put(first, shape())
    put(second, shape())
    a = catalog(tmp_path, first).query(Filters()).assets[0]
    b = catalog(tmp_path, second).query(Filters()).assets[0]
    assert a.source_id == b.source_id and a.key != b.key
    cache = tmp_path / 'previews'
    preview(a, 64, cache)
    png = next(cache.glob('*.png'))
    png.write_bytes(b'corrupt')
    assert not preview(a, 64, cache)[2]
    with Image.open(png) as image:
        assert image.size == (64, 64)
    with closing(sqlite3.connect(first)) as conn, conn:
        conn.execute('UPDATE shapes SET name=? WHERE id=1', ('B21_Changed',))
    with pytest.raises(ValueError, match='changed'):
        payload(a)
    replacement = catalog(tmp_path, first).query(Filters()).assets[0]
    assert replacement.key != a.key


def test_import_svg_raster_swf_and_atomic_cancel(tmp_path):
    svg = tmp_path / 'B21_Artwork.svg'
    svg.write_text(SVG)
    digest = hashlib.sha256(svg.read_bytes()).hexdigest()
    entries, warnings = import_file(str(svg), 128)
    assert len(entries) == 1 and not warnings
    personal = tmp_path / 'personal.db'
    cancelled = Event()
    cancelled.set()
    with pytest.raises(Cancelled):
        add_personal(personal, 'fo4', entries, cancelled)
    assert not personal.exists()
    assert add_personal(personal, 'fo4', entries) == 1
    assert Catalog(None, personal, tmp_path / 'metadata', 'fo4').query(Filters(origin='user')).total == 1
    assert Catalog(None, personal, tmp_path / 'metadata', 'skyrimse').query(Filters()).total == 0
    image = tmp_path / 'B21_Raster.png'
    tile = Image.new('RGB', (100, 100), 'black')
    tile.paste('white', (20, 20, 80, 80))
    tile.save(image)
    assert import_file(str(image), 128)[0]
    from creation_lib.swf.project import build_document
    from creation_lib.swf.writer import write_swf
    movie = tmp_path / 'B21_Artwork.swf'
    movie.write_bytes(write_swf(build_document({'shapes': [{'id': 1, 'svg': svg.name}]}, tmp_path)))
    assert import_file(str(movie), 128)[0]
    assert hashlib.sha256(svg.read_bytes()).hexdigest() == digest
    invalid = tmp_path / 'invalid.svg'
    invalid.write_text('broken')
    with pytest.raises(Exception):
        import_file(str(invalid), 128)


def test_actual_worker_stale_jobs_game_changes_import_capture_and_cleanup(tmp_path):
    base = tmp_path / 'base.db'
    make_fixture(base, 700)
    controller = LibraryController(base, tmp_path / 'personal.db', tmp_path / 'cache', 'fo4')
    process = None
    try:
        settle(controller, lambda: controller.total == 700 and not controller.loading)
        first = controller.asset_at(0)
        controller.select(first)
        controller.want_previews([controller.asset_at(i) for i in range(6)], 96)
        settle(controller, lambda: controller._process is not None)
        process = controller._process
        for text in ('B21_Asset0001', 'B21_Asset0002', '\"bad :'):
            controller.search(Filters(query=text))
        controller.set_source(base, 'skyrimse')
        assert controller.selection is None and controller.geometry is None
        settle(controller, lambda: controller.total == 700 and not controller.loading)
        second = controller.asset_at(1)
        controller.select(second)
        controller.want_previews([second], 64)
        settle(controller, lambda: controller.shape is not None and (second.key, 64) in controller.pixels)
        assert controller.selection == second
        assert len(controller.pages) <= MAX_PAGES and sum(map(len, controller.pixels.values())) <= MAX_PIXELS_BYTES
        source = tmp_path / 'B21_Copy.svg'
        source.write_text(SVG)
        assert controller.request_import(source)
        controller.set_source(base, 'fo4')
        settle(controller, lambda: not controller.importing)
        assert 'Added 1' in controller.import_status
        assert Catalog(None, controller.catalog.personal, tmp_path / 'verify', 'skyrimse').query(Filters()).total == 1
        assert Catalog(None, controller.catalog.personal, tmp_path / 'verify', 'fo4').query(Filters()).total == 0
        assert controller.request_import(source)
        controller.cancel_import()
        settle(controller, lambda: not controller.importing)
        controller.pause()
        assert not controller._wanted
    finally:
        controller.close()
        controller.close()
    assert process is not None and process._closed


def test_invalid_catalog_is_visible_and_retry_recovers(tmp_path):
    base = tmp_path / 'broken.db'
    base.write_bytes(b'not sqlite')
    controller = LibraryController(base, tmp_path / 'user.db', tmp_path / 'cache', 'fo4')
    try:
        deadline = time.monotonic() + 5
        while not controller.error and time.monotonic() < deadline:
            controller.poll()
            time.sleep(.005)
        assert controller.error and controller.total == 0
        base.unlink()
        put(base, shape())
        controller.retry()
        settle(controller, lambda: controller.total == 1 and not controller.loading)
    finally:
        controller.close()


def test_cache_eviction_releases_real_gl_textures_exactly_once(tmp_path):
    import moderngl
    from ui.swf_editor.panels.library_panel import LibraryPanel
    context = moderngl.create_standalone_context()
    editor = SwfEditorApp()
    editor.ctx = context
    panel = LibraryPanel(editor, db_paths={}, user_db=tmp_path / 'missing.db', cache_dir=tmp_path / 'cache')
    objects = []
    class OwnedTexture:
        def __init__(self, inner):
            self.inner, self.releases = inner, 0
        def release(self):
            self.releases += 1
            self.inner.release()
    class Context:
        def texture(self, *a):
            owned = OwnedTexture(context.texture(*a))
            objects.append(owned)
            return owned
    editor.ctx = Context()
    try:
        for index in range(150):
            key = (str(index), 64)
            panel.controller.pixels[key] = b'\xff' * (64 * 64 * 4)
            panel._uploads = 0
            assert panel._texture(key, 64) is not None
        assert len(panel._textures) == 128 and sum(t.releases for t in objects) == 22
        panel.close()
        panel.close()
        assert all(t.releases == 1 for t in objects)
    finally:
        panel.close()
        editor.cleanup()
        context.release()


def test_cancel_during_personal_transaction_rolls_back_all_rows(tmp_path):
    entries = [('B21_Copy', json.dumps(encoded_geometry(shape())).encode(), 'test.svg', 'Imported')]
    class CancelAtCommit:
        calls = 0
        def is_set(self):
            self.calls += 1
            return self.calls >= 2
    path = tmp_path / 'personal.db'
    with pytest.raises(Cancelled):
        add_personal(path, 'fo4', entries, CancelAtCommit())
    assert Catalog(None, path, tmp_path / 'metadata', 'fo4').query(Filters()).total == 0


def test_personal_wal_changes_invalidate_cached_assets_and_snapshot_repairs(tmp_path):
    base = tmp_path / 'wal.db'
    put(base, shape())
    with closing(sqlite3.connect(base)) as writer:
        writer.execute('PRAGMA journal_mode=WAL')
        c = catalog(tmp_path, base)
        asset = c.query(Filters()).assets[0]
        writer.execute('UPDATE shapes SET name=?', ('B21_NewName',))
        writer.commit()
        with pytest.raises(ValueError, match='changed'):
            payload(asset)
        refreshed = c.query(Filters()).assets[0]
        assert refreshed.key != asset.key and refreshed.name == 'B21_NewName'
        snapshot, _ = c.snapshot()
        snapshot.write_bytes(b'corrupt private sidecar')
        assert c.query(Filters()).assets[0].name == 'B21_NewName'


def test_legacy_svg_curves_close_without_twip_drift_and_keep_alpha(tmp_path):
    svg = '<svg><path fill="#ffffff" fill-opacity=".4" d="M 7.33 -15.31 C 7.525 -15.285 7.665 -15.185 7.755 -15.005 L 7.39 -12.63 C 6.685 -12.795 6.495 -12.765 6.305 -12.67 Z"/></svg>'
    item, warning = decode_geometry(b'{}', svg)
    assert item.fill_styles[0].color.a == 102 and warning
    base = tmp_path / 'legacy.db'
    put(base, item)
    asset = catalog(tmp_path, base).query(Filters()).assets[0]
    assert max(preview(asset, 96, tmp_path / 'previews')[0][3::4]) > 0
    from creation_lib.swf.svg_io import _parse_svg_path_d, _cubic_to_quadratics
    assert len(_cubic_to_quadratics(0,0,100,100,-100,100,0,200)) > 1
    assert _parse_svg_path_d('M1e1 10 l10 0 0 10 -10 0 z')
    with pytest.raises(ValueError, match='Unsupported'):
        _parse_svg_path_d('M0 0 A1 1 0 0 1 2 2 z')
    with pytest.raises(ValueError):
        _parse_svg_path_d('M0 0 L1 1 Z 2 2')
    unsupported = tmp_path / 'transformed.svg'
    unsupported.write_text('<svg><rect width="20" height="20" transform="translate(5 5)"/></svg>')
    with pytest.raises(ValueError, match='Expand transforms'):
        import_file(str(unsupported), 128)


def test_dead_helper_reports_error_and_explicit_retry_recovers(tmp_path):
    base = tmp_path / 'base.db'
    put(base, shape())
    controller = LibraryController(base, tmp_path / 'user.db', tmp_path / 'cache', 'fo4')
    try:
        settle(controller, lambda: controller.total == 1)
        asset = controller.asset_at(0)
        controller.select(asset)
        settle(controller, lambda: controller.shape is not None)
        owned_process = controller._process
        owned_process.terminate()
        owned_process.join(2)
        controller.poll()
        assert 'helper exited' in controller.error and controller._process is None
        controller.retry()
        controller.select(None)
        controller.select(asset)
        settle(controller, lambda: controller.shape is not None)
    finally:
        controller.close()


def test_close_during_worker_start_releases_late_process(tmp_path):
    controller = LibraryController(None, tmp_path / 'user.db', tmp_path / 'cache', 'fo4')
    settle(controller, lambda: not controller.loading)
    gate, started = Event(), Event()
    create = controller._create_worker
    def delayed(counts):
        started.set()
        assert gate.wait(2)
        return create(counts)
    controller._create_worker = delayed
    controller._ensure_worker()
    assert started.wait(2)
    future = controller._worker_future
    controller.close()
    gate.set()
    process = future.result(timeout=5)[0]
    deadline = time.monotonic() + 5
    while not process._closed and time.monotonic() < deadline:
        time.sleep(.005)
    assert process._closed
