from dataclasses import replace
from pathlib import Path
import threading

import pytest

from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.save_inspection import inspect, screenshot
from .test_save_inspection import fixture


def test_generated_rgba_premultiplied_colors_and_source_bytes(tmp_path):
    source = fixture(tmp_path / "colors.fos", screenshot=(3, 1, bytes((255, 0, 0, 255, 0, 255, 0, 255, 64, 0, 0, 128))))
    before = Path(source.path).read_bytes()
    image = screenshot(inspect(source))
    assert image.shape == (1, 3, 3) and image[0, 0].tolist() == [255, 0, 0] and image[0, 1].tolist() == [0, 255, 0]
    assert 126 <= image[0, 2, 0] <= 128 and Path(source.path).read_bytes() == before


def test_thumbnail_and_cancel_before_read(tmp_path):
    source = fixture(tmp_path / "wide.fos", screenshot=(2048, 1, bytes((0, 0, 255, 255)) * 2048))
    saved = inspect(source)
    image = screenshot(saved)
    assert image.shape == (1, 1024, 3) and image[0, 0].tolist() == [0, 0, 255]
    event = threading.Event()
    event.set()
    with pytest.raises(Cancelled):
        screenshot(saved, event)


@pytest.mark.parametrize("fault", ["size", "offset", "dimensions", "stale"])
def test_preview_bounds_and_stale_metadata_are_explicit(tmp_path, fault):
    source = fixture(tmp_path / "generated.fos")
    saved = inspect(source)
    if fault == "size":
        saved = replace(saved, width=8192, height=8192)
    elif fault == "offset":
        saved = replace(saved, image_offset=0)
    elif fault == "dimensions":
        saved = replace(saved, width=1, height=1)
    else:
        Path(source.path).write_bytes(Path(source.path).read_bytes() + b"generated changed fixture")
    with pytest.raises(ManagerError):
        screenshot(saved)


def test_cancellation_between_screenshot_chunks_closes_stream(tmp_path, monkeypatch):
    source = fixture(tmp_path / "chunked.fos", screenshot=(384, 216, bytes((0, 0, 255, 255)) * (384 * 216)))
    saved, event = inspect(source), threading.Event()
    original, opened = Path.open, []
    class CancelRead:
        def __init__(self, stream):
            self.stream = stream
        def __getattr__(self, name):
            return getattr(self.stream, name)
        def __enter__(self):
            self.stream.__enter__()
            return self
        def __exit__(self, *args):
            return self.stream.__exit__(*args)
        def read(self, size):
            data = self.stream.read(size)
            if size >= 256 * 1024:
                event.set()
            return data
    def patched(path, *args, **kwargs):
        stream = original(path, *args, **kwargs)
        if path == Path(source.path) and args and args[0] == "rb":
            opened.append(stream)
            return CancelRead(stream)
        return stream
    monkeypatch.setattr(Path, "open", patched)
    with pytest.raises(Cancelled):
        screenshot(saved, event)
    assert opened and all(stream.closed for stream in opened)
