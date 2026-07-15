import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent))

from creation_lib.swf.shapes import ShapeDef, StyleChange, StraightEdge, CurvedEdge, EndShape
from creation_lib.swf.types import FillStyle, RGBA


def _make_triangle_shape() -> ShapeDef:
    """Minimal ShapeDef: a white filled triangle."""
    fill = FillStyle(fill_type=0, color=RGBA(255, 255, 255, 255))
    records = [
        StyleChange(move_x=0, move_y=0, fill1=1),
        StraightEdge(dx=200, dy=0),
        StraightEdge(dx=-100, dy=200),
        StraightEdge(dx=-100, dy=-200),
        EndShape(),
    ]
    return ShapeDef(
        shape_id=1,
        bounds=(0, 0, 200, 200),
        fill_styles=[fill],
        line_styles=[],
        records=records,
    )


def test_serialize_roundtrip():
    """Serialize ShapeDef -> BLOB -> deserialize -> same record count and types."""
    from creation_lib.preprocessor.swf import serialize_shape_records, deserialize_shape_records

    shape = _make_triangle_shape()
    blob = serialize_shape_records(shape)
    assert isinstance(blob, bytes)
    assert len(blob) > 0

    data = deserialize_shape_records(blob)
    assert "records" in data
    assert "fill_styles" in data
    assert "bounds" in data
    assert len(data["records"]) == len(shape.records)
    assert data["records"][0]["type"] == "sc"
    assert data["records"][1]["type"] == "se"
    assert data["records"][-1]["type"] == "end"
    assert data["fill_styles"][0]["color"] == "#ffffff"


import tempfile


def test_thumbnail_cache_miss():
    from ui.swf_editor.thumbnail_cache import ThumbnailCache

    with tempfile.TemporaryDirectory() as tmp:
        cache = ThumbnailCache("fo4", Path(tmp))
        assert cache.get(1, 64) is None
        assert cache.load_png(1, 64) is None


def test_thumbnail_cache_put_and_get():
    from ui.swf_editor.thumbnail_cache import ThumbnailCache

    with tempfile.TemporaryDirectory() as tmp:
        cache = ThumbnailCache("fo4", Path(tmp))
        fake_pixels = b"\xff\x00\x00\xff" * (64 * 64)  # red 64x64 RGBA
        cache.put(shape_id=1, size=64, texture_id=42, pixels=fake_pixels)
        assert cache.get(1, 64) == 42
        # Check PNG on disk
        png_path = Path(tmp) / "fo4" / "1_64.png"
        assert png_path.exists()


def test_thumbnail_cache_load_png_after_put():
    from ui.swf_editor.thumbnail_cache import ThumbnailCache

    with tempfile.TemporaryDirectory() as tmp:
        cache = ThumbnailCache("fo4", Path(tmp))
        fake_pixels = b"\x00\xff\x00\xff" * (96 * 96)
        cache.put(shape_id=5, size=96, texture_id=7, pixels=fake_pixels)
        # Fresh cache (no memory), disk hit
        cache2 = ThumbnailCache("fo4", Path(tmp))
        raw = cache2.load_png(5, 96)
        assert raw is not None
        assert len(raw) > 0


def test_thumbnail_cache_invalidate_clears_memory():
    from ui.swf_editor.thumbnail_cache import ThumbnailCache

    with tempfile.TemporaryDirectory() as tmp:
        cache = ThumbnailCache("fo4", Path(tmp))
        fake_pixels = b"\x00\x00\xff\xff" * (64 * 64)
        cache.put(1, 64, 99, fake_pixels)
        assert cache.get(1, 64) == 99
        cache.invalidate()
        assert cache.get(1, 64) is None
        # Disk still present
        assert cache.load_png(1, 64) is not None


import queue as _queue
import time


def test_tessellate_from_blob_triangle():
    """_tessellate_from_blob returns non-empty vertex array for a triangle."""
    from ui.swf_editor.thumbnail_loader import _tessellate_from_blob
    from creation_lib.preprocessor.swf import serialize_shape_records

    shape = _make_triangle_shape()
    blob = serialize_shape_records(shape)
    bounds = list(shape.bounds_px)
    verts = _tessellate_from_blob(blob, bounds, 64)
    # Triangle has 3 vertices -> 1 triangle -> 3 verts x 6 floats = 18
    assert isinstance(verts, list)
    assert len(verts) >= 18


def test_tessellate_from_blob_empty():
    """Empty shape_data returns empty list without raising."""
    from ui.swf_editor.thumbnail_loader import _tessellate_from_blob

    result = _tessellate_from_blob(b"", [0, 0, 100, 100], 64)
    assert result == []


def test_thumbnail_loader_delivers_result():
    """ThumbnailLoader posts tessellated results to queue."""
    from ui.swf_editor.thumbnail_loader import ThumbnailLoader
    from creation_lib.preprocessor.swf import serialize_shape_records

    result_q: _queue.Queue = _queue.Queue()
    loader = ThumbnailLoader(result_q)

    shape = _make_triangle_shape()
    blob = serialize_shape_records(shape)
    bounds = list(shape.bounds_px)

    loader.request(shape_id=1, shape_data=blob, bounds=bounds, size=64)

    # Wait up to 2s for result
    result = None
    for _ in range(20):
        try:
            result = result_q.get(timeout=0.1)
            break
        except _queue.Empty:
            pass

    loader.shutdown()
    assert result is not None
    shape_id, size, verts, _ = result
    assert shape_id == 1
    assert size == 64
    assert verts is not None and len(verts) > 0
