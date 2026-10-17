"""Command benchmarks; use hyperfine for wall timing and native UI traces for frames."""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import sqlite3
from types import SimpleNamespace


def make_fixture(path: Path, rows: int = 25000):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        return
    from creation_lib.preprocessor.swf import CREATE_TABLES_DDL
    blob = json.dumps({"records": [{"type": "sc", "move": True, "dx": 0, "dy": 0, "fill1": 1},
                                  {"type": "se", "dx": 600, "dy": 0}, {"type": "se", "dx": -300, "dy": 400},
                                  {"type": "se", "dx": -300, "dy": -400}, {"type": "end"}],
                       "fill_styles": [{"type": 0, "color": "#ffffff"}], "bounds": [0, 0, 30, 20],
                       "benchmark_padding": "x" * 7800}).encode()
    svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 30 20"><path d="M0 0 L30 0 L15 20 Z" fill="#ffffff"/></svg>'
    with sqlite3.connect(path) as conn:
        conn.executescript(CREATE_TABLES_DDL)
        conn.executemany("INSERT INTO shapes(name,source_swf,source_tag,bounds,fill_colors,path_count,shape_data,svg_preview,tags,user_added) VALUES (?,?,?,?,?,?,?,?,?,?)",
                         ((f"B21_Asset{i:06}", f"B21_Source{i // 100:04}.swf", i % 100 + 1, "[0,0,30,20]", '["#ffffff"]',
                           1, blob, svg, f"Perk/Group{i % 15}", int(i >= rows - 500)) for i in range(rows)))
        conn.execute("INSERT INTO shapes_fts(shapes_fts) VALUES ('rebuild')")


def baseline(database: Path, repeat: int, warm: bool):
    saved = Path('.codex/tmp/swf-library/baseline_library_panel.py').resolve()
    spec = importlib.util.spec_from_file_location("swf_library_before", saved)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    # The historical panel opens databases read/write. Keep this experiment read-only.
    connect = sqlite3.connect
    module.sqlite3 = SimpleNamespace(Row=sqlite3.Row, connect=lambda path: connect(Path(path).resolve().as_uri()+"?mode=ro", uri=True))
    panel = module.LibraryPanel(SimpleNamespace(renderer=None))
    panel._db_path = lambda: database
    renderer = None
    if warm:
        import moderngl
        from ui.swf_editor.swf_renderer import SwfRenderer
        from ui.swf_editor.thumbnail_cache import ThumbnailCache
        renderer = SwfRenderer(moderngl.create_standalone_context())
        panel.app.renderer = renderer
        cache = ThumbnailCache("benchmark", Path('.codex/tmp/swf-library/baseline-cache'))
        panel._cache = cache
        panel._search()
        for row in panel._results:
            cache.put(row['id'], 96, 0, b'\xff\xff\xff\xff' * (96 * 96))
        cache.invalidate()
    try:
        for _ in range(repeat):
            if warm:
                panel._cache.invalidate()
            panel._search()
        fetched = sum(len(row.get('shape_data') or b'') + len(row.get('svg_preview') or '') for row in panel._results)
        print(json.dumps({'mode': 'baseline_warm' if warm else 'baseline_query', 'rows': len(panel._results),
                          'payload_bytes_loaded_for_search': fetched, 'textures': len(panel._tex_objects), 'repeat': repeat}))
    finally:
        for texture in panel._tex_objects:
            texture.release()
        if renderer:
            renderer.cleanup()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--mode', choices=('prepare', 'baseline_query', 'baseline_warm', 'indexed_query'), required=True)
    parser.add_argument('--database', type=Path, required=True)
    parser.add_argument('--repeat', type=int, default=8)
    parser.add_argument('--rows', type=int, default=25000)
    args = parser.parse_args()
    if args.mode == 'prepare':
        make_fixture(args.database, args.rows)
        print(f'Prepared {args.rows:,} disposable assets: {args.database}')
    elif args.mode == 'indexed_query':
        from ui.swf_editor.library.catalog import Catalog, Filters
        catalog = Catalog(args.database, Path('.codex/tmp/swf-library/benchmark-personal-missing.db'),
                          Path('.codex/tmp/swf-library/benchmark-metadata'), 'fo4')
        for _ in range(args.repeat):
            page = catalog.query(Filters())
        print(json.dumps({'mode': args.mode, 'rows':len(page.assets), 'total':page.total,
                          'payload_bytes_loaded_for_search':0, 'repeat':args.repeat}))
    else:
        baseline(args.database.resolve(), args.repeat, args.mode == 'baseline_warm')


if __name__ == '__main__':
    main()
